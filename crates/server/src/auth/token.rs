//! Random tokens and their at-rest hashes.
//!
//! Session and setup tokens are secrets. Only a SHA-256 hash is stored, so a
//! leaked database copy cannot be replayed against the API.

use std::fmt::Write as _;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Entropy of a token in bytes.
const TOKEN_BYTES: usize = 32;

/// Token generation failures.
#[derive(Debug, Error)]
pub enum TokenError {
    /// The operating system random source failed.
    #[error("cannot read random bytes: {0}")]
    Random(String),
}

/// Returns a fresh 256-bit token encoded as unpadded base64url.
pub fn random_token() -> Result<String, TokenError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| TokenError::Random(error.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Returns the lowercase hex SHA-256 of a token.
#[must_use]
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{hash_token, random_token};

    #[test]
    fn tokens_are_unique_and_url_safe() {
        let mut seen = HashSet::new();
        for _ in 0..32 {
            let token = random_token().expect("random token");
            assert_eq!(token.len(), 43);
            assert!(
                token
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            );
            assert!(seen.insert(token));
        }
    }

    #[test]
    fn hashes_are_stable_and_hex() {
        let hash = hash_token("example-token");

        assert_eq!(hash, hash_token("example-token"));
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(hash, "example-token");
    }
}
