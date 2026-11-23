//! Auth status, the signed-in owner, and logout.

use axum::{
    Json,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};

use crate::{
    app::ApiError,
    auth::{
        middleware::RequestUser,
        session::{cookie_name, now_millis, token_from_header},
        store::SessionRecord,
    },
    state::AppState,
};

use super::{
    ApiResult, audit, internal,
    types::{AuthStatus, MeResponse},
    with_cleared_cookie,
};

/// Reports whether setup is needed and whether the caller is signed in.
#[utoipa::path(get, path = "/api/auth/status", responses((status = 200, body = AuthStatus)))]
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<AuthStatus>> {
    let setup_required = !state.auth.setup_complete().await.map_err(internal)?;
    let authenticated = session_from_headers(&state, &headers).await?.is_some();

    Ok(Json(AuthStatus {
        setup_required,
        authenticated,
    }))
}

/// Returns the signed-in owner.
#[utoipa::path(get, path = "/api/auth/me", responses((status = 200, body = MeResponse), (status = 401, body = ApiError)))]
pub async fn me(
    State(state): State<AppState>,
    Extension(user): Extension<RequestUser>,
) -> ApiResult<Json<MeResponse>> {
    let owner = state
        .auth
        .store()
        .user(&user.user_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            crate::auth::middleware::error_response(
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
                "sign in required",
            )
        })?;

    Ok(Json(MeResponse {
        username: owner.username,
        display_name: owner.display_name,
    }))
}

/// Ends the current session.
#[utoipa::path(post, path = "/api/auth/logout", responses((status = 204), (status = 401, body = ApiError)))]
pub async fn logout(
    State(state): State<AppState>,
    Extension(user): Extension<RequestUser>,
) -> ApiResult<Response> {
    state
        .auth
        .store()
        .delete_session(&user.token_hash)
        .await
        .map_err(internal)?;
    state.auth.revoke_session(&user.token_hash);
    audit(&state, "owner", "auth.logout", "session", None).await;

    Ok(with_cleared_cookie(&state))
}

/// Ends every session of the owner.
#[utoipa::path(post, path = "/api/auth/logout-all", responses((status = 204), (status = 401, body = ApiError)))]
pub async fn logout_all(
    State(state): State<AppState>,
    Extension(user): Extension<RequestUser>,
) -> ApiResult<Response> {
    state
        .auth
        .store()
        .delete_sessions(&user.user_id)
        .await
        .map_err(internal)?;
    state.auth.revoke_user(&user.user_id);
    audit(&state, "owner", "auth.logout.all", "owner", None).await;

    Ok(with_cleared_cookie(&state))
}

async fn session_from_headers(
    state: &AppState,
    headers: &HeaderMap,
) -> ApiResult<Option<SessionRecord>> {
    let expected = cookie_name(state.auth.origin());
    let Some(token) = headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|header| token_from_header(expected, header))
    else {
        return Ok(None);
    };
    state
        .auth
        .session(&token, now_millis())
        .await
        .map_err(internal)
}
