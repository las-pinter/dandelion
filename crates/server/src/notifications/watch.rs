//! Background watchers that turn state transitions into pushes.
//!
//! Agent changes arrive on the `agents` hub topic, metrics are sampled from
//! the latest collector snapshot, and units are listed on a slow poll. Each
//! watcher keeps its own transition memory, so a restart may repeat a notice
//! that is still true rather than lose one.

use std::time::Duration;

use dandelion_terminals::{TerminalChange, TerminalId, TerminalInfo};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;

use super::{Notice, Triggers};
use crate::{agents::TOPIC, auth::session::now_millis, claude::ClaudeMonitor, state::AppState};

/// How often the metrics are checked.
const METRICS_INTERVAL: Duration = Duration::from_secs(5);
/// A sample older than this means the collector stopped; a stale sample must
/// not age into a "sustained high" notice.
const STALE_SAMPLE_MS: u64 = 30_000;
/// How often the unit list is read; units change slowly.
const UNIT_INTERVAL: Duration = Duration::from_secs(60);

/// Starts every notification watcher; a disabled service starts none.
pub fn spawn_watchers(state: AppState) {
    if !state.notifications.is_enabled() {
        return;
    }
    spawn_agent_watcher(state.clone());
    spawn_metrics_watcher(state.clone());
    spawn_unit_watcher(state);
}

/// Hands one notice to delivery, so a slow push service never stalls a
/// watcher's receive loop; each device still has its own timeout.
fn deliver(state: &AppState, notice: Notice) {
    let notifications = state.notifications.clone();
    tokio::spawn(async move {
        let _ = notifications.send(&notice).await;
    });
}

fn spawn_agent_watcher(state: AppState) {
    // Subscribe before the task starts so no event between here and the first
    // receive is lost.
    let mut receiver = state.hub.subscribe(TOPIC);
    tokio::spawn(async move {
        let mut triggers = Triggers::default();
        // Announce what is already blocked before the first backend event.
        // The snapshot and the hub stream are not ordered: a transition in
        // the gap can replay one stale event, so a rare extra waiting notice
        // is preferred to a missed one.
        match state.agents.snapshot().await {
            Ok(listing) => {
                for notice in triggers.observe_resync(&listing.agents) {
                    deliver(&state, notice);
                }
            }
            Err(error) => {
                tracing::debug!(%error, "cannot read agents for the first notification pass");
            }
        }
        loop {
            match receiver.recv().await {
                Ok(payload) => {
                    for notice in handle_payload(&mut triggers, &payload, &state.claude) {
                        deliver(&state, notice);
                    }
                }
                Err(RecvError::Lagged(skipped)) => {
                    // A dropped transition would otherwise go unannounced, so
                    // rebuild the memory from the backend's own snapshot.
                    tracing::warn!(skipped, "agent events lagged; resyncing notification state");
                    match state.agents.snapshot().await {
                        Ok(listing) => {
                            for notice in triggers.observe_resync(&listing.agents) {
                                deliver(&state, notice);
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, "cannot resync agent notifications");
                        }
                    }
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

fn spawn_metrics_watcher(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(METRICS_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut triggers = Triggers::default();
        loop {
            tick.tick().await;
            let Some(snapshot) = state.metrics.latest() else {
                continue;
            };
            let now = u64::try_from(now_millis()).unwrap_or(0);
            if sample_is_stale(now, snapshot.timestamp_ms) {
                tracing::debug!(
                    age_ms = now.saturating_sub(snapshot.timestamp_ms),
                    "metrics sample is stale; resource windows reset"
                );
                triggers.reset_resources();
                continue;
            }
            for notice in triggers.observe_sample(&snapshot, now) {
                deliver(&state, notice);
            }
        }
    });
}

fn spawn_unit_watcher(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(UNIT_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut triggers = Triggers::default();
        loop {
            tick.tick().await;
            match state.services.snapshot().await {
                Ok(snapshot) => {
                    for notice in triggers.observe_units(&snapshot.units) {
                        deliver(&state, notice);
                    }
                }
                Err(error) => {
                    tracing::debug!(%error, "cannot read units for notifications");
                }
            }
        }
    });
}

fn sample_is_stale(now_ms: u64, sample_ms: u64) -> bool {
    now_ms.saturating_sub(sample_ms) > STALE_SAMPLE_MS
}

/// One message on the `agents` topic, as the agents publisher writes it.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AgentEvent {
    Resync {
        terminals: Vec<TerminalInfo>,
    },
    Terminal {
        terminal: TerminalId,
        change: TerminalChange,
    },
    Claude,
    #[serde(other)]
    Other,
}

fn handle_payload(triggers: &mut Triggers, payload: &Value, claude: &ClaudeMonitor) -> Vec<Notice> {
    match serde_json::from_value::<AgentEvent>(payload.clone()) {
        Ok(AgentEvent::Resync { terminals }) => triggers.observe_resync(&terminals),
        Ok(AgentEvent::Terminal { terminal, change }) => triggers
            .observe_event(&terminal.to_string(), &change)
            .into_iter()
            .collect(),
        Ok(AgentEvent::Claude) => triggers.observe_claude(&claude.snapshot()),
        Ok(AgentEvent::Other) => Vec::new(),
        Err(error) => {
            tracing::debug!(%error, "cannot read an agents event");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Triggers, handle_payload, sample_is_stale};
    use crate::claude::ClaudeMonitor;

    #[test]
    fn a_sample_goes_stale_after_thirty_seconds() {
        assert!(!sample_is_stale(30_000, 0));
        assert!(!sample_is_stale(30_000, 30_000));
        assert!(sample_is_stale(30_001, 0));
        assert!(
            !sample_is_stale(0, 5_000),
            "a future timestamp never counts as stale"
        );
    }

    #[test]
    fn a_publisher_payload_becomes_a_notice() {
        let mut triggers = Triggers::default();
        let claude = ClaudeMonitor::disabled();
        let resync = json!({
            "kind": "resync",
            "terminals": [{
                "id": "t7",
                "title": "claude in dandelion",
                "labels": {},
                "agent_kind": "claude-code",
                "lifecycle": "live"
            }]
        });
        assert_eq!(handle_payload(&mut triggers, &resync, &claude).len(), 0);

        let blocked = json!({
            "kind": "terminal",
            "terminal": "t7",
            "change": {
                "type": "status",
                "status": { "state": "blocked", "blocked_reason": "permission" }
            }
        });
        let notices = handle_payload(&mut triggers, &blocked, &claude);
        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices[0].body,
            "claude in dandelion is waiting for permission."
        );

        let claude_event = json!({ "kind": "claude" });
        assert_eq!(
            handle_payload(&mut triggers, &claude_event, &claude).len(),
            0
        );
    }
}
