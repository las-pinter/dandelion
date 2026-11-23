//! Metrics endpoints and collector-ingest acceptance tests.

use std::time::Duration;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use dandelion_server::{
    app::build_router,
    audit::AuditLog,
    auth::{
        Auth,
        origin::PublicOrigin,
        session::now_millis,
        token::{hash_token, random_token},
    },
    metrics::{self, Metrics, TOPIC},
    services::Services,
    state::AppState,
    storage,
    ws::hub::Hub,
};
use dandelion_sysmon::{Cpu, LoadAverage, Memory, MetricsPoint, MinuteAggregate, Snapshot};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";

async fn test_state() -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        Hub::default(),
        Metrics::new(pool),
        Services::disabled(),
    )
}

async fn session_cookie(state: &AppState) -> String {
    let now = now_millis();
    let owner = state.auth.store().ensure_owner(now).await.expect("owner");
    let token = random_token().expect("session token");
    state
        .auth
        .store()
        .insert_session(&hash_token(&token), &owner.id, now, now + 3_600_000)
        .await
        .expect("session");

    format!("{COOKIE}={token}")
}

async fn get(state: &AppState, uri: &str, cookie: Option<&str>) -> Response {
    let mut builder = Request::builder().method(Method::GET).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    build_router(state.clone())
        .oneshot(builder.body(Body::empty()).expect("request"))
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

fn snapshot(timestamp_ms: u64, cpu_percent: f32) -> Snapshot {
    Snapshot {
        timestamp_ms,
        uptime_secs: 100.0,
        cpu: Cpu {
            usage_percent: cpu_percent,
            per_core_percent: vec![cpu_percent],
            load: LoadAverage {
                one: 0.5,
                five: 0.4,
                fifteen: 0.3,
            },
        },
        memory: Memory {
            total_bytes: 1_000,
            used_bytes: 400,
            ..Memory::default()
        },
        ..Snapshot::default()
    }
}

#[tokio::test]
async fn metrics_require_a_session() {
    let state = test_state().await;

    let response = get(&state, "/api/system/latest", None).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn latest_reports_not_ready_before_the_first_sample() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let response = get(&state, "/api/system/latest", Some(&cookie)).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "not_ready");
}

#[tokio::test]
async fn latest_returns_the_newest_snapshot() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    state.metrics.set_latest(snapshot(1_700_000_000_000, 42.5));

    let response = get(&state, "/api/system/latest", Some(&cookie)).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["timestamp_ms"], 1_700_000_000_000_u64);
    assert_eq!(body["cpu"]["usage_percent"], 42.5);
}

#[tokio::test]
async fn history_returns_seconds_for_an_hour() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let now = u64::try_from(now_millis()).expect("positive clock");
    state
        .metrics
        .store()
        .insert_point(&MetricsPoint {
            timestamp_ms: now - 1_000,
            cpu_percent: 12.0,
            ..MetricsPoint::default()
        })
        .await
        .expect("insert");

    let response = get(&state, "/api/system/history?range=1h", Some(&cookie)).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["seconds"].as_array().expect("seconds").len(), 1);
    assert_eq!(body["seconds"][0]["timestamp_ms"], now - 1_000);
    assert_eq!(body["minutes"], serde_json::json!([]));
}

#[tokio::test]
async fn history_returns_minutes_for_a_week() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let now = u64::try_from(now_millis()).expect("positive clock");
    state
        .metrics
        .store()
        .insert_minute(&MinuteAggregate {
            minute_start_ms: now - 60_000,
            sample_count: 60,
            ..MinuteAggregate::default()
        })
        .await
        .expect("insert");

    let response = get(&state, "/api/system/history?range=7d", Some(&cookie)).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["seconds"], serde_json::json!([]));
    assert_eq!(body["minutes"].as_array().expect("minutes").len(), 1);
    assert_eq!(body["minutes"][0]["minute_start_ms"], now - 60_000);
}

#[tokio::test]
async fn history_defaults_to_the_hour() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let now = u64::try_from(now_millis()).expect("positive clock");
    state
        .metrics
        .store()
        .insert_point(&MetricsPoint {
            timestamp_ms: now - 1_000,
            cpu_percent: 12.0,
            ..MetricsPoint::default()
        })
        .await
        .expect("insert");

    let response = get(&state, "/api/system/history", Some(&cookie)).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["seconds"].as_array().expect("seconds").len(), 1);
    assert_eq!(body["minutes"], serde_json::json!([]));
}

#[tokio::test]
async fn history_caps_a_long_window() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;
    let now = u64::try_from(now_millis()).expect("positive clock");
    let store = state.metrics.store();
    for index in 0..1_000_u64 {
        store
            .insert_point(&MetricsPoint {
                timestamp_ms: now - (999 - index) * 1_000,
                cpu_percent: 1.0,
                ..MetricsPoint::default()
            })
            .await
            .expect("insert");
    }

    let response = get(&state, "/api/system/history?range=1h", Some(&cookie)).await;
    let status = response.status();
    let body = json_body(response).await;
    let seconds = body["seconds"].as_array().expect("seconds");

    assert_eq!(status, StatusCode::OK);
    assert!(seconds.len() <= 720, "{} points", seconds.len());
    assert!(
        seconds[0]["timestamp_ms"].as_u64().expect("first")
            < seconds.last().expect("last")["timestamp_ms"]
                .as_u64()
                .expect("last")
    );
}

#[tokio::test]
async fn history_rejects_an_unknown_range() {
    let state = test_state().await;
    let cookie = session_cookie(&state).await;

    let response = get(&state, "/api/system/history?range=3d", Some(&cookie)).await;
    let status = response.status();
    let body = json_body(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_range");
}

#[tokio::test]
async fn ingest_publishes_persists_and_records_the_latest_snapshot() {
    let state = test_state().await;
    let mut events = state.hub.subscribe(TOPIC);

    metrics::collector::ingest(&state, snapshot(1_000, 12.5)).await;

    let event = events.try_recv().expect("a live event");
    assert_eq!(event["snapshot"]["cpu"]["usage_percent"], 12.5);
    assert_eq!(event["point"]["cpu_percent"], 12.5);

    let latest = state.metrics.latest().expect("the latest snapshot");
    assert_eq!(latest.timestamp_ms, 1_000);

    let points = state
        .metrics
        .store()
        .seconds_between(0, 2_000)
        .await
        .expect("stored points");
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].timestamp_ms, 1_000);
}

#[tokio::test]
async fn the_collector_publishes_a_live_sample() {
    let state = test_state().await;
    let mut events = state.hub.subscribe(TOPIC);

    metrics::collector::spawn(state.clone());

    let event = tokio::time::timeout(Duration::from_secs(3), events.recv())
        .await
        .expect("a sample within three seconds")
        .expect("a live event");
    assert!(event["snapshot"]["cpu"]["usage_percent"].is_number());
    assert!(event["point"]["timestamp_ms"].is_number());
    assert!(state.metrics.latest().is_some());
}

#[tokio::test]
async fn ingest_persists_a_completed_minute_once() {
    let state = test_state().await;

    for timestamp in [0, 30_000, 60_000, 61_000] {
        metrics::collector::ingest(&state, snapshot(timestamp, 10.0)).await;
    }

    let minutes = state
        .metrics
        .store()
        .minutes_between(0, 120_000)
        .await
        .expect("stored minutes");
    assert_eq!(minutes.len(), 1);
    assert_eq!(minutes[0].minute_start_ms, 0);
    assert_eq!(minutes[0].sample_count, 2);
}
