//! Service endpoint acceptance tests.
//!
//! These use a disabled `Services`, so they cover authentication, validation,
//! the allowlist refusal, and the audit row without touching D-Bus or sudo. The
//! live path is an ignored test.

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
    notifications::Notifications,
    services::Services,
    state::AppState,
    storage,
    ws::hub::Hub,
};
use dandelion_system::systemd::{
    ActionMechanism, SudoAllowlist, UnitAction, UnitActionRequest, UnitScope,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";

fn allowlist() -> SudoAllowlist {
    let mut list = SudoAllowlist::new();
    list.allow("nginx.service", vec![UnitAction::Restart])
        .expect("test rule");
    list
}

async fn state_with(services: Services) -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        Hub::default(),
        Metrics::new(pool),
        services,
        Containers::disabled(),
        Agents::disabled(),
        ClaudeMonitor::disabled(),
        Notifications::disabled(),
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
async fn services_require_a_session() {
    let state = state_with(Services::disabled()).await;

    let response = send(&state, Method::GET, "/api/services", None, None, None).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn snapshot_is_unavailable_without_systemd() {
    let state = state_with(Services::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/services",
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
async fn an_invalid_unit_is_refused_and_audited() {
    let state = state_with(Services::with_allowlist(allowlist())).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/services/action",
        Some(&json!({"scope": "system", "unit": "-nginx.service", "action": "restart"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_unit");
    let (action, target, detail) = one_audit_row(&state).await;
    assert_eq!(action, "service.action");
    assert_eq!(target, "system:-nginx.service");
    assert_eq!(detail["outcome"], "invalid_unit");
}

#[tokio::test]
async fn an_action_outside_the_allowlist_is_refused_and_audited() {
    let state = state_with(Services::with_allowlist(allowlist())).await;
    let cookie = session_cookie(&state).await;

    let wrong_verb = send(
        &state,
        Method::POST,
        "/api/services/action",
        Some(&json!({"scope": "system", "unit": "nginx.service", "action": "stop"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    assert_eq!(wrong_verb.status(), StatusCode::FORBIDDEN);
    let body = json_body(wrong_verb).await;
    assert_eq!(body["error"], "not_allowed");
    let (_, target, detail) = one_audit_row(&state).await;
    assert_eq!(target, "system:nginx.service");
    assert_eq!(detail["outcome"], "not_allowed");

    let unlisted = state_with(Services::with_allowlist(allowlist())).await;
    let cookie = session_cookie(&unlisted).await;
    let response = send(
        &unlisted,
        Method::POST,
        "/api/services/action",
        Some(&json!({"scope": "system", "unit": "other.service", "action": "restart"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "not_allowed");
}

#[tokio::test]
async fn an_allowlisted_action_reports_the_missing_systemd() {
    let state = state_with(Services::with_allowlist(allowlist())).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/services/action",
        Some(&json!({"scope": "system", "unit": "nginx.service", "action": "restart"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "unavailable");
    let (_, target, detail) = one_audit_row(&state).await;
    assert_eq!(target, "system:nginx.service");
    assert_eq!(detail["action"], "restart");
    assert_eq!(detail["outcome"], "unavailable");
}

#[tokio::test]
async fn a_user_action_is_audited_and_unavailable_without_systemd() {
    let state = state_with(Services::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/services/action",
        Some(&json!({"scope": "user", "unit": "app.service", "action": "start"})),
        Some(&cookie),
        Some(ORIGIN),
    )
    .await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (_, target, detail) = one_audit_row(&state).await;
    assert_eq!(target, "user:app.service");
    assert_eq!(detail["outcome"], "unavailable");
}

#[tokio::test]
async fn journal_validates_scope_and_unit() {
    let state = state_with(Services::disabled()).await;
    let cookie = session_cookie(&state).await;

    let bad_scope = send(
        &state,
        Method::GET,
        "/api/services/admin/nginx.service/journal",
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(bad_scope.status(), StatusCode::BAD_REQUEST);
    let body = json_body(bad_scope).await;
    assert_eq!(body["error"], "invalid_scope");

    let bad_unit = send(
        &state,
        Method::GET,
        "/api/services/user/-nginx.service/journal",
        None,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(bad_unit.status(), StatusCode::BAD_REQUEST);
    let body = json_body(bad_unit).await;
    assert_eq!(body["error"], "invalid_unit");
}

#[tokio::test]
async fn journal_is_unavailable_without_systemd() {
    let state = state_with(Services::disabled()).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::GET,
        "/api/services/user/nginx.service/journal?lines=50",
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
    let state = state_with(Services::with_allowlist(allowlist())).await;
    let cookie = session_cookie(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/services/action",
        Some(&json!({"scope": "system", "unit": "nginx.service", "action": "restart"})),
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

/// Lists the live managers; needs a session bus and a system bus.
#[tokio::test]
#[ignore = "needs a live D-Bus session and system manager"]
async fn the_live_managers_report_units() {
    let services = Services::connect(SudoAllowlist::new()).await;
    let snapshot = services.snapshot().await.expect("live snapshot");

    assert!(!snapshot.units.is_empty(), "the managers report units");

    let first = snapshot.units.first().expect("a unit");
    services
        .journal_tail(first.scope, &first.name, 5)
        .await
        .expect("the journal tail runs");
}

/// Restarts one real system unit through the installed sudoers drop-in.
///
/// Set `DANDELION_LIVE_SUDO_UNIT` to a unit listed in the allowlist file, run
/// `just install-sudoers`, then run this test with `--ignored`.
#[tokio::test]
#[ignore = "needs the installed sudoers drop-in and DANDELION_LIVE_SUDO_UNIT"]
async fn an_allowlisted_system_unit_restarts_through_the_installed_drop_in() {
    let unit = std::env::var("DANDELION_LIVE_SUDO_UNIT")
        .expect("set DANDELION_LIVE_SUDO_UNIT to an allowlisted unit");
    let mut allowlist = SudoAllowlist::new();
    allowlist
        .allow(unit.clone(), vec![UnitAction::Restart])
        .expect("valid unit name");
    let services = Services::connect(allowlist).await;

    let result = services
        .act(&UnitActionRequest {
            scope: UnitScope::System,
            unit: unit.clone(),
            action: UnitAction::Restart,
        })
        .await
        .expect("the installed drop-in permits the restart");

    assert_eq!(result.mechanism, ActionMechanism::Sudo);
    assert_eq!(result.unit, unit);
}
