//! Agent endpoint and attach-socket acceptance tests.
//!
//! A fake backend stands in for herdr, so the tests cover authentication,
//! every action mapping, the audit rows, the error statuses, and the attach
//! socket without a live daemon. The live path is an ignored test in
//! `agents_live.rs`.

mod fake;

use std::{net::SocketAddr, time::Duration};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use dandelion_server::{
    agents::Agents,
    app::build_router,
    audit::AuditLog,
    auth::{
        Auth,
        origin::PublicOrigin,
        session::now_millis,
        token::{hash_token, random_token},
    },
    claude::ClaudeMonitor,
    containers::Containers,
    metrics::Metrics,
    services::Services,
    state::AppState,
    storage,
    ws::hub::Hub,
};
use dandelion_terminals::{AttachCommand, KeyName, TerminalBackend, TerminalSize};
use fake::{Behaviour, COOKIE, FakeBackend, ORIGIN, TERMINAL};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as WsError, Message, client::IntoClientRequest},
};
use tower::ServiceExt;

const WAIT: Duration = Duration::from_secs(5);

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn state_with(agents: Agents) -> AppState {
    state_with_claude(agents, ClaudeMonitor::disabled()).await
}

async fn state_with_claude(agents: Agents, claude: ClaudeMonitor) -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        Hub::default(),
        Metrics::new(pool),
        Services::disabled(),
        Containers::disabled(),
        agents,
        claude,
    )
}

async fn agents_state(backend: &std::sync::Arc<FakeBackend>) -> AppState {
    state_with(Agents::with_backend(
        std::sync::Arc::clone(backend) as std::sync::Arc<dyn TerminalBackend>
    ))
    .await
}

async fn session_cookie(state: &AppState) -> String {
    let now = now_millis();
    let owner = state.auth.store().ensure_owner(now).await.expect("owner");
    let token = random_token().expect("session token");
    state
        .auth
        .store()
        .insert_session(&hash_token(&token), &owner.id, now, now + 3_600_000)
        .await
        .expect("session");

    format!("{COOKIE}={token}")
}

async fn send(
    state: &AppState,
    method: Method,
    uri: &str,
    body: Option<&Value>,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> Response {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };

    build_router(state.clone())
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response")
}

async fn json_body(response: Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON body")
}

async fn act(state: &AppState, cookie: &str, action: Value) -> Response {
    send(
        state,
        Method::POST,
        "/api/agents/action",
        Some(&json!({"terminal": TERMINAL, "action": action})),
        Some(cookie),
        Some(ORIGIN),
    )
    .await
}

async fn one_audit_row(state: &AppState) -> (String, String, Value) {
    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1, "exactly one row per mutating request");
    let detail =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    (records[0].action.clone(), records[0].target.clone(), detail)
}

async fn spawn_server(state: AppState) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a test listener");
    let address = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        axum::serve(listener, build_router(state))
            .await
            .expect("test server");
    });

    address
}

async fn handshake(
    address: SocketAddr,
    uri: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> Result<Client, WsError> {
    let mut request = format!("ws://{address}{uri}")
        .into_client_request()
        .expect("websocket request");
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("cookie", cookie.parse().expect("cookie header"));
    }
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("origin", origin.parse().expect("origin header"));
    }

    connect_async(request).await.map(|(socket, _)| socket)
}

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
    let session = &body["claude"][0];
    assert_eq!(session["terminal"], TERMINAL);
    assert_eq!(session["sessionId"], "session-424242");
    assert_eq!(session["status"], "waiting");
    assert_eq!(session["waitingFor"], "permission prompt");
    assert_eq!(session["usage"]["tokens"]["inputTokens"], 10);
    assert_eq!(session["usage"]["messages"], 1);
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
async fn the_attach_socket_observes_and_relays_both_ways() {
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

    socket
        .send(Message::Binary(b"ls\r".to_vec().into()))
        .await
        .expect("input frame");
    assert_eq!(
        attach.next_command(WAIT).await,
        AttachCommand::Input(b"ls\r".to_vec())
    );

    socket
        .send(Message::Text(
            json!({"type": "resize", "cols": 100, "rows": 30})
                .to_string()
                .into(),
        ))
        .await
        .expect("resize frame");
    assert_eq!(
        attach.next_command(WAIT).await,
        AttachCommand::Resize(TerminalSize {
            cols: 100,
            rows: 30,
        })
    );

    socket.close(None).await.expect("close");
    assert_eq!(attach.next_command(WAIT).await, AttachCommand::Close);
}
