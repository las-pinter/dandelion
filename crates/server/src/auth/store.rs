//! Persistence for users, passkeys, sessions, setup tokens, and failures.
//!
//! Every method is a single statement or a short transaction; policy lives in
//! the callers so the storage layer stays a thin, testable boundary.

use sqlx::SqlitePool;
use thiserror::Error;
use uuid::Uuid;

/// Storage failures.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The database rejected an operation.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// The single owner account.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct User {
    /// Stable user id (UUID v4).
    pub id: String,
    /// Login name; always `owner` in v1.
    pub username: String,
    /// Name shown in the UI.
    pub display_name: String,
    /// Argon2id PHC string, once a password is set.
    pub password_hash: Option<String>,
    /// Unix epoch milliseconds.
    pub created_at: i64,
}

/// A stored `WebAuthn` credential.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredPasskey {
    /// Raw credential id.
    pub credential_id: Vec<u8>,
    /// Owning user.
    pub user_id: String,
    /// Serialized `webauthn_rs::prelude::Passkey`.
    pub passkey: String,
    /// Unix epoch milliseconds.
    pub created_at: i64,
    /// Last successful authentication, in epoch milliseconds.
    pub last_used_at: Option<i64>,
}

/// A server-side session.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SessionRecord {
    /// SHA-256 of the cookie token.
    pub token_hash: String,
    /// Owning user.
    pub user_id: String,
    /// Unix epoch milliseconds.
    pub created_at: i64,
    /// Last request that proved the session, in epoch milliseconds.
    pub last_seen_at: i64,
    /// Sliding expiry in epoch milliseconds.
    pub expires_at: i64,
}

/// The authentication tables.
#[derive(Debug, Clone)]
pub struct AuthStore {
    pool: SqlitePool,
}

impl AuthStore {
    /// Wraps an open database pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Returns the owner account, if one exists.
    pub async fn owner(&self) -> Result<Option<User>, StoreError> {
        let user = sqlx::query_as::<_, User>(
            "SELECT id, username, display_name, password_hash, created_at
             FROM users ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    /// Loads one user by id.
    pub async fn user(&self, id: &str) -> Result<Option<User>, StoreError> {
        let user = sqlx::query_as::<_, User>(
            "SELECT id, username, display_name, password_hash, created_at
             FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    /// Returns the owner account, creating it when absent.
    pub async fn ensure_owner(&self, now: i64) -> Result<User, StoreError> {
        if let Some(user) = self.owner().await? {
            return Ok(user);
        }
        sqlx::query(
            "INSERT INTO users (id, username, display_name, password_hash, created_at)
             VALUES (?, 'owner', 'Owner', NULL, ?)
             ON CONFLICT(username) DO NOTHING",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.owner()
            .await?
            .ok_or_else(|| StoreError::Sqlx(sqlx::Error::RowNotFound))
    }

    /// Stores a new password hash.
    pub async fn set_password(&self, user_id: &str, hash: &str) -> Result<(), StoreError> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Lists the passkeys of one user.
    pub async fn passkeys(&self, user_id: &str) -> Result<Vec<StoredPasskey>, StoreError> {
        let passkeys = sqlx::query_as::<_, StoredPasskey>(
            "SELECT credential_id, user_id, passkey, created_at, last_used_at
             FROM passkeys WHERE user_id = ? ORDER BY created_at",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(passkeys)
    }

    /// Lists every passkey; v1 has exactly one owner.
    pub async fn all_passkeys(&self) -> Result<Vec<StoredPasskey>, StoreError> {
        let passkeys = sqlx::query_as::<_, StoredPasskey>(
            "SELECT credential_id, user_id, passkey, created_at, last_used_at
             FROM passkeys ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(passkeys)
    }

    /// Adds a passkey for a user.
    pub async fn insert_passkey(
        &self,
        user_id: &str,
        credential_id: &[u8],
        passkey: &str,
        now: i64,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO passkeys (credential_id, user_id, passkey, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(credential_id)
        .bind(user_id)
        .bind(passkey)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Whether a credential id is already registered to any user.
    pub async fn credential_exists(&self, credential_id: &[u8]) -> Result<bool, StoreError> {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM passkeys WHERE credential_id = ?")
                .bind(credential_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(exists.is_some())
    }

    /// Loads a passkey by credential id.
    pub async fn passkey_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<StoredPasskey>, StoreError> {
        let passkey = sqlx::query_as::<_, StoredPasskey>(
            "SELECT credential_id, user_id, passkey, created_at, last_used_at
             FROM passkeys WHERE credential_id = ?",
        )
        .bind(credential_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(passkey)
    }

    /// Persists an updated passkey (counter and backup state).
    pub async fn update_passkey(
        &self,
        credential_id: &[u8],
        passkey: &str,
        now: i64,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE passkeys SET passkey = ?, last_used_at = ? WHERE credential_id = ?")
            .bind(passkey)
            .bind(now)
            .bind(credential_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Stores a session row.
    pub async fn insert_session(
        &self,
        token_hash: &str,
        user_id: &str,
        now: i64,
        expires_at: i64,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, last_seen_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(token_hash)
        .bind(user_id)
        .bind(now)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Loads a session by token hash, regardless of expiry.
    pub async fn session(&self, token_hash: &str) -> Result<Option<SessionRecord>, StoreError> {
        let session = sqlx::query_as::<_, SessionRecord>(
            "SELECT token_hash, user_id, created_at, last_seen_at, expires_at
             FROM sessions WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(session)
    }

    /// Slides a session: new last-seen time and expiry.
    pub async fn touch_session(
        &self,
        token_hash: &str,
        now: i64,
        expires_at: i64,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?")
            .bind(now)
            .bind(expires_at)
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deletes one session.
    pub async fn delete_session(&self, token_hash: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deletes every session of a user.
    pub async fn delete_sessions(&self, user_id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Stores a setup token.
    pub async fn insert_setup_token(
        &self,
        token_hash: &str,
        now: i64,
        expires_at: i64,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO setup_tokens (token_hash, created_at, expires_at, used_at)
             VALUES (?, ?, ?, NULL)",
        )
        .bind(token_hash)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Deletes every setup token, used or not.
    pub async fn clear_setup_tokens(&self) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM setup_tokens")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Whether a setup token is unused and unexpired.
    pub async fn setup_token_valid(&self, token_hash: &str, now: i64) -> Result<bool, StoreError> {
        let valid: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM setup_tokens
             WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(valid.is_some())
    }

    /// Marks a setup token used; returns whether this call consumed it.
    pub async fn consume_setup_token(
        &self,
        token_hash: &str,
        now: i64,
    ) -> Result<bool, StoreError> {
        let result = sqlx::query(
            "UPDATE setup_tokens SET used_at = ?
             WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
        )
        .bind(now)
        .bind(token_hash)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Records one failed login attempt.
    pub async fn record_failure(&self, source: &str, now: i64) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO login_failures (source, occurred_at) VALUES (?, ?)")
            .bind(source)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Counts failures for one source since a timestamp.
    pub async fn failures_since(&self, source: &str, since: i64) -> Result<i64, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM login_failures WHERE source = ? AND occurred_at > ?",
        )
        .bind(source)
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Returns the newest failure timestamp for one source.
    pub async fn last_failure(&self, source: &str) -> Result<Option<i64>, StoreError> {
        let last: Option<i64> =
            sqlx::query_scalar("SELECT MAX(occurred_at) FROM login_failures WHERE source = ?")
                .bind(source)
                .fetch_one(&self.pool)
                .await?;
        Ok(last)
    }

    /// Drops failure rows older than a timestamp.
    pub async fn delete_failures_before(&self, cutoff: i64) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM login_failures WHERE occurred_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Deletes every credential, session, setup token, and the user account.
    pub async fn wipe_auth(&self) -> Result<(), StoreError> {
        let mut transaction = self.pool.begin().await?;
        for statement in [
            "DELETE FROM sessions",
            "DELETE FROM passkeys",
            "DELETE FROM setup_tokens",
            "DELETE FROM login_failures",
            "DELETE FROM users",
        ] {
            sqlx::query(statement).execute(&mut *transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
