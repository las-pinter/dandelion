//! `/api/auth` routes: setup, login, session inspection, logout.
#![allow(clippy::result_large_err)]

mod login;
mod session;
mod setup;
mod types;

pub use login::{login_passkey_finish, login_passkey_start, login_password};
pub use session::{logout, logout_all, me, status};
pub use setup::{
    setup_complete, setup_passkey_finish, setup_passkey_start, setup_password, setup_status,
};

// utoipa generates one marker type per path; re-export them so the document
// can name `crate::auth::routes::<handler>` through this facade.
pub use login::{__path_login_passkey_finish, __path_login_passkey_start, __path_login_password};
pub use session::{__path_logout, __path_logout_all, __path_me, __path_status};
pub use setup::{
    __path_setup_complete, __path_setup_passkey_finish, __path_setup_passkey_start,
    __path_setup_password, __path_setup_status,
};
pub use types::{
    AuthStatus, ChallengeResponse, FinishAuthenticationRequest, FinishRegistrationRequest,
    LoginPasswordRequest, MeResponse, SessionResponse, SetupPasswordRequest, SetupStatus,
    SetupTokenRequest, TokenQuery,
};

use std::time::Duration;

use axum::{
    Router,
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{RETRY_AFTER, SET_COOKIE},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use uuid::Uuid;

use crate::{audit::AuditEntry, auth::middleware::error_response, state::AppState};

/// Public auth routes: everything needed before a session exists.
pub fn public() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/setup/status", get(setup_status))
        .route("/setup/passkey/start", post(setup_passkey_start))
        .route("/setup/passkey/finish", post(setup_passkey_finish))
        .route("/setup/password", post(setup_password))
        .route("/setup/complete", post(setup_complete))
        .route("/login/password", post(login_password))
        .route("/login/passkey/start", post(login_passkey_start))
        .route("/login/passkey/finish", post(login_passkey_finish))
}

/// Protected auth routes: they require a session cookie.
pub fn protected() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/logout", post(logout))
        .route("/logout-all", post(logout_all))
}

pub(super) type ApiResult<T> = Result<T, Response>;

/// Trusts `X-Forwarded-For` because the listener is loopback-only: only
/// tailscaled and local processes can reach it, and tailscaled sets the
/// header for remote tailnet clients. The last hop is the one tailscaled
/// wrote; earlier hops could be client-supplied.
pub(super) fn request_source(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("local")
        .to_owned()
}

pub(super) async fn factor_state(state: &AppState) -> ApiResult<(bool, bool)> {
    let Some(owner) = state.auth.store().owner().await.map_err(internal)? else {
        return Ok((false, false));
    };
    let has_password = owner.password_hash.is_some();
    let has_passkey = !state
        .auth
        .store()
        .passkeys(&owner.id)
        .await
        .map_err(internal)?
        .is_empty();
    Ok((has_password, has_passkey))
}

pub(super) async fn setup_snapshot(state: &AppState) -> ApiResult<axum::Json<SetupStatus>> {
    let (has_password, has_passkey) = factor_state(state).await?;
    Ok(axum::Json(SetupStatus {
        valid: true,
        has_password,
        has_passkey,
    }))
}

pub(super) async fn require_setup_token(state: &AppState, token: &str, now: i64) -> ApiResult<()> {
    if state
        .auth
        .setup_token_valid(token, now)
        .await
        .map_err(internal)?
    {
        return Ok(());
    }
    Err(bad_request(
        "invalid_setup_token",
        "the setup link is invalid or has expired",
    ))
}

pub(super) async fn throttle(state: &AppState, source: &str, now: i64) -> ApiResult<()> {
    let Some(retry) = state
        .auth
        .limiter()
        .retry_after(source, now)
        .await
        .map_err(internal)?
    else {
        return Ok(());
    };
    Err(throttled(retry))
}

pub(super) async fn login_failed(
    state: &AppState,
    source: &str,
    factor: &str,
    now: i64,
) -> ApiResult<()> {
    state
        .auth
        .limiter()
        .record_failure(source, now)
        .await
        .map_err(internal)?;
    tracing::warn!(%source, factor, "failed login");
    audit(
        state,
        "anonymous",
        "auth.login.failure",
        source,
        Some(&format!(r#"{{"factor":"{factor}"}}"#)),
    )
    .await;
    Ok(())
}

pub(super) fn parse_ceremony(raw: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(raw)
        .map_err(|_| bad_request("invalid_ceremony", "the ceremony id is malformed"))
}

pub(super) fn with_cookie(state: &AppState, token: &str, body: impl IntoResponse) -> Response {
    let mut response = body.into_response();
    if let Ok(value) = HeaderValue::from_str(&crate::auth::session::set_cookie(
        state.auth.origin(),
        token,
    )) {
        response.headers_mut().append(SET_COOKIE, value);
    }
    response
}

pub(super) fn with_cleared_cookie(state: &AppState) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Ok(value) =
        HeaderValue::from_str(&crate::auth::session::clear_cookie(state.auth.origin()))
    {
        response.headers_mut().append(SET_COOKIE, value);
    }
    response
}

fn throttled(retry: Duration) -> Response {
    let seconds = retry.as_secs().max(1);
    let mut response = error_response(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "too many failed attempts; wait before trying again",
    );
    if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
        response.headers_mut().insert(RETRY_AFTER, value);
    }
    response
}

pub(super) fn bad_request(code: &str, message: &str) -> Response {
    error_response(StatusCode::BAD_REQUEST, code, message)
}

pub(super) fn internal(error: impl std::fmt::Display) -> Response {
    tracing::error!(%error, "auth request failed");
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "an internal error occurred",
    )
}

pub(super) async fn audit(
    state: &AppState,
    actor: &str,
    action: &str,
    target: &str,
    detail: Option<&str>,
) {
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor,
            action,
            target,
            detail,
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::request_source;

    #[test]
    fn the_rate_limit_source_is_the_last_forwarded_hop() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.9, 100.64.0.1"),
        );

        assert_eq!(request_source(&headers), "100.64.0.1");
        assert_eq!(request_source(&HeaderMap::new()), "local");
    }
}
