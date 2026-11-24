//! Push notification routes, storage, audit, and delivery.
//!
//! A fake sender records what would go to the push service, so the tests
//! cover the validation, the audit rows, and the endpoint retirement without
//! a browser.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dandelion_server::{
    agents::Agents,
    app::build_router,
    audit::AuditLog,
    auth::{
        Auth,
        origin::PublicOrigin,
        session::now_millis,
        token::{hash_token, random_token},
    },
    claude::ClaudeMonitor,
    containers::Containers,
    metrics::Metrics,
    notifications::{Notice, Notifications, PushError, PushSender, PushStore, PushSubscription},
    services::Services,
    state::AppState,
    storage,
    ws::hub::Hub,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use web_push::WebPushError;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";
const ENDPOINT: &str = "https://push.example/send/abc";

#[derive(Debug, Clone, Copy)]
enum FakeFailure {
    Generic,
    PayloadTooLarge,
    InvalidCryptoKeys,
}

#[derive(Debug, Default)]
struct FakeSender {
    sent: Mutex<Vec<(PushSubscription, Notice)>>,
    gone: bool,
    failures: Mutex<VecDeque<FakeFailure>>,
}

#[async_trait]
impl PushSender for FakeSender {
    async fn send(
        &self,
        subscription: &PushSubscription,
        notice: &Notice,
    ) -> Result<(), PushError> {
        if self.gone {
            return Err(PushError::Gone);
        }
        if let Some(failure) = self.failures.lock().expect("failures").pop_front() {
            return Err(PushError::WebPush(match failure {
                FakeFailure::Generic => WebPushError::Unspecified,
                FakeFailure::PayloadTooLarge => WebPushError::PayloadTooLarge,
                FakeFailure::InvalidCryptoKeys => WebPushError::InvalidCryptoKeys,
            }));
        }
        self.sent
            .lock()
            .expect("sent")
            .push((subscription.clone(), notice.clone()));
        Ok(())
    }
}

struct Harness {
    state: AppState,
    sender: Arc<FakeSender>,
    cookie: String,
}

async fn harness() -> Harness {
    harness_with(Arc::new(FakeSender::default())).await
}

async fn harness_with(sender: Arc<FakeSender>) -> Harness {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");
    let notifications = Notifications::with_sender(
        PushStore::new(pool.clone()),
        Arc::clone(&sender) as Arc<dyn PushSender>,
        "test-public-key".to_owned(),
    );
    let state = AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        Hub::default(),
        Metrics::new(pool),
        Services::disabled(),
        Containers::disabled(),
        Agents::disabled(),
        ClaudeMonitor::disabled(),
        notifications,
    );

    let now = now_millis();
    let owner = state.auth.store().ensure_owner(now).await.expect("owner");
    let token = random_token().expect("session token");
    state
        .auth
        .store()
        .insert_session(&hash_token(&token), &owner.id, now, now + 3_600_000)
        .await
        .expect("session");
    let cookie = format!("{COOKIE}={token}");

    Harness {
        state,
        sender,
        cookie,
    }
}

async fn send(
    state: &AppState,
    method: Method,
    uri: &str,
    body: Option<&Value>,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> Response {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    let body = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    build_router(state.clone())
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response")
}

async fn json_body(response: Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON body")
}

fn subscription_body() -> Value {
    json!({
        "endpoint": ENDPOINT,
        "keys": {
            "p256dh": URL_SAFE_NO_PAD.encode([7u8; 65]),
            "auth": URL_SAFE_NO_PAD.encode([9u8; 16]),
        }
    })
}

async fn subscribe(harness: &Harness) -> Response {
    send(
        &harness.state,
        Method::POST,
        "/api/notifications/subscribe",
        Some(&subscription_body()),
        Some(&harness.cookie),
        Some(ORIGIN),
    )
    .await
}

async fn stored(state: &AppState) -> Vec<PushSubscription> {
    state
        .notifications
        .store()
        .expect("notifications are enabled")
        .list()
        .await
        .expect("stored subscriptions")
}

#[tokio::test]
async fn notifications_require_a_session() {
    let harness = harness().await;

    let response = send(
        &harness.state,
        Method::GET,
        "/api/notifications/key",
        None,
        None,
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_public_key_is_served() {
    let harness = harness().await;

    let response = send(
        &harness.state,
        Method::GET,
        "/api/notifications/key",
        None,
        Some(&harness.cookie),
        None,
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["publicKey"], "test-public-key");
}

#[tokio::test]
async fn subscribe_stores_one_row_and_audits() {
    let harness = harness().await;

    let response = subscribe(&harness).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let stored = stored(&harness.state).await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].endpoint, ENDPOINT);

    let records = harness.state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action, "notifications.subscribe");
    assert_eq!(records[0].target, "push");
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail");
    assert_eq!(detail["outcome"], "ok");
    assert!(
        !records[0].target.contains("push.example"),
        "the endpoint is a capability URL and stays out of the log"
    );
}

#[tokio::test]
async fn an_invalid_subscription_is_refused_and_audited() {
    let harness = harness().await;
    let body = json!({
        "endpoint": "http://push.example/send/abc",
        "keys": { "p256dh": "short", "auth": "short" }
    });

    let response = send(
        &harness.state,
        Method::POST,
        "/api/notifications/subscribe",
        Some(&body),
        Some(&harness.cookie),
        Some(ORIGIN),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(stored(&harness.state).await.len(), 0);
    let records = harness.state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records[0].action, "notifications.subscribe");
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail");
    assert_eq!(detail["outcome"], "invalid_subscription");
}

#[tokio::test]
async fn a_local_or_private_endpoint_is_refused() {
    let harness = harness().await;

    for endpoint in [
        "https://127.0.0.1/send/abc",
        "https://[::1]/send/abc",
        "https://[::ffff:127.0.0.1]/send/abc",
        "https://[fc00::1]/send/abc",
        "https://[fe80::1]/send/abc",
        "https://[64:ff9b::7f00:1]/send/abc",
        "https://[2002:7f00:1::]/send/abc",
        "https://[::ffff:169.254.169.254]/send/abc",
        "https://10.1.2.3/send/abc",
        "https://100.64.0.1/send/abc",
        "https://100.100.1.1/send/abc",
        "https://[::ffff:100.100.1.1]/send/abc",
        "https://192.168.1.10/send/abc",
        "https://169.254.169.254/latest",
        "https://localhost/send/abc",
        "https://push.localhost/send/abc",
        "https://:secret@push.example/send/abc",
    ] {
        let mut body = subscription_body();
        body["endpoint"] = json!(endpoint);

        let response = send(
            &harness.state,
            Method::POST,
            "/api/notifications/subscribe",
            Some(&body),
            Some(&harness.cookie),
            Some(ORIGIN),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{endpoint} must not become a local request proxy"
        );
    }
    assert_eq!(stored(&harness.state).await.len(), 0);
}

#[tokio::test]
async fn unsubscribe_removes_the_row_and_audits() {
    let harness = harness().await;
    assert_eq!(subscribe(&harness).await.status(), StatusCode::NO_CONTENT);

    let response = send(
        &harness.state,
        Method::POST,
        "/api/notifications/unsubscribe",
        Some(&json!({ "endpoint": ENDPOINT })),
        Some(&harness.cookie),
        Some(ORIGIN),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(stored(&harness.state).await.len(), 0);
    let records = harness.state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records[0].action, "notifications.unsubscribe");
    let detail: Value =
        serde_json::from_str(records[0].detail.as_deref().expect("detail")).expect("detail");
    assert_eq!(detail["removed"], true);
}

#[tokio::test]
async fn a_subscription_is_retired_after_ten_failures() {
    let harness = harness().await;
    assert_eq!(subscribe(&harness).await.status(), StatusCode::NO_CONTENT);
    let notice = Notice::new("Dandelion", "test", "test", "/dashboard");

    for _ in 0..9 {
        harness
            .sender
            .failures
            .lock()
            .expect("failures")
            .push_back(FakeFailure::Generic);
        harness.state.notifications.send(&notice).await;
    }
    assert_eq!(
        stored(&harness.state).await.len(),
        1,
        "nine failures keep it"
    );

    harness
        .sender
        .failures
        .lock()
        .expect("failures")
        .push_back(FakeFailure::Generic);
    harness.state.notifications.send(&notice).await;
    assert_eq!(
        stored(&harness.state).await.len(),
        0,
        "the tenth failure retires it"
    );
}

#[tokio::test]
async fn a_payload_or_key_failure_does_not_retire_a_subscription() {
    let harness = harness().await;
    assert_eq!(subscribe(&harness).await.status(), StatusCode::NO_CONTENT);
    let notice = Notice::new("Dandelion", "test", "test", "/dashboard");

    for failure in std::iter::repeat_n(FakeFailure::PayloadTooLarge, 6)
        .chain(std::iter::repeat_n(FakeFailure::InvalidCryptoKeys, 6))
    {
        harness
            .sender
            .failures
            .lock()
            .expect("failures")
            .push_back(failure);
        harness.state.notifications.send(&notice).await;
    }

    assert_eq!(
        stored(&harness.state).await.len(),
        1,
        "content failures are not counted against the device"
    );
}

#[test]
fn a_subscription_debug_hides_the_capability_url() {
    let subscription = PushSubscription {
        endpoint: ENDPOINT.to_owned(),
        p256dh: "browser-key".to_owned(),
        auth: "browser-secret".to_owned(),
    };

    let printed = format!("{subscription:?}");
    assert!(!printed.contains("push.example"));
    assert!(!printed.contains("browser-key"));
    assert!(!printed.contains("browser-secret"));
}

#[tokio::test]
async fn the_test_endpoint_pushes_to_every_subscription() {
    let harness = harness().await;
    assert_eq!(subscribe(&harness).await.status(), StatusCode::NO_CONTENT);

    let response = send(
        &harness.state,
        Method::POST,
        "/api/notifications/test",
        None,
        Some(&harness.cookie),
        Some(ORIGIN),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["delivered"], 1);
    {
        let sent = harness.sender.sent.lock().expect("sent");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0.endpoint, ENDPOINT);
        assert_eq!(sent[0].1.title, "Dandelion");
        assert_eq!(sent[0].1.tag, "test");
    }

    let records = harness.state.audit.recent(10).await.expect("audit rows");
    assert_eq!(records[0].action, "notifications.test");
}

#[tokio::test]
async fn a_test_with_no_subscription_reports_zero() {
    let harness = harness().await;

    let response = send(
        &harness.state,
        Method::POST,
        "/api/notifications/test",
        None,
        Some(&harness.cookie),
        Some(ORIGIN),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["delivered"], 0);
}

#[tokio::test]
async fn a_gone_endpoint_is_retired() {
    let harness = harness_with(Arc::new(FakeSender {
        gone: true,
        ..FakeSender::default()
    }))
    .await;
    assert_eq!(subscribe(&harness).await.status(), StatusCode::NO_CONTENT);

    let response = send(
        &harness.state,
        Method::POST,
        "/api/notifications/test",
        None,
        Some(&harness.cookie),
        Some(ORIGIN),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["delivered"], 0, "the gone endpoint accepts nothing");
    assert_eq!(
        stored(&harness.state).await.len(),
        0,
        "the gone endpoint is removed"
    );
}

#[tokio::test]
async fn a_mutating_request_without_an_origin_is_refused() {
    let harness = harness().await;

    let response = send(
        &harness.state,
        Method::POST,
        "/api/notifications/subscribe",
        Some(&subscription_body()),
        Some(&harness.cookie),
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(stored(&harness.state).await.len(), 0);
    assert_eq!(harness.state.audit.recent(10).await.expect("rows").len(), 0);
}
