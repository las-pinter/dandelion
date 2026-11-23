//! In-memory `WebAuthn` ceremony state.
//!
//! `webauthn-rs` requires the server to keep the challenge state between the
//! start and finish calls. Keeping it in memory (rather than the database)
//! means a restart simply asks the browser to begin again.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration};

/// How long a client has to finish a ceremony.
pub const TTL: Duration = Duration::from_secs(300);
/// Upper bound on unfinished ceremonies, so abandoned tabs cannot grow memory.
const MAX_ENTRIES: usize = 32;

/// A pending ceremony.
#[derive(Debug)]
enum Ceremony {
    Registration {
        token_hash: String,
        state: PasskeyRegistration,
        created: Instant,
    },
    Authentication {
        state: PasskeyAuthentication,
        created: Instant,
    },
}

impl Ceremony {
    fn created(&self) -> Instant {
        match self {
            Self::Registration { created, .. } | Self::Authentication { created, .. } => *created,
        }
    }

    fn expired(&self, now: Instant) -> bool {
        now.duration_since(self.created()) >= TTL
    }
}

/// Ceremonies shared by every request.
#[derive(Debug, Clone, Default)]
pub struct CeremonyStore {
    inner: Arc<Mutex<HashMap<Uuid, Ceremony>>>,
}

impl CeremonyStore {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a registration ceremony and returns its id.
    #[must_use]
    pub fn insert_registration(&self, token_hash: String, state: PasskeyRegistration) -> Uuid {
        self.insert_registration_at(token_hash, state, Instant::now())
    }

    fn insert_registration_at(
        &self,
        token_hash: String,
        state: PasskeyRegistration,
        created: Instant,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let mut entries = self.lock();
        purge(&mut entries, Instant::now());
        evict_oldest(&mut entries);
        entries.insert(
            id,
            Ceremony::Registration {
                token_hash,
                state,
                created,
            },
        );
        id
    }

    /// Takes a registration ceremony, refusing expired or mismatched kinds.
    #[must_use]
    pub fn take_registration(&self, id: Uuid) -> Option<(String, PasskeyRegistration)> {
        match self.lock().remove(&id)? {
            Ceremony::Registration {
                token_hash,
                state,
                created,
            } if created.elapsed() < TTL => Some((token_hash, state)),
            _ => None,
        }
    }

    /// Stores an authentication ceremony and returns its id.
    #[must_use]
    pub fn insert_authentication(&self, state: PasskeyAuthentication) -> Uuid {
        let id = Uuid::new_v4();
        let mut entries = self.lock();
        purge(&mut entries, Instant::now());
        evict_oldest(&mut entries);
        entries.insert(
            id,
            Ceremony::Authentication {
                state,
                created: Instant::now(),
            },
        );
        id
    }

    /// Takes an authentication ceremony, refusing expired or mismatched kinds.
    #[must_use]
    pub fn take_authentication(&self, id: Uuid) -> Option<PasskeyAuthentication> {
        match self.lock().remove(&id)? {
            Ceremony::Authentication { state, created } if created.elapsed() < TTL => Some(state),
            _ => None,
        }
    }

    /// Number of stored ceremonies; used by tests.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Uuid, Ceremony>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn purge(entries: &mut HashMap<Uuid, Ceremony>, now: Instant) {
    entries.retain(|_, ceremony| !ceremony.expired(now));
}

fn evict_oldest(entries: &mut HashMap<Uuid, Ceremony>) {
    while entries.len() >= MAX_ENTRIES {
        let Some(oldest) = entries
            .iter()
            .min_by_key(|(_, ceremony)| ceremony.created())
            .map(|(id, _)| *id)
        else {
            return;
        };
        entries.remove(&oldest);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use url::Url;
    use uuid::Uuid;
    use webauthn_rs::prelude::{
        PasskeyAuthentication, PasskeyRegistration, Webauthn, WebauthnBuilder,
    };

    use super::{CeremonyStore, MAX_ENTRIES, TTL};

    fn webauthn() -> Webauthn {
        let origin = Url::parse("https://example.test").expect("origin");
        WebauthnBuilder::new("example.test", &origin)
            .expect("builder")
            .rp_name("Dandelion")
            .build()
            .expect("webauthn")
    }

    fn registration() -> PasskeyRegistration {
        webauthn()
            .start_passkey_registration(Uuid::new_v4(), "owner", "Owner", None)
            .expect("start registration")
            .1
    }

    fn authentication() -> PasskeyAuthentication {
        // An authentication ceremony over no credentials still gives state to store.
        webauthn()
            .start_passkey_authentication(&[])
            .expect("start authentication")
            .1
    }

    #[test]
    fn registrations_are_taken_once() {
        let store = CeremonyStore::new();
        let id = store.insert_registration("token-hash".to_owned(), registration());

        let taken = store.take_registration(id).expect("first take");

        assert_eq!(taken.0, "token-hash");
        assert!(store.take_registration(id).is_none());
    }

    #[test]
    fn ceremonies_do_not_cross_kinds() {
        let store = CeremonyStore::new();
        let id = store.insert_authentication(authentication());

        assert!(store.take_registration(id).is_none());
        assert!(store.take_authentication(id).is_none());
    }

    #[test]
    fn expired_registrations_are_refused() {
        let store = CeremonyStore::new();
        let created = Instant::now()
            .checked_sub(TTL + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        let id = store.insert_registration_at("token-hash".to_owned(), registration(), created);

        assert!(store.take_registration(id).is_none());
    }

    #[test]
    fn the_store_is_bounded() {
        let store = CeremonyStore::new();
        for _ in 0..(MAX_ENTRIES + 4) {
            let _ = store.insert_authentication(authentication());
        }

        assert_eq!(store.len(), MAX_ENTRIES);
    }
}
