//! `pass` (passwordstore.org): one GPG file per entry under `~/.password-store` (or
//! `PASSWORD_STORE_DIR`). The entries are read through the `pass` command, so the person's
//! own key and agent do the decrypting; nothing here touches GPG. The first line is the
//! password; the lines after it are `key: value` pairs by convention (`username:`,
//! `login:`, `user:`, `email:`, `url:`) and an `otpauth://` line from pass-otp. The site
//! comes from a `url:` line, else from the entry's path (`github.com/ada` is the site and
//! the name).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{ImportReport, item_from};

/// Where the store is: `PASSWORD_STORE_DIR`, else `~/.password-store`.
pub fn store_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        return Some(PathBuf::from(dir));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| Path::new(&home).join(".password-store"))
}

/// Every entry name in the store (the path without `.gpg`), sorted.
pub fn entries(dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let read =
        std::fs::read_dir(dir).map_err(|e| format!("could not read {}: {e}", dir.display()))?;
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path.extension().is_some_and(|e| e == "gpg")
            && let Ok(rel) = path.strip_prefix(root)
        {
            let rel = rel.with_extension("");
            out.push(rel.to_string_lossy().to_string());
        }
    }
    Ok(())
}

/// One entry's decrypted text, through `pass show`.
pub fn show(entry: &str) -> Result<String, String> {
    let output = Command::new("pass")
        .arg("show")
        .arg(entry)
        .output()
        .map_err(|e| format!("could not run pass: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "pass show {entry} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// The whole store, one `pass show` per entry. Entries that fail to decrypt are skipped and
/// counted; the first failure's reason is returned with the report so the person sees it.
pub fn read_store(dir: &Path) -> Result<(ImportReport, Option<String>), String> {
    let names = entries(dir)?;
    let mut report = ImportReport {
        source: "pass",
        ..ImportReport::default()
    };
    let mut first_error = None;
    for name in names {
        match show(&name) {
            Ok(text) => match parse_entry(&name, &text) {
                Some(item) => report.items.push(item),
                None => report.skipped += 1,
            },
            Err(error) => {
                report.skipped += 1;
                first_error.get_or_insert(error);
            }
        }
    }
    Ok((report, first_error))
}

/// An entry's text into an item. Public for the tests; the text is a decrypted secret.
pub fn parse_entry(name: &str, text: &str) -> Option<super::ImportedItem> {
    let mut lines = text.lines();
    let password = lines.next().unwrap_or("").to_string();
    let mut username = String::new();
    let mut url = String::new();
    let mut otp = String::new();
    let mut notes = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("otpauth://") {
            otp = trimmed.to_string();
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            if !trimmed.is_empty() {
                notes.push(trimmed.to_string());
            }
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "username" | "user" | "login" | "email" if username.is_empty() => {
                username = value.to_string()
            }
            "url" | "website" | "site" if url.is_empty() => url = value.to_string(),
            "otpauth" => otp = format!("otpauth:{value}"),
            _ => notes.push(trimmed.to_string()),
        }
    }
    // The path fills what the lines did not: `github.com/ada` is the site and the name.
    let (dir, leaf) = name.rsplit_once('/').unwrap_or(("", name));
    if url.is_empty() {
        url = if dir.is_empty() {
            leaf.to_string()
        } else {
            dir.to_string()
        };
    }
    if username.is_empty() && !dir.is_empty() {
        username = leaf.to_string();
    }
    let title = if dir.is_empty() { leaf } else { dir };
    item_from(&url, &username, &password, &otp, title, &notes.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_read_from_its_lines_and_its_path() {
        let item = parse_entry(
            "github.com/ada",
            "Yw|ZSNH!}z\"6{ym9pI\nURL: *.github.com/*\nUsername: AdaL\notpauth://totp/GitHub:ada?secret=JBSWY3DPEHPK3PXP\nSecret question: none\n",
        )
        .expect("item");
        assert_eq!(item.origin, "github.com");
        assert_eq!(
            item.username, "AdaL",
            "the Username line wins over the path"
        );
        assert_eq!(item.password.as_deref(), Some("Yw|ZSNH!}z\"6{ym9pI"));
        assert!(item.otpauth.is_some());
        assert_eq!(item.notes, "Secret question: none");
        let bare = parse_entry("amazon.com/alice", "pw\n").expect("item");
        assert_eq!(bare.username, "alice");
        assert_eq!(bare.origin, "amazon.com");
        assert!(
            parse_entry("just-a-note", "text\n").is_none(),
            "no site, no name"
        );
    }
}
