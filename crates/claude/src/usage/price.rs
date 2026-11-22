//! Token prices used to estimate session cost.
//!
//! This table is maintained by hand and must be reviewed when Anthropic
//! changes prices or ships a model. Rates are US dollars per million tokens
//! for the first-party API.
//!
//! Assumptions:
//! - Cache writes are priced at the five-minute cache rate.
//! - The base tier is used for models whose price depends on prompt length.
//! - Models missing from the table are reported without a cost rather than
//!   estimated from a similar model.

use serde::Serialize;
use utoipa::ToSchema;

use super::TokenUsage;

/// The date the price table was last reviewed against published rates.
pub const PRICE_TABLE_REVIEWED_ON: &str = "2026-11-03";

/// Per-million-token rates for one model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    /// Uncached input tokens.
    pub input_per_mtok: f64,
    /// Output tokens.
    pub output_per_mtok: f64,
    /// Input tokens read from the prompt cache.
    pub cache_read_per_mtok: f64,
    /// Input tokens written to the prompt cache.
    pub cache_write_per_mtok: f64,
}

impl ModelPrice {
    /// Creates a price entry.
    #[must_use]
    pub const fn new(
        input_per_mtok: f64,
        output_per_mtok: f64,
        cache_read_per_mtok: f64,
        cache_write_per_mtok: f64,
    ) -> Self {
        Self {
            input_per_mtok,
            output_per_mtok,
            cache_read_per_mtok,
            cache_write_per_mtok,
        }
    }

    /// Cost in US dollars for a token count at these rates.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "token totals are far below 2^53 and precision beyond that is immaterial to a cost estimate"
    )]
    pub fn cost(&self, usage: &TokenUsage) -> f64 {
        const PER_MILLION: f64 = 1_000_000.0;
        let input = usage.input_tokens as f64 * self.input_per_mtok;
        let output = usage.output_tokens as f64 * self.output_per_mtok;
        let cache_read = usage.cache_read_input_tokens as f64 * self.cache_read_per_mtok;
        let cache_write = usage.cache_creation_input_tokens as f64 * self.cache_write_per_mtok;
        (input + output + cache_read + cache_write) / PER_MILLION
    }
}

/// Published model rates. Update both the rates and
/// [`PRICE_TABLE_REVIEWED_ON`] in the same change.
pub const PRICE_TABLE: &[(&str, ModelPrice)] = &[
    ("claude-fable-5-2", ModelPrice::new(8.0, 40.0, 0.20, 10.0)),
    ("claude-mythos-5-2", ModelPrice::new(8.0, 40.0, 0.20, 10.0)),
    (
        "claude-mythos-5-1",
        ModelPrice::new(10.0, 50.0, 1.00, 12.50),
    ),
    ("claude-fable-5-1", ModelPrice::new(10.0, 50.0, 1.00, 12.50)),
    ("claude-fable-5", ModelPrice::new(10.0, 50.0, 1.00, 12.50)),
    ("claude-opus-5-5", ModelPrice::new(4.0, 20.0, 0.20, 5.00)),
    ("claude-opus-5", ModelPrice::new(5.0, 25.0, 0.50, 6.25)),
    ("claude-opus-4-8", ModelPrice::new(5.0, 25.0, 0.50, 6.25)),
    ("claude-opus-4-7", ModelPrice::new(5.0, 25.0, 0.50, 6.25)),
    ("claude-opus-4-6", ModelPrice::new(5.0, 25.0, 0.50, 6.25)),
    ("claude-sonnet-5-5", ModelPrice::new(2.0, 10.0, 0.10, 2.50)),
    ("claude-sonnet-5", ModelPrice::new(2.0, 10.0, 0.20, 2.50)),
    ("claude-sonnet-4-6", ModelPrice::new(3.0, 15.0, 0.30, 3.75)),
    ("claude-haiku-5-5", ModelPrice::new(0.10, 0.50, 0.01, 0.125)),
    ("claude-haiku-4-5", ModelPrice::new(1.0, 5.0, 0.10, 1.25)),
];

/// The price entry for an exact model id, if the table has one.
#[must_use]
pub fn price_for(model: &str) -> Option<ModelPrice> {
    PRICE_TABLE
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, price)| *price)
}

#[cfg(test)]
mod tests {
    use super::{ModelPrice, price_for};
    use crate::usage::TokenUsage;

    #[test]
    fn known_model_has_a_price() {
        let price = price_for("claude-opus-5-5");

        assert_eq!(price.map(|value| value.input_per_mtok), Some(4.0));
    }

    #[test]
    fn unknown_model_has_no_price() {
        assert_eq!(price_for("some-future-model"), None);
    }

    #[test]
    fn cost_is_the_sum_of_all_four_token_classes() {
        let price = ModelPrice::new(1.0, 2.0, 0.5, 1.5);
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_input_tokens: 1_000_000,
            cache_creation_input_tokens: 1_000_000,
        };

        let cost = price.cost(&usage);

        assert!((cost - 5.0).abs() < f64::EPSILON);
    }
}
