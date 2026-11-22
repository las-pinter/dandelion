//! Read-only discovery and token accounting for Claude Code CLI sessions.
//!
//! Dandelion observes Claude Code through two interfaces that need no changes
//! to the user's configuration:
//!
//! - the session registry at `~/.claude/sessions/<pid>.json`, watched with
//!   inotify, gives pid, session id, cwd, name, version, and state;
//! - the transcript at `~/.claude/projects/<slug>/<sessionId>.jsonl`, tailed
//!   incrementally, gives token usage deduplicated by `message.id`.
//!
//! Sessions are matched to a herdr-core terminal through `HC_TERMINAL_ID` in
//! `/proc/<pid>/environ`. A session file outlives a crashed process, so a
//! record is trusted only when `/proc/<pid>/stat` start time equals its
//! `procStart`.
//!
//! # Privacy
//!
//! The crate never writes under the Claude state directory and never keeps
//! conversation content: only accounting fields are deserialized, and errors
//! and warnings name files, not their contents.
//!
//! # Example
//!
//! ```no_run
//! use dandelion_claude::{ClaudePaths, SessionWatcher, discover};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let paths = ClaudePaths::from_home()?;
//! let watcher = SessionWatcher::new(&paths.sessions_dir())?;
//! for session in discover(&paths)?.sessions {
//!     println!("{} {} {:?}", session.pid, session.cwd, session.status);
//! }
//! let _changed = watcher.wait_for_change(std::time::Duration::from_secs(5))?;
//! # Ok(())
//! # }
//! ```

mod discovery;
mod error;
mod model;
mod paths;
mod proc;
mod usage;
mod watch;

pub use discovery::{DiscoveryReport, discover};
pub use error::ClaudeError;
pub use model::{Session, SessionStatus};
pub use paths::{ClaudePaths, find_transcript, is_safe_session_id, project_slug};
pub use usage::{
    ModelPrice, ModelUsage, PRICE_TABLE, PRICE_TABLE_REVIEWED_ON, SessionUsage, TokenUsage,
    UNKNOWN_MODEL, UsageTracker, collect_session_usage, price_for,
};
pub use watch::SessionWatcher;
