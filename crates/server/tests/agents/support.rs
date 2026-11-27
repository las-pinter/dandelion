//! Shared harness for the agent acceptance tests.
//!
//! Each test binary includes this module; it builds an in-memory server, signs
//! a session cookie, and speaks HTTP or WebSocket to the router.

#![allow(dead_code)] // each test binary uses a subset

use dandelion_documents::Documents;
use std::{net::SocketAddr, time::Duration};

use crate::fake::{COOKIE, FakeBackend, ORIGIN, TERMINAL};
use axum::{
    body::Body,
    http::{Method, Request, header},
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
use dandelion_terminals::TerminalBackend;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error as WsError, client::IntoClientRequest},
};
use tower::ServiceExt;

pub const WAIT: Duration = Duration::from_secs(5);

pub type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn state_with(agents: Agents) -> AppState {
    state_with_claude(agents, ClaudeMonitor::disabled()).await
}

pub async fn state_with_claude(agents: Agents, claude: ClaudeMonitor) -> AppState {
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
        Documents::disabled(),
    )
}

pub async fn agents_state(backend: &std::sync::Arc<FakeBackend>) -> AppState {
    state_with(Agents::with_backend(
        std::sync::Arc::clone(backend) as std::sync::Arc<dyn TerminalBackend>
    ))
    .await
}

pub async fn session_cookie(state: &AppState) -> String {
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

pub async fn send(
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

pub async fn json_body(response: Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON body")
}

pub async fn act(state: &AppState, cookie: &str, action: Value) -> Response {
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

pub async fn one_audit_row(state: &AppState) -> (String, String, Value) {
    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1, "exactly one row per mutating request");
    let detail =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    (records[0].action.clone(), records[0].target.clone(), detail)
}

pub async fn spawn_server(state: AppState) -> SocketAddr {
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

pub async fn handshake(
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
