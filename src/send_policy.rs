//! What a composer send does while the coworker is busy.
//!
//! Three states of the thread, one answer each: idle posts; parked (a card
//! is waiting on the person) steers, because the server settles the card on
//! the next message and the person's words are that message; running queues
//! unless the person asked to interrupt — with ⌘⇧↩ or the `on_send`
//! preference. Pure on purpose: no gpui, no state, so it compiles and tests
//! on its own while the crate's test target is out of reach.

/// What a plain send does while a turn is running. Persisted in prefs.json.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OnSend {
    /// Hold the message and send it when the turn ends.
    #[default]
    Queue,
    /// Stop the turn at its next step and send now.
    Steer,
}

impl OnSend {
    /// Unknown words fall back to `Queue`, so a preference written by a later
    /// build (say, `auto`) never turns into an interrupt on this one.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "steer" | "interrupt" => Self::Steer,
            _ => Self::Queue,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Steer => "steer",
        }
    }
}

/// The thread as the composer finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Busy {
    Idle,
    /// A run is in flight and nothing is waiting on the person.
    Running,
    /// A user-form or approval card is waiting on the person, or
    /// the chrome still says so.
    Parked,
}

/// What to do with the message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendPlan {
    Post,
    /// Settle what is parked (or stop what is running), then post.
    Steer,
    /// Keep it until the thread is idle.
    Queue,
}

pub fn plan_send(busy: Busy, on_send: OnSend, force_steer: bool) -> SendPlan {
    match busy {
        Busy::Idle => SendPlan::Post,
        // Parked is always a steer: the server ends the parked run on the
        // next message whatever the preference says, and holding the words
        // back would leave the card open for nothing.
        Busy::Parked => SendPlan::Steer,
        Busy::Running if force_steer || on_send == OnSend::Steer => SendPlan::Steer,
        Busy::Running => SendPlan::Queue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_posts_whatever_the_preference_or_chord() {
        for on_send in [OnSend::Queue, OnSend::Steer] {
            for force in [false, true] {
                assert_eq!(plan_send(Busy::Idle, on_send, force), SendPlan::Post);
            }
        }
    }

    #[test]
    fn parked_always_steers() {
        for on_send in [OnSend::Queue, OnSend::Steer] {
            for force in [false, true] {
                assert_eq!(plan_send(Busy::Parked, on_send, force), SendPlan::Steer);
            }
        }
    }

    #[test]
    fn running_queues_by_default_and_steers_on_request() {
        assert_eq!(
            plan_send(Busy::Running, OnSend::Queue, false),
            SendPlan::Queue
        );
        assert_eq!(
            plan_send(Busy::Running, OnSend::Queue, true),
            SendPlan::Steer
        );
        assert_eq!(
            plan_send(Busy::Running, OnSend::Steer, false),
            SendPlan::Steer
        );
        assert_eq!(
            plan_send(Busy::Running, OnSend::Steer, true),
            SendPlan::Steer
        );
    }

    #[test]
    fn preference_words_round_trip_and_unknown_means_queue() {
        assert_eq!(OnSend::parse("steer"), OnSend::Steer);
        assert_eq!(OnSend::parse(" Interrupt "), OnSend::Steer);
        assert_eq!(OnSend::parse("queue"), OnSend::Queue);
        assert_eq!(OnSend::parse("auto"), OnSend::Queue);
        assert_eq!(OnSend::parse(""), OnSend::Queue);
        for on_send in [OnSend::Queue, OnSend::Steer] {
            assert_eq!(OnSend::parse(on_send.as_str()), on_send);
        }
        assert_eq!(OnSend::default(), OnSend::Queue);
    }
}
