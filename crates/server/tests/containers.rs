//! Container endpoint acceptance tests.
//!
//! These use a disabled `Containers`, so they cover authentication, reference
//! validation, the unavailable path, and the audit row without touching the
//! Docker daemon. The live path is an ignored test.

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
    containers::{ContainerAction, ContainerActionRequest, Containers},
    metrics::Metrics,
    services::Services,
    state::AppState,
    storage,
    ws::hub::Hub,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";

async fn state_with(containers: Containers) -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        Hub::default(),
        Metrics::new(pool),
        Services::disabled(),
        containers,
        Agents::disabled(),
        ClaudeMonitor::disabled(),
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

async fn one_audit_row(state: &AppState) -> (String, String, Value) {
    let records = state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1, "exactly one row per mutating request");
    let detail =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail JSON");
    (records[0].action.clone(), records[0].target.clone(), detail)
}

#[tokio::test]
async fn containers_require_a_session() {
    let state = state_with(Containers::disabled()).await;

    let response = send(&state, Method::GET, "/api/containers", None, None, None).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_snapshot_reports_unavailable_docker() {
    let state = state_with(Containers::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/containers",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["availability"]["available"], false);
    assert!(
        body["availability"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("Docker is unavailable")),
        "a reason is reported: {body}"
    );
    assert_eq!(body["containers"], json!([]));
}

#[tokio::test]
async fn an_invalid_reference_is_refused_and_audited() {
    let state = state_with(Containers::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/containers/action",
        Some(&json!({"container": "-probe", "action": "start"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_container");
    let (action, target, detail) = one_audit_row(&state).await;
    assert_eq!(action, "container.action");
    assert_eq!(target, "container:-probe");
    assert_eq!(detail["action"], "start");
    assert_eq!(detail["outcome"], "invalid_container");
}

#[tokio::test]
async fn an_action_without_docker_is_unavailable_and_audited() {
    let state = state_with(Containers::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/containers/action",
        Some(&json!({"container": "probe", "action": "restart", "timeout_secs": 5})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "unavailable");
    let (_, target, detail) = one_audit_row(&state).await;
    assert_eq!(target, "container:probe");
    assert_eq!(detail["action"], "restart");
    assert_eq!(detail["outcome"], "unavailable");
}

#[tokio::test]
async fn logs_validate_the_reference() {
    let state = state_with(Containers::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/containers/-probe/logs?tail=50",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_container");
    assert!(
        state.audit.recent(10).await.expect("audit rows").is_empty(),
        "a read is never audited"
    );
}

#[tokio::test]
async fn logs_report_unavailable_docker() {
    let state = state_with(Containers::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/containers/probe/logs",
        None,
        Some(&cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "unavailable");
}

#[tokio::test]
async fn a_mutating_request_without_an_origin_is_refused() {
    let state = state_with(Containers::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/containers/action",
        Some(&json!({"container": "probe", "action": "stop"})),
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

/// Lists the live daemon's containers and reads the first one's logs.
#[tokio::test]
#[ignore = "needs a live Docker daemon"]
async fn the_live_daemon_lists_containers_and_logs() {
    let containers = Containers::connect();
    let snapshot = containers.snapshot().await.expect("live snapshot");
    assert!(snapshot.availability.available, "the daemon answers");

    if let Some(first) = snapshot.containers.first() {
        containers.logs(&first.name, 5).await.expect("log tail");
    }
}

/// Stops, starts, restarts, and reads the logs of one probe container.
///
/// Create it first, then pass its name:
///
/// ```text
/// docker run -d --name dandelion-probe alpine:3.21 \
///   sh -c 'echo probe-started; sleep 300'
/// DANDELION_LIVE_CONTAINER=dandelion-probe \
///   cargo test -p dandelion-server --test containers -- --ignored
/// ```
#[tokio::test]
#[ignore = "needs a live Docker daemon and DANDELION_LIVE_CONTAINER"]
async fn the_live_daemon_cycles_a_probe_container() {
    let container =
        std::env::var("DANDELION_LIVE_CONTAINER").expect("set DANDELION_LIVE_CONTAINER");
    let containers = Containers::connect();
    let request = |action| ContainerActionRequest {
        container: container.clone(),
        action,
        timeout_secs: Some(5),
    };

    containers
        .act(&request(ContainerAction::Stop))
        .await
        .expect("the probe container stops");
    containers
        .act(&request(ContainerAction::Start))
        .await
        .expect("the probe container starts");
    containers
        .act(&request(ContainerAction::Restart))
        .await
        .expect("the probe container restarts");

    let lines = containers
        .logs(&container, 50)
        .await
        .expect("the probe logs are readable");
    assert!(
        lines
            .iter()
            .any(|line| line.message.contains("probe-started")),
        "the logs hold the probe line: {lines:?}"
    );
}
