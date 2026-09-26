use std::fmt;

use super::pending::PendingCustom;

/// The machine a request could not reach.
///
/// The two are different machines with different fixes, and the app names which one because a
/// person reading "unreachable" cannot tell them apart. That cost real time once: the message
/// named a gateway URL when what needed restarting was a pair of database containers behind it,
/// and it could as easily have sent someone to restart the wrong thing in the other direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreachable {
    /// The OpenGrok server: the app's own request never got an answer at all.
    Server,
    /// OpenGrok answered, and what it said was that the model gateway behind it did not.
    Gateway,
}

impl Unreachable {
    /// The machine's own name, for a driver assert that would rather not match on copy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Gateway => "gateway",
        }
    }
}

/// What a failure *is*, as opposed to what it says.
///
/// Three kinds, and they are three because collapsing any two of them is how a missing
/// credential once reached a person as a sentence about spend limits. Each has a different
/// answer to "what makes this stop being true", and that is the only question a caller ever
/// wants answered:
///
/// | kind | the wire | the server | clears by |
/// |---|---|---|---|
/// | [`Failure::OutOfReach`] | down | said nothing | itself, when the wire returns |
/// | [`Failure::SignedOut`] | fine | answered `401` | the person signing in |
/// | [`Failure::Verdict`] | fine | answered with a reason | nothing; it is a decision |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Something answered the request and decided against it. It will decide the same way until
    /// something changes, so there is nothing to retry and nothing to wait for: this is the one
    /// kind that belongs in a transcript, because it is a fact about one request at one moment.
    Verdict,
    /// Nothing answered. The named machine is out of reach, nothing was decided about the
    /// request at all, and it stops being true without anybody doing anything.
    OutOfReach(Unreachable),
    /// The server answered, and what it answered was that it does not know who is asking.
    ///
    /// Neither of the other two. The wire is fine — a `401` is proof of it — and the server
    /// decided nothing about what was asked, because it never got as far as the question.
    /// Waiting will not fix it and neither will asking again; a person has to sign in.
    SignedOut,
}

/// HTTP failure against OpenGrok. Status is preserved so login 401 is distinguishable.
///
/// A failure is one of three different kinds of thing, and this is where they are told apart —
/// once, here, rather than at every call site that has to decide what to do about one. See
/// [`Failure`] for what the three are and why none of them may be folded into another.
#[derive(Debug)]
pub struct OpenGrokError {
    pub status: Option<u16>,
    pub message: String,
    /// What kind of failure this is. Private so the invariant holds — a failure is exactly one
    /// of the three, never two of them at once.
    failure: Failure,
    /// The `pending-user-message` CUSTOM a pending-route refusal carried.
    pending_event: Option<serde_json::Value>,
    /// A run route's `503` carried `historyMissed`: see [`Self::history_missed`].
    history_missed: bool,
}

impl OpenGrokError {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
            failure: Failure::Verdict,
            pending_event: None,
            history_missed: false,
        }
    }

    pub fn status(status: u16, message: impl Into<String>) -> Self {
        Self {
            status: Some(status),
            message: message.into(),
            failure: Failure::Verdict,
            pending_event: None,
            history_missed: false,
        }
    }

    /// The server does not know who is asking.
    ///
    /// Built where the route is known rather than read off the status, because `401` means two
    /// unrelated things depending on what was asked: on `/auth/login` it is a wrong password,
    /// which is a verdict about what the person typed, and on anything else it is the session
    /// being gone. Only the client, which knows the path, can tell those apart.
    pub fn signed_out(message: impl Into<String>) -> Self {
        Self {
            status: Some(401),
            message: message.into(),
            failure: Failure::SignedOut,
            pending_event: None,
            history_missed: false,
        }
    }

    /// A request that never came back with an answer.
    ///
    /// Connect, timeout, and a body that stopped arriving are all the same fact — the app is not
    /// reaching OpenGrok — and none of them is anything OpenGrok decided. Everything else reqwest
    /// reports (a status it was told to reject, a body that would not parse, a request this app
    /// built wrong) is a real answer or our own bug, and neither improves by waiting.
    pub fn transport(error: &reqwest::Error) -> Self {
        let out_of_reach =
            error.is_connect() || error.is_timeout() || error.is_request() || error.is_body();
        Self {
            status: None,
            message: error.to_string(),
            failure: if out_of_reach {
                Failure::OutOfReach(Unreachable::Server)
            } else {
                Failure::Verdict
            },
            pending_event: None,
            history_missed: false,
        }
    }

    /// Something the server said, read for whether it is saying the gateway is out of reach.
    pub fn from_server(status: Option<u16>, message: impl Into<String>) -> Self {
        let message = message.into();
        let failure = if reads_as_gateway_unreachable(&message) {
            Failure::OutOfReach(Unreachable::Gateway)
        } else if matches!(status, Some(502..=504)) {
            // Nothing this app talks to answers these itself: they come from whatever stands in
            // front of OpenGrok, saying it could not get to OpenGrok either. That is the server
            // being out of reach with an extra hop in the middle.
            Failure::OutOfReach(Unreachable::Server)
        } else {
            Failure::Verdict
        };
        Self {
            status,
            message,
            failure,
            pending_event: None,
            history_missed: false,
        }
    }

    pub fn is_unauthorized(&self) -> bool {
        self.status == Some(401)
    }

    /// What kind of failure this is, for the one place that sorts them.
    pub fn failure(&self) -> Failure {
        self.failure
    }

    /// Which machine was out of reach, when that is what happened.
    pub fn unreachable(&self) -> Option<Unreachable> {
        match self.failure {
            Failure::OutOfReach(what) => Some(what),
            _ => None,
        }
    }

    /// The server answered and did not know who was asking.
    ///
    /// Not the same question as [`Self::is_unauthorized`], which is only about the status line:
    /// a wrong password is a `401` too, and it is a verdict about what somebody typed rather
    /// than a session that has gone.
    pub fn is_signed_out(&self) -> bool {
        self.failure == Failure::SignedOut
    }

    /// Nothing at this path, or a thread this account does not own. Pending-user-message
    /// routes treat both as "keep the local queue": an OpenGrok that has not shipped the
    /// store yet answers 404 the same way an unknown thread does.
    pub fn is_not_found(&self) -> bool {
        self.status == Some(404)
    }

    /// `POST /ag-ui` (or a retry enqueue) for a follow-up that already became a run.
    pub fn is_already_consumed(&self) -> bool {
        self.status == Some(409) && self.message == "already-consumed"
    }

    /// `POST /ag-ui` named a pending id that was canceled (or never heard of).
    pub fn is_not_pending(&self) -> bool {
        self.status == Some(409) && self.message == "not-pending"
    }

    /// `POST /ag-ui` fired a queued send that no longer matches its row. The row is left
    /// queued, and [`Self::pending_custom`] is the row as it stands now.
    pub fn is_stale_pending(&self) -> bool {
        self.status == Some(409) && self.message == "stale-pending-message"
    }

    /// `POST /pending` lost the race the server describes as "another writer got there
    /// first; retry". The insert collided and the winning row was gone before it could be
    /// read, so the same POST is worth one more try. Any other 409 is a decision.
    pub fn is_enqueue_conflict(&self) -> bool {
        self.status == Some(409) && self.message == "another writer got there first; retry"
    }

    /// The `pending-user-message` CUSTOM a pending refusal carried.
    pub fn pending_custom(&self) -> Option<PendingCustom> {
        self.pending_event
            .as_ref()
            .and_then(PendingCustom::from_agui)
    }

    pub(super) fn with_pending_event(mut self, event: Option<serde_json::Value>) -> Self {
        self.pending_event = event;
        self
    }

    /// The run PLAYED, and the server could not write it into the recipe's history: a run
    /// route's `503` with `historyMissed` beside the receipt (opengrok-server #217). Not a
    /// refusal of the run — the clicks happened — and the message is the server's sentence
    /// saying so.
    pub fn history_missed(&self) -> bool {
        self.history_missed
    }

    pub(crate) fn with_history_missed(mut self) -> Self {
        self.history_missed = true;
        self
    }
}

/// Whether `POST /pending` should be sent again. The server asks for one retry of this
/// conflict. A second one is the same answer, and asking in a loop would not change it.
pub fn retry_enqueue(error: &OpenGrokError, attempt: u32) -> bool {
    error.is_enqueue_conflict() && attempt < 2
}

/// The server's own words for the gateway being out of reach.
///
/// The wire carries no code for this. A `RUN_ERROR` frame is a message and nothing else, and
/// `/models` answers `200` with an empty catalogue and a note — so the sentences the server
/// writes are the only signal there is. They are matched in this one function so that the day
/// the wire grows a code, there is a single place to change.
pub fn reads_as_gateway_unreachable(message: &str) -> bool {
    /// The server's wordings, from `opengrok-harness`'s `ModelError::Unreachable` and the
    /// `/models` catalogue note.
    const MARKS: [&str; 3] = [
        "the model gateway is unreachable",
        "the gateway is unreachable",
        "the gateway could not be reached",
    ];
    let text = message.to_ascii_lowercase();
    MARKS.iter().any(|mark| text.contains(mark))
}

impl fmt::Display for OpenGrokError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(code) => write!(f, "{code}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for OpenGrokError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_is_a_verdict_and_not_a_state() {
        let refused = OpenGrokError::status(401, "invalid email or password");
        assert!(refused.is_unauthorized());
        assert_eq!(refused.unreachable(), None);
        assert_eq!(refused.failure(), Failure::Verdict);

        let declined = OpenGrokError::from_server(Some(403), "this model is not on your plan");
        assert_eq!(declined.unreachable(), None);
        assert_eq!(declined.failure(), Failure::Verdict);
    }

    #[test]
    fn the_server_and_the_gateway_are_named_apart() {
        let gateway = OpenGrokError::from_server(
            None,
            "the model gateway is unreachable: error sending request for url \
             (http://127.0.0.1:29080/v1/chat/completions)",
        );
        assert_eq!(gateway.unreachable(), Some(Unreachable::Gateway));
        assert_eq!(
            gateway.unreachable().map(Unreachable::as_str),
            Some("gateway")
        );

        let in_front = OpenGrokError::from_server(Some(502), "Bad Gateway");
        assert_eq!(in_front.unreachable(), Some(Unreachable::Server));
    }

    #[test]
    fn the_models_note_reads_as_the_gateway_being_down() {
        assert!(reads_as_gateway_unreachable(
            "the gateway could not be reached: error sending request for url \
             (http://127.0.0.1:29080/v1/models)"
        ));
        // A gateway that answered and had nothing to offer is not a gateway out of reach: the
        // catalogue it sent is the truth, and waiting will not lengthen it.
        assert!(!reads_as_gateway_unreachable(
            "the gateway advertises no models on this key's route — a pin can still be typed by hand"
        ));
        assert!(!reads_as_gateway_unreachable(
            "the gateway answered 429 Too Many Requests when asked for its models"
        ));
    }

    /// The three kinds are three, and no two of them read alike.
    ///
    /// This is the bug the whole distinction exists for: a turn went out with no credential on
    /// it, the server had nobody to bill and held it, and the sentence it sent back landed in
    /// the transcript as a red line about spend limits. A session that is gone is not a verdict
    /// about the turn, and it is not the wire being down.
    #[test]
    fn a_session_that_is_gone_is_neither_a_verdict_nor_a_state_that_clears_itself() {
        let gone =
            OpenGrokError::signed_out("this turn does not say whose spend it is — sign in again");
        assert_eq!(gone.failure(), Failure::SignedOut);
        assert!(gone.is_signed_out());
        assert!(gone.is_unauthorized(), "it is still a 401 on the wire");
        assert_eq!(
            gone.unreachable(),
            None,
            "nothing is out of reach: the server answered"
        );
    }

    #[test]
    fn a_verdict_with_a_reason_stays_a_verdict() {
        // The exact sentence that reached a person as a red line while the real trouble was a
        // missing credential. Read on its own it *is* a verdict — a refusal with a reason — and
        // it has to stay one, or the fix for today's bug would swallow every real refusal too.
        let refused = OpenGrokError::from_server(
            Some(402),
            "the model gateway refused: 402 spend cap reached for this org",
        );
        assert_eq!(refused.failure(), Failure::Verdict);
        assert!(!refused.is_signed_out());
        assert_eq!(refused.unreachable(), None);
    }

    #[test]
    fn a_pending_conflict_is_a_verdict_about_that_id() {
        let consumed = OpenGrokError::from_server(Some(409), "already-consumed");
        assert!(consumed.is_already_consumed());
        assert!(!consumed.is_not_pending());
        assert_eq!(consumed.failure(), Failure::Verdict);

        let canceled = OpenGrokError::from_server(Some(409), "not-pending");
        assert!(canceled.is_not_pending());
        assert!(!canceled.is_already_consumed());

        let missing = OpenGrokError::from_server(Some(404), "no such thread");
        assert!(missing.is_not_found());
        assert!(!missing.is_already_consumed());

        let raced = OpenGrokError::from_server(Some(409), "another writer got there first; retry");
        assert!(raced.is_enqueue_conflict());
        assert!(!consumed.is_enqueue_conflict());
        assert!(crate::opengrok::retry_enqueue(&raced, 1));
        assert!(
            !crate::opengrok::retry_enqueue(&raced, 2),
            "one retry, then the conflict is a decision"
        );
        assert!(!crate::opengrok::retry_enqueue(&consumed, 1));
    }

    #[test]
    fn a_wrong_password_is_not_a_session_that_has_gone() {
        // Both are `401`. One is the person mistyping and belongs under the password field; the
        // other is the app holding nothing the server recognises. Status alone cannot tell them
        // apart, which is why the client builds the second one by route.
        let typed_wrong = OpenGrokError::from_server(Some(401), "invalid email or password");
        assert!(typed_wrong.is_unauthorized());
        assert!(!typed_wrong.is_signed_out());
        assert_eq!(typed_wrong.failure(), Failure::Verdict);
    }
}
