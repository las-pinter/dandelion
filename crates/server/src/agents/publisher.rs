//! Publishes agent state changes on the `agents` hub topic.
//!
//! Output is not here: it rides the attach socket. This task only mirrors the
//! backend's event stream so lists update without polling. A closed or failed
//! stream reconnects with a fresh resync after a short pause. This is the only
//! owner of the subscription: the task waits for a backend that starts after
//! the server rather than exiting, so the startup retry never starts a second.

use std::time::Duration;

use dandelion_terminals::{BackendEvent, TerminalError, TerminalFilter};
use serde_json::json;

use super::TOPIC;
use crate::state::AppState;

/// How long to wait before resubscribing after the backend went away, and
/// before rechecking for a backend that has not appeared yet.
const RETRY: Duration = Duration::from_secs(5);

/// Starts the background publisher; it waits for a backend when there is none.
pub fn spawn_publisher(state: AppState) {
    spawn_publisher_with(state, RETRY);
}

/// Starts the publisher with a given wait.
///
/// This is a test seam: an integration test passes milliseconds, because the
/// production wait is seconds. Production code calls [`spawn_publisher`].
#[doc(hidden)]
pub fn spawn_publisher_with(state: AppState, retry: Duration) {
    tokio::spawn(async move {
        loop {
            let Some(backend) = state.agents.backend() else {
                tokio::time::sleep(retry).await;
                continue;
            };
            match backend.subscribe(&TerminalFilter::all()).await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(BackendEvent::Resync(resync)) => {
                                let payload = json!({
                                    "kind": "resync",
                                    "terminals": resync.terminals,
                                });
                                let _ = state.hub.publish(TOPIC, payload);
                            }
                            Ok(BackendEvent::Terminal(event)) => {
                                let payload = json!({
                                    "kind": "terminal",
                                    "terminal": event.terminal,
                                    "change": event.change,
                                });
                                let _ = state.hub.publish(TOPIC, payload);
                            }
                            Ok(BackendEvent::Heartbeat { .. }) => {}
                            // One event this build cannot read is skipped;
                            // only a dead stream is worth reconnecting.
                            Err(TerminalError::Json(error)) => {
                                tracing::warn!(%error, "skipping an unreadable agent event");
                            }
                            Err(error) => {
                                tracing::warn!(%error, "agent event stream failed");
                                break;
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "cannot subscribe to agent events");
                }
            }
            tokio::time::sleep(retry).await;
        }
    });
}
