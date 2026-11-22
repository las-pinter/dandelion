//! State shared by HTTP handlers.

use crate::audit::AuditLog;

/// Handlers share the audit log; later tasks add the database and config.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Append-only log for control actions.
    pub audit: AuditLog,
}

impl AppState {
    /// Builds the shared state.
    #[must_use]
    pub const fn new(audit: AuditLog) -> Self {
        Self { audit }
    }
}
