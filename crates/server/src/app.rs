//! Router construction.

use axum::{Json, Router, http::StatusCode, middleware, routing::get};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    agents, auth, containers, health, metrics, openapi, processes, services, state::AppState,
    static_files, terminal, ws,
};

/// Error body returned by API routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ApiError {
    /// Stable machine-readable code.
    pub error: String,
    /// Human-readable detail.
    pub message: String,
}

/// Builds the application router with state applied and request checks on.
///
/// Mutating requests must come from the public origin, and protected routes
/// need a live session cookie. Add a domain router to `protected_routes`, not
/// `public_routes`: only health, `OpenAPI`, and the pre-session auth endpoints
/// are public.
pub fn build_router(state: AppState) -> Router {
    let origin_state = state.clone();
    let auth_state = state.clone();

    let public = public_routes();
    let protected = protected_routes().route_layer(middleware::from_fn_with_state(
        auth_state,
        auth::middleware::require_auth,
    ));
    let api =
        protected
            .merge(public)
            .fallback(api_not_found)
            .layer(middleware::from_fn_with_state(
                origin_state,
                auth::middleware::check_origin,
            ));

    Router::new()
        .nest("/api", api)
        .fallback(static_files::spa)
        .with_state(state)
}

/// Builds the router before state is applied; tests add routes to this.
///
/// This seam deliberately omits the auth and origin layers so a test can add
/// a handler and call it directly. `build_router` is what the binary serves;
/// never expose this one.
pub fn router_for_tests() -> Router<AppState> {
    let api = protected_routes()
        .merge(public_routes())
        .fallback(api_not_found);

    Router::new().nest("/api", api).fallback(static_files::spa)
}

fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/openapi.json", get(openapi::openapi_json))
        .nest("/auth", auth::routes::public())
}

/// Every route that needs a session belongs here.
fn protected_routes() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::routes::protected())
        .nest("/system", metrics::routes::router())
        .nest("/processes", processes::router())
        .nest("/services", services::router())
        .nest("/containers", containers::router())
        .nest("/terminals", terminal::router())
        .nest("/agents", agents::router())
        .route("/ws", get(ws::handler))
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
