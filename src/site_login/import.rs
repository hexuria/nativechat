//! Reading a passwords export (the Passwords app, Chrome, 1Password all write the same
//! shape: a header row naming the columns, then one login per row, quoted when needed).
//!
//! Only the site, the name and the password are read; a title, notes or a one-time-code
//! secret in the file are left where they are. The file never leaves the Mac except as the
//! rows the person then saves.

use super::origin::registrable_origin;

/// One login read from an export. The password is redacted in Debug like a pending save.
#[derive(Clone, PartialEq, Eq)]
pub struct ImportedLogin {
    pub origin: String,
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for ImportedLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportedLogin")
            .field("origin", &self.origin)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// What the file gave and what it did not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub logins: Vec<ImportedLogin>,
    /// Rows with no usable site, name or password, counted so the person knows.
    pub skipped: usize,
}

/// Parse a CSV export. The header decides the columns: `url` (or `website`, `site`,
/// `origin`), `username` (or `user`, `login`, `email`), `password`. Rows are trimmed; a
/// site is reduced to its registrable origin, so `https://www.facebook.com/login` files
/// under `facebook.com`.
pub fn parse_export(text: &str) -> Result<ImportReport, String> {
    // A file re-saved by a spreadsheet may start with a byte-order mark; it is not a column.
    let rows = parse_csv(text.strip_prefix('\u{FEFF}').unwrap_or(text));
    let mut rows = rows.into_iter();
    let header = rows.next().ok_or_else(|| "the file is empty".to_string())?;
    let find = |names: &[&str]| {
        header
            .iter()
            .position(|cell| names.contains(&cell.trim().to_ascii_lowercase().as_str()))
    };
    let url = find(&["url", "website", "site", "origin", "web site", "login_uri"])
        .ok_or_else(|| "no url column in the header".to_string())?;
    let user = find(&["username", "user", "login", "email", "login_username"])
        .ok_or_else(|| "no username column in the header".to_string())?;
    let pass = find(&["password", "login_password"])
        .ok_or_else(|| "no password column in the header".to_string())?;
    let mut report = ImportReport::default();
    for row in rows {
        let cell = |i: usize| row.get(i).map(|s| s.trim()).unwrap_or("");
        let origin = registrable_origin(cell(url));
        let username = cell(user);
        // A password is taken as written: a space at either end is part of it.
        let password = row.get(pass).map(String::as_str).unwrap_or("");
        match origin {
            Some(origin) if !username.is_empty() && !password.is_empty() => {
                report.logins.push(ImportedLogin {
                    origin,
                    username: username.to_string(),
                    password: password.to_string(),
                });
            }
            _ => {
                if row.iter().any(|cell| !cell.trim().is_empty()) {
                    report.skipped += 1;
                }
            }
        }
    }
    Ok(report)
}

/// RFC 4180, the parts exports use: commas, CRLF or LF, double quotes with `""` inside.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            }
            (true, other) => cell.push(other),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut cell)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (false, other) => cell.push(other),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passwords_app_export_is_read_by_its_header() {
        let text = "Title,URL,Username,Password,Notes,OTPAuth\r\n\
                    Facebook,https://www.facebook.com/login,ada@example.com,\"p,w\"\"x\",,\r\n\
                    Blank,,,,,\r\n\
                    NoPass,https://github.com,ada,,,\r\n";
        let report = parse_export(text).expect("parse");
        assert_eq!(report.logins.len(), 1, "{report:?}");
        assert_eq!(report.logins[0].origin, "facebook.com");
        assert_eq!(report.logins[0].username, "ada@example.com");
        assert_eq!(report.logins[0].password, "p,w\"x");
        assert_eq!(report.skipped, 2);
        assert!(!format!("{report:?}").contains("p,w"));
    }

    #[test]
    fn a_bom_is_not_a_column_and_a_password_keeps_its_spaces() {
        let text = "\u{FEFF}url,username,password\nhttps://x.com, ada , p w \n";
        let report = parse_export(text).expect("parse");
        assert_eq!(report.logins[0].username, "ada");
        assert_eq!(report.logins[0].password, " p w ");
    }

    #[test]
    fn a_chrome_export_uses_other_column_names_and_lf() {
        let text = "name,url,username,password,note\ngh,https://github.com/login,ada,secret,\n";
        let report = parse_export(text).expect("parse");
        assert_eq!(report.logins[0].origin, "github.com");
        assert_eq!(report.logins[0].password, "secret");
    }

    #[test]
    fn a_file_without_the_columns_says_which_is_missing() {
        assert_eq!(parse_export("").unwrap_err(), "the file is empty");
        assert_eq!(
            parse_export("a,b\n1,2\n").unwrap_err(),
            "no url column in the header"
        );
        assert_eq!(
            parse_export("url,password\n1,2\n").unwrap_err(),
            "no username column in the header"
        );
    }
}
