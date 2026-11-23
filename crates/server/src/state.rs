//! State shared by HTTP handlers.

use crate::{audit::AuditLog, auth::Auth, ws::hub::Hub};

/// Handlers share the audit log, the authentication services, and the hub.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Append-only log for control actions.
    pub audit: AuditLog,
    /// Sessions, passkeys, setup, and origin checks.
    pub auth: Auth,
    /// Fan-out point for live WebSocket events.
    pub hub: Hub,
}

impl AppState {
    /// Builds the shared state.
    #[must_use]
    pub const fn new(audit: AuditLog, auth: Auth, hub: Hub) -> Self {
        Self { audit, auth, hub }
    }
}
