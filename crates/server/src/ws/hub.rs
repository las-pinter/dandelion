//! Topic-addressed broadcast hub for live streams.
//!
//! Each topic has its own bounded channel. Publishers never block, and a
//! connection only receives the topics it subscribed to, so a flood on one
//! topic cannot lag a client that never asked for it. A receiver that falls
//! behind on its own topic is closed by its connection.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use serde_json::Value;
use tokio::sync::broadcast;

/// Events a healthy connection may fall behind before it is dropped.
pub const DEFAULT_CAPACITY: usize = 256;
/// Largest serialized event payload the hub accepts.
pub const MAX_PAYLOAD_BYTES: usize = 64 * 1024;
/// Unused topic channels are pruned once the map reaches this size.
const PRUNE_THRESHOLD: usize = 256;

type TopicSenders = HashMap<String, broadcast::Sender<Arc<Value>>>;

/// Fan-out point shared by every WebSocket connection.
#[derive(Debug, Clone)]
pub struct Hub {
    capacity: usize,
    topics: Arc<Mutex<TopicSenders>>,
}

impl Hub {
    /// Creates a hub; a capacity of zero becomes one.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            topics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Publishes to every subscriber of one topic without blocking.
    ///
    /// Returns how many receivers accepted the event; zero when nobody is
    /// subscribed or the payload exceeds [`MAX_PAYLOAD_BYTES`].
    #[must_use]
    pub fn publish(&self, topic: &str, data: Value) -> usize {
        match serde_json::to_vec(&data) {
            Ok(bytes) if bytes.len() > MAX_PAYLOAD_BYTES => {
                tracing::warn!(
                    topic,
                    bytes = bytes.len(),
                    "dropping an oversized websocket event"
                );
                return 0;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, topic, "cannot measure a websocket event");
                return 0;
            }
        }

        self.topics()
            .get(topic)
            .cloned()
            .and_then(|sender| sender.send(Arc::new(data)).ok())
            .unwrap_or(0)
    }

    /// Creates a receiver for one topic; the channel appears on first use.
    #[must_use]
    pub fn subscribe(&self, topic: &str) -> broadcast::Receiver<Arc<Value>> {
        let mut topics = self.topics();
        if topics.len() >= PRUNE_THRESHOLD {
            topics.retain(|_, sender| sender.receiver_count() > 0);
        }
        topics
            .entry(topic.to_owned())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .subscribe()
    }

    /// Number of live receivers across every topic; used by tests.
    #[must_use]
    pub fn receivers(&self) -> usize {
        self.topics()
            .values()
            .map(broadcast::Sender::receiver_count)
            .sum()
    }

    fn topics(&self) -> MutexGuard<'_, TopicSenders> {
        self.topics.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Hub, MAX_PAYLOAD_BYTES};

    #[test]
    fn publish_reaches_a_receiver() {
        let hub = Hub::new(4);
        let mut receiver = hub.subscribe("alpha");

        let sent = hub.publish("alpha", json!({"n": 1}));
        let event = receiver.try_recv().expect("event");

        assert_eq!(sent, 1);
        assert_eq!(event["n"], 1);
    }

    #[test]
    fn publish_without_receivers_is_harmless() {
        let hub = Hub::new(4);

        assert_eq!(hub.publish("alpha", json!({})), 0);
        assert_eq!(hub.receivers(), 0);
    }

    #[test]
    fn topics_are_isolated() {
        let hub = Hub::new(4);
        let mut quiet = hub.subscribe("quiet");

        for n in 0..1_000 {
            assert_eq!(hub.publish("noise", json!({ "n": n })), 0);
        }
        assert!(quiet.try_recv().is_err());

        assert_eq!(hub.publish("quiet", json!({"n": 1})), 1);
        assert_eq!(quiet.try_recv().expect("event")["n"], 1);
    }

    #[test]
    fn oversized_payloads_are_dropped() {
        let hub = Hub::new(4);
        let mut receiver = hub.subscribe("big");
        let payload = "x".repeat(MAX_PAYLOAD_BYTES + 1);

        assert_eq!(hub.publish("big", json!({ "payload": payload })), 0);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn a_zero_capacity_still_works() {
        let hub = Hub::new(0);
        let mut receiver = hub.subscribe("alpha");

        assert_eq!(hub.publish("alpha", json!({"n": 1})), 1);
        assert_eq!(receiver.try_recv().expect("event")["n"], 1);
    }
}
