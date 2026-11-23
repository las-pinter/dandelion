//! State shared by HTTP handlers.

use std::sync::Arc;

use crate::{audit::AuditLog, auth::Auth, metrics::Metrics, services::Services, ws::hub::Hub};

/// Handlers share the audit log, authentication, the hub, live metrics, and
/// systemd access.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Append-only log for control actions.
    pub audit: AuditLog,
    /// Sessions, passkeys, setup, and origin checks.
    pub auth: Auth,
    /// Fan-out point for live WebSocket events.
    pub hub: Hub,
    /// System metrics, persistence, and the latest snapshot.
    pub metrics: Metrics,
    /// systemd access and the root-action allowlist.
    pub services: Arc<Services>,
}

impl AppState {
    /// Builds the shared state.
    #[must_use]
    pub fn new(
        audit: AuditLog,
        auth: Auth,
        hub: Hub,
        metrics: Metrics,
        services: Services,
    ) -> Self {
        Self {
            audit,
            auth,
            hub,
            metrics,
            services: Arc::new(services),
        }
    }
}
