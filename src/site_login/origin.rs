//! Registrable origin (eTLD+1) for site-login metadata.

use std::net::IpAddr;

use url::Url;

/// Multi-part ICANN suffixes, so `login.google.co.uk` → `google.co.uk`.
///
/// A hand list is the wrong long-term answer here and this one was short enough
/// to be dangerous: anything not on it collapsed to its last two labels, so
/// `bank.com.cn` and `evil.com.cn` compared equal and one site was offered
/// another site's saved login. The durable fix is a real public-suffix list —
/// see `PRIVATE_SUFFIXES` below for why that is a separate decision.
const MULTI_PART_TLDS: &[&str] = &[
    "ac.uk", "co.uk", "gov.uk", "org.uk", "me.uk", "net.uk", "sch.uk", "com.au", "net.au",
    "org.au", "edu.au", "gov.au", "id.au", "co.jp", "ne.jp", "or.jp", "ac.jp", "go.jp", "com.br",
    "net.br", "org.br", "com.mx", "co.nz", "net.nz", "org.nz", "govt.nz", "ac.nz", "co.za",
    "org.za", "co.in", "net.in", "org.in", "com.sg", "com.hk", "com.cn", "net.cn", "org.cn",
    "gov.cn", "edu.cn", "co.kr", "or.kr", "ne.kr", "com.tr", "org.tr", "net.tr", "co.il", "org.il",
    "com.tw", "org.tw", "com.my", "com.ph", "co.th", "in.th", "com.ar", "net.ar", "org.ar",
    "co.id", "web.id", "com.pl", "net.pl", "org.pl", "com.ua", "com.vn", "com.pk", "co.ke",
    "com.ng", "com.eg", "com.sa", "co.ao", "com.pe", "com.co", "com.uy", "com.ec", "com.do",
    "com.gt", "com.ve", "com.bo", "com.py",
];

/// Suffixes under which *anyone* can register a name. These are the dangerous
/// ones for a credential gate: `alice.github.io` and `bob.github.io` are two
/// unrelated people, and treating them as one site hands one of them the
/// other's password. Under these, the registrable unit is three labels.
///
/// This is the PRIVATE section of the public suffix list, abbreviated, and it
/// cannot be complete. A real list belongs here; the only public-suffix crate
/// vendored in this workspace (`publicsuffix`) is a parser that needs the
/// ~250KB list shipped beside it, so adopting it means vendoring that file.
const PRIVATE_SUFFIXES: &[&str] = &[
    "github.io",
    "gitlab.io",
    "vercel.app",
    "herokuapp.com",
    "pages.dev",
    "workers.dev",
    "netlify.app",
    "blogspot.com",
    "wordpress.com",
    "web.app",
    "firebaseapp.com",
    "azurewebsites.net",
    "cloudfront.net",
    "s3.amazonaws.com",
    "elasticbeanstalk.com",
    "onrender.com",
    "fly.dev",
    "surge.sh",
    "glitch.me",
    "repl.co",
    "ngrok.io",
    "ngrok-free.app",
    "trycloudflare.com",
    "githubusercontent.com",
    "appspot.com",
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
    if labels.len() >= 3 {
        let last_three = format!("{}.{last_two}", labels[labels.len() - 3]);
        // A private suffix can itself be three labels (`s3.amazonaws.com`), so
        // check the longer candidate before the shorter one.
        if labels.len() >= 4 && PRIVATE_SUFFIXES.iter().any(|s| *s == last_three) {
            return Some(format!("{}.{last_three}", labels[labels.len() - 4]));
        }
        if PRIVATE_SUFFIXES.iter().any(|s| *s == last_two)
            || MULTI_PART_TLDS.iter().any(|tld| *tld == last_two)
        {
            return Some(last_three);
        }
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

/// Settings→Logins row vs the login card's site. Username `None` means any
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
        // Both sides trimmed and compared without case: the stored side was
        // previously used raw, so a row saved as `Ada@Example.com ` never
        // matched a request for `ada@example.com`.
        Some(want) => row_username.trim().eq_ignore_ascii_case(want),
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
    fn distinct_sites_under_a_shared_suffix_do_not_match() {
        // Each of these pairs compared EQUAL before this fix, which meant one
        // site could be offered another site's saved login.
        for (a, b) in [
            ("bank.com.cn", "evil.com.cn"),
            ("alice.github.io", "bob.github.io"),
            ("x.vercel.app", "y.vercel.app"),
            ("site.co.kr", "other.co.kr"),
            ("a.herokuapp.com", "b.herokuapp.com"),
            ("one.pages.dev", "two.pages.dev"),
            ("example.s3.amazonaws.com", "other.s3.amazonaws.com"),
        ] {
            assert!(
                !origins_match(a, b),
                "{a} must not match {b}: they are different sites"
            );
        }
    }

    #[test]
    fn a_site_still_matches_its_own_subdomains() {
        assert!(origins_match("alice.github.io", "www.alice.github.io"));
        assert!(origins_match("bank.com.cn", "login.bank.com.cn"));
        assert!(origins_match("google.co.uk", "accounts.google.co.uk"));
    }

    #[test]
    fn username_compare_is_trimmed_and_case_insensitive() {
        assert!(login_matches_request(
            "facebook.com",
            " Ada@Example.com ",
            "facebook.com",
            Some("ada@example.com")
        ));
        assert!(!login_matches_request(
            "facebook.com",
            "ada",
            "facebook.com",
            Some("adam")
        ));
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
