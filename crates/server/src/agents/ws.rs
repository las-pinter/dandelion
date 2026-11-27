//! The agent attach socket: raw pane bytes and agent state in one stream.
//!
//! Read-only by design: the pane observes the agent, and browser input or
//! resizes are dropped here so control stays in the audited action API.

use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use dandelion_terminals::AttachEvent;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::{
    auth::{Revocation, middleware::RequestUser},
    state::AppState,
};

/// Largest frame accepted from the browser. Frames are read and discarded:
/// the attach socket never forwards them.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// How often the socket revalidates its session.
const SESSION_RECHECK: Duration = Duration::from_secs(30);

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
                    // The pane is observe-only: browser input and resizes are
                    // dropped here, not just in the backend, so control stays
                    // in the audited action API.
                    Message::Binary(_) | Message::Text(_) | Message::Ping(_) | Message::Pong(_) => {}
                    Message::Close(_) => break,
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
