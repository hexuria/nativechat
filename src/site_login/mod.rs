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
pub mod import;
mod origin;
mod secrets;
mod store;
pub mod touch_id;

pub use broker::SESSION_BROKER_AVAILABLE;
pub use extract::{LoginFields, PendingSave, login_fields, login_origin, save_candidate};
pub use origin::{login_matches_request, origins_match, registrable_origin};
pub use store::{SiteLoginRecord, SiteLoginVault};

/// macOS Keychain / `keyring` service. Account is the sqlite `site_logins.id`.
pub const KEYCHAIN_SERVICE: &str = "ai.nativechat.site-login";

/// Fallback file when OS Keychain is not available (Linux/dev). Mode 0600.
pub const VAULT_FILE: &str = "site-login.vault";

/// Where a "Use saved login" press is, painted on the card until the form settles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavedLoginUse {
    /// The Touch ID sheet is up.
    Confirming { username: String },
    /// Touch ID passed; the values are on their way to the box.
    Filling { username: String },
    /// The person closed the sheet.
    Cancelled { username: String },
    /// The server would not put a saved login on this computer (a shared box).
    Refused { message: String },
    /// Touch ID could not run, or the password is not in this Mac's keychain.
    Unavailable { message: String },
}

impl SavedLoginUse {
    /// The line under the buttons.
    pub fn note(&self) -> String {
        match self {
            Self::Confirming { username } => {
                format!("Confirm with Touch ID to log in as {username}.")
            }
            Self::Filling { username } => {
                format!("Logging in as {username}. The password goes straight to the computer.")
            }
            Self::Cancelled { username } => {
                format!("Touch ID was cancelled. Try {username} again, or type the login.")
            }
            Self::Refused { message } | Self::Unavailable { message } => message.clone(),
        }
    }

    /// While the sheet is up or the fill is in flight, the card's own buttons wait.
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Confirming { .. } | Self::Filling { .. })
    }
}

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
