//! Token accounting from transcript files.
//!
//! Usage is read incrementally from the session transcript and its subagent
//! transcripts. A response may be split across several records that repeat the
//! same usage, so records are deduplicated by `message.id` within a session.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use utoipa::ToSchema;

use crate::error::ClaudeError;
use crate::model::Session;
use crate::paths::{ClaudePaths, find_transcript, is_safe_session_id};

mod price;
mod transcript;

pub use price::{ModelPrice, PRICE_TABLE, PRICE_TABLE_REVIEWED_ON, price_for};
use transcript::AssistantLine;

/// Model name used when a transcript record does not name one.
pub const UNKNOWN_MODEL: &str = "unknown";

/// Token counts for one model or one session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    /// Uncached input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Input tokens read from the prompt cache.
    pub cache_read_input_tokens: u64,
    /// Input tokens written to the prompt cache.
    pub cache_creation_input_tokens: u64,
}

impl TokenUsage {
    /// Sum of all four token classes.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_read_input_tokens)
            .saturating_add(self.cache_creation_input_tokens)
    }

    /// Adds another count into this one.
    pub fn add(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_input_tokens = self
            .cache_read_input_tokens
            .saturating_add(other.cache_read_input_tokens);
        self.cache_creation_input_tokens = self
            .cache_creation_input_tokens
            .saturating_add(other.cache_creation_input_tokens);
    }
}

/// Token usage and estimated cost for one model.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    /// Model id exactly as written in the transcript.
    pub model: String,
    /// Tokens attributed to this model.
    pub tokens: TokenUsage,
    /// Estimated cost, or `None` when the model is absent from the price table.
    pub cost_usd: Option<f64>,
}

/// Token usage and estimated cost for one session.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    /// Session the usage belongs to.
    pub session_id: String,
    /// Per-model usage, sorted by model id.
    pub models: Vec<ModelUsage>,
    /// Sum across all models.
    pub tokens: TokenUsage,
    /// Sum of priced models. Unpriced models contribute nothing.
    pub cost_usd: f64,
    /// Models that could not be priced, sorted.
    pub unpriced_models: Vec<String>,
    /// Number of distinct assistant responses counted.
    pub messages: usize,
}

impl SessionUsage {
    /// Usage with no tokens, for a session without a readable transcript.
    #[must_use]
    pub fn empty(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_owned(),
            models: Vec::new(),
            tokens: TokenUsage::default(),
            cost_usd: 0.0,
            unpriced_models: Vec::new(),
            messages: 0,
        }
    }
}

/// Incrementally accumulates usage from transcript files.
///
/// Keep one tracker per discovery source. Calling [`UsageTracker::update`]
/// again reads only appended bytes, and message ids seen in an earlier pass for
/// the same session are never counted twice. Call [`UsageTracker::forget`] when
/// a session ends so its state does not accumulate for the process lifetime.
#[derive(Debug, Default)]
pub struct UsageTracker {
    offsets: HashMap<(String, PathBuf), u64>,
    sessions: HashMap<String, SessionAccumulator>,
}

#[derive(Debug, Default)]
struct SessionAccumulator {
    models: HashMap<String, TokenUsage>,
    seen: HashSet<String>,
    messages: usize,
}

impl UsageTracker {
    /// Creates an empty tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads new transcript records for one session.
    ///
    /// Returns the number of newly counted assistant responses. A missing
    /// transcript is not an error; it yields zero.
    ///
    /// # Errors
    ///
    /// Returns [`ClaudeError::Io`] when a transcript exists but cannot be read.
    pub fn update(
        &mut self,
        paths: &ClaudePaths,
        session_id: &str,
        cwd: &str,
    ) -> Result<usize, ClaudeError> {
        if !is_safe_session_id(session_id) {
            return Ok(0);
        }
        let Some(transcript) = find_transcript(paths, session_id, cwd) else {
            return Ok(0);
        };

        let mut ingested = self.read_file(&transcript, session_id)?;
        for subagent in subagent_transcripts(&transcript, session_id) {
            ingested += self.read_file(&subagent, session_id)?;
        }
        Ok(ingested)
    }

    /// The accumulated usage for one session.
    #[must_use]
    pub fn session_usage(&self, session_id: &str) -> SessionUsage {
        let Some(accumulator) = self.sessions.get(session_id) else {
            return SessionUsage::empty(session_id);
        };

        let mut models: Vec<ModelUsage> = accumulator
            .models
            .iter()
            .filter(|(_, tokens)| tokens.total() > 0)
            .map(|(model, tokens)| ModelUsage {
                model: model.clone(),
                tokens: *tokens,
                cost_usd: price_for(model).map(|price| price.cost(tokens)),
            })
            .collect();
        models.sort_by(|left, right| left.model.cmp(&right.model));

        let mut tokens = TokenUsage::default();
        for model in &models {
            tokens.add(model.tokens);
        }
        let cost_usd = models
            .iter()
            .filter_map(|model| model.cost_usd)
            .sum::<f64>();
        let unpriced_models = models
            .iter()
            .filter(|model| model.cost_usd.is_none())
            .map(|model| model.model.clone())
            .collect();

        SessionUsage {
            session_id: session_id.to_owned(),
            models,
            tokens,
            cost_usd,
            unpriced_models,
            messages: accumulator.messages,
        }
    }

    /// Drops all accumulated state for one session.
    ///
    /// Use this when a session ends. A later [`UsageTracker::update`] for the
    /// same session rebuilds its totals from the transcript.
    pub fn forget(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
        self.offsets.retain(|(session, _), _| session != session_id);
    }

    /// Usage for every session seen so far, sorted by session id.
    #[must_use]
    pub fn report(&self) -> Vec<SessionUsage> {
        let mut session_ids: Vec<&String> = self.sessions.keys().collect();
        session_ids.sort();
        session_ids
            .into_iter()
            .map(|session_id| self.session_usage(session_id))
            .collect()
    }

    fn read_file(&mut self, path: &Path, session_id: &str) -> Result<usize, ClaudeError> {
        let key = (session_id.to_owned(), path.to_path_buf());
        let offset = self.offsets.entry(key).or_default();
        let lines = transcript::read_new_assistant_lines(path, offset)?;
        let mut ingested = 0;
        for line in lines {
            if self.ingest(session_id, line) {
                ingested += 1;
            }
        }
        Ok(ingested)
    }

    fn ingest(&mut self, session_id: &str, line: AssistantLine) -> bool {
        let accumulator = self.sessions.entry(session_id.to_owned()).or_default();
        if let Some(message_id) = &line.message_id
            && !accumulator.seen.insert(message_id.clone())
        {
            return false;
        }
        let model = line.model.unwrap_or_else(|| UNKNOWN_MODEL.to_owned());
        accumulator.messages = accumulator.messages.saturating_add(1);
        accumulator.models.entry(model).or_default().add(line.usage);
        true
    }
}

/// Collects usage for one session in a single pass.
///
/// # Errors
///
/// Returns [`ClaudeError::Io`] when a transcript exists but cannot be read.
pub fn collect_session_usage(
    paths: &ClaudePaths,
    session: &Session,
) -> Result<SessionUsage, ClaudeError> {
    let mut tracker = UsageTracker::new();
    tracker.update(paths, &session.session_id, &session.cwd)?;
    Ok(tracker.session_usage(&session.session_id))
}

fn subagent_transcripts(transcript: &Path, session_id: &str) -> Vec<PathBuf> {
    let Some(project_dir) = transcript.parent() else {
        return Vec::new();
    };
    let directory = project_dir.join(session_id).join("subagents");
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();
    files
}
