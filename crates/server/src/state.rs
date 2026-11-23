//! State shared by HTTP handlers.

use crate::{audit::AuditLog, auth::Auth};

/// Handlers share the audit log and the authentication services.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Append-only log for control actions.
    pub audit: AuditLog,
    /// Sessions, passkeys, setup, and origin checks.
    pub auth: Auth,
}

impl AppState {
    /// Builds the shared state.
    #[must_use]
    pub const fn new(audit: AuditLog, auth: Auth) -> Self {
        Self { audit, auth }
    }
}
