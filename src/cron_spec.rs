//! What the routine editor's schedule is, and the one cron line it comes down to.
//!
//! The picker in the editor was written for a person: "Every hour", "Weekdays", a time of day,
//! a list of dates. The server takes one cron line. This module is where the two meet, and it
//! is deliberately a plain module with no GPUI in it — the interesting part is arithmetic about
//! when a thing fires, and arithmetic is worth being able to test without a window.
//!
//! Standalone, too: `rustc --edition 2024 --test src/cron_spec.rs` builds and runs the tests at
//! the bottom without the rest of the app. Nothing here may reach for `crate::`.
//!
//! The translation is not total in either direction, and that is the point:
//!
//! * [`ScheduleSpec::to_cron`] says no to combinations that are not one line — two times of day
//!   with different minutes past the hour, a weekly schedule with no day picked — with a
//!   sentence to show the person instead of a schedule that would never fire.
//! * [`ScheduleSpec::from_cron`] reads back the shapes the editor can draw and leaves everything
//!   else as `Custom`, where the line is shown as written rather than approximated.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleUiMode {
    Interval,
    Custom,
    Advanced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleUnit {
    Minutes,
    Hours,
    Days,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleDayKind {
    EveryDay,
    Weekdays,
    DaysOfMonth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleSpec {
    pub mode: ScheduleUiMode,
    pub every: u32,
    pub unit: ScheduleUnit,
    pub expr: String,
    pub months: Vec<u8>,
    pub day_kind: ScheduleDayKind,
    pub weekdays: Vec<u8>,
    pub month_days: Vec<u8>,
    pub times: Vec<(u8, u8)>,
}

impl ScheduleSpec {
    pub fn interval(every: u32, unit: ScheduleUnit) -> Self {
        Self {
            mode: ScheduleUiMode::Interval,
            every,
            unit,
            expr: String::new(),
            months: Vec::new(),
            day_kind: ScheduleDayKind::EveryDay,
            weekdays: Vec::new(),
            month_days: Vec::new(),
            times: vec![(9, 0)],
        }
    }

    pub fn custom(expr: &str) -> Self {
        let mut spec = Self::interval(1, ScheduleUnit::Hours);
        spec.mode = ScheduleUiMode::Custom;
        spec.expr = expr.to_string();
        spec
    }

    pub fn advanced_daily(hour: u8, minute: u8) -> Self {
        let mut spec = Self::interval(1, ScheduleUnit::Days);
        spec.mode = ScheduleUiMode::Advanced;
        spec.day_kind = ScheduleDayKind::EveryDay;
        spec.times = vec![(hour, minute)];
        spec
    }

    pub fn from_preset(name: &str) -> Self {
        match name {
            "Every hour" => Self::interval(1, ScheduleUnit::Hours),
            "Every day" => Self::advanced_daily(9, 0),
            "Weekdays" => {
                let mut spec = Self::advanced_daily(9, 0);
                spec.day_kind = ScheduleDayKind::Weekdays;
                spec.weekdays = vec![1, 2, 3, 4, 5];
                spec
            }
            "Every week" => {
                let mut spec = Self::advanced_daily(9, 0);
                spec.day_kind = ScheduleDayKind::Weekdays;
                spec.weekdays = vec![1];
                spec
            }
            "Every month" => {
                let mut spec = Self::advanced_daily(8, 0);
                spec.day_kind = ScheduleDayKind::DaysOfMonth;
                spec.month_days = vec![1];
                spec
            }
            "Interval" => Self::interval(30, ScheduleUnit::Minutes),
            "Advanced..." => Self::advanced_daily(9, 0),
            _ => Self::interval(30, ScheduleUnit::Minutes),
        }
    }

    pub fn label(&self) -> String {
        match self.mode {
            ScheduleUiMode::Interval => match (self.every, self.unit) {
                (1, ScheduleUnit::Minutes) => "Every minute".into(),
                (n, ScheduleUnit::Minutes) => format!("Every {n} minutes"),
                (1, ScheduleUnit::Hours) => "Every hour".into(),
                (n, ScheduleUnit::Hours) => format!("Every {n} hours"),
                (1, ScheduleUnit::Days) => "Every day".into(),
                (n, ScheduleUnit::Days) => format!("Every {n} days"),
            },
            ScheduleUiMode::Custom => {
                if self.expr.trim().is_empty() {
                    "Custom schedule".into()
                } else {
                    self.expr.clone()
                }
            }
            ScheduleUiMode::Advanced => advanced_label(self),
        }
    }
}

fn format_clock(hour: u8, minute: u8) -> String {
    let (h12, am) = if hour == 0 {
        (12, true)
    } else if hour < 12 {
        (hour, true)
    } else if hour == 12 {
        (12, false)
    } else {
        (hour - 12, false)
    };
    format!("{}:{:02} {}", h12, minute, if am { "AM" } else { "PM" })
}

fn ordinal(n: u8) -> String {
    let suffix = if matches!(n % 100, 11 | 12 | 13) {
        "th"
    } else {
        match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{n}{suffix}")
}

fn advanced_label(spec: &ScheduleSpec) -> String {
    let time = spec
        .times
        .first()
        .map(|(h, m)| format_clock(*h, *m))
        .unwrap_or_else(|| "9:00 AM".into());
    match spec.day_kind {
        ScheduleDayKind::EveryDay => format!("Every day at {time}"),
        ScheduleDayKind::Weekdays if spec.weekdays == [1, 2, 3, 4, 5] => {
            format!("Weekdays at {time}")
        }
        ScheduleDayKind::Weekdays if spec.weekdays.len() == 1 => {
            format!("Every week at {time}")
        }
        ScheduleDayKind::DaysOfMonth if spec.month_days == [1] => {
            format!("Monthly on the 1st at {time}")
        }
        ScheduleDayKind::DaysOfMonth => {
            let days = spec
                .month_days
                .iter()
                .map(|d| ordinal(*d))
                .collect::<Vec<_>>()
                .join(", ");
            format!("Monthly on the {days} at {time}")
        }
        _ => format!("Scheduled at {time}"),
    }
}

/// Why a schedule the editor can draw is not a cron line the server can take.
///
/// It carries the sentence and nothing else: there is no code to switch on here, because the
/// only thing to do with one of these is show it to the person who built the schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleNotCron {
    sentence: String,
}

impl ScheduleNotCron {
    fn new(sentence: impl Into<String>) -> Self {
        Self {
            sentence: sentence.into(),
        }
    }

    pub fn sentence(&self) -> &str {
        &self.sentence
    }
}

impl std::fmt::Display for ScheduleNotCron {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.sentence)
    }
}

impl ScheduleSpec {
    /// The one line `POST /schedules` takes, or the sentence saying why there isn't one.
    pub fn to_cron(&self) -> Result<String, ScheduleNotCron> {
        match self.mode {
            ScheduleUiMode::Interval => interval_cron(self.every, self.unit),
            ScheduleUiMode::Custom => {
                let line = self.expr.trim();
                if line.is_empty() {
                    Err(ScheduleNotCron::new(
                        "A custom schedule with nothing written in it is not a schedule: type a \
                         cron line, like `0 9 * * 1-5`.",
                    ))
                } else {
                    // Whatever was typed, as typed. `@every 1h` is a line this server takes and
                    // no cron parser here would recognise, and guessing at it would be the app
                    // overruling somebody who knows what they meant. The server is the judge,
                    // and its refusal is a sentence the app already knows how to show.
                    Ok(line.to_string())
                }
            }
            ScheduleUiMode::Advanced => advanced_cron(self),
        }
    }

    /// The editor's schedule for a line the server sent back.
    ///
    /// Only the shapes the pickers can draw are read back; anything else is `Custom`, which
    /// shows the line as written. That is the honest reading — an approximation would let
    /// somebody save the editor's idea of their line over their own.
    pub fn from_cron(line: &str) -> Self {
        parse_cron(line).unwrap_or_else(|| Self::custom(line.trim()))
    }
}

/// `Every N minutes/hours/days` as a cron line.
///
/// Cron steps within one field, so an interval only survives the trip while it fits inside the
/// field above it: 90 minutes is not `*/90` in a field that counts to 59, it is nothing at all.
fn interval_cron(every: u32, unit: ScheduleUnit) -> Result<String, ScheduleNotCron> {
    if every == 0 {
        return Err(ScheduleNotCron::new(
            "An interval of zero never comes round: ask for at least one minute, hour or day.",
        ));
    }
    let (ceiling, noun, line) = match unit {
        ScheduleUnit::Minutes => (59, "minutes", format!("*/{every} * * * *")),
        ScheduleUnit::Hours => (23, "hours", format!("0 */{every} * * *")),
        ScheduleUnit::Days => (31, "days", format!("0 0 */{every} * *")),
    };
    if every > ceiling {
        return Err(ScheduleNotCron::new(format!(
            "Every {every} {noun} is longer than a cron line can step; the most is {ceiling}. \
             Choose a larger unit, or write the line under Custom."
        )));
    }
    if every == 1 {
        return Ok(match unit {
            ScheduleUnit::Minutes => "* * * * *".to_string(),
            ScheduleUnit::Hours => "0 * * * *".to_string(),
            ScheduleUnit::Days => "0 0 * * *".to_string(),
        });
    }
    Ok(line)
}

/// A time of day, the days it lands on, and the months it is allowed in.
fn advanced_cron(spec: &ScheduleSpec) -> Result<String, ScheduleNotCron> {
    let Some((minute, hours)) = one_minute_and_its_hours(&spec.times) else {
        return Err(times_not_one_line(&spec.times));
    };
    let (day_of_month, day_of_week) = match spec.day_kind {
        ScheduleDayKind::EveryDay => ("*".to_string(), "*".to_string()),
        ScheduleDayKind::Weekdays => {
            if spec.weekdays.is_empty() {
                return Err(ScheduleNotCron::new(
                    "A schedule with no day of the week picked never runs: choose at least one.",
                ));
            }
            ("*".to_string(), number_list(&spec.weekdays))
        }
        ScheduleDayKind::DaysOfMonth => {
            if spec.month_days.is_empty() {
                return Err(ScheduleNotCron::new(
                    "A schedule with no date picked never runs: choose at least one day of the \
                     month.",
                ));
            }
            (number_list(&spec.month_days), "*".to_string())
        }
    };
    let months = if spec.months.is_empty() {
        "*".to_string()
    } else {
        number_list(&spec.months)
    };
    Ok(format!(
        "{minute} {} {day_of_month} {months} {day_of_week}",
        number_list(&hours)
    ))
}

/// The times, when they are one minute past the hour and a set of hours; `None` when they are
/// not.
///
/// Cron has one minute field for the whole line, so 9:00 and 5:00 are `0 9,17 * * *` and 9:00
/// and 5:30 are two lines. The sorted hours are the ones that go in the line.
fn one_minute_and_its_hours(times: &[(u8, u8)]) -> Option<(u8, Vec<u8>)> {
    let (_, minute) = *times.first()?;
    if times.iter().any(|(_, m)| *m != minute) {
        return None;
    }
    let mut hours: Vec<u8> = times.iter().map(|(h, _)| *h).collect();
    hours.sort_unstable();
    hours.dedup();
    Some((minute, hours))
}

/// The sentence for times that are not one line: empty, or minutes that disagree.
fn times_not_one_line(times: &[(u8, u8)]) -> ScheduleNotCron {
    if times.is_empty() {
        return ScheduleNotCron::new(
            "A schedule with no time of day has nothing to fire at: pick one.",
        );
    }
    let clocks: Vec<String> = times
        .iter()
        .map(|(hour, minute)| format_clock(*hour, *minute))
        .collect();
    ScheduleNotCron::new(format!(
        "{} are two schedules and not one, because they fire at different minutes past the \
         hour. Make a routine for each.",
        clocks.join(" and ")
    ))
}

fn number_list(numbers: &[u8]) -> String {
    let mut sorted = numbers.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// One field of a cron line, in the three shapes this module can read.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CronField {
    /// `*`: every one of them.
    Any,
    /// `*/n`: every nth.
    Every(u32),
    /// `3` or `1,2,3`: these ones.
    List(Vec<u8>),
}

/// One field read, or `None` for a shape this module has no picker for — a range (`1-5`), a
/// name (`MON`), a stepped list. Those lines stay `Custom` and are shown as written.
fn read_field(raw: &str, ceiling: u8) -> Option<CronField> {
    if raw == "*" {
        return Some(CronField::Any);
    }
    if let Some(step) = raw.strip_prefix("*/") {
        let step: u32 = step.parse().ok()?;
        return (step >= 1).then_some(CronField::Every(step));
    }
    let mut numbers = Vec::new();
    for part in raw.split(',') {
        let number: u8 = part.parse().ok()?;
        if number > ceiling {
            return None;
        }
        numbers.push(number);
    }
    (!numbers.is_empty()).then_some(CronField::List(numbers))
}

fn parse_cron(line: &str) -> Option<ScheduleSpec> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let [minute, hour, day_of_month, month, day_of_week] = fields[..] else {
        return None;
    };
    let minute = read_field(minute, 59)?;
    let hour = read_field(hour, 23)?;
    // Cron counts Sunday as both 0 and 7.
    let day_of_week = read_field(day_of_week, 7)?;
    let day_of_month = read_field(day_of_month, 31)?;
    let month = read_field(month, 12)?;
    let every_day = day_of_month == CronField::Any && day_of_week == CronField::Any;

    // The interval readings first: a `*` or a `*/n` where a time of day would be is the shape
    // the Interval picker draws, and reading those back as "Advanced, at midnight" would turn
    // "every two days" into a daily schedule the moment somebody saved it.
    if every_day && month == CronField::Any {
        match (&minute, &hour) {
            (CronField::Any, CronField::Any) => {
                return Some(ScheduleSpec::interval(1, ScheduleUnit::Minutes));
            }
            (CronField::Every(step), CronField::Any) => {
                return Some(ScheduleSpec::interval(*step, ScheduleUnit::Minutes));
            }
            (CronField::List(minutes), CronField::Any) if minutes == &[0] => {
                return Some(ScheduleSpec::interval(1, ScheduleUnit::Hours));
            }
            (CronField::List(minutes), CronField::Every(step)) if minutes == &[0] => {
                return Some(ScheduleSpec::interval(*step, ScheduleUnit::Hours));
            }
            _ => {}
        }
    }
    if month == CronField::Any
        && day_of_week == CronField::Any
        && let (CronField::List(minutes), CronField::List(hours), CronField::Every(step)) =
            (&minute, &hour, &day_of_month)
        && minutes == &[0]
        && hours == &[0]
    {
        return Some(ScheduleSpec::interval(*step, ScheduleUnit::Days));
    }

    // Everything else the editor can draw is Advanced: a literal time of day, on days that are
    // named rather than stepped.
    let (CronField::List(minutes), CronField::List(hours)) = (&minute, &hour) else {
        return None;
    };
    let &[minute] = minutes.as_slice() else {
        return None;
    };
    let mut spec = ScheduleSpec::advanced_daily(*hours.first()?, minute);
    spec.times = hours.iter().map(|hour| (*hour, minute)).collect();
    spec.months = match month {
        CronField::Any => Vec::new(),
        CronField::List(months) => months,
        CronField::Every(_) => return None,
    };
    match (day_of_month, day_of_week) {
        (CronField::Any, CronField::Any) => spec.day_kind = ScheduleDayKind::EveryDay,
        (CronField::Any, CronField::List(days)) => {
            spec.day_kind = ScheduleDayKind::Weekdays;
            spec.weekdays = days;
        }
        (CronField::List(days), CronField::Any) => {
            spec.day_kind = ScheduleDayKind::DaysOfMonth;
            spec.month_days = days;
        }
        // Cron reads a line with both a date and a weekday as "either", which is not a thing
        // the pickers can say and not a thing anybody means. The line stays as written.
        _ => return None,
    }
    Some(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seven things the "On a schedule" menu offers, as the lines the server will keep.
    #[test]
    fn every_preset_is_one_cron_line() {
        let cron = |preset: &str| ScheduleSpec::from_preset(preset).to_cron().unwrap();
        assert_eq!(cron("Every hour"), "0 * * * *");
        assert_eq!(cron("Every day"), "0 9 * * *");
        assert_eq!(cron("Weekdays"), "0 9 * * 1,2,3,4,5");
        assert_eq!(cron("Every week"), "0 9 * * 1");
        assert_eq!(cron("Every month"), "0 8 1 * *");
        assert_eq!(cron("Interval"), "*/30 * * * *");
        assert_eq!(cron("Advanced..."), "0 9 * * *");
    }

    /// Every preset survives the round trip unchanged, which is what lets the editor open a
    /// routine the server sent and show the same words the person chose it by.
    #[test]
    fn a_preset_read_back_off_the_wire_is_the_preset_again() {
        for preset in [
            "Every hour",
            "Every day",
            "Weekdays",
            "Every week",
            "Every month",
            "Interval",
        ] {
            let spec = ScheduleSpec::from_preset(preset);
            let line = spec.to_cron().unwrap();
            assert_eq!(
                ScheduleSpec::from_cron(&line),
                spec,
                "{preset} came back as something else off `{line}`"
            );
            assert_ne!(
                ScheduleSpec::from_cron(&line).mode,
                ScheduleUiMode::Custom,
                "{preset} fell through to the raw line"
            );
        }
    }

    #[test]
    fn an_interval_steps_the_field_it_fits_in() {
        let cron = |every, unit| ScheduleSpec::interval(every, unit).to_cron().unwrap();
        assert_eq!(cron(1, ScheduleUnit::Minutes), "* * * * *");
        assert_eq!(cron(5, ScheduleUnit::Minutes), "*/5 * * * *");
        assert_eq!(cron(1, ScheduleUnit::Hours), "0 * * * *");
        assert_eq!(cron(6, ScheduleUnit::Hours), "0 */6 * * *");
        assert_eq!(cron(1, ScheduleUnit::Days), "0 0 * * *");
        assert_eq!(cron(2, ScheduleUnit::Days), "0 0 */2 * *");
    }

    /// Cron steps inside one field, so an interval that overflows its field is not a schedule
    /// at all — `*/90` in a field that counts to 59 fires never.
    #[test]
    fn an_interval_longer_than_its_field_is_refused_with_a_sentence() {
        let error = ScheduleSpec::interval(90, ScheduleUnit::Minutes)
            .to_cron()
            .unwrap_err();
        assert!(error.sentence().contains("59"), "{error}");
        assert!(
            ScheduleSpec::interval(0, ScheduleUnit::Hours)
                .to_cron()
                .is_err()
        );
        assert!(
            ScheduleSpec::interval(48, ScheduleUnit::Hours)
                .to_cron()
                .is_err()
        );
        assert!(
            ScheduleSpec::interval(60, ScheduleUnit::Days)
                .to_cron()
                .is_err()
        );
    }

    /// Two times of day are one line while they share the minute, and two routines when they
    /// do not: cron has one minute field for the whole line.
    #[test]
    fn two_times_of_day_are_one_line_only_when_the_minute_agrees() {
        let mut spec = ScheduleSpec::advanced_daily(9, 0);
        spec.times = vec![(9, 0), (17, 0)];
        assert_eq!(spec.to_cron().unwrap(), "0 9,17 * * *");

        spec.times = vec![(9, 0), (17, 30)];
        let error = spec.to_cron().unwrap_err();
        assert!(error.sentence().contains("9:00 AM"), "{error}");
        assert!(error.sentence().contains("5:30 PM"), "{error}");
    }

    #[test]
    fn a_schedule_with_nothing_picked_never_runs_and_says_so() {
        let mut spec = ScheduleSpec::advanced_daily(9, 0);
        spec.times.clear();
        assert!(
            spec.to_cron()
                .unwrap_err()
                .sentence()
                .contains("time of day")
        );

        let mut spec = ScheduleSpec::advanced_daily(9, 0);
        spec.day_kind = ScheduleDayKind::Weekdays;
        assert!(
            spec.to_cron()
                .unwrap_err()
                .sentence()
                .contains("day of the week")
        );

        let mut spec = ScheduleSpec::advanced_daily(9, 0);
        spec.day_kind = ScheduleDayKind::DaysOfMonth;
        assert!(spec.to_cron().unwrap_err().sentence().contains("date"));
    }

    #[test]
    fn months_narrow_the_line_and_read_back() {
        let mut spec = ScheduleSpec::advanced_daily(8, 30);
        spec.day_kind = ScheduleDayKind::DaysOfMonth;
        spec.month_days = vec![1, 15];
        spec.months = vec![3, 6, 9, 12];
        let line = spec.to_cron().unwrap();
        assert_eq!(line, "30 8 1,15 3,6,9,12 *");
        assert_eq!(ScheduleSpec::from_cron(&line), spec);
    }

    /// Whatever was typed under Custom goes as typed. `@every 1h` is a line this server takes
    /// and no cron parser here would recognise; the app is not the judge of it.
    #[test]
    fn a_custom_line_goes_as_written_and_comes_back_as_written() {
        let spec = ScheduleSpec::custom("  @every 1h  ");
        assert_eq!(spec.to_cron().unwrap(), "@every 1h");
        assert_eq!(
            ScheduleSpec::from_cron("@every 1h"),
            ScheduleSpec::custom("@every 1h")
        );
        assert!(ScheduleSpec::custom("   ").to_cron().is_err());
    }

    /// A line with a shape the pickers cannot draw is kept, not approximated: ranges, names,
    /// and the one cron reads as "either the date or the weekday".
    #[test]
    fn a_line_the_pickers_cannot_draw_stays_the_line() {
        for line in [
            "0 9 * * 1-5",
            "0 9 * * MON",
            "0 9 1 * 1",
            "0 9 * *",
            "",
            "0 9 * * * *",
        ] {
            let spec = ScheduleSpec::from_cron(line);
            assert_eq!(spec.mode, ScheduleUiMode::Custom, "{line}");
            assert_eq!(spec.expr, line.trim(), "{line}");
        }
    }

    /// An hourly line with an offset is not the Interval picker's `every hour`, which fires on
    /// the hour: reading it back as one would move somebody's schedule by half an hour.
    #[test]
    fn an_offset_hour_is_not_the_hourly_interval() {
        assert_eq!(
            ScheduleSpec::from_cron("30 * * * *"),
            ScheduleSpec::custom("30 * * * *")
        );
        assert_eq!(
            ScheduleSpec::from_cron("0 * * * *"),
            ScheduleSpec::interval(1, ScheduleUnit::Hours)
        );
    }

    /// The label under the trigger row is what the person reads, and it comes off the spec the
    /// line was read back into.
    #[test]
    fn a_line_read_back_is_labelled_the_way_it_was_chosen() {
        let label = |line: &str| ScheduleSpec::from_cron(line).label();
        assert_eq!(label("0 * * * *"), "Every hour");
        assert_eq!(label("*/30 * * * *"), "Every 30 minutes");
        assert_eq!(label("0 9 * * *"), "Every day at 9:00 AM");
        assert_eq!(label("0 9 * * 1,2,3,4,5"), "Weekdays at 9:00 AM");
        assert_eq!(label("0 8 1 * *"), "Monthly on the 1st at 8:00 AM");
        assert_eq!(label("@every 1h"), "@every 1h");
    }
}
