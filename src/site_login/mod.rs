//! Host site-login vault. Phase A.0: save / list / delete + protocol stubs.
//!
//! Storage is **local to this NativeChat install**, not OpenGrok:
//! * Metadata (id, origin, username, label, timestamps) → sqlite `site_logins`
//!   in the app-support DB (`data.db` under [`crate::config::Config::data_dir`]).
//! * Password → OS Keychain service [`KEYCHAIN_SERVICE`] (account = row id),
//!   never a sqlite column, never AG-UI `content`, never a `ChatPart`.
//! * When Keychain is missing (Linux/dev), [`VAULT_FILE`] in that same data
//!   dir, mode 0600.
//!
//! Reinstall with the same bundle id may keep Keychain items; wiping app
//! support drops sqlite metadata so Settings→Logins looks empty and those
//! secrets are orphaned. A future server-backed vault is out of scope.
//!
//! * `filled` is **not** typing into Box Chromium. It is cookies/profile on Box
//!   after the **session broker** (A.1). A.0 has no broker; `credential.request`
//!   confirms then posts `denied` / `missing` / `error`.
//!
//! The LLM `credentials` table and local-exec daemon JSON are not this store.

mod broker;
mod extract;
mod origin;
mod secrets;
mod store;

pub use broker::SESSION_BROKER_AVAILABLE;
pub use extract::{PendingSave, save_candidate};
pub use origin::{login_matches_request, origins_match, registrable_origin};
pub use store::{SiteLoginRecord, SiteLoginVault};

/// macOS Keychain / `keyring` service. Account is the sqlite `site_logins.id`.
pub const KEYCHAIN_SERVICE: &str = "ai.nativechat.site-login";

/// Fallback file when OS Keychain is not available (Linux/dev). Mode 0600.
pub const VAULT_FILE: &str = "site-login.vault";

#[derive(Debug, Clone, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Db(String),
    #[error("{0}")]
    Secret(String),
    #[error("{0}")]
    Io(String),
}

impl From<sqlx::Error> for StoreError {
    fn from(err: sqlx::Error) -> Self {
        Self::Db(err.to_string())
    }
}
