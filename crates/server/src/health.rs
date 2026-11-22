//! The liveness endpoint.

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

/// Current health of the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Always `ok` while the process serves requests.
    pub status: String,
    /// Binary name.
    pub name: String,
    /// Cargo package version.
    pub version: String,
}

/// Reports that the server is alive.
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    responses((status = 200, description = "The server is healthy", body = HealthResponse))
)]
// An axum handler must be async even when it has nothing to await.
#[allow(clippy::unused_async)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_owned(),
        name: "dandelion".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}
