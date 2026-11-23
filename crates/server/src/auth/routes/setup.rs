//! Setup: issue factors through a single-use token, then start a session.

use axum::{
    Json,
    extract::{Query, State},
};
use uuid::Uuid;
use webauthn_rs::prelude::RegisterPublicKeyCredential;

use crate::{
    app::ApiError,
    auth::{password::PasswordError, session::now_millis, token::hash_token},
    state::AppState,
};

use super::{
    ApiResult, audit, bad_request, factor_state, internal, parse_ceremony, require_setup_token,
    setup_snapshot,
    types::{
        ChallengeResponse, FinishRegistrationRequest, SessionResponse, SetupPasswordRequest,
        SetupStatus, SetupTokenRequest, TokenQuery,
    },
    with_cookie,
};

/// Reports setup token validity and which factors are already registered.
#[utoipa::path(get, path = "/api/auth/setup/status", params(("token" = String, Query, description = "setup token")), responses((status = 200, body = SetupStatus)))]
pub async fn setup_status(
    State(state): State<AppState>,
    Query(query): Query<TokenQuery>,
) -> ApiResult<Json<SetupStatus>> {
    let now = now_millis();
    let valid = state
        .auth
        .setup_token_valid(&query.token, now)
        .await
        .map_err(internal)?;
    let (has_password, has_passkey) = if valid {
        factor_state(&state).await?
    } else {
        (false, false)
    };

    Ok(Json(SetupStatus {
        valid,
        has_password,
        has_passkey,
    }))
}

/// Starts passkey registration for the owner during setup.
#[utoipa::path(post, path = "/api/auth/setup/passkey/start", request_body = SetupTokenRequest, responses((status = 200, body = ChallengeResponse), (status = 400, body = ApiError)))]
pub async fn setup_passkey_start(
    State(state): State<AppState>,
    Json(request): Json<SetupTokenRequest>,
) -> ApiResult<Json<ChallengeResponse>> {
    let now = now_millis();
    require_setup_token(&state, &request.token, now).await?;

    let owner = state
        .auth
        .store()
        .ensure_owner(now)
        .await
        .map_err(internal)?;
    let existing = state
        .auth
        .store()
        .passkeys(&owner.id)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|passkey| passkey.credential_id)
        .collect();
    let user_id = Uuid::parse_str(&owner.id).unwrap_or_else(|_| Uuid::new_v4());

    let (challenge, registration) = state
        .auth
        .webauthn()
        .start_passkey_registration(
            user_id,
            &owner.username,
            &owner.display_name,
            Some(existing),
        )
        .map_err(|error| bad_request("passkey_start_failed", &error.to_string()))?;
    let ceremony_id = state
        .auth
        .ceremonies()
        .insert_registration(hash_token(&request.token), registration);
    let options = serde_json::to_value(&challenge).map_err(internal)?;

    Ok(Json(ChallengeResponse {
        ceremony_id: ceremony_id.to_string(),
        options,
    }))
}

/// Completes passkey registration for the owner.
#[utoipa::path(post, path = "/api/auth/setup/passkey/finish", request_body = FinishRegistrationRequest, responses((status = 200, body = SetupStatus), (status = 400, body = ApiError)))]
pub async fn setup_passkey_finish(
    State(state): State<AppState>,
    Json(request): Json<FinishRegistrationRequest>,
) -> ApiResult<Json<SetupStatus>> {
    let now = now_millis();
    require_setup_token(&state, &request.token, now).await?;

    let ceremony_id = parse_ceremony(&request.ceremony_id)?;
    let Some((token_hash, registration)) = state.auth.ceremonies().take_registration(ceremony_id)
    else {
        return Err(bad_request(
            "ceremony_not_found",
            "the registration challenge has expired; start again",
        ));
    };
    if token_hash != hash_token(&request.token) {
        return Err(bad_request(
            "ceremony_mismatch",
            "the registration belongs to another setup token",
        ));
    }

    let credential: RegisterPublicKeyCredential = serde_json::from_value(request.credential)
        .map_err(|_| bad_request("invalid_credential", "the credential response is malformed"))?;
    let passkey = state
        .auth
        .webauthn()
        .finish_passkey_registration(&credential, &registration)
        .map_err(|error| bad_request("passkey_registration_failed", &error.to_string()))?;

    if state
        .auth
        .store()
        .credential_exists(passkey.cred_id())
        .await
        .map_err(internal)?
    {
        return Err(bad_request(
            "credential_registered",
            "this passkey is already registered",
        ));
    }

    let owner = state
        .auth
        .store()
        .ensure_owner(now)
        .await
        .map_err(internal)?;
    let encoded = serde_json::to_string(&passkey).map_err(internal)?;
    state
        .auth
        .store()
        .insert_passkey(&owner.id, passkey.cred_id(), &encoded, now)
        .await
        .map_err(internal)?;
    audit(&state, "owner", "auth.setup.passkey", &owner.username, None).await;

    setup_snapshot(&state).await
}

/// Stores the owner password during setup.
#[utoipa::path(post, path = "/api/auth/setup/password", request_body = SetupPasswordRequest, responses((status = 200, body = SetupStatus), (status = 400, body = ApiError)))]
pub async fn setup_password(
    State(state): State<AppState>,
    Json(request): Json<SetupPasswordRequest>,
) -> ApiResult<Json<SetupStatus>> {
    let now = now_millis();
    require_setup_token(&state, &request.token, now).await?;

    let hash = crate::auth::password::hash(&request.password).map_err(|error| match error {
        PasswordError::TooShort => bad_request("password_too_short", &error.to_string()),
        PasswordError::Hash(_) => internal(error),
    })?;
    let owner = state
        .auth
        .store()
        .ensure_owner(now)
        .await
        .map_err(internal)?;
    state
        .auth
        .store()
        .set_password(&owner.id, &hash)
        .await
        .map_err(internal)?;
    audit(
        &state,
        "owner",
        "auth.setup.password",
        &owner.username,
        None,
    )
    .await;

    setup_snapshot(&state).await
}

/// Consumes the setup token and starts the first session.
#[utoipa::path(post, path = "/api/auth/setup/complete", request_body = SetupTokenRequest, responses((status = 200, body = SessionResponse), (status = 400, body = ApiError)))]
pub async fn setup_complete(
    State(state): State<AppState>,
    Json(request): Json<SetupTokenRequest>,
) -> ApiResult<axum::response::Response> {
    let now = now_millis();
    require_setup_token(&state, &request.token, now).await?;

    let (has_password, has_passkey) = factor_state(&state).await?;
    if !has_password || !has_passkey {
        return Err(bad_request(
            "setup_incomplete",
            "register a passkey and set a password before finishing",
        ));
    }
    if !state
        .auth
        .consume_setup_token(&request.token, now)
        .await
        .map_err(internal)?
    {
        return Err(bad_request(
            "invalid_setup_token",
            "the setup link is invalid or has expired",
        ));
    }

    let owner = state
        .auth
        .store()
        .ensure_owner(now)
        .await
        .map_err(internal)?;
    let token = state
        .auth
        .start_session(&owner.id, now)
        .await
        .map_err(internal)?;
    audit(
        &state,
        "owner",
        "auth.setup.complete",
        &owner.username,
        None,
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
