//! Change notification for the session registry.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::time::Duration;

use notify::{Event, RecursiveMode, Watcher};

use crate::error::ClaudeError;

/// Watches the session registry for changes.
///
/// Events are intentionally coalesced into one wake-up: every status change
/// rewrites a session file, and a consumer rescans the directory once rather
/// than once per event. The watcher is read-only.
pub struct SessionWatcher {
    _watcher: notify::RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
}

impl std::fmt::Debug for SessionWatcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionWatcher")
            .finish_non_exhaustive()
    }
}

impl SessionWatcher {
    /// Starts watching `sessions_dir`.
    ///
    /// # Errors
    ///
    /// Returns [`ClaudeError::Watch`] when the platform watcher cannot be
    /// created or the directory cannot be watched. The directory must already
    /// exist; on a host where Claude Code has never run, callers should retry
    /// once the directory appears. This crate never creates it, because it
    /// never writes under the Claude state directory.
    pub fn new(sessions_dir: &Path) -> Result<Self, ClaudeError> {
        let (sender, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            // A closed channel means the watcher was dropped; dropping the
            // event is the only sensible action.
            let _ = sender.send(event);
        })?;
        watcher.watch(sessions_dir, RecursiveMode::NonRecursive)?;
        Ok(Self {
            _watcher: watcher,
            events,
        })
    }

    /// Blocks until a change is reported or `timeout` elapses.
    ///
    /// Returns `Ok(true)` when at least one change was seen. Any further events
    /// already queued are consumed, so the caller can rescan once for a burst.
    ///
    /// # Errors
    ///
    /// Returns [`ClaudeError::Watch`] for a backend error and
    /// [`ClaudeError::WatchClosed`] if the watcher stopped.
    pub fn wait_for_change(&self, timeout: Duration) -> Result<bool, ClaudeError> {
        match self.events.recv_timeout(timeout) {
            Ok(Ok(_event)) => {
                loop {
                    match self.events.try_recv() {
                        Ok(Ok(_)) => {}
                        Ok(Err(error)) => return Err(error.into()),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return Err(ClaudeError::WatchClosed),
                    }
                }
                Ok(true)
            }
            Ok(Err(error)) => Err(error.into()),
            Err(RecvTimeoutError::Timeout) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => Err(ClaudeError::WatchClosed),
        }
    }
}
