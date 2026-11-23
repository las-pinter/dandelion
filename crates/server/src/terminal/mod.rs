//! Shell terminals: one local PTY per browser tab.
//!
//! Opening a terminal spawns the server account's shell on a pseudo-terminal.
//! The socket carries raw bytes in binary frames and a small JSON control
//! channel in text frames (resize today). A terminal lives only as long as its
//! socket: closing the socket, closing the tab, or losing the session kills
//! the shell. At most [`MAX_TERMINALS`] shells may be open at once.

mod ws;

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{Extension, Path as UrlPath, State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use dandelion_pty::{Pty, PtyError, PtyOptions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    app::ApiError,
    audit::AuditEntry,
    auth::middleware::{RequestUser, error_response, origin_allowed},
    state::AppState,
};

/// The only account this build authenticates.
pub(crate) const ACTOR: &str = "owner";
/// Most shells one server will hold open at once.
pub const MAX_TERMINALS: usize = 8;
/// Seconds an opened but never-attached shell is kept before it is reaped.
pub const UNATTACHED_TTL_SECS: i64 = 30;

/// What the page needs to know about one open shell.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ShellInfo {
    /// Terminal id, as a UUID string.
    pub id: String,
    /// The shell that runs in it.
    pub shell: String,
    /// Current width in cells.
    pub cols: u16,
    /// Current height in cells.
    pub rows: u16,
    /// Creation time as a Unix timestamp in seconds.
    pub created_at_unix: i64,
    /// True while a browser socket is attached to this shell.
    pub attached: bool,
}

/// A request to open a shell.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OpenTerminalRequest {
    /// Initial width in cells; defaults to 80.
    pub cols: Option<u16>,
    /// Initial height in cells; defaults to 24.
    pub rows: Option<u16>,
}

/// Why a terminal request failed.
#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    /// The server already holds the maximum number of shells.
    #[error("at most {MAX_TERMINALS} terminals may be open")]
    Limit,
    /// The terminal is not open.
    #[error("no such terminal")]
    NotFound,
    /// The shell could not be opened or driven.
    #[error(transparent)]
    Pty(#[from] PtyError),
}

/// One open shell.
#[derive(Debug)]
pub struct Terminal {
    id: Uuid,
    pty: Arc<Pty>,
    created_at_unix: i64,
    attached: AtomicBool,
}

impl Terminal {
    /// The terminal's id.
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// Claims the shell for one socket.
    ///
    /// A terminal serves one socket at a time: two readers on the same PTY
    /// would split the output and the first close would kill both.
    #[must_use]
    pub fn try_attach(&self) -> bool {
        self.attached
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Releases the shell so a later socket may attach.
    pub fn detach(&self) {
        self.attached.store(false, Ordering::SeqCst);
    }

    /// The terminal's current description.
    #[must_use]
    pub fn info(&self) -> ShellInfo {
        let (cols, rows) = self.pty.size();
        ShellInfo {
            id: self.id.to_string(),
            shell: self.pty.shell().to_owned(),
            cols,
            rows,
            created_at_unix: self.created_at_unix,
            attached: self.attached.load(Ordering::SeqCst),
        }
    }

    /// The pseudo-terminal behind this terminal.
    #[must_use]
    pub fn pty(&self) -> &Arc<Pty> {
        &self.pty
    }
}

/// The shells this server holds open.
#[derive(Debug, Clone, Default)]
pub struct Terminals {
    sessions: Arc<Mutex<HashMap<Uuid, Arc<Terminal>>>>,
}

impl Terminals {
    /// Opens a shell, or refuses once the cap is reached.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Limit`] when [`MAX_TERMINALS`] are open and
    /// [`TerminalError::Pty`] when the shell cannot start.
    pub fn open(&self, request: &OpenTerminalRequest) -> Result<ShellInfo, TerminalError> {
        let mut sessions = self.sessions.lock().map_err(|_| TerminalError::NotFound)?;
        reap(&mut sessions);
        if sessions.len() >= MAX_TERMINALS {
            return Err(TerminalError::Limit);
        }
        let options = PtyOptions {
            cols: request.cols.unwrap_or(80),
            rows: request.rows.unwrap_or(24),
            ..PtyOptions::default()
        };
        let pty = Arc::new(Pty::spawn(&options)?);
        let id = Uuid::new_v4();
        let terminal = Terminal {
            id,
            pty,
            created_at_unix: now_seconds(),
            attached: AtomicBool::new(false),
        };
        let info = terminal.info();
        sessions.insert(id, Arc::new(terminal));
        Ok(info)
    }

    /// One open terminal, if it exists.
    #[must_use]
    pub fn get(&self, id: Uuid) -> Option<Arc<Terminal>> {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&id).cloned())
    }

    /// Closes a shell and ends its process, even while a socket holds it.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::NotFound`] when the id is not open.
    pub fn close(&self, id: Uuid) -> Result<(), TerminalError> {
        let mut sessions = self.sessions.lock().map_err(|_| TerminalError::NotFound)?;
        let terminal = sessions.remove(&id).ok_or(TerminalError::NotFound)?;
        // The socket's own `Arc` keeps the terminal around; kill the shell so
        // the reader sees EOF and the socket loop exits.
        terminal.pty().kill();
        Ok(())
    }

    /// Every open terminal, oldest first.
    #[must_use]
    pub fn list(&self) -> Vec<ShellInfo> {
        let Ok(mut sessions) = self.sessions.lock() else {
            return Vec::new();
        };
        reap(&mut sessions);
        let mut infos: Vec<ShellInfo> = sessions.values().map(|terminal| terminal.info()).collect();
        infos.sort_by(|left, right| {
            (left.created_at_unix, &left.id).cmp(&(right.created_at_unix, &right.id))
        });
        infos
    }
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// True when a terminal was opened but never attached, long enough ago.
fn stale(attached: bool, created_at_unix: i64, now: i64) -> bool {
    !attached && now.saturating_sub(created_at_unix) >= UNATTACHED_TTL_SECS
}

/// Drops shells whose page never attached, so a failed upgrade cannot exhaust
/// the cap; dropping a terminal ends its shell.
fn reap(sessions: &mut HashMap<Uuid, Arc<Terminal>>) {
    let now = now_seconds();
    sessions.retain(|_, terminal| {
        !stale(
            terminal.attached.load(Ordering::SeqCst),
            terminal.created_at_unix,
            now,
        )
    });
}

/// Routes mounted under `/api/terminals`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(open))
        .route("/{id}", delete(close))
        .route("/{id}/ws", get(upgrade))
}

/// Opens a shell.
#[utoipa::path(
    post,
    path = "/api/terminals",
    tag = "terminals",
    operation_id = "terminals_open",
    request_body = OpenTerminalRequest,
    responses(
        (status = 200, description = "The shell opened", body = ShellInfo),
        (status = 400, description = "The shell could not start", body = ApiError),
        (status = 409, description = "Too many terminals are open", body = ApiError)
    )
)]
pub async fn open(
    State(state): State<AppState>,
    Json(request): Json<OpenTerminalRequest>,
) -> Response {
    let cols = request.cols.unwrap_or(80);
    let rows = request.rows.unwrap_or(24);
    let registry = state.terminals.clone();
    let result = tokio::task::spawn_blocking(move || registry.open(&request)).await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error, "terminal open task failed");
            return internal_error();
        }
    };
    let detail = json!({
        "shell": result.as_ref().ok().map(|info| info.shell.as_str()).unwrap_or_default(),
        "cols": cols,
        "rows": rows,
        "outcome": if result.is_ok() { "ok" } else { "refused" },
    });
    let target = result.as_ref().map_or_else(
        |_| "terminal:new".to_owned(),
        |info| format!("terminal:{}", info.id),
    );
    record(&state, "terminal.open", &target, detail).await;

    match result {
        Ok(info) => Json(info).into_response(),
        Err(error) => terminal_error_response(&error),
    }
}

/// Lists the open shells.
#[utoipa::path(
    get,
    path = "/api/terminals",
    tag = "terminals",
    operation_id = "terminals_list",
    responses(
        (status = 200, description = "Every open terminal", body = Vec<ShellInfo>)
    )
)]
pub async fn list(State(state): State<AppState>) -> Response {
    Json(state.terminals.list()).into_response()
}

/// Closes one shell.
#[utoipa::path(
    delete,
    path = "/api/terminals/{id}",
    tag = "terminals",
    operation_id = "terminals_close",
    params(("id" = String, Path, description = "terminal id")),
    responses(
        (status = 204, description = "The shell closed"),
        (status = 404, description = "No such terminal", body = ApiError)
    )
)]
pub async fn close(State(state): State<AppState>, UrlPath(id): UrlPath<String>) -> Response {
    let result = parse_id(&id)
        .ok_or(TerminalError::NotFound)
        .and_then(|id| state.terminals.close(id));
    let outcome = if result.is_ok() { "ok" } else { "not_found" };
    record(
        &state,
        "terminal.close",
        &format!("terminal:{id}"),
        json!({ "outcome": outcome }),
    )
    .await;

    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => terminal_error_response(&error),
    }
}

/// Upgrades a same-origin request to the terminal's socket.
pub async fn upgrade(
    State(state): State<AppState>,
    Extension(user): Extension<RequestUser>,
    UrlPath(id): UrlPath<String>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(state.auth.origin(), &headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "origin_rejected",
            "the WebSocket handshake did not come from the Dandelion origin",
        );
    }
    let Some(terminal) = parse_id(&id).and_then(|id| state.terminals.get(id)) else {
        return error_response(StatusCode::NOT_FOUND, "not_found", "no such terminal");
    };
    if !terminal.try_attach() {
        return error_response(
            StatusCode::CONFLICT,
            "terminal_busy",
            "another browser session holds this terminal",
        );
    }

    upgrade
        .max_message_size(ws::MAX_INPUT_BYTES)
        .max_frame_size(ws::MAX_INPUT_BYTES)
        .on_upgrade(move |socket| ws::connection(socket, state, user, terminal))
}

fn parse_id(raw: &str) -> Option<Uuid> {
    Uuid::parse_str(raw).ok()
}

/// Appends one audit row; a failure is logged, never returned to the caller.
async fn record(state: &AppState, action: &str, target: &str, detail: serde_json::Value) {
    let detail = detail.to_string();
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor: ACTOR,
            action,
            target,
            detail: Some(&detail),
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}

fn internal_error() -> Response {
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "an internal error occurred",
    )
}

fn terminal_error_response(error: &TerminalError) -> Response {
    match error {
        TerminalError::Limit => {
            error_response(StatusCode::CONFLICT, "limit_reached", &error.to_string())
        }
        TerminalError::NotFound => {
            error_response(StatusCode::NOT_FOUND, "not_found", &error.to_string())
        }
        TerminalError::Pty(PtyError::Spawn { .. } | PtyError::Open(_)) => {
            tracing::warn!(%error, "cannot open a terminal");
            error_response(
                StatusCode::BAD_REQUEST,
                "spawn_failed",
                "the shell could not be started",
            )
        }
        TerminalError::Pty(_) => internal_error(),
    }
}

#[cfg(test)]
mod tests {
    use super::{UNATTACHED_TTL_SECS, stale};

    #[test]
    fn unattached_shells_age_out() {
        assert!(!stale(true, 0, 10_000), "an attached shell is never reaped");
        assert!(!stale(false, 100, 100 + UNATTACHED_TTL_SECS - 1));
        assert!(stale(false, 100, 100 + UNATTACHED_TTL_SECS));
        assert!(stale(false, 100, 100 + 3_600));
    }
}
