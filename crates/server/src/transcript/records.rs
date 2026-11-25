//! Turns one Claude Code transcript line into what the transcript tab shows.
//!
//! The file is JSONL with many bookkeeping record types; only messages, tool
//! calls, tool results, and compaction boundaries carry something to read.
//! Anything unrecognized is skipped, never fatal. Text is bounded so one huge
//! tool result cannot bloat a socket frame.

use serde::Serialize;
use serde_json::Value;

/// Longest text kept for one block, in bytes.
pub const TEXT_LIMIT: usize = 8 * 1024;
/// Longest line parsed into a record, in bytes.
///
/// A line is held whole before it can be bounded for display, so a runaway
/// record must be refused before it is deserialized.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
/// Longest first-line tool summary, in characters.
const SUMMARY_LIMIT: usize = 120;

/// One rendered record from the transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// Byte offset of the line inside the transcript file.
    pub offset: u64,
    /// `user`, `assistant`, or `system`.
    pub role: String,
    /// The record's own timestamp, when it has one.
    pub at: Option<String>,
    /// The record's displayable parts, in order.
    pub blocks: Vec<Block>,
}

/// One displayable part of a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Block {
    /// Plain text, rendered as markdown.
    Text {
        /// The text.
        text: String,
    },
    /// The model's reasoning, collapsed by default.
    Thinking {
        /// The thinking text.
        text: String,
    },
    /// One tool call.
    ToolUse {
        /// The transcript's tool-use id.
        id: String,
        /// The tool name.
        name: String,
        /// One-line summary of the input.
        summary: String,
        /// The pretty-printed input, bounded.
        input: String,
    },
    /// The result of a tool call.
    ToolResult {
        /// The tool-use id this answers.
        tool_use_id: Option<String>,
        /// Whether the tool failed.
        is_error: bool,
        /// The result text, bounded.
        text: String,
    },
    /// A structural marker such as a compaction boundary.
    Divider {
        /// The label to show.
        label: String,
    },
}

/// Parses one line; `None` for blank, malformed, or bookkeeping records.
#[must_use]
pub fn parse_line(line: &str) -> Option<Record> {
    if line.len() > MAX_LINE_BYTES {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    let kind = value.get("type")?.as_str()?;
    let at = value
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match kind {
        "user" => user_record(value.get("message")?, at),
        "assistant" => assistant_record(value.get("message")?, at),
        "system" => system_record(&value, at),
        _ => None,
    }
}

fn user_record(message: &Value, at: Option<String>) -> Option<Record> {
    let content = message.get("content")?;
    let mut blocks = Vec::new();
    match content {
        Value::String(text) => blocks.push(Block::Text {
            text: bounded(text),
        }),
        Value::Array(items) => {
            for item in items {
                match item.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            blocks.push(Block::Text {
                                text: bounded(text),
                            });
                        }
                    }
                    Some("tool_result") => blocks.push(Block::ToolResult {
                        tool_use_id: item
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        is_error: item
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        text: content_text(item.get("content").unwrap_or(&Value::Null)),
                    }),
                    _ => {}
                }
            }
        }
        _ => return None,
    }
    finish("user", at, blocks)
}

fn assistant_record(message: &Value, at: Option<String>) -> Option<Record> {
    let content = message.get("content")?;
    let mut blocks = Vec::new();
    match content {
        Value::String(text) => blocks.push(Block::Text {
            text: bounded(text),
        }),
        Value::Array(items) => {
            for item in items {
                match item.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = item.get("text").and_then(Value::as_str) {
                            blocks.push(Block::Text {
                                text: bounded(text),
                            });
                        }
                    }
                    Some("thinking") => {
                        if let Some(text) = item.get("thinking").and_then(Value::as_str) {
                            blocks.push(Block::Thinking {
                                text: bounded(text),
                            });
                        }
                    }
                    Some("tool_use") => {
                        let input = item.get("input").unwrap_or(&Value::Null);
                        blocks.push(Block::ToolUse {
                            id: item
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            name: item
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_owned(),
                            summary: summarize(input),
                            input: pretty_bounded(input),
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => return None,
    }
    finish("assistant", at, blocks)
}

fn system_record(value: &Value, at: Option<String>) -> Option<Record> {
    match value.get("subtype").and_then(Value::as_str) {
        Some("compact_boundary") => Some(Record {
            offset: 0,
            role: "system".to_owned(),
            at,
            blocks: vec![Block::Divider {
                label: "Conversation compacted".to_owned(),
            }],
        }),
        _ => None,
    }
}

fn finish(role: &str, at: Option<String>, blocks: Vec<Block>) -> Option<Record> {
    (!blocks.is_empty()).then(|| Record {
        offset: 0,
        role: role.to_owned(),
        at,
        blocks,
    })
}

/// Plain text from a tool-result content value, bounded: a string, or text
/// parts joined, stopping once the bound is reached so a huge result is not
/// copied whole.
fn content_text(value: &Value) -> String {
    match value {
        Value::String(text) => maybe_bound(text),
        Value::Array(items) => {
            let mut joined = String::new();
            for item in items {
                let Some(text) = item.get("text").and_then(Value::as_str) else {
                    continue;
                };
                if !joined.is_empty() {
                    joined.push('\n');
                }
                joined.push_str(&maybe_bound(text));
                if joined.len() > TEXT_LIMIT {
                    break;
                }
            }
            maybe_bound(&joined)
        }
        Value::Null => String::new(),
        other => maybe_bound(&other.to_string()),
    }
}

/// A short first-line summary of a tool input.
fn summarize(input: &Value) -> String {
    const KEYS: [&str; 9] = [
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "query",
        "description",
        "prompt",
        "name",
    ];
    let text = KEYS
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .map_or_else(|| compact(input), str::to_owned);
    let line = text.lines().next().unwrap_or_default().trim();
    let mut summary: String = line.chars().take(SUMMARY_LIMIT).collect();
    if line.chars().count() > SUMMARY_LIMIT {
        summary.push('…');
    }
    summary
}

/// One length-bounded line of compact JSON.
fn compact(value: &Value) -> String {
    truncate(&value.to_string(), SUMMARY_LIMIT * 4)
}

/// Pretty JSON for the expanded tool input, bounded.
///
/// A huge input is emitted compactly, because pretty-printing first would
/// build a much larger string only to throw most of it away.
fn pretty_bounded(value: &Value) -> String {
    let compact = value.to_string();
    if compact.len() > TEXT_LIMIT {
        return maybe_bound(&compact);
    }
    maybe_bound(&serde_json::to_string_pretty(value).unwrap_or(compact))
}

/// Bounds a string to [`TEXT_LIMIT`] bytes on a character boundary.
fn bounded(text: &str) -> String {
    maybe_bound(text)
}

fn maybe_bound(text: &str) -> String {
    if text.len() <= TEXT_LIMIT {
        return text.to_owned();
    }
    let mut end = TEXT_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… (truncated)", &text[..end])
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{Block, MAX_LINE_BYTES, Record, TEXT_LIMIT, parse_line};

    #[test]
    fn an_oversized_line_is_skipped_before_parsing() {
        let line = format!(
            r#"{{"type":"user","message":{{"content":"{}"}}}}"#,
            "x".repeat(MAX_LINE_BYTES)
        );

        assert!(parse_line(&line).is_none());
    }

    #[test]
    fn block_fields_travel_in_camel_case() {
        let record = Record {
            offset: 0,
            role: "user".to_owned(),
            at: None,
            blocks: vec![Block::ToolResult {
                tool_use_id: Some("toolu_1".to_owned()),
                is_error: true,
                text: "failed".to_owned(),
            }],
        };

        let json = serde_json::to_string(&record).expect("json");

        assert!(json.contains(r#""kind":"tool_result""#), "{json}");
        assert!(json.contains(r#""toolUseId":"toolu_1""#), "{json}");
        assert!(json.contains(r#""isError":true"#), "{json}");
        assert!(!json.contains("tool_use_id"), "{json}");
    }

    #[test]
    fn a_user_prompt_becomes_text() {
        let record = parse_line(
            r#"{"type":"user","message":{"role":"user","content":"run the tests"},"timestamp":"2026-11-25T10:00:00Z"}"#,
        )
        .expect("a record");

        assert_eq!(record.role, "user");
        assert_eq!(record.at.as_deref(), Some("2026-11-25T10:00:00Z"));
        assert_eq!(
            record.blocks,
            vec![Block::Text {
                text: "run the tests".to_owned()
            }]
        );
    }

    #[test]
    fn an_assistant_turn_keeps_text_thinking_and_tool_calls() {
        let line = r#"{"type":"assistant","message":{"content":[
            {"type":"thinking","thinking":"check first"},
            {"type":"text","text":"Running the tests."},
            {"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"just test"}}
        ]}}"#;

        let record = parse_line(line).expect("a record");

        assert_eq!(record.role, "assistant");
        let Block::Thinking { text } = &record.blocks[0] else {
            panic!("thinking first");
        };
        assert_eq!(text, "check first");
        let Block::Text { text } = &record.blocks[1] else {
            panic!("then text");
        };
        assert_eq!(text, "Running the tests.");
        let Block::ToolUse {
            id, name, summary, ..
        } = &record.blocks[2]
        else {
            panic!("then the tool call");
        };
        assert_eq!(id, "toolu_1");
        assert_eq!(name, "Bash");
        assert_eq!(summary, "just test");
    }

    #[test]
    fn a_tool_result_keeps_its_error_flag_and_text() {
        let line = r#"{"type":"user","message":{"content":[
            {"type":"tool_result","tool_use_id":"toolu_1","is_error":true,
             "content":[{"type":"text","text":"command failed"}]}
        ]}}"#;

        let record = parse_line(line).expect("a record");

        assert_eq!(
            record.blocks,
            vec![Block::ToolResult {
                tool_use_id: Some("toolu_1".to_owned()),
                is_error: true,
                text: "command failed".to_owned(),
            }]
        );
    }

    #[test]
    fn a_compaction_boundary_becomes_a_divider() {
        let record =
            parse_line(r#"{"type":"system","subtype":"compact_boundary"}"#).expect("a record");

        assert_eq!(record.role, "system");
        assert_eq!(
            record.blocks,
            vec![Block::Divider {
                label: "Conversation compacted".to_owned()
            }]
        );
    }

    #[test]
    fn bookkeeping_and_unknown_records_are_skipped() {
        assert!(parse_line(r#"{"type":"cost-state","totalCostUSD":1.5}"#).is_none());
        assert!(parse_line(r#"{"type":"system","subtype":"turn_duration"}"#).is_none());
        assert!(parse_line("not json").is_none());
        assert!(parse_line("").is_none());
    }

    #[test]
    fn oversized_text_is_truncated_on_a_character_boundary() {
        let long = "é".repeat(TEXT_LIMIT);
        let line = format!(r#"{{"type":"user","message":{{"content":"{long}"}}}}"#);

        let record = parse_line(&line).expect("a record");
        let Block::Text { text } = &record.blocks[0] else {
            panic!("text");
        };

        assert!(text.ends_with("… (truncated)"));
        assert!(text.len() <= TEXT_LIMIT + 32);
        assert!(text.is_char_boundary(text.len() - "… (truncated)".len()));
    }
}
