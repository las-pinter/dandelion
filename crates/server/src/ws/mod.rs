//! The authenticated WebSocket endpoint and its connection loop.
//!
//! One socket carries every topic a screen needs: subscribe and unsubscribe
//! messages change the set, and each connection receives only its own topics.
//! A client that cannot keep up is closed, never allowed to stall the hub.
//! The loop also revalidates the session so a revoked or expired login cannot
//! keep listening.

pub mod hub;
pub mod protocol;

use std::{
    collections::HashSet,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    extract::{
        Extension, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast::{self, error::RecvError};

use self::protocol::{ClientMessage, MAX_TOPICS, ServerMessage, validate_topics};
use crate::{
    auth::{
        Revocation,
        middleware::{RequestUser, error_response, origin_allowed},
        session::now_millis,
    },
    state::AppState,
};

/// Largest frame accepted from a client.
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
/// How long one write may block before the client is considered too slow.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a live socket revalidates its session against the database.
const SESSION_RECHECK: Duration = Duration::from_secs(30);

/// Upgrades an authenticated, same-origin request to a WebSocket.
pub async fn handler(
    State(state): State<AppState>,
    Extension(user): Extension<RequestUser>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(state.auth.origin(), &headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "origin_rejected",
            "the WebSocket handshake did not come from the Dandelion origin",
        );
    }

    upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .max_frame_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection(socket, state, user))
}

async fn connection(socket: WebSocket, state: AppState, user: RequestUser) {
    let (mut sink, mut stream) = socket.split();
    let mut events = Events::default();
    let mut subscriptions = HashSet::new();
    let mut revocations = state.auth.subscribe_revocations();
    let mut recheck = tokio::time::interval(SESSION_RECHECK);
    recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    recheck.tick().await;

    if !send(&mut sink, &ServerMessage::Welcome { topics: Vec::new() }).await {
        return;
    }

    loop {
        tokio::select! {
            frame = stream.next() => {
                let Some(Ok(message)) = frame else { break };
                match message {
                    Message::Text(text) => {
                        let reply = match serde_json::from_str::<ClientMessage>(text.as_str()) {
                            Ok(ClientMessage::Ping) => ServerMessage::Pong,
                            Ok(ClientMessage::Subscribe { topics }) => {
                                match subscribe(&mut subscriptions, &topics) {
                                    Ok(added) => {
                                        for topic in &added {
                                            events.add(topic.clone(), state.hub.subscribe(topic));
                                        }
                                        ServerMessage::Subscribed {
                                            topics: sorted(&subscriptions),
                                        }
                                    }
                                    Err(message) => ServerMessage::Error { message },
                                }
                            }
                            Ok(ClientMessage::Unsubscribe { topics }) => {
                                match unsubscribe(&mut subscriptions, &topics) {
                                    Ok(removed) => {
                                        for topic in &removed {
                                            events.remove(topic);
                                        }
                                        ServerMessage::Unsubscribed {
                                            topics: sorted(&subscriptions),
                                        }
                                    }
                                    Err(message) => ServerMessage::Error { message },
                                }
                            }
                            Err(_) => ServerMessage::Error {
                                message: "unrecognized message".to_owned(),
                            },
                        };
                        if !send(&mut sink, &reply).await {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            event = events.next() => {
                match event {
                    Some(Ok((topic, data))) => {
                        if !send_event(&mut sink, &topic, &data).await {
                            tracing::warn!("dropping a websocket client that cannot keep up");
                            break;
                        }
                    }
                    Some(Err(error)) => {
                        tracing::warn!(?error, "dropping a lagging websocket client");
                        break;
                    }
                    None => {}
                }
            }
            _ = recheck.tick() => {
                if !session_is_live(&state, &user.token_hash).await {
                    tracing::info!("closing a websocket whose session ended");
                    break;
                }
            }
            revocation = revocations.recv() => {
                match revocation {
                    Ok(Revocation::Session(hash)) if hash == user.token_hash => break,
                    Ok(Revocation::User(id)) if id == user.user_id => break,
                    Ok(_) | Err(RecvError::Closed) => {}
                    Err(RecvError::Lagged(_)) => break,
                }
            }
        }
    }
    // Dropping both halves closes the socket.
}

async fn session_is_live(state: &AppState, token_hash: &str) -> bool {
    match state.auth.session_live(token_hash, now_millis()).await {
        Ok(live) => live,
        Err(error) => {
            tracing::error!(%error, "session recheck failed");
            false
        }
    }
}

/// One event, or the receiver error that closes the connection.
type TopicEvent = Result<(String, Arc<Value>), RecvError>;
type TopicStream = Pin<Box<dyn Stream<Item = TopicEvent> + Send>>;

fn topic_stream(topic: String, receiver: broadcast::Receiver<Arc<Value>>) -> TopicStream {
    Box::pin(futures_util::stream::unfold(
        (topic, receiver),
        |(topic, mut receiver)| async move {
            let item = match receiver.recv().await {
                Ok(data) => Ok((topic.clone(), data)),
                Err(error) => Err(error),
            };
            Some((item, (topic, receiver)))
        },
    ))
}

/// Events queued for one connection, one stream per subscribed topic.
#[derive(Default)]
struct Events {
    streams: Vec<(String, TopicStream)>,
    cursor: usize,
}

impl Events {
    fn add(&mut self, topic: String, receiver: broadcast::Receiver<Arc<Value>>) {
        let stream = topic_stream(topic.clone(), receiver);
        self.streams.push((topic, stream));
    }

    fn remove(&mut self, topic: &str) {
        self.streams.retain(|(name, _)| name != topic);
        if self.streams.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor %= self.streams.len();
        }
    }
}

impl Stream for Events {
    type Item = TopicEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let len = this.streams.len();
        if len == 0 {
            return Poll::Pending;
        }

        let start = this.cursor % len;
        for offset in 0..len {
            let index = (start + offset) % len;
            let (_, stream) = &mut this.streams[index];
            if let Poll::Ready(Some(item)) = stream.as_mut().poll_next(cx) {
                this.cursor = (index + 1) % len;
                return Poll::Ready(Some(item));
            }
        }
        this.cursor = (start + 1) % len;
        Poll::Pending
    }
}

async fn send<S>(sink: &mut S, message: &ServerMessage) -> bool
where
    S: Sink<Message> + Unpin,
{
    send_text(sink, message).await
}

async fn send_event<S>(sink: &mut S, topic: &str, data: &Value) -> bool
where
    S: Sink<Message> + Unpin,
{
    send_text(
        sink,
        &EventWire {
            kind: "event",
            topic,
            data,
        },
    )
    .await
}

/// Serializes without cloning the event payload.
#[derive(Serialize)]
struct EventWire<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    topic: &'a str,
    data: &'a Value,
}

async fn send_text<S, T>(sink: &mut S, message: &T) -> bool
where
    S: Sink<Message> + Unpin,
    T: Serialize + ?Sized,
{
    let Ok(text) = serde_json::to_string(message) else {
        return false;
    };
    matches!(
        tokio::time::timeout(WRITE_TIMEOUT, sink.send(Message::Text(text.into()))).await,
        Ok(Ok(()))
    )
}

fn subscribe(
    subscriptions: &mut HashSet<String>,
    topics: &[String],
) -> Result<Vec<String>, String> {
    validate_topics(topics).map_err(str::to_owned)?;
    let before = subscriptions.clone();
    let mut added = Vec::new();
    for topic in topics {
        if subscriptions.insert(topic.clone()) {
            added.push(topic.clone());
        }
    }
    if subscriptions.len() > MAX_TOPICS {
        *subscriptions = before;
        return Err(format!("a connection may hold at most {MAX_TOPICS} topics"));
    }
    Ok(added)
}

fn unsubscribe(
    subscriptions: &mut HashSet<String>,
    topics: &[String],
) -> Result<Vec<String>, String> {
    validate_topics(topics).map_err(str::to_owned)?;
    let mut removed = Vec::new();
    for topic in topics {
        if subscriptions.remove(topic) {
            removed.push(topic.clone());
        }
    }
    Ok(removed)
}

fn sorted(set: &HashSet<String>) -> Vec<String> {
    let mut topics: Vec<String> = set.iter().cloned().collect();
    topics.sort();
    topics
}
