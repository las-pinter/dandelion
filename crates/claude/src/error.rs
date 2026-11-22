//! Errors returned by the crate.
//!
//! Messages name files and paths only. Nothing in a Claude transcript or session
//! record is ever copied into an error.

use std::io;
use std::path::PathBuf;

/// An error while discovering sessions or reading usage.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClaudeError {
    /// A file could not be read.
    #[error("could not read {path}: {source}")]
    Io {
        /// The file that failed.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// The filesystem watcher could not be created or failed while running.
    #[error("filesystem watch failed: {0}")]
    Watch(#[from] notify::Error),
    /// The watcher's event channel closed unexpectedly.
    #[error("filesystem watch channel closed")]
    WatchClosed,
    /// Neither `$HOME` nor an explicit path is available.
    #[error("no home directory available to locate the Claude Code state directory")]
    NoHomeDir,
}

impl ClaudeError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
