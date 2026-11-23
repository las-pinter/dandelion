//! systemd unit endpoints and the root-action allowlist.
//!
//! Listing and user-unit actions go over D-Bus. A system-unit action must pass
//! the TOML allowlist and then runs through `sudo -n`; the sudoers drop-in that
//! permits it is generated from the same allowlist file. Every mutating request
//! writes exactly one audit row, including a refused request.

mod allowlist;

pub use allowlist::{AllowlistFileError, DropInError, load_allowlist, sudoers_drop_in};

use axum::{
    Json, Router,
    extract::{Path as UrlPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use dandelion_system::systemd::{
    AllowRule, AllowlistError, JournalEntry, JournalTailRequest, SudoAllowlist, SudoError, Systemd,
    SystemdError, UnitActionRequest, UnitActionResult, UnitInfo, UnitNameError, UnitScope,
    validate_unit_name,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use crate::{app::ApiError, audit::AuditEntry, auth::middleware::error_response, state::AppState};

/// The only account this build authenticates.
const ACTOR: &str = "owner";
/// Journal entries returned when the request does not ask for a count.
pub const DEFAULT_JOURNAL_LINES: usize = 200;
/// Most journal entries one request may ask for.
pub const MAX_JOURNAL_LINES: usize = 1000;

/// What the services page needs in one response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ServicesSnapshot {
    /// Every unit both managers report.
    pub units: Vec<UnitInfo>,
    /// The system-scope rules the sudo allowlist permits.
    pub allowlist: Vec<AllowRule>,
}

/// Why a service request failed.
#[derive(Debug, thiserror::Error)]
pub enum ServicesError {
    /// This process could not reach systemd, so unit actions are disabled.
    #[error("systemd is unavailable to the dandelion server")]
    Unavailable,
    /// The unit name is not one this server may pass to systemd.
    #[error(transparent)]
    InvalidUnit(#[from] UnitNameError),
    /// The allowlist refused the unit or the action.
    #[error(transparent)]
    Allowlist(#[from] AllowlistError),
    /// systemd, sudo, or the journal failed.
    #[error(transparent)]
    Systemd(#[from] SystemdError),
}

/// systemd access plus the allowlist that governs root actions.
#[derive(Debug)]
pub struct Services {
    systemd: Option<Systemd>,
    allowlist: SudoAllowlist,
}

impl Services {
    /// Connects to systemd, keeping the server up when a bus is unreachable.
    pub async fn connect(allowlist: SudoAllowlist) -> Self {
        match Systemd::connect(allowlist.clone()).await {
            Ok(systemd) => Self {
                systemd: Some(systemd),
                allowlist,
            },
            Err(error) => {
                tracing::warn!(%error, "systemd is unavailable; unit actions are disabled");
                Self {
                    systemd: None,
                    allowlist,
                }
            }
        }
    }

    /// A disabled service; tests use this to avoid D-Bus.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            systemd: None,
            allowlist: SudoAllowlist::new(),
        }
    }

    /// A disabled service with an allowlist, for refusal tests.
    #[must_use]
    pub fn with_allowlist(allowlist: SudoAllowlist) -> Self {
        Self {
            systemd: None,
            allowlist,
        }
    }

    /// The allowlist in force.
    #[must_use]
    pub const fn allowlist(&self) -> &SudoAllowlist {
        &self.allowlist
    }

    /// Lists every unit both managers report, with the allowlist rules.
    pub async fn snapshot(&self) -> Result<ServicesSnapshot, ServicesError> {
        let systemd = self.systemd.as_ref().ok_or(ServicesError::Unavailable)?;
        let (user, system) = tokio::join!(
            systemd.list_units(UnitScope::User),
            systemd.list_units(UnitScope::System)
        );
        let mut units = user?;
        units.extend(system?);
        Ok(ServicesSnapshot {
            units,
            allowlist: self.allowlist.rules().to_vec(),
        })
    }

    /// Reads the newest journal entries for one unit.
    pub async fn journal_tail(
        &self,
        scope: UnitScope,
        unit: &str,
        lines: usize,
    ) -> Result<Vec<JournalEntry>, ServicesError> {
        validate_unit_name(unit)?;
        let systemd = self.systemd.as_ref().ok_or(ServicesError::Unavailable)?;
        let request = JournalTailRequest {
            scope,
            unit: unit.to_owned(),
            lines,
        };
        systemd.journal_tail(&request).await.map_err(Into::into)
    }

    /// Checks the allowlist, then runs one unit action.
    ///
    /// The allowlist is checked here as well as inside the crate, so an
    /// unlisted system unit is refused even when systemd is unreachable.
    pub async fn act(
        &self,
        request: &UnitActionRequest,
    ) -> Result<UnitActionResult, ServicesError> {
        validate_unit_name(&request.unit)?;
        if request.scope == UnitScope::System {
            self.allowlist.check(&request.unit, request.action)?;
        }
        let systemd = self.systemd.as_ref().ok_or(ServicesError::Unavailable)?;
        systemd.act(request).await.map_err(Into::into)
    }
}

/// Routes mounted under `/api/services`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(snapshot))
        .route("/action", post(action))
        .route("/{scope}/{unit}/journal", get(journal))
}

/// Query for the journal endpoint.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct JournalQuery {
    /// Number of newest entries to return (default 200, maximum 1000).
    pub lines: Option<usize>,
}

/// Every unit both managers report, with the allowlist rules.
#[utoipa::path(
    get,
    path = "/api/services",
    tag = "services",
    responses(
        (status = 200, description = "Units and allowlist rules", body = ServicesSnapshot),
        (status = 502, description = "systemd did not answer", body = ApiError),
        (status = 503, description = "systemd is unavailable", body = ApiError)
    )
)]
pub async fn snapshot(State(state): State<AppState>) -> Response {
    match state.services.snapshot().await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => services_error_response(&error),
    }
}

/// Sends one mutating action to a unit.
#[utoipa::path(
    post,
    path = "/api/services/action",
    tag = "services",
    request_body = UnitActionRequest,
    responses(
        (status = 200, description = "The action ran", body = UnitActionResult),
        (status = 400, description = "The unit name is not valid", body = ApiError),
        (status = 403, description = "The allowlist or sudo refused the action", body = ApiError),
        (status = 502, description = "systemd or sudo failed", body = ApiError),
        (status = 503, description = "systemd or passwordless sudo is unavailable", body = ApiError)
    )
)]
pub async fn action(
    State(state): State<AppState>,
    Json(request): Json<UnitActionRequest>,
) -> Response {
    let result = state.services.act(&request).await;
    let detail = json!({
        "action": request.action.systemctl_verb(),
        "outcome": outcome(&result),
    });
    record(&state, &request, detail).await;

    match result {
        Ok(result) => Json(result).into_response(),
        Err(error) => services_error_response(&error),
    }
}

/// Reads the newest journal entries for one unit.
#[utoipa::path(
    get,
    path = "/api/services/{scope}/{unit}/journal",
    tag = "services",
    params(
        ("scope" = String, Path, description = "user or system"),
        ("unit" = String, Path, description = "full unit name"),
        JournalQuery
    ),
    responses(
        (status = 200, description = "Journal entries", body = Vec<JournalEntry>),
        (status = 400, description = "The scope or unit name is not valid", body = ApiError),
        (status = 500, description = "The journal could not be read", body = ApiError),
        (status = 503, description = "systemd is unavailable", body = ApiError)
    )
)]
pub async fn journal(
    State(state): State<AppState>,
    UrlPath((scope, unit)): UrlPath<(String, String)>,
    Query(query): Query<JournalQuery>,
) -> Response {
    let Some(scope) = parse_scope(&scope) else {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_scope",
            "scope must be user or system",
        );
    };
    let lines = query
        .lines
        .unwrap_or(DEFAULT_JOURNAL_LINES)
        .clamp(1, MAX_JOURNAL_LINES);

    match state.services.journal_tail(scope, &unit, lines).await {
        Ok(entries) => Json(entries).into_response(),
        Err(error) => services_error_response(&error),
    }
}

fn parse_scope(raw: &str) -> Option<UnitScope> {
    match raw {
        "user" => Some(UnitScope::User),
        "system" => Some(UnitScope::System),
        _ => None,
    }
}

fn scope_name(scope: UnitScope) -> &'static str {
    match scope {
        UnitScope::User => "user",
        UnitScope::System => "system",
    }
}

/// Appends one audit row; a failure is logged, never returned to the caller.
async fn record(state: &AppState, request: &UnitActionRequest, detail: serde_json::Value) {
    let target = format!("{}:{}", scope_name(request.scope), request.unit);
    let detail = detail.to_string();
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor: ACTOR,
            action: "service.action",
            target: &target,
            detail: Some(&detail),
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}

fn outcome(result: &Result<UnitActionResult, ServicesError>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(error) => error_code(error),
    }
}

/// The stable machine-readable code for one service error.
fn error_code(error: &ServicesError) -> &'static str {
    match error {
        ServicesError::Unavailable => "unavailable",
        ServicesError::InvalidUnit(_)
        | ServicesError::Allowlist(AllowlistError::InvalidName(_))
        | ServicesError::Systemd(
            SystemdError::UnitName(_) | SystemdError::Allowlist(AllowlistError::InvalidName(_)),
        ) => "invalid_unit",
        ServicesError::Allowlist(_) | ServicesError::Systemd(SystemdError::Allowlist(_)) => {
            "not_allowed"
        }
        ServicesError::Systemd(SystemdError::Sudo(SudoError::Unavailable { .. })) => {
            "sudo_unavailable"
        }
        ServicesError::Systemd(SystemdError::Sudo(SudoError::Denied { .. })) => "sudo_denied",
        ServicesError::Systemd(SystemdError::Sudo(
            SudoError::Failed { .. } | SudoError::Timeout { .. },
        )) => "action_failed",
        ServicesError::Systemd(SystemdError::Journal(_)) => "journal_error",
        ServicesError::Systemd(SystemdError::Bus(_)) => "systemd_error",
        ServicesError::Systemd(_) => "internal",
    }
}

/// Maps a service error to a status, a stable code, and a message.
fn services_error_response(error: &ServicesError) -> Response {
    let code = error_code(error);
    match error {
        ServicesError::Unavailable
        | ServicesError::Systemd(SystemdError::Sudo(SudoError::Unavailable { .. })) => {
            error_response(StatusCode::SERVICE_UNAVAILABLE, code, &error.to_string())
        }
        ServicesError::InvalidUnit(_) | ServicesError::Systemd(SystemdError::UnitName(_)) => {
            error_response(StatusCode::BAD_REQUEST, code, &error.to_string())
        }
        ServicesError::Allowlist(_)
        | ServicesError::Systemd(
            SystemdError::Allowlist(_) | SystemdError::Sudo(SudoError::Denied { .. }),
        ) => error_response(StatusCode::FORBIDDEN, code, &error.to_string()),
        ServicesError::Systemd(
            SystemdError::Sudo(SudoError::Failed { .. } | SudoError::Timeout { .. })
            | SystemdError::Bus(_),
        ) => error_response(StatusCode::BAD_GATEWAY, code, &error.to_string()),
        ServicesError::Systemd(_) => {
            tracing::error!(%error, "service request failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                code,
                "an internal error occurred",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scope_name;
    use dandelion_system::systemd::UnitScope;

    #[test]
    fn scope_names_are_the_api_words() {
        assert_eq!(scope_name(UnitScope::User), "user");
        assert_eq!(scope_name(UnitScope::System), "system");
    }
}
