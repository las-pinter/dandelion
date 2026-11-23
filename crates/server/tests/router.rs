//! Router acceptance tests.

use axum::{
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode, header},
    response::Response,
    routing::post,
};
use dandelion_server::{
    app::{build_router, router_for_tests},
    audit::{AuditEntry, AuditLog},
    auth::{Auth, origin::PublicOrigin},
    metrics::Metrics,
    services::Services,
    state::AppState,
    storage,
    ws::hub::Hub,
};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn test_state() -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse("https://example.test").expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        Hub::default(),
        Metrics::new(pool),
        Services::disabled(),
    )
}

async fn send(uri: &str) -> Response {
    build_router(test_state().await)
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("the request builder is valid"),
        )
        .await
        .expect("the router answers")
}

async fn test_mutation(State(state): State<AppState>) -> StatusCode {
    state
        .audit
        .record(AuditEntry {
            actor: "test",
            action: "test.mutate",
            target: "thing",
            detail: None,
        })
        .await
        .expect("audit write");
    StatusCode::NO_CONTENT
}

async fn json_body(response: Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body is readable")
        .to_bytes();

    serde_json::from_slice(&bytes).expect("the body is JSON")
}

#[tokio::test]
async fn health_returns_ok_with_build_info() {
    let response = send("/api/health").await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["name"], "dandelion");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn unknown_api_route_returns_json_not_found() {
    let response = send("/api/no-such-route").await;
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(content_type.starts_with("application/json"));
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn openapi_document_exposes_health() {
    let response = send("/api/openapi.json").await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body["paths"]["/api/health"].is_object());
}

#[tokio::test]
async fn mutating_handler_writes_one_audit_row() {
    let state = test_state().await;
    let audit = state.audit.clone();
    let app = router_for_tests()
        .route("/api/test-mutation", post(test_mutation))
        .with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/test-mutation")
                .body(Body::empty())
                .expect("the request builder is valid"),
        )
        .await
        .expect("the router answers");
    let records = audit.recent(10).await.expect("read the audit log");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].actor, "test");
    assert_eq!(records[0].action, "test.mutate");
    assert_eq!(records[0].target, "thing");
}
