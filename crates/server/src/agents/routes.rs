//! Agent routes: snapshot, actions, spawn, screen, and the attach socket.

use axum::{
    Json, Router,
    extract::{Extension, Path as UrlPath, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use dandelion_terminals::{ScreenText, TerminalInfo};
use serde_json::json;

use super::{
    AgentActionRequest, AgentsError, AgentsSnapshot, SpawnAgentRequest, error_code_and_status, ws,
};
use crate::{
    app::ApiError,
    audit::AuditEntry,
    auth::middleware::{RequestUser, error_response, origin_allowed},
    state::AppState,
};

/// The only account this build authenticates.
const ACTOR: &str = "owner";

/// Routes mounted under `/api/agents`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(snapshot))
        .route("/action", post(action))
        .route("/spawn", post(spawn))
        .route("/{terminal}/screen", get(screen))
        .route("/{terminal}/ws", get(upgrade))
}

/// Every live terminal that reports an agent.
#[utoipa::path(
    get,
    path = "/api/agents",
    tag = "agents",
    operation_id = "agents_snapshot",
    responses(
        (status = 200, description = "Agents and the backend descriptor", body = AgentsSnapshot),
        (status = 502, description = "The backend failed", body = ApiError),
        (status = 503, description = "No backend is available", body = ApiError)
    )
)]
pub async fn snapshot(State(state): State<AppState>) -> Response {
    match state.agents.snapshot().await {
        Ok(listing) => {
            // The Claude cache is independent of the terminal backend, so the
            // page still shows sessions while herdr is down.
            Json(AgentsSnapshot::compose(listing, state.claude.snapshot())).into_response()
        }
        Err(error) => agents_error_response(&error),
    }
}

/// Runs one agent action.
#[utoipa::path(
    post,
    path = "/api/agents/action",
    tag = "agents",
    operation_id = "agents_action",
    request_body = AgentActionRequest,
    responses(
        (status = 204, description = "The action ran"),
        (status = 400, description = "A key name is not valid", body = ApiError),
        (status = 404, description = "No such terminal", body = ApiError),
        (status = 409, description = "The agent is blocked or the backend refuses", body = ApiError),
        (status = 502, description = "The backend failed", body = ApiError),
        (status = 503, description = "No backend is available", body = ApiError)
    )
)]
pub async fn action(
    State(state): State<AppState>,
    Json(request): Json<AgentActionRequest>,
) -> Response {
    let result = state.agents.act(&request).await;
    let detail = json!({
        "kind": request.action.name(),
        "outcome": outcome(&result),
    });
    record(
        &state,
        "agent.action",
        &format!("terminal:{}", request.terminal),
        detail,
    )
    .await;

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => agents_error_response(&error),
    }
}

/// Starts an agent in a directory.
#[utoipa::path(
    post,
    path = "/api/agents/spawn",
    tag = "agents",
    operation_id = "agents_spawn",
    request_body = SpawnAgentRequest,
    responses(
        (status = 200, description = "The new terminal", body = TerminalInfo),
        (status = 409, description = "The backend refuses the spawn", body = ApiError),
        (status = 502, description = "The backend failed", body = ApiError),
        (status = 503, description = "No backend is available", body = ApiError)
    )
)]
pub async fn spawn(
    State(state): State<AppState>,
    Json(request): Json<SpawnAgentRequest>,
) -> Response {
    let result = state.agents.spawn(&request).await;
    let detail = json!({
        "line": request.line,
        "cwd": request.cwd,
        "outcome": outcome(&result),
    });
    record(&state, "agent.spawn", "terminal:new", detail).await;

    match result {
        Ok(info) => Json(info).into_response(),
        Err(error) => agents_error_response(&error),
    }
}

/// Reads one agent's visible screen.
///
/// History is never served: herdr-core scrolls a full-screen program to stitch
/// it, and scrolling is input to that program.
#[utoipa::path(
    get,
    path = "/api/agents/{terminal}/screen",
    tag = "agents",
    operation_id = "agents_screen",
    params(
        ("terminal" = String, Path, description = "backend terminal id")
    ),
    responses(
        (status = 200, description = "The screen text", body = ScreenText),
        (status = 404, description = "No such terminal", body = ApiError),
        (status = 502, description = "The backend failed", body = ApiError),
        (status = 503, description = "No backend is available", body = ApiError)
    )
)]
pub async fn screen(State(state): State<AppState>, UrlPath(terminal): UrlPath<String>) -> Response {
    match state.agents.screen(&terminal).await {
        Ok(screen) => Json(screen).into_response(),
        Err(error) => agents_error_response(&error),
    }
}

/// Upgrades a same-origin request to the agent's attach socket.
pub async fn upgrade(
    State(state): State<AppState>,
    Extension(user): Extension<RequestUser>,
    UrlPath(terminal): UrlPath<String>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(state.auth.origin(), &headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "origin_rejected",
            "the WebSocket handshake did not come from the Dandelion origin",
        );
    }
    upgrade
        .max_message_size(ws::MAX_INPUT_BYTES)
        .max_frame_size(ws::MAX_INPUT_BYTES)
        .on_upgrade(move |socket| ws::connection(socket, state, user, terminal))
}

/// Appends one audit row; a failure is logged, never returned to the caller.
async fn record(state: &AppState, action: &str, target: &str, detail: serde_json::Value) {
    let detail = detail.to_string();
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor: ACTOR,
            action,
            target,
            detail: Some(&detail),
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}

fn outcome<T>(result: &Result<T, AgentsError>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err(error) => error_code_and_status(error).0.to_owned(),
    }
}

fn agents_error_response(error: &AgentsError) -> Response {
    let (code, status) = error_code_and_status(error);
    error_response(status, code, &error.to_string())
}
