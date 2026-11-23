//! Terminal endpoint and socket acceptance tests.
//!
//! These spawn real `/bin/sh` shells on real pseudo-terminals, which every
//! supported CI runner provides, so the round trip is exercised end to end.

use std::{net::SocketAddr, time::Duration};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use dandelion_server::{
    app::build_router,
    audit::AuditLog,
    auth::{
        Auth,
        origin::PublicOrigin,
        session::now_millis,
        token::{hash_token, random_token},
    },
    containers::Containers,
    metrics::Metrics,
    services::Services,
    state::AppState,
    storage,
    terminal::MAX_TERMINALS,
    ws::hub::Hub,
};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as WsError, Message, client::IntoClientRequest},
};
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";
const WAIT: Duration = Duration::from_secs(10);

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn test_state() -> AppState {
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
    )
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

async fn open_terminal(state: &AppState, cookie: &str) -> Value {
    let response = send(
        state,
        Method::POST,
        "/api/terminals",
        Some(&json!({"cols": 90, "rows": 30})),
        Some(cookie),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn list_terminals(state: &AppState, cookie: &str) -> Value {
    let response = send(
        state,
        Method::GET,
        "/api/terminals",
        None,
        Some(cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn spawn(state: AppState) -> SocketAddr {
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

async fn handshake(address: SocketAddr, uri: &str, cookie: &str) -> Result<Client, WsError> {
    let mut request = format!("ws://{address}{uri}")
        .into_client_request()
        .expect("websocket request");
    request
        .headers_mut()
        .insert("cookie", cookie.parse().expect("cookie header"));
    request
        .headers_mut()
        .insert("origin", ORIGIN.parse().expect("origin header"));

    connect_async(request).await.map(|(socket, _)| socket)
}

#[tokio::test]
async fn terminals_require_a_session() {
    let state = test_state().await;

    let response = send(&state, Method::GET, "/api/terminals", None, None, None).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn open_spawns_a_shell_and_audits_it() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let info = open_terminal(&state, &cookie).await;

    let id = info["id"].as_str().expect("id");
    let shell = info["shell"].as_str().expect("shell");
    assert_ne!(id, "");
    assert_ne!(shell, "");
    assert_eq!(info["cols"], 90);
    assert_eq!(info["rows"], 30);

    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action, "terminal.open");
    assert_eq!(
        records[0].target,
        format!("terminal:{}", info["id"].as_str().expect("id"))
    );
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "ok");
}

#[tokio::test]
async fn list_shows_open_terminals_and_close_ends_them() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let first = open_terminal(&state, &cookie).await;
    let second = open_terminal(&state, &cookie).await;

    let listed = list_terminals(&state, &cookie).await;
    assert_eq!(listed.as_array().expect("array").len(), 2);

    let response = send(
        &state,
        Method::DELETE,
        &format!("/api/terminals/{}", first["id"].as_str().expect("id")),
        None,
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let listed = list_terminals(&state, &cookie).await;
    let ids: Vec<&str> = listed
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|info| info["id"].as_str())
        .collect();
    assert_eq!(ids, vec![second["id"].as_str().expect("id")]);

    let actions: Vec<String> = state
        .audit
        .recent(10)
        .await
        .expect("audit rows")
        .into_iter()
        .map(|record| record.action)
        .collect();
    assert!(actions.contains(&"terminal.open".to_owned()));
    assert!(actions.contains(&"terminal.close".to_owned()));
}

#[tokio::test]
async fn closing_an_unknown_terminal_is_refused_and_audited() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    for id in ["00000000-0000-4000-8000-000000000000", "nonsense"] {
        let response = send(
            &state,
            Method::DELETE,
            &format!("/api/terminals/{id}"),
            None,
            Some(&cookie),
            Some(ORIGIN),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{id}");
    }

    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].action, "terminal.close");
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "not_found");
}

#[tokio::test]
async fn a_mutating_request_without_an_origin_is_refused() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/terminals",
        Some(&json!({})),
        Some(&cookie),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(
        records.len(),
        0,
        "a refused origin never reaches the handler"
    );
}

#[tokio::test]
async fn the_cap_refuses_one_more_shell() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    for _ in 0..MAX_TERMINALS {
        let _ = open_terminal(&state, &cookie).await;
    }

    let response = send(
        &state,
        Method::POST,
        "/api/terminals",
        Some(&json!({})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "limit_reached");
    let records = state.audit.recent(20).await.expect("audit rows");
    assert_eq!(records.len(), MAX_TERMINALS + 1);
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "refused");
}

#[tokio::test]
async fn the_socket_carries_output_input_and_resize() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let info = open_terminal(&state, &cookie).await;
    let id = info["id"].as_str().expect("id").to_owned();
    let address = spawn(state.clone()).await;

    let mut socket = handshake(address, &format!("/api/terminals/{id}/ws"), &cookie)
        .await
        .expect("terminal handshake");

    socket
        .send(Message::Binary(b"printf terminal-probe\\n".to_vec().into()))
        .await
        .expect("input frame");

    let mut output = String::new();
    while !output.contains("terminal-probe") {
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .expect("output within the timeout");
        match frame {
            Some(Ok(Message::Binary(bytes))) => {
                output.push_str(&String::from_utf8_lossy(&bytes));
            }
            Some(Ok(_)) => {}
            other => panic!("socket ended early: {other:?}"),
        }
    }

    socket
        .send(Message::Text(
            json!({"type": "resize", "cols": 100, "rows": 40})
                .to_string()
                .into(),
        ))
        .await
        .expect("resize frame");

    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let listed = list_terminals(&state, &cookie).await;
        let size = listed
            .as_array()
            .expect("array")
            .first()
            .map(|info| (info["cols"].clone(), info["rows"].clone()));
        if size == Some((json!(100), json!(40))) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the resize reached the terminal: {listed}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    socket.close(None).await.expect("close");

    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        if list_terminals(&state, &cookie)
            .await
            .as_array()
            .is_some_and(Vec::is_empty)
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the shell ends with the socket"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let records = state.audit.recent(20).await.expect("audit rows");
    assert!(
        records.iter().any(|record| {
            record.action == "terminal.close"
                && record
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("socket_closed"))
        }),
        "the socket close is audited"
    );
}

#[tokio::test]
async fn a_second_socket_is_refused() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let info = open_terminal(&state, &cookie).await;
    let id = info["id"].as_str().expect("id").to_owned();
    let address = spawn(state.clone()).await;

    let mut first = handshake(address, &format!("/api/terminals/{id}/ws"), &cookie)
        .await
        .expect("the first socket attaches");

    match handshake(address, &format!("/api/terminals/{id}/ws"), &cookie).await {
        Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::CONFLICT),
        other => panic!("a second socket must be refused, got {other:?}"),
    }

    first.close(None).await.expect("close");
}

#[tokio::test]
async fn delete_ends_an_attached_shell() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let info = open_terminal(&state, &cookie).await;
    let id = info["id"].as_str().expect("id").to_owned();
    let address = spawn(state.clone()).await;
    let mut socket = handshake(address, &format!("/api/terminals/{id}/ws"), &cookie)
        .await
        .expect("the socket attaches");

    let response = send(
        &state,
        Method::DELETE,
        &format!("/api/terminals/{id}"),
        None,
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        match tokio::time::timeout(WAIT, socket.next()).await {
            Ok(None | Some(Ok(Message::Close(_)) | Err(_))) => break,
            Ok(Some(Ok(_))) => {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "the deleted shell closes its socket"
                );
            }
            Err(error) => panic!("the deleted shell never closed its socket: {error}"),
        }
    }
}

#[tokio::test]
async fn the_socket_rejects_a_foreign_origin() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let info = open_terminal(&state, &cookie).await;
    let id = info["id"].as_str().expect("id").to_owned();
    let address = spawn(state.clone()).await;

    let mut request = format!("ws://{address}/api/terminals/{id}/ws")
        .into_client_request()
        .expect("websocket request");
    request
        .headers_mut()
        .insert("cookie", cookie.parse().expect("cookie header"));
    request.headers_mut().insert(
        "origin",
        "https://elsewhere.example".parse().expect("origin header"),
    );

    match connect_async(request).await {
        Err(WsError::Http(response)) => {
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        other => panic!("expected a refused handshake, got {other:?}"),
    }
}
