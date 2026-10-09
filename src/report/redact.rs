//! What a report may say: a fault's text with everything that could name a person, a machine or a
//! secret replaced by a placeholder, before the preview shows it. What the preview shows is what
//! is sent, so this runs once, here, and nothing downstream sees the text before it.
//!
//! The rules are ordered: a secret is caught by the rule for its own shape (a JSON field, a
//! header, a `key: value` in prose, a vendor's key prefix) before the general rules for URLs and
//! hosts run over what is left. Each rule has the vectors that once leaked as tests below.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// What this Mac knows that a pattern cannot: the names a person gave things. A Bot called
/// "Acme Jira Sync" is as identifying as an email, and no regex finds it.
#[derive(Debug, Clone, Default)]
pub struct Known {
    /// The home directory, `/Users/someone`.
    pub home: Option<String>,
    /// Names to hide wherever they appear: Bots, plugins, sites, usernames.
    pub names: Vec<String>,
}

/// The shortest name hidden by [`Known::names`]: a two-letter Bot name would take every "ai"
/// out of the text with it.
const SHORTEST_NAME: usize = 3;

/// Top-level domains a bare host may end in. Deliberately not "every two letters": `state.rs`,
/// `main.py` and `index.ts` are file names, and a report without them is a report without its
/// most useful line.
const HOST_TLDS: &str = "com|net|org|io|dev|app|ai|co|cloud|sh|me|info|biz|gov|edu|test|local|\
                         localhost|internal|lan|home|ph|uk|us|de|jp|cn|fr|ca|au|nl|in|br|eu|xyz|\
                         site|tech|online|run|page|so|gg|tv";

fn re(pattern: &str) -> Regex {
    // The patterns are constants in this file; one that does not compile is a bug the first test
    // finds, not a condition a person can cause.
    #[allow(clippy::expect_used)]
    Regex::new(pattern).expect("a redaction pattern compiles")
}

/// `"access_token" : "…"`, any spacing, any of the words a secret is filed under.
static JSON_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)"([a-z0-9_\-]*(?:token|secret|password|passwd|api[_-]?key|session|cookie|authorization|credential)[a-z0-9_\-]*)"\s*:\s*"(?:[^"\\]|\\.)*""#,
    )
});
/// A header that carries a credential: everything after its colon, to the end of the line.
static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?im)\b(proxy-authorization|authorization|set-cookie|cookie|x-api-key|api-key|x-auth-token)\s*:\s*[^\r\n]*",
    )
});
/// `Bearer <token>` anywhere in prose.
static BEARER: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=\-]+"));
/// `token: abc`, `password=abc`, `session=abc` in prose or a query string.
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b([a-z_\-]*(?:token|secret|password|passwd|api[_-]?key|session|cookie))\s*[:=]\s*[^\s,;&]+",
    )
});
/// Keys recognisable by their vendor prefix, standing alone in prose.
static PREFIXED_KEY: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\b(?:sk-[A-Za-z0-9_\-]{8,}|sk_(?:live|test)_[A-Za-z0-9]{8,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[abprs]-[A-Za-z0-9\-]{8,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_\-]{30,}|eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,})",
    )
});
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| re(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)+"));
/// `scheme://authority/path?query`: the host goes, the path keeps its shape with ids taken out,
/// the query goes (it is where tokens ride).
static URL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(?i)\b([a-z][a-z0-9+.\-]*)://[^\s/?#"'<>)\]]+(/[^\s?#"'<>)\]]*)?(?:\?[^\s#"'<>)\]]*)?(?:#[^\s"'<>)\]]*)?"#,
    )
});
static HOME: LazyLock<Regex> = LazyLock::new(|| re(r"/(?:Users|home)/[^/\s:]+"));
static IPV6: LazyLock<Regex> =
    LazyLock::new(|| re(r"\[[0-9A-Fa-f:.]*:[0-9A-Fa-f:.]*\](?::\d{1,5})?"));
static IPV4: LazyLock<Regex> = LazyLock::new(|| re(r"\b(?:\d{1,3}\.){3}\d{1,3}(?::\d{1,5})?\b"));
/// A bare host, with or without a port: a dotted name ending in a known top-level domain.
static HOST: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)\b(?:[a-z0-9](?:[a-z0-9\-]*[a-z0-9])?\.)+(?:{HOST_TLDS})\b(?::\d{{1,5}})?|\blocalhost(?::\d{{1,5}})?\b"
    ))
});
static UUID: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b"));
static PREFIXED_ID: LazyLock<Regex> = LazyLock::new(|| re(r"\b([a-z]{2,8})_([A-Za-z0-9\-]{8,})\b"));
static LONG_HEX: LazyLock<Regex> = LazyLock::new(|| re(r"\b[0-9a-fA-F]{16,}\b"));

/// Whether `prefix_body` (split at the first `_`) is one of the server's ids. Every id the
/// server makes has digits in its body (`cw_` and 32 hex); a code word never does, whatever its
/// prefix: `cw_unavailable`, `run_cancelled` and `plan_unavailable` stay words.
pub(crate) fn is_prefixed_id(_prefix: &str, body: &str) -> bool {
    body.chars().any(|c| c.is_ascii_digit())
}

/// Every id in `text` as `{id}`: the server's prefixed ids, UUIDs and long hex runs.
pub fn hide_ids(text: &str) -> String {
    let text = UUID.replace_all(text, "{id}");
    let text = PREFIXED_ID.replace_all(&text, |c: &Captures| {
        if is_prefixed_id(&c[1], &c[2]) {
            "{id}".to_string()
        } else {
            c[0].to_string()
        }
    });
    LONG_HEX.replace_all(&text, "{id}").into_owned()
}

/// A URL's path with its ids hidden, as the request it names: `/coworkers/{id}/usage`.
fn url_path(path: Option<&str>) -> String {
    hide_ids(path.unwrap_or(""))
}

/// `text` as a report may carry it.
pub fn redact(text: &str, known: &Known) -> String {
    let mut out = text.to_string();
    // The person's own words first, while they are whole: a name can contain a dot or an `@`
    // that a later rule would split.
    let mut names: Vec<&str> = known
        .names
        .iter()
        .map(|name| name.trim())
        .filter(|name| name.chars().count() >= SHORTEST_NAME)
        .collect();
    // Longest first, so "Acme Jira Sync" goes before "Acme".
    names.sort_by_key(|name| std::cmp::Reverse(name.len()));
    for name in names {
        out = replace_ignoring_case(&out, name, "{name}");
    }
    if let Some(home) = known.home.as_deref().filter(|home| !home.is_empty()) {
        out = out.replace(home, "~");
    }
    out = JSON_SECRET
        .replace_all(&out, |c: &Captures| format!("\"{}\": \"<secret>\"", &c[1]))
        .into_owned();
    out = HEADER
        .replace_all(&out, |c: &Captures| format!("{}: <secret>", &c[1]))
        .into_owned();
    out = BEARER
        .replace_all(&out, |c: &Captures| format!("{} <secret>", &c[1]))
        .into_owned();
    out = KEY_VALUE
        .replace_all(&out, |c: &Captures| format!("{}: <secret>", &c[1]))
        .into_owned();
    out = PREFIXED_KEY.replace_all(&out, "<secret>").into_owned();
    out = EMAIL.replace_all(&out, "{email}").into_owned();
    out = URL
        .replace_all(&out, |c: &Captures| {
            format!(
                "{}://{{host}}{}",
                &c[1],
                url_path(c.get(2).map(|m| m.as_str()))
            )
        })
        .into_owned();
    out = HOME.replace_all(&out, "~").into_owned();
    out = IPV6.replace_all(&out, "{ip}").into_owned();
    out = IPV4.replace_all(&out, "{ip}").into_owned();
    out = HOST.replace_all(&out, "{host}").into_owned();
    hide_ids(&out)
}

/// `haystack` with every case-insensitive `needle` replaced, keeping the rest as it was.
fn replace_ignoring_case(haystack: &str, needle: &str, with: &str) -> String {
    let lower = haystack.to_lowercase();
    let wanted = needle.to_lowercase();
    // Lowercasing can change byte lengths outside ASCII; then the offsets would not line up, so
    // only the exact spelling is replaced.
    if lower.len() != haystack.len() || wanted.len() != needle.len() {
        return haystack.replace(needle, with);
    }
    let mut out = String::with_capacity(haystack.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&wanted) {
        let start = at + found;
        out.push_str(&haystack[at..start]);
        out.push_str(with);
        at = start + wanted.len();
    }
    out.push_str(&haystack[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> Known {
        Known {
            home: Some("/Users/exampleuser".into()),
            names: vec![
                "Acme Jira Sync".into(),
                "person@example.test".into(),
                "Blitz".into(),
            ],
        }
    }

    // Key-shaped vectors are built with `concat!` so the source holds no literal a secret
    // scanner would take for a real key.
    /// Every vector that leaked in the grounding probe or in the validation's adversarial pass,
    /// with the words that must not survive. One table, so a new leak is one more row.
    #[test]
    fn nothing_that_names_a_person_a_machine_or_a_secret_survives() {
        let cases: &[(&str, &[&str])] = &[
            // Transport failures as OpenGrokError::detail() writes them.
            (
                "error sending request for url (http://127.0.0.1:1447/coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage?window=month)",
                &[
                    "127.0.0.1",
                    "1447",
                    "cw_018f3a2b9c7d7e10a1b2c3d4e5f60718",
                    "window=month",
                ],
            ),
            ("Authorization: Bearer abc.def.ghi", &["abc.def.ghi"]),
            ("Set-Cookie: session=s3cr3t; Path=/", &["s3cr3t"]),
            ("Cookie: abc123", &["abc123"]),
            (
                r#"{"error":"bad","access_token":"tok_live_123"}"#,
                &["tok_live_123"],
            ),
            (
                "could not read /Users/exampleuser/Library/Caches/x",
                &["exampleuser"],
            ),
            (
                "could not read /Users/otheruser/Library/Caches/x",
                &["otheruser"],
            ),
            (
                "person@example.test failed on opengrok.example.test:1447",
                &["person@example.test", "opengrok.example.test", "1447"],
            ),
            (
                "coworker cw_018f3a2b9c7d7e10a1b2c3d4e5f60718 not found; login.example.test:443 refused",
                &["cw_018f3a2b9c7d7e10a1b2c3d4e5f60718", "login.example.test"],
            ),
            (
                "upstream returned 503 at 10.0.0.4:29080 after 1200 ms",
                &["10.0.0.4", "29080"],
            ),
            ("upstream says [::1]:1447 is down", &["::1", "1447"]),
            ("ntf_0190a2b3c4d5e6f7 said", &["0190a2b3c4d5e6f7"]),
            (
                "person@example.test on https://login.example.test/: keychain locked",
                &["person@example.test", "login.example.test"],
            ),
            (
                "connect to opengrok.example.test failed",
                &["opengrok.example.test"],
            ),
            (
                "someone.else@corp.example.com could not sign in",
                &["someone.else", "corp.example.com"],
            ),
            // The validation's adversarial vectors, each of which leaked before.
            (
                r#"{"error":"bad","access_token": "tok_live_123"}"#,
                &["tok_live_123"],
            ),
            ("Authorization: Basic dXNlcjpwYXNz", &["dXNlcjpwYXNz"]),
            (
                concat!("X-Api-Key: sk", "-ant-api03-ABCDEF123"),
                &[concat!("sk", "-ant-api03-ABCDEF123"), "ABCDEF123"],
            ),
            (
                concat!("invalid key sk", "-ant-api03-ZZZZ9999"),
                &[concat!("sk", "-ant-api03-ZZZZ9999"), "ZZZZ9999"],
            ),
            ("token: abc123xyz", &["abc123xyz"]),
            ("login.example.test refused", &["login.example.test"]),
            (
                "plugin 'Acme Jira Sync' failed",
                &["Acme Jira Sync", "Acme"],
            ),
            (
                "request failed: Authorization: Bearer abc.def.ghi and retry",
                &["abc.def.ghi"],
            ),
        ];
        for (input, gone) in cases {
            let out = redact(input, &known());
            for word in *gone {
                assert!(
                    !out.contains(word),
                    "{word:?} survived in {out:?} (from {input:?})"
                );
            }
        }
    }

    /// What a report is for stays: the failure's own words, the code word, the status, the file
    /// and line, and the shape of the request.
    #[test]
    fn the_failure_itself_is_kept() {
        let cases: &[(&str, &str)] = &[
            (
                "upstream anthropic returned 400",
                "upstream anthropic returned 400",
            ),
            (
                "Connection refused (os error 61)",
                "Connection refused (os error 61)",
            ),
            ("raised at src/state.rs:8258", "raised at src/state.rs:8258"),
            ("plan_unavailable: no plan", "plan_unavailable: no plan"),
            (
                "error sending request for url (http://127.0.0.1:1458/coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage?window=month)",
                "error sending request for url (http://{host}/coworkers/{id}/usage)",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(redact(input, &known()), *want, "{input:?}");
        }
    }

    /// A name shorter than three letters is not hidden: it would take ordinary words with it.
    #[test]
    fn a_very_short_name_is_left_alone() {
        let known = Known {
            home: None,
            names: vec!["AI".into()],
        };
        assert_eq!(redact("AI said no", &known), "AI said no");
    }
}
