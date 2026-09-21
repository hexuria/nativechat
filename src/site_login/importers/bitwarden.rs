//! Bitwarden's unencrypted JSON export: `items[]` with `type` 1 for a login, `name`,
//! `notes`, and `login { username, password, totp, uris[] { uri } }`. `totp` is an
//! `otpauth://` URI or a bare seed.

use serde_json::Value;

use super::{ImportReport, item_from};

pub fn read_json(text: &str) -> Result<ImportReport, String> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{FEFF}'))
        .map_err(|e| format!("not a Bitwarden export: {e}"))?;
    if value.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(
            "this Bitwarden export is encrypted; export it unencrypted (JSON) to import it"
                .to_string(),
        );
    }
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| "not a Bitwarden export: no items".to_string())?;
    let mut report = ImportReport {
        source: "Bitwarden",
        ..ImportReport::default()
    };
    for item in items {
        if item.get("type").and_then(Value::as_i64) != Some(1) {
            continue;
        }
        let login = item.get("login").cloned().unwrap_or(Value::Null);
        let text =
            |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        let url = login
            .get("uris")
            .and_then(Value::as_array)
            .and_then(|uris| {
                uris.iter()
                    .find_map(|u| u.get("uri").and_then(Value::as_str))
            })
            .unwrap_or("")
            .to_string();
        match item_from(
            &url,
            &text(&login, "username"),
            &text(&login, "password"),
            &text(&login, "totp"),
            &text(item, "name"),
            &text(item, "notes"),
        ) {
            Some(item) => report.items.push(item),
            None => report.skipped += 1,
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bitwarden_export_is_read_and_an_encrypted_one_is_refused() {
        let text = r#"{"encrypted":false,"items":[
          {"type":1,"name":"Amazon","notes":"n","login":{"username":"alice","password":"pw","totp":"otpauth://totp/Amazon:alice?secret=JBSWY3DPEHPK3PXP","uris":[{"uri":"https://www.amazon.com/"}]}},
          {"type":2,"name":"A note","notes":"secure note"},
          {"type":1,"name":"No site","login":{"username":"a","password":"b","uris":[]}}
        ]}"#;
        let report = read_json(text).expect("parse");
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].origin, "amazon.com");
        assert_eq!(
            report.skipped, 1,
            "the login with no site is counted; the note is not"
        );
        assert!(read_json(r#"{"encrypted":true,"items":[]}"#).is_err());
    }
}
