//! herdr-core event subscriptions.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::error::TerminalError;
use crate::events::{BackendEvent, EventGate, Resync};
use crate::jsonrpc::{self, Incoming, Request};
use crate::model::TerminalFilter;
use crate::stream::EventStream;
use crate::util::CALL_TIMEOUT;

use super::client::CoreClient;
use super::map::{self, CoreNotice};
use super::wire::SubscribeResult;

/// How many events may wait for the consumer before the core is slowed down.
const CHANNEL_CAPACITY: usize = 256;

type EventSender = mpsc::Sender<Result<BackendEvent, TerminalError>>;

/// Opens a subscription and starts its reader task.
pub(crate) async fn subscribe(
    client: &CoreClient,
    filter: &TerminalFilter,
) -> Result<EventStream, TerminalError> {
    let params = subscribe_params(filter)?;
    let mut stream = client.connect().await?;
    let request = Request::new("sub", "events.subscribe", params);
    stream.write_all(request.encode_line().as_bytes()).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let snapshot = read_snapshot(&mut reader).await?;
    let (sender, receiver) = mpsc::channel(CHANNEL_CAPACITY);
    let mut gate = EventGate::new();
    gate.resync(snapshot.seq);
    sender
        .send(Ok(BackendEvent::Resync(resync_from(snapshot))))
        .await
        .map_err(|_| TerminalError::Closed)?;

    tokio::spawn(run(reader, sender, gate));
    Ok(EventStream::new(receiver))
}

fn subscribe_params(filter: &TerminalFilter) -> Result<Value, TerminalError> {
    let mut params = serde_json::Map::new();
    if !filter.labels.is_empty() {
        params.insert("labels".to_owned(), serde_json::to_value(&filter.labels)?);
    }
    if let Some(terminal) = &filter.terminal {
        params.insert(
            "terminal".to_owned(),
            Value::String(terminal.as_str().to_owned()),
        );
    }
    Ok(Value::Object(params))
}

async fn read_snapshot(
    reader: &mut BufReader<UnixStream>,
) -> Result<SubscribeResult, TerminalError> {
    let mut line = String::new();
    let read = timeout(CALL_TIMEOUT, reader.read_line(&mut line))
        .await
        .map_err(|_| TerminalError::Timeout)??;
    if read == 0 {
        return Err(TerminalError::Protocol(
            "core closed the subscription without a snapshot".into(),
        ));
    }
    match jsonrpc::parse_line(&line)? {
        Incoming::Response(response) => Ok(serde_json::from_value(response.into_result()?)?),
        Incoming::Notification(notification) => Err(TerminalError::Protocol(format!(
            "unexpected notification {} before the subscription snapshot",
            notification.method
        ))),
    }
}

fn resync_from(snapshot: SubscribeResult) -> Resync {
    Resync {
        seq: snapshot.seq,
        terminals: snapshot
            .terminals
            .into_iter()
            .map(map::terminal_info)
            .collect(),
    }
}

/// Reads notifications until the core closes the stream or the consumer goes
/// away.
async fn run(mut reader: BufReader<UnixStream>, sender: EventSender, mut gate: EventGate) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                let _ = sender
                    .send(Err(TerminalError::Unavailable(
                        "herdr-core closed the event stream".into(),
                    )))
                    .await;
                return;
            }
            Ok(_) => {}
            Err(error) => {
                let _ = sender.send(Err(TerminalError::Io(error))).await;
                return;
            }
        }
        let notice = match jsonrpc::parse_line(&line) {
            Ok(Incoming::Notification(notification)) => {
                map::parse_notice(&notification.method, &notification.params)
            }
            Ok(Incoming::Response(_)) => Ok(None),
            Err(error) => Err(error),
        };
        match notice {
            Ok(Some(CoreNotice::Resync(resync))) => {
                gate.resync(resync.seq);
                if !forward(&sender, BackendEvent::Resync(resync)).await {
                    return;
                }
            }
            Ok(Some(CoreNotice::Event(event))) => {
                if gate.accepts(event.seq) && !forward(&sender, BackendEvent::Terminal(event)).await
                {
                    return;
                }
            }
            Ok(Some(CoreNotice::Heartbeat { seq })) => {
                if !forward(&sender, BackendEvent::Heartbeat { seq }).await {
                    return;
                }
            }
            Ok(None) => {}
            Err(error) => {
                if sender.send(Err(error)).await.is_err() {
                    return;
                }
            }
        }
    }
}

async fn forward(sender: &EventSender, event: BackendEvent) -> bool {
    sender.send(Ok(event)).await.is_ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;
    use tokio::time::timeout;

    use super::subscribe;
    use crate::error::TerminalError;
    use crate::events::{BackendEvent, TerminalChange};
    use crate::herdr_core::client::CoreClient;
    use crate::model::TerminalFilter;

    fn socket_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dandelion-terminals-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("core.sock")
    }

    #[allow(clippy::needless_pass_by_value)]
    fn notification(method: &str, params: serde_json::Value) -> String {
        let mut line = json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string();
        line.push('\n');
        line
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn recorded_stream_replays_snapshot_events_heartbeats_and_missed_resync() {
        let path = socket_path("subscribe");
        let listener = UnixListener::bind(&path).expect("bind");
        let snapshot = json!({
            "jsonrpc": "2.0",
            "id": "sub",
            "result": {"seq": 100, "terminals": [{"address": "t1"}]}
        })
        .to_string()
            + "\n";
        let dropped_before = notification(
            "event",
            json!({"seq": 99, "terminal": "t1", "type": "labels",
                   "data": {"labels": {"old": "1"}}}),
        );
        let kept_after = notification(
            "event",
            json!({"seq": 101, "terminal": "t1", "type": "labels",
                   "data": {"labels": {"new": "1"}}}),
        );
        let heartbeat = notification("heartbeat", json!({"seq": 101}));
        let missed = notification(
            "missed",
            json!({"seq": 200, "terminals": [{"address": "t1", "title": "after"}]}),
        );
        let dropped_after_resync = notification(
            "event",
            json!({"seq": 150, "terminal": "t1", "type": "labels",
                   "data": {"labels": {"late": "1"}}}),
        );
        let after_resync = notification(
            "event",
            json!({"seq": 201, "terminal": "t1", "type": "labels",
                   "data": {"labels": {"final": "1"}}}),
        );
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read, mut write) = stream.into_split();
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("request");
            let request: serde_json::Value = serde_json::from_str(&line).expect("json");
            assert_eq!(request["method"], "events.subscribe");
            write
                .write_all(snapshot.as_bytes())
                .await
                .expect("snapshot");
            for line in [
                dropped_before,
                kept_after,
                heartbeat,
                missed,
                dropped_after_resync,
                after_resync,
            ] {
                write.write_all(line.as_bytes()).await.expect("event");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        });

        let client = CoreClient::new(&path);
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
        assert_eq!(resync.seq, 100);
        assert_eq!(resync.terminals.len(), 1);

        let kept = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        let BackendEvent::Terminal(event) = kept else {
            panic!("expected event")
        };
        assert_eq!(event.seq, 101);
        assert_eq!(
            event.change,
            TerminalChange::Labels {
                labels: BTreeMap::from([("new".to_owned(), "1".to_owned())])
            }
        );

        let heartbeat = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        assert_eq!(heartbeat, BackendEvent::Heartbeat { seq: 101 });

        let missed = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        let BackendEvent::Resync(resync) = missed else {
            panic!("expected resync")
        };
        assert_eq!(resync.seq, 200);
        assert_eq!(resync.terminals[0].title, "after");

        let final_event = timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("timeout")
            .expect("stream")
            .expect("event");
        let BackendEvent::Terminal(event) = final_event else {
            panic!("expected event")
        };
        assert_eq!(event.seq, 201);

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
