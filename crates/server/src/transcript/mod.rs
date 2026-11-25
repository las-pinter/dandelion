//! Read-only streaming of a Claude Code transcript for the Transcript tab.
//!
//! Nothing is stored: the socket tails Claude Code's own JSONL file and
//! forwards what it finds. The raw terminal stays the ground truth, and the
//! tab is best effort — an unreadable line is skipped, never fatal.
//!
//! Frames (JSON text):
//! - server: `page {records, start, hasEarlier}` at open (and again when the
//!   file is replaced), `appended {records}` while tailing, `earlier
//!   {records, start, hasEarlier}` on request, `subagents {items}` at open of
//!   any transcript socket and again when the list changes, and `error`.
//! - client: `earlier` only; the server owns the paging cursor.
//!
//! File reads run on the blocking pool: a transcript can be several megabytes
//! and paging walks backwards from the page end in 64 KiB blocks.

mod files;
mod reader;
mod records;

use std::{
    path::{Path as FsPath, PathBuf},
    time::Duration,
};

use axum::{
    Router,
    extract::{
        Path as UrlPath, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::{
    auth::{
        Revocation,
        middleware::{RequestUser, error_response, origin_allowed},
    },
    state::AppState,
};

use files::{SubagentInfo, resolve, subagents};

/// Largest frame accepted from the browser.
const MAX_INPUT_BYTES: usize = 16 * 1024;
/// How often the socket revalidates its session.
const SESSION_RECHECK: Duration = Duration::from_secs(30);
/// How often the tail looks for new records.
const TAIL_INTERVAL: Duration = Duration::from_millis(500);
/// How long one frame may wait for a slow client before the socket closes.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Records per `appended` frame; a long stall sends several frames.
const APPEND_CHUNK: usize = 200;
/// Tail ticks between subagent list rescans while nothing is appended.
///
/// A run's metadata appears after the `Task` call is recorded, so the list
/// can change while the parent writes no displayable record.
const SUBAGENT_RESCAN: u32 = 4;

/// Routes mounted under `/api/claude`.
pub fn router() -> Router<AppState> {
    Router::new().route("/{session}/transcript/ws", get(upgrade))
}

/// Optional subagent selection for the socket.
#[derive(Debug, Deserialize)]
struct TranscriptQuery {
    agent: Option<String>,
}

/// Control messages a browser may send as text frames.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Control {
    /// Asks for the page before the oldest record already sent.
    Earlier,
}

/// What sending an earlier page achieved.
enum Earlier {
    /// The page went out; the cursor now points at its first record.
    Sent(u64),
    /// The client left.
    Gone,
    /// The file could not be read.
    Failed,
}

/// Upgrades an authenticated, same-origin request to the transcript socket.
async fn upgrade(
    State(state): State<AppState>,
    axum::Extension(user): axum::Extension<RequestUser>,
    UrlPath(session): UrlPath<String>,
    Query(query): Query<TranscriptQuery>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(state.auth.origin(), &headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "origin_rejected",
            "the WebSocket handshake did not come from the Dandelion origin",
        );
    }
    upgrade
        .max_message_size(MAX_INPUT_BYTES)
        .max_frame_size(MAX_INPUT_BYTES)
        .on_upgrade(move |socket| connection(socket, state, user, session, query.agent))
}

/// Streams one transcript until either side leaves.
async fn connection(
    socket: WebSocket,
    state: AppState,
    user: RequestUser,
    session: String,
    agent: Option<String>,
) {
    let (mut sink, mut stream) = socket.split();
    let Some(path) = resolve(state.clone(), session.clone(), agent.clone()).await else {
        tracing::info!(session = %session, agent = ?agent, "no readable transcript for this session");
        refuse(&mut sink, "no readable transcript for this session").await;
        return;
    };

    let Ok(page) = page(path.clone(), None).await else {
        tracing::warn!(transcript = %path.display(), "cannot read the transcript at open");
        refuse(&mut sink, "cannot read the transcript").await;
        return;
    };
    let mut start = page.start;
    let Ok(mut cursor) = cursor_at(path.clone(), page.end).await else {
        tracing::warn!(transcript = %path.display(), "cannot fingerprint the transcript at open");
        refuse(&mut sink, "cannot read the transcript").await;
        return;
    };
    let Some(mut known_subagents) = send_open(&mut sink, &path, page).await else {
        return;
    };

    let mut revocations = state.auth.subscribe_revocations();
    let mut recheck = tokio::time::interval(SESSION_RECHECK);
    recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    recheck.tick().await;
    let mut tail_interval = tokio::time::interval(TAIL_INTERVAL);
    tail_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut ticks: u32 = 0;

    loop {
        tokio::select! {
            _ = tail_interval.tick() => {
                ticks = ticks.wrapping_add(1);
                match send_tail(&mut sink, &path, tail(path.clone(), cursor).await).await {
                    TailOutcome::Sent {
                        cursor: advanced,
                        reset_start,
                        appended,
                    } => {
                        cursor = advanced;
                        if let Some(new_start) = reset_start {
                            start = new_start;
                        }
                        if (appended || ticks.is_multiple_of(SUBAGENT_RESCAN))
                            && !refresh_subagents(&mut sink, &path, &mut known_subagents).await
                        {
                            break;
                        }
                    }
                    TailOutcome::Gone => break,
                    TailOutcome::Failed(message) => {
                        let _ = sink
                            .send(text(&json!({"type": "error", "message": message})))
                            .await;
                        break;
                    }
                }
            }
            frame = stream.next() => {
                let Some(Ok(message)) = frame else { break };
                match message {
                    Message::Text(text_frame) => {
                        match handle_frame(&mut sink, text_frame.as_str(), path.clone(), start).await {
                            FrameOutcome::Continue => {}
                            FrameOutcome::NewStart(older) => start = older,
                            FrameOutcome::Gone => break,
                            FrameOutcome::Failed => {
                                let _ = sink
                                    .send(text(&json!({
                                        "type": "error",
                                        "message": "cannot read an earlier page",
                                    })))
                                    .await;
                                break;
                            }
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            _ = recheck.tick() => {
                if !state.session_is_live(&user.token_hash).await {
                    tracing::info!("closing a transcript socket whose session ended");
                    break;
                }
            }
            revocation = revocations.recv() => {
                match revocation {
                    Ok(Revocation::Session(hash)) if hash == user.token_hash => break,
                    Ok(Revocation::User(id)) if id == user.user_id => break,
                    Ok(_) | Err(RecvError::Closed) => {}
                    // A lagged receiver may have missed this session's own
                    // revocation, so it must not linger.
                    Err(RecvError::Lagged(_)) => break,
                }
            }
        }
    }
}

/// Sends one error frame; used when the socket cannot start.
async fn refuse(sink: &mut SplitSink<WebSocket, Message>, message: &str) {
    send_frame(sink, &json!({"type": "error", "message": message})).await;
}

/// Sends one frame, giving up on a client that stops reading.
async fn send_frame(sink: &mut SplitSink<WebSocket, Message>, value: &serde_json::Value) -> bool {
    matches!(
        tokio::time::timeout(WRITE_TIMEOUT, sink.send(text(value))).await,
        Ok(Ok(()))
    )
}

/// Sends the opening page and the subagent list.
///
/// Returns `None` when the client left while the frames were sent.
async fn send_open(
    sink: &mut SplitSink<WebSocket, Message>,
    path: &FsPath,
    page: reader::Page,
) -> Option<Vec<SubagentInfo>> {
    let opening = json!({
        "type": "page",
        "records": page.records,
        "start": page.start,
        "hasEarlier": page.has_earlier,
    });
    if !send_frame(sink, &opening).await {
        return None;
    }
    let items = subagents(path).await;
    let frame = json!({"type": "subagents", "items": items});
    if !send_frame(sink, &frame).await {
        return None;
    }
    Some(items)
}

/// What a tail pass produced for the client.
enum TailOutcome {
    /// Frames went out; the cursor advanced and the file may have restarted.
    Sent {
        /// The advanced cursor.
        cursor: reader::Cursor,
        /// The paging start when the file was replaced.
        reset_start: Option<u64>,
        /// Whether the tail carried new records.
        appended: bool,
    },
    /// The client left.
    Gone,
    /// The file could not be read.
    Failed(&'static str),
}

/// Sends what the tail found, reloading the pane when the file was replaced.
async fn send_tail(
    sink: &mut SplitSink<WebSocket, Message>,
    path: &FsPath,
    found: std::io::Result<(reader::Tail, reader::Cursor)>,
) -> TailOutcome {
    let Ok((tail, cursor)) = found else {
        tracing::warn!(transcript = %path.display(), "the transcript could not be read");
        return TailOutcome::Failed("the transcript went away");
    };
    if tail.reset {
        tracing::info!(transcript = %path.display(), "the transcript was replaced");
        // The file was replaced: a fresh capped page, never the whole file.
        let Ok(fresh) = page(path.to_path_buf(), None).await else {
            return TailOutcome::Failed("the transcript went away");
        };
        let Ok(cursor) = cursor_at(path.to_path_buf(), fresh.end).await else {
            return TailOutcome::Failed("the transcript went away");
        };
        let frame = json!({
            "type": "page",
            "records": fresh.records,
            "start": fresh.start,
            "hasEarlier": fresh.has_earlier,
        });
        return if send_frame(sink, &frame).await {
            TailOutcome::Sent {
                cursor,
                reset_start: Some(fresh.start),
                appended: false,
            }
        } else {
            TailOutcome::Gone
        };
    }
    if tail.records.is_empty() {
        return TailOutcome::Sent {
            cursor,
            reset_start: None,
            appended: false,
        };
    }
    for chunk in tail.records.chunks(APPEND_CHUNK) {
        let frame = json!({"type": "appended", "records": chunk});
        if !send_frame(sink, &frame).await {
            return TailOutcome::Gone;
        }
    }
    TailOutcome::Sent {
        cursor,
        reset_start: None,
        appended: true,
    }
}

/// What handling one client frame achieved.
enum FrameOutcome {
    /// Nothing changed.
    Continue,
    /// The paging start moved to the first record of the older page.
    NewStart(u64),
    /// The client left.
    Gone,
    /// The older page could not be read.
    Failed,
}

/// Handles one client text frame.
async fn handle_frame(
    sink: &mut SplitSink<WebSocket, Message>,
    text_frame: &str,
    path: PathBuf,
    start: u64,
) -> FrameOutcome {
    match serde_json::from_str::<Control>(text_frame) {
        Ok(Control::Earlier) => match send_earlier(sink, path, start).await {
            Earlier::Sent(older) => FrameOutcome::NewStart(older),
            Earlier::Gone => FrameOutcome::Gone,
            Earlier::Failed => FrameOutcome::Failed,
        },
        Err(_) => FrameOutcome::Continue,
    }
}

/// Sends a fresh subagent list when a Task call started a run.
///
/// Returns false when the client left.
async fn refresh_subagents(
    sink: &mut SplitSink<WebSocket, Message>,
    path: &FsPath,
    known: &mut Vec<SubagentInfo>,
) -> bool {
    let items = subagents(path).await;
    if items == *known {
        return true;
    }
    known.clone_from(&items);
    let frame = json!({"type": "subagents", "items": items});
    send_frame(sink, &frame).await
}

/// Sends the page before `start`.
async fn send_earlier(
    sink: &mut SplitSink<WebSocket, Message>,
    path: PathBuf,
    start: u64,
) -> Earlier {
    let Ok(older) = page(path.clone(), Some(start)).await else {
        tracing::debug!(transcript = %path.display(), "cannot read an earlier page");
        return Earlier::Failed;
    };
    let frame = json!({
        "type": "earlier",
        "records": older.records,
        "start": older.start,
        "hasEarlier": older.has_earlier,
    });
    if send_frame(sink, &frame).await {
        Earlier::Sent(older.start)
    } else {
        Earlier::Gone
    }
}

/// Reads one page on the blocking pool.
async fn page(path: PathBuf, before: Option<u64>) -> std::io::Result<reader::Page> {
    tokio::task::spawn_blocking(move || reader::page_before(&path, before, reader::PAGE))
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?
}

/// Fingerprints the current end of the file on the blocking pool.
async fn cursor_at(path: PathBuf, offset: u64) -> std::io::Result<reader::Cursor> {
    tokio::task::spawn_blocking(move || reader::Cursor::at(&path, offset))
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?
}

/// Reads new records on the blocking pool, returning the advanced cursor.
async fn tail(
    path: PathBuf,
    mut cursor: reader::Cursor,
) -> std::io::Result<(reader::Tail, reader::Cursor)> {
    tokio::task::spawn_blocking(move || {
        let found = reader::tail(&path, &mut cursor)?;
        Ok::<_, std::io::Error>((found, cursor))
    })
    .await
    .map_err(|error| std::io::Error::other(error.to_string()))?
}

fn text(value: &serde_json::Value) -> Message {
    Message::Text(value.to_string().into())
}
