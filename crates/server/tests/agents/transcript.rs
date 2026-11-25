//! Transcript socket acceptance tests.
//!
//! A synthetic Claude state directory supplies a session, a transcript, and a
//! subagent; the tests cover auth, the opening page, tailing, paging, and the
//! subagent list without a live Claude Code process.

mod fake;
mod support;

use axum::http::StatusCode;
use dandelion_server::agents::Agents;
use fake::ORIGIN;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use support::{Client, handshake, session_cookie, spawn_server, state_with, state_with_claude};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

async fn next_json(socket: &mut Client) -> Value {
    loop {
        let frame = tokio::time::timeout(support::WAIT, socket.next())
            .await
            .expect("a frame within the timeout")
            .expect("a frame")
            .expect("a readable frame");
        if let Message::Text(text) = frame {
            return serde_json::from_str(text.as_str()).expect("JSON frame");
        }
    }
}

#[tokio::test]
async fn the_transcript_socket_requires_a_session_and_the_origin() {
    let state = state_with(Agents::disabled()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let uri = "/api/claude/session-424242/transcript/ws";

    match handshake(address, uri, None, Some(ORIGIN)).await {
        Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::UNAUTHORIZED),
        other => panic!("a socket without a session must be refused, got {other:?}"),
    }
    match handshake(address, uri, Some(&cookie), Some("https://evil.example")).await {
        Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::FORBIDDEN),
        other => panic!("a foreign origin must be refused, got {other:?}"),
    }
}

#[tokio::test]
async fn the_transcript_socket_pages_tails_and_lists_subagents() {
    use std::io::Write as _;

    let fixture = fake::claude_fixture();
    assert!(fixture.monitor.refresh(), "the fixture session is found");
    let state = state_with_claude(Agents::disabled(), fixture.monitor.clone()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let mut socket = handshake(
        address,
        "/api/claude/session-424242/transcript/ws",
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("transcript handshake");

    let page = next_json(&mut socket).await;
    assert_eq!(page["type"], "page");
    let text = serde_json::to_string(&page["records"]).expect("json");
    assert!(text.contains("hello there"), "{text}");
    assert_eq!(page["hasEarlier"], false);

    let subagents = next_json(&mut socket).await;
    assert_eq!(subagents["type"], "subagents");
    assert_eq!(subagents["items"][0]["agent"], "agent-a1");
    assert_eq!(subagents["items"][0]["toolUseId"], "toolu_x");

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(fixture.transcript())
        .expect("append");
    writeln!(
        file,
        r#"{{"type":"user","message":{{"content":"later message"}}}}"#
    )
    .expect("line");
    drop(file);

    let appended = loop {
        let frame = next_json(&mut socket).await;
        if frame["type"] == "appended" {
            break frame;
        }
    };
    let text = serde_json::to_string(&appended["records"]).expect("json");
    assert!(text.contains("later message"), "{text}");
}

#[tokio::test]
async fn the_transcript_socket_pages_backwards_on_request() {
    use std::io::Write as _;

    let fixture = fake::claude_fixture();
    assert!(fixture.monitor.refresh(), "the fixture session is found");
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(fixture.transcript())
            .expect("append");
        for index in 0..60 {
            writeln!(
                file,
                r#"{{"type":"user","message":{{"content":"line {index}"}}}}"#
            )
            .expect("line");
        }
    }
    let state = state_with_claude(Agents::disabled(), fixture.monitor.clone()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let mut socket = handshake(
        address,
        "/api/claude/session-424242/transcript/ws",
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("transcript handshake");

    let page = next_json(&mut socket).await;
    assert_eq!(page["type"], "page");
    assert_eq!(page["records"].as_array().expect("records").len(), 50);
    assert_eq!(page["hasEarlier"], true);

    socket
        .send(Message::Text(r#"{"type":"earlier"}"#.into()))
        .await
        .expect("earlier request");
    let earlier = loop {
        let frame = next_json(&mut socket).await;
        if frame["type"] == "earlier" {
            break frame;
        }
    };
    assert_eq!(earlier["records"].as_array().expect("records").len(), 12);
    assert_eq!(earlier["hasEarlier"], false);
    let text = serde_json::to_string(&earlier["records"]).expect("json");
    assert!(text.contains("hello there"), "{text}");
}

#[tokio::test]
async fn the_transcript_socket_streams_one_subagent_file() {
    let fixture = fake::claude_fixture();
    assert!(fixture.monitor.refresh(), "the fixture session is found");
    // Runs are flat: a run started by the subagent sits beside it.
    let runs = fixture
        .directory
        .path()
        .join("claude/projects/-work-example/session-424242/subagents");
    std::fs::write(
        runs.join("agent-a2.meta.json"),
        r#"{"toolUseId":"toolu_z","description":"nested run"}"#,
    )
    .expect("nested meta");
    std::fs::write(
        runs.join("agent-a2.jsonl"),
        "{\"type\":\"user\",\"message\":{\"content\":\"nested work\"}}\n",
    )
    .expect("nested transcript");
    let state = state_with_claude(Agents::disabled(), fixture.monitor.clone()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let mut socket = handshake(
        address,
        "/api/claude/session-424242/transcript/ws?agent=agent-a1",
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("subagent handshake");

    let page = next_json(&mut socket).await;

    assert_eq!(page["type"], "page");
    let text = serde_json::to_string(&page["records"]).expect("json");
    assert!(text.contains("sub work"), "{text}");

    let subagents = next_json(&mut socket).await;
    assert_eq!(subagents["type"], "subagents");
    let items = subagents["items"].as_array().expect("items");
    assert!(
        items
            .iter()
            .any(|item| item["agent"] == "agent-a2" && item["toolUseId"] == "toolu_z"),
        "a run started by the subagent is listed too: {items:?}"
    );
}

#[tokio::test]
async fn an_unknown_session_gets_an_error_frame() {
    let state = state_with(Agents::disabled()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let mut socket = handshake(
        address,
        "/api/claude/no-such-session/transcript/ws",
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("handshake");

    let frame = next_json(&mut socket).await;

    assert_eq!(frame["type"], "error");
    assert_eq!(frame["message"], "no readable transcript for this session");
}

#[tokio::test]
async fn a_subagent_started_while_watching_is_announced() {
    use std::io::Write as _;

    let fixture = fake::claude_fixture();
    assert!(fixture.monitor.refresh(), "the fixture session is found");
    let state = state_with_claude(Agents::disabled(), fixture.monitor.clone()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let mut socket = handshake(
        address,
        "/api/claude/session-424242/transcript/ws",
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("transcript handshake");
    assert_eq!(next_json(&mut socket).await["type"], "page");
    assert_eq!(
        next_json(&mut socket).await["items"]
            .as_array()
            .expect("items")
            .len(),
        1
    );

    let subagents = fixture
        .directory
        .path()
        .join("claude/projects/-work-example/session-424242/subagents/agent-a2.meta.json");
    std::fs::write(
        &subagents,
        r#"{"toolUseId":"toolu_y","description":"late run"}"#,
    )
    .expect("new meta");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(fixture.transcript())
        .expect("append");
    writeln!(
        file,
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"toolu_y","name":"Task","input":{{}}}}]}}}}"#
    )
    .expect("line");
    drop(file);

    let frame = loop {
        let frame = next_json(&mut socket).await;
        if frame["type"] == "subagents" {
            break frame;
        }
    };
    let items = frame["items"].as_array().expect("items");
    assert_eq!(items.len(), 2, "the new run is announced");
    assert_eq!(items[1]["agent"], "agent-a2");
}

#[tokio::test]
async fn a_run_is_announced_after_its_task_record_without_another_append() {
    use std::io::Write as _;

    let fixture = fake::claude_fixture();
    assert!(fixture.monitor.refresh(), "the fixture session is found");
    let state = state_with_claude(Agents::disabled(), fixture.monitor.clone()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let mut socket = handshake(
        address,
        "/api/claude/session-424242/transcript/ws",
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("transcript handshake");
    assert_eq!(next_json(&mut socket).await["type"], "page");
    assert_eq!(next_json(&mut socket).await["type"], "subagents");

    // Claude Code records the Task call before the run's metadata exists.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(fixture.transcript())
        .expect("append");
    writeln!(
        file,
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"toolu_w","name":"Task","input":{{}}}}]}}}}"#
    )
    .expect("line");
    drop(file);
    loop {
        if next_json(&mut socket).await["type"] == "appended" {
            break;
        }
    }

    let runs = fixture
        .directory
        .path()
        .join("claude/projects/-work-example/session-424242/subagents");
    std::fs::write(
        runs.join("agent-a3.meta.json"),
        r#"{"toolUseId":"toolu_w","description":"late meta"}"#,
    )
    .expect("late meta");

    let frame = loop {
        let frame = next_json(&mut socket).await;
        if frame["type"] == "subagents" {
            break frame;
        }
    };
    assert!(
        frame["items"]
            .as_array()
            .expect("items")
            .iter()
            .any(|item| item["agent"] == "agent-a3"),
        "the late metadata is announced without another record: {frame}"
    );
}

#[tokio::test]
async fn a_replaced_transcript_reloads_a_capped_page() {
    use std::io::Write as _;

    let fixture = fake::claude_fixture();
    assert!(fixture.monitor.refresh(), "the fixture session is found");
    let state = state_with_claude(Agents::disabled(), fixture.monitor.clone()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn_server(state).await;
    let mut socket = handshake(
        address,
        "/api/claude/session-424242/transcript/ws",
        Some(&cookie),
        Some(ORIGIN),
    )
    .await
    .expect("transcript handshake");
    assert_eq!(next_json(&mut socket).await["type"], "page");
    assert_eq!(next_json(&mut socket).await["type"], "subagents");

    // Grow the file well past the replacement, so the replacement shrinks it.
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(fixture.transcript())
            .expect("append");
        for index in 0..200 {
            writeln!(
                file,
                r#"{{"type":"user","message":{{"content":"grow {index}"}}}}"#
            )
            .expect("line");
        }
    }
    loop {
        if next_json(&mut socket).await["type"] == "appended" {
            break;
        }
    }

    // Replace it with 60 short records: the tail resets and pages.
    let lines: Vec<String> = (0..60)
        .map(|index| format!(r#"{{"type":"user","message":{{"content":"r{index}"}}}}"#))
        .collect();
    let replaced = format!("{}\n", lines.join("\n"));
    std::fs::write(fixture.transcript(), replaced).expect("replace");

    let page = loop {
        let frame = next_json(&mut socket).await;
        if frame["type"] == "page" {
            break frame;
        }
    };
    assert_eq!(page["records"].as_array().expect("records").len(), 50);
    assert_eq!(page["hasEarlier"], true);
}
