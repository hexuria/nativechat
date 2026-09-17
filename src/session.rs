//! Whether the app still has a session, held as a state rather than written down as a verdict.
//!
//! This is the third of the three things a failed request can mean, and it has a file of its own
//! because the other two already have theirs and folding it into either is the mistake that
//! produced the bug it exists for. A turn went out with no `Authorization` header, the server had
//! no principal to bill it to and held it, and the sentence it sent back landed in the transcript
//! as a red line about spend limits — a verdict, for something that was never about the turn.
//!
//! [`crate::reachability`] holds the state that clears itself: the wire is down, nothing was
//! decided, and it stops being true when the wire returns. The transcript holds verdicts: the
//! server heard the request and decided against it, at a time, and that stays true forever.
//! Signed-out is neither. The wire is fine — a `401` is proof of it — and nothing was decided
//! about the request, because the server never got as far as the question. It will not clear on
//! its own and it will not clear by asking again. Only a person signing in clears it.
//!
//! So it is a state, like reachability, and not a line in the transcript; but its own state,
//! because a "reconnecting" pill that never reconnects is a lie, and a retry loop against it
//! would ask the same unanswerable question until the app was closed.

use crate::opengrok::Failure;

/// What the app knows about its own session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    /// The server has said it does not know who is asking. Set by a `401` on anything that is
    /// not a sign-in, cleared by signing in, and by nothing else.
    expired: bool,
}

impl Session {
    pub fn is_expired(&self) -> bool {
        self.expired
    }

    /// Read a failure for what it says about who the app is.
    ///
    /// Every failure the app hears about comes through here, and all but one kind leave it
    /// alone. A machine out of reach says nothing about the session — nothing was asked and
    /// nothing answered. A verdict says the opposite of this state: the server knew perfectly
    /// well who was asking and refused anyway. Only [`Failure::SignedOut`] is about the app's
    /// own name, and taking any of the others would be the flattening that caused the bug.
    ///
    /// Answers true when this is news, so a caller can notify only when something on screen
    /// would change.
    pub fn note(&mut self, failure: Failure) -> bool {
        match failure {
            Failure::SignedOut => {
                let news = !self.expired;
                self.expired = true;
                news
            }
            Failure::OutOfReach(_) | Failure::Verdict => false,
        }
    }

    /// Somebody signed in, or signed out on purpose. Answers true when that cleared something.
    ///
    /// Signing out deliberately clears it for the same reason signing in does: the state is
    /// "you believe you are working and you are not", and a person who has just pressed Sign Out
    /// believes no such thing.
    pub fn signed_in(&mut self) -> bool {
        let news = self.expired;
        self.expired = false;
        news
    }

    /// Whether a turn may be sent at all.
    ///
    /// Two facts, and they are different facts. The app has been told the session is gone — a
    /// `401` came back from a request that had already been given every chance the client has,
    /// including a refresh. Or the app is holding nothing it could authenticate with at all,
    /// which is knowable here, before anything leaves, without a round trip.
    ///
    /// The second is the one that matters most. Sending anyway costs a round trip and comes
    /// back as a red line in the transcript for something the app already knew.
    pub fn may_send(&self, holds_credential: bool) -> bool {
        !self.expired && holds_credential
    }

    /// The banner's two lines, or `None` while the session holds.
    ///
    /// It says what happened and what to do, in that order, and it does not say "try again":
    /// there is nothing to try. It also says plainly that nothing is being sent, because the
    /// alternative — a composer that looks ready and quietly refuses — is the shape of the bug
    /// this is here to end.
    pub fn banner(&self) -> Option<(String, String)> {
        self.expired.then(|| {
            (
                "You are signed out.".to_string(),
                "OpenGrok no longer recognises this app, so nothing is being sent. \
                 Sign in again to carry on."
                    .to_string(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opengrok::{OpenGrokError, Unreachable};

    #[test]
    fn a_fresh_session_holds_and_shows_nothing() {
        let session = Session::default();
        assert!(!session.is_expired());
        assert_eq!(session.banner(), None);
        assert!(session.may_send(true));
    }

    /// A turn's `401` puts the app in this state. A turn that never arrived does not.
    ///
    /// The two came back from the same button a second apart on the day this was written, and
    /// telling them apart is the whole job: one is fixed by waiting and one is fixed only by a
    /// person. The wire failure must leave this state exactly as it found it, or the app would
    /// ask somebody to sign in over a router that was rebooting.
    #[test]
    fn a_401_on_a_turn_signs_the_app_out_and_a_wire_failure_does_not() {
        let mut session = Session::default();
        let gone = OpenGrokError::signed_out(
            "this turn does not say whose spend it is, so it cannot be counted against \
             anybody's limits",
        );
        assert!(session.note(gone.failure()), "the first one is the news");
        assert!(session.is_expired());
        assert!(session.banner().is_some());

        let mut wire = Session::default();
        let never_arrived = OpenGrokError::from_server(Some(502), "Bad Gateway");
        assert_eq!(
            never_arrived.unreachable(),
            Some(Unreachable::Server),
            "the failure this test is about"
        );
        assert!(!wire.note(never_arrived.failure()));
        assert!(
            !wire.is_expired(),
            "a machine out of reach says nothing about who the app is"
        );
        assert_eq!(wire.banner(), None);
    }

    /// A refusal with a reason is a verdict, and stays one.
    ///
    /// This is the sentence that reached somebody as a red line while the real trouble was a
    /// missing credential. Read on its own it is a real refusal — the gateway heard the request
    /// and said no — and the fix for the missing credential must not swallow it: it belongs in
    /// the transcript, it does not sign anybody out, and nothing about it clears on its own.
    #[test]
    fn a_refusal_with_a_reason_stays_a_verdict() {
        let mut session = Session::default();
        let refused = OpenGrokError::from_server(
            Some(402),
            "the model gateway refused: 402 spend cap reached for this org",
        );
        assert!(!session.note(refused.failure()));
        assert!(!session.is_expired());
        assert_eq!(session.banner(), None);
        assert_eq!(
            refused.unreachable(),
            None,
            "and nothing about it is going to come back on its own"
        );
        assert!(
            session.may_send(true),
            "one turn was refused; the next one is still worth sending"
        );
    }

    #[test]
    fn signing_in_clears_it_and_nothing_else_does() {
        let mut session = Session::default();
        assert!(
            session.note(Failure::SignedOut),
            "the first 401 is the news"
        );
        assert!(!session.note(Failure::SignedOut), "and the second is not");
        assert!(session.is_expired());
        assert!(session.banner().is_some());

        assert!(session.signed_in(), "signing in is the news that clears it");
        assert!(!session.is_expired());
        assert_eq!(session.banner(), None);
        assert!(
            !session.signed_in(),
            "and there is nothing left to clear afterwards"
        );
    }

    /// The bug in one assertion: the app held no credential and sent the turn anyway.
    #[test]
    fn a_turn_is_not_attempted_without_something_to_send() {
        let session = Session::default();
        assert!(
            !session.may_send(false),
            "no credential means the answer is already known: do not spend the round trip"
        );

        let mut gone = Session::default();
        gone.note(Failure::SignedOut);
        assert!(
            !gone.may_send(true),
            "a token the server has already refused is not a reason to ask again"
        );

        assert!(
            gone.signed_in() && gone.may_send(true),
            "and signing in is what makes turns possible again"
        );
    }

    #[test]
    fn the_banner_says_what_to_do_rather_than_what_broke() {
        let mut session = Session::default();
        session.note(Failure::SignedOut);
        let (title, detail) = session.banner().expect("an expired session shows");
        assert!(title.contains("signed out"), "{title}");
        assert!(
            detail.contains("Sign in again"),
            "the only thing that clears it is the thing it asks for: {detail}"
        );
        assert!(
            detail.contains("nothing is being sent"),
            "a composer that looks ready and quietly refuses is the bug: {detail}"
        );
        assert!(
            !detail.to_ascii_lowercase().contains("reconnect"),
            "this one never reconnects on its own: {detail}"
        );
    }
}
