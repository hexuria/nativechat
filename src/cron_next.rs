//! When a routine's line next runs, worked out by the parser opengrok-server runs it with.
//!
//! The server puts a seconds field of `0` in front of a five-field line, hands it to the `cron`
//! crate and asks for the next time after now, in UTC (opengrok-core `schedule.rs`,
//! `normalized_cron` and `next_fire_ms`). This does the same, with the same crate at the same
//! version, so what the wake editor says will run next is what the server will run, and a line
//! it refuses is one the server would refuse, in the parser's own words.
//!
//! Kept apart from `cron_spec`, which stays standalone with no crate to lean on.

use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};

/// The line's next run after `after`; `Ok(None)` for a line that never runs again, such as the
/// 31st of February. `Err` is why the server's parser refuses the line.
pub fn next_run(line: &str, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, String> {
    let line = line.trim();
    let normalized = if line.split_whitespace().count() == 5 {
        format!("0 {line}")
    } else {
        line.to_string()
    };
    let schedule = cron::Schedule::from_str(&normalized).map_err(|error| refusal(&error))?;
    Ok(schedule.after(&after).next())
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
/// is not the clock the line is written in (the server's is UTC).
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

    /// The next run is the server's: in UTC, Monday to Friday by name, and a numbered day of the
    /// week counted from Sunday as 1, so `1-5` skips Friday.
    #[test]
    fn the_next_run_is_the_one_the_server_would_run() {
        // A Wednesday, at noon UTC.
        let noon = Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap();
        let next = |line: &str| next_run(line, noon).unwrap().unwrap();
        assert_eq!(
            next("0 9 * * MON-FRI"),
            Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
        );
        assert_eq!(
            next("*/15 * * * *"),
            Utc.with_ymd_and_hms(2026, 10, 7, 12, 15, 0).unwrap()
        );
        // Friday is 6 there: `1-5` runs Thursday, then Sunday.
        let thursday = next("0 9 * * 1-5");
        let after_thursday = next_run("0 9 * * 1-5", thursday).unwrap().unwrap();
        assert_eq!(
            after_thursday,
            Utc.with_ymd_and_hms(2026, 10, 11, 9, 0, 0).unwrap(),
            "Sunday, not Friday"
        );
        assert_eq!(next_run("0 9 31 2 *", noon), Ok(None), "never");
    }

    /// A line the server's parser refuses is refused here in its words, without the line it
    /// echoes and the caret it points with.
    #[test]
    fn a_refused_line_says_why_in_the_parsers_words() {
        let noon = Utc.with_ymd_and_hms(2026, 10, 7, 12, 0, 0).unwrap();
        let why = next_run("0 9 * * 0", noon).unwrap_err();
        assert!(why.contains("greater than or equal to 1"), "{why}");
        assert!(!why.contains('^'), "{why}");
        let why = next_run("0 9 L * *", noon).unwrap_err();
        assert!(why.contains("'L'"), "{why}");
        assert!(next_run("@every 1h", noon).is_err());
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
