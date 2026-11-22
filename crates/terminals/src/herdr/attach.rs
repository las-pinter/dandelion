//! Classic herdr attach through the documented CLI bridge.
//!
//! `herdr terminal session observe|control` prints NDJSON frames on stdout and
//! reads NDJSON commands on stdin. Observe never types or resizes.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

use crate::error::TerminalError;
use crate::model::{AttachMode, AttachOptions, TerminalId};
use crate::stream::{AttachCommand, AttachEvent, AttachSession};

use super::wire::ControlCommand;

const EVENT_CAPACITY: usize = 128;
const COMMAND_CAPACITY: usize = 32;

/// Spawns the bridge and returns its session.
pub(crate) fn attach(
    binary: &Path,
    id: &TerminalId,
    options: AttachOptions,
) -> Result<AttachSession, TerminalError> {
    let mode = match options.mode {
        AttachMode::Observe => "observe",
        AttachMode::Control => "control",
    };
    let mut command = Command::new(binary);
    command
        .arg("terminal")
        .arg("session")
        .arg(mode)
        .arg(id.as_str())
        .arg("--cols")
        .arg(options.size.cols.to_string())
        .arg("--rows")
        .arg(options.size.rows.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        TerminalError::Unavailable(format!("failed to start {}: {error}", binary.display()))
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| TerminalError::Protocol("bridge has no stdout".into()))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| TerminalError::Protocol("bridge has no stdin".into()))?;

    let (event_sender, event_receiver) = mpsc::channel(EVENT_CAPACITY);
    let (command_sender, command_receiver) = mpsc::channel(COMMAND_CAPACITY);
    tokio::spawn(read_frames(
        BufReader::new(stdout).lines(),
        child,
        event_sender,
    ));
    tokio::spawn(write_commands(
        stdin,
        command_receiver,
        options.mode == AttachMode::Control,
    ));
    Ok(AttachSession::new(event_receiver, command_sender))
}

async fn read_frames(
    mut lines: Lines<BufReader<ChildStdout>>,
    mut child: Child,
    sender: mpsc::Sender<Result<AttachEvent, TerminalError>>,
) {
    let mut closed = false;
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => match parse_stream_line(&line) {
                Ok(Some(AttachEvent::Data(bytes))) => {
                    if sender.send(Ok(AttachEvent::Data(bytes))).await.is_err() {
                        break;
                    }
                }
                Ok(Some(AttachEvent::Closed { reason })) => {
                    let _ = sender.send(Ok(AttachEvent::Closed { reason })).await;
                    closed = true;
                    break;
                }
                Ok(Some(event)) => {
                    if sender.send(Ok(event)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = sender.send(Err(error)).await;
                    break;
                }
            },
            Ok(None) => break,
            Err(error) => {
                let _ = sender.send(Err(TerminalError::Io(error))).await;
                break;
            }
        }
    }
    let _ = child.wait().await;
    if !closed {
        let _ = sender.send(Ok(AttachEvent::Closed { reason: None })).await;
    }
}

async fn write_commands(
    mut stdin: ChildStdin,
    mut commands: mpsc::Receiver<AttachCommand>,
    allow_control: bool,
) {
    while let Some(command) = commands.recv().await {
        let encoded = match command {
            AttachCommand::Input(bytes) if allow_control => match String::from_utf8(bytes) {
                Ok(text) => serde_json::to_string(&ControlCommand::Input { text: &text }),
                Err(error) => serde_json::to_string(&ControlCommand::InputBytes {
                    bytes: STANDARD.encode(error.into_bytes()),
                }),
            },
            AttachCommand::Resize(size) if allow_control => {
                serde_json::to_string(&ControlCommand::Resize {
                    cols: size.cols,
                    rows: size.rows,
                })
            }
            AttachCommand::Close => break,
            AttachCommand::Input(_) | AttachCommand::Resize(_) => continue,
        };
        let Ok(mut encoded) = encoded else {
            break;
        };
        encoded.push('\n');
        if stdin.write_all(encoded.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
            break;
        }
    }
}

/// Parses one NDJSON line from the bridge.
///
/// # Errors
///
/// Returns [`TerminalError::Protocol`] for a frame whose base64 payload is
/// invalid.
pub(crate) fn parse_stream_line(line: &str) -> Result<Option<AttachEvent>, TerminalError> {
    let value: Value = serde_json::from_str(line)?;
    match value.get("type").and_then(Value::as_str) {
        Some("terminal.frame") => {
            let encoded = value
                .get("bytes")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let bytes = STANDARD.decode(encoded).map_err(|error| {
                TerminalError::Protocol(format!("invalid base64 frame: {error}"))
            })?;
            Ok(Some(AttachEvent::Data(bytes)))
        }
        Some("terminal.closed") => Ok(Some(AttachEvent::Closed {
            reason: value
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })),
        _ => Ok(None),
    }
}

/// Resolves the classic herdr binary.
#[must_use]
pub fn default_binary() -> PathBuf {
    PathBuf::from("herdr")
}

#[cfg(test)]
mod tests {
    use super::parse_stream_line;
    use crate::stream::AttachEvent;

    #[test]
    fn frames_decode_their_base64_bytes() {
        let line = r#"{"type":"terminal.frame","seq":1,"full":true,"width":80,"height":24,"encoding":"ansi","bytes":"aGVsbG8="}"#;
        let event = parse_stream_line(line).expect("frame").expect("event");
        assert_eq!(event, AttachEvent::Data(b"hello".to_vec()));
    }

    #[test]
    fn closed_frames_carry_their_reason() {
        let line = r#"{"type":"terminal.closed","reason":"detached"}"#;
        let event = parse_stream_line(line).expect("frame").expect("event");
        assert_eq!(
            event,
            AttachEvent::Closed {
                reason: Some("detached".to_owned())
            }
        );
    }

    #[test]
    fn unknown_frames_and_bad_base64_are_handled() {
        assert_eq!(
            parse_stream_line(r#"{"type":"terminal.scroll","lines":3}"#).expect("other"),
            None
        );
        assert!(parse_stream_line(r#"{"type":"terminal.frame","bytes":"***"}"#).is_err());
    }
}

#[cfg(test)]
mod bridge_tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use tokio::time::timeout;

    use super::attach;
    use crate::model::{AttachOptions, TerminalId};
    use crate::stream::AttachEvent;

    fn fake_binary(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dandelion-terminals-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let binary = dir.join("fake-herdr");
        std::fs::write(
            &binary,
            "#!/bin/sh\n\
             printf '%s\\n' '{\"type\":\"terminal.frame\",\"seq\":1,\"full\":true,\"width\":80,\"height\":24,\"encoding\":\"ansi\",\"bytes\":\"aGVsbG8=\"}'\n\
             printf '%s\\n' '{\"type\":\"terminal.closed\",\"reason\":\"detached\"}'\n",
        )
        .expect("script");
        let mut permissions = std::fs::metadata(&binary).expect("meta").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&binary, permissions).expect("chmod");
        binary
    }

    #[tokio::test]
    async fn bridge_frames_reach_the_session_in_order() {
        let binary = fake_binary("classic-attach");
        let session =
            attach(&binary, &TerminalId::new("w1:p1"), AttachOptions::default()).expect("attach");
        let mut session = session;

        let first = timeout(Duration::from_secs(2), session.next_event())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        assert_eq!(first, AttachEvent::Data(b"hello".to_vec()));

        let second = timeout(Duration::from_secs(2), session.next_event())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        assert_eq!(
            second,
            AttachEvent::Closed {
                reason: Some("detached".to_owned())
            }
        );

        let _ = std::fs::remove_dir_all(binary.parent().expect("parent"));
    }
}
