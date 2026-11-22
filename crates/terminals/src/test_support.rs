//! Fixture loaders for unit tests. Compiled only under `cfg(test)`.

use crate::herdr::wire::{
    LineResponse, PaneRead, PaneReadResult, Pong, Snapshot, SubscriptionFrame,
};
use crate::herdr_core::map::{self, CoreNotice};
use crate::herdr_core::wire::{HelloResult, ListResult, ReadResult, SubscribeResult};
use crate::jsonrpc::{self, Incoming};

const CORE_HELLO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr_core/hello.json"
));
const CORE_LIST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr_core/list.json"
));
const CORE_READ: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr_core/read.json"
));
const CORE_SUBSCRIBE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr_core/subscribe.json"
));
const CORE_EVENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr_core/events.ndjson"
));
const CORE_ATTACH_FRAMES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr_core/attach_frames.hex"
));
const CLASSIC_PING: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/ping.json"
));
const CLASSIC_SNAPSHOT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/snapshot_response.json"
));
const CLASSIC_READ: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/read_response.json"
));
const CLASSIC_EVENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/herdr/events.ndjson"
));

/// The recorded herdr-core `hello` result.
pub fn core_hello() -> HelloResult {
    serde_json::from_str(CORE_HELLO).expect("hello fixture")
}

/// The recorded herdr-core `terminal.list` result.
pub fn core_list() -> ListResult {
    serde_json::from_str(CORE_LIST).expect("list fixture")
}

/// The recorded herdr-core `events.subscribe` snapshot.
pub fn core_subscribe() -> SubscribeResult {
    serde_json::from_str(CORE_SUBSCRIBE).expect("subscribe fixture")
}

/// The recorded herdr-core `terminal.read` result.
pub fn core_read() -> ReadResult {
    serde_json::from_str(CORE_READ).expect("read fixture")
}

/// Every notification in the herdr-core event fixture, parsed.
pub fn core_events() -> Vec<CoreNotice> {
    CORE_EVENTS
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let Incoming::Notification(notification) =
                jsonrpc::parse_line(line).expect("notification fixture")
            else {
                panic!("expected notification line");
            };
            map::parse_notice(&notification.method, &notification.params)
                .expect("notice fixture")
                .expect("modelled notice")
        })
        .collect()
}

/// The raw bytes of each recorded attach frame, in order.
pub fn core_attach_frames() -> Vec<Vec<u8>> {
    CORE_ATTACH_FRAMES
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            assert!(line.len() % 2 == 0, "odd hex line");
            (0..line.len())
                .step_by(2)
                .map(|index| u8::from_str_radix(&line[index..index + 2], 16).expect("hex byte"))
                .collect()
        })
        .collect()
}

/// The recorded classic herdr `ping` result.
pub fn classic_pong() -> Pong {
    serde_json::from_str(CLASSIC_PING).expect("ping fixture")
}

/// The snapshot inside the recorded classic herdr snapshot response.
pub fn classic_snapshot() -> Snapshot {
    let response: LineResponse = serde_json::from_str(CLASSIC_SNAPSHOT).expect("snapshot fixture");
    let result = response.result.expect("snapshot result");
    let snapshot = result.get("snapshot").expect("snapshot object");
    serde_json::from_value(snapshot.clone()).expect("snapshot shape")
}

/// The recorded classic herdr `pane.read` result.
pub fn classic_read() -> PaneRead {
    let response: LineResponse = serde_json::from_str(CLASSIC_READ).expect("read fixture");
    let result: PaneReadResult =
        serde_json::from_value(response.result.expect("read result")).expect("read shape");
    result.read
}

/// Every subscription frame in the classic event fixture, parsed.
pub fn classic_events() -> Vec<SubscriptionFrame> {
    CLASSIC_EVENTS
        .lines()
        .filter(|line| !line.trim().is_empty() && line.contains("\"event\":"))
        .map(|line| serde_json::from_str(line).expect("event fixture"))
        .collect()
}
