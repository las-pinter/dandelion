//! The `OpenAPI` document printed by `dandelion openapi`.

use axum::Json;
use utoipa::OpenApi;

/// Every HTTP endpoint Dandelion exposes.
#[derive(Debug, OpenApi)]
#[openapi(
    info(title = "Dandelion API", version = env!("CARGO_PKG_VERSION")),
    paths(crate::health::health),
    components(schemas(crate::health::HealthResponse, crate::app::ApiError))
)]
pub struct ApiDoc;

/// Serves the generated `OpenAPI` document.
// An axum handler must be async even when it has nothing to await.
#[allow(clippy::unused_async)]
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use utoipa::OpenApi;

    use super::ApiDoc;

    #[test]
    fn document_lists_the_health_route() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        assert!(document["paths"]["/api/health"].is_object());
    }
}
