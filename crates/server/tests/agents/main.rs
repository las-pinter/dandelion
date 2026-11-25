//! Agent endpoint acceptance tests.
//!
//! A fake backend stands in for herdr, so the tests cover authentication,
//! every action mapping, the audit rows, and the error statuses without a live
//! daemon. The attach socket lives in `attach.rs`; the live path is an ignored
//! test in `agents_live.rs`.

mod fake;
mod support;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use dandelion_server::agents::{Agents, spawn_publisher_with};
use dandelion_terminals::KeyName;
use fake::{Behaviour, FakeBackend, ORIGIN, TERMINAL};
use serde_json::json;
use support::{
    act, agents_state, json_body, one_audit_row, send, session_cookie, state_with,
    state_with_claude,
};

#[tokio::test]
async fn agents_require_a_session() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Normal))).await;

    let response = send(&state, Method::GET, "/api/agents", None, None, None).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_snapshot_lists_agents() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Normal))).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/agents",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], true);
    assert_eq!(body["agents"][0]["id"], TERMINAL);
    assert_eq!(body["agents"][0]["agent_state"]["state"], "blocked");
    assert_eq!(
        body["agents"][0]["agent_state"]["blocked_reason"],
        "permission"
    );
}

#[tokio::test]
async fn an_adopted_backend_serves_the_listing_and_actions() {
    let agents = Agents::disabled();
    let state = state_with(agents.clone()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/agents",
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["available"], false);

    let backend = FakeBackend::new(Behaviour::Normal);
    agents.install(backend.clone());

    let response = send(
        &state,
        Method::GET,
        "/api/agents",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let body = json_body(response).await;
    assert_eq!(body["available"], true, "no restart was needed");
    assert_eq!(body["agents"][0]["id"], TERMINAL);

    let response = act(
        &state,
        &cookie,
        json!({"kind": "prompt", "text": "run the tests"}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "actions work too"
    );
    {
        let recorded = backend.recorded.lock().expect("recorded");
        assert_eq!(
            recorded.texts,
            vec![(TERMINAL.to_owned(), "run the tests".to_owned())]
        );
    }
}

#[tokio::test]
async fn the_publisher_follows_a_backend_that_appears_after_startup() {
    let agents = Agents::disabled();
    let state = state_with(agents.clone()).await;
    let mut events = state.hub.subscribe("agents");
    spawn_publisher_with(state.clone(), Duration::from_millis(1));
    // Let the publisher see no backend once, so its wait branch is what runs.
    tokio::task::yield_now().await;

    agents.install(FakeBackend::new(Behaviour::Normal));

    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .expect("a resync arrives once the backend appears")
        .expect("the subscription is live");
    assert_eq!(event["kind"], "resync");
}

#[tokio::test]
async fn the_snapshot_carries_the_claude_session() {
    let fixture = fake::claude_fixture();
    assert!(fixture.monitor.refresh(), "the fixture session is found");
    let state = state_with_claude(Agents::disabled(), fixture.monitor.clone()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/agents",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    let sessions = body["claude"].as_array().expect("a Claude list");
    let session = sessions
        .iter()
        .find(|entry| entry["terminal"] == TERMINAL)
        .expect("the managed session");
    assert_eq!(session["sessionId"], "session-424242");
    assert_eq!(session["cwd"], "/work/example");
    assert_eq!(session["status"], "waiting");
    assert_eq!(session["waitingFor"], "permission prompt");
    assert_eq!(session["usage"]["tokens"]["inputTokens"], 10);
    assert_eq!(session["usage"]["messages"], 1);

    let unmanaged = sessions
        .iter()
        .find(|entry| entry["terminal"].is_null())
        .expect("the session outside herdr");
    assert_eq!(unmanaged["sessionId"], "session-424243");
    assert_eq!(unmanaged["cwd"], "/work/plain");
}

#[tokio::test]
async fn the_snapshot_reports_no_backend() {
    let state = state_with(Agents::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/agents",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["available"], false);
    assert_eq!(body["agents"], json!([]));
}

#[tokio::test]
async fn a_prompt_is_sent_and_audited() {
    let backend = FakeBackend::new(Behaviour::Normal);
    let state = agents_state(&backend).await;
    let cookie = session_cookie(&state).await;

    let response = act(
        &state,
        &cookie,
        json!({"kind": "prompt", "text": "run the tests"}),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    {
        let recorded = backend.recorded.lock().expect("recorded");
        assert_eq!(
            recorded.texts,
            vec![(TERMINAL.to_owned(), "run the tests".to_owned())]
        );
    }
    let (action, target, detail) = one_audit_row(&state).await;
    assert_eq!(action, "agent.action");
    assert_eq!(target, format!("terminal:{TERMINAL}"));
    assert_eq!(detail["kind"], "prompt");
    assert_eq!(detail["outcome"], "ok");
}

#[tokio::test]
async fn approve_sends_enter_and_deny_sends_escape() {
    let backend = FakeBackend::new(Behaviour::Normal);
    let state = agents_state(&backend).await;
    let cookie = session_cookie(&state).await;

    assert_eq!(
        act(&state, &cookie, json!({"kind": "approve"}))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        act(&state, &cookie, json!({"kind": "deny"})).await.status(),
        StatusCode::NO_CONTENT
    );

    let recorded = backend.recorded.lock().expect("recorded");
    assert_eq!(recorded.keys[0].1, vec!["enter".to_owned()]);
    assert_eq!(recorded.keys[1].1, vec![KeyName::escape().to_string()]);
}

#[tokio::test]
async fn interrupt_and_kill_reach_the_backend() {
    let backend = FakeBackend::new(Behaviour::Normal);
    let state = agents_state(&backend).await;
    let cookie = session_cookie(&state).await;

    assert_eq!(
        act(&state, &cookie, json!({"kind": "interrupt"}))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        act(&state, &cookie, json!({"kind": "kill"})).await.status(),
        StatusCode::NO_CONTENT
    );

    let recorded = backend.recorded.lock().expect("recorded");
    assert_eq!(recorded.interrupted, vec![TERMINAL.to_owned()]);
    assert_eq!(recorded.killed, vec![TERMINAL.to_owned()]);
}

#[tokio::test]
async fn a_blocked_send_is_refused_and_audited() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Blocked))).await;
    let cookie = session_cookie(&state).await;

    let response = act(&state, &cookie, json!({"kind": "prompt", "text": "hello"})).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "agent_blocked");
    let (_, _, detail) = one_audit_row(&state).await;
    assert_eq!(detail["outcome"], "agent_blocked");
}

#[tokio::test]
async fn an_unknown_agent_is_404() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Missing))).await;
    let cookie = session_cookie(&state).await;

    let response = act(&state, &cookie, json!({"kind": "prompt", "text": "hello"})).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_bad_key_name_is_refused() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Normal))).await;
    let cookie = session_cookie(&state).await;

    let response = act(
        &state,
        &cookie,
        json!({"kind": "keys", "keys": ["not a key"]}),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_key");
}

#[tokio::test]
async fn spawn_returns_the_terminal_and_audits() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Normal))).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/agents/spawn",
        Some(&json!({
            "line": "claude",
            "cwd": "/home/dev/worktree",
            "name": "worktree-shell",
        })),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], TERMINAL);
    let (action, target, detail) = one_audit_row(&state).await;
    assert_eq!(action, "agent.spawn");
    assert_eq!(target, "terminal:new");
    assert_eq!(detail["outcome"], "ok");
}

#[tokio::test]
async fn the_screen_is_served() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Normal))).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        &format!("/api/agents/{TERMINAL}/screen"),
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["text"], "Allow this tool?");
    assert_eq!(
        state.audit.recent(10).await.expect("audit rows").len(),
        0,
        "a read is never audited"
    );
}

#[tokio::test]
async fn a_mutating_request_without_an_origin_is_refused() {
    let state = state_with(Agents::with_backend(FakeBackend::new(Behaviour::Normal))).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/agents/action",
        Some(&json!({"terminal": TERMINAL, "action": {"kind": "approve"}})),
        Some(&cookie),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(state.audit.recent(10).await.expect("audit rows").len(), 0);
}
