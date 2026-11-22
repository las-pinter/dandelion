//! Journal reading through `journalctl --output=json`.
//!
//! The journal has no D-Bus read API that is both simple and stable, so this
//! module uses the CLI. JSON output avoids parsing the human format, and one
//! record per line makes following a unit a plain line stream.

use std::fmt;
use std::process::Stdio;

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

use super::SystemdError;
use super::allowlist::validate_unit_name;
use super::types::{JournalEntry, JournalFollowRequest, JournalTailRequest, UnitScope};

/// Errors from reading the journal.
#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    /// A journal line was not the JSON object journalctl promises.
    #[error("journal line is not valid JSON: {source}")]
    Json {
        /// The parser error.
        #[source]
        source: serde_json::Error,
    },
    /// `journalctl` exited with a failure.
    #[error("journalctl failed with status {status:?}: {detail}")]
    CommandFailed {
        /// The exit code, when the process reported one.
        status: Option<i32>,
        /// What journalctl wrote to stderr.
        detail: String,
    },
    /// The process started but produced no stdout to read.
    #[error("journalctl produced no stdout pipe")]
    NoStdout,
}

/// A live stream of journal records for one unit.
///
/// Dropping the stream kills the `journalctl` child process, so a closed browser
/// tab cannot leave a follower behind.
pub struct JournalStream {
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
    stderr: Option<ChildStderr>,
}

impl JournalStream {
    /// Returns the next record, or `None` when the journal output ends.
    ///
    /// A malformed line is returned as an error so callers can log it and keep
    /// following; it does not terminate the stream.
    pub async fn next_entry(&mut self) -> Option<Result<JournalEntry, SystemdError>> {
        loop {
            match self.lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    return Some(parse_entry(&line));
                }
                Ok(None) => return self.end_of_stream().await,
                Err(source) => return Some(Err(SystemdError::Io(source))),
            }
        }
    }

    /// Turns end of output into an error when `journalctl` failed, so a dead
    /// follower is distinguishable from a clean end of stream.
    async fn end_of_stream(&mut self) -> Option<Result<JournalEntry, SystemdError>> {
        let status = match self.child.wait().await {
            Ok(exit) => exit.code(),
            Err(_) => None,
        };
        let mut detail = String::new();
        if let Some(mut stderr) = self.stderr.take() {
            let _ = stderr.read_to_string(&mut detail).await;
        }
        match status {
            Some(code) if code != 0 => {
                Some(Err(SystemdError::Journal(JournalError::CommandFailed {
                    status,
                    detail: detail.trim().to_owned(),
                })))
            }
            _ => None,
        }
    }

    /// Stops following and reaps the child process.
    pub async fn stop(mut self) -> Result<(), SystemdError> {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        Ok(())
    }

    /// The child process id, for logs and tests.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }
}

impl Drop for JournalStream {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl fmt::Debug for JournalStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JournalStream")
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

/// Reads the most recent journal records for a unit.
pub(crate) async fn tail(request: &JournalTailRequest) -> Result<Vec<JournalEntry>, SystemdError> {
    validate_unit_name(&request.unit)?;
    let args = journal_args(request.scope, &request.unit, Some(request.lines), false);
    let output = Command::new("journalctl")
        .args(&args)
        .kill_on_drop(true)
        .output()
        .await?;
    if !output.status.success() {
        return Err(SystemdError::Journal(JournalError::CommandFailed {
            status: output.status.code(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        }));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_entry)
        .collect()
}

/// Starts following a unit's journal.
pub(crate) fn follow(request: &JournalFollowRequest) -> Result<JournalStream, SystemdError> {
    validate_unit_name(&request.unit)?;
    let args = journal_args(request.scope, &request.unit, request.lines, true);
    let mut child = Command::new("journalctl")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or(SystemdError::Journal(JournalError::NoStdout))?;
    let stderr = child.stderr.take();
    Ok(JournalStream {
        child,
        lines: BufReader::new(stdout).lines(),
        stderr,
    })
}

/// Builds the journalctl arguments for a unit selector.
fn journal_args(scope: UnitScope, unit: &str, lines: Option<usize>, follow: bool) -> Vec<String> {
    let mut args = vec![
        "--output=json".to_owned(),
        "--no-pager".to_owned(),
        "--quiet".to_owned(),
    ];
    match scope {
        UnitScope::User => args.push(format!("--user-unit={unit}")),
        UnitScope::System => args.push(format!("--unit={unit}")),
    }
    if let Some(lines) = lines {
        args.push(format!("--lines={lines}"));
    }
    if follow {
        args.push("--follow".to_owned());
    }
    args
}

/// Parses one `--output=json` line.
fn parse_entry(line: &str) -> Result<JournalEntry, SystemdError> {
    let raw: RawEntry = serde_json::from_str(line)
        .map_err(|source| SystemdError::Journal(JournalError::Json { source }))?;
    Ok(JournalEntry {
        timestamp_usec: raw
            .realtime
            .and_then(|value| value.u64())
            .and_then(|value| i64::try_from(value).ok()),
        message: raw
            .message
            .map_or_else(String::new, MessageField::into_text),
        priority: raw
            .priority
            .and_then(|value| value.u64())
            .and_then(|value| u8::try_from(value).ok())
            .filter(|priority| *priority <= 7),
        unit: raw
            .system_unit
            .map(Scalar::into_text)
            .filter(|unit| !unit.is_empty())
            .or_else(|| {
                raw.user_unit
                    .map(Scalar::into_text)
                    .filter(|unit| !unit.is_empty())
            }),
        identifier: raw
            .identifier
            .map(Scalar::into_text)
            .filter(|identifier| !identifier.is_empty())
            .or_else(|| {
                raw.comm
                    .map(Scalar::into_text)
                    .filter(|identifier| !identifier.is_empty())
            }),
        pid: raw
            .pid
            .and_then(|value| value.u64())
            .and_then(|value| u32::try_from(value).ok()),
    })
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(rename = "__REALTIME_TIMESTAMP", default)]
    realtime: Option<Scalar>,
    #[serde(rename = "MESSAGE", default)]
    message: Option<MessageField>,
    #[serde(rename = "PRIORITY", default)]
    priority: Option<Scalar>,
    #[serde(rename = "SYSLOG_IDENTIFIER", default)]
    identifier: Option<Scalar>,
    #[serde(rename = "_COMM", default)]
    comm: Option<Scalar>,
    #[serde(rename = "_PID", default)]
    pid: Option<Scalar>,
    #[serde(rename = "_SYSTEMD_UNIT", default)]
    system_unit: Option<Scalar>,
    #[serde(rename = "_SYSTEMD_USER_UNIT", default)]
    user_unit: Option<Scalar>,
}

/// A journal field that is a string in most records but can be a number.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Scalar {
    Text(String),
    Number(i64),
}

impl Scalar {
    fn into_text(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Number(number) => number.to_string(),
        }
    }

    fn u64(&self) -> Option<u64> {
        match self {
            Self::Text(text) => text.trim().parse().ok(),
            Self::Number(number) => u64::try_from(*number).ok(),
        }
    }
}

/// `MESSAGE`, which journald emits as text or as raw bytes.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MessageField {
    Text(String),
    Bytes(Vec<u8>),
    Number(i64),
}

impl MessageField {
    fn into_text(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Self::Number(number) => number.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JournalError, journal_args, parse_entry};
    use crate::systemd::{SystemdError, UnitScope};

    const USER_LINE: &str = r#"{"__REALTIME_TIMESTAMP":"1700000000123456","MESSAGE":"started probe","PRIORITY":"6","SYSLOG_IDENTIFIER":"dandelion-probe","_PID":"4242","_SYSTEMD_USER_UNIT":"dandelion-probe.service"}"#;

    #[test]
    fn a_user_record_parses_every_common_field() {
        let entry = parse_entry(USER_LINE).expect("sample line must parse");

        assert_eq!(entry.timestamp_usec, Some(1_700_000_000_123_456));
        assert_eq!(entry.message, "started probe");
        assert_eq!(entry.priority, Some(6));
        assert_eq!(entry.identifier.as_deref(), Some("dandelion-probe"));
        assert_eq!(entry.pid, Some(4242));
        assert_eq!(entry.unit.as_deref(), Some("dandelion-probe.service"));
    }

    #[test]
    fn a_system_unit_field_wins_over_the_user_field() {
        let line = r#"{"MESSAGE":"both","_SYSTEMD_UNIT":"system.service","_SYSTEMD_USER_UNIT":"user.service"}"#;

        let entry = parse_entry(line).expect("line must parse");

        assert_eq!(entry.unit.as_deref(), Some("system.service"));
    }

    #[test]
    fn a_byte_array_message_is_decoded_lossily() {
        let line = r#"{"MESSAGE":[104,105,32,255],"PRIORITY":3}"#;

        let entry = parse_entry(line).expect("byte messages must parse");

        assert_eq!(entry.message, "hi \u{fffd}");
        assert_eq!(entry.priority, Some(3));
    }

    #[test]
    fn identifier_falls_back_to_the_command_name() {
        let line = r#"{"MESSAGE":"no syslog id","_COMM":"sleep"}"#;

        let entry = parse_entry(line).expect("line must parse");

        assert_eq!(entry.identifier.as_deref(), Some("sleep"));
        assert_eq!(entry.unit, None);
    }

    #[test]
    fn missing_and_empty_fields_become_none_or_empty() {
        let entry = parse_entry("{}").expect("empty object must parse");

        assert_eq!(entry.message, "");
        assert_eq!(entry.timestamp_usec, None);
        assert_eq!(entry.priority, None);
        assert_eq!(entry.unit, None);
        assert_eq!(entry.identifier, None);
        assert_eq!(entry.pid, None);

        let entry =
            parse_entry(r#"{"MESSAGE":"x","_SYSTEMD_UNIT":""}"#).expect("empty unit must parse");
        assert_eq!(entry.unit, None);
    }

    #[test]
    fn malformed_json_is_reported_as_a_journal_error() {
        let err = parse_entry("not json").expect_err("malformed line must fail");

        assert!(matches!(
            err,
            SystemdError::Journal(JournalError::Json { .. })
        ));
    }

    #[test]
    fn out_of_range_numbers_are_dropped_rather_than_wrapped() {
        let line = r#"{"MESSAGE":"x","PRIORITY":"99","_PID":"-4","__REALTIME_TIMESTAMP":"-1"}"#;

        let entry = parse_entry(line).expect("line must parse");

        assert_eq!(entry.priority, None);
        assert_eq!(entry.pid, None);
        assert_eq!(entry.timestamp_usec, None);
    }

    #[test]
    fn user_units_select_the_user_journal() {
        let args = journal_args(UnitScope::User, "probe.service", Some(20), false);

        assert_eq!(
            args,
            vec![
                "--output=json",
                "--no-pager",
                "--quiet",
                "--user-unit=probe.service",
                "--lines=20",
            ]
        );
    }

    #[test]
    fn system_units_select_the_system_journal_and_follow() {
        let args = journal_args(UnitScope::System, "probe.service", None, true);

        assert_eq!(
            args,
            vec![
                "--output=json",
                "--no-pager",
                "--quiet",
                "--unit=probe.service",
                "--follow",
            ]
        );
    }

    #[test]
    fn zero_lines_is_passed_through_to_follow_only_new_entries() {
        let args = journal_args(UnitScope::User, "probe.service", Some(0), true);

        assert!(args.contains(&"--lines=0".to_owned()));
        assert!(args.contains(&"--follow".to_owned()));
    }
}
