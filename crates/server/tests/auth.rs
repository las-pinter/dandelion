//! Auth route acceptance tests: setup, login, sessions, origin, throttling.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use dandelion_server::{
    app::build_router,
    audit::AuditLog,
    auth::{Auth, origin::PublicOrigin, password, session::now_millis, token::hash_token},
    state::AppState,
    storage,
    ws::hub::Hub,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const PASSWORD: &str = "correct horse battery";

async fn test_state() -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(AuditLog::new(pool), auth, Hub::default())
}

async fn send(
    state: &AppState,
    method: Method,
    uri: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> Response {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };

    build_router(state.clone())
        .oneshot(builder.body(body).expect("the request builder is valid"))
        .await
        .expect("the router answers")
}

async fn send_with_origin(
    state: &AppState,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> Response {
    send(state, method, uri, body, &[("origin", ORIGIN)]).await
}

async fn json_body(response: Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is readable")
        .to_bytes();

    serde_json::from_slice(&bytes).expect("the body is JSON")
}

fn session_cookie(response: &Response) -> String {
    let header = response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("set-cookie header");
    header.split(';').next().expect("cookie pair").to_owned()
}

async fn issue_setup_token(state: &AppState) -> String {
    state
        .auth
        .issue_setup_token(now_millis())
        .await
        .expect("setup token")
}

#[tokio::test]
async fn unauthenticated_api_call_returns_401() {
    let state = test_state().await;

    let response = send(&state, Method::GET, "/api/auth/me", None, &[]).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "unauthenticated");
}

#[tokio::test]
async fn health_and_openapi_stay_public() {
    let state = test_state().await;

    let health = send(&state, Method::GET, "/api/health", None, &[]).await;
    let openapi = send(&state, Method::GET, "/api/openapi.json", None, &[]).await;

    assert_eq!(health.status(), StatusCode::OK);
    assert_eq!(openapi.status(), StatusCode::OK);
}

#[tokio::test]
async fn mutating_request_without_origin_is_refused() {
    let state = test_state().await;

    let response = send(
        &state,
        Method::POST,
        "/api/auth/login/password",
        Some(json!({ "password": PASSWORD })),
        &[],
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "origin_rejected");
}

#[tokio::test]
async fn mutating_request_from_another_origin_is_refused() {
    let state = test_state().await;

    let response = send(
        &state,
        Method::POST,
        "/api/auth/login/password",
        Some(json!({ "password": PASSWORD })),
        &[("origin", "https://evil.test")],
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn same_origin_fetch_metadata_is_accepted() {
    let state = test_state().await;

    let response = send(
        &state,
        Method::POST,
        "/api/auth/login/password",
        Some(json!({ "password": PASSWORD })),
        &[("sec-fetch-site", "same-origin")],
    )
    .await;

    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn password_setup_then_login_logout() {
    let state = test_state().await;
    let token = issue_setup_token(&state).await;

    let setup = send_with_origin(
        &state,
        Method::POST,
        "/api/auth/setup/password",
        Some(json!({ "token": &token, "password": PASSWORD })),
    )
    .await;
    assert_eq!(setup.status(), StatusCode::OK);
    assert_eq!(json_body(setup).await["has_password"], true);

    let login = send_with_origin(
        &state,
        Method::POST,
        "/api/auth/login/password",
        Some(json!({ "password": PASSWORD })),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie(&login);

    let me = send(
        &state,
        Method::GET,
        "/api/auth/me",
        None,
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(me.status(), StatusCode::OK);
    assert_eq!(json_body(me).await["username"], "owner");

    let logout = send(
        &state,
        Method::POST,
        "/api/auth/logout",
        None,
        &[("cookie", &cookie), ("origin", ORIGIN)],
    )
    .await;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);

    let after = send(
        &state,
        Method::GET,
        "/api/auth/me",
        None,
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn setup_cannot_complete_without_a_passkey() {
    let state = test_state().await;
    let token = issue_setup_token(&state).await;

    send_with_origin(
        &state,
        Method::POST,
        "/api/auth/setup/password",
        Some(json!({ "token": &token, "password": PASSWORD })),
    )
    .await;

    let response = send_with_origin(
        &state,
        Method::POST,
        "/api/auth/setup/complete",
        Some(json!({ "token": &token })),
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "setup_incomplete");
    assert!(
        state
            .auth
            .setup_token_valid(&token, now_millis())
            .await
            .expect("token check")
    );
}

#[tokio::test]
async fn an_unknown_setup_token_is_refused() {
    let state = test_state().await;

    let response = send_with_origin(
        &state,
        Method::POST,
        "/api/auth/setup/password",
        Some(json!({ "token": "not-a-token", "password": PASSWORD })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["error"], "invalid_setup_token");
}

#[tokio::test]
async fn an_expired_setup_token_reports_invalid() {
    let state = test_state().await;
    let now = now_millis();
    let token = "expired-token";
    state
        .auth
        .store()
        .insert_setup_token(&hash_token(token), now - 1000, now - 1)
        .await
        .expect("insert token");

    let response = send(
        &state,
        Method::GET,
        &format!("/api/auth/setup/status?token={token}"),
        None,
        &[],
    )
    .await;
    let body = json_body(response).await;

    assert_eq!(body["valid"], false);
}

#[tokio::test]
async fn wrong_passwords_are_throttled_after_five() {
    let state = test_state().await;
    set_password(&state).await;

    for _ in 0..5 {
        let response = send_with_origin(
            &state,
            Method::POST,
            "/api/auth/login/password",
            Some(json!({ "password": "wrong password here" })),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let response = send_with_origin(
        &state,
        Method::POST,
        "/api/auth/login/password",
        Some(json!({ "password": PASSWORD })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key(header::RETRY_AFTER));
}

#[tokio::test]
async fn an_expired_session_is_rejected() {
    let state = test_state().await;
    let now = now_millis();
    let owner = state.auth.store().ensure_owner(now).await.expect("owner");
    state
        .auth
        .store()
        .insert_session(&hash_token("expired"), &owner.id, now - 1000, now - 1)
        .await
        .expect("session");

    let response = send(
        &state,
        Method::GET,
        "/api/auth/me",
        None,
        &[("cookie", "__Host-dandelion_session=expired")],
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_all_revokes_every_session() {
    let state = test_state().await;
    set_password(&state).await;
    let first = login(&state).await;
    let second = login(&state).await;

    let response = send(
        &state,
        Method::POST,
        "/api/auth/logout-all",
        None,
        &[("cookie", &first), ("origin", ORIGIN)],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    for cookie in [&first, &second] {
        let me = send(
            &state,
            Method::GET,
            "/api/auth/me",
            None,
            &[("cookie", cookie)],
        )
        .await;
        assert_eq!(me.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn status_reports_setup_and_authentication() {
    let state = test_state().await;

    let fresh = json_body(send(&state, Method::GET, "/api/auth/status", None, &[]).await).await;
    assert_eq!(fresh["setup_required"], true);
    assert_eq!(fresh["authenticated"], false);

    set_password(&state).await;
    let cookie = login(&state).await;
    let signed_in = json_body(
        send(
            &state,
            Method::GET,
            "/api/auth/status",
            None,
            &[("cookie", &cookie)],
        )
        .await,
    )
    .await;
    assert_eq!(signed_in["authenticated"], true);
}

#[tokio::test]
async fn login_and_failures_are_audited() {
    let state = test_state().await;
    set_password(&state).await;

    send_with_origin(
        &state,
        Method::POST,
        "/api/auth/login/password",
        Some(json!({ "password": "wrong password here" })),
    )
    .await;
    login(&state).await;

    let records = state.audit.recent(10).await.expect("audit log");
    assert!(records.iter().any(|row| row.action == "auth.login.failure"));
    assert!(records.iter().any(|row| row.action == "auth.login"));
}

async fn set_password(state: &AppState) {
    let now = now_millis();
    let owner = state.auth.store().ensure_owner(now).await.expect("owner");
    let hash = password::hash(PASSWORD).expect("password hash");
    state
        .auth
        .store()
        .set_password(&owner.id, &hash)
        .await
        .expect("set password");
}

async fn login(state: &AppState) -> String {
    let response = send_with_origin(
        state,
        Method::POST,
        "/api/auth/login/password",
        Some(json!({ "password": PASSWORD })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    session_cookie(&response)
}

#[tokio::test]
async fn completing_setup_sets_a_session_cookie() {
    let state = test_state().await;
    set_password(&state).await;
    let now = now_millis();
    let owner = state
        .auth
        .store()
        .owner()
        .await
        .expect("read")
        .expect("owner");
    state
        .auth
        .store()
        .insert_passkey(&owner.id, b"credential", "{}", now)
        .await
        .expect("passkey");
    let token = issue_setup_token(&state).await;

    let response = send_with_origin(
        &state,
        Method::POST,
        "/api/auth/setup/complete",
        Some(json!({ "token": &token })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let cookie = session_cookie(&response);
    assert!(cookie.starts_with("__Host-dandelion_session="));
    let me = send(
        &state,
        Method::GET,
        "/api/auth/me",
        None,
        &[("cookie", &cookie)],
    )
    .await;
    assert_eq!(me.status(), StatusCode::OK);
    assert!(
        !state
            .auth
            .setup_token_valid(&token, now_millis())
            .await
            .expect("token check")
    );
}

#[tokio::test]
async fn the_https_origin_ignores_the_plain_cookie_name() {
    let state = test_state().await;
    set_password(&state).await;
    let cookie = login(&state).await;
    let plain = cookie.replacen("__Host-dandelion_session", "dandelion_session", 1);

    let forged = send(
        &state,
        Method::GET,
        "/api/auth/me",
        None,
        &[("cookie", &plain)],
    )
    .await;
    let real = send(
        &state,
        Method::GET,
        "/api/auth/me",
        None,
        &[("cookie", &cookie)],
    )
    .await;

    assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(real.status(), StatusCode::OK);
}
