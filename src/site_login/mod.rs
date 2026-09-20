//! Host site-login vault. Phase A.0: save / list / delete + protocol stubs.
//!
//! * Metadata (id, origin, username, label, timestamps) → sqlite `site_logins`.
//! * Password → OS Keychain service [`KEYCHAIN_SERVICE`], never a sqlite column,
//!   never AG-UI `content`, never a `ChatPart`, never the transcript.
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
pub use origin::registrable_origin;
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
