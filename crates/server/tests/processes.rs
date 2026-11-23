//! Process endpoint acceptance tests.
//!
//! The list and the signal tests touch this host's `/proc` and one throwaway
//! `sleep` child. A refused request still writes one audit row, so an attempt
//! is visible when nothing changed.

use std::{process::Command, time::Duration};

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
    ws::hub::Hub,
};
use dandelion_system::process::ProcessService;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";

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

/// Spawns a throwaway process the test may signal or renice.
fn spawn_sleeper() -> std::process::Child {
    Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn a sleep child")
}

/// Waits up to two seconds for a child to exit.
fn wait_for_exit(child: &mut std::process::Child) -> bool {
    for _ in 0..100 {
        if child.try_wait().expect("child status").is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn pid_of(child: &std::process::Child) -> i32 {
    i32::try_from(child.id()).expect("pid fits i32")
}

#[tokio::test]
async fn processes_require_a_session() {
    let state = test_state().await;

    let list = send(&state, Method::GET, "/api/processes", None, None, None).await;
    let signal = send(
        &state,
        Method::POST,
        "/api/processes/signal",
        Some(&json!({"pid": 2, "signal": "term"})),
        None,
        Some(ORIGIN),
    )
    .await;

    assert_eq!(list.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(signal.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_returns_the_process_table() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/processes",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    let processes = body.as_array().expect("array of processes");
    assert!(!processes.is_empty(), "the process table is not empty");
    let own = processes
        .iter()
        .find(|process| process["pid"] == std::process::id())
        .expect("the test process is in the table");
    assert!(own["name"].as_str().is_some_and(|name| !name.is_empty()));
    assert!(own["user"].as_str().is_some());
    assert!(own["niceness"].as_i64().is_some());
}

#[tokio::test]
async fn a_signal_to_pid_zero_is_refused_and_audited() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/processes/signal",
        Some(&json!({"pid": 0, "signal": "term"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "unsafe_target");

    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action, "process.signal");
    assert_eq!(records[0].actor, "owner");
    assert_eq!(records[0].target, "pid:0");
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "unsafe_target");
    assert_eq!(detail["signal"], "TERM");
}

#[tokio::test]
async fn a_signal_terminates_a_test_process_and_is_audited() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let mut child = spawn_sleeper();
    let pid = pid_of(&child);

    let response = send(
        &state,
        Method::POST,
        "/api/processes/signal",
        Some(&json!({"pid": pid, "signal": "term"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        wait_for_exit(&mut child),
        "the sleep child exits on SIGTERM"
    );

    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].target, format!("pid:{pid}"));
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "ok");
}

#[tokio::test]
async fn a_missing_process_is_not_found_and_audited() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/processes/signal",
        Some(&json!({"pid": 2_000_000_000_i64, "signal": "term"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "no_such_process");

    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1);
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "no_such_process");
}

#[tokio::test]
async fn renice_refuses_a_value_out_of_range() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let own_pid = i32::try_from(std::process::id()).expect("pid fits i32");

    let response = send(
        &state,
        Method::POST,
        "/api/processes/renice",
        Some(&json!({"pid": own_pid, "priority": 50})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_nice");

    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action, "process.renice");
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "invalid_nice");
    assert_eq!(detail["priority"], 50);
}

#[tokio::test]
async fn renice_changes_a_test_process_and_is_audited() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let mut child = spawn_sleeper();
    let pid = pid_of(&child);

    let response = send(
        &state,
        Method::POST,
        "/api/processes/renice",
        Some(&json!({"pid": pid, "priority": 19})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();

    let niceness = ProcessService::new()
        .list()
        .expect("process table")
        .into_iter()
        .find(|process| i32::try_from(process.pid).ok() == Some(pid))
        .map(|process| process.niceness);

    child.kill().ok();
    child.wait().ok();

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(niceness, Some(19));

    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action, "process.renice");
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    assert_eq!(detail["outcome"], "ok");
    assert_eq!(detail["priority"], 19);
}

#[tokio::test]
async fn a_mutating_request_without_an_origin_is_refused() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/processes/signal",
        Some(&json!({"pid": 2, "signal": "term"})),
        Some(&cookie),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        state.audit.recent(10).await.expect("audit rows").is_empty(),
        "a refused origin never reaches the handler"
    );
}
