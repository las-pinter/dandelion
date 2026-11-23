//! Process listing and control endpoints.
//!
//! Listing reads `/proc` on a blocking thread. Signals and nice values go
//! through the process crate, which refuses pid 0 and pid 1 before the
//! syscall. Every mutating request writes exactly one audit row, including a
//! refused request, so an attempt is visible even when nothing changed.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use dandelion_system::process::{
    ProcessError, ProcessInfo, ProcessService, ReniceRequest, SignalRequest,
};
use serde_json::json;

use crate::{app::ApiError, audit::AuditEntry, auth::middleware::error_response, state::AppState};

/// The only account this build authenticates.
const ACTOR: &str = "owner";

/// Routes mounted under `/api/processes`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/signal", post(signal))
        .route("/renice", post(renice))
}

/// Lists every readable process, sorted by pid.
#[utoipa::path(
    get,
    path = "/api/processes",
    tag = "processes",
    responses(
        (status = 200, description = "Every readable process", body = Vec<ProcessInfo>),
        (status = 500, description = "The process table could not be read", body = ApiError)
    )
)]
pub async fn list() -> Response {
    match tokio::task::spawn_blocking(|| ProcessService::new().list()).await {
        Ok(Ok(processes)) => Json(processes).into_response(),
        Ok(Err(error)) => {
            tracing::error!(%error, "cannot list processes");
            internal_error()
        }
        Err(error) => {
            tracing::error!(%error, "process listing task failed");
            internal_error()
        }
    }
}

/// Sends one supported signal to a process.
#[utoipa::path(
    post,
    path = "/api/processes/signal",
    tag = "processes",
    request_body = SignalRequest,
    responses(
        (status = 204, description = "Signal sent"),
        (status = 400, description = "The pid or signal is not allowed", body = ApiError),
        (status = 403, description = "The process belongs to another user", body = ApiError),
        (status = 404, description = "The process does not exist", body = ApiError),
        (status = 500, description = "The signal failed", body = ApiError)
    )
)]
pub async fn signal(State(state): State<AppState>, Json(request): Json<SignalRequest>) -> Response {
    let result = ProcessService::new().signal(&request);
    let detail = json!({
        "signal": request.signal.name(),
        "outcome": outcome(&result),
    });
    record(&state, "process.signal", request.pid, detail).await;

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => process_error_response(&error),
    }
}

/// Changes a process's nice value.
#[utoipa::path(
    post,
    path = "/api/processes/renice",
    tag = "processes",
    request_body = ReniceRequest,
    responses(
        (status = 204, description = "Nice value changed"),
        (status = 400, description = "The pid or nice value is not allowed", body = ApiError),
        (status = 403, description = "The process belongs to another user", body = ApiError),
        (status = 404, description = "The process does not exist", body = ApiError),
        (status = 500, description = "The renice failed", body = ApiError)
    )
)]
pub async fn renice(State(state): State<AppState>, Json(request): Json<ReniceRequest>) -> Response {
    let result = ProcessService::new().renice(&request);
    let detail = json!({
        "priority": request.priority,
        "outcome": outcome(&result),
    });
    record(&state, "process.renice", request.pid, detail).await;

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => process_error_response(&error),
    }
}

/// Appends one audit row; a failure is logged, never returned to the caller.
async fn record(state: &AppState, action: &str, pid: i32, detail: serde_json::Value) {
    let target = format!("pid:{pid}");
    let detail = detail.to_string();
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor: ACTOR,
            action,
            target: &target,
            detail: Some(&detail),
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}

/// The stable outcome name stored in the audit detail.
fn outcome(result: &Result<(), ProcessError>) -> &'static str {
    match result {
        Ok(()) => "ok",
        Err(error) => error_code(error),
    }
}

/// The stable machine-readable code for a process error.
fn error_code(error: &ProcessError) -> &'static str {
    match error {
        ProcessError::UnsafeTarget { .. } => "unsafe_target",
        ProcessError::InvalidNice { .. } => "invalid_nice",
        ProcessError::NoSuchProcess { .. } => "no_such_process",
        ProcessError::PermissionDenied { .. } => "permission_denied",
        ProcessError::Io { .. }
        | ProcessError::Malformed { .. }
        | ProcessError::SignalFailed { .. }
        | ProcessError::ReniceFailed { .. } => "internal",
    }
}

/// Maps a process error to a status, a stable code, and the crate's message.
fn process_error_response(error: &ProcessError) -> Response {
    let code = error_code(error);
    match error {
        ProcessError::UnsafeTarget { .. } | ProcessError::InvalidNice { .. } => {
            error_response(StatusCode::BAD_REQUEST, code, &error.to_string())
        }
        ProcessError::NoSuchProcess { .. } => {
            error_response(StatusCode::NOT_FOUND, code, &error.to_string())
        }
        ProcessError::PermissionDenied { .. } => {
            error_response(StatusCode::FORBIDDEN, code, &error.to_string())
        }
        ProcessError::Io { .. }
        | ProcessError::Malformed { .. }
        | ProcessError::SignalFailed { .. }
        | ProcessError::ReniceFailed { .. } => {
            tracing::error!(%error, "process action failed");
            internal_error()
        }
    }
}

fn internal_error() -> Response {
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "an internal error occurred",
    )
}
