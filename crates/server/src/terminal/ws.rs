//! The terminal socket loop.
//!
//! Binary frames carry raw terminal bytes in both directions; text frames
//! carry control messages. Two blocking threads own the pseudo-terminal: one
//! reads the shell into a bounded queue, one drains a bounded input queue, so
//! a slow browser or a slow shell never blocks a Tokio worker. The shell ends
//! when the socket does.

use std::{sync::Arc, thread::JoinHandle, time::Duration};

use axum::extract::ws::{Message, WebSocket};
use dandelion_pty::Pty;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{broadcast::error::RecvError, mpsc};

use super::{ACTOR, Terminal};
use crate::{
    audit::AuditEntry,
    auth::{Revocation, middleware::RequestUser},
    state::AppState,
};

/// Largest frame accepted from the browser.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Output frames queued between the shell and the socket.
const OUTPUT_QUEUE: usize = 64;
/// Input frames queued between the socket and the shell.
const INPUT_QUEUE: usize = 16;
/// How often the socket revalidates its session.
const SESSION_RECHECK: Duration = Duration::from_secs(30);
/// How long a cleanup waits for the terminal's threads.
const JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Control messages a browser may send as text frames.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Control {
    /// Applies a new size to the shell's terminal.
    Resize { cols: u16, rows: u16 },
}

/// The two blocking threads and the queues between them and the socket.
struct Io {
    output_rx: mpsc::Receiver<Vec<u8>>,
    input_tx: mpsc::Sender<Vec<u8>>,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}

impl Io {
    /// Starts the reader and writer threads for one terminal.
    fn start(pty: &Arc<Pty>) -> std::io::Result<Self> {
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(OUTPUT_QUEUE);
        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(INPUT_QUEUE);

        let reader = std::thread::Builder::new()
            .name("dandelion-pty-reader".to_owned())
            .spawn({
                let pty = Arc::clone(pty);
                move || {
                    let mut buf = [0u8; 8192];
                    loop {
                        match pty.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(count) => {
                                if output_tx.blocking_send(buf[..count].to_vec()).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            })?;
        let writer = std::thread::Builder::new()
            .name("dandelion-pty-writer".to_owned())
            .spawn({
                let pty = Arc::clone(pty);
                move || {
                    while let Some(bytes) = input_rx.blocking_recv() {
                        if pty.write(&bytes).is_err() {
                            break;
                        }
                    }
                }
            })?;

        Ok(Self {
            output_rx,
            input_tx,
            reader,
            writer,
        })
    }

    /// Drops the queues, which wakes both threads, then joins them off the
    /// runtime with a bound: a process that somehow still holds the terminal
    /// must not pin the task or the slot.
    async fn shutdown(self) {
        let Self {
            output_rx,
            input_tx,
            reader,
            writer,
        } = self;
        drop(output_rx);
        drop(input_tx);
        let joined = tokio::task::spawn_blocking(move || {
            let _ = reader.join();
            let _ = writer.join();
        });
        if tokio::time::timeout(JOIN_TIMEOUT, joined).await.is_err() {
            tracing::warn!("a terminal I/O thread outlived the terminal");
        }
    }
}

/// Drives one terminal socket until the browser leaves or the session ends.
pub async fn connection(
    socket: WebSocket,
    state: AppState,
    user: RequestUser,
    terminal: Arc<Terminal>,
) {
    let Ok(mut io) = Io::start(terminal.pty()) else {
        tracing::error!("cannot start the terminal I/O threads");
        terminal.detach();
        return;
    };
    let (mut sink, mut stream) = socket.split();

    let mut revocations = state.auth.subscribe_revocations();
    let mut recheck = tokio::time::interval(SESSION_RECHECK);
    recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    recheck.tick().await;

    loop {
        tokio::select! {
            frame = stream.next() => {
                let Some(Ok(message)) = frame else { break };
                match message {
                    Message::Binary(data) => {
                        if io.input_tx.send(data.to_vec()).await.is_err() {
                            break;
                        }
                    }
                    Message::Text(text) => match serde_json::from_str::<Control>(text.as_str()) {
                        Ok(Control::Resize { cols, rows }) => {
                            if terminal.pty().resize(cols, rows).is_err() {
                                break;
                            }
                        }
                        Err(_) => {}
                    },
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            output = io.output_rx.recv() => {
                let Some(bytes) = output else { break };
                if sink.send(Message::Binary(bytes.into())).await.is_err() {
                    break;
                }
            }
            _ = recheck.tick() => {
                if !state.session_is_live(&user.token_hash).await {
                    tracing::info!("closing a terminal whose session ended");
                    break;
                }
            }
            revocation = revocations.recv() => {
                match revocation {
                    Ok(Revocation::Session(hash)) if hash == user.token_hash => break,
                    Ok(Revocation::User(id)) if id == user.user_id => break,
                    Ok(_) | Err(RecvError::Closed) => {}
                    Err(RecvError::Lagged(_)) => break,
                }
            }
        }
    }

    // Free the slot before joining anything: the shell kill ends the reader,
    // and the bounded join below covers a process that outlives it.
    terminal.pty().kill();
    terminal.detach();
    if let Err(error) = state.terminals.close(terminal.id()) {
        tracing::debug!(%error, "the terminal was already closed");
    }
    audit_close(&state, terminal.id()).await;
    io.shutdown().await;
}

/// Records the socket-driven close so the audit trail has both ends.
async fn audit_close(state: &AppState, id: uuid::Uuid) {
    let detail = serde_json::json!({ "outcome": "socket_closed" }).to_string();
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor: ACTOR,
            action: "terminal.close",
            target: &format!("terminal:{id}"),
            detail: Some(&detail),
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}
