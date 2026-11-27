//! Locating a session's transcript and its subagent files.
//!
//! Everything here touches the filesystem, so the socket calls these on the
//! blocking pool.

use std::path::{Path as FsPath, PathBuf};

use serde::Serialize;

use crate::state::AppState;

/// One subagent run of a session, as its metadata file describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentInfo {
    /// The file id, addressable as `?agent=` on the socket.
    pub agent: String,
    /// The tool-use id this run answers.
    pub tool_use_id: Option<String>,
    /// The run's description.
    pub description: Option<String>,
}

/// Reads a session's subagent metadata on the blocking pool.
pub async fn subagents(path: &FsPath) -> Vec<SubagentInfo> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || list_subagents(&path))
        .await
        .unwrap_or_default()
}

/// Finds the file for a session, or a subagent of it, off the runtime.
pub async fn resolve(state: AppState, session: String, agent: Option<String>) -> Option<PathBuf> {
    tokio::task::spawn_blocking(move || resolve_now(&state, &session, agent.as_deref()))
        .await
        .ok()
        .flatten()
}

/// Finds the file for a session, or a subagent of it.
fn resolve_now(state: &AppState, session: &str, agent: Option<&str>) -> Option<PathBuf> {
    let transcript = state.claude.transcript_path(session)?;
    match agent {
        None => transcript.is_file().then_some(transcript),
        Some(agent) => {
            if !valid_agent_id(agent) {
                return None;
            }
            let path = subagents_dir(&transcript).join(format!("{agent}.jsonl"));
            path.is_file().then_some(path)
        }
    }
}

/// The subagent directory that belongs to a session transcript file.
fn subagents_dir(transcript: &FsPath) -> PathBuf {
    transcript.with_extension("").join("subagents")
}

/// The directory holding a transcript's subagent runs.
///
/// Runs are flat: a session's own runs sit beside its file, and a run's own
/// runs sit in the same flat directory, so a subagent file lists them from
/// its parent.
fn subagents_home(transcript: &FsPath) -> PathBuf {
    let beside = subagents_dir(transcript);
    if beside.is_dir() {
        return beside;
    }
    match transcript.parent() {
        Some(parent) if parent.file_name() == Some(std::ffi::OsStr::new("subagents")) => {
            parent.to_path_buf()
        }
        _ => beside,
    }
}

/// `agent-` followed by letters and digits; no path separators.
fn valid_agent_id(agent: &str) -> bool {
    agent
        .strip_prefix("agent-")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Reads the metadata files beside a session's subagent transcripts.
fn list_subagents(transcript: &FsPath) -> Vec<SubagentInfo> {
    let Ok(entries) = std::fs::read_dir(subagents_home(transcript)) else {
        return Vec::new();
    };
    let mut items: Vec<SubagentInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let agent = name.strip_suffix(".meta.json")?;
            let raw = std::fs::read_to_string(entry.path()).ok()?;
            let meta: serde_json::Value = serde_json::from_str(&raw).ok()?;
            Some(SubagentInfo {
                agent: agent.to_owned(),
                tool_use_id: meta
                    .get("toolUseId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                description: meta
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect();
    items.sort_by(|left, right| left.agent.cmp(&right.agent));
    items
}

#[cfg(test)]
mod tests {
    use super::{subagents_dir, subagents_home, valid_agent_id};

    #[test]
    fn agent_ids_cannot_escape_the_subagent_directory() {
        assert!(valid_agent_id("agent-a3be97a2729079bbd"));
        assert!(!valid_agent_id("agent-"));
        assert!(!valid_agent_id("agent-../../etc"));
        assert!(!valid_agent_id("other-123"));
        assert!(!valid_agent_id(""));
    }

    #[test]
    fn a_subagent_file_lists_runs_from_its_flat_directory() {
        let session = std::path::Path::new("/p/-work-x/session-1.jsonl");
        let run = std::path::Path::new("/p/-work-x/session-1/subagents/agent-a1.jsonl");

        assert_eq!(
            subagents_home(run),
            std::path::PathBuf::from("/p/-work-x/session-1/subagents")
        );
        assert_eq!(
            subagents_home(session),
            std::path::PathBuf::from("/p/-work-x/session-1/subagents")
        );
    }

    #[test]
    fn the_subagent_directory_sits_beside_the_session_file() {
        let dir = subagents_dir(std::path::Path::new("/p/-work-x/session-1.jsonl"));

        assert_eq!(
            dir,
            std::path::PathBuf::from("/p/-work-x/session-1/subagents")
        );
    }
}
