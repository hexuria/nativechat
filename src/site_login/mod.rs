//! Saved site logins.
//!
//! A login is a row on the person's own server, sealed there, with a copy of
//! the password in the keychain of each Mac that has used it:
//! * Metadata (id, origin, username, label, timestamps) → sqlite `site_logins`
//!   in the app-support DB (`data.db` under [`crate::config::Config::data_dir`]).
//!   A sync keeps that list level with the server's.
//! * Password → OS Keychain service [`KEYCHAIN_SERVICE`] (account = row id),
//!   never a sqlite column, never AG-UI `content`, never a `ChatPart`. A row
//!   the server has and this Mac does not is fetched after Touch ID the first
//!   time it is used here, then kept.
//! * When Keychain is missing (Linux/dev), [`VAULT_FILE`] in that same data
//!   dir, mode 0600.
//!
//! A saved login is offered on the login card, and only for the card's own
//! site. Every use asks for Touch ID first; then the password goes straight to
//! the computer down the same channel a typed card uses. It is never painted,
//! never put in the card's inputs, and the Bot never sees it.
//!
//! The LLM `credentials` table and local-exec daemon JSON are not this store.

mod extract;
pub mod import;
mod origin;
mod secrets;
mod store;
pub mod touch_id;

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
