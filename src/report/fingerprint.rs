//! Which failure a report is, as twelve hex characters: the same for the same failure on any
//! Mac, any Bot, any port and any build of one minor version; different for a different failure.
//! The relay files one issue per fingerprint, so two people hitting one bug add to one issue.
//!
//! Everything that varies between occurrences is taken out before hashing: ids, numbers, hosts,
//! the query, and the patch version.

use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

use super::Kind;
use super::redact::{re, route_shape};

/// The longest normalized message hashed: a failure is told apart by its start.
const HASHED_MESSAGE_CHARS: usize = 200;

/// What a fingerprint is made of, already redacted.
#[derive(Debug, Clone, Copy)]
pub struct FingerprintInput<'a> {
    pub kind: Kind,
    /// The app's version; only major.minor is hashed.
    pub app_version: &'a str,
    /// The place word (`usage`).
    pub place: &'a str,
    /// The request, `GET /coworkers/{id}/usage`, or none.
    pub endpoint: Option<&'a str>,
    pub status: Option<u16>,
    /// The failure's text.
    pub message: &'a str,
}

/// `0.9.3` → `0.9`.
pub fn major_minor(version: &str) -> &str {
    let end = version
        .match_indices('.')
        .nth(1)
        .map_or(version.len(), |(at, _)| at);
    &version[..end]
}

/// `GET /coworkers/cw_…/usage?window=month` → `GET /coworkers/{id}/usage`.
pub fn endpoint_template(endpoint: &str) -> String {
    let (method, rest) = endpoint.split_once(' ').unwrap_or(("", endpoint));
    let path = route_shape(rest.split(['?', '#']).next().unwrap_or(""));
    if method.is_empty() {
        path
    } else {
        format!("{method} {path}")
    }
}

fn status_class(status: Option<u16>) -> &'static str {
    match status {
        None => "none",
        Some(400..=499) => "4xx",
        Some(500..=599) => "5xx",
        Some(_) => "other",
    }
}

static NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"\d+"));
static SPACE: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));

/// The message with what varies between occurrences taken out: every number, so a port, a
/// duration or an os error number does not split one failure. Lowercased, one space.
fn normalize_message(message: &str) -> String {
    let text = message.to_lowercase();
    let text = NUMBER.replace_all(&text, "#");
    let text = SPACE.replace_all(text.trim(), " ");
    text.chars().take(HASHED_MESSAGE_CHARS).collect()
}

/// The fingerprint: the first six bytes of the SHA-256 of `kind | major.minor | place | request
/// | status class | message`, as twelve hex characters.
pub fn fingerprint(input: &FingerprintInput) -> String {
    let hashed = [
        input.kind.word().to_string(),
        major_minor(input.app_version).to_string(),
        input.place.to_string(),
        input.endpoint.map(endpoint_template).unwrap_or_default(),
        status_class(input.status).to_string(),
        normalize_message(input.message),
    ]
    .join("|");
    let digest = Sha256::digest(hashed.as_bytes());
    digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFUSED: &str = "error sending request for url (http://{host}/coworkers/{id}/usage)\n\nCaused by:\n    0: client error (Connect)\n    1: tcp connect error\n    2: Connection refused (os error 61)";
    const TIMED_OUT: &str = "error sending request for url (http://{host}/coworkers/{id}/usage)\n\nCaused by:\n    0: operation timed out (os error 60)";
    const USAGE: &str = "GET /coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage?window=month";

    fn fault<'a>(
        version: &'a str,
        place: &'a str,
        endpoint: &'a str,
        status: Option<u16>,
        message: &'a str,
    ) -> FingerprintInput<'a> {
        FingerprintInput {
            kind: Kind::Fault,
            app_version: version,
            place,
            endpoint: Some(endpoint),
            status,
            message,
        }
    }

    /// The same failure on another patch version, Bot, window and os error number is one issue;
    /// another place, status class, minor version or cause is another.
    #[test]
    fn one_failure_is_one_fingerprint_and_another_is_not() {
        let other_errno = REFUSED.replace("61", "62");
        let a = fault("0.9.3", "usage", USAGE, None, REFUSED);
        let same = fault(
            "0.9.7",
            "usage",
            "GET /coworkers/cw_018f3b11d04e7a52c9e8f1a0b3c4d5e6/usage?window=week",
            None,
            &other_errno,
        );
        assert_eq!(fingerprint(&a), fingerprint(&same));
        for other in [
            fault("0.9.3", "usage", USAGE, None, TIMED_OUT),
            fault("0.9.3", "tools", USAGE, None, REFUSED),
            fault("0.10.0", "usage", USAGE, None, REFUSED),
            fault("0.9.3", "usage", USAGE, Some(503), REFUSED),
        ] {
            assert_ne!(fingerprint(&a), fingerprint(&other), "{other:?}");
        }
        assert_eq!(fingerprint(&a).len(), 12);
    }

    /// A code word is not an id, so `plan_unavailable` and `model_unavailable` stay two failures,
    /// and a path keeps its code words.
    #[test]
    fn code_words_are_not_collapsed_into_ids() {
        let plan = fault(
            "0.9.3",
            "turn",
            "POST /ag-ui",
            Some(400),
            "plan_unavailable",
        );
        let model = fault(
            "0.9.3",
            "turn",
            "POST /ag-ui",
            Some(400),
            "model_unavailable",
        );
        assert_ne!(fingerprint(&plan), fingerprint(&model));
        assert_eq!(
            endpoint_template("GET /plugins/catalog/acme-jira?x=1"),
            "GET /plugins/catalog/{x}"
        );
    }
}
