//! The second gate: the person's own agent weighs a fault the first gate let through, on the
//! server's `POST /triage`, and for a bug writes the issue from the evidence it was given.
//!
//! A verdict is advice. It is taken only when the model is confident and the server answered in
//! the shape agreed; anything else (an error, a timeout, a server without the route, "unsure",
//! a low confidence) is the person filling the report in by hand, prefilled with what the app
//! already knows. Whatever the model wrote is redacted again before it is shown, because a model
//! can still write a name.

use super::redact::{Known, redact};
use crate::opengrok::{OpenGrokError, TriageCall, TriageVerdict};

/// The least confidence a verdict is taken at: below it, the person decides.
pub const SURE_ENOUGH: f64 = 0.8;

/// What the agent found, ready for the issue: every line redacted again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Findings {
    pub title: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub suspect: String,
    pub repro: Vec<String>,
}

/// Where the second gate is for one report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agent {
    /// The server is being asked.
    Asking,
    /// A confident bug: the issue is written from these.
    Found(Findings),
    /// A confident "your side" or "noise": the person's to fix, or nothing to fix, in the
    /// agent's words.
    Advice { your_side: bool, words: String },
    /// No verdict to take: the person fills it in. Why, in their words.
    Manual(&'static str),
}

pub const NO_ROUTE: &str = "Your server cannot triage yet, so fill it in yourself.";
pub const NO_ANSWER: &str = "Your agent did not answer in time, so fill it in yourself.";
pub const NOT_SURE: &str = "Your agent was not sure, so fill it in yourself.";
pub const NOT_READ: &str = "Your agent's answer could not be read, so fill it in yourself.";
pub const NOT_ASKED: &str = "Triage was not asked, so fill it in yourself.";

/// What the server's answer means for the report.
pub fn decide(answer: Result<TriageVerdict, OpenGrokError>, known: &Known) -> Agent {
    let verdict = match answer {
        Ok(verdict) => verdict,
        Err(error) => {
            return Agent::Manual(match error.status {
                Some(404 | 405) => NO_ROUTE,
                Some(422) => NOT_READ,
                _ => NO_ANSWER,
            });
        }
    };
    if !(SURE_ENOUGH..=1.0).contains(&verdict.confidence) {
        return Agent::Manual(NOT_SURE);
    }
    let clean = |text: &str| redact(text.trim(), known);
    match verdict.verdict {
        TriageCall::Bug if !verdict.title.trim().is_empty() => Agent::Found(Findings {
            title: clean(&verdict.title),
            summary: clean(&verdict.summary),
            evidence: verdict.evidence.iter().map(|line| clean(line)).collect(),
            suspect: clean(&verdict.suspect),
            repro: verdict.repro.iter().map(|line| clean(line)).collect(),
        }),
        TriageCall::YourSide | TriageCall::Noise => {
            let words = if verdict.advice.trim().is_empty() {
                clean(&verdict.summary)
            } else {
                clean(&verdict.advice)
            };
            if words.is_empty() {
                return Agent::Manual(NOT_SURE);
            }
            Agent::Advice {
                your_side: verdict.verdict == TriageCall::YourSide,
                words,
            }
        }
        TriageCall::Bug | TriageCall::Unsure => Agent::Manual(NOT_SURE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(call: TriageCall, confidence: f64) -> TriageVerdict {
        TriageVerdict {
            verdict: call,
            confidence,
            title: "Usage fails when Blitz Ops hits a 502".into(),
            summary: "The read got a 502 from intranet.acme.corp.".into(),
            evidence: vec!["status 502".into()],
            suspect: "src/state.rs:8258".into(),
            repro: vec!["open Usage".into()],
            advice: String::new(),
        }
    }

    fn known() -> Known {
        Known {
            home: None,
            names: vec!["Blitz Ops".into()],
        }
    }

    /// A confident bug writes the issue, with whatever the model wrote redacted again.
    #[test]
    fn a_confident_bug_writes_the_issue_redacted() {
        let Agent::Found(found) = decide(Ok(verdict(TriageCall::Bug, 0.9)), &known()) else {
            panic!("a confident bug is found");
        };
        assert_eq!(found.title, "Usage fails when {name} hits a 502");
        assert!(!found.summary.contains("acme"), "{}", found.summary);
        assert_eq!(found.suspect, "src/state.rs:8258");
    }

    /// Anything short of a confident, readable verdict is the manual report, with why.
    #[test]
    fn anything_short_is_filled_in_by_hand() {
        let k = known();
        assert_eq!(
            decide(Ok(verdict(TriageCall::Bug, 0.5)), &k),
            Agent::Manual(NOT_SURE)
        );
        assert_eq!(
            decide(Ok(verdict(TriageCall::Unsure, 0.95)), &k),
            Agent::Manual(NOT_SURE)
        );
        let untitled = TriageVerdict {
            title: " ".into(),
            ..verdict(TriageCall::Bug, 0.9)
        };
        assert_eq!(decide(Ok(untitled), &k), Agent::Manual(NOT_SURE));
        let cases = [
            (OpenGrokError::status(404, "not found"), NO_ROUTE),
            (
                OpenGrokError::status(422, "the model did not answer"),
                NOT_READ,
            ),
            (OpenGrokError::status(502, "gateway"), NO_ANSWER),
            (OpenGrokError::message("operation timed out"), NO_ANSWER),
        ];
        for (error, why) in cases {
            assert_eq!(decide(Err(error), &k), Agent::Manual(why));
        }
    }

    /// A confident "your side" gives the person the agent's advice, redacted.
    #[test]
    fn a_confident_your_side_gives_advice() {
        let mine = TriageVerdict {
            advice: "Blitz Ops cannot reach your proxy; check it is running.".into(),
            ..verdict(TriageCall::YourSide, 0.85)
        };
        assert_eq!(
            decide(Ok(mine), &known()),
            Agent::Advice {
                your_side: true,
                words: "{name} cannot reach your proxy; check it is running.".into(),
            }
        );
    }
}
