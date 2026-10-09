//! A report as a GitHub issue: its title, its body, and the link that opens GitHub's new-issue
//! page with both filled in.
//!
//! This is the report's first way out, before the relay exists: the person's browser opens the
//! page, they sign in to GitHub if they are not, and they press Submit there. Nothing is sent by
//! the app. GitHub's page takes the title and body from the link's query, and a link that long
//! is refused (`414`, measured at about 6,500 characters logged out: scratchpad
//! issue-reporting-grounded.html), so a body that would make the link too long goes on the
//! clipboard and the page says to paste it.

use super::Report;
use super::agent::Findings;

/// The repository the desktop app's reports are filed in.
pub const REPO: &str = "hexuria/opengrok";

/// The label every report carries, so a maintainer can find them; the fingerprint rides in the
/// body as a hidden marker rather than as a label of its own.
pub const LABEL: &str = "auto-report";

/// The longest link opened. Under the measured limit with room for a browser's own encoding.
pub const LINK_CHARS: usize = 6000;

/// What the body says when the full body would not fit in the link and is on the clipboard.
pub const PASTE_NOTE: &str = "The full report is on your clipboard: paste it here, over this line.";

/// How the report leaves: the page to open, and the body to put on the clipboard first when it
/// did not fit in the link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueLink {
    pub url: String,
    pub clipboard: Option<String>,
}

/// The issue's title: what failed, where, and how it answered. `Usage failed: GET
/// /coworkers/{id}/usage, no answer`.
pub fn title(report: &Report, found: Option<&Findings>) -> String {
    if let Some(found) = found.filter(|found| !found.title.is_empty()) {
        return neutralize(&found.title);
    }
    let place = report.place;
    let request = report.endpoint.as_deref().unwrap_or("in the app");
    let answer = match report.status {
        Some(status) => format!("HTTP {status}"),
        None => "no answer".to_string(),
    };
    let mut place = place.chars();
    let place: String = place
        .next()
        .map(|first| first.to_uppercase().chain(place).collect())
        .unwrap_or_default();
    neutralize(&format!("{place} failed: {request}, {answer}"))
}

/// The issue's body, in Markdown: the facts as a list, the app's sentence, the failure's text in
/// a fenced block when the person included it, and the fingerprint as a hidden marker the relay
/// will match on.
pub fn body(report: &Report, found: Option<&Findings>) -> String {
    let mut lines = vec![
        format!("**What failed:** {}", neutralize(&report.said)),
        String::new(),
    ];
    if let Some(found) = found {
        lines.extend(findings(found));
    }
    lines.extend([
        format!("- **Place:** {}", report.place),
        format!(
            "- **Request:** `{}`",
            report.endpoint.as_deref().unwrap_or("none")
        ),
        format!(
            "- **Status:** {}",
            report
                .status
                .map_or_else(|| "no answer".to_string(), |status| status.to_string())
        ),
        format!("- **Raised at:** `{}`", report.raised_at),
        format!(
            "- **Seen:** {} time{}, {} to {}",
            report.count,
            if report.count == 1 { "" } else { "s" },
            report.first_day,
            report.last_day
        ),
        format!("- **App:** {} on {}", report.app_version, report.os),
    ]);
    if let Some(text) = &report.text {
        lines.push(String::new());
        lines.push("```text".to_string());
        lines.push(fence_safe(text));
        lines.push("```".to_string());
    }
    lines.push(String::new());
    lines.push(
        "_Filed from OpenGrok's fault window. Nothing here names the person or their machine._"
            .to_string(),
    );
    lines.push(format!("<!-- fp:{} -->", report.fingerprint));
    lines.join("\n")
}

/// What the agent found, as the issue's second section: its summary, the evidence it rests on,
/// where it suspects the code, and the steps to see it again.
fn findings(found: &Findings) -> Vec<String> {
    let mut lines = vec!["### What the person's agent found".to_string()];
    if !found.summary.is_empty() {
        lines.push(neutralize(&found.summary));
    }
    for line in &found.evidence {
        lines.push(format!("- {}", neutralize(line)));
    }
    if !found.suspect.is_empty() {
        lines.push(format!("- **Suspect:** `{}`", fence_safe(&found.suspect)));
    }
    if !found.repro.is_empty() {
        lines.push(String::new());
        lines.push("**To see it again:**".to_string());
        for (at, step) in found.repro.iter().enumerate() {
            lines.push(format!("{}. {}", at + 1, neutralize(step)));
        }
    }
    lines.push(String::new());
    lines
}

/// The new-issue page for `report`, with the body in the link when it fits and on the clipboard
/// when it does not.
pub fn issue_link(report: &Report, found: Option<&Findings>) -> IssueLink {
    let title = title(report, found);
    let body = body(report, found);
    let full = new_issue_url(&title, &body);
    if full.len() <= LINK_CHARS {
        return IssueLink {
            url: full,
            clipboard: None,
        };
    }
    let short = format!("{PASTE_NOTE}\n\n<!-- fp:{} -->", report.fingerprint);
    IssueLink {
        url: new_issue_url(&title, &short),
        clipboard: Some(body),
    }
}

fn new_issue_url(title: &str, body: &str) -> String {
    let base = format!("https://github.com/{REPO}/issues/new");
    // The base is a constant that parses; the pairs are encoded by the url crate.
    #[allow(clippy::expect_used)]
    let mut url = url::Url::parse(&base).expect("the new-issue address parses");
    url.query_pairs_mut()
        .append_pair("title", title)
        .append_pair("labels", LABEL)
        .append_pair("body", body);
    url.into()
}

/// Text that cannot ping anyone or close an issue: an `@name` would notify a GitHub user, and
/// `#123` or "fixes #123" would link or close an issue. A zero-width joiner after `@` and `#`
/// keeps them readable and inert.
fn neutralize(text: &str) -> String {
    text.replace('@', "@\u{2060}").replace('#', "#\u{2060}")
}

/// The failure's text as the inside of a fenced block: a run of backticks in it would end the
/// fence early and let the rest render as Markdown, so each backtick gets a zero-width joiner.
fn fence_safe(text: &str) -> String {
    neutralize(text).replace('`', "`\u{2060}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Kind, SCHEMA};

    fn report(text: Option<&str>) -> Report {
        Report {
            schema: SCHEMA,
            kind: Kind::Fault,
            component: "desktop",
            app_version: "0.9.3".into(),
            os: "macOS 26.1".into(),
            place: "usage",
            endpoint: Some("GET /coworkers/{id}/usage".into()),
            status: None,
            raised_at: "src/state.rs:8258".into(),
            count: 3,
            first_day: "2026-10-09".into(),
            last_day: "2026-10-09".into(),
            said: "Could not load this bot's usage.".into(),
            text: text.map(str::to_string),
            fingerprint: "3fa91c0e2b7d".into(),
        }
    }

    fn query(url: &str, key: &str) -> Option<String> {
        url::Url::parse(url)
            .ok()?
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    }

    /// The link opens the project's new-issue page with the title, the label and a body that
    /// carries the facts and the hidden fingerprint.
    #[test]
    fn the_link_opens_a_filled_new_issue_page() {
        let link = issue_link(&report(Some("Connection refused (os error 61)")), None);
        assert!(
            link.url
                .starts_with("https://github.com/hexuria/opengrok/issues/new?")
        );
        assert_eq!(link.clipboard, None);
        assert_eq!(
            query(&link.url, "title").as_deref(),
            Some("Usage failed: GET /coworkers/{id}/usage, no answer")
        );
        assert_eq!(query(&link.url, "labels").as_deref(), Some(LABEL));
        let body = query(&link.url, "body").expect("a body");
        assert!(body.contains("**What failed:** Could not load this bot's usage."));
        assert!(body.contains("`src/state.rs:8258`"));
        assert!(body.contains("Connection refused (os error 61)"));
        assert!(body.ends_with("<!-- fp:3fa91c0e2b7d -->"));
    }

    /// A body too long for a link goes on the clipboard, and the page asks for it to be pasted;
    /// the fingerprint stays in the link either way.
    #[test]
    fn a_long_body_goes_on_the_clipboard() {
        let long = "x ".repeat(5000);
        let link = issue_link(&report(Some(&long)), None);
        assert!(link.url.len() <= LINK_CHARS, "{}", link.url.len());
        let clipboard = link.clipboard.expect("the full body is on the clipboard");
        assert!(clipboard.contains(long.trim()));
        let body = query(&link.url, "body").expect("a body");
        assert!(body.starts_with(PASTE_NOTE));
        assert!(body.contains("fp:3fa91c0e2b7d"));
    }

    /// What the person's agent found writes the issue's title and its own section, before the
    /// facts, and the fingerprint does not change.
    #[test]
    fn the_agents_findings_write_the_title_and_a_section() {
        let found = Findings {
            title: "Usage fails when the gateway returns 502".into(),
            summary: "The usage read got a 502 while other reads worked.".into(),
            evidence: vec!["status 502".into(), "other reads ok".into()],
            suspect: "src/state.rs:8258".into(),
            repro: vec!["open a Bot's settings".into()],
        };
        let r = report(None);
        assert_eq!(
            title(&r, Some(&found)),
            "Usage fails when the gateway returns 502"
        );
        let body = body(&r, Some(&found));
        let agent = body
            .find("### What the person's agent found")
            .expect("a section");
        let facts = body.find("- **Place:**").expect("the facts");
        assert!(agent < facts, "the agent's section comes before the facts");
        for line in [
            "- status 502",
            "- **Suspect:** `src/state.rs:8258`",
            "1. open a Bot's settings",
        ] {
            assert!(body.contains(line), "{line} in {body}");
        }
        assert!(body.ends_with("<!-- fp:3fa91c0e2b7d -->"));
    }

    /// Nothing in a report can ping a GitHub user, link or close an issue, or break out of its
    /// code block.
    #[test]
    fn a_report_cannot_ping_close_or_escape() {
        let tricky = "fixes #1, cc @maintainer\n```\n**bold**";
        let body = body(&report(Some(tricky)), None);
        assert!(!body.contains("@maintainer"), "{body}");
        assert!(!body.contains("#1"), "{body}");
        let inside = body.split("```text\n").nth(1).expect("a fenced block");
        let closing = inside.find("\n```").expect("the fence closes");
        assert!(
            inside[..closing].contains("**bold**"),
            "the text stays inside the fence"
        );
    }
}
