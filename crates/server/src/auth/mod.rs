//! Single-owner authentication: passkey and password login, sessions, setup.
//!
//!
//! The process is reachable only on loopback, and `tailscale serve` puts it on
//! the tailnet. Every state-changing API request must come from the resolved
//! public origin, and every protected route needs a live session cookie.

pub mod ceremony;
pub mod middleware;
pub mod origin;
pub mod password;
pub mod rate_limit;
pub mod routes;
pub mod session;
pub mod store;
pub mod token;

use sqlx::SqlitePool;
use thiserror::Error;
use webauthn_rs::prelude::{Webauthn, WebauthnBuilder};

use self::{
    ceremony::CeremonyStore,
    origin::{OriginError, PublicOrigin},
    rate_limit::RateLimiter,
    session::{SESSION_TTL_MS, TOUCH_AFTER_MS},
    store::{AuthStore, SessionRecord, StoreError},
    token::{TokenError, hash_token, random_token},
};

/// Setup links expire after fifteen minutes.
pub const SETUP_TOKEN_TTL_MS: i64 = 15 * 60 * 1000;

/// Authentication failures.
#[derive(Debug, Error)]
pub enum AuthError {
    /// The public origin could not be resolved or validated.
    #[error(transparent)]
    Origin(#[from] OriginError),
    /// The database rejected an operation.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Random bytes or hashing failed.
    #[error(transparent)]
    Token(#[from] TokenError),
    /// `webauthn-rs` rejected the relying-party configuration.
    #[error("cannot configure WebAuthn: {0}")]
    Webauthn(String),
}

/// Authentication services shared by HTTP handlers.
#[derive(Debug, Clone)]
pub struct Auth {
    store: AuthStore,
    origin: PublicOrigin,
    webauthn: Webauthn,
    ceremonies: CeremonyStore,
    limiter: RateLimiter,
}

impl Auth {
    /// Builds the services for one origin.
    pub fn new(pool: SqlitePool, origin: PublicOrigin) -> Result<Self, AuthError> {
        let webauthn = WebauthnBuilder::new(origin.rp_id(), origin.url())
            .and_then(|builder| builder.rp_name("Dandelion").build())
            .map_err(|error| AuthError::Webauthn(error.to_string()))?;
        let store = AuthStore::new(pool);
        Ok(Self {
            limiter: RateLimiter::new(store.clone()),
            store,
            origin,
            webauthn,
            ceremonies: CeremonyStore::new(),
        })
    }

    /// The resolved public origin.
    #[must_use]
    pub fn origin(&self) -> &PublicOrigin {
        &self.origin
    }

    /// The persistence layer.
    #[must_use]
    pub fn store(&self) -> &AuthStore {
        &self.store
    }

    /// The `WebAuthn` relying party.
    #[must_use]
    pub fn webauthn(&self) -> &Webauthn {
        &self.webauthn
    }

    /// Pending ceremony state.
    #[must_use]
    pub fn ceremonies(&self) -> &CeremonyStore {
        &self.ceremonies
    }

    /// Failed-login throttling.
    #[must_use]
    pub fn limiter(&self) -> &RateLimiter {
        &self.limiter
    }

    /// Whether the owner has both a password and at least one passkey.
    pub async fn setup_complete(&self) -> Result<bool, StoreError> {
        let Some(owner) = self.store.owner().await? else {
            return Ok(false);
        };
        let has_password = owner.password_hash.is_some();
        let has_passkey = !self.store.passkeys(&owner.id).await?.is_empty();
        Ok(has_password && has_passkey)
    }

    /// Issues a fresh single-use setup token and returns it in clear text.
    ///
    /// Any older setup link stops working first, so only one link is live.
    pub async fn issue_setup_token(&self, now: i64) -> Result<String, AuthError> {
        let token = random_token()?;
        self.store.clear_setup_tokens().await?;
        self.store
            .insert_setup_token(&hash_token(&token), now, now + SETUP_TOKEN_TTL_MS)
            .await?;
        Ok(token)
    }

    /// Whether a setup token may still be used.
    pub async fn setup_token_valid(&self, token: &str, now: i64) -> Result<bool, StoreError> {
        self.store.setup_token_valid(&hash_token(token), now).await
    }

    /// Marks a setup token used; true only for this call.
    pub async fn consume_setup_token(&self, token: &str, now: i64) -> Result<bool, StoreError> {
        self.store
            .consume_setup_token(&hash_token(token), now)
            .await
    }

    /// Creates a session and returns the cookie token.
    pub async fn start_session(&self, user_id: &str, now: i64) -> Result<String, AuthError> {
        let token = random_token()?;
        self.store
            .insert_session(&hash_token(&token), user_id, now, now + SESSION_TTL_MS)
            .await?;
        Ok(token)
    }

    /// Looks up a live session from a cookie token.
    pub async fn session(
        &self,
        token: &str,
        now: i64,
    ) -> Result<Option<SessionRecord>, StoreError> {
        let Some(session) = self.store.session(&hash_token(token)).await? else {
            return Ok(None);
        };
        if session.expires_at <= now {
            if let Err(error) = self.store.delete_session(&session.token_hash).await {
                tracing::warn!(%error, "cannot delete an expired session");
            }
            return Ok(None);
        }
        Ok(Some(session))
    }

    /// Slides a session at most once an hour; true when the cookie must be
    /// refreshed.
    pub async fn touch_session(
        &self,
        session: &SessionRecord,
        now: i64,
    ) -> Result<bool, StoreError> {
        if now.saturating_sub(session.last_seen_at) < TOUCH_AFTER_MS {
            return Ok(false);
        }
        self.store
            .touch_session(&session.token_hash, now, now + SESSION_TTL_MS)
            .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{Auth, SETUP_TOKEN_TTL_MS};
    use crate::{
        auth::{origin::PublicOrigin, token::hash_token},
        storage::open_in_memory,
    };

    async fn auth() -> Auth {
        let pool = open_in_memory().await.expect("in-memory database");
        let origin = PublicOrigin::parse("https://example.test").expect("origin");
        Auth::new(pool, origin).expect("auth services")
    }

    #[tokio::test]
    async fn a_new_setup_token_invalidates_the_old_one() {
        let auth = auth().await;

        let first = auth.issue_setup_token(1).await.expect("first token");
        let second = auth.issue_setup_token(2).await.expect("second token");

        assert!(!auth.setup_token_valid(&first, 3).await.expect("check"));
        assert!(auth.setup_token_valid(&second, 3).await.expect("check"));
    }

    #[tokio::test]
    async fn setup_tokens_expire() {
        let auth = auth().await;
        let token = auth.issue_setup_token(1).await.expect("token");

        assert!(
            auth.setup_token_valid(&token, SETUP_TOKEN_TTL_MS)
                .await
                .expect("check")
        );
        assert!(
            !auth
                .setup_token_valid(&token, 1 + SETUP_TOKEN_TTL_MS)
                .await
                .expect("check")
        );
    }

    #[tokio::test]
    async fn an_expired_session_is_dropped() {
        let auth = auth().await;
        let owner = auth.store().ensure_owner(1).await.expect("owner");
        let token_hash = hash_token("expired");
        auth.store()
            .insert_session(&token_hash, &owner.id, 1, 10)
            .await
            .expect("session");

        let session = auth.session("expired", 20).await.expect("lookup");

        assert!(session.is_none());
        assert!(
            auth.store()
                .session(&token_hash)
                .await
                .expect("read")
                .is_none()
        );
    }

    #[tokio::test]
    async fn setup_is_incomplete_until_both_factors_exist() {
        let auth = auth().await;
        assert!(!auth.setup_complete().await.expect("check"));

        let owner = auth.store().ensure_owner(1).await.expect("owner");
        auth.store()
            .set_password(&owner.id, "$argon2id$placeholder")
            .await
            .expect("password");
        assert!(!auth.setup_complete().await.expect("check"));

        auth.store()
            .insert_passkey(&owner.id, b"credential", "{}", 1)
            .await
            .expect("passkey");
        assert!(auth.setup_complete().await.expect("check"));
    }

    #[tokio::test]
    async fn a_localhost_origin_configures_webauthn() {
        let pool = open_in_memory().await.expect("in-memory database");
        let origin = PublicOrigin::parse("http://localhost:5173").expect("origin");

        assert!(Auth::new(pool, origin).is_ok());
    }

    #[tokio::test]
    async fn challenges_carry_the_public_key_wrapper() {
        let auth = auth().await;
        let (challenge, _state) = auth
            .webauthn()
            .start_passkey_registration(Uuid::new_v4(), "owner", "Owner", None)
            .expect("registration challenge");

        let value = serde_json::to_value(&challenge).expect("serialize");
        let public_key = &value["publicKey"];

        assert!(public_key["challenge"].is_string());
        assert!(public_key["user"]["id"].is_string());
        assert_eq!(public_key["rp"]["name"], "Dandelion");
    }
}
