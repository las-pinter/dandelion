//! Argon2id password hashing.

use argon2::{Argon2, PasswordHasher as _, PasswordVerifier as _};
use thiserror::Error;

/// Minimum password length in characters.
pub const MIN_PASSWORD_CHARS: usize = 12;

/// Password failures.
#[derive(Debug, Error)]
pub enum PasswordError {
    /// The password is shorter than the minimum.
    #[error("password must be at least {MIN_PASSWORD_CHARS} characters")]
    TooShort,
    /// Hashing failed.
    #[error("cannot hash the password: {0}")]
    Hash(String),
}

/// Enforces the minimum length before hashing.
pub fn validate(password: &str) -> Result<(), PasswordError> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(PasswordError::TooShort);
    }
    Ok(())
}

/// Hashes a password into a PHC string with argon2id and default parameters.
pub fn hash(password: &str) -> Result<String, PasswordError> {
    validate(password)?;
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| PasswordError::Hash(error.to_string()))
}

/// Checks a password against a stored PHC string.
///
/// An unreadable stored hash is a failed verification, never a match.
#[must_use]
pub fn verify(password: &str, stored: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), stored)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::{MIN_PASSWORD_CHARS, PasswordError, hash, validate, verify};

    const PASSWORD: &str = "correct horse battery";

    #[test]
    fn hashes_and_verifies_a_password() {
        let stored = hash(PASSWORD).expect("hash");

        assert!(stored.starts_with("$argon2id$"));
        assert!(verify(PASSWORD, &stored));
        assert!(!verify("wrong password here", &stored));
    }

    #[test]
    fn rejects_short_passwords() {
        let short = "a".repeat(MIN_PASSWORD_CHARS - 1);

        assert!(matches!(validate(&short), Err(PasswordError::TooShort)));
        assert!(hash(&short).is_err());
        assert!(validate(&"a".repeat(MIN_PASSWORD_CHARS)).is_ok());
    }

    #[test]
    fn counts_characters_not_bytes() {
        let password = "é".repeat(MIN_PASSWORD_CHARS);

        assert!(validate(&password).is_ok());
    }

    #[test]
    fn an_unreadable_hash_never_matches() {
        assert!(!verify(PASSWORD, "not a phc string"));
    }

    #[test]
    fn each_hash_uses_a_fresh_salt() {
        let first = hash(PASSWORD).expect("first hash");
        let second = hash(PASSWORD).expect("second hash");

        assert_ne!(first, second);
        assert!(verify(PASSWORD, &first));
        assert!(verify(PASSWORD, &second));
    }
}
