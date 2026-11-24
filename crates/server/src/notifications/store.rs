//! Persistence for push subscriptions.

use sqlx::{Row, SqlitePool};

use super::PushSubscription;

/// The push subscriptions one browser each.
#[derive(Debug, Clone)]
pub struct PushStore {
    pool: SqlitePool,
}

impl PushStore {
    /// Wraps the shared database pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Adds a subscription, or refreshes its keys when the endpoint is known.
    ///
    /// # Errors
    ///
    /// Returns the database error.
    pub async fn upsert(
        &self,
        subscription: &PushSubscription,
        now_ms: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO push_subscriptions (endpoint, p256dh, auth, created_ms)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(endpoint) DO UPDATE SET
                 p256dh = excluded.p256dh,
                 auth = excluded.auth,
                 failure_count = 0,
                 last_success_ms = NULL",
        )
        .bind(&subscription.endpoint)
        .bind(&subscription.p256dh)
        .bind(&subscription.auth)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Removes one subscription; returns whether a row was deleted.
    ///
    /// # Errors
    ///
    /// Returns the database error.
    pub async fn remove(&self, endpoint: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
            .bind(endpoint)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Every stored subscription.
    ///
    /// # Errors
    ///
    /// Returns the database error.
    pub async fn list(&self) -> Result<Vec<PushSubscription>, sqlx::Error> {
        let rows = sqlx::query("SELECT endpoint, p256dh, auth FROM push_subscriptions")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| PushSubscription {
                endpoint: row.get("endpoint"),
                p256dh: row.get("p256dh"),
                auth: row.get("auth"),
            })
            .collect())
    }

    /// Notes a delivered notification.
    ///
    /// # Errors
    ///
    /// Returns the database error.
    pub async fn record_success(&self, endpoint: &str, now_ms: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE push_subscriptions SET last_success_ms = ?, failure_count = 0
             WHERE endpoint = ?",
        )
        .bind(now_ms)
        .bind(endpoint)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Notes a failed delivery that did not retire the endpoint; returns the
    /// consecutive failure count.
    ///
    /// # Errors
    ///
    /// Returns the database error.
    pub async fn record_failure(&self, endpoint: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "UPDATE push_subscriptions SET failure_count = failure_count + 1
             WHERE endpoint = ?
             RETURNING failure_count",
        )
        .bind(endpoint)
        .fetch_one(&self.pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::{notifications::PushSubscription, storage};

    use super::PushStore;

    fn subscription(endpoint: &str) -> PushSubscription {
        PushSubscription {
            endpoint: endpoint.to_owned(),
            p256dh: "p".to_owned(),
            auth: "a".to_owned(),
        }
    }

    #[tokio::test]
    async fn upsert_list_and_remove() -> Result<(), Box<dyn std::error::Error>> {
        let pool = storage::open_in_memory().await?;
        let store = PushStore::new(pool);

        store
            .upsert(&subscription("https://push.example/1"), 1)
            .await?;
        store
            .upsert(&subscription("https://push.example/1"), 2)
            .await?;
        store
            .upsert(&subscription("https://push.example/2"), 3)
            .await?;
        assert_eq!(store.list().await?.len(), 2);

        assert!(store.remove("https://push.example/1").await?);
        assert!(!store.remove("https://push.example/1").await?);
        assert_eq!(store.list().await?.len(), 1);

        assert_eq!(store.record_failure("https://push.example/2").await?, 1);
        assert_eq!(store.record_failure("https://push.example/2").await?, 2);
        store.record_success("https://push.example/2", 4).await?;
        assert_eq!(
            store.record_failure("https://push.example/2").await?,
            1,
            "a delivery resets the failure count"
        );
        store
            .upsert(&subscription("https://push.example/2"), 5)
            .await?;
        assert_eq!(
            store.record_failure("https://push.example/2").await?,
            1,
            "a fresh subscription starts with no failures"
        );

        Ok(())
    }
}
