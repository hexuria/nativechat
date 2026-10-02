//! The key for this computer's opencodex, which the relay sends with every call it makes there
//! (`opengrok::relay`). It is the person's, for a proxy on this computer, and it is kept in the
//! macOS Keychain and nowhere else: not sqlite, not a file, not a log, and never sent to the
//! server, whose relay frames carry no key at all (opengrok-server #292). Settings → Relay writes
//! it and says whether one is kept; only the relay reads it back, when it starts.

use std::sync::{Arc, Mutex, PoisonError};

use crate::opengrok::RelayKey;

/// The Keychain service the key is kept under, apart from the site logins'.
pub const KEYCHAIN_SERVICE: &str = "ai.nativechat.opencodex";

/// Its account: there is one opencodex on a Mac, and one key for it.
#[cfg(target_os = "macos")]
const KEYCHAIN_ACCOUNT: &str = "relay";

/// Where the key is kept.
pub trait RelayKeyStore: Send + Sync {
    /// Whether a key is kept, asked without reading it, so asking puts up no password sheet.
    fn holds_key(&self) -> bool;
    /// The key, for the relay as it starts.
    fn read(&self) -> Result<Option<RelayKey>, String>;
    /// Keep this key in place of any other.
    fn keep(&self, key: &RelayKey) -> Result<(), String>;
    /// Forget the key; forgetting none is not a failure.
    fn forget(&self) -> Result<(), String>;
}

/// This Mac's Keychain, where the app runs. Anywhere else, and in tests, the key lives in memory
/// for as long as the app does, and is never written down.
pub fn open_store() -> Arc<dyn RelayKeyStore> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(KeychainKeyStore)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(MemoryKeyStore::default())
    }
}

/// A key kept in memory alone.
#[derive(Default)]
pub struct MemoryKeyStore(Mutex<Option<RelayKey>>);

impl RelayKeyStore for MemoryKeyStore {
    fn holds_key(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    fn read(&self) -> Result<Option<RelayKey>, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .and_then(|key| RelayKey::new(key.expose())))
    }

    fn keep(&self, key: &RelayKey) -> Result<(), String> {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = RelayKey::new(key.expose());
        Ok(())
    }

    fn forget(&self) -> Result<(), String> {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = None;
        Ok(())
    }
}

/// The key in the macOS Keychain, as a generic password under [`KEYCHAIN_SERVICE`].
#[cfg(target_os = "macos")]
pub struct KeychainKeyStore;

#[cfg(target_os = "macos")]
impl RelayKeyStore for KeychainKeyStore {
    /// Searched by its attributes alone: reading a secret is what makes the Keychain ask the
    /// person to unlock it, and a search that loads no data does not.
    fn holds_key(&self) -> bool {
        use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
        ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(KEYCHAIN_SERVICE)
            .account(KEYCHAIN_ACCOUNT)
            .load_attributes(true)
            .limit(Limit::Max(1))
            .search()
            .is_ok_and(|found| !found.is_empty())
    }

    fn read(&self) -> Result<Option<RelayKey>, String> {
        use security_framework::passwords::{PasswordOptions, generic_password};
        match generic_password(PasswordOptions::new_generic_password(
            KEYCHAIN_SERVICE,
            KEYCHAIN_ACCOUNT,
        )) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(|key| RelayKey::new(&key))
                .map_err(|_| "the key kept for opencodex is not text".to_string()),
            Err(error) if is_not_found(&error) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn keep(&self, key: &RelayKey) -> Result<(), String> {
        security_framework::passwords::set_generic_password(
            KEYCHAIN_SERVICE,
            KEYCHAIN_ACCOUNT,
            key.expose().as_bytes(),
        )
        .map_err(|error| error.to_string())
    }

    fn forget(&self) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(
            KEYCHAIN_SERVICE,
            KEYCHAIN_ACCOUNT,
        ) {
            Ok(()) => Ok(()),
            Err(error) if is_not_found(&error) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// errSecItemNotFound: nothing kept, which is an answer rather than a failure.
#[cfg(target_os = "macos")]
fn is_not_found(error: &security_framework::base::Error) -> bool {
    error.code() == -25300
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The memory store keeps one key, replaced by the next, and forgets it; forgetting none is
    /// no failure. What it hands back is the key it was given.
    #[test]
    fn a_key_is_kept_replaced_and_forgotten() {
        let store = MemoryKeyStore::default();
        assert!(!store.holds_key());
        assert_eq!(store.read(), Ok(None));
        store
            .keep(&RelayKey::new("first-test-key").unwrap())
            .unwrap();
        store
            .keep(&RelayKey::new("second-test-key").unwrap())
            .unwrap();
        assert!(store.holds_key());
        assert_eq!(store.read(), Ok(RelayKey::new("second-test-key")));
        store.forget().unwrap();
        store.forget().unwrap();
        assert!(!store.holds_key());
    }
}
