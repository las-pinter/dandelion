//! JSON-RPC 2.0 line codec for herdr-core.
//!
//! herdr-core accepts one JSON object per line, numbers or strings as ids, and
//! carries stable error kinds in `error.data.kind`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::TerminalError;

/// The JSON-RPC version both sides send.
pub const VERSION: &str = "2.0";

/// A request or notification id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// Numeric id.
    Number(u64),
    /// String id, which herdr-core also accepts.
    Text(String),
}

impl From<u64> for RequestId {
    fn from(value: u64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for RequestId {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for RequestId {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(value) => write!(f, "{value}"),
            Self::Text(value) => f.write_str(value),
        }
    }
}

/// A request on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Always [`VERSION`].
    pub jsonrpc: String,
    /// Request id.
    pub id: RequestId,
    /// Method name.
    pub method: String,
    /// Method parameters.
    pub params: Value,
}

impl Request {
    /// Builds a request.
    #[must_use]
    pub fn new(id: impl Into<RequestId>, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: VERSION.to_owned(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }

    /// Encodes the request as exactly one newline-terminated line.
    #[must_use]
    pub fn encode_line(&self) -> String {
        let mut line = serde_json::to_string(self).unwrap_or_else(|_| {
            // A Request only holds serialisable values; this branch keeps the
            // function infallible without panicking.
            String::from(r#"{"jsonrpc":"2.0","id":0,"method":"invalid","params":null}"#)
        });
        line.push('\n');
        line
    }
}

/// The stable error payload herdr-core adds to JSON-RPC errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorData {
    /// Stable kind such as `not_found`, `blocked` or `status_mismatch`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Hint for the caller.
    #[serde(default)]
    pub hint: Option<String>,
}

/// A JSON-RPC error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    /// Numeric code (`-32601` unknown method, `-32000` domain error).
    pub code: i64,
    /// Human-readable message.
    pub message: String,
    /// Optional structured data.
    #[serde(default)]
    pub data: Option<ErrorData>,
}

impl From<RpcError> for TerminalError {
    fn from(error: RpcError) -> Self {
        let kind = error.data.as_ref().and_then(|data| data.kind.clone());
        let hint = error.data.as_ref().and_then(|data| data.hint.clone());
        match kind.as_deref() {
            Some("not_found") => Self::NotFound(error.message),
            Some("blocked") => Self::Blocked(error.message),
            Some("status_mismatch") => Self::StatusMismatch(error.message),
            Some("live") => Self::StillLive(error.message),
            Some("unknown_method" | "needs_framed" | "unsupported_generation") => {
                Self::Unsupported(error.message)
            }
            _ => Self::Backend {
                code: Some(error.code),
                kind,
                message: error.message,
                hint,
            },
        }
    }
}

/// The payload of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsePayload {
    /// The method succeeded.
    Result(Value),
    /// The method failed.
    Error(RpcError),
}

/// A response to a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Echoed request id.
    pub id: Option<RequestId>,
    /// Result or error.
    pub payload: ResponsePayload,
}

impl Response {
    /// Returns the result, or maps the error.
    ///
    /// # Errors
    ///
    /// Returns the mapped [`TerminalError`] for an error response.
    pub fn into_result(self) -> Result<Value, TerminalError> {
        match self.payload {
            ResponsePayload::Result(value) => Ok(value),
            ResponsePayload::Error(error) => Err(error.into()),
        }
    }
}

/// A server notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Method name such as `event` or `heartbeat`.
    pub method: String,
    /// Its parameters.
    pub params: Value,
}

/// Anything the server can send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// A response to one of our requests.
    Response(Response),
    /// A server notification.
    Notification(Notification),
}

/// Parses one line from the socket.
///
/// # Errors
///
/// Returns [`TerminalError::Protocol`] for a non-object message, a message
/// with neither result nor error, or an unsupported `jsonrpc` version.
pub fn parse_line(line: &str) -> Result<Incoming, TerminalError> {
    let value: Value = serde_json::from_str(line)?;
    let object = value
        .as_object()
        .ok_or_else(|| TerminalError::Protocol("message is not a JSON object".into()))?;

    if let Some(version) = object.get("jsonrpc").and_then(Value::as_str)
        && version != VERSION
    {
        return Err(TerminalError::Protocol(format!(
            "unsupported jsonrpc version {version}"
        )));
    }

    if let Some(method) = object.get("method").and_then(Value::as_str) {
        return Ok(Incoming::Notification(Notification {
            method: method.to_owned(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        }));
    }

    let id = object
        .get("id")
        .filter(|value| !value.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;

    if let Some(error) = object.get("error") {
        let error: RpcError = serde_json::from_value(error.clone())?;
        return Ok(Incoming::Response(Response {
            id,
            payload: ResponsePayload::Error(error),
        }));
    }

    if let Some(result) = object.get("result") {
        return Ok(Incoming::Response(Response {
            id,
            payload: ResponsePayload::Result(result.clone()),
        }));
    }

    Err(TerminalError::Protocol(
        "message has neither result, error nor method".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{Incoming, Request, ResponsePayload, parse_line};
    use crate::error::TerminalError;

    #[test]
    fn request_encodes_as_one_newline_terminated_line() {
        let request = Request::new(7_u64, "terminal.list", serde_json::json!({}));

        assert_eq!(
            request.encode_line(),
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"terminal.list\",\"params\":{}}\n"
        );
    }

    #[test]
    fn string_ids_are_accepted_in_requests_and_responses() {
        let request = Request::new("s1", "events.subscribe", serde_json::json!({}));
        assert!(request.encode_line().contains("\"id\":\"s1\""));

        let incoming =
            parse_line(r#"{"jsonrpc":"2.0","id":"s1","result":{"seq":3}}"#).expect("response");
        let Incoming::Response(response) = incoming else {
            panic!("expected response");
        };
        assert_eq!(
            response.id.as_ref().map(ToString::to_string),
            Some("s1".to_owned())
        );
    }

    #[test]
    fn response_result_is_returned() {
        let incoming =
            parse_line(r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#).expect("response");
        let Incoming::Response(response) = incoming else {
            panic!("expected response");
        };
        let value = response.into_result().expect("result");
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn error_kinds_map_to_domain_errors() {
        let line = r#"{"jsonrpc":"2.0","id":5,"error":{"code":-32000,"data":{"hint":"see `hc ls --all`","kind":"not_found"},"message":"no terminal \"t999\""}}"#;
        let Incoming::Response(response) = parse_line(line).expect("response") else {
            panic!("expected response");
        };
        let error = response.into_result().expect_err("error");
        assert!(matches!(error, TerminalError::NotFound(message) if message.contains("t999")));
    }

    #[test]
    fn blocked_and_status_mismatch_keep_their_variants() {
        let blocked = r#"{"jsonrpc":"2.0","id":6,"error":{"code":-32000,"data":{"kind":"blocked"},"message":"t37 is blocked"}}"#;
        let Incoming::Response(response) = parse_line(blocked).expect("response") else {
            panic!("expected response");
        };
        assert!(matches!(
            response.into_result(),
            Err(TerminalError::Blocked(_))
        ));

        let mismatch = r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32000,"data":{"kind":"status_mismatch"},"message":"nothing was typed"}}"#;
        let Incoming::Response(response) = parse_line(mismatch).expect("response") else {
            panic!("expected response");
        };
        assert!(matches!(
            response.into_result(),
            Err(TerminalError::StatusMismatch(_))
        ));
    }

    #[test]
    fn notifications_are_recognised() {
        let incoming =
            parse_line(r#"{"jsonrpc":"2.0","method":"heartbeat","params":{"seq":3705}}"#)
                .expect("notification");
        let Incoming::Notification(notification) = incoming else {
            panic!("expected notification");
        };
        assert_eq!(notification.method, "heartbeat");
        assert_eq!(notification.params["seq"], 3705);
    }

    #[test]
    fn malformed_messages_are_protocol_errors() {
        assert!(matches!(parse_line("[]"), Err(TerminalError::Protocol(_))));
        assert!(matches!(
            parse_line(r#"{"jsonrpc":"2.0","id":1}"#),
            Err(TerminalError::Protocol(_))
        ));
        assert!(matches!(
            parse_line(r#"{"jsonrpc":"1.0","id":1,"result":{}}"#),
            Err(TerminalError::Protocol(_))
        ));
        assert!(matches!(
            parse_line(r#"{"id":2,"error":{"code":-32000,"message":"x","data":null}}"#),
            Ok(Incoming::Response(response)) if matches!(response.payload, ResponsePayload::Error(_))
        ));
    }
}
