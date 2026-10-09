//! Which faults are worth reporting at all: the first gate, rules on this Mac, instant and free.
//!
//! Most faults are not bugs. A Wi-Fi drop fails every read at once, a session that ran out
//! answers 401, a refusal the server wrote is an answer; reporting those files the person's own
//! afternoon as the project's issue. So a fault is weighed before a report is offered: what this
//! Mac already knows decides the plain cases, and only what is left is worth a report (and, once
//! the server's triage route exists, worth asking the person's agent about).

use crate::faults::Place;
use crate::notifications::Notice;

/// How close in time another failure has to be for the two to be one outage: the reads a Bot's
/// settings make together all fail within a second or two of each other.
const SAME_MOMENT_MS: i64 = 10_000;

/// How many times a fault that fixed itself has to come back before it is worth a report: once
/// is a blip, three times is a pattern.
const BLIP_TIMES: u32 = 3;

/// What the first gate makes of a fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate1 {
    /// Not the project's to fix, and nothing for the person to do: why, in their words.
    Noise(&'static str),
    /// The person's to fix: what to do, in their words.
    YourSide(&'static str),
    /// Possibly a bug: worth a report.
    Ask,
}

impl Gate1 {
    /// Whether a report is offered without the person insisting.
    pub fn worth_reporting(self) -> bool {
        matches!(self, Gate1::Ask)
    }
}

pub const OFFLINE: &str = "You were offline: every request failed at the same moment.";
pub const SERVER_DOWN: &str =
    "Your OpenGrok server was not answering. Check that it is running and reachable.";
pub const SIGNED_OUT: &str = "Your session ended. Sign in again.";
pub const SLOW_DOWN: &str = "The server asked to slow down. Wait a moment and try again.";
pub const AN_ANSWER: &str = "The server answered this on purpose: it is a refusal, not a bug.";
pub const FIXED_ITSELF: &str =
    "It fixed itself on the next try. Only one that keeps coming back is worth a report.";

/// The first gate for `notice`, given every notice this Mac keeps (`all`). A notice that is not
/// a fault is never worth a report.
pub fn gate1(notice: &Notice, all: &[Notice]) -> Gate1 {
    let Some(fault) = notice.fault.as_ref() else {
        return Gate1::Noise(AN_ANSWER);
    };
    if fault.place == Place::Server {
        return Gate1::YourSide(SERVER_DOWN);
    }
    match fault.status {
        Some(401) => return Gate1::YourSide(SIGNED_OUT),
        Some(429) => return Gate1::YourSide(SLOW_DOWN),
        Some(400..=499) => return Gate1::Noise(AN_ANSWER),
        _ => {}
    }
    if fault.status.is_none() && offline_then(notice, all) {
        return Gate1::Noise(OFFLINE);
    }
    if fault.resolved && fault.count < BLIP_TIMES {
        return Gate1::Noise(FIXED_ITSELF);
    }
    Gate1::Ask
}

/// Whether the server was out of reach when `notice` failed: a Server fault (the reconnect
/// banner's) spans its moment, or another place's read failed without an answer at the same
/// moment, which is what a lost connection looks like from inside.
fn offline_then(notice: &Notice, all: &[Notice]) -> bool {
    let at = notice.at_ms;
    let place = notice.fault.as_ref().map(|fault| fault.place);
    all.iter()
        .filter(|other| other.id != notice.id)
        .filter_map(|other| other.fault.as_ref().map(|fault| (other.at_ms, fault)))
        .any(|(other_at, fault)| {
            let server_down = fault.place == Place::Server
                && other_at - SAME_MOMENT_MS <= at
                && at <= fault.last_ms + SAME_MOMENT_MS;
            let another_place_failed = Some(fault.place) != place
                && fault.status.is_none()
                && (other_at - at).abs() <= SAME_MOMENT_MS;
            server_down || another_place_failed
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::FaultFacts;

    fn fault(id: &str, place: Place, status: Option<u16>, at_ms: i64) -> Notice {
        let mut notice = Notice::new(
            None,
            place.label(),
            "failed",
            std::panic::Location::caller(),
        );
        notice.id = id.into();
        notice.at_ms = at_ms;
        notice.fault = Some(FaultFacts {
            place,
            endpoint: Some("GET /coworkers/{id}/usage".into()),
            status,
            count: 1,
            last_ms: at_ms,
            resolved: false,
        });
        notice
    }

    /// The plain cases: the server's own outage, a sign-out, a rate limit and a refusal are not
    /// the project's bugs, and a server error on its own is worth a report.
    #[test]
    fn plain_cases_are_decided_without_a_report() {
        let t = 1_000_000;
        let cases = [
            (
                fault("a", Place::Server, None, t),
                Gate1::YourSide(SERVER_DOWN),
            ),
            (
                fault("b", Place::Usage, Some(401), t),
                Gate1::YourSide(SIGNED_OUT),
            ),
            (
                fault("c", Place::Usage, Some(429), t),
                Gate1::YourSide(SLOW_DOWN),
            ),
            (
                fault("d", Place::Usage, Some(404), t),
                Gate1::Noise(AN_ANSWER),
            ),
            (fault("e", Place::Usage, Some(502), t), Gate1::Ask),
            (fault("f", Place::Usage, None, t), Gate1::Ask),
        ];
        for (notice, want) in cases {
            assert_eq!(
                gate1(&notice, std::slice::from_ref(&notice)),
                want,
                "{}",
                notice.id
            );
        }
    }

    /// A read that failed without an answer while the server was out of reach, or while another
    /// place failed the same way at the same moment, is the connection, not a bug; the same
    /// failure an hour away from any other is worth a report.
    #[test]
    fn a_lost_connection_is_not_a_bug() {
        let t = 5_000_000;
        let usage = fault("usage", Place::Usage, None, t);
        let mut server = fault("server", Place::Server, None, t - 3_000);
        if let Some(f) = server.fault.as_mut() {
            f.last_ms = t + 20_000;
        }
        assert_eq!(
            gate1(&usage, &[usage.clone(), server]),
            Gate1::Noise(OFFLINE)
        );

        let tools = fault("tools", Place::Tools, None, t + 800);
        assert_eq!(
            gate1(&usage, &[usage.clone(), tools]),
            Gate1::Noise(OFFLINE)
        );

        let long_ago = fault("tools", Place::Tools, None, t - 3_600_000);
        assert_eq!(gate1(&usage, &[usage.clone(), long_ago]), Gate1::Ask);

        let answered = fault("models", Place::Models, Some(500), t + 500);
        assert_eq!(
            gate1(&usage, &[usage.clone(), answered]),
            Gate1::Ask,
            "a server that answered was reachable"
        );
    }

    /// A fault that fixed itself is a blip until it keeps coming back.
    #[test]
    fn a_blip_is_not_worth_a_report_until_it_repeats() {
        let mut blip = fault("blip", Place::Usage, Some(502), 7_000_000);
        if let Some(f) = blip.fault.as_mut() {
            f.resolved = true;
        }
        assert_eq!(
            gate1(&blip, std::slice::from_ref(&blip)),
            Gate1::Noise(FIXED_ITSELF)
        );
        if let Some(f) = blip.fault.as_mut() {
            f.count = BLIP_TIMES;
        }
        assert_eq!(gate1(&blip, std::slice::from_ref(&blip)), Gate1::Ask);
    }
}
