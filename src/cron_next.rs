//! When a routine's line next runs, worked out by the parser opengrok-server runs it with.
//!
//! The server puts a seconds field of `0` in front of a five-field line, names its numbered days
//! of the week the standard way, hands it to the `cron` crate and asks for the next time after
//! now, read in the routine's own IANA zone (opengrok-core `schedule.rs`, `normalized_cron` and
//! `next_fire_ms`; the days named since #331, on main 5567f91, and the zone since #316, UTC
//! before it). This does the same, with the same crate at the same version, so what the wake
//! editor says will run next is what the server will run, and a line it refuses is one the server
//! would refuse, in the parser's own words, or in the server's for a day of the week.
//!
//! Kept apart from `cron_spec`, which stays standalone with no crate to lean on.

use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};

/// The line's next run after `after`, the line read in `zone`, the routine's: `0 9 * * *` in
/// UTC+8 is 01:00 UTC. `Ok(None)` for a line that never runs again, such as the 31st of
/// February. `Err` is why the server's parser refuses the line.
pub fn next_run<Tz: TimeZone>(
    line: &str,
    after: DateTime<Utc>,
    zone: &Tz,
) -> Result<Option<DateTime<Utc>>, String> {
    let normalized = normalized_cron(line)?;
    let schedule = cron::Schedule::from_str(&normalized).map_err(|error| refusal(&error))?;
    Ok(schedule
        .after(&after.with_timezone(zone))
        .next()
        .map(|when| when.with_timezone(&Utc)))
}

/// A line as the server hands it to the crate (`normalized_cron` in opengrok-core
/// `schedule.rs`): five fields get a seconds field of `0` in front and their numbered days of the
/// week named, standard cron's way ([`crate::cron_spec::standard_days_named`], #331); six or
/// seven pass as they are, their days the crate's. `Err` is the server's sentence for a day of the
/// week it refuses.
fn normalized_cron(line: &str) -> Result<String, String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let [minute, hour, day_of_month, month, days] = fields[..] else {
        return Ok(line.trim().to_string());
    };
    let days = crate::cron_spec::standard_days_named(days)?;
    Ok(format!("0 {minute} {hour} {day_of_month} {month} {days}"))
}

/// The parser's reason, without the line it echoes back and the caret it points with.
fn refusal(error: &cron::error::Error) -> String {
    let text = error.to_string();
    text.lines()
        .map(str::trim)
        .rev()
        .find(|line| !line.is_empty() && !line.starts_with('^') && !line.starts_with("0 "))
        .map(str::to_string)
        .unwrap_or_else(|| "The server's cron parser cannot read this line.".to_string())
}

/// "Next run: Thu 8 Oct at 5:00 PM your time": when the next run lands where the person is, which
/// need not be the zone the line is read in (a routine's own, which may be another's or UTC).
pub fn next_run_words<Tz: TimeZone>(when: DateTime<Utc>, zone: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let local = when.with_timezone(zone);
    format!(
        "Next run: {} at {} your time",
        local.format("%a %-d %b"),
        local.format("%-I:%M %p")
    )
}

#[cfg(test)]
mod tests {
    use super::{next_run, next_run_words};
    use chrono::{FixedOffset, TimeZone, Utc};

    /// The next run is the server's: Monday to Friday by name, and a numbered day of the week
    /// counted as standard cron counts it, as the server counts it since #331 (on main 5567f91):
    /// `1-5` is Monday to Friday, and 0 and 7 are Sunday. Counted the `cron` crate's way, as it
    /// was before, `1-5` skipped Friday and ran Sunday. Here the routine's zone is UTC.
    #[test]
    fn the_next_run_is_the_one_the_server_would_run() {
        // A Wednesday, at noon UTC.
        let noon = Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap();
        let next = |line: &str| next_run(line, noon, &Utc).unwrap().unwrap();
        assert_eq!(
            next("0 9 * * MON-FRI"),
            Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
        );
        assert_eq!(
            next("*/15 * * * *"),
            Utc.with_ymd_and_hms(2026, 10, 7, 12, 15, 0).unwrap()
        );
        // `1-5` runs Thursday, then Friday, as `MON-FRI` does.
        let thursday = next("0 9 * * 1-5");
        assert_eq!(
            thursday,
            Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
        );
        let after_thursday = next_run("0 9 * * 1-5", thursday, &Utc).unwrap().unwrap();
        assert_eq!(
            after_thursday,
            Utc.with_ymd_and_hms(2026, 10, 9, 9, 0, 0).unwrap(),
            "Friday, not Sunday"
        );
        for sunday in ["0 9 * * 0", "0 9 * * 7", "0 9 * * SUN"] {
            assert_eq!(
                next(sunday),
                Utc.with_ymd_and_hms(2026, 10, 11, 9, 0, 0).unwrap(),
                "{sunday}"
            );
        }
        assert_eq!(
            next("0 9 * * 5-7"),
            Utc.with_ymd_and_hms(2026, 10, 9, 9, 0, 0).unwrap(),
            "Friday to Sunday, from Friday"
        );
        assert_eq!(next_run("0 9 31 2 *", noon, &Utc), Ok(None), "never");
    }

    /// The line is read in the routine's own zone, as the server reads it (#316): nine in the
    /// morning in UTC+8 is 01:00 UTC, the next day's when that has passed there.
    #[test]
    fn the_line_is_read_in_the_routines_zone() {
        let manila = FixedOffset::east_opt(8 * 3600).unwrap();
        // A Wednesday, at noon UTC: 8 PM in UTC+8.
        let noon = Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap();
        assert_eq!(
            next_run("0 9 * * *", noon, &manila),
            Ok(Some(Utc.with_ymd_and_hms(2026, 10, 8, 1, 0, 0).unwrap())),
            "Thursday nine there"
        );
        // A Wednesday, at midnight UTC: still Wednesday's eight in the morning there.
        let early = Utc.with_ymd_and_hms(2026, 10, 7, 0, 0, 0).unwrap();
        assert_eq!(
            next_run("0 9 * * *", early, &manila),
            Ok(Some(Utc.with_ymd_and_hms(2026, 10, 7, 1, 0, 0).unwrap())),
            "Wednesday nine there"
        );
        assert_eq!(
            next_run("0 9 * * MON-FRI", early, &Utc),
            Ok(Some(Utc.with_ymd_and_hms(2026, 10, 7, 9, 0, 0).unwrap())),
            "and nine in UTC is nine UTC"
        );
    }

    /// A line the server's parser refuses is refused here in its words, without the line it
    /// echoes and the caret it points with; a day of the week the server refuses before the
    /// parser sees it, in the server's words (#331).
    #[test]
    fn a_refused_line_says_why_in_the_parsers_words() {
        let noon = Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap();
        let why = next_run("0 9 * * 8", noon, &Utc).unwrap_err();
        assert_eq!(why, format!("8 {}", crate::cron_spec::NOT_A_DAY));
        let why = next_run("0 9 * * 1-5/0", noon, &Utc).unwrap_err();
        assert!(why.starts_with("1-5/0 is not a day of the week"), "{why}");
        let why = next_run("0 0 9 * * 0", noon, &Utc).unwrap_err();
        assert!(
            why.contains("greater than or equal to 1"),
            "six fields are the crate's, which refuses day 0: {why}"
        );
        assert!(!why.contains('^'), "{why}");
        let why = next_run("0 9 L * *", noon, &Utc).unwrap_err();
        assert!(why.contains("'L'"), "{why}");
        assert!(next_run("@every 1h", noon, &Utc).is_err());
    }

    /// The next run is said where the person is: 9:00 UTC is 5:00 PM in UTC+8.
    #[test]
    fn the_next_run_is_said_in_the_persons_time() {
        let when = Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap();
        let manila = FixedOffset::east_opt(8 * 3600).unwrap();
        assert_eq!(
            next_run_words(when, &manila),
            "Next run: Thu 8 Oct at 5:00 PM your time"
        );
    }
}
