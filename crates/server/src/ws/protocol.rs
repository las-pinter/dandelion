//! WebSocket wire protocol.
//!
//! Messages are JSON text frames tagged with `type`. The TypeScript mirror is
//! generated from these types by `dandelion ts-types`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Most topics one connection may hold.
pub const MAX_TOPICS: usize = 32;
/// Longest topic name.
pub const MAX_TOPIC_LEN: usize = 64;

/// A message from the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Add topics to this connection.
    Subscribe {
        /// Topic names to add.
        topics: Vec<String>,
    },
    /// Remove topics from this connection.
    Unsubscribe {
        /// Topic names to remove.
        topics: Vec<String>,
    },
    /// Application-level keepalive.
    Ping,
}

/// A message from the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Sent once when the socket opens.
    Welcome {
        /// Current subscriptions; empty on a fresh connection.
        topics: Vec<String>,
    },
    /// The full subscription set after a subscribe.
    Subscribed {
        /// Current subscriptions, sorted.
        topics: Vec<String>,
    },
    /// The full subscription set after an unsubscribe.
    Unsubscribed {
        /// Current subscriptions, sorted.
        topics: Vec<String>,
    },
    /// One published event.
    Event {
        /// Topic the event belongs to.
        topic: String,
        /// Opaque payload for that topic.
        #[ts(type = "unknown")]
        data: Value,
    },
    /// A client message was rejected.
    Error {
        /// Human-readable reason.
        message: String,
    },
    /// Answer to [`ClientMessage::Ping`].
    Pong,
}

/// Checks that a topic list is well formed and bounded.
pub fn validate_topics(topics: &[String]) -> Result<(), &'static str> {
    if topics.is_empty() {
        return Err("the topics list is empty");
    }
    if topics.len() > MAX_TOPICS {
        return Err("too many topics in one message");
    }
    for topic in topics {
        if topic.is_empty()
            || topic.len() > MAX_TOPIC_LEN
            || !topic
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err("topics are 1-64 characters: letters, digits, dot, dash, underscore");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ClientMessage, MAX_TOPICS, validate_topics};

    #[test]
    fn client_messages_are_tagged_json() {
        let message = ClientMessage::Subscribe {
            topics: vec!["system.metrics".to_owned()],
        };

        let value = serde_json::to_value(&message).expect("serialize");

        assert_eq!(value["type"], "subscribe");
        assert_eq!(value["topics"][0], "system.metrics");
    }

    #[test]
    fn rejects_empty_and_oversized_lists() {
        assert!(validate_topics(&[]).is_err());
        assert!(validate_topics(&vec!["a".to_owned(); MAX_TOPICS + 1]).is_err());
    }

    #[test]
    fn rejects_malformed_topic_names() {
        for topic in ["", "has space", "sla/sh", "é", &"a".repeat(65)] {
            assert!(validate_topics(&[topic.to_owned()]).is_err(), "{topic}");
        }
        for topic in ["a", "system.metrics", "agent-review_bot.1"] {
            assert!(validate_topics(&[topic.to_owned()]).is_ok(), "{topic}");
        }
    }
}
