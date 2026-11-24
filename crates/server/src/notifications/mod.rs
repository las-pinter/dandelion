//! Web push notifications.
//!
//! One VAPID key pair signs every message; one SQLite row per browser keeps
//! its subscription. A watcher turns agent, metric, and systemd transitions
//! into notices, and a notice goes to every stored subscription. An endpoint
//! that the push service reports gone is removed. The endpoint is a capability
//! URL, so it never reaches the audit log.

mod key;
pub mod routes;
mod store;
mod triggers;
mod watch;

pub use key::{KeyError, VapidKey};
pub use store::PushStore;
pub use triggers::{Resource, Triggers};
pub use watch::spawn_watchers;

use std::{fmt, path::Path, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use web_push::{
    ContentEncoding, HyperWebPushClient, SubscriptionInfo, Urgency, VapidSignature, WebPushClient,
    WebPushError, WebPushMessageBuilder,
};

/// How long a push service keeps an undelivered notification, in seconds.
const PUSH_TTL_SECS: u32 = 600;
/// How long one delivery may take before it counts as failed; a hung push
/// service must not stall the watcher or the other devices.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Consecutive failed deliveries after which a subscription is retired: a
/// dead endpoint, or a key the browser no longer holds, would otherwise be
/// retried on every notice forever.
const MAX_FAILURES: i64 = 10;

/// One browser subscription, as the browser sends it.
#[derive(Clone, PartialEq, Eq)]
pub struct PushSubscription {
    /// The push service URL for this browser.
    pub endpoint: String,
    /// The browser's public key, base64url.
    pub p256dh: String,
    /// The browser's shared secret, base64url.
    pub auth: String,
}

impl fmt::Debug for PushSubscription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PushSubscription")
            .field("endpoint", &"<capability url>")
            .field("p256dh", &"<redacted>")
            .field("auth", &"<redacted>")
            .finish()
    }
}

/// What one notification says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    /// Notification title.
    pub title: String,
    /// Notification body.
    pub body: String,
    /// Collapses repeats of the same fact in the browser.
    pub tag: String,
    /// In-app path to open when the notification is tapped.
    pub url: String,
}

impl Notice {
    /// Builds a notice.
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        body: impl Into<String>,
        tag: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            tag: tag.into(),
            url: url.into(),
        }
    }

    /// The JSON payload the service worker reads.
    ///
    /// # Errors
    ///
    /// Returns the serialization error, which cannot happen for these fields.
    pub fn payload(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Why a push could not be delivered.
#[derive(Debug, thiserror::Error)]
pub enum PushError {
    /// The push service says the endpoint no longer exists.
    #[error("the push endpoint is gone")]
    Gone,
    /// The push service refused or could not be reached.
    #[error(transparent)]
    WebPush(#[from] WebPushError),
}

/// Delivers one notice to one subscription.
#[async_trait]
pub trait PushSender: Send + Sync {
    /// Sends the notice; [`PushError::Gone`] asks the caller to drop the row.
    async fn send(&self, subscription: &PushSubscription, notice: &Notice)
    -> Result<(), PushError>;
}

/// Sends through the browser's push service, signing with VAPID.
#[derive(Clone)]
pub struct WebPushSender {
    key: VapidKey,
    client: HyperWebPushClient,
    /// The VAPID `sub` contact URI.
    subject: String,
}

impl fmt::Debug for WebPushSender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebPushSender")
            .finish_non_exhaustive()
    }
}

impl WebPushSender {
    /// Builds a sender over one VAPID key.
    #[must_use]
    pub fn new(key: VapidKey, subject: impl Into<String>) -> Self {
        Self {
            key,
            client: HyperWebPushClient::new(),
            subject: subject.into(),
        }
    }
}

#[async_trait]
impl PushSender for WebPushSender {
    async fn send(
        &self,
        subscription: &PushSubscription,
        notice: &Notice,
    ) -> Result<(), PushError> {
        let info = SubscriptionInfo::new(
            &subscription.endpoint,
            &subscription.p256dh,
            &subscription.auth,
        );
        let payload = notice.payload().map_err(|_| WebPushError::Unspecified)?;
        let mut builder = WebPushMessageBuilder::new(&info);
        builder.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
        builder.set_ttl(PUSH_TTL_SECS);
        builder.set_urgency(Urgency::Normal);
        builder.set_vapid_signature(self.signature(&info)?);
        let message = builder.build()?;
        match WebPushClient::send(&self.client, message).await {
            Ok(()) => Ok(()),
            Err(WebPushError::EndpointNotValid(_) | WebPushError::EndpointNotFound(_)) => {
                Err(PushError::Gone)
            }
            Err(error) => Err(PushError::WebPush(error)),
        }
    }
}

impl WebPushSender {
    fn signature(&self, info: &SubscriptionInfo) -> Result<VapidSignature, WebPushError> {
        self.key.signature(info, &self.subject)
    }
}

/// Why a notification request failed.
#[derive(Debug, thiserror::Error)]
pub enum NotificationsError {
    /// This server has no VAPID key or no database.
    #[error("push notifications are not configured on this server")]
    Unavailable,
    /// The database refused the operation.
    #[error(transparent)]
    Storage(#[from] sqlx::Error),
}

/// Push subscriptions and delivery.
#[derive(Clone)]
pub struct Notifications {
    inner: Option<Arc<Inner>>,
}

struct Inner {
    store: PushStore,
    sender: Arc<dyn PushSender>,
    public_key: String,
}

impl fmt::Debug for Notifications {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Notifications")
            .field("enabled", &self.inner.is_some())
            .finish()
    }
}

impl Notifications {
    /// Loads or creates the VAPID key and prepares delivery.
    ///
    /// A key that cannot be read or written disables push with a warning
    /// instead of failing the server. `public_origin` becomes the VAPID `sub`
    /// contact URI, which push services use for abuse reports.
    #[must_use]
    pub fn boot(data_dir: &Path, pool: SqlitePool, public_origin: &str) -> Self {
        let path = data_dir.join("vapid.json");
        match VapidKey::load_or_create(&path) {
            Ok(key) => {
                tracing::info!(path = %path.display(), "push notifications enabled");
                Self::with_sender(
                    PushStore::new(pool),
                    Arc::new(WebPushSender::new(
                        key.clone(),
                        subject_for_origin(public_origin),
                    )),
                    key.public_key().to_owned(),
                )
            }
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "push notifications are disabled");
                Self::disabled()
            }
        }
    }

    /// Uses an explicit store, sender, and public key; tests inject all three.
    #[must_use]
    pub fn with_sender(store: PushStore, sender: Arc<dyn PushSender>, public_key: String) -> Self {
        Self {
            inner: Some(Arc::new(Inner {
                store,
                sender,
                public_key,
            })),
        }
    }

    /// A service with no key, for builds and tests that do not push.
    #[must_use]
    pub fn disabled() -> Self {
        Self { inner: None }
    }

    /// Whether pushes can be sent.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    /// The public key a browser subscribes with.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationsError::Unavailable`] when the service is off.
    pub fn public_key(&self) -> Result<&str, NotificationsError> {
        Ok(&self
            .inner
            .as_ref()
            .ok_or(NotificationsError::Unavailable)?
            .public_key)
    }

    /// Stores or refreshes one browser subscription.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationsError`] when the service is off or the write
    /// fails.
    pub async fn subscribe(
        &self,
        subscription: PushSubscription,
    ) -> Result<(), NotificationsError> {
        let inner = self.inner.as_ref().ok_or(NotificationsError::Unavailable)?;
        inner
            .store
            .upsert(&subscription, crate::auth::session::now_millis())
            .await?;
        Ok(())
    }

    /// Removes one browser subscription.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationsError`] when the service is off or the delete
    /// fails.
    pub async fn unsubscribe(&self, endpoint: &str) -> Result<bool, NotificationsError> {
        let inner = self.inner.as_ref().ok_or(NotificationsError::Unavailable)?;
        Ok(inner.store.remove(endpoint).await?)
    }

    /// Sends one notice to every stored subscription; returns how many were
    /// delivered.
    ///
    /// A gone endpoint is removed. Other failures are logged and counted.
    pub async fn send(&self, notice: &Notice) -> usize {
        let Some(inner) = &self.inner else {
            return 0;
        };
        let subscriptions = match inner.store.list().await {
            Ok(subscriptions) => subscriptions,
            Err(error) => {
                tracing::error!(%error, "cannot read push subscriptions");
                return 0;
            }
        };
        let now = crate::auth::session::now_millis();
        // Devices deliver in parallel, so one slow endpoint costs one timeout
        // rather than blocking every other device.
        let deliveries = subscriptions.into_iter().map(|subscription| {
            let sender = Arc::clone(&inner.sender);
            async move {
                let result =
                    tokio::time::timeout(DELIVERY_TIMEOUT, sender.send(&subscription, notice))
                        .await;
                (subscription, result)
            }
        });
        let mut delivered = 0;
        for (subscription, result) in futures_util::future::join_all(deliveries).await {
            match result {
                Ok(Ok(())) => {
                    delivered += 1;
                    if let Err(error) = inner
                        .store
                        .record_success(&subscription.endpoint, now)
                        .await
                    {
                        tracing::warn!(%error, "cannot note a delivered push");
                    }
                }
                Ok(Err(PushError::Gone)) => {
                    tracing::info!("removing a push subscription the service reported gone");
                    if let Err(error) = inner.store.remove(&subscription.endpoint).await {
                        tracing::warn!(%error, "cannot remove a gone push subscription");
                    }
                }
                Ok(Err(error)) => {
                    // A payload or key the browser rejects is a problem with
                    // this notice, not evidence the subscription is dead.
                    match error {
                        PushError::WebPush(
                            WebPushError::PayloadTooLarge | WebPushError::InvalidCryptoKeys,
                        ) => {
                            tracing::warn!(
                                %error,
                                "a push payload or key was rejected; the subscription is kept"
                            );
                        }
                        error => {
                            tracing::warn!(%error, "push delivery failed");
                            note_failure(inner, &subscription.endpoint).await;
                        }
                    }
                }
                Err(_) => {
                    tracing::warn!("a push endpoint did not answer in time");
                    note_failure(inner, &subscription.endpoint).await;
                }
            }
        }
        delivered
    }

    /// The store, when notifications are enabled.
    #[must_use]
    pub fn store(&self) -> Option<&PushStore> {
        self.inner.as_ref().map(|inner| &inner.store)
    }
}

/// VAPID's `sub` is a contact URI. The public origin is an HTTPS contact; a
/// plain-HTTP loopback development origin falls back to a local mailto.
fn subject_for_origin(origin: &str) -> String {
    if origin.starts_with("https://") {
        origin.to_owned()
    } else {
        "mailto:dandelion@localhost".to_owned()
    }
}

/// Counts one failed delivery and retires the subscription when it keeps
/// failing.
async fn note_failure(inner: &Inner, endpoint: &str) {
    match inner.store.record_failure(endpoint).await {
        Ok(count) if count >= MAX_FAILURES => {
            tracing::warn!(
                failures = count,
                "retiring a push subscription that keeps failing"
            );
            if let Err(error) = inner.store.remove(endpoint).await {
                tracing::warn!(%error, "cannot remove a failing push subscription");
            }
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "cannot note a failed push"),
    }
}
