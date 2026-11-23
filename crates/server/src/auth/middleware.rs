//! Axum middleware: session authentication and request-origin checks.

use axum::{
    Json,
    extract::{Request, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{CACHE_CONTROL, COOKIE, ORIGIN, SET_COOKIE},
    },
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    app::ApiError,
    auth::session::{cookie_name, now_millis, set_cookie, token_from_header},
    state::AppState,
};

/// The authenticated caller attached to a protected request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestUser {
    /// Owner user id.
    pub user_id: String,
    /// Hash of the session token, for logout.
    pub token_hash: String,
}

/// Rejects protected requests without a live session cookie.
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let now = now_millis();
    let expected = cookie_name(state.auth.origin());
    let token = request
        .headers()
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| token_from_header(expected, header));

    let session = match token {
        Some(ref token) => match state.auth.session(token, now).await {
            Ok(session) => session,
            Err(error) => {
                tracing::error!(%error, "session lookup failed");
                return internal_error();
            }
        },
        None => None,
    };

    let Some(session) = session else {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "sign in required",
        );
    };

    let refreshed = state
        .auth
        .touch_session(&session, now)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(%error, "session touch failed");
            false
        });
    request.extensions_mut().insert(RequestUser {
        user_id: session.user_id,
        token_hash: session.token_hash,
    });

    let mut response = next.run(request).await;
    if refreshed
        && let Some(token) = token
        && let Ok(value) = HeaderValue::from_str(&set_cookie(state.auth.origin(), &token))
    {
        response.headers_mut().append(SET_COOKIE, value);
    }
    response
}

/// Rejects state-changing requests that did not come from the public origin.
///
/// Browsers send `Origin` on every `POST`; `Sec-Fetch-Site: same-origin` is the
/// fallback. A missing or foreign origin is refused, so a page on another
/// site cannot drive the API with a stolen cookie.
pub async fn check_origin(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let allowed = safe || request_allowed(&state, &request);

    let mut response = if allowed {
        next.run(request).await
    } else {
        error_response(
            StatusCode::FORBIDDEN,
            "origin_rejected",
            "the request did not come from the Dandelion origin",
        )
    };
    // Auth bodies and live data must never land in a shared cache.
    if !response.headers().contains_key(CACHE_CONTROL) {
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

fn request_allowed(state: &AppState, request: &Request) -> bool {
    origin_allowed(state.auth.origin(), request.headers())
}

/// Whether a request's origin metadata matches the public origin.
///
/// Shared by the mutating-request guard and the WebSocket handshake.
#[must_use]
pub fn origin_allowed(origin: &crate::auth::origin::PublicOrigin, headers: &HeaderMap) -> bool {
    match headers.get(ORIGIN).and_then(|value| value.to_str().ok()) {
        Some(origin_header) => origin.accepts_origin(origin_header),
        None => {
            headers
                .get("sec-fetch-site")
                .and_then(|value| value.to_str().ok())
                == Some("same-origin")
        }
    }
}

/// Builds the JSON error body every API failure uses.
#[must_use]
pub fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(ApiError {
            error: code.to_owned(),
            message: message.to_owned(),
        }),
    )
        .into_response()
}

fn internal_error() -> Response {
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "an internal error occurred",
    )
}
