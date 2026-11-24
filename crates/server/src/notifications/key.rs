//! The VAPID key pair that signs every push.

use std::{
    fmt, fs,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use web_push::{SubscriptionInfo, VapidSignature, VapidSignatureBuilder};

/// Why the VAPID key could not be loaded or created.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The key file could not be read or written.
    #[error("cannot read or write the VAPID key: {0}")]
    Io(#[from] io::Error),
    /// The key file is not the JSON this server writes.
    #[error("the VAPID key file is not valid: {0}")]
    Invalid(String),
    /// The system random source failed.
    #[error("cannot generate a VAPID key: {0}")]
    Random(String),
    /// The key is not a valid P-256 private key.
    #[error(transparent)]
    WebPush(#[from] web_push::WebPushError),
}

/// The on-disk form of the key pair.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    private_key: String,
    public_key: String,
}

/// A P-256 key pair in the base64url form web push expects.
#[derive(Clone)]
pub struct VapidKey {
    private: String,
    public: String,
}

impl fmt::Debug for VapidKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VapidKey")
            .field("private", &"<redacted>")
            .field("public", &self.public)
            .finish()
    }
}

impl VapidKey {
    /// Loads `<data-dir>/vapid.json`, creating it with owner-only permissions
    /// on first start.
    ///
    /// # Errors
    ///
    /// Returns [`KeyError`] when the file cannot be read or written, or the
    /// stored key is not valid.
    pub fn load_or_create(path: &Path) -> Result<Self, KeyError> {
        match fs::read(path) {
            Ok(bytes) => Self::from_bytes(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let key = Self::generate()?;
                let stored = Stored {
                    private_key: key.private.clone(),
                    public_key: key.public.clone(),
                };
                let bytes = serde_json::to_vec(&stored)
                    .map_err(|error| KeyError::Invalid(error.to_string()))?;
                match Self::write_new(path, &bytes)? {
                    None => Ok(key),
                    // Another process created the key first; use theirs.
                    Some(existing) => Self::from_bytes(&existing),
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Writes a new key file without ever replacing an existing one.
    ///
    /// The bytes go to a temporary file first, so an interrupted write leaves
    /// no half-written `vapid.json` behind. Returns the existing bytes when
    /// another process created the key first.
    fn write_new(path: &Path, bytes: &[u8]) -> Result<Option<Vec<u8>>, KeyError> {
        let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
        let write = (|| -> io::Result<()> {
            // Create with 0600 from the first byte, never world-readable.
            let open = || {
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&temporary)
            };
            let mut file = match open() {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    // A stale file from an interrupted first start.
                    fs::remove_file(&temporary)?;
                    open()?
                }
                result => result?,
            };
            file.write_all(bytes)?;
            file.sync_all()
        })();
        if let Err(error) = write {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        // A hard link publishes the finished file atomically and fails rather
        // than replacing a key another process already wrote.
        let published = fs::hard_link(&temporary, path);
        let _ = fs::remove_file(&temporary);
        match published {
            Ok(()) => Ok(None),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(Some(fs::read(path)?)),
            Err(error) => Err(error.into()),
        }
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, KeyError> {
        let stored: Stored =
            serde_json::from_slice(bytes).map_err(|error| KeyError::Invalid(error.to_string()))?;
        Self::from_parts(stored.private_key, stored.public_key)
    }

    /// Generates a fresh key pair.
    ///
    /// # Errors
    ///
    /// Returns [`KeyError::Random`] when the random source fails and
    /// [`KeyError::WebPush`] when the draw is not a valid scalar.
    pub fn generate() -> Result<Self, KeyError> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|error| KeyError::Random(error.to_string()))?;
        let private = URL_SAFE_NO_PAD.encode(bytes);
        let public = URL_SAFE_NO_PAD
            .encode(VapidSignatureBuilder::from_base64_no_sub(&private)?.get_public_key());
        Ok(Self { private, public })
    }

    /// The base64url public key a browser subscribes with.
    #[must_use]
    pub fn public_key(&self) -> &str {
        &self.public
    }

    /// Signs one push message for a subscription.
    pub(crate) fn signature(
        &self,
        subscription: &SubscriptionInfo,
        subject: &str,
    ) -> Result<VapidSignature, web_push::WebPushError> {
        let mut builder =
            VapidSignatureBuilder::from_base64_no_sub(&self.private)?.add_sub_info(subscription);
        builder.add_claim("sub", subject);
        builder.build()
    }

    fn from_parts(private: String, public: String) -> Result<Self, KeyError> {
        // Rebuild the public key so a corrupted stored pair is caught here.
        let derived = URL_SAFE_NO_PAD
            .encode(VapidSignatureBuilder::from_base64_no_sub(&private)?.get_public_key());
        if derived != public {
            return Err(KeyError::Invalid(
                "the stored public key does not match the private key".to_owned(),
            ));
        }
        Ok(Self { private, public })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use super::VapidKey;

    #[test]
    fn creates_the_key_once_with_owner_only_permissions() -> Result<(), Box<dyn std::error::Error>>
    {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().join("vapid.json");

        let first = VapidKey::load_or_create(&path)?;
        let second = VapidKey::load_or_create(&path)?;

        assert_eq!(first.public_key(), second.public_key());
        assert_ne!(first.public_key(), "");
        let mode = fs::metadata(&path)?.permissions();
        assert_eq!(mode.mode() & 0o777, 0o600);
        assert_eq!(
            fs::read_dir(temporary.path())?.count(),
            1,
            "the temporary file is gone"
        );

        Ok(())
    }

    #[test]
    fn rejects_an_invalid_file() -> Result<(), Box<dyn std::error::Error>> {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().join("vapid.json");
        fs::write(&path, b"not json")?;

        assert!(VapidKey::load_or_create(&path).is_err());

        Ok(())
    }

    #[test]
    fn a_generated_key_derives_its_public_half() -> Result<(), Box<dyn std::error::Error>> {
        let key = VapidKey::generate()?;

        assert_ne!(key.public_key(), "");
        let printed = format!("{key:?}");
        assert!(printed.contains("<redacted>"));
        assert!(
            !printed.contains(&key.private),
            "the private key never prints"
        );

        Ok(())
    }
}
