//! Failed-login throttling.
//!
//! Five failures from one source inside fifteen minutes start an exponential
//! backoff, capped at the window length. There is no permanent lockout; the
//! rows age out.

use std::time::Duration;

use super::store::{AuthStore, StoreError};

/// Failures are counted over this window.
pub const WINDOW_MS: i64 = 15 * 60 * 1000;
/// The first failure that triggers a delay.
pub const MAX_FAILURES: i64 = 5;
/// The backoff cap.
pub const MAX_BACKOFF_MS: i64 = WINDOW_MS;
/// Failure rows older than this are deleted.
const RETENTION_MS: i64 = 24 * 60 * 60 * 1000;

/// Per-source failure accounting.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    store: AuthStore,
}

impl RateLimiter {
    /// Wraps the auth store.
    #[must_use]
    pub const fn new(store: AuthStore) -> Self {
        Self { store }
    }

    /// Records a failure and prunes old rows.
    pub async fn record_failure(&self, source: &str, now: i64) -> Result<(), StoreError> {
        self.store.record_failure(source, now).await?;
        self.store
            .delete_failures_before(now - RETENTION_MS)
            .await?;
        Ok(())
    }

    /// Returns how long the source must wait, if it is throttled.
    pub async fn retry_after(
        &self,
        source: &str,
        now: i64,
    ) -> Result<Option<Duration>, StoreError> {
        let failures = self.store.failures_since(source, now - WINDOW_MS).await?;
        let Some(delay) = backoff(failures) else {
            return Ok(None);
        };
        let Some(last) = self.store.last_failure(source).await? else {
            return Ok(None);
        };
        let waited = (now - last).max(0);
        if waited >= delay {
            return Ok(None);
        }
        Ok(Some(Duration::from_millis(
            u64::try_from(delay - waited).unwrap_or_default(),
        )))
    }
}

/// Exponential delay after a failure count; `None` below the threshold.
#[must_use]
pub fn backoff(failures: i64) -> Option<i64> {
    if failures < MAX_FAILURES {
        return None;
    }
    let step = failures - MAX_FAILURES;
    let factor = 1_i64
        .checked_shl(u32::try_from(step).unwrap_or_default())
        .unwrap_or(i64::MAX);
    Some(60_000_i64.saturating_mul(factor).min(MAX_BACKOFF_MS))
}

#[cfg(test)]
mod tests {
    use super::{RateLimiter, backoff};
    use crate::{auth::store::AuthStore, storage::open_in_memory};

    async fn limiter() -> RateLimiter {
        RateLimiter::new(AuthStore::new(
            open_in_memory().await.expect("in-memory database"),
        ))
    }

    #[test]
    fn backoff_starts_at_five_and_doubles() {
        assert_eq!(backoff(0), None);
        assert_eq!(backoff(4), None);
        assert_eq!(backoff(5), Some(60_000));
        assert_eq!(backoff(6), Some(120_000));
        assert_eq!(backoff(7), Some(240_000));
        assert_eq!(backoff(20), Some(900_000));
    }

    #[tokio::test]
    async fn five_failures_throttle_for_a_minute() {
        let limiter = limiter().await;
        let mut now = 1_000_000;

        for _ in 0..5 {
            limiter.record_failure("local", now).await.expect("record");
            now += 1;
        }

        let wait = limiter
            .retry_after("local", now)
            .await
            .expect("read")
            .expect("throttled");
        assert!(wait.as_millis() > 0);
        assert!(wait.as_millis() <= 60_000);
    }

    #[tokio::test]
    async fn the_delay_expires_without_permanent_lockout() {
        let limiter = limiter().await;
        let mut now = 1_000_000;
        for _ in 0..5 {
            limiter.record_failure("local", now).await.expect("record");
            now += 1;
        }

        assert!(
            limiter
                .retry_after("local", now + 60_000)
                .await
                .expect("read")
                .is_none()
        );
    }

    #[tokio::test]
    async fn sources_are_isolated() {
        let limiter = limiter().await;
        for _ in 0..5 {
            limiter.record_failure("a", 1_000).await.expect("record");
        }

        assert!(
            limiter
                .retry_after("a", 1_000)
                .await
                .expect("read")
                .is_some()
        );
        assert!(
            limiter
                .retry_after("b", 1_000)
                .await
                .expect("read")
                .is_none()
        );
    }

    #[tokio::test]
    async fn old_failures_fall_out_of_the_window() {
        let limiter = limiter().await;
        for _ in 0..5 {
            limiter.record_failure("a", 1_000).await.expect("record");
        }

        assert!(
            limiter
                .retry_after("a", 1_000 + super::WINDOW_MS + 1)
                .await
                .expect("read")
                .is_none()
        );
    }
}
