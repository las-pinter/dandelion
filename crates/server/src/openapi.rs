//! The `OpenAPI` document printed by `dandelion openapi`.

use axum::Json;
use utoipa::OpenApi;

use crate::auth::routes::{
    AuthStatus, ChallengeResponse, FinishAuthenticationRequest, FinishRegistrationRequest,
    LoginPasswordRequest, MeResponse, SessionResponse, SetupPasswordRequest, SetupStatus,
    SetupTokenRequest, TokenQuery,
};

/// Every HTTP endpoint Dandelion exposes.
#[derive(Debug, OpenApi)]
#[openapi(
    info(title = "Dandelion API", version = env!("CARGO_PKG_VERSION")),
    paths(
        crate::health::health,
        crate::auth::routes::status,
        crate::auth::routes::setup_status,
        crate::auth::routes::setup_passkey_start,
        crate::auth::routes::setup_passkey_finish,
        crate::auth::routes::setup_password,
        crate::auth::routes::setup_complete,
        crate::auth::routes::login_password,
        crate::auth::routes::login_passkey_start,
        crate::auth::routes::login_passkey_finish,
        crate::auth::routes::me,
        crate::auth::routes::logout,
        crate::auth::routes::logout_all,
        crate::metrics::routes::latest,
        crate::metrics::routes::history,
        crate::processes::list,
        crate::processes::signal,
        crate::processes::renice,
        crate::services::snapshot,
        crate::services::action,
        crate::services::journal,
        crate::containers::snapshot,
        crate::containers::action,
        crate::containers::logs,
        crate::terminal::open,
        crate::terminal::list,
        crate::terminal::close,
        crate::agents::routes::snapshot,
        crate::agents::routes::action,
        crate::agents::routes::spawn,
        crate::agents::routes::screen,
    ),
    components(schemas(
        crate::health::HealthResponse,
        crate::app::ApiError,
        crate::metrics::LiveSample,
        dandelion_sysmon::Snapshot,
        dandelion_sysmon::MetricsPoint,
        dandelion_sysmon::MinuteAggregate,
        dandelion_sysmon::HistorySnapshot,
        dandelion_system::process::ProcessInfo,
        dandelion_system::process::ProcessState,
        dandelion_system::process::ProcessSignal,
        dandelion_system::process::SignalRequest,
        dandelion_system::process::ReniceRequest,
        crate::services::ServicesSnapshot,
        dandelion_system::systemd::UnitInfo,
        dandelion_system::systemd::UnitScope,
        dandelion_system::systemd::UnitAction,
        dandelion_system::systemd::UnitActionRequest,
        dandelion_system::systemd::UnitActionResult,
        dandelion_system::systemd::ActionMechanism,
        dandelion_system::systemd::AllowRule,
        dandelion_system::systemd::JournalEntry,
        crate::containers::ContainersSnapshot,        crate::containers::ContainerAction,
        crate::containers::ContainerActionRequest,
        dandelion_system::docker::ContainerInfo,
        dandelion_system::docker::ContainerState,
        dandelion_system::docker::ContainerLogLine,
        dandelion_system::docker::LogStream,
        dandelion_system::docker::DockerAvailability,
        crate::terminal::ShellInfo,
        crate::terminal::OpenTerminalRequest,
        crate::agents::AgentsSnapshot,
        crate::claude::ClaudeAgent,
        dandelion_claude::SessionStatus,
        dandelion_claude::SessionUsage,
        dandelion_claude::ModelUsage,
        dandelion_claude::TokenUsage,
        crate::agents::AgentAction,
        crate::agents::AgentActionRequest,
        crate::agents::SpawnAgentRequest,
        dandelion_terminals::AgentKind,
        dandelion_terminals::AgentState,
        dandelion_terminals::AgentStatus,
        dandelion_terminals::Access,
        dandelion_terminals::BackendDescriptor,
        dandelion_terminals::BackendKind,
        dandelion_terminals::BlockedReason,
        dandelion_terminals::ProtocolId,
        dandelion_terminals::ReadOrigin,
        dandelion_terminals::ScreenText,
        dandelion_terminals::TerminalId,
        dandelion_terminals::TerminalInfo,
        dandelion_terminals::TerminalLifecycle,
        dandelion_terminals::TerminalSize,
        AuthStatus,
        MeResponse,
        SessionResponse,
        SetupStatus,
        TokenQuery,
        SetupTokenRequest,
        ChallengeResponse,
        FinishRegistrationRequest,
        SetupPasswordRequest,
        LoginPasswordRequest,
        FinishAuthenticationRequest,
    ))
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

    #[test]
    fn document_lists_the_system_routes() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        for path in ["/api/system/latest", "/api/system/history"] {
            assert!(document["paths"][path].is_object(), "missing {path}");
        }
        assert!(document["components"]["schemas"]["Snapshot"].is_object());
    }

    #[test]
    fn document_lists_the_process_routes() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        for path in [
            "/api/processes",
            "/api/processes/signal",
            "/api/processes/renice",
        ] {
            assert!(document["paths"][path].is_object(), "missing {path}");
        }
        assert!(
            document["components"]["schemas"]["ProcessInfo"].is_object(),
            "missing ProcessInfo"
        );
    }

    #[test]
    fn document_lists_the_service_routes() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        for path in [
            "/api/services",
            "/api/services/action",
            "/api/services/{scope}/{unit}/journal",
        ] {
            assert!(document["paths"][path].is_object(), "missing {path}");
        }
        assert!(
            document["components"]["schemas"]["ServicesSnapshot"].is_object(),
            "missing ServicesSnapshot"
        );
    }

    #[test]
    fn document_lists_the_container_routes() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        for path in [
            "/api/containers",
            "/api/containers/action",
            "/api/containers/{container}/logs",
        ] {
            assert!(document["paths"][path].is_object(), "missing {path}");
        }
        assert!(
            document["components"]["schemas"]["ContainersSnapshot"].is_object(),
            "missing ContainersSnapshot"
        );
    }

    #[test]
    fn operation_ids_are_unique() {
        // The generated TypeScript client keys `operations` by operation id, so
        // two handlers that share a function name would silently swap their
        // request and response types.
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");
        let mut seen = std::collections::BTreeMap::new();
        for (path, item) in document["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("path item") {
                let Some(id) = operation["operationId"].as_str() else {
                    continue;
                };
                if let Some(other) = seen.insert(id.to_owned(), format!("{method} {path}")) {
                    panic!("operation id `{id}` is reused by {other}");
                }
            }
        }
        assert!(!seen.is_empty(), "the document lists operations");
    }

    #[test]
    fn document_lists_the_terminal_routes() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        for path in ["/api/terminals", "/api/terminals/{id}"] {
            assert!(document["paths"][path].is_object(), "missing {path}");
        }
        assert!(
            document["components"]["schemas"]["ShellInfo"].is_object(),
            "missing ShellInfo"
        );
    }

    #[test]
    fn document_lists_the_agent_routes() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        for path in [
            "/api/agents",
            "/api/agents/action",
            "/api/agents/spawn",
            "/api/agents/{terminal}/screen",
        ] {
            assert!(document["paths"][path].is_object(), "missing {path}");
        }
        assert!(
            document["components"]["schemas"]["AgentsSnapshot"].is_object(),
            "missing AgentsSnapshot"
        );
        assert!(
            document["components"]["schemas"]["ClaudeAgent"].is_object(),
            "missing ClaudeAgent"
        );
    }

    #[test]
    fn document_lists_the_auth_routes() {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("document serialises");

        for path in [
            "/api/auth/status",
            "/api/auth/login/password",
            "/api/auth/login/passkey/start",
            "/api/auth/login/passkey/finish",
            "/api/auth/setup/complete",
            "/api/auth/logout",
        ] {
            assert!(document["paths"][path].is_object(), "missing {path}");
        }
    }
}
