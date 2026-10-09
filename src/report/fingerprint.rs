//! Which failure a report is, as twelve hex characters: the same for the same failure on any
//! Mac, any Bot, any port and any build of one minor version; different for a different failure.
//! The relay files one issue per fingerprint, so two people hitting one bug add to one issue.
//!
//! Everything that varies between occurrences is taken out before hashing: ids, numbers, hosts,
//! the query, the patch version, and, for a stack, the per-build crate hash (`nativechat[95ef…]`)
//! and symbol hash (`::h0123…`) that would otherwise make every build a new issue.

use std::sync::LazyLock;

use regex::Regex;
use sha2::{Digest, Sha256};

use super::redact::hide_ids;

/// How many frames of a stack name a freeze: deep enough to tell two freezes apart, shallow
/// enough that a change three calls down does not split one freeze into two issues.
pub const TOP_FRAMES: usize = 8;

/// The longest normalized message hashed: a failure is told apart by its start.
const MESSAGE_CHARS: usize = 200;

/// What kind of report: the first word of the canon, so a fault and a freeze never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Fault,
    Freeze,
    Crash,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Fault => "fault",
            Kind::Freeze => "freeze",
            Kind::Crash => "crash",
        }
    }
}

/// Everything a fingerprint is made of, already redacted.
#[derive(Debug, Clone)]
pub struct Parts<'a> {
    pub kind: Kind,
    /// The app's version; only major.minor is hashed.
    pub app_version: &'a str,
    /// The place word (`usage`).
    pub place: &'a str,
    /// `GET /coworkers/{id}/usage?window=month`, or none.
    pub endpoint: Option<&'a str>,
    pub status: Option<u16>,
    /// The failure's text.
    pub message: &'a str,
    /// Demangled frame names, innermost first.
    pub frames: &'a [String],
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
    let path = rest.split(['?', '#']).next().unwrap_or("");
    let path = hide_ids(path);
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

static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"\d+").expect("the number pattern compiles")
});
static SPACE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"\s+").expect("the space pattern compiles")
});

/// The message with what varies between occurrences taken out: ids, then every number, so a
/// port, a duration or an os error number does not split one failure. Lowercased, one space.
pub fn normalize_message(message: &str) -> String {
    let text = hide_ids(message).to_lowercase();
    let text = NUMBER.replace_all(&text, "#");
    let text = SPACE.replace_all(text.trim(), " ");
    text.chars().take(MESSAGE_CHARS).collect()
}

static CRATE_HASH: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"\[[0-9a-f]{8,}\]").expect("the crate-hash pattern compiles")
});
static SYMBOL_HASH: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"::h[0-9a-f]{16}\b").expect("the symbol-hash pattern compiles")
});
static OFFSET: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"\s*\+\s*(?:0x[0-9a-f]+|\d+)\b|\s+\(in [^)]*\)|\s+at\s+\S+$|\s+\[[^\]]*:\d+\]$")
        .expect("the offset pattern compiles")
});

/// A frame as it reads on any build: the function's path, without the crate's build hash, the
/// symbol hash, the offset or the file and line, all of which change with every build.
pub fn normalize_frame(frame: &str) -> String {
    let frame = CRATE_HASH.replace_all(frame, "");
    let frame = SYMBOL_HASH.replace_all(&frame, "");
    OFFSET.replace_all(&frame, "").trim().to_string()
}

/// The canonical text a fingerprint hashes, kept for tests and for the preview's "what makes
/// this the same issue" line.
pub fn canon(parts: &Parts) -> String {
    let mut canon = vec![
        parts.kind.word().to_string(),
        major_minor(parts.app_version).to_string(),
        parts.place.to_string(),
        parts.endpoint.map(endpoint_template).unwrap_or_default(),
        status_class(parts.status).to_string(),
        normalize_message(parts.message),
    ];
    canon.extend(
        parts
            .frames
            .iter()
            .take(TOP_FRAMES)
            .map(|f| normalize_frame(f)),
    );
    canon.join("|")
}

/// The fingerprint: the first six bytes of the canon's SHA-256, as twelve hex characters.
pub fn fingerprint(parts: &Parts) -> String {
    let digest = Sha256::digest(canon(parts).as_bytes());
    digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSPORT_A: &str = "error sending request for url (http://{host}/coworkers/{id}/usage)\n\nCaused by:\n    0: client error (Connect)\n    1: tcp connect error\n    2: Connection refused (os error 61)";
    const TIMEOUT: &str = "error sending request for url (http://{host}/coworkers/{id}/usage)\n\nCaused by:\n    0: operation timed out (os error 60)";

    fn fault<'a>(
        version: &'a str,
        place: &'a str,
        endpoint: &'a str,
        status: Option<u16>,
        message: &'a str,
    ) -> Parts<'a> {
        Parts {
            kind: Kind::Fault,
            app_version: version,
            place,
            endpoint: Some(endpoint),
            status,
            message,
            frames: &[],
        }
    }

    /// The same failure on another patch version, Bot, window and port is one issue; another
    /// place, status class, minor version or chain is another.
    #[test]
    fn one_failure_is_one_fingerprint_and_another_is_not() {
        let a = fault(
            "0.9.3",
            "usage",
            "GET /coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage?window=month",
            None,
            TRANSPORT_A,
        );
        let other_errno = TRANSPORT_A.replace("61", "62");
        let same = fault(
            "0.9.7",
            "usage",
            "GET /coworkers/cw_018f3b11d04e7a52c9e8f1a0b3c4d5e6/usage?window=week",
            None,
            &other_errno,
        );
        assert_eq!(
            fingerprint(&a),
            fingerprint(&same),
            "{}\n{}",
            canon(&a),
            canon(&same)
        );

        let others = [
            fault(
                "0.9.3",
                "usage",
                "GET /coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage",
                None,
                TIMEOUT,
            ),
            fault(
                "0.9.3",
                "tools",
                "GET /coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage",
                None,
                TRANSPORT_A,
            ),
            fault(
                "0.10.0",
                "usage",
                "GET /coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage",
                None,
                TRANSPORT_A,
            ),
            fault(
                "0.9.3",
                "usage",
                "GET /coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage",
                Some(503),
                TRANSPORT_A,
            ),
        ];
        for other in others {
            assert_ne!(fingerprint(&a), fingerprint(&other), "{}", canon(&other));
        }
        assert_eq!(fingerprint(&a).len(), 12);
    }

    /// A code word is not an id: `plan_unavailable` and `model_unavailable` stay two failures.
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
            endpoint_template("GET /coworkers/cw_unavailable/x"),
            "GET /coworkers/cw_unavailable/x"
        );
    }

    /// A freeze on a rebuilt app (new crate hash, new symbol hashes, new offsets and lines) is
    /// the same freeze; a different top frame is a different one.
    #[test]
    fn a_rebuild_does_not_split_a_freeze() {
        let first: Vec<String> = vec![
            "nativechat[95efe4507a457832]::state::AppState::sync_site_logins::h0123456789abcdef + 0x1a4".into(),
            "gpui[aa01bb02cc03dd04]::app::App::update (in nativechat) + 112".into(),
            "CFRunLoopRun".into(),
        ];
        let rebuilt: Vec<String> = vec![
            "nativechat[11223344aabbccdd]::state::AppState::sync_site_logins::hfedcba9876543210 + 0x2b0".into(),
            "gpui[99887766ffeeddcc]::app::App::update (in nativechat) + 140".into(),
            "CFRunLoopRun".into(),
        ];
        let other: Vec<String> = vec![
            "nativechat[95efe4507a457832]::state::AppState::reload_recipes::h0123456789abcdef + 0x10".into(),
            "gpui[aa01bb02cc03dd04]::app::App::update (in nativechat) + 112".into(),
            "CFRunLoopRun".into(),
        ];
        let freeze = |frames| Parts {
            kind: Kind::Freeze,
            app_version: "0.9.3",
            place: "app",
            endpoint: None,
            status: None,
            message: "",
            frames,
        };
        assert_eq!(
            fingerprint(&freeze(&first)),
            fingerprint(&freeze(&rebuilt)),
            "{}",
            canon(&freeze(&rebuilt))
        );
        assert_ne!(fingerprint(&freeze(&first)), fingerprint(&freeze(&other)));
        assert_eq!(
            normalize_frame(&first[0]),
            "nativechat::state::AppState::sync_site_logins"
        );
    }
}
