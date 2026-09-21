//! LastPass's generic CSV: `url,username,password,totp,extra,name,grouping,fav`. The
//! `totp` column is a bare seed; `extra` is the notes; `name` the title. Rows whose url is
//! `http://sn` are secure notes, not logins, and are skipped.

use super::csv::{column, parse_csv};
use super::{ImportReport, item_from};

pub fn read_csv(text: &str) -> Result<ImportReport, String> {
    let mut rows = parse_csv(text).into_iter();
    let header = rows.next().ok_or_else(|| "the file is empty".to_string())?;
    let url = column(&header, &["url"]).ok_or_else(|| "no url column in the header".to_string())?;
    let user = column(&header, &["username"]).ok_or_else(|| "no username column".to_string())?;
    let pass = column(&header, &["password"]).ok_or_else(|| "no password column".to_string())?;
    let totp = column(&header, &["totp"]);
    let extra = column(&header, &["extra"]);
    let name = column(&header, &["name"]);
    let mut report = ImportReport {
        source: "LastPass",
        ..ImportReport::default()
    };
    for row in rows {
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).map(String::as_str).unwrap_or("");
        let site = cell(Some(url));
        if site.trim() == "http://sn" {
            report.skipped += 1;
            continue;
        }
        match item_from(
            site,
            cell(Some(user)),
            cell(Some(pass)),
            cell(totp),
            cell(name),
            cell(extra),
        ) {
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
    fn a_lastpass_export_is_read_and_its_notes_are_not_logins() {
        let text = "url,username,password,totp,extra,name,grouping,fav\nhttps://x.com/login,ada,pw,JBSWY3DPEHPK3PXP,my note,X site,,0\nhttp://sn,,,,a secure note,Note,,0\n";
        let report = read_csv(text).expect("parse");
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.skipped, 1);
        assert_eq!(report.items[0].label, "X site");
        assert_eq!(report.items[0].notes, "my note");
        assert_eq!(
            report.items[0].otpauth.as_deref(),
            Some("otpauth://totp/X%20site:ada?secret=JBSWY3DPEHPK3PXP")
        );
    }
}
