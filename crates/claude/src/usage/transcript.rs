//! Incremental reading of transcript JSONL files.
//!
//! Only the fields needed for token accounting are deserialized. Message
//! content is ignored by serde and never enters this crate's memory model
//! beyond the line buffer, which is discarded after each line.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;

use super::TokenUsage;
use crate::error::ClaudeError;

/// One assistant response, reduced to its accounting fields.
#[derive(Debug)]
pub(crate) struct AssistantLine {
    pub message_id: Option<String>,
    pub model: Option<String>,
    pub usage: TokenUsage,
}

#[derive(Debug, Deserialize)]
struct TranscriptRecord {
    #[serde(rename = "type")]
    kind: Option<String>,
    message: Option<TranscriptMessage>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    id: Option<String>,
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Debug, Default, Deserialize)]
struct RawUsage {
    #[serde(rename = "input_tokens")]
    input: Option<u64>,
    #[serde(rename = "output_tokens")]
    output: Option<u64>,
    #[serde(rename = "cache_read_input_tokens")]
    cache_read: Option<u64>,
    #[serde(rename = "cache_creation_input_tokens")]
    cache_creation: Option<u64>,
}

impl From<RawUsage> for TokenUsage {
    fn from(raw: RawUsage) -> Self {
        Self {
            input_tokens: raw.input.unwrap_or_default(),
            output_tokens: raw.output.unwrap_or_default(),
            cache_read_input_tokens: raw.cache_read.unwrap_or_default(),
            cache_creation_input_tokens: raw.cache_creation.unwrap_or_default(),
        }
    }
}

/// Reads complete new lines from `path`, advancing `offset` only on success.
///
/// A trailing line without a newline is left for the next call, so a write in
/// progress is never counted twice or half-parsed. When the file has shrunk
/// below `offset` it is treated as a new file and read from the beginning. If
/// reading fails, `offset` is left untouched so the same bytes are retried.
pub(crate) fn read_new_assistant_lines(
    path: &Path,
    offset: &mut u64,
) -> Result<Vec<AssistantLine>, ClaudeError> {
    let file = File::open(path).map_err(|error| ClaudeError::io(path, error))?;
    let length = file
        .metadata()
        .map_err(|error| ClaudeError::io(path, error))?
        .len();
    if length < *offset {
        // The file was replaced, so the old position is meaningless even if
        // the read below fails.
        *offset = 0;
    }

    let mut start = *offset;
    let mut file = file;
    file.seek(SeekFrom::Start(start))
        .map_err(|error| ClaudeError::io(path, error))?;
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    let mut lines = Vec::new();

    loop {
        buffer.clear();
        let read = reader
            .read_until(b'\n', &mut buffer)
            .map_err(|error| ClaudeError::io(path, error))?;
        if read == 0 {
            break;
        }
        if buffer.last() != Some(&b'\n') {
            break;
        }
        let consumed = u64::try_from(read).unwrap_or(u64::MAX);
        start = start.saturating_add(consumed);
        if let Some(line) = parse_line(&buffer) {
            lines.push(line);
        }
    }

    *offset = start;
    Ok(lines)
}

fn parse_line(bytes: &[u8]) -> Option<AssistantLine> {
    let record: TranscriptRecord = serde_json::from_slice(bytes).ok()?;
    if record.kind.as_deref() != Some("assistant") {
        return None;
    }
    let message = record.message?;
    let usage = TokenUsage::from(message.usage?);
    Some(AssistantLine {
        message_id: message.id,
        model: message.model,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn ignores_non_assistant_records() {
        let line = br#"{"type":"user","message":{"usage":{"input_tokens":5}}}"#;

        assert!(parse_line(line).is_none());
    }

    #[test]
    fn ignores_assistant_records_without_usage() {
        let line = br#"{"type":"assistant","message":{"id":"m1","model":"claude-opus-5-5"}}"#;

        assert!(parse_line(line).is_none());
    }

    #[test]
    fn reads_usage_and_model() {
        let line = br#"{"type":"assistant","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":30,"cache_creation_input_tokens":40}}}"#;

        let parsed = parse_line(line);

        let Some(parsed) = parsed else {
            panic!("expected an assistant line");
        };
        assert_eq!(parsed.message_id.as_deref(), Some("m1"));
        assert_eq!(parsed.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(parsed.usage.input_tokens, 10);
        assert_eq!(parsed.usage.output_tokens, 20);
        assert_eq!(parsed.usage.cache_read_input_tokens, 30);
        assert_eq!(parsed.usage.cache_creation_input_tokens, 40);
    }

    #[test]
    fn malformed_line_is_skipped() {
        assert!(parse_line(b"{not json").is_none());
    }
}
