//! The CSV exports that share one shape: a header row naming the columns, then one login
//! per row. The Passwords app (`Title,URL,Username,Password,Notes,OTPAuth`), Chrome
//! (`name,url,username,password,note`) and 1Password (`Title,Website,Username,Password,
//! OTPAuth,…,Notes`) all fit.

use super::{ImportReport, Source, item_from};

/// RFC 4180, the parts exports use: commas, CRLF or LF, double quotes with `""` inside.
pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
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

/// The column with one of these names, ignoring case.
pub fn column(header: &[String], names: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|cell| names.contains(&cell.trim().to_ascii_lowercase().as_str()))
}

pub fn read_login_csv(text: &str, source: Source) -> Result<ImportReport, String> {
    let mut rows = parse_csv(text).into_iter();
    let header = rows.next().ok_or_else(|| "the file is empty".to_string())?;
    let url = column(
        &header,
        &["url", "website", "site", "origin", "web site", "login_uri"],
    )
    .ok_or_else(|| "no url column in the header".to_string())?;
    let user = column(
        &header,
        &["username", "user", "login", "email", "login_username"],
    )
    .ok_or_else(|| "no username column in the header".to_string())?;
    let pass = column(&header, &["password", "login_password"])
        .ok_or_else(|| "no password column in the header".to_string())?;
    let title = column(&header, &["title", "name"]);
    let notes = column(&header, &["notes", "note", "extra"]);
    let otp = column(
        &header,
        &["otpauth", "one-time password", "totp", "login_totp"],
    );
    let mut report = ImportReport {
        source: source.name(),
        ..ImportReport::default()
    };
    for row in rows {
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).map(String::as_str).unwrap_or("");
        let item = item_from(
            cell(Some(url)),
            cell(Some(user)),
            cell(Some(pass)),
            cell(otp),
            cell(title),
            cell(notes),
        );
        match item {
            Some(item) => report.items.push(item),
            None => {
                if row.iter().any(|c| !c.trim().is_empty()) {
                    report.skipped += 1;
                }
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_passwords_app_and_1password_csvs_are_read_with_their_codes() {
        let apple = "Title,URL,Username,Password,Notes,OTPAuth\r\nGitHub,https://github.com/login,ada,\"p,w\",note here,otpauth://totp/GitHub:ada?secret=JBSWY3DPEHPK3PXP\r\n";
        let report = read_login_csv(apple, Source::AppleCsv).expect("parse");
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].origin, "github.com");
        assert_eq!(report.items[0].label, "GitHub");
        assert_eq!(report.items[0].notes, "note here");
        assert!(
            report.items[0]
                .otpauth
                .as_deref()
                .unwrap()
                .starts_with("otpauth://")
        );
        let onep = "Title,Website,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\nAWS,https://aws.amazon.com,root,secret,,,,,\n";
        let report = read_login_csv(onep, Source::OnePasswordCsv).expect("parse");
        assert_eq!(report.items[0].origin, "amazon.com");
        assert_eq!(report.items[0].otpauth, None);
    }

    /// Each refusal is the sentence the person is shown, not merely some error: a file with no
    /// header, and a header missing any of the three columns a login needs.
    #[test]
    fn a_csv_without_the_columns_a_login_needs_is_refused_by_name() {
        let refused = |text: &str| read_login_csv(text, Source::ChromeCsv).unwrap_err();
        assert_eq!(refused(""), "the file is empty");
        assert_eq!(
            refused("name,username,password\n"),
            "no url column in the header"
        );
        assert_eq!(
            refused("name,url,password\n"),
            "no username column in the header"
        );
        assert_eq!(
            refused("name,url,username\n"),
            "no password column in the header"
        );
    }

    /// A row with something in it that is not a login is counted as skipped, so the person is
    /// told; a blank line is nothing and is not.
    #[test]
    fn a_row_that_is_not_a_login_is_counted_and_a_blank_one_is_not() {
        let text = "name,url,username,password\nX,https://x.com,ada,pw\nNo site,,bob,pw\n,,,\n";
        let report = read_login_csv(text, Source::ChromeCsv).expect("parse");
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.skipped, 1);
    }
}
