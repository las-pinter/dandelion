//! Backend selection and the retry that adopts one starting after the server.
//!
//! herdr-core starts beside the service, so a boot can have the server probe
//! first, find nothing and stay disabled until a restart. The retry here
//! covers that race. Selection happens once per process: a backend that dies
//! for good afterwards is not replaced.

use std::{future::Future, sync::Arc, time::Duration};

use dandelion_terminals::{
    TerminalBackend, herdr::ClassicHerdrBackend, herdr_core::HerdrCoreBackend,
};
use serde_json::json;

use super::TOPIC;
use crate::state::AppState;

/// First wait before the retry probes again.
const FIRST_WAIT: Duration = Duration::from_secs(5);
/// Cap on the retry's wait between probes.
const MAX_WAIT: Duration = Duration::from_secs(60);

/// Probes herdr-core, then classic herdr, naming the winner.
pub(super) async fn detect() -> Option<Arc<dyn TerminalBackend>> {
    let core = HerdrCoreBackend::from_environment();
    if core.detect().await.available {
        tracing::info!(socket = %core.socket().display(), "agent backend: herdr-core");
        return Some(Arc::new(core));
    }
    let classic = ClassicHerdrBackend::from_environment();
    if classic.detect().await.available {
        tracing::info!(socket = %classic.socket().display(), "agent backend: classic herdr");
        return Some(Arc::new(classic));
    }
    None
}

/// Keeps probing for a backend that starts after the server.
///
/// Does nothing when a backend was found at startup. The first answer is
/// adopted, and open pages are told through the `agents` hub topic; the event
/// publisher was started at boot and waits for the same backend.
///
/// The retry runs for the process lifetime: herdr may be started at any time,
/// and each probe is two local socket calls, so a capped poll costs little.
pub fn spawn_backend_retry(state: AppState) {
    spawn_backend_retry_with(state, FIRST_WAIT, MAX_WAIT, detect);
}

/// Starts the retry with given waits and probe; tests pass a fake probe.
fn spawn_backend_retry_with<F, Fut>(state: AppState, first: Duration, max: Duration, probe: F)
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = Option<Arc<dyn TerminalBackend>>> + Send,
{
    if state.agents.backend().is_some() {
        return;
    }
    tracing::info!("no terminal backend yet; retrying in the background");
    tokio::spawn(async move {
        retry_until_available(first, max, probe, |backend| {
            adopt(&state, backend);
        })
        .await;
    });
}

/// Installs a backend found after startup and announces it.
fn adopt(state: &AppState, backend: Arc<dyn TerminalBackend>) {
    state.agents.install(backend);
    let _ = state.hub.publish(TOPIC, json!({ "kind": "backend" }));
}

/// Probes until one answers, then hands the result over once.
///
/// The waits are parameters so a test can run without a paused clock.
async fn retry_until_available<T, F, Fut, P>(
    mut wait: Duration,
    max: Duration,
    mut probe: F,
    mut adopt: P,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
    P: FnMut(T),
{
    loop {
        tokio::time::sleep(wait).await;
        if let Some(found) = probe().await {
            adopt(found);
            return;
        }
        tracing::debug!(wait = ?wait, "no terminal backend yet; retrying");
        wait = wait.saturating_mul(2).min(max);
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use std::sync::atomic::{AtomicU32, Ordering};

    use dandelion_documents::Documents;
    use dandelion_terminals::{TerminalBackend, herdr_core::HerdrCoreBackend};

    use super::{TOPIC, adopt, retry_until_available, spawn_backend_retry_with};
    use crate::{
        agents::Agents, audit::AuditLog, auth::Auth, auth::origin::PublicOrigin,
        claude::ClaudeMonitor, containers::Containers, metrics::Metrics, services::Services,
        state::AppState, storage, ws::hub::Hub,
    };

    /// A state with no backend and an in-memory database.
    async fn agent_state() -> AppState {
        let pool = storage::open_in_memory().await.expect("in-memory database");
        let origin = PublicOrigin::parse("https://example.test").expect("test origin");
        let auth = Auth::new(pool.clone(), origin).expect("auth services");
        AppState::new(
            AuditLog::new(pool.clone()),
            auth,
            Hub::default(),
            Metrics::new(pool),
            Services::disabled(),
            Containers::disabled(),
            Agents::disabled(),
            ClaudeMonitor::disabled(),
            Documents::disabled(),
        )
    }

    #[tokio::test]
    async fn the_retry_probes_until_a_backend_answers_then_adopts_it_once() {
        let mut probes = 0_u32;
        let mut adopted = Vec::new();

        retry_until_available(
            Duration::from_millis(1),
            Duration::from_millis(2),
            || {
                probes += 1;
                std::future::ready((probes == 3).then_some(probes))
            },
            |value| adopted.push(value),
        )
        .await;

        assert_eq!(probes, 3, "one probe per wait until the third answers");
        assert_eq!(adopted, vec![3], "the answer is adopted exactly once");
    }

    #[tokio::test]
    async fn adopting_a_backend_announces_it_on_the_agents_topic() {
        let state = agent_state().await;
        let mut events = state.hub.subscribe(TOPIC);

        // Any backend object stands in: adoption never probes it.
        adopt(&state, Arc::new(HerdrCoreBackend::from_environment()));

        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("an event arrives")
            .expect("the subscription is live");
        assert_eq!(event["kind"], "backend");
        assert!(state.agents.backend().is_some(), "the backend is installed");
    }

    #[tokio::test]
    async fn the_startup_retry_adopts_a_backend_that_answers_late() {
        let state = agent_state().await;
        let probes = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&probes);
        spawn_backend_retry_with(
            state.clone(),
            Duration::from_millis(1),
            Duration::from_millis(2),
            move || {
                let seen = counter.fetch_add(1, Ordering::SeqCst);
                std::future::ready((seen >= 1).then(|| {
                    Arc::new(HerdrCoreBackend::from_environment()) as Arc<dyn TerminalBackend>
                }))
            },
        );

        tokio::time::timeout(Duration::from_secs(2), async {
            while state.agents.backend().is_none() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("the late backend is adopted");

        assert_eq!(probes.load(Ordering::SeqCst), 2, "the first probe missed");
    }

    #[tokio::test]
    async fn a_live_backend_is_not_replaced_by_the_retry() {
        let state = agent_state().await;
        state
            .agents
            .install(Arc::new(HerdrCoreBackend::from_environment()));
        let probes = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&probes);

        spawn_backend_retry_with(
            state.clone(),
            Duration::from_millis(1),
            Duration::from_millis(2),
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                std::future::ready(None)
            },
        );
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert_eq!(probes.load(Ordering::SeqCst), 0, "no probe with a backend");
        assert!(state.agents.backend().is_some(), "the live backend stays");
    }
}
