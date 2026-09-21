//! Reading what other password managers export, into one shape.
//!
//! Five sources, one result: the Passwords app and Chrome (CSV), 1Password (1PUX or CSV),
//! LastPass (CSV), Bitwarden (JSON), and `pass` (a directory of GPG files, read through the
//! `pass` command with the person's own key). Each row becomes an [`ImportedItem`]; the
//! secrets are redacted in Debug like everywhere else, the file is read once and not kept.

pub mod bitwarden;
pub mod csv;
pub mod lastpass;
pub mod onepassword;
pub mod pass;

use super::origin::registrable_origin;

/// One login, code or note read from an export.
#[derive(Clone, PartialEq, Eq)]
pub struct ImportedItem {
    /// `password` (a login, with or without a code) or `code` (a code with no password).
    pub kind: &'static str,
    pub origin: String,
    pub username: String,
    pub password: Option<String>,
    /// An `otpauth://` URI, built from a bare seed when the export only had that.
    pub otpauth: Option<String>,
    pub label: String,
    pub notes: String,
}

impl std::fmt::Debug for ImportedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportedItem")
            .field("kind", &self.kind)
            .field("origin", &self.origin)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("otpauth", &self.otpauth.as_ref().map(|_| "<redacted>"))
            .field("label", &self.label)
            .finish()
    }
}

/// What a file gave and what it did not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub source: &'static str,
    pub items: Vec<ImportedItem>,
    /// Rows with no usable site, name or secret, counted so the person knows.
    pub skipped: usize,
    /// The entry the read stopped at, when it stopped before the end. The skipped count
    /// is then "not read", not "nothing in them".
    pub stopped_early: Option<String>,
}

/// Build an item from the pieces an export gives, or nothing when it is not a login: no
/// site, no name, or neither a password nor a code.
pub fn item_from(
    url: &str,
    username: &str,
    password: &str,
    otp: &str,
    title: &str,
    notes: &str,
) -> Option<ImportedItem> {
    let origin = registrable_origin(url.trim())?;
    let username = username.trim();
    let password = password.trim_end_matches(['\r', '\n']);
    let otpauth = otpauth_from(otp, title, username);
    if username.is_empty() || (password.is_empty() && otpauth.is_none()) {
        return None;
    }
    Some(ImportedItem {
        kind: if password.is_empty() {
            "code"
        } else {
            "password"
        },
        origin,
        username: username.to_string(),
        password: (!password.is_empty()).then(|| password.to_string()),
        otpauth,
        label: title.trim().to_string(),
        notes: notes.trim().to_string(),
    })
}

/// A code column holds either a full `otpauth://` URI or a bare base32 seed; a bare seed
/// becomes a URI with the defaults every authenticator uses (SHA-1, 6 digits, 30 s).
pub fn otpauth_from(raw: &str, title: &str, username: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with("otpauth://") {
        return Some(raw.to_string());
    }
    let seed: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase();
    let base32 = !seed.is_empty()
        && seed
            .chars()
            .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c) || c == '=');
    if !base32 {
        return None;
    }
    let label = if title.trim().is_empty() {
        username.trim().to_string()
    } else {
        format!("{}:{}", title.trim(), username.trim())
    };
    Some(format!(
        "otpauth://totp/{}?secret={seed}",
        percent_encode(&label)
    ))
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b':' | b'@' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Which export a file is, from its name and the first bytes; None when it is none of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    AppleCsv,
    ChromeCsv,
    OnePasswordCsv,
    OnePux,
    LastPassCsv,
    BitwardenJson,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Self::AppleCsv => "the Passwords app",
            Self::ChromeCsv => "Chrome",
            Self::OnePasswordCsv => "1Password (CSV)",
            Self::OnePux => "1Password (1PUX)",
            Self::LastPassCsv => "LastPass",
            Self::BitwardenJson => "Bitwarden",
        }
    }
}

pub fn detect(path: &std::path::Path, head: &[u8]) -> Option<Source> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "1pux" || head.starts_with(b"PK") {
        return Some(Source::OnePux);
    }
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start_matches('\u{FEFF}').trim_start();
    if ext == "json" || text.starts_with('{') {
        return text.contains("\"items\"").then_some(Source::BitwardenJson);
    }
    let header = text.lines().next().unwrap_or("").to_ascii_lowercase();
    let cols: Vec<&str> = header.split(',').map(str::trim).collect();
    if cols.first() == Some(&"url") && cols.contains(&"totp") {
        return Some(Source::LastPassCsv);
    }
    if cols.first() == Some(&"title") && cols.contains(&"website") {
        return Some(Source::OnePasswordCsv);
    }
    if cols.first() == Some(&"title") && cols.contains(&"url") {
        return Some(Source::AppleCsv);
    }
    if cols.first() == Some(&"name") && cols.contains(&"url") {
        return Some(Source::ChromeCsv);
    }
    None
}

/// Read one export file, whatever it is.
pub fn read_file(path: &std::path::Path) -> Result<ImportReport, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let head = &bytes[..bytes.len().min(4096)];
    let source = detect(path, head).ok_or_else(|| {
        "not an export this app reads: expected a CSV from the Passwords app, Chrome, 1Password or LastPass, a 1PUX file, or a Bitwarden JSON export".to_string()
    })?;
    match source {
        Source::OnePux => onepassword::read_1pux(&bytes),
        Source::BitwardenJson => bitwarden::read_json(&String::from_utf8_lossy(&bytes)),
        Source::AppleCsv | Source::ChromeCsv | Source::OnePasswordCsv => {
            csv::read_login_csv(&String::from_utf8_lossy(&bytes), source)
        }
        Source::LastPassCsv => lastpass::read_csv(&String::from_utf8_lossy(&bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_seed_becomes_a_uri_and_a_uri_stays() {
        assert_eq!(
            otpauth_from("jbsw y3dp-ehpk 3pxp", "GitHub", "ada").as_deref(),
            Some("otpauth://totp/GitHub:ada?secret=JBSWY3DPEHPK3PXP")
        );
        assert_eq!(
            otpauth_from("otpauth://totp/x?secret=ABC", "", "").as_deref(),
            Some("otpauth://totp/x?secret=ABC")
        );
        assert_eq!(otpauth_from("not base32!", "", ""), None);
        assert_eq!(otpauth_from("", "", ""), None);
    }

    #[test]
    fn an_item_needs_a_site_a_name_and_a_secret() {
        assert!(item_from("https://x.com", "ada", "pw", "", "X", "").is_some());
        let code = item_from("x.com", "ada", "", "JBSWY3DPEHPK3PXP", "X", "").expect("code");
        assert_eq!(code.kind, "code");
        assert!(code.password.is_none());
        assert!(item_from("x.com", "", "pw", "", "", "").is_none());
        assert!(item_from("x.com", "ada", "", "", "", "").is_none());
        assert!(item_from("", "ada", "pw", "", "", "").is_none());
        assert!(!format!("{:?}", item_from("x.com", "ada", "pw", "", "", "")).contains("pw\""));
    }

    #[test]
    fn a_file_is_told_by_its_header() {
        use std::path::Path;
        assert_eq!(
            detect(
                Path::new("a.csv"),
                b"Title,URL,Username,Password,Notes,OTPAuth\n"
            ),
            Some(Source::AppleCsv)
        );
        assert_eq!(
            detect(Path::new("a.csv"), b"name,url,username,password,note\n"),
            Some(Source::ChromeCsv)
        );
        assert_eq!(
            detect(
                Path::new("a.csv"),
                b"Title,Website,Username,Password,OTPAuth\n"
            ),
            Some(Source::OnePasswordCsv)
        );
        assert_eq!(
            detect(
                Path::new("a.csv"),
                b"url,username,password,totp,extra,name,grouping,fav\n"
            ),
            Some(Source::LastPassCsv)
        );
        assert_eq!(
            detect(Path::new("a.json"), b"{\"encrypted\":false,\"items\":[]}"),
            Some(Source::BitwardenJson)
        );
        assert_eq!(
            detect(Path::new("a.1pux"), b"PK\x03\x04"),
            Some(Source::OnePux)
        );
        assert_eq!(detect(Path::new("a.txt"), b"hello"), None);
    }
}
