//! State shared by HTTP handlers.

use std::sync::Arc;

use crate::{
    agents::Agents, audit::AuditLog, auth::Auth, claude::ClaudeMonitor, containers::Containers,
    metrics::Metrics, services::Services, terminal::Terminals, ws::hub::Hub,
};

/// Handlers share the audit log, authentication, the hub, live metrics,
/// systemd access, Docker access, and the Claude Code session cache.
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
    /// Docker access; actions are disabled when the socket is missing.
    pub containers: Containers,
    /// The shells this server holds open for the web terminal.
    pub terminals: Terminals,
    /// The herdr backend that lists and drives agents.
    pub agents: Agents,
    /// Claude Code sessions, their tokens, and what they wait for.
    pub claude: ClaudeMonitor,
}

impl AppState {
    /// Builds the shared state.
    // The composition root lists every service; grouping them into a struct
    // would only move the same arguments to another type.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        audit: AuditLog,
        auth: Auth,
        hub: Hub,
        metrics: Metrics,
        services: Services,
        containers: Containers,
        agents: Agents,
        claude: ClaudeMonitor,
    ) -> Self {
        Self {
            audit,
            auth,
            hub,
            metrics,
            services: Arc::new(services),
            containers,
            terminals: Terminals::default(),
            agents,
            claude,
        }
    }

    /// True while the session behind `token_hash` is still valid.
    ///
    /// Live sockets use this to drop a revoked or expired login.
    pub async fn session_is_live(&self, token_hash: &str) -> bool {
        match self
            .auth
            .session_live(token_hash, crate::auth::session::now_millis())
            .await
        {
            Ok(live) => live,
            Err(error) => {
                tracing::error!(%error, "session recheck failed");
                false
            }
        }
    }
}
