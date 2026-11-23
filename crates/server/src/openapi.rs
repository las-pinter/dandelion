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
