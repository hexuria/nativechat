//! A report: what a fault becomes when the person chooses to send it to the project's issues.
//!
//! A report is built from an allowlist. Every field here is one this module chose to send, each
//! passed through [`redact::redact`] first; nothing else of the notice leaves the Mac, whatever
//! it grows to hold. The preview shows [`Report`] as it is, so what is seen is what is sent.

pub mod fingerprint;
pub mod redact;

use crate::notifications::Notice;
use fingerprint::{Kind, Parts};
use redact::{Known, redact};

/// The payload's version. The relay refuses a schema it does not know rather than guessing.
pub const SCHEMA: u32 = 1;

/// The longest message a report carries: the start says what failed, and a report is read on a
/// phone as often as not.
const MESSAGE_CHARS: usize = 2000;

/// What is sent, all of it. Built only by [`Report::from_notice`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    pub schema: u32,
    /// `fault` today; `freeze` and `crash` when gpui-sentry reports.
    pub kind: &'static str,
    pub app_version: String,
    /// `macos` and nothing finer: the version would narrow who sent it for little gain.
    pub os: &'static str,
    /// The place word, `usage`.
    pub place: &'static str,
    /// `GET /coworkers/{id}/usage`: the request's shape, never its ids or query.
    pub endpoint: Option<String>,
    pub status: Option<u16>,
    /// The source line the app raised it at, `src/state.rs:8258`: repo-relative, no home path.
    pub raised_at: String,
    /// How many times it happened on this Mac.
    pub count: u32,
    /// The failure's own text, redacted. Absent unless the person ticks "include the server's
    /// own text" in the preview: the default report says what failed, not what was said.
    pub message: Option<String>,
    /// Which failure this is, for the relay to find an issue already filed.
    pub fingerprint: String,
}

impl Report {
    /// The report a fault notice would send, or `None` for a notice that is not a fault.
    /// `with_message` is the preview's "include the server's own text".
    pub fn from_notice(notice: &Notice, known: &Known, with_message: bool) -> Option<Self> {
        let fault = notice.fault.as_ref()?;
        let app_version = env!("CARGO_PKG_VERSION").to_string();
        let raw = notice.raw.as_deref().unwrap_or(&notice.said);
        let redacted = redact(raw, known);
        let endpoint = fault
            .endpoint
            .as_deref()
            .map(|endpoint| fingerprint::endpoint_template(&redact(endpoint, known)));
        let fingerprint = fingerprint::fingerprint(&Parts {
            kind: Kind::Fault,
            app_version: &app_version,
            place: fault.place.word(),
            endpoint: endpoint.as_deref(),
            status: fault.status,
            message: &redacted,
            frames: &[],
        });
        Some(Self {
            schema: SCHEMA,
            kind: Kind::Fault.word(),
            app_version,
            os: "macos",
            place: fault.place.word(),
            endpoint,
            status: fault.status,
            raised_at: repo_relative(&notice.code),
            count: fault.count,
            message: with_message.then(|| redacted.chars().take(MESSAGE_CHARS).collect()),
            fingerprint,
        })
    }
}

/// `notice.code` as a path in the repository: `src/state.rs:8258`. A build from another
/// checkout records an absolute path, which names the person's home; only the part from `src/`
/// is kept.
fn repo_relative(code: &str) -> String {
    match code.find("src/") {
        Some(at) => code[at..].to_string(),
        None => code.rsplit('/').next().unwrap_or(code).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faults::Place;
    use crate::notifications::FaultFacts;

    fn notice(raw: &str, code: &str) -> Notice {
        let mut notice = Notice::new(
            Some("cw_018f3a2b9c7d7e10a1b2c3d4e5f60718".into()),
            "Usage",
            "Could not load this bot's usage.",
            std::panic::Location::caller(),
        );
        notice.code = code.into();
        notice.raw = Some(raw.into());
        notice.fault = Some(FaultFacts {
            place: Place::Usage,
            endpoint: Some(
                "GET /coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage?window=month".into(),
            ),
            status: None,
            count: 3,
            last_ms: 0,
            resolved: false,
        });
        notice
    }

    /// The report carries the failure's shape and nothing of who or where: no Bot id, no host,
    /// no query, no home path; and the message only when the person asked for it.
    #[test]
    fn a_report_says_what_failed_and_not_who_or_where() {
        let raw = "error sending request for url (http://10.0.0.4:1447/coworkers/cw_018f3a2b9c7d7e10a1b2c3d4e5f60718/usage?window=month)";
        let n = notice(raw, "/Users/exampleuser/code/opengrok/src/state.rs:8258");
        let known = Known {
            home: Some("/Users/exampleuser".into()),
            names: vec![],
        };
        let report = Report::from_notice(&n, &known, false).expect("a fault reports");
        assert_eq!(
            report.endpoint.as_deref(),
            Some("GET /coworkers/{id}/usage")
        );
        assert_eq!(report.raised_at, "src/state.rs:8258");
        assert_eq!(report.message, None, "the text only when ticked");
        assert_eq!(report.count, 3);
        let sent = serde_json::to_string(&report).expect("a report serializes");
        for gone in ["cw_018f", "10.0.0.4", "1447", "window", "exampleuser"] {
            assert!(!sent.contains(gone), "{gone} in {sent}");
        }

        let with_text = Report::from_notice(&n, &known, true).expect("a fault reports");
        let message = with_text.message.expect("ticked, the text comes");
        assert!(message.contains("error sending request"));
        assert!(!message.contains("10.0.0.4"));
        assert_eq!(
            with_text.fingerprint, report.fingerprint,
            "ticking does not change which issue"
        );
    }

    /// A notice that is not a fault (a failed turn, a test notice) has no report.
    #[test]
    fn only_a_fault_has_a_report() {
        let mut n = notice("boom", "src/state.rs:1");
        n.fault = None;
        assert_eq!(Report::from_notice(&n, &Known::default(), true), None);
    }
}
