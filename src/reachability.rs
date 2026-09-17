//! Whether the app can reach what it talks to, held as a state rather than written down as a
//! verdict.
//!
//! A refusal from the server is a fact about one request at one moment, and a transcript — a list
//! of things that happened, each with a time against it — is the right place for it. Not being
//! able to reach the server is not that. It is true now and it stops being true later, without
//! anything happening in the app at all, and the moment it is written into a transcript it
//! becomes a line that outlives the condition it describes. So it lives here instead: one value
//! that the chrome reads, that a retry can clear, and that nothing has to remember to delete.

use std::time::Duration;

pub use crate::opengrok::Unreachable;

/// How long the app waits before asking again the first time.
///
/// Two seconds, the same wait the local-exec daemon loop uses (`RECONNECT_WAIT` in
/// `opengrok/local_exec.rs`), because a person who has just watched something fail wants to see
/// it tried again soon and a server that has just come back is usually back for good.
pub const FIRST_WAIT: Duration = Duration::from_secs(2);

/// The longest the app will ever wait between tries.
///
/// A gateway whose database containers are being brought back takes minutes, and hammering it
/// every two seconds for that whole time helps nobody. Thirty seconds is still soon enough that
/// a person who fixes the thing sees the app notice while they are still looking at it.
pub const MAX_WAIT: Duration = Duration::from_secs(30);

/// Doubling past this many failures cannot reach further than [`MAX_WAIT`], and stopping the
/// count here keeps the shift from running away on a connection that has been down for hours.
const MAX_DOUBLINGS: u32 = 8;

/// What the app can and cannot reach right now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reachability {
    /// The machine that is out of reach, or `None` while everything answers.
    unreachable: Option<Unreachable>,
    /// Tries that have failed since the last time something answered. The wait is read off this,
    /// and it is what a success resets.
    failures: u32,
    /// The last failure's own words, kept for the second line of the indicator — "error sending
    /// request for url (…)" is what tells a developer which port is dead.
    detail: String,
}

impl Reachability {
    pub fn unreachable(&self) -> Option<Unreachable> {
        self.unreachable
    }

    pub fn is_reachable(&self) -> bool {
        self.unreachable.is_none()
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// A try that did not get through. Answers true when this is news — a first failure, or a
    /// different machine than the one that was failing before — so a caller can notify only when
    /// something on screen would change.
    pub fn fail(&mut self, what: Unreachable, detail: &str) -> bool {
        let news = self.unreachable != Some(what);
        if news {
            // A different machine is a different problem, so the wait starts over rather than
            // inheriting a backoff that grew against something else.
            self.failures = 0;
        }
        self.unreachable = Some(what);
        self.failures = self.failures.saturating_add(1);
        self.detail = detail.trim().to_string();
        news
    }

    /// The server answered, whatever it answered — a `401`, a `500`, anything at all. Answers
    /// true when that cleared something.
    ///
    /// A refusal is proof the wire works, so it clears the server's own state. It clears nothing
    /// else: OpenGrok answering about coworkers says nothing about whether the gateway behind it
    /// has come back, and treating it as proof would take the indicator down while the thing it
    /// was about is still broken.
    pub fn server_answered(&mut self) -> bool {
        if self.unreachable != Some(Unreachable::Server) {
            return false;
        }
        self.clear();
        true
    }

    /// The whole path worked: the server answered, and what it answered came from the gateway.
    /// Answers true when that cleared something.
    pub fn all_clear(&mut self) -> bool {
        if self.unreachable.is_none() {
            return false;
        }
        self.clear();
        true
    }

    fn clear(&mut self) {
        self.unreachable = None;
        self.failures = 0;
        self.detail.clear();
    }

    /// How long to wait before the next try: [`FIRST_WAIT`], doubling, capped at [`MAX_WAIT`].
    pub fn wait(&self) -> Duration {
        let doublings = self.failures.saturating_sub(1).min(MAX_DOUBLINGS);
        let wait = FIRST_WAIT.saturating_mul(1u32 << doublings);
        wait.min(MAX_WAIT)
    }

    /// The two lines the indicator shows, or `None` while everything answers.
    ///
    /// Each names its own machine, and the gateway's line says in so many words that OpenGrok is
    /// fine — because the expensive mistake is not failing to name a machine, it is naming one
    /// and having the person restart it when the trouble was somewhere else.
    pub fn indicator(&self) -> Option<(String, String)> {
        let what = self.unreachable?;
        let (title, lead) = match what {
            Unreachable::Server => (
                "Reconnecting to OpenGrok…".to_string(),
                "The server is not answering.",
            ),
            Unreachable::Gateway => (
                "Waiting for the model gateway…".to_string(),
                "OpenGrok is answering; the model gateway behind it is not.",
            ),
        };
        let detail = if self.detail.is_empty() {
            lead.to_string()
        } else {
            format!("{lead} {}", clip(&self.detail, DETAIL_CHARS))
        };
        Some((title, detail))
    }
}

/// How much of a failure's own words the indicator's second line carries. Enough for the URL
/// that failed, which is the useful part, without the pill growing into a paragraph.
const DETAIL_CHARS: usize = 160;

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let head: String = text.chars().take(limit).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_state_is_reachable_and_shows_nothing() {
        let state = Reachability::default();
        assert!(state.is_reachable());
        assert_eq!(state.indicator(), None);
    }

    #[test]
    fn the_wait_grows_and_is_capped() {
        let mut state = Reachability::default();
        let mut waits = Vec::new();
        for _ in 0..10 {
            state.fail(Unreachable::Server, "error sending request");
            waits.push(state.wait());
        }
        assert_eq!(
            &waits[..5],
            &[
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                MAX_WAIT,
            ]
        );
        assert!(
            waits.iter().all(|wait| *wait <= MAX_WAIT),
            "the wait never passes the cap: {waits:?}"
        );
        assert_eq!(waits.last(), Some(&MAX_WAIT));
    }

    #[test]
    fn a_success_resets_the_wait() {
        let mut state = Reachability::default();
        for _ in 0..4 {
            state.fail(Unreachable::Server, "error sending request");
        }
        assert_eq!(state.wait(), Duration::from_secs(16));
        assert!(state.all_clear());
        assert_eq!(state.failures(), 0);
        assert_eq!(state.wait(), FIRST_WAIT, "the next outage starts over");
    }

    #[test]
    fn a_reachability_state_clears_itself() {
        let mut state = Reachability::default();
        assert!(state.fail(Unreachable::Server, "error sending request"));
        assert!(!state.is_reachable());
        assert!(state.all_clear(), "the first success is the news");
        assert!(state.is_reachable());
        assert!(!state.all_clear(), "and there is nothing left to clear");
        assert_eq!(state.indicator(), None);
    }

    #[test]
    fn the_server_answering_does_not_vouch_for_the_gateway() {
        let mut state = Reachability::default();
        state.fail(Unreachable::Gateway, "the gateway could not be reached");
        // The roster loaded, the recipes loaded, a coworker was renamed — OpenGrok is plainly up.
        // None of that is evidence about the machine it asks for models.
        assert!(!state.server_answered());
        assert_eq!(state.unreachable(), Some(Unreachable::Gateway));
        assert!(state.all_clear(), "only a model answer clears the gateway");
    }

    #[test]
    fn a_refusal_clears_the_server_because_it_proves_the_wire() {
        let mut state = Reachability::default();
        state.fail(Unreachable::Server, "error sending request");
        assert!(state.server_answered());
        assert!(state.is_reachable());
    }

    #[test]
    fn switching_machines_starts_the_wait_over() {
        let mut state = Reachability::default();
        for _ in 0..4 {
            state.fail(Unreachable::Server, "error sending request");
        }
        assert!(
            state.fail(Unreachable::Gateway, "the gateway could not be reached"),
            "a different machine is news"
        );
        assert_eq!(state.failures(), 1);
        assert_eq!(state.wait(), FIRST_WAIT);
        assert!(
            !state.fail(Unreachable::Gateway, "the gateway could not be reached"),
            "the same machine again is not"
        );
        assert_eq!(state.failures(), 2);
    }

    #[test]
    fn each_machine_is_named_and_the_gateway_says_the_server_is_fine() {
        let mut state = Reachability::default();
        state.fail(Unreachable::Server, "error sending request for url (…)");
        let (title, detail) = state.indicator().expect("a server outage shows");
        assert!(title.contains("OpenGrok"), "{title}");
        assert!(detail.contains("The server is not answering."), "{detail}");

        state.fail(
            Unreachable::Gateway,
            "the gateway could not be reached: error sending request for url \
             (http://127.0.0.1:29080/v1/models)",
        );
        let (title, detail) = state.indicator().expect("a gateway outage shows");
        assert!(title.contains("model gateway"), "{title}");
        assert!(
            detail.contains("OpenGrok is answering"),
            "nobody should restart the server over this: {detail}"
        );
        assert!(detail.contains("29080"), "the dead port survives: {detail}");
    }

    #[test]
    fn a_long_reason_is_clipped_rather_than_sprawling() {
        let mut state = Reachability::default();
        state.fail(Unreachable::Server, &"x".repeat(500));
        let (_, detail) = state.indicator().expect("shows");
        assert!(detail.chars().count() < 260, "{}", detail.len());
        assert!(detail.ends_with('…'), "{detail}");
    }
}
