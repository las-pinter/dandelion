//! Classic herdr event subscriptions.
//!
//! Classic herdr needs one subscription entry per pane for agent-status
//! changes, so panes created after the subscription starts report their status
//! only through `pane_created` until the next resync.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::error::TerminalError;
use crate::events::{BackendEvent, Resync, TerminalEvent};
use crate::model::TerminalFilter;
use crate::stream::EventStream;

use super::client::ClassicClient;
use super::map::{self, ClassicNotice};
use super::wire::SubscriptionFrame;

const CHANNEL_CAPACITY: usize = 256;

/// Opens a subscription over a fresh connection.
pub(crate) async fn subscribe(
    client: &ClassicClient,
    filter: &TerminalFilter,
) -> Result<EventStream, TerminalError> {
    let snapshot = client.snapshot().await?;
    let subscriptions = subscription_list(&snapshot);
    let (reader, _id) = client.subscribe_connection(subscriptions).await?;

    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let terminals = map::snapshot_terminals(&snapshot)
        .into_iter()
        .filter(|info| filter.matches(info))
        .collect();
    sender
        .send(Ok(BackendEvent::Resync(Resync { seq: 0, terminals })))
        .await
        .map_err(|_| TerminalError::Closed)?;

    let client = client.clone();
    let filter = filter.clone();
    tokio::spawn(run(client, reader, sender, filter));
    Ok(EventStream::new(receiver))
}

fn subscription_list(snapshot: &super::wire::Snapshot) -> Value {
    let mut subscriptions = Vec::new();
    for kind in [
        "pane.created",
        "pane.updated",
        "pane.closed",
        "pane.exited",
        "pane.agent_detected",
        "workspace.created",
        "workspace.closed",
        "workspace.renamed",
        "workspace.metadata_updated",
        "tab.created",
        "tab.closed",
        "tab.renamed",
        "layout.updated",
    ] {
        subscriptions.push(serde_json::json!({"type": kind}));
    }
    for pane in &snapshot.panes {
        subscriptions.push(serde_json::json!({
            "type": "pane.agent_status_changed",
            "pane_id": pane.pane_id,
        }));
    }
    Value::Array(subscriptions)
}

async fn run(
    client: ClassicClient,
    mut reader: BufReader<UnixStream>,
    sender: mpsc::Sender<Result<BackendEvent, TerminalError>>,
    filter: TerminalFilter,
) {
    let mut seq = 0_u64;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                let _ = sender
                    .send(Err(TerminalError::Unavailable(
                        "herdr closed the event stream".into(),
                    )))
                    .await;
                break;
            }
            Ok(_) => {}
            Err(error) => {
                let _ = sender.send(Err(TerminalError::Io(error))).await;
                break;
            }
        }
        let frame: SubscriptionFrame = match serde_json::from_str(&line) {
            Ok(frame) => frame,
            Err(error) => {
                if sender.send(Err(TerminalError::Json(error))).await.is_err() {
                    break;
                }
                continue;
            }
        };
        match map::event_notice(&frame) {
            Ok(ClassicNotice::Resync) => match client.snapshot().await {
                Ok(snapshot) => {
                    let terminals = map::snapshot_terminals(&snapshot)
                        .into_iter()
                        .filter(|info| filter.matches(info))
                        .collect();
                    if sender
                        .send(Ok(BackendEvent::Resync(Resync { seq, terminals })))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    if sender.send(Err(error)).await.is_err() {
                        break;
                    }
                }
            },
            Ok(ClassicNotice::Change { pane, change }) => {
                if !pane_allowed(&filter, &pane, &change) {
                    continue;
                }
                seq = seq.saturating_add(1);
                let event = TerminalEvent {
                    seq,
                    terminal: pane,
                    change,
                };
                if sender
                    .send(Ok(BackendEvent::Terminal(event)))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Ok(ClassicNotice::Ignore) => {}
            Err(error) => {
                if sender.send(Err(error)).await.is_err() {
                    break;
                }
            }
        }
    }
}

fn pane_allowed(
    filter: &TerminalFilter,
    pane: &crate::model::TerminalId,
    change: &crate::events::TerminalChange,
) -> bool {
    if filter
        .terminal
        .as_ref()
        .is_some_and(|allowed| allowed != pane)
    {
        return false;
    }
    if filter.labels.is_empty() {
        return true;
    }
    match change {
        crate::events::TerminalChange::Upserted { terminal } => filter.matches(terminal),
        // Later facts do not repeat the labels; classic herdr has no server-side
        // label filter, so they are forwarded and the consumer refines.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{UnixListener, UnixStream};
    use tokio::time::timeout;

    use super::subscribe;
    use crate::error::TerminalError;
    use crate::events::{BackendEvent, TerminalChange};
    use crate::herdr::client::ClassicClient;
    use crate::model::AgentState;
    use crate::model::TerminalFilter;

    fn socket_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dandelion-terminals-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("herdr.sock")
    }

    async fn read_request(stream: UnixStream) -> (UnixStream, serde_json::Value) {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("request");
        let request = serde_json::from_str(&line).expect("request json");
        (reader.into_inner(), request)
    }

    async fn reply(stream: &mut UnixStream, id: &str, result: serde_json::Value) {
        let mut line = json!({"id": id, "result": result}).to_string();
        line.push('\n');
        stream.write_all(line.as_bytes()).await.expect("reply");
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn snapshot_and_pane_events_become_common_events() {
        let path = socket_path("classic-subscribe");
        let listener = UnixListener::bind(&path).expect("bind");
        let server = tokio::spawn(async move {
            let (first, _) = listener.accept().await.expect("snapshot accept");
            let (first, request) = read_request(first).await;
            let mut first = first;
            assert_eq!(request["method"], "session.snapshot");
            let id = request["id"].as_str().expect("id");
            reply(
                &mut first,
                id,
                json!({
                    "type": "session_snapshot",
                    "snapshot": {
                        "version": "0.9.5", "protocol": 22,
                        "workspaces": [], "tabs": [], "agents": [], "layouts": [],
                        "panes": [{
                            "pane_id": "w1:p1", "workspace_id": "w1",
                            "tab_id": "w1:t1", "agent": "claude",
                            "agent_status": "idle", "cwd": "/home/dev"
                        }]
                    }
                }),
            )
            .await;
            drop(first);

            let (second, _) = listener.accept().await.expect("subscribe accept");
            let (second, request) = read_request(second).await;
            let mut second = second;
            assert_eq!(request["method"], "events.subscribe");
            let subscriptions = request["params"]["subscriptions"]
                .as_array()
                .expect("subscriptions");
            assert!(subscriptions.iter().any(|subscription| {
                subscription["type"] == "pane.agent_status_changed"
                    && subscription["pane_id"] == "w1:p1"
            }));
            let id = request["id"].as_str().expect("id");
            reply(&mut second, id, json!({"type": "subscription_started"})).await;
            for frame in [
                json!({
                    "event": "pane.agent_status_changed",
                    "data": {
                        "type": "pane.agent_status_changed", "pane_id": "w1:p1",
                        "workspace_id": "w1", "agent": "claude", "agent_status": "working"
                    }
                }),
                json!({
                    "event": "pane_closed",
                    "data": {
                        "type": "pane_closed", "pane_id": "w1:p1", "workspace_id": "w1"
                    }
                }),
            ] {
                let mut line = frame.to_string();
                line.push('\n');
                second.write_all(line.as_bytes()).await.expect("event");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        });

        let client = ClassicClient::new(&path);
        let mut stream = subscribe(&client, &TerminalFilter::all())
            .await
            .expect("subscribe");

        let first = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        let BackendEvent::Resync(resync) = first else {
            panic!("expected snapshot")
        };
        assert_eq!(resync.terminals.len(), 1);
        assert_eq!(resync.terminals[0].id.as_str(), "w1:p1");

        let status = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        let BackendEvent::Terminal(status) = status else {
            panic!("expected status")
        };
        assert_eq!(status.seq, 1);
        let TerminalChange::Status {
            agent_kind, status, ..
        } = status.change
        else {
            panic!("expected status change")
        };
        assert_eq!(
            agent_kind.as_ref().map(crate::model::AgentKind::as_str),
            Some("claude-code")
        );
        assert_eq!(status.state, AgentState::Working);

        let removed = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        let BackendEvent::Terminal(removed) = removed else {
            panic!("expected removal")
        };
        assert_eq!(removed.seq, 2);
        assert_eq!(removed.change, TerminalChange::Removed);

        let closed = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect_err("stream should report the close");
        assert!(matches!(closed, TerminalError::Unavailable(_)));

        server.await.expect("server");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }
}
