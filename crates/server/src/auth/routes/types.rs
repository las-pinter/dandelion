//! Request and response bodies for the auth routes.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Setup and login state for the initial screen.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AuthStatus {
    /// No complete owner exists yet.
    pub setup_required: bool,
    /// The request carried a live session.
    pub authenticated: bool,
}

/// The signed-in owner.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MeResponse {
    /// Login name.
    pub username: String,
    /// Display name.
    pub display_name: String,
}

/// A successful session start.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SessionResponse {
    /// Always true; present so the body is never empty.
    pub authenticated: bool,
}

/// Setup progress for one token.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SetupStatus {
    /// The token is unused and unexpired.
    pub valid: bool,
    /// A password is set.
    pub has_password: bool,
    /// At least one passkey is registered.
    pub has_passkey: bool,
}

/// Setup token in or out of a query string.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TokenQuery {
    /// The single-use setup token.
    pub token: String,
}

/// A request carrying a setup token.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SetupTokenRequest {
    /// The single-use setup token.
    pub token: String,
}

/// A `WebAuthn` challenge and the server-side ceremony id.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChallengeResponse {
    /// Opaque id for the finish call.
    pub ceremony_id: String,
    /// The `PublicKeyCredentialCreationOptions` or request options.
    #[schema(value_type = Object)]
    pub options: serde_json::Value,
}

/// Finishes a setup passkey registration.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FinishRegistrationRequest {
    /// The single-use setup token.
    pub token: String,
    /// Id from the start call.
    pub ceremony_id: String,
    /// The browser's registration response.
    #[schema(value_type = Object)]
    pub credential: serde_json::Value,
}

/// Sets the owner password during setup.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SetupPasswordRequest {
    /// The single-use setup token.
    pub token: String,
    /// At least 12 characters.
    pub password: String,
}

/// Password login.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct LoginPasswordRequest {
    /// The candidate password.
    pub password: String,
}

/// Finishes a passkey login.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FinishAuthenticationRequest {
    /// Id from the start call.
    pub ceremony_id: String,
    /// The browser's authentication response.
    #[schema(value_type = Object)]
    pub credential: serde_json::Value,
}
