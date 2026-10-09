//! A report: what a fault becomes when the person chooses to send it to the project's issues.
//!
//! A report is built from an allowlist. Every field here is one this module chose to send, each
//! passed through [`redact::redact`] first; nothing else of the notice leaves the Mac, whatever
//! it grows to hold. The preview shows [`Report`] as it is, so what is seen is what is sent.

pub mod fingerprint;
pub mod redact;

use crate::notifications::Notice;
use fingerprint::FingerprintInput;
use redact::{Known, redact};

/// The payload's version. The relay refuses a schema it does not know rather than guessing.
pub const SCHEMA: u32 = 1;

/// The longest text a report carries: the start says what failed, and a report is read on a
/// phone as often as not.
const SENT_TEXT_CHARS: usize = 2000;

/// What kind of thing a report is about. Only faults today; gpui-sentry adds freezes and
/// crashes, each with its own fingerprint inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Fault,
}

impl Kind {
    /// The kind as the report and the fingerprint spell it.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Fault => "fault",
        }
    }
}

/// What is sent, all of it, read by the preview and built only by [`Report::from_notice`].
///
/// This is the wire shape the issue relay receives. The relay does not exist yet, so this crate
/// is its source: the relay's schema is to be transcribed from here, naming this file, and a
/// change here is a new `SCHEMA` (the plan: scratchpad issue-reporting-grounded.html §6.3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    pub schema: u32,
    pub kind: Kind,
    /// Which part of OpenGrok it is about, for the relay to file it in the right repository:
    /// `desktop` (hexuria/opengrok) for every fault the app raises.
    pub component: &'static str,
    pub app_version: String,
    /// `macOS 26.1`: major and minor only, which says which system without narrowing who.
    pub os: String,
    /// The place word, `usage`.
    pub place: &'static str,
    /// `GET /coworkers/{id}/usage`: the request's shape, never its ids, names or query.
    pub endpoint: Option<String>,
    pub status: Option<u16>,
    /// The source line the app raised it at, `src/state.rs:8258`: repo-relative, no home path.
    pub raised_at: String,
    /// How many times it happened on this Mac, and on which days (UTC, no time of day).
    pub count: u32,
    pub first_day: String,
    pub last_day: String,
    /// The sentence the app wrote for the person, "Could not load this bot's usage.": always
    /// sent, because the app wrote it.
    pub said: String,
    /// The failure's own text (the server's or the transport's), redacted. Only when the person
    /// ticks "include the server's own text" in the preview.
    pub text: Option<String>,
    /// Which failure this is, for the relay to find an issue already filed.
    pub fingerprint: String,
}

impl Report {
    /// The report a fault notice would send, or `None` for a notice that is not a fault.
    /// `with_text` is the preview's "include the server's own text".
    pub fn from_notice(notice: &Notice, known: &Known, with_text: bool) -> Option<Self> {
        let fault = notice.fault.as_ref()?;
        let app_version = env!("CARGO_PKG_VERSION").to_string();
        let said = redact(&notice.said, known);
        let text = redact(notice.raw.as_deref().unwrap_or(&notice.said), known);
        let endpoint = fault
            .endpoint
            .as_deref()
            .map(fingerprint::endpoint_template);
        let fingerprint = fingerprint::fingerprint(&FingerprintInput {
            kind: Kind::Fault,
            app_version: &app_version,
            place: fault.place.word(),
            endpoint: endpoint.as_deref(),
            status: fault.status,
            message: &text,
        });
        Some(Self {
            schema: SCHEMA,
            kind: Kind::Fault,
            component: "desktop",
            app_version,
            os: macos_version(),
            place: fault.place.word(),
            endpoint,
            status: fault.status,
            raised_at: repo_relative(&notice.code),
            count: fault.count,
            first_day: day(notice.at_ms),
            last_day: day(fault.last_ms.max(notice.at_ms)),
            said,
            text: with_text.then(|| text.chars().take(SENT_TEXT_CHARS).collect()),
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

/// The UTC day of `ms`, `2026-10-09`.
fn day(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|at| at.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// `macOS 26.1`, read from the system's own version file; `macOS` where it cannot be read.
fn macos_version() -> String {
    std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist")
        .ok()
        .and_then(|plist| product_version(&plist))
        .map_or_else(|| "macOS".to_string(), |version| format!("macOS {version}"))
}

/// The major.minor of `ProductVersion` in a SystemVersion.plist.
fn product_version(plist: &str) -> Option<String> {
    let after = plist.split("<key>ProductVersion</key>").nth(1)?;
    let value = after.split("<string>").nth(1)?.split("</string>").next()?;
    let mut parts = value.trim().split('.');
    let major = parts.next().filter(|part| !part.is_empty())?;
    Some(match parts.next() {
        Some(minor) => format!("{major}.{minor}"),
        None => major.to_string(),
    })
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
            last_ms: notice.at_ms,
            resolved: false,
        });
        notice
    }

    /// The report carries the failure's shape and the app's own sentence, and nothing of who or
    /// where: no Bot id, host, query or home path; the failure's own text only when asked for.
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
        assert_eq!(report.said, "Could not load this bot's usage.");
        assert_eq!(report.text, None, "the failure's own text only when ticked");
        assert_eq!(report.count, 3);
        assert_eq!(report.first_day, report.last_day);
        assert!(report.os.starts_with("macOS"), "{}", report.os);
        let sent = serde_json::to_string(&report).expect("a report serializes");
        assert!(sent.contains(r#""kind":"fault""#), "{sent}");
        for gone in ["cw_018f", "10.0.0.4", "1447", "window", "exampleuser"] {
            assert!(!sent.contains(gone), "{gone} in {sent}");
        }

        let ticked = Report::from_notice(&n, &known, true).expect("a fault reports");
        let text = ticked.text.expect("ticked, the text comes");
        assert!(text.contains("error sending request"));
        assert!(!text.contains("10.0.0.4"));
        assert_eq!(
            ticked.fingerprint, report.fingerprint,
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

    /// The system's version is cut to major.minor: the patch narrows who sent it for nothing.
    #[test]
    fn the_system_version_is_major_and_minor() {
        let plist = "<dict><key>ProductName</key><string>macOS</string>\
                     <key>ProductVersion</key><string>26.1.2</string></dict>";
        assert_eq!(product_version(plist).as_deref(), Some("26.1"));
        assert_eq!(product_version("<dict/>"), None);
    }
}
