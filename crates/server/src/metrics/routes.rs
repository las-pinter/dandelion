//! Live metrics endpoints.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use dandelion_sysmon::HistorySnapshot;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    app::ApiError,
    auth::{middleware::error_response, session::now_millis},
    state::AppState,
};

use super::{HOUR_MS, WEEK_MS};

/// Most points returned in one history response; the chart downsamples again.
const MAX_HISTORY_POINTS: usize = 720;

/// Query for the history endpoint.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Window to return: `1h` for second points, `7d` for minute aggregates.
    pub range: Option<String>,
}

/// The newest full snapshot.
#[utoipa::path(
    get,
    path = "/api/system/latest",
    tag = "system",
    responses(
        (status = 200, description = "The newest sample", body = dandelion_sysmon::Snapshot),
        (status = 503, description = "No sample yet", body = ApiError)
    )
)]
pub async fn latest(State(state): State<AppState>) -> Response {
    state.metrics.latest().map_or_else(
        || {
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "not_ready",
                "the first metrics sample has not arrived",
            )
        },
        |snapshot| Json(snapshot).into_response(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Range {
    Hour,
    Week,
}

impl Range {
    fn parse(value: Option<&str>) -> Option<Self> {
        match value {
            None | Some("1h") => Some(Self::Hour),
            Some("7d") => Some(Self::Week),
            Some(_) => None,
        }
    }
}

/// Stored history for one window.
#[utoipa::path(
    get,
    path = "/api/system/history",
    tag = "system",
    params(HistoryQuery),
    responses(
        (status = 200, description = "Stored points for the window", body = dandelion_sysmon::HistorySnapshot),
        (status = 400, description = "Unknown range", body = ApiError),
        (status = 500, description = "Storage failure", body = ApiError)
    )
)]
pub async fn history(State(state): State<AppState>, Query(query): Query<HistoryQuery>) -> Response {
    let Some(range) = Range::parse(query.range.as_deref()) else {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_range",
            "range must be 1h or 7d",
        );
    };

    let end = u64::try_from(now_millis()).unwrap_or(0).saturating_add(1);
    let store = state.metrics.store();
    let result = match range {
        Range::Hour => store
            .seconds_between(end.saturating_sub(HOUR_MS), end)
            .await
            .map(|seconds| HistorySnapshot {
                seconds: decimate(seconds, MAX_HISTORY_POINTS),
                minutes: Vec::new(),
            }),
        Range::Week => store
            .minutes_between(end.saturating_sub(WEEK_MS), end)
            .await
            .map(|minutes| HistorySnapshot {
                seconds: Vec::new(),
                minutes: decimate(minutes, MAX_HISTORY_POINTS),
            }),
    };

    result.map_or_else(
        |error| {
            tracing::error!(%error, "cannot read metrics history");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "an internal error occurred",
            )
        },
        |history| Json(history).into_response(),
    )
}

/// Routes mounted under `/api/system`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/latest", get(latest))
        .route("/history", get(history))
}

/// Keeps at most `max` evenly spaced rows, preserving order and the ends.
fn decimate<T>(items: Vec<T>, max: usize) -> Vec<T> {
    if max == 0 || items.len() <= max {
        return items;
    }
    let stride = items.len().div_ceil(max);
    items.into_iter().step_by(stride).collect()
}
