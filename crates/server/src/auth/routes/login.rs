//! Password and passkey sign-in.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use webauthn_rs::prelude::{Passkey, PublicKeyCredential};

use crate::{
    app::ApiError,
    auth::{password, session::now_millis, store::StoredPasskey},
    state::AppState,
};

use super::{
    ApiResult, audit, bad_request, internal, login_failed, parse_ceremony, request_source,
    throttle,
    types::{
        ChallengeResponse, FinishAuthenticationRequest, LoginPasswordRequest, SessionResponse,
    },
    with_cookie,
};

/// Logs in with the owner password.
#[utoipa::path(post, path = "/api/auth/login/password", request_body = LoginPasswordRequest, responses((status = 200, body = SessionResponse), (status = 401, body = ApiError), (status = 429, body = ApiError)))]
pub async fn login_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginPasswordRequest>,
) -> ApiResult<Response> {
    let now = now_millis();
    let source = request_source(&headers);
    throttle(&state, &source, now).await?;

    let Some(owner) = state.auth.store().owner().await.map_err(internal)? else {
        return Err(bad_request(
            "password_login_unavailable",
            "setup is not complete",
        ));
    };
    let Some(stored) = owner.password_hash.as_deref() else {
        return Err(bad_request(
            "password_login_unavailable",
            "no password is set",
        ));
    };

    if !password::verify(&request.password, stored) {
        login_failed(&state, &source, "password", now).await?;
        return Err(crate::auth::middleware::error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "the password is incorrect",
        ));
    }

    let token = state
        .auth
        .start_session(&owner.id, now)
        .await
        .map_err(internal)?;
    audit(
        &state,
        "owner",
        "auth.login",
        &owner.username,
        Some(r#"{"factor":"password"}"#),
    )
    .await;

    Ok(with_cookie(
        &state,
        &token,
        Json(SessionResponse {
            authenticated: true,
        }),
    ))
}

/// Starts a passkey login ceremony.
#[utoipa::path(post, path = "/api/auth/login/passkey/start", responses((status = 200, body = ChallengeResponse), (status = 400, body = ApiError), (status = 429, body = ApiError)))]
pub async fn login_passkey_start(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<ChallengeResponse>> {
    let now = now_millis();
    let source = request_source(&headers);
    throttle(&state, &source, now).await?;

    let stored = state.auth.store().all_passkeys().await.map_err(internal)?;
    let passkeys = usable_passkeys(stored);
    if passkeys.is_empty() {
        return Err(bad_request(
            "passkey_login_unavailable",
            "no passkey is registered",
        ));
    }

    let (challenge, authentication) = state
        .auth
        .webauthn()
        .start_passkey_authentication(&passkeys)
        .map_err(|error| bad_request("passkey_start_failed", &error.to_string()))?;
    let ceremony_id = state
        .auth
        .ceremonies()
        .insert_authentication(authentication);
    let options = serde_json::to_value(&challenge).map_err(internal)?;

    Ok(Json(ChallengeResponse {
        ceremony_id: ceremony_id.to_string(),
        options,
    }))
}

/// Completes a passkey login ceremony.
#[utoipa::path(post, path = "/api/auth/login/passkey/finish", request_body = FinishAuthenticationRequest, responses((status = 200, body = SessionResponse), (status = 401, body = ApiError), (status = 429, body = ApiError)))]
pub async fn login_passkey_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<FinishAuthenticationRequest>,
) -> ApiResult<Response> {
    let now = now_millis();
    let source = request_source(&headers);
    throttle(&state, &source, now).await?;

    let ceremony_id = parse_ceremony(&request.ceremony_id)?;
    let Some(authentication) = state.auth.ceremonies().take_authentication(ceremony_id) else {
        return Err(bad_request(
            "ceremony_not_found",
            "the login challenge has expired; start again",
        ));
    };
    let credential: PublicKeyCredential = serde_json::from_value(request.credential)
        .map_err(|_| bad_request("invalid_credential", "the credential response is malformed"))?;

    let result = match state
        .auth
        .webauthn()
        .finish_passkey_authentication(&credential, &authentication)
    {
        Ok(result) if result.user_verified() => result,
        _ => {
            login_failed(&state, &source, "passkey", now).await?;
            return Err(crate::auth::middleware::error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "the passkey could not be verified",
            ));
        }
    };

    let Some(stored) = state
        .auth
        .store()
        .passkey_by_credential(result.cred_id())
        .await
        .map_err(internal)?
    else {
        login_failed(&state, &source, "passkey", now).await?;
        return Err(crate::auth::middleware::error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "the passkey is not registered",
        ));
    };

    update_credential(&state, &stored, &result, now).await;
    let Some(owner) = state
        .auth
        .store()
        .user(&stored.user_id)
        .await
        .map_err(internal)?
    else {
        return Err(crate::auth::middleware::error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "the passkey owner no longer exists",
        ));
    };

    let token = state
        .auth
        .start_session(&owner.id, now)
        .await
        .map_err(internal)?;
    audit(
        &state,
        "owner",
        "auth.login",
        &owner.username,
        Some(r#"{"factor":"passkey"}"#),
    )
    .await;

    Ok(with_cookie(
        &state,
        &token,
        Json(SessionResponse {
            authenticated: true,
        }),
    ))
}

fn usable_passkeys(stored: Vec<StoredPasskey>) -> Vec<Passkey> {
    stored
        .into_iter()
        .filter_map(|row| match serde_json::from_str::<Passkey>(&row.passkey) {
            Ok(passkey) => Some(passkey),
            Err(error) => {
                tracing::warn!(%error, "skipping unreadable passkey");
                None
            }
        })
        .collect()
}

async fn update_credential(
    state: &AppState,
    stored: &StoredPasskey,
    result: &webauthn_rs::prelude::AuthenticationResult,
    now: i64,
) {
    let Ok(mut passkey) = serde_json::from_str::<Passkey>(&stored.passkey) else {
        return;
    };
    if passkey.update_credential(result) != Some(true) {
        return;
    }
    let Ok(encoded) = serde_json::to_string(&passkey) else {
        return;
    };
    if let Err(error) = state
        .auth
        .store()
        .update_passkey(&stored.credential_id, &encoded, now)
        .await
    {
        tracing::error!(%error, "cannot persist passkey counter");
    }
}
