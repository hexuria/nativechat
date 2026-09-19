//! Registrable origin (eTLD+1) for site-login metadata.

use std::net::IpAddr;

use url::Url;

/// A small multi-part public-suffix set so `login.google.co.uk` → `google.co.uk`.
const MULTI_PART_TLDS: &[&str] = &[
    "ac.uk", "co.uk", "gov.uk", "org.uk", "com.au", "net.au", "org.au", "co.jp", "ne.jp", "or.jp",
    "com.br", "com.mx", "co.nz", "co.za", "co.in", "com.sg", "com.hk",
];

/// Host / eTLD+1 from a domain, live host, or URL. None when empty.
pub fn registrable_origin(raw: &str) -> Option<String> {
    let host = host_of(raw)?;
    if host.parse::<IpAddr>().is_ok() {
        return Some(host);
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Some("localhost".into());
    }
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() < 2 {
        return Some(host);
    }
    let last_two = format!("{}.{}", labels[labels.len() - 2], labels[labels.len() - 1]);
    if MULTI_PART_TLDS.iter().any(|tld| *tld == last_two) && labels.len() >= 3 {
        return Some(format!("{}.{last_two}", labels[labels.len() - 3]));
    }
    Some(last_two)
}

fn host_of(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(url) = Url::parse(raw) {
        return url.host_str().map(str::to_string);
    }
    if let Ok(url) = Url::parse(&format!("https://{raw}")) {
        return url.host_str().map(str::to_string);
    }
    let host = raw.split('/').next().unwrap_or(raw);
    let host = host.split(':').next().unwrap_or(host).trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// eTLD+1 equality so `facebook.com` matches `https://www.facebook.com/login`.
pub fn origins_match(left: &str, right: &str) -> bool {
    match (registrable_origin(left), registrable_origin(right)) {
        (Some(a), Some(b)) => a == b,
        _ => left.trim().eq_ignore_ascii_case(right.trim()),
    }
}

/// Settings→Logins row vs `credential.request`. Username `None` means any
/// row for the origin; a named username must match exactly.
pub fn login_matches_request(
    row_origin: &str,
    row_username: &str,
    origin: &str,
    username: Option<&str>,
) -> bool {
    if !origins_match(row_origin, origin) {
        return false;
    }
    match username.map(str::trim).filter(|name| !name.is_empty()) {
        Some(want) => row_username == want,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etld_plus_one() {
        assert_eq!(
            registrable_origin("https://accounts.google.com/signin").as_deref(),
            Some("google.com")
        );
        assert_eq!(
            registrable_origin("login.google.co.uk").as_deref(),
            Some("google.co.uk")
        );
        assert_eq!(
            registrable_origin("127.0.0.1:8443").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(
            registrable_origin("http://localhost:3000").as_deref(),
            Some("localhost")
        );
        assert_eq!(registrable_origin("").as_deref(), None);
    }

    #[test]
    fn request_origin_matches_saved_etld() {
        assert!(origins_match(
            "facebook.com",
            "https://www.facebook.com/login"
        ));
        assert!(origins_match("127.0.0.1", "http://127.0.0.1:8765/"));
        assert!(!origins_match("facebook.com", "google.com"));
        assert!(login_matches_request(
            "facebook.com",
            "ada",
            "https://www.facebook.com",
            None
        ));
        assert!(login_matches_request(
            "facebook.com",
            "ada",
            "facebook.com",
            Some("ada")
        ));
        assert!(!login_matches_request(
            "facebook.com",
            "ada",
            "facebook.com",
            Some("other")
        ));
        assert!(!login_matches_request(
            "google.com",
            "ada",
            "facebook.com",
            None
        ));
    }
}
