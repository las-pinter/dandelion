//! Router construction.

use axum::{Json, Router, http::StatusCode, routing::get};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{health, openapi, static_files};

/// Error body returned by API routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ApiError {
    /// Stable machine-readable code.
    pub error: String,
    /// Human-readable detail.
    pub message: String,
}

/// Builds the application router.
///
/// API routes live under `/api` and answer unknown paths with JSON. Every
/// other path falls through to the embedded web application.
pub fn build_router() -> Router {
    let api = Router::new()
        .route("/health", get(health::health))
        .route("/openapi.json", get(openapi::openapi_json))
        .fallback(api_not_found);

    Router::new().nest("/api", api).fallback(static_files::spa)
}

// An axum handler must be async even when it has nothing to await.
#[allow(clippy::unused_async)]
async fn api_not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "not_found".to_owned(),
            message: "no such API route".to_owned(),
        }),
    )
}
