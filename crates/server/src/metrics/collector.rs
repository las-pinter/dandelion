//! The live collector task: sample once per second, persist, and publish.

use dandelion_sysmon::{Collector, Snapshot};
use tokio::sync::mpsc;

use super::{HOUR_MS, LiveSample, TOPIC, WEEK_MS};
use crate::state::AppState;

/// Starts sampling once per second on the current runtime.
///
/// The task is detached: it stops when the runtime shuts down. A sample that
/// the writer cannot accept is dropped rather than stalling the sampler.
pub fn spawn(state: AppState) {
    drop(tokio::spawn(async move {
        let (sender, mut receiver) = mpsc::channel::<Snapshot>(8);
        drop(tokio::spawn(async move {
            Collector::new()
                .run(move |result| match result {
                    Ok(snapshot) => {
                        if sender.try_send(snapshot).is_err() {
                            tracing::warn!("dropping a metrics sample; the writer is behind");
                        }
                    }
                    Err(error) => tracing::warn!(%error, "metrics sampling failed"),
                })
                .await;
        }));

        while let Some(snapshot) = receiver.recv().await {
            ingest(&state, snapshot).await;
        }
    }));
}

/// Persists one sample, records the latest snapshot, and publishes it live.
pub async fn ingest(state: &AppState, snapshot: Snapshot) {
    let point = dandelion_sysmon::MetricsPoint::from_snapshot(&snapshot);
    state.metrics.set_latest(snapshot.clone());

    match serde_json::to_value(&LiveSample { snapshot, point }) {
        Ok(data) => {
            let _ = state.hub.publish(TOPIC, data);
        }
        Err(error) => tracing::warn!(%error, "cannot encode a metrics event"),
    }

    if let Err(error) = state.metrics.store().insert_point(&point).await {
        tracing::warn!(%error, "cannot persist a metrics point");
    }

    if let Some(minute) = state.metrics.push(point) {
        if let Err(error) = state.metrics.store().insert_minute(&minute).await {
            tracing::warn!(%error, "cannot persist a minute aggregate");
        }

        let now = point.timestamp_ms;
        if let Err(error) = state
            .metrics
            .store()
            .prune(now.saturating_sub(HOUR_MS), now.saturating_sub(WEEK_MS))
            .await
        {
            tracing::warn!(%error, "cannot prune metrics history");
        }
    }
}
