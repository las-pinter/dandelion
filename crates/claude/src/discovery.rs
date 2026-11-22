//! Discovery of live sessions from the Claude Code session registry.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ClaudeError;
use crate::model::{Session, SessionRecord};
use crate::paths::ClaudePaths;
use crate::proc;

/// The result of one discovery pass.
///
/// `warnings` explains records that were skipped. Warnings contain file names
/// and fixed reasons only, never record contents.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryReport {
    /// Sessions whose process is alive and matches the recorded start time.
    pub sessions: Vec<Session>,
    /// Session files that were ignored, and why.
    pub warnings: Vec<String>,
}

/// Scans the session registry and returns every live session.
///
/// A missing registry directory is treated as "no sessions". Individual files
/// that are malformed, stale, or unverifiable are skipped with a warning so one
/// bad record cannot hide the others.
///
/// # Errors
///
/// Returns [`ClaudeError::Io`] only when the registry directory exists but
/// cannot be read.
pub fn discover(paths: &ClaudePaths) -> Result<DiscoveryReport, ClaudeError> {
    let sessions_dir = paths.sessions_dir();
    let entries = match fs::read_dir(&sessions_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DiscoveryReport::default());
        }
        Err(error) => return Err(ClaudeError::io(sessions_dir, error)),
    };

    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();

    let mut report = DiscoveryReport::default();
    for file in files {
        match load_session(paths, &file) {
            Ok(Some(session)) => report.sessions.push(session),
            Ok(None) => {}
            Err(warning) => report.warnings.push(warning),
        }
    }
    report.sessions.sort_by_key(|session| session.pid);
    Ok(report)
}

fn load_session(paths: &ClaudePaths, file: &Path) -> Result<Option<Session>, String> {
    let name = display_name(file);
    let expected_pid = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.parse::<u32>().ok());

    let bytes = fs::read(file).map_err(|error| format!("{name}: could not be read ({error})"))?;
    // The parse error is reduced to its position: its message can quote the
    // rejected value, and a warning must never carry record contents.
    let record: SessionRecord = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "{name}: invalid session record at line {}, column {}",
            error.line(),
            error.column()
        )
    })?;

    if expected_pid != Some(record.pid) {
        return Err(format!(
            "{name}: file name does not match the recorded pid {}",
            record.pid
        ));
    }

    let Some(proc_start) = record.proc_start.as_deref() else {
        return Err(format!(
            "{name}: no procStart, so the process cannot be verified"
        ));
    };
    if !proc::process_start_matches(paths.proc_root(), record.pid, proc_start) {
        return Err(format!(
            "{name}: process is gone or the pid was reused by another process"
        ));
    }

    let terminal_id = proc::terminal_id(paths.proc_root(), record.pid);
    Ok(Some(record.into_session(terminal_id)))
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || "session record".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}
