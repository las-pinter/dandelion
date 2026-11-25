//! Claude Code session metadata for the agents page.
//!
//! A background monitor reads the session registry and tails each transcript
//! for token usage through `dandelion-claude`. The agents snapshot reads the
//! cache, so a request never waits on the disk. The usage reader deserializes
//! accounting fields only; conversation content never enters this process.

use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use dandelion_claude::{ClaudePaths, SessionStatus, SessionUsage, UsageTracker, discover};
use serde::Serialize;
use serde_json::json;
use utoipa::ToSchema;

use crate::{agents::TOPIC, state::AppState};

/// How often the monitor rescans the session registry.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);

/// One Claude Code session, as the agents page sees it.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeAgent {
    /// herdr-core terminal the session runs in, from its environment.
    pub terminal: Option<String>,
    /// Session id, used to locate the transcript.
    pub session_id: String,
    /// Working directory the session was launched in.
    pub cwd: String,
    /// Human-readable session name, when set.
    pub name: Option<String>,
    /// Agent name when the session runs an agent.
    pub agent: Option<String>,
    /// Claude Code version reported by the session.
    pub version: Option<String>,
    /// Current state, when the record carries one.
    pub status: Option<SessionStatus>,
    /// What a waiting session is waiting for, when reported.
    pub waiting_for: Option<String>,
    /// Last update time in Unix milliseconds.
    pub updated_at: Option<i64>,
    /// Token totals and the estimated cost.
    pub usage: SessionUsage,
}

/// The cached view of Claude Code's sessions.
///
/// Cloning is cheap and shares one cache. A monitor without `~/.claude` is
/// disabled: it reports no sessions and never fails.
#[derive(Debug, Clone, Default)]
pub struct ClaudeMonitor {
    inner: Option<Arc<Inner>>,
}

#[derive(Debug)]
struct Inner {
    paths: ClaudePaths,
    /// The published snapshot. Only a short swap holds this lock, so a request
    /// never waits on the disk.
    cache: Mutex<Cache>,
    /// Usage state. Only a refresh pass touches it, never a request.
    tracker: Mutex<UsageTracker>,
}

#[derive(Debug)]
struct Cache {
    agents: Vec<ClaudeAgent>,
    /// The last serialized snapshot, so an unchanged pass publishes nothing.
    digest: String,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            agents: Vec::new(),
            // The serialization of an empty list, so an empty first pass is
            // not reported as a change.
            digest: "[]".to_owned(),
        }
    }
}

/// Locks a monitor mutex, recovering from a poison so one failed scan cannot
/// hide every later session.
fn lock<'a, T>(mutex: &'a Mutex<T>, name: &str) -> MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::warn!(
            lock = name,
            "recovering from a poisoned Claude monitor lock"
        );
        poisoned.into_inner()
    })
}

impl ClaudeMonitor {
    /// Watches the current user's `~/.claude` directory.
    #[must_use]
    pub fn from_home() -> Self {
        match ClaudePaths::from_home() {
            Ok(paths) => Self::from_paths(paths),
            Err(error) => {
                tracing::info!(%error, "Claude Code state is unavailable; sessions are not shown");
                Self::default()
            }
        }
    }

    /// Watches an explicit state directory; tests use a synthetic one.
    #[must_use]
    pub fn from_paths(paths: ClaudePaths) -> Self {
        Self {
            inner: Some(Arc::new(Inner {
                paths,
                cache: Mutex::new(Cache::default()),
                tracker: Mutex::new(UsageTracker::new()),
            })),
        }
    }

    /// A monitor with no state directory.
    #[must_use]
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Whether this monitor has a state directory to read.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    /// Locates a known session's transcript file.
    ///
    /// Returns `None` for an unknown session, an unreadable state directory,
    /// or a transcript that is not on disk.
    #[must_use]
    pub fn transcript_path(&self, session_id: &str) -> Option<std::path::PathBuf> {
        let inner = self.inner.as_ref()?;
        let cwd = self
            .snapshot()
            .into_iter()
            .find(|agent| agent.session_id == session_id)
            .map(|agent| agent.cwd)?;
        inner.paths.transcript(session_id, &cwd)
    }

    /// The cached sessions, sorted by terminal then session id.
    ///
    /// Takes the cache lock for a clone only; it never waits on file I/O.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ClaudeAgent> {
        let Some(inner) = &self.inner else {
            return Vec::new();
        };
        lock(&inner.cache, "cache").agents.clone()
    }

    /// Runs one discovery and usage pass.
    ///
    /// Returns whether the public snapshot changed. This is blocking file
    /// I/O; the background loop runs it on a blocking thread.
    pub fn refresh(&self) -> bool {
        let Some(inner) = &self.inner else {
            return false;
        };
        let report = match discover(&inner.paths) {
            Ok(report) => report,
            Err(error) => {
                tracing::warn!(%error, "cannot read the Claude session registry");
                return false;
            }
        };
        for warning in &report.warnings {
            tracing::debug!(%warning, "skipped a Claude session record");
        }

        let mut agents = Vec::with_capacity(report.sessions.len());
        {
            // The transcript I/O happens here, outside the cache lock.
            let mut tracker = lock(&inner.tracker, "tracker");
            for session in &report.sessions {
                let usage = match tracker.update(&inner.paths, &session.session_id, &session.cwd) {
                    Ok(_) => tracker.session_usage(&session.session_id),
                    Err(error) => {
                        tracing::warn!(%error, "cannot read a Claude transcript");
                        SessionUsage::empty(&session.session_id)
                    }
                };
                agents.push(ClaudeAgent {
                    terminal: session.terminal_id.clone(),
                    session_id: session.session_id.clone(),
                    cwd: session.cwd.clone(),
                    name: session.name.clone(),
                    agent: session.agent.clone(),
                    version: session.version.clone(),
                    status: session.status.clone(),
                    waiting_for: session.waiting_for.clone(),
                    updated_at: session.updated_at,
                    usage,
                });
            }

            // A session that ended keeps no accumulator for the process
            // lifetime.
            let live: Vec<&str> = report
                .sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect();
            for usage in tracker.report() {
                if !live.contains(&usage.session_id.as_str()) {
                    tracker.forget(&usage.session_id);
                }
            }
        }
        agents.sort_by(|left, right| {
            left.terminal
                .cmp(&right.terminal)
                .then_with(|| left.session_id.cmp(&right.session_id))
        });

        let digest = serde_json::to_string(&agents).unwrap_or_default();
        let mut cache = lock(&inner.cache, "cache");
        if digest == cache.digest {
            return false;
        }
        cache.digest = digest;
        cache.agents = agents;
        true
    }
}

/// Starts the background scan and publishes a change on the `agents` topic.
///
/// A disabled monitor starts nothing.
pub fn spawn_monitor(state: AppState) {
    if !state.claude.is_enabled() {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SCAN_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        tick.tick().await;
        loop {
            tick.tick().await;
            let monitor = state.claude.clone();
            let changed = match tokio::task::spawn_blocking(move || monitor.refresh()).await {
                Ok(changed) => changed,
                Err(error) => {
                    tracing::warn!(%error, "a Claude scan failed");
                    false
                }
            };
            if changed {
                // The browser refetches the snapshot on any agents event; the
                // payload only names the source.
                let _ = state.hub.publish(TOPIC, json!({ "kind": "claude" }));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fs, io, path::Path};

    use dandelion_claude::{ClaudePaths, SessionStatus};

    use super::ClaudeMonitor;

    fn write_proc(
        proc_root: &Path,
        pid: u32,
        start_time: &str,
        environ: Option<&[u8]>,
    ) -> io::Result<()> {
        let directory = proc_root.join(pid.to_string());
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join("stat"),
            format!(
                "{pid} (claude) R 1 {pid} {pid} 0 -1 4194304 100 0 0 0 1 2 0 0 20 0 5 0 {start_time} 100000 123"
            ),
        )?;
        if let Some(environ) = environ {
            fs::write(directory.join("environ"), environ)?;
        }
        Ok(())
    }

    fn session_record(pid: u32, start_time: &str) -> String {
        format!(
            r#"{{"pid":{pid},"sessionId":"session-{pid}","cwd":"/work/example","procStart":"{start_time}","version":"9.9.9","name":"example-{pid}","agent":"example-agent","status":"waiting","waitingFor":"permission prompt","updatedAt":1795372085544}}"#
        )
    }

    #[test]
    fn refresh_collects_a_session_and_its_tokens() -> Result<(), Box<dyn Error>> {
        let temporary = tempfile::tempdir()?;
        let claude_dir = temporary.path().join("claude");
        fs::create_dir_all(claude_dir.join("sessions"))?;
        fs::write(
            claude_dir.join("sessions").join("424242.json"),
            session_record(424_242, "7001"),
        )?;

        // `find_transcript` maps /work/example to this project directory.
        let project = claude_dir.join("projects").join("-work-example");
        fs::create_dir_all(&project)?;
        fs::write(
            project.join("session-424242.jsonl"),
            concat!(
                r#"{"type":"assistant","message":{"id":"msg_1","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":2}}}"#,
                "\n"
            ),
        )?;

        let proc_root = temporary.path().join("proc");
        write_proc(
            &proc_root,
            424_242,
            "7001",
            Some(b"PATH=/usr/bin\0HC_TERMINAL_ID=t42\0"),
        )?;
        let paths = ClaudePaths::new(&claude_dir).with_proc_root(&proc_root);
        let monitor = ClaudeMonitor::from_paths(paths);

        assert!(monitor.refresh(), "the first pass changes the empty cache");
        let agents = monitor.snapshot();
        assert_eq!(agents.len(), 1);
        let agent = agents.first().expect("one session");
        assert_eq!(agent.terminal.as_deref(), Some("t42"));
        assert_eq!(agent.session_id, "session-424242");
        assert_eq!(agent.cwd, "/work/example");
        assert_eq!(agent.status, Some(SessionStatus::Waiting));
        assert_eq!(agent.waiting_for.as_deref(), Some("permission prompt"));
        assert_eq!(agent.usage.tokens.input_tokens, 10);
        assert_eq!(agent.usage.tokens.output_tokens, 5);
        assert_eq!(agent.usage.tokens.total(), 117);
        assert_eq!(agent.usage.messages, 1);
        assert!(!monitor.refresh(), "an unchanged pass publishes nothing");

        Ok(())
    }

    #[test]
    fn a_missing_directory_reports_nothing() {
        let temporary = tempfile::tempdir().expect("temp dir");
        let monitor = ClaudeMonitor::from_paths(ClaudePaths::new(temporary.path().join("absent")));

        assert!(!monitor.refresh());
        assert!(monitor.snapshot().is_empty());
    }
}
