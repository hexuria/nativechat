use std::fmt;

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

/// HTTP failure against OpenGrok. Status is preserved so login 401 is distinguishable.
///
/// A failure is one of two different kinds of thing, and this is where the two are told apart —
/// once, here, rather than at every call site that has to decide what to do about one. A refusal
/// is a **verdict**: the server heard the request and decided against it, and it will decide the
/// same way until something changes. Not being able to reach the server, or the server not being
/// able to reach the gateway, is a **state**: nothing was decided, and it stops being true on its
/// own. Only the second kind is worth retrying and worth saying "reconnecting" about.
#[derive(Debug)]
pub struct OpenGrokError {
    pub status: Option<u16>,
    pub message: String,
    /// Which machine was out of reach, when that is what this failure was. `None` is a verdict:
    /// something answered, and this is what it said. Private so the invariant holds — a failure
    /// is never both a state and a verdict.
    unreachable: Option<Unreachable>,
}

impl OpenGrokError {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
            unreachable: None,
        }
    }

    pub fn status(status: u16, message: impl Into<String>) -> Self {
        Self {
            status: Some(status),
            message: message.into(),
            unreachable: None,
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
            unreachable: out_of_reach.then_some(Unreachable::Server),
        }
    }

    /// Something the server said, read for whether it is saying the gateway is out of reach.
    pub fn from_server(status: Option<u16>, message: impl Into<String>) -> Self {
        let message = message.into();
        let unreachable = if reads_as_gateway_unreachable(&message) {
            Some(Unreachable::Gateway)
        } else if matches!(status, Some(502..=504)) {
            // Nothing this app talks to answers these itself: they come from whatever stands in
            // front of OpenGrok, saying it could not get to OpenGrok either. That is the server
            // being out of reach with an extra hop in the middle.
            Some(Unreachable::Server)
        } else {
            None
        };
        Self {
            status,
            message,
            unreachable,
        }
    }

    pub fn is_unauthorized(&self) -> bool {
        self.status == Some(401)
    }

    /// Which machine was out of reach, when that is what happened.
    pub fn unreachable(&self) -> Option<Unreachable> {
        self.unreachable
    }
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

        let declined = OpenGrokError::from_server(Some(403), "this model is not on your plan");
        assert_eq!(declined.unreachable(), None);
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
}
