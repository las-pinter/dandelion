//! Push notification routes: the public key, subscribe, unsubscribe, and a
//! test message.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use super::{Notice, NotificationsError, PushSubscription};
use crate::{app::ApiError, audit::AuditEntry, auth::middleware::error_response, state::AppState};

/// The only account this build authenticates.
const ACTOR: &str = "owner";
/// Longest push endpoint accepted.
const MAX_ENDPOINT_BYTES: usize = 2048;
/// Size of the browser's public key.
const P256DH_BYTES: usize = 65;
/// Size of the browser's auth secret.
const AUTH_BYTES: usize = 16;

/// Routes mounted under `/api/notifications`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/key", get(key))
        .route("/subscribe", post(subscribe))
        .route("/unsubscribe", post(unsubscribe))
        .route("/test", post(test))
}

/// The VAPID public key a browser subscribes with.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PushKeyResponse {
    /// Base64url, unpadded.
    pub public_key: String,
}

/// The browser's half of the push encryption.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct PushKeys {
    /// The browser's public key, base64url.
    pub p256dh: String,
    /// The browser's shared secret, base64url.
    pub auth: String,
}

/// One browser subscription.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct PushSubscribeRequest {
    /// The push service URL for this browser.
    pub endpoint: String,
    /// The browser's keys.
    pub keys: PushKeys,
}

/// Removes one browser subscription.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct PushUnsubscribeRequest {
    /// The push service URL to forget.
    pub endpoint: String,
}

/// The outcome of a test notification.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PushTestResponse {
    /// How many subscribed devices accepted the notification.
    pub delivered: usize,
}

/// Returns the key this browser subscribes with.
#[utoipa::path(
    get,
    path = "/api/notifications/key",
    tag = "notifications",
    operation_id = "notifications_key",
    responses(
        (status = 200, description = "The VAPID public key", body = PushKeyResponse),
        (status = 503, description = "Push is not configured", body = ApiError)
    )
)]
pub async fn key(State(state): State<AppState>) -> Response {
    match state.notifications.public_key() {
        Ok(public_key) => Json(PushKeyResponse {
            public_key: public_key.to_owned(),
        })
        .into_response(),
        Err(error) => notifications_error(&error),
    }
}

/// Stores one browser subscription.
#[utoipa::path(
    post,
    path = "/api/notifications/subscribe",
    tag = "notifications",
    operation_id = "notifications_subscribe",
    request_body = PushSubscribeRequest,
    responses(
        (status = 204, description = "The subscription is stored"),
        (status = 400, description = "The subscription is not valid", body = ApiError),
        (status = 500, description = "The subscription could not be stored", body = ApiError),
        (status = 503, description = "Push is not configured", body = ApiError)
    )
)]
pub async fn subscribe(
    State(state): State<AppState>,
    Json(request): Json<PushSubscribeRequest>,
) -> Response {
    let valid = endpoint_ok(&request.endpoint).await
        && key_ok(&request.keys.p256dh, P256DH_BYTES)
        && key_ok(&request.keys.auth, AUTH_BYTES);
    if !valid {
        record(
            &state,
            "notifications.subscribe",
            json!({ "outcome": "invalid_subscription" }),
        )
        .await;
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_subscription",
            "the endpoint or keys are not a valid push subscription",
        );
    }

    let result = state
        .notifications
        .subscribe(PushSubscription {
            endpoint: request.endpoint,
            p256dh: request.keys.p256dh,
            auth: request.keys.auth,
        })
        .await;
    let outcome = match &result {
        Ok(()) => "ok".to_owned(),
        Err(error) => error_code(error).0.to_owned(),
    };
    record(
        &state,
        "notifications.subscribe",
        json!({ "outcome": outcome }),
    )
    .await;
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => notifications_error(&error),
    }
}

/// Forgets one browser subscription.
#[utoipa::path(
    post,
    path = "/api/notifications/unsubscribe",
    tag = "notifications",
    operation_id = "notifications_unsubscribe",
    request_body = PushUnsubscribeRequest,
    responses(
        (status = 204, description = "The subscription is gone"),
        (status = 400, description = "The endpoint is not valid", body = ApiError),
        (status = 500, description = "The subscription could not be removed", body = ApiError),
        (status = 503, description = "Push is not configured", body = ApiError)
    )
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Json(request): Json<PushUnsubscribeRequest>,
) -> Response {
    if !endpoint_shape_ok(&request.endpoint) {
        record(
            &state,
            "notifications.unsubscribe",
            json!({ "outcome": "invalid_subscription" }),
        )
        .await;
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_subscription",
            "the endpoint is not a valid push subscription",
        );
    }
    match state.notifications.unsubscribe(&request.endpoint).await {
        Ok(removed) => {
            record(
                &state,
                "notifications.unsubscribe",
                json!({ "outcome": "ok", "removed": removed }),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => {
            record(
                &state,
                "notifications.unsubscribe",
                json!({ "outcome": error_code(&error).0 }),
            )
            .await;
            notifications_error(&error)
        }
    }
}

/// Sends a test notification to every subscribed browser.
#[utoipa::path(
    post,
    path = "/api/notifications/test",
    tag = "notifications",
    operation_id = "notifications_test",
    responses(
        (status = 200, description = "The test was sent", body = PushTestResponse),
        (status = 503, description = "Push is not configured", body = ApiError)
    )
)]
pub async fn test(State(state): State<AppState>) -> Response {
    if !state.notifications.is_enabled() {
        record(
            &state,
            "notifications.test",
            json!({ "outcome": "unavailable" }),
        )
        .await;
        return notifications_error(&NotificationsError::Unavailable);
    }
    let delivered = state
        .notifications
        .send(&Notice::new(
            "Dandelion",
            "Notifications are working.",
            "test",
            "/dashboard",
        ))
        .await;
    record(
        &state,
        "notifications.test",
        json!({ "outcome": "ok", "delivered": delivered }),
    )
    .await;
    Json(PushTestResponse { delivered }).into_response()
}

fn endpoint_shape_ok(endpoint: &str) -> bool {
    endpoint.len() <= MAX_ENDPOINT_BYTES
        && url::Url::parse(endpoint).is_ok_and(|url| {
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.host().is_some()
        })
}

/// A subscription endpoint must not reach the machine or its tailnet. The
/// host is checked as a literal and, for a name, resolved once; a name that
/// does not resolve is accepted, so DNS rebinding remains possible.
async fn endpoint_ok(endpoint: &str) -> bool {
    if !endpoint_shape_ok(endpoint) {
        return false;
    }
    let Ok(url) = url::Url::parse(endpoint) else {
        return false;
    };
    let Some(host) = url.host() else {
        return false;
    };
    match host {
        url::Host::Domain(name) => domain_is_public(name).await,
        url::Host::Ipv4(address) => ipv4_is_global(address),
        url::Host::Ipv6(address) => ipv6_is_global(address),
    }
}

async fn domain_is_public(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    if name == "localhost" || name.ends_with(".localhost") {
        return false;
    }
    // RFC 2606 reserved names never resolve on the public internet; tests and
    // documentation use them.
    if name
        .rsplit('.')
        .next()
        .is_some_and(|tld| matches!(tld, "example" | "test" | "invalid"))
    {
        return true;
    }
    match tokio::net::lookup_host((name.as_str(), 443)).await {
        Ok(addresses) => {
            let mut resolved = false;
            for address in addresses {
                resolved = true;
                if !ip_is_global(address.ip()) {
                    return false;
                }
            }
            resolved
        }
        // An unresolvable name is kept: the push service is reached later,
        // and a test host has no DNS entry.
        Err(_) => true,
    }
}

fn ip_is_global(address: std::net::IpAddr) -> bool {
    match address {
        std::net::IpAddr::V4(address) => ipv4_is_global(address),
        std::net::IpAddr::V6(address) => ipv6_is_global(address),
    }
}

/// `Ipv4Addr::is_global` is still unstable, so the reserved ranges are listed
/// here: loopback, private, link-local, shared (CGNAT and Tailscale),
/// documentation, benchmarking, multicast, and the reserved top block.
fn ipv4_is_global(address: std::net::Ipv4Addr) -> bool {
    let octets = address.octets();
    !(address.is_loopback()
        || address.is_private()
        || address.is_link_local()
        || address.is_unspecified()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_multicast()
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

/// The IPv6 counterpart: no loopback, unspecified, multicast, documentation,
/// unique-local, link-local, deprecated site-local, 6to4, Teredo, or NAT64
/// address. IPv4-mapped addresses are judged as the IPv4 address they carry.
fn ipv6_is_global(address: std::net::Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return ipv4_is_global(mapped);
    }
    let segments = address.segments();
    let first = segments[0];
    !(address.is_loopback()
        || address.is_unspecified()
        || address.is_multicast()
        || (first == 0x2001 && segments[1] == 0x0db8)
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first & 0xffc0) == 0xfec0
        || first == 0x2002
        || (first == 0x2001 && segments[1] == 0)
        || (first == 0x0064 && segments[1] == 0xff9b))
}

fn key_ok(value: &str, bytes: usize) -> bool {
    URL_SAFE_NO_PAD
        .decode(value)
        .is_ok_and(|decoded| decoded.len() == bytes)
}

fn error_code(error: &NotificationsError) -> (&'static str, StatusCode) {
    match error {
        NotificationsError::Unavailable => ("unavailable", StatusCode::SERVICE_UNAVAILABLE),
        NotificationsError::Storage(_) => ("storage_error", StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn notifications_error(error: &NotificationsError) -> Response {
    let (code, status) = error_code(error);
    error_response(status, code, &error.to_string())
}

/// Appends one audit row; a failure is logged, never returned to the caller.
async fn record(state: &AppState, action: &str, detail: serde_json::Value) {
    let detail = detail.to_string();
    if let Err(error) = state
        .audit
        .record(AuditEntry {
            actor: ACTOR,
            action,
            // The endpoint is a capability URL and never belongs in the log.
            target: "push",
            detail: Some(&detail),
        })
        .await
    {
        tracing::error!(%error, "audit write failed");
    }
}
