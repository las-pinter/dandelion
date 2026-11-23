//! Docker container endpoints.
//!
//! Docker is optional on this host: a missing socket is a normal state, so a
//! snapshot reports availability instead of failing and the page still
//! renders. A mutating request while the daemon is unreachable answers `503`
//! and is audited. Every mutating request writes exactly one audit row,
//! including a refused request.

use axum::{
    Json, Router,
    extract::{Path as UrlPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use dandelion_system::docker::{
    ContainerInfo, ContainerLogLine, ContainerLogsRequest, Docker, DockerAvailability, DockerError,
    RestartContainerRequest, StartContainerRequest, StopContainerRequest, validate_container,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::{app::ApiError, audit::AuditEntry, auth::middleware::error_response, state::AppState};

/// The only account this build authenticates.
const ACTOR: &str = "owner";
/// Log lines returned when the request does not ask for a count.
pub const DEFAULT_LOG_LINES: usize = 200;
/// Most log lines one request may ask for.
pub const MAX_LOG_LINES: usize = 1000;

/// One container action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContainerAction {
    /// Starts a stopped container.
    Start,
    /// Stops a running container.
    Stop,
    /// Restarts a container.
    Restart,
}

impl ContainerAction {
    /// The stable name stored in the audit detail.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

/// A request to start, stop, or restart one container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContainerActionRequest {
    /// Container name or id, as listed by `GET /api/containers`.
    pub container: String,
    /// What to do with the container.
    pub action: ContainerAction,
    /// Seconds to wait before killing the container; stop and restart only.
    pub timeout_secs: Option<u32>,
}

/// What the containers page needs in one response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ContainersSnapshot {
    /// Whether the daemon answered, and why not when it did not.
    pub availability: DockerAvailability,
    /// Every container the daemon lists, sorted by name.
    pub containers: Vec<ContainerInfo>,
}

/// Why a container request failed.
#[derive(Debug, thiserror::Error)]
pub enum ContainersError {
    /// This process cannot reach Docker, so actions are disabled.
    #[error("Docker is unavailable to the dandelion server")]
    Unavailable,
    /// The daemon refused, or a reference or timeout is not valid.
    #[error(transparent)]
    Docker(#[from] DockerError),
}

/// Docker access that stays disabled when the socket is missing.
#[derive(Debug, Clone)]
pub struct Containers {
    docker: Option<Docker>,
}

impl Containers {
    /// Connects to the local Docker socket, keeping the server up when absent.
    #[must_use]
    pub fn connect() -> Self {
        match Docker::connect_local() {
            Ok(docker) => Self {
                docker: Some(docker),
            },
            Err(error) => {
                tracing::warn!(%error, "Docker is unavailable; container actions are disabled");
                Self { docker: None }
            }
        }
    }

    /// A disabled client; tests use this to avoid the daemon.
    #[must_use]
    pub const fn disabled() -> Self {
        Self { docker: None }
    }

    /// Lists every container, with the daemon's availability.
    ///
    /// # Errors
    ///
    /// Returns [`ContainersError::Docker`] when the daemon is reachable but
    /// the listing fails. An unreachable daemon is not an error here: the
    /// snapshot reports it through `availability` so the page still renders.
    pub async fn snapshot(&self) -> Result<ContainersSnapshot, ContainersError> {
        let Some(docker) = self.docker.as_ref() else {
            return Ok(ContainersSnapshot {
                availability: DockerAvailability {
                    available: false,
                    reason: Some("Docker is unavailable to the dandelion server".to_owned()),
                    server_version: None,
                },
                containers: Vec::new(),
            });
        };
        let availability = docker.availability().await;
        if !availability.available {
            return Ok(ContainersSnapshot {
                availability,
                containers: Vec::new(),
            });
        }
        let mut containers = docker.list_containers().await?;
        containers.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(ContainersSnapshot {
            availability,
            containers,
        })
    }

    /// Runs one action against one container.
    ///
    /// The reference is validated before the client check, so a bad reference
    /// is reported even when Docker is unreachable.
    ///
    /// # Errors
    ///
    /// Returns [`ContainersError::Unavailable`] without a client, and the
    /// crate's error for an invalid reference, timeout, or daemon failure.
    pub async fn act(&self, request: &ContainerActionRequest) -> Result<(), ContainersError> {
        validate_container(&request.container)?;
        let docker = self.docker.as_ref().ok_or(ContainersError::Unavailable)?;
        match request.action {
            ContainerAction::Start => {
                docker
                    .start(&StartContainerRequest {
                        container: request.container.clone(),
                    })
                    .await?;
            }
            ContainerAction::Stop => {
                docker
                    .stop(&StopContainerRequest {
                        container: request.container.clone(),
                        timeout_secs: request.timeout_secs,
                    })
                    .await?;
            }
            ContainerAction::Restart => {
                docker
                    .restart(&RestartContainerRequest {
                        container: request.container.clone(),
                        timeout_secs: request.timeout_secs,
                    })
                    .await?;
            }
        }
        Ok(())
    }

    /// Reads one container's newest log lines.
    ///
    /// # Errors
    ///
    /// Returns [`ContainersError::Unavailable`] without a client, and the
    /// crate's error for an invalid reference or a failed stream.
    pub async fn logs(
        &self,
        container: &str,
        tail: usize,
    ) -> Result<Vec<ContainerLogLine>, ContainersError> {
        validate_container(container)?;
        let docker = self.docker.as_ref().ok_or(ContainersError::Unavailable)?;
        let request = ContainerLogsRequest {
            container: container.to_owned(),
            tail: Some(u32::try_from(tail).unwrap_or(u32::MAX)),
            follow: false,
            timestamps: true,
        };
        let mut stream = docker.logs(&request)?;
        let mut lines = Vec::new();
        while let Some(item) = stream.next().await {
            lines.push(item?);
        }
        Ok(lines)
    }
}

/// Routes mounted under `/api/containers`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(snapshot))
        .route("/action", post(action))
        .route("/{container}/logs", get(logs))
}

/// Query for the log endpoint.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LogsQuery {
    /// Number of newest lines to return (default 200, maximum 1000).
    pub tail: Option<usize>,
}

/// Every container the daemon lists, with its availability.
#[utoipa::path(
    get,
    path = "/api/containers",
    tag = "containers",
    operation_id = "containers_snapshot",
    responses(
        (status = 200, description = "Containers and daemon availability", body = ContainersSnapshot),
        (status = 502, description = "Docker answered with an error", body = ApiError),
        (status = 503, description = "Docker became unavailable", body = ApiError)
    )
)]
pub async fn snapshot(State(state): State<AppState>) -> Response {
    match state.containers.snapshot().await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => containers_error_response(&error),
    }
}

/// Starts, stops, or restarts one container.
#[utoipa::path(
    post,
    path = "/api/containers/action",
    tag = "containers",
    operation_id = "containers_action",
    request_body = ContainerActionRequest,
    responses(
        (status = 204, description = "The action ran"),
        (status = 400, description = "The container reference or timeout is not valid", body = ApiError),
        (status = 502, description = "Docker answered with an error", body = ApiError),
        (status = 503, description = "Docker is unavailable", body = ApiError)
    )
)]
pub async fn action(
    State(state): State<AppState>,
    Json(request): Json<ContainerActionRequest>,
) -> Response {
    let result = state.containers.act(&request).await;
    let detail = json!({
        "action": request.action.name(),
        "outcome": outcome(&result),
    });
    record(&state, &request.container, detail).await;

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => containers_error_response(&error),
    }
}

/// Reads one container's newest log lines.
#[utoipa::path(
    get,
    path = "/api/containers/{container}/logs",
    tag = "containers",
    operation_id = "containers_logs",
    params(
        ("container" = String, Path, description = "container name or id"),
        LogsQuery
    ),
    responses(
        (status = 200, description = "Log lines, oldest first", body = Vec<ContainerLogLine>),
        (status = 400, description = "The container reference is not valid", body = ApiError),
        (status = 502, description = "Docker answered with an error", body = ApiError),
        (status = 503, description = "Docker is unavailable", body = ApiError)
    )
)]
pub async fn logs(
    State(state): State<AppState>,
    UrlPath(container): UrlPath<String>,
    Query(query): Query<LogsQuery>,
) -> Response {
    let tail = query
        .tail
        .unwrap_or(DEFAULT_LOG_LINES)
        .clamp(1, MAX_LOG_LINES);

    match state.containers.logs(&container, tail).await {
        Ok(lines) => Json(lines).into_response(),
        Err(error) => containers_error_response(&error),
    }
}

/// Appends one audit row; a failure is logged, never returned to the caller.
async fn record(state: &AppState, container: &str, detail: serde_json::Value) {
    let target = format!("container:{container}");
    let detail = detail.to_string();
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor: ACTOR,
            action: "container.action",
            target: &target,
            detail: Some(&detail),
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}

fn outcome(result: &Result<(), ContainersError>) -> &'static str {
    match result {
        Ok(()) => "ok",
        Err(error) => error_code(error),
    }
}

/// The stable machine-readable code for one container error.
fn error_code(error: &ContainersError) -> &'static str {
    match error {
        ContainersError::Unavailable | ContainersError::Docker(DockerError::Unavailable { .. }) => {
            "unavailable"
        }
        ContainersError::Docker(DockerError::InvalidContainer { .. }) => "invalid_container",
        ContainersError::Docker(DockerError::InvalidTimeout { .. }) => "invalid_timeout",
        ContainersError::Docker(DockerError::Api(_)) => "docker_error",
    }
}

/// Maps a container error to a status, a stable code, and a message.
fn containers_error_response(error: &ContainersError) -> Response {
    let code = error_code(error);
    match error {
        ContainersError::Unavailable | ContainersError::Docker(DockerError::Unavailable { .. }) => {
            error_response(StatusCode::SERVICE_UNAVAILABLE, code, &error.to_string())
        }
        ContainersError::Docker(
            DockerError::InvalidContainer { .. } | DockerError::InvalidTimeout { .. },
        ) => error_response(StatusCode::BAD_REQUEST, code, &error.to_string()),
        ContainersError::Docker(DockerError::Api(_)) => {
            error_response(StatusCode::BAD_GATEWAY, code, &error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContainerAction, ContainersError, error_code};
    use dandelion_system::docker::DockerError;

    #[test]
    fn actions_have_stable_audit_names() {
        assert_eq!(ContainerAction::Start.name(), "start");
        assert_eq!(ContainerAction::Stop.name(), "stop");
        assert_eq!(ContainerAction::Restart.name(), "restart");
    }

    #[test]
    fn error_codes_match_the_api_document() {
        assert_eq!(error_code(&ContainersError::Unavailable), "unavailable");
        assert_eq!(
            error_code(&ContainersError::Docker(DockerError::InvalidContainer {
                name: "-probe".to_owned(),
            })),
            "invalid_container"
        );
        assert_eq!(
            error_code(&ContainersError::Docker(DockerError::InvalidTimeout {
                seconds: u32::MAX,
            })),
            "invalid_timeout"
        );
        assert_eq!(
            error_code(&ContainersError::Docker(DockerError::Api(
                "boom".to_owned()
            ))),
            "docker_error"
        );
    }
}
