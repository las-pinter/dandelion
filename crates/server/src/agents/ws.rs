//! The agent attach socket: raw pane bytes and agent state in one stream.

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use dandelion_terminals::{AttachEvent, TerminalSize};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::{
    auth::{Revocation, middleware::RequestUser},
    state::AppState,
};

/// Largest frame accepted from the browser.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// How often the socket revalidates its session.
const SESSION_RECHECK: Duration = Duration::from_secs(30);

/// Control messages a browser may send as text frames.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Control {
    /// Applies a new size to the attached terminal.
    Resize { cols: u16, rows: u16 },
}

/// Attaches to one agent and pumps bytes until either side leaves.
pub async fn connection(socket: WebSocket, state: AppState, user: RequestUser, terminal: String) {
    let (mut sink, mut stream) = socket.split();
    let Ok(mut session) = state.agents.attach(&terminal).await else {
        let _ = sink
            .send(Message::Text(
                json!({"type": "error", "message": "cannot attach to this agent"})
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    };

    let mut revocations = state.auth.subscribe_revocations();
    let mut recheck = tokio::time::interval(SESSION_RECHECK);
    recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    recheck.tick().await;

    loop {
        tokio::select! {
            event = session.next_event() => {
                match event {
                    Some(Ok(AttachEvent::Data(bytes))) => {
                        if sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(AttachEvent::Resized(size))) => {
                        let message = json!({
                            "type": "resized",
                            "cols": size.cols,
                            "rows": size.rows,
                        });
                        if sink.send(Message::Text(message.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(AttachEvent::Closed { reason })) => {
                        let message = json!({"type": "closed", "reason": reason});
                        let _ = sink.send(Message::Text(message.to_string().into())).await;
                        break;
                    }
                    Some(Err(error)) => {
                        tracing::warn!(%error, "agent attach stream failed");
                        break;
                    }
                    None => break,
                }
            }
            frame = stream.next() => {
                let Some(Ok(message)) = frame else { break };
                match message {
                    Message::Binary(data) => {
                        if session.input(data.to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Message::Text(text) => match serde_json::from_str::<Control>(text.as_str()) {
                        Ok(Control::Resize { cols, rows }) => {
                            if session.resize(TerminalSize { cols, rows }).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => {}
                    },
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            _ = recheck.tick() => {
                if !state.session_is_live(&user.token_hash).await {
                    tracing::info!("closing an agent socket whose session ended");
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

    let _ = session.close().await;
}
