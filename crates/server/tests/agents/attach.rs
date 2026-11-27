//! Agent attach-socket acceptance tests.
//!
//! The pane socket takes the same session-cookie and origin handshake as the
//! rest of the API. It relays output from the fake backend, drops browser
//! input and resizes, and closes with the socket, all without a live daemon.

mod fake;
mod support;

use axum::http::StatusCode;
use dandelion_server::agents::Agents;
use dandelion_terminals::{AttachCommand, TerminalSize};
use fake::{Behaviour, FakeBackend, ORIGIN, TERMINAL};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use support::{WAIT, agents_state, handshake, session_cookie, spawn_server, state_with};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

#[tokio::test]
async fn the_attach_socket_requires_a_session_and_the_origin() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Normal))).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let uri = format!("/api/agents/{TERMINAL}/ws");

    match handshake(address, &uri, None, Some(ORIGIN)).await {
        Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::UNAUTHORIZED),
        other => panic!("a socket without a session must be refused, got {other:?}"),
    }
    match handshake(address, &uri, Some(&cookie), Some("https://evil.example")).await {
        Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::FORBIDDEN),
        other => panic!("a foreign origin must be refused, got {other:?}"),
    }
}

#[tokio::test]
async fn the_attach_socket_observes_output_and_drops_input() {
    let backend = FakeBackend::new(Behaviour::Normal);
    let state = agents_state(&backend).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;

    let mut socket = handshake(
        address,
        &format!("/api/agents/{TERMINAL}/ws"),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("agent handshake");
    let mut attach = backend.attach_side(WAIT).await;
    assert_eq!(
        backend.recorded.lock().expect("recorded").attach,
        Some(dandelion_terminals::AttachOptions {
            mode: dandelion_terminals::AttachMode::Observe,
            size: TerminalSize {
                cols: 120,
                rows: 40
            },
            history: 200,
        })
    );

    attach.send_data(b"Allow this tool?").await;
    let frame = tokio::time::timeout(WAIT, socket.next())
        .await
        .expect("output within the timeout")
        .expect("a frame")
        .expect("a readable frame");
    match frame {
        Message::Binary(bytes) => assert_eq!(bytes.as_ref(), b"Allow this tool?"),
        other => panic!("expected the terminal output, got {other:?}"),
    }

    // The pane is read-only: browser input and resizes die at the server, so
    // the next backend command is the close, never an input or resize.
    socket
        .send(Message::Binary(b"ls\r".to_vec().into()))
        .await
        .expect("input frame");
    socket
        .send(Message::Text(
            json!({"type": "resize", "cols": 100, "rows": 30})
                .to_string()
                .into(),
        ))
        .await
        .expect("resize frame");
    socket.close(None).await.expect("close");
    assert_eq!(attach.next_command(WAIT).await, AttachCommand::Close);
}
