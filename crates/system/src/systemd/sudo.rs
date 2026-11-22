//! The `sudo -n` execution path for system-unit actions.
//!
//! The caller must have checked the allowlist first; this module only builds the
//! command, runs it without a terminal, and classifies the failure. `-n` makes
//! sudo fail instead of prompting, which matters for a server process with no
//! tty: it must never hang waiting for a password.

use std::process::Stdio;
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process_group};
use tokio::process::Command;

use super::UnitAction;

/// How long a `systemctl` action may take before it is abandoned.
const ACTION_TIMEOUT: Duration = Duration::from_secs(60);

/// Errors from the sudo path.
#[derive(Debug, thiserror::Error)]
pub enum SudoError {
    /// The binary could not be started at all.
    #[error("could not run sudo: {0}")]
    Spawn(#[from] std::io::Error),
    /// Passwordless sudo is not configured for this user.
    #[error("passwordless sudo is unavailable: {detail}")]
    Unavailable {
        /// What sudo printed.
        detail: String,
    },
    /// The sudoers policy refused the command.
    #[error("sudo denied `systemctl {verb} {unit}`: {detail}")]
    Denied {
        /// The attempted verb.
        verb: &'static str,
        /// The attempted unit.
        unit: String,
        /// What sudo printed.
        detail: String,
    },
    /// The command ran and failed.
    #[error("`sudo systemctl {verb} {unit}` failed with status {status:?}: {detail}")]
    Failed {
        /// The attempted verb.
        verb: &'static str,
        /// The attempted unit.
        unit: String,
        /// The exit code, when the process reported one.
        status: Option<i32>,
        /// The most useful output line.
        detail: String,
    },
    /// The command did not finish in time.
    #[error("`sudo systemctl {verb} {unit}` timed out after {timeout_secs} seconds")]
    Timeout {
        /// The attempted verb.
        verb: &'static str,
        /// The attempted unit.
        unit: String,
        /// The timeout that elapsed.
        timeout_secs: u64,
    },
}

/// Builds the sudo command line for an action.
pub(crate) fn command_args(action: UnitAction, unit: &str) -> Vec<String> {
    vec![
        "-n".to_owned(),
        "systemctl".to_owned(),
        action.systemctl_verb().to_owned(),
        unit.to_owned(),
    ]
}

/// Runs a system-unit action through `sudo -n systemctl`.
///
/// The unit must already have passed the allowlist check. The command gets its
/// own process group, so a timed-out action can be killed as a whole instead of
/// leaving a privileged `systemctl` behind.
pub(crate) async fn run(action: UnitAction, unit: &str) -> Result<(), SudoError> {
    let args = command_args(action, unit);
    let child = Command::new("sudo")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()?;
    let group = child
        .id()
        .and_then(|pid| i32::try_from(pid).ok())
        .and_then(Pid::from_raw);
    let output =
        if let Ok(result) = tokio::time::timeout(ACTION_TIMEOUT, child.wait_with_output()).await {
            result?
        } else {
            if let Some(group) = group {
                let _ = kill_process_group(group, Signal::KILL);
            }
            return Err(SudoError::Timeout {
                verb: action.systemctl_verb(),
                unit: unit.to_owned(),
                timeout_secs: ACTION_TIMEOUT.as_secs(),
            });
        };
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(classify(
        action,
        unit,
        output.status.code(),
        &stderr,
        &stdout,
    ))
}

/// Classifies a failed sudo run so the server can tell "not installed" from
/// "not allowed" from "the unit failed".
fn classify(
    action: UnitAction,
    unit: &str,
    status: Option<i32>,
    stderr: &str,
    stdout: &str,
) -> SudoError {
    let detail = if stderr.trim().is_empty() {
        stdout.trim().to_owned()
    } else {
        stderr.trim().to_owned()
    };
    let lowered = detail.to_ascii_lowercase();
    if lowered.contains("password is required")
        || lowered.contains("no tty present")
        || lowered.contains("a terminal is required")
    {
        SudoError::Unavailable { detail }
    } else if lowered.contains("not allowed to execute") || lowered.contains("sudoers") {
        SudoError::Denied {
            verb: action.systemctl_verb(),
            unit: unit.to_owned(),
            detail,
        }
    } else {
        SudoError::Failed {
            verb: action.systemctl_verb(),
            unit: unit.to_owned(),
            status,
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SudoError, classify, command_args};
    use crate::systemd::UnitAction;

    #[test]
    fn command_is_non_interactive_and_uses_typed_arguments() {
        let args = command_args(UnitAction::Restart, "nginx.service");

        assert_eq!(args, vec!["-n", "systemctl", "restart", "nginx.service"]);
    }

    #[test]
    fn every_verb_is_the_systemctl_verb() {
        for (action, verb) in [
            (UnitAction::Start, "start"),
            (UnitAction::Stop, "stop"),
            (UnitAction::Restart, "restart"),
            (UnitAction::Enable, "enable"),
            (UnitAction::Disable, "disable"),
        ] {
            assert_eq!(action.systemctl_verb(), verb);
            assert_eq!(command_args(action, "unit.service")[2], verb);
        }
    }

    #[test]
    fn a_password_prompt_is_unavailable_not_a_failure() {
        let err = classify(
            UnitAction::Restart,
            "nginx.service",
            Some(1),
            "sudo: a password is required",
            "",
        );

        assert!(matches!(err, SudoError::Unavailable { .. }));
    }

    #[test]
    fn a_missing_tty_is_unavailable() {
        let err = classify(
            UnitAction::Stop,
            "nginx.service",
            Some(1),
            "sudo: no tty present and no askpass program specified",
            "",
        );

        assert!(matches!(err, SudoError::Unavailable { .. }));
    }

    #[test]
    fn a_sudoers_refusal_is_denied() {
        let err = classify(
            UnitAction::Start,
            "nginx.service",
            Some(1),
            "probe is not allowed to execute '/usr/bin/systemctl start nginx.service' as root",
            "",
        );

        assert!(matches!(err, SudoError::Denied { .. }));
    }

    #[test]
    fn a_unit_failure_is_a_failure_with_status_and_output() {
        let err = classify(
            UnitAction::Restart,
            "nginx.service",
            Some(5),
            "",
            "Failed to restart nginx.service: Unit not found.",
        );

        match err {
            SudoError::Failed {
                status,
                detail,
                verb,
                unit,
            } => {
                assert_eq!(status, Some(5));
                assert_eq!(detail, "Failed to restart nginx.service: Unit not found.");
                assert_eq!(verb, "restart");
                assert_eq!(unit, "nginx.service");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
