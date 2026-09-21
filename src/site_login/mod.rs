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
pub mod importers;
mod origin;
mod secrets;
mod store;
pub mod totp;
pub mod touch_id;

pub use extract::{
    CardTarget, LoginFields, PendingSave, card_target, login_fields, login_origin, save_candidate,
};
pub use origin::{login_matches_request, origins_match, registrable_origin};
pub use store::{SiteLoginRecord, SiteLoginVault};

/// macOS Keychain / `keyring` service. Account is the sqlite `site_logins.id`.
pub const KEYCHAIN_SERVICE: &str = "ai.nativechat.site-login";

/// Fallback file when OS Keychain is not available (Linux/dev). Mode 0600.
pub const VAULT_FILE: &str = "site-login.vault";

/// The three kinds of row. A word the server adds later still lists under All.
pub const KIND_PASSWORD: &str = "password";
pub const KIND_CODE: &str = "code";
pub const KIND_PASSKEY: &str = "passkey";

/// The title a row gets when the person gave none.
pub fn default_label(username: &str, origin: &str) -> String {
    format!("{username} on {origin}")
}

/// The sections of the list on Settings → Logins, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteLoginGroup {
    Passwords,
    Passkeys,
    Codes,
    /// Rows with a `Security:` line in their notes, listed here as well as under their kind.
    Security,
}

impl SiteLoginGroup {
    pub const ALL: [Self; 4] = [Self::Passwords, Self::Passkeys, Self::Codes, Self::Security];

    /// The word in the header's id, `settings-logins-group-{id}`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Passwords => "passwords",
            Self::Passkeys => "passkeys",
            Self::Codes => "codes",
            Self::Security => "security",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Passwords => "Passwords",
            Self::Passkeys => "Passkeys",
            Self::Codes => "Codes",
            Self::Security => "Security",
        }
    }

    /// Whether a row is filed under this section. A kind this app has not heard of files
    /// under Passwords, so a word from a newer server is still on the list.
    pub fn holds(self, row: &SiteLoginRecord) -> bool {
        match self {
            Self::Passwords => row.kind != KIND_PASSKEY && row.kind != KIND_CODE,
            Self::Passkeys => row.kind == KIND_PASSKEY,
            Self::Codes => row.kind == KIND_CODE,
            Self::Security => has_security_note(row),
        }
    }
}

/// A line of the notes starting `Security:` files the row under Security as well.
pub fn has_security_note(row: &SiteLoginRecord) -> bool {
    row.notes
        .lines()
        .any(|line| line.trim_start().starts_with("Security:"))
}

/// What the list calls a row: its title, or the site when it has none.
pub fn login_title(row: &SiteLoginRecord) -> &str {
    let label = row.label.trim();
    if label.is_empty() { &row.origin } else { label }
}

/// The search field: title, site or name, any case. Nothing typed matches everything.
pub fn matches_query(row: &SiteLoginRecord, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    [
        row.label.as_str(),
        row.origin.as_str(),
        row.username.as_str(),
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(&query))
}

/// The sections the list shows for a search, each with its rows in list order: by title,
/// then by name for two rows with one title. The three kinds are always there — a 0 says
/// what the kind is — until a search is on, when a section with no match is left out;
/// Security is there only while some row has a `Security:` note.
/// `with_code` names the rows that carry an authenticator-code seed beside their password;
/// they show under Codes as well as under Passwords, so the Codes count is what the person
/// can fill a code card with.
pub fn grouped_logins<'a>(
    rows: &'a [SiteLoginRecord],
    query: &str,
    with_code: &std::collections::HashSet<String>,
) -> Vec<(SiteLoginGroup, Vec<&'a SiteLoginRecord>)> {
    let searching = !query.trim().is_empty();
    SiteLoginGroup::ALL
        .into_iter()
        .filter_map(|group| {
            let mut members: Vec<&SiteLoginRecord> = rows
                .iter()
                .filter(|row| {
                    (group.holds(row)
                        || (group == SiteLoginGroup::Codes && with_code.contains(&row.id)))
                        && matches_query(row, query)
                })
                .collect();
            members.sort_by_cached_key(|row| {
                (
                    login_title(row).to_lowercase(),
                    row.username.to_lowercase(),
                    row.id.clone(),
                )
            });
            let shown = match group {
                SiteLoginGroup::Security => !members.is_empty(),
                _ => !searching || !members.is_empty(),
            };
            shown.then_some((group, members))
        })
        .collect()
}

/// A row's stamp as unix milliseconds. The table holds two spellings: `datetime('now')`
/// writes `2026-09-21 10:00:00`, the app writes RFC 3339.
pub fn timestamp_ms(text: &str) -> Option<i64> {
    if let Ok(at) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(at.timestamp_millis());
    }
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|at| at.and_utc().timestamp_millis())
}

/// "Last used" in words, for a stamp against now, both in unix milliseconds.
pub fn relative_time(at_ms: i64, now_ms: i64) -> String {
    let secs = (now_ms - at_ms).max(0) / 1000;
    let plural = |n: i64, unit: &str| {
        if n == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{n} {unit}s ago")
        }
    };
    if secs < 60 {
        "Just now".to_string()
    } else if secs < 3600 {
        plural(secs / 60, "minute")
    } else if secs < 86_400 {
        plural(secs / 3600, "hour")
    } else if secs < 172_800 {
        "Yesterday".to_string()
    } else if secs < 604_800 {
        plural(secs / 86_400, "day")
    } else if secs < 2_592_000 {
        plural(secs / 604_800, "week")
    } else if secs < 31_536_000 {
        plural(secs / 2_592_000, "month")
    } else {
        plural(secs / 31_536_000, "year")
    }
}

/// "Added" as a date, in the person's own time zone; the stamp as written when it does
/// not parse.
pub fn added_date(created_at: &str) -> String {
    match timestamp_ms(created_at).and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis)
    {
        Some(at) => at
            .with_timezone(&chrono::Local)
            .format("%b %-d, %Y")
            .to_string(),
        None => created_at.to_string(),
    }
}

/// Where a pick from the card's account list is, until the form settles.
#[derive(Clone, PartialEq, Eq)]
pub enum SavedLoginUse {
    /// The Touch ID sheet is up.
    Confirming { username: String },
    /// Touch ID passed: the name is in its field, the password is held for the submit and
    /// shown as dots. Log in sends both.
    Ready {
        login_id: String,
        username: String,
        password: String,
    },
    /// Log in was pressed; the values are on their way to the box, named by the row they
    /// came from so the server can stamp its use.
    Filling { username: String, login_id: String },
    /// The person closed the sheet.
    Cancelled { username: String },
    /// The server would not put a saved login on this computer (a shared box).
    Refused { message: String },
    /// Touch ID could not run, or the password is not in this Mac's keychain.
    Unavailable { message: String },
}

impl std::fmt::Debug for SavedLoginUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready {
                login_id, username, ..
            } => f
                .debug_struct("Ready")
                .field("login_id", login_id)
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
            Self::Confirming { username } => write!(f, "Confirming({username})"),
            Self::Filling { username, .. } => write!(f, "Filling({username})"),
            Self::Cancelled { username } => write!(f, "Cancelled({username})"),
            Self::Refused { message } => write!(f, "Refused({message})"),
            Self::Unavailable { message } => write!(f, "Unavailable({message})"),
        }
    }
}

impl SavedLoginUse {
    /// The line under the field.
    pub fn note(&self) -> String {
        match self {
            Self::Confirming { username } => {
                format!("Confirm with Touch ID to fill in {username}.")
            }
            Self::Ready { username, .. } => {
                format!("Password for {username} from your keychain. Press Log in.")
            }
            Self::Filling { username, .. } => {
                format!("Logging in as {username}. The password goes straight to the computer.")
            }
            Self::Cancelled { username } => {
                format!("Touch ID was cancelled. Try {username} again, or type the login.")
            }
            Self::Refused { message } | Self::Unavailable { message } => message.clone(),
        }
    }

    /// The row in flight, once Log in was pressed.
    pub fn filling_id(&self) -> Option<&str> {
        match self {
            Self::Filling { login_id, .. } => Some(login_id.as_str()),
            _ => None,
        }
    }

    /// The held secret (a password, a code seed, or nothing for a passkey), once Touch ID
    /// passed.
    pub fn ready(&self) -> Option<(&str, &str)> {
        match self {
            Self::Ready {
                username, password, ..
            } => Some((username.as_str(), password.as_str())),
            _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        id: &str,
        origin: &str,
        username: &str,
        label: &str,
        kind: &str,
        notes: &str,
    ) -> SiteLoginRecord {
        SiteLoginRecord {
            id: id.into(),
            origin: origin.into(),
            username: username.into(),
            label: label.into(),
            kind: kind.into(),
            notes: notes.into(),
            last_used_at_ms: None,
            created_at: "2026-09-21T10:00:00+00:00".into(),
            updated_at: "2026-09-21T10:00:00+00:00".into(),
        }
    }

    fn shape<'a>(
        groups: &'a [(SiteLoginGroup, Vec<&SiteLoginRecord>)],
    ) -> Vec<(&'a str, Vec<&'a str>)> {
        groups
            .iter()
            .map(|(group, members)| {
                (
                    group.id(),
                    members.iter().map(|row| row.id.as_str()).collect(),
                )
            })
            .collect()
    }

    /// The three kinds are always sections, Security only while a row has a `Security:`
    /// line; a row with one is under Security as well as under its kind, and a kind this
    /// app has not heard of is a password.
    #[test]
    fn the_list_is_grouped_by_kind_and_security_only_when_there_is_one() {
        let rows = vec![
            row("1", "a.com", "ada", "", "password", ""),
            row("2", "b.com", "bea", "", "passkey", "Security: hardware key"),
            row(
                "3",
                "c.com",
                "cy",
                "",
                "code",
                "plain words\n  Security: recovery codes in the safe",
            ),
            row(
                "4",
                "d.com",
                "dee",
                "",
                "password",
                "Not security: just a note",
            ),
            row("5", "e.com", "eve", "", "something-new", ""),
        ];
        assert_eq!(
            shape(&grouped_logins(&rows, "", &Default::default())),
            [
                ("passwords", vec!["1", "4", "5"]),
                ("passkeys", vec!["2"]),
                ("codes", vec!["3"]),
                ("security", vec!["2", "3"]),
            ]
        );
        // No `Security:` line anywhere: no Security section. The three kinds stay, empty or not.
        let plain = vec![row("1", "a.com", "ada", "", "password", "")];
        assert_eq!(
            shape(&grouped_logins(&plain, "", &Default::default())),
            [
                ("passwords", vec!["1"]),
                ("passkeys", vec![]),
                ("codes", vec![]),
            ]
        );
        assert_eq!(
            shape(&grouped_logins(&[], "", &Default::default())),
            [
                ("passwords", vec![]),
                ("passkeys", vec![]),
                ("codes", vec![])
            ]
        );
    }

    /// Rows are by title (the site when there is none), any case; a search reads the title,
    /// the site and the name, and leaves out a section it empties.
    #[test]
    fn the_list_sorts_by_title_and_the_search_reads_three_fields() {
        let rows = vec![
            row("1", "zeta.com", "ada", "", "password", ""),
            row(
                "2",
                "github.com",
                "bea@work.example",
                "Work GitHub",
                "password",
                "",
            ),
            row("3", "apple.com", "cy", "beta", "password", ""),
            row("4", "keys.example", "dee", "", "passkey", ""),
        ];
        let passwords = |query: &str| -> Vec<String> {
            grouped_logins(&rows, query, &Default::default())
                .into_iter()
                .find(|(group, _)| *group == SiteLoginGroup::Passwords)
                .map(|(_, members)| {
                    members
                        .into_iter()
                        .map(|row| login_title(row).to_string())
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(passwords(""), ["beta", "Work GitHub", "zeta.com"]);
        assert_eq!(
            passwords("WORK"),
            ["Work GitHub"],
            "the title and the name both say work"
        );
        assert_eq!(passwords("apple"), ["beta"]);
        assert_eq!(passwords("   ").len(), 3);
        let sections = |query: &str| -> Vec<&str> {
            grouped_logins(&rows, query, &Default::default())
                .iter()
                .map(|(group, _)| group.id())
                .collect()
        };
        assert_eq!(sections("zeta"), ["passwords"]);
        assert_eq!(sections("keys"), ["passkeys"]);
        assert!(sections("nothing here").is_empty());
    }

    /// Both spellings the table holds read as the same moment; words for a distance.
    #[test]
    fn stamps_parse_in_both_spellings_and_read_as_words() {
        assert_eq!(
            timestamp_ms("2026-09-21T10:00:00+00:00"),
            timestamp_ms("2026-09-21 10:00:00")
        );
        assert_eq!(timestamp_ms("yesterday-ish"), None);
        let now = 1_800_000_000_000;
        assert_eq!(relative_time(now - 5_000, now), "Just now");
        assert_eq!(relative_time(now - 60_000, now), "1 minute ago");
        assert_eq!(relative_time(now - 5 * 3_600_000, now), "5 hours ago");
        assert_eq!(relative_time(now - 30 * 3_600_000, now), "Yesterday");
        assert_eq!(relative_time(now - 3 * 86_400_000, now), "3 days ago");
        assert_eq!(relative_time(now - 14 * 86_400_000, now), "2 weeks ago");
        assert_eq!(relative_time(now - 40 * 86_400_000, now), "1 month ago");
        assert_eq!(relative_time(now - 800 * 86_400_000, now), "2 years ago");
        assert_eq!(
            relative_time(now + 60_000, now),
            "Just now",
            "a clock ahead is not the future"
        );
        assert!(added_date("2026-09-21 10:00:00").contains("2026"));
        assert_eq!(added_date("not a date"), "not a date");
    }
}
