//! Router acceptance tests.

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use dandelion_server::app::build_router;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn send(uri: &str) -> Response {
    build_router()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("the request builder is valid"),
        )
        .await
        .expect("the router answers")
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
