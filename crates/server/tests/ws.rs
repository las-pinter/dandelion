//! WebSocket acceptance tests: auth, origin, topics, revocation, backpressure.

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
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
    notifications::Notifications,
    services::Services,
    state::AppState,
    storage,
    ws::{
        hub::Hub,
        protocol::{ClientMessage, ServerMessage},
    },
};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpSocket};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, client_async, connect_async,
    tungstenite::{Error as WsError, Message, client::IntoClientRequest},
};
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";

type Client = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

async fn test_state(hub: Hub) -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        hub,
        Metrics::new(pool),
        Services::disabled(),
        Containers::disabled(),
        Agents::disabled(),
        ClaudeMonitor::disabled(),
        Notifications::disabled(),
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

async fn spawn(state: AppState) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a test listener");
    let address = listener.local_addr().expect("listener address");
    tokio::spawn(async move {
        axum::serve(listener, build_router(state))
            .await
            .expect("test server");
    });

    address
}

async fn handshake(
    address: SocketAddr,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> Result<Client, WsError> {
    let mut request = format!("ws://{address}/api/ws")
        .into_client_request()
        .expect("websocket request");
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("cookie", cookie.parse().expect("cookie header"));
    }
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("origin", origin.parse().expect("origin header"));
    }

    connect_async(request).await.map(|(socket, _)| socket)
}

async fn connect(address: SocketAddr, cookie: &str) -> Client {
    handshake(address, Some(cookie), Some(ORIGIN))
        .await
        .expect("websocket handshake")
}

/// A client with a tiny receive buffer, so a burst reliably outruns it.
async fn connect_slow(address: SocketAddr, cookie: &str) -> WebSocketStream<tokio::net::TcpStream> {
    let socket = TcpSocket::new_v4().expect("tcp socket");
    socket
        .set_recv_buffer_size(4096)
        .expect("receive buffer size");
    let stream = socket.connect(address).await.expect("connect");
    let mut request = format!("ws://{address}/api/ws")
        .into_client_request()
        .expect("websocket request");
    request
        .headers_mut()
        .insert("cookie", cookie.parse().expect("cookie header"));
    request
        .headers_mut()
        .insert("origin", ORIGIN.parse().expect("origin header"));

    client_async(request, stream)
        .await
        .expect("slow websocket handshake")
        .0
}

async fn send<S>(socket: &mut S, message: ClientMessage)
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    let text = serde_json::to_string(&message).expect("serialize");
    socket
        .send(Message::Text(text.into()))
        .await
        .expect("send frame");
}

async fn next_server<S>(socket: &mut S) -> ServerMessage
where
    S: Stream<Item = Result<Message, WsError>> + Unpin,
{
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .expect("a frame within two seconds")
        .expect("an open socket")
        .expect("a valid frame");

    match frame {
        Message::Text(text) => serde_json::from_str(text.as_str()).expect("a server message"),
        other => panic!("unexpected frame: {other:?}"),
    }
}

async fn subscribe<S>(socket: &mut S, topics: &[&str])
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    send(
        socket,
        ClientMessage::Subscribe {
            topics: topics.iter().map(|topic| (*topic).to_owned()).collect(),
        },
    )
    .await;
}

/// Waits for the server to close the socket.
async fn assert_closed<S>(socket: &mut S)
where
    S: Stream<Item = Result<Message, WsError>> + Unpin,
{
    tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(frame) = socket.next().await {
            if frame.is_err() || matches!(frame, Ok(Message::Close(_))) {
                return;
            }
        }
    })
    .await
    .expect("the server must close the socket");
}

async fn post_logout(state: &AppState, path: &str, cookie: &str) -> StatusCode {
    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("cookie", cookie)
                .header("origin", ORIGIN)
                .body(Body::empty())
                .expect("logout request"),
        )
        .await
        .expect("logout response");

    response.status()
}

#[tokio::test]
async fn handshake_requires_a_session() {
    let state = test_state(Hub::default()).await;
    let address = spawn(state).await;

    let error = handshake(address, None, Some(ORIGIN))
        .await
        .expect_err("no session must fail");

    match error {
        WsError::Http(response) => assert_eq!(response.status(), 401),
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn handshake_rejects_a_foreign_or_missing_origin() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state).await;

    let foreign = handshake(address, Some(&cookie), Some("https://evil.test"))
        .await
        .expect_err("foreign origin must fail");
    let missing = handshake(address, Some(&cookie), None)
        .await
        .expect_err("missing origin must fail");

    for error in [foreign, missing] {
        match error {
            WsError::Http(response) => assert_eq!(response.status(), 403),
            other => panic!("unexpected error: {other}"),
        }
    }
}

#[tokio::test]
async fn same_origin_fetch_metadata_is_accepted() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state).await;

    let mut request = format!("ws://{address}/api/ws")
        .into_client_request()
        .expect("websocket request");
    request
        .headers_mut()
        .insert("cookie", cookie.parse().expect("cookie header"));
    request.headers_mut().insert(
        "sec-fetch-site",
        "same-origin".parse().expect("fetch metadata header"),
    );

    let mut socket = connect_async(request).await.expect("fallback handshake").0;

    assert_eq!(
        next_server(&mut socket).await,
        ServerMessage::Welcome { topics: Vec::new() }
    );
}

#[tokio::test]
async fn receives_only_subscribed_topics() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state.clone()).await;
    let mut socket = connect(address, &cookie).await;

    assert_eq!(
        next_server(&mut socket).await,
        ServerMessage::Welcome { topics: Vec::new() }
    );

    subscribe(&mut socket, &["alpha", "beta"]).await;
    assert_eq!(
        next_server(&mut socket).await,
        ServerMessage::Subscribed {
            topics: vec!["alpha".to_owned(), "beta".to_owned()]
        }
    );

    let _ = state.hub.publish("alpha", json!({"n": 1}));
    let _ = state.hub.publish("beta", json!({"n": 2}));
    let _ = state.hub.publish("gamma", json!({"n": 3}));

    // Topics are independent queues, so their relative order is not defined.
    let mut received = vec![
        event_parts(next_server(&mut socket).await),
        event_parts(next_server(&mut socket).await),
    ];
    received.sort();
    assert_eq!(
        received,
        vec![("alpha".to_owned(), 1), ("beta".to_owned(), 2)]
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), socket.next())
            .await
            .is_err(),
        "an unsubscribed topic must not arrive"
    );

    send(
        &mut socket,
        ClientMessage::Unsubscribe {
            topics: vec!["alpha".to_owned()],
        },
    )
    .await;
    assert_eq!(
        next_server(&mut socket).await,
        ServerMessage::Unsubscribed {
            topics: vec!["beta".to_owned()]
        }
    );
    let _ = state.hub.publish("alpha", json!({"n": 4}));
    let _ = state.hub.publish("beta", json!({"n": 5}));
    assert_event(next_server(&mut socket).await, "beta", 5);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), socket.next())
            .await
            .is_err(),
        "an unsubscribed topic must not arrive"
    );
}

#[tokio::test]
async fn a_flood_on_an_unsubscribed_topic_does_not_disconnect() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state.clone()).await;
    let mut socket = connect(address, &cookie).await;
    let _ = next_server(&mut socket).await;
    subscribe(&mut socket, &["quiet"]).await;
    let _ = next_server(&mut socket).await;

    for n in 0..10_000 {
        assert_eq!(
            state.hub.publish("noise", json!({ "n": n })),
            0,
            "nobody subscribed to the noise topic"
        );
    }
    let _ = state.hub.publish("quiet", json!({"n": 1}));

    assert_event(next_server(&mut socket).await, "quiet", 1);
}

#[tokio::test]
async fn the_cumulative_topic_cap_is_enforced() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state).await;
    let mut socket = connect(address, &cookie).await;
    let _ = next_server(&mut socket).await;

    let topics: Vec<String> = (0..32).map(|n| format!("topic{n}")).collect();
    send(&mut socket, ClientMessage::Subscribe { topics }).await;
    let ServerMessage::Subscribed { topics } = next_server(&mut socket).await else {
        panic!("expected a subscription acknowledgement");
    };
    assert_eq!(topics.len(), 32);

    subscribe(&mut socket, &["overflow"]).await;
    assert!(matches!(
        next_server(&mut socket).await,
        ServerMessage::Error { .. }
    ));

    send(
        &mut socket,
        ClientMessage::Unsubscribe {
            topics: vec!["topic0".to_owned()],
        },
    )
    .await;
    let ServerMessage::Unsubscribed { topics } = next_server(&mut socket).await else {
        panic!("expected an unsubscription acknowledgement");
    };
    assert_eq!(topics.len(), 31);
    assert!(!topics.contains(&"overflow".to_owned()));
}

#[tokio::test]
async fn ping_answers_and_bad_messages_report_errors() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state).await;
    let mut socket = connect(address, &cookie).await;
    let _ = next_server(&mut socket).await;

    send(&mut socket, ClientMessage::Ping).await;
    assert_eq!(next_server(&mut socket).await, ServerMessage::Pong);

    socket
        .send(Message::Text(r#"{"type":"nonsense"}"#.into()))
        .await
        .expect("send bad message");
    assert!(matches!(
        next_server(&mut socket).await,
        ServerMessage::Error { .. }
    ));

    subscribe(&mut socket, &["has space"]).await;
    assert!(matches!(
        next_server(&mut socket).await,
        ServerMessage::Error { .. }
    ));
}

#[tokio::test]
async fn an_oversized_frame_closes_the_socket() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state).await;
    let mut socket = connect(address, &cookie).await;
    let _ = next_server(&mut socket).await;

    socket
        .send(Message::Text("x".repeat(64 * 1024 + 1).into()))
        .await
        .expect("send an oversized frame");

    assert_closed(&mut socket).await;
}

#[tokio::test]
async fn logout_closes_the_live_socket() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state.clone()).await;
    let mut socket = connect(address, &cookie).await;
    let _ = next_server(&mut socket).await;

    let status = post_logout(&state, "/api/auth/logout", &cookie).await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_closed(&mut socket).await;
}

#[tokio::test]
async fn logout_all_closes_the_live_socket() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state.clone()).await;
    let mut socket = connect(address, &cookie).await;
    let _ = next_server(&mut socket).await;

    let status = post_logout(&state, "/api/auth/logout-all", &cookie).await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_closed(&mut socket).await;
}

#[tokio::test]
async fn a_slow_client_is_dropped_without_blocking_others() {
    let state = test_state(Hub::default()).await;
    let cookie = session_cookie(&state).await;
    let address = spawn(state.clone()).await;

    let mut slow = connect_slow(address, &cookie).await;
    let mut fast = connect(address, &cookie).await;
    let _ = next_server(&mut slow).await;
    let _ = next_server(&mut fast).await;
    subscribe(&mut slow, &["burst"]).await;
    subscribe(&mut fast, &["burst"]).await;
    let _ = next_server(&mut slow).await;
    let _ = next_server(&mut fast).await;

    let received = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&received);
    let reader = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = fast.next().await {
            if matches!(
                serde_json::from_str::<ServerMessage>(text.as_str()),
                Ok(ServerMessage::Event { .. })
            ) {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    let payload = "x".repeat(2048);
    let data: Value = json!({ "payload": payload });
    for n in 0..10_000 {
        let _ = state
            .hub
            .publish("burst", json!({ "n": n, "payload": data["payload"] }));
        tokio::task::yield_now().await;
    }

    tokio::time::timeout(Duration::from_secs(10), async {
        while received.load(Ordering::SeqCst) < 100 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the fast client keeps receiving");

    // Give the server's write timeout time to fire while the slow client is
    // still not reading.
    tokio::time::sleep(Duration::from_secs(6)).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut dropped = false;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, slow.next()).await {
            Ok(None | Some(Err(_))) => {
                dropped = true;
                break;
            }
            Ok(Some(Ok(_))) => {}
            Err(_) => break,
        }
    }
    reader.abort();

    assert!(dropped, "the slow client must be disconnected");
    assert!(
        received.load(Ordering::SeqCst) >= 100,
        "the fast client must keep its stream"
    );
}

fn assert_event(message: ServerMessage, topic: &str, n: i64) {
    assert_eq!(event_parts(message), (topic.to_owned(), n));
}

fn event_parts(message: ServerMessage) -> (String, i64) {
    let ServerMessage::Event { topic, data } = message else {
        panic!("expected an event, got {message:?}");
    };
    (topic, data["n"].as_i64().expect("an event number"))
}
