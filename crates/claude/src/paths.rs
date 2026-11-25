//! Locations inside the Claude Code state directory.

use std::path::{Path, PathBuf};

use crate::error::ClaudeError;

/// Where Claude Code keeps its state, and where `/proc` is mounted.
///
/// The type is read-only by design: nothing in this crate creates, rewrites, or
/// deletes anything under the Claude directory.
#[derive(Debug, Clone)]
pub struct ClaudePaths {
    claude_dir: PathBuf,
    proc_root: PathBuf,
}

impl ClaudePaths {
    /// Uses `claude_dir` as the Claude state directory and `/proc` for process
    /// checks.
    #[must_use]
    pub fn new(claude_dir: impl Into<PathBuf>) -> Self {
        Self {
            claude_dir: claude_dir.into(),
            proc_root: PathBuf::from("/proc"),
        }
    }

    /// Uses the current user's `~/.claude` directory.
    ///
    /// # Errors
    ///
    /// Returns [`ClaudeError::NoHomeDir`] when `$HOME` is unset.
    pub fn from_home() -> Result<Self, ClaudeError> {
        let home = std::env::var_os("HOME").ok_or(ClaudeError::NoHomeDir)?;
        Ok(Self::new(PathBuf::from(home).join(".claude")))
    }

    /// Overrides the `/proc` mount point. Intended for tests.
    #[must_use]
    pub fn with_proc_root(mut self, proc_root: impl Into<PathBuf>) -> Self {
        self.proc_root = proc_root.into();
        self
    }

    /// The Claude state directory, normally `~/.claude`.
    #[must_use]
    pub fn claude_dir(&self) -> &Path {
        &self.claude_dir
    }

    /// The session registry directory.
    #[must_use]
    pub fn sessions_dir(&self) -> PathBuf {
        self.claude_dir.join("sessions")
    }

    /// The transcript root, one directory per project slug.
    #[must_use]
    pub fn projects_dir(&self) -> PathBuf {
        self.claude_dir.join("projects")
    }

    /// The `/proc` mount point used for liveness and terminal checks.
    #[must_use]
    pub fn proc_root(&self) -> &Path {
        &self.proc_root
    }

    /// The transcript file for a session started in `cwd`, when it exists.
    #[must_use]
    pub fn transcript(&self, session_id: &str, cwd: &str) -> Option<PathBuf> {
        find_transcript(self, session_id, cwd)
    }
}

/// The project-directory name Claude Code derives from a working directory.
///
/// Every character outside `[A-Za-z0-9]` becomes `-`. For example
/// `/home/dev/dandelion` becomes `-home-dev-dandelion`. Claude Code caps long
/// names with a hash suffix; such a slug is not reproduced here, and
/// [`find_transcript`] falls back to scanning the project directories.
#[must_use]
pub fn project_slug(cwd: &str) -> String {
    cwd.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

/// Whether a session id can safely be used as a single path component.
#[must_use]
pub fn is_safe_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id != "."
        && session_id != ".."
        && !session_id.contains(['/', '\\', '\0'])
}

/// Finds the transcript for a session, if one exists.
///
/// The preferred location is the project directory named by `cwd`. A resumed
/// session keeps the directory it was first created in, so the other project
/// directories are searched as a fallback.
#[must_use]
pub fn find_transcript(paths: &ClaudePaths, session_id: &str, cwd: &str) -> Option<PathBuf> {
    if !is_safe_session_id(session_id) {
        return None;
    }

    let projects = paths.projects_dir();
    let preferred = projects
        .join(project_slug(cwd))
        .join(format!("{session_id}.jsonl"));
    if preferred.is_file() {
        return Some(preferred);
    }

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&projects)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path().join(format!("{session_id}.jsonl")))
        .filter(|candidate| candidate.is_file())
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::{is_safe_session_id, project_slug};

    #[test]
    fn slug_replaces_every_non_alphanumeric_character() {
        assert_eq!(project_slug("/home/dev/dandelion"), "-home-dev-dandelion");
    }

    #[test]
    fn slug_turns_dots_and_slashes_into_separate_dashes() {
        assert_eq!(
            project_slug("/home/dev/.herdr/worktrees/x"),
            "-home-dev--herdr-worktrees-x"
        );
    }

    #[test]
    fn slug_replaces_non_ascii_characters() {
        assert_eq!(project_slug("/work/café"), "-work-caf-");
    }

    #[test]
    fn rejects_session_ids_that_could_escape_the_projects_directory() {
        assert!(is_safe_session_id("0f0f0f0f-0000-4000-8000-000000000000"));
        assert!(!is_safe_session_id(""));
        assert!(!is_safe_session_id("."));
        assert!(!is_safe_session_id(".."));
        assert!(!is_safe_session_id("../other"));
        assert!(!is_safe_session_id("a/b"));
        assert!(!is_safe_session_id("a\\b"));
        assert!(!is_safe_session_id("a\0b"));
    }
}
