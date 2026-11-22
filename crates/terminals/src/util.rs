//! Small helpers shared by both backends.

use std::time::Duration;

/// How long one non-streaming backend call may take.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns the first candidate that is not blank.
pub(crate) fn first_non_empty<const N: usize>(candidates: [&str; N]) -> String {
    candidates
        .into_iter()
        .find(|candidate| !candidate.trim().is_empty())
        .unwrap_or_default()
        .to_owned()
}
