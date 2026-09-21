//! 1Password's 1PUX export: a ZIP with `export.data`, a JSON document of `accounts[]` →
//! `vaults[]` → `items[]`. A login item has `overview { title, url, urls[] { url } }` and
//! `details { loginFields[] { designation: username|password, value }, sections[] {
//! fields[] { value { totp } } }, notesPlain }`.

use std::io::Read;

use serde_json::Value;

use super::{ImportReport, item_from};

pub fn read_1pux(bytes: &[u8]) -> Result<ImportReport, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| format!("not a 1PUX file: {e}"))?;
    let mut data = String::new();
    zip.by_name("export.data")
        .map_err(|_| "not a 1PUX file: no export.data inside".to_string())?
        .read_to_string(&mut data)
        .map_err(|e| format!("could not read export.data: {e}"))?;
    read_export_data(&data)
}

pub fn read_export_data(text: &str) -> Result<ImportReport, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("export.data is not JSON: {e}"))?;
    let mut report = ImportReport {
        source: "1Password",
        ..ImportReport::default()
    };
    let empty = Vec::new();
    for account in value
        .get("accounts")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
    {
        for vault in account
            .get("vaults")
            .and_then(Value::as_array)
            .unwrap_or(&empty)
        {
            for item in vault
                .get("items")
                .and_then(Value::as_array)
                .unwrap_or(&empty)
            {
                if item
                    .get("categoryUuid")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c != "001")
                {
                    continue; // 001 is a login; the rest are cards, notes, identities
                }
                if item.get("trashed").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                let overview = item.get("overview").cloned().unwrap_or(Value::Null);
                let details = item.get("details").cloned().unwrap_or(Value::Null);
                let title = overview.get("title").and_then(Value::as_str).unwrap_or("");
                let url = overview
                    .get("url")
                    .and_then(Value::as_str)
                    .filter(|u| !u.is_empty())
                    .or_else(|| {
                        overview
                            .get("urls")
                            .and_then(Value::as_array)
                            .and_then(|urls| {
                                urls.iter()
                                    .find_map(|u| u.get("url").and_then(Value::as_str))
                            })
                    })
                    .unwrap_or("");
                let field = |designation: &str| {
                    details
                        .get("loginFields")
                        .and_then(Value::as_array)
                        .and_then(|fields| {
                            fields.iter().find(|f| {
                                f.get("designation").and_then(Value::as_str) == Some(designation)
                            })
                        })
                        .and_then(|f| f.get("value").and_then(Value::as_str))
                        .unwrap_or("")
                        .to_string()
                };
                let totp = details
                    .get("sections")
                    .and_then(Value::as_array)
                    .and_then(|sections| {
                        sections.iter().find_map(|s| {
                            s.get("fields")
                                .and_then(Value::as_array)
                                .and_then(|fields| {
                                    fields.iter().find_map(|f| {
                                        f.pointer("/value/totp").and_then(Value::as_str)
                                    })
                                })
                        })
                    })
                    .unwrap_or("");
                let notes = details
                    .get("notesPlain")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match item_from(
                    url,
                    &field("username"),
                    &field("password"),
                    totp,
                    title,
                    notes,
                ) {
                    Some(item) => report.items.push(item),
                    None => report.skipped += 1,
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
    fn a_1pux_login_is_read_with_its_code_and_other_categories_are_left() {
        let data = r#"{"accounts":[{"vaults":[{"items":[
          {"categoryUuid":"001","overview":{"title":"GitHub","url":"https://github.com"},"details":{"loginFields":[{"designation":"username","value":"ada"},{"designation":"password","value":"pw"}],"sections":[{"fields":[{"title":"one-time password","value":{"totp":"otpauth://totp/GitHub:ada?secret=JBSWY3DPEHPK3PXP"}}]}],"notesPlain":"hi"}},
          {"categoryUuid":"002","overview":{"title":"Visa"},"details":{}},
          {"categoryUuid":"001","trashed":true,"overview":{"title":"Old","url":"https://old.com"},"details":{"loginFields":[{"designation":"username","value":"a"},{"designation":"password","value":"b"}]}}
        ]}]}]}"#;
        let report = read_export_data(data).expect("parse");
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].label, "GitHub");
        assert_eq!(report.items[0].notes, "hi");
        assert!(report.items[0].otpauth.is_some());
        assert_eq!(report.skipped, 0);
    }
}
