//! What a routine's schedule is, the one cron line it comes down to, and what a line means in
//! words.
//!
//! The wake editor is written for a person: every 30 minutes, weekdays at nine, the 1st of the
//! month. The server takes one cron line. This module is where the two meet, and it is
//! deliberately a plain module with no GPUI in it — the interesting part is arithmetic about
//! when a thing fires, and arithmetic is worth being able to test without a window.
//!
//! Standalone, too: `rustc --edition 2024 --test src/cron_spec.rs` builds and runs the tests at
//! the bottom without the rest of the app. Nothing here may reach for `crate::`.
//!
//! # How the server reads a line
//!
//! opengrok-server puts a seconds field of `0` in front of a five-field line and hands it to the
//! `cron` crate, 0.17 (opengrok-core `schedule.rs`, `normalized_cron`). That is not quite the cron
//! most people know, and what this module writes and reads follows the server, because the
//! server is what runs it:
//!
//! * Times are the routine's own zone's. The server reads a routine's line in the IANA zone it
//!   keeps for it (opengrok-server #316: `tz`, the account's `timeZone` for one this app makes,
//!   UTC for one stored before zones), so `0 9 * * *` in Asia/Manila runs at 9:00 there. The
//!   words here name no zone: the editor names the routine's beside them where it is not this
//!   computer's, and says when the next run lands in the person's own time.
//! * Days of the week count from Sunday as 1 to Saturday as 7, and 0 is refused. In the cron most
//!   people know Sunday is 0 and Monday 1, so `1-5` there is Monday to Friday and here is Sunday
//!   to Thursday. The lines written here name their days (`MON-FRI`), which both read alike; a
//!   number in a line read here is read the server's way.
//! * A day of the month and a day of the week given together must both hold: `0 9 1 * MON` is a
//!   1st that falls on a Monday, where the usual cron runs on either.
//! * A step counts from the start of its field: `*/45` minutes is :00 and :45 of each hour, and
//!   `*/10` days of the month is the 1st, 11th, 21st and 31st.
//! * Five fields, or one of `@hourly`, `@daily`, `@weekly`, `@monthly` and `@yearly`. Not
//!   `@every`, and not `L` for the last day of the month. The editor's Cron tab takes five fields.
//!
//! The translation is not total in either direction, and that is the point:
//!
//! * [`ScheduleSpec::to_cron`] says no to a schedule that is not one line the server takes — a
//!   weekly schedule with no day picked, a line that is not five fields — with a sentence to show
//!   the person instead of a schedule that would never fire.
//! * [`ScheduleSpec::from_cron`] reads back the shapes the editor's tabs draw and leaves the rest
//!   to the Cron tab, where the line is shown as written rather than approximated, with what it
//!   means in words beside it ([`describe_cron`]).

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

/// The wake editor's tabs: one for each shape of schedule it draws, and the webhook, which is no
/// schedule at all but is the other way a routine can be set off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeTab {
    Every,
    Daily,
    Weekly,
    Monthly,
    Webhook,
    Cron,
}

impl WakeTab {
    pub const ALL: [Self; 6] = [
        Self::Every,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Webhook,
        Self::Cron,
    ];

    /// The tab's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Every => "Every",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Webhook => "Webhook",
            Self::Cron => "Cron",
        }
    }

    /// The tab's word in ids: `routine-wake-tab-{word}`.
    pub fn word(self) -> &'static str {
        match self {
            Self::Every => "every",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Webhook => "webhook",
            Self::Cron => "cron",
        }
    }
}

/// The days of the week, Sunday first, as the weekly tab's chips and the summaries name them.
/// A schedule keeps its days by their place here: 0 is Sunday and 6 Saturday.
pub const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAYS_IN_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
/// The same days as a written line names them.
const CRON_WEEKDAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

/// The months, as the month chips and the summaries name them. A schedule keeps its months as
/// cron does, January as 1.
pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleSpec {
    pub mode: ScheduleUiMode,
    pub every: u32,
    pub unit: ScheduleUnit,
    pub expr: String,
    pub months: Vec<u8>,
    pub day_kind: ScheduleDayKind,
    /// Days of the week, 0 for Sunday to 6 for Saturday ([`WEEKDAYS`]).
    pub weekdays: Vec<u8>,
    pub month_days: Vec<u8>,
    /// The time of day, as (hour, minute) on a 24-hour clock. The tabs draw one; a line read
    /// back with more is the Cron tab's.
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

    /// The tab that draws this schedule.
    pub fn tab(&self) -> WakeTab {
        match (self.mode, self.day_kind) {
            (ScheduleUiMode::Interval, _) => WakeTab::Every,
            (ScheduleUiMode::Custom, _) => WakeTab::Cron,
            // The tabs draw one time of day; more is a line for the Cron tab.
            (ScheduleUiMode::Advanced, _) if self.times.len() != 1 => WakeTab::Cron,
            (ScheduleUiMode::Advanced, ScheduleDayKind::EveryDay) => WakeTab::Daily,
            (ScheduleUiMode::Advanced, ScheduleDayKind::Weekdays) => WakeTab::Weekly,
            (ScheduleUiMode::Advanced, ScheduleDayKind::DaysOfMonth) => WakeTab::Monthly,
        }
    }

    /// The same schedule as far as it goes, drawn by another tab: the time of day and the months
    /// carry over, a tab with days of its own starts from Monday to Friday or from the 1st, and
    /// the Cron tab starts from the line the schedule was, so it can be written further.
    pub fn on_tab(&self, tab: WakeTab) -> Self {
        let mut next = self.clone();
        let time = self.times.first().copied().unwrap_or((9, 0));
        match tab {
            WakeTab::Every => {
                if self.mode != ScheduleUiMode::Interval {
                    next.every = 1;
                    next.unit = ScheduleUnit::Hours;
                }
                next.mode = ScheduleUiMode::Interval;
            }
            WakeTab::Daily | WakeTab::Weekly | WakeTab::Monthly => {
                next.mode = ScheduleUiMode::Advanced;
                next.times = vec![time];
                next.day_kind = match tab {
                    WakeTab::Daily => ScheduleDayKind::EveryDay,
                    WakeTab::Weekly => ScheduleDayKind::Weekdays,
                    _ => ScheduleDayKind::DaysOfMonth,
                };
                if tab == WakeTab::Weekly && next.weekdays.is_empty() {
                    next.weekdays = vec![1, 2, 3, 4, 5];
                }
                if tab == WakeTab::Monthly && next.month_days.is_empty() {
                    next.month_days = vec![1];
                }
            }
            WakeTab::Cron => {
                if self.mode != ScheduleUiMode::Custom {
                    next.expr = self.to_cron().unwrap_or_default();
                }
                next.mode = ScheduleUiMode::Custom;
            }
            // A webhook is not a schedule; the editor keeps the schedule as it was under it.
            WakeTab::Webhook => {}
        }
        next
    }

    /// The schedule in plain words, as the list of when a routine runs says it and as the editor
    /// says what was picked: "Weekdays at 9:00 AM, in Jan and Mar".
    pub fn label(&self) -> String {
        match self.mode {
            ScheduleUiMode::Interval => interval_label(self.every, self.unit),
            ScheduleUiMode::Custom => {
                let line = self.expr.trim();
                if line.is_empty() {
                    "No cron line yet".into()
                } else {
                    describe_cron(line).unwrap_or_else(|| line.to_string())
                }
            }
            ScheduleUiMode::Advanced => advanced_label(self),
        }
    }
}

/// "9:00 AM": a time of day as the summaries say it.
pub fn format_clock(hour: u8, minute: u8) -> String {
    let (h12, am) = twelve_hour(hour);
    format!("{}:{:02} {}", h12, minute, if am { "AM" } else { "PM" })
}

/// "9 AM": an hour on its own.
fn format_hour(hour: u8) -> String {
    let (h12, am) = twelve_hour(hour);
    format!("{} {}", h12, if am { "AM" } else { "PM" })
}

/// An hour of a 24-hour clock on a 12-hour one, and whether it is before noon.
pub fn twelve_hour(hour: u8) -> (u8, bool) {
    match hour {
        0 => (12, true),
        1..=11 => (hour, true),
        12 => (12, false),
        _ => (hour - 12, false),
    }
}

fn ordinal(n: u8) -> String {
    let suffix = if matches!(n % 100, 11..=13) {
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

/// "a", "a and b", "a, b and c".
fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The runs of consecutive numbers in a sorted list, as (first, last).
fn runs(sorted: &[u8]) -> Vec<(u8, u8)> {
    let mut runs: Vec<(u8, u8)> = Vec::new();
    for &n in sorted {
        match runs.last_mut() {
            Some((_, last)) if n == *last + 1 => *last = n,
            _ => runs.push((n, n)),
        }
    }
    runs
}

/// Numbers named the way a summary names them: a run of three or more as "a to b", the rest
/// one by one, all joined with "and".
fn named_runs(numbers: &[u8], name: impl Fn(u8) -> String) -> String {
    let mut sorted = numbers.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    for (first, last) in runs(&sorted) {
        if last >= first + 2 {
            parts.push(format!("{} to {}", name(first), name(last)));
        } else {
            for n in first..=last {
                parts.push(name(n));
            }
        }
    }
    join_and(&parts)
}

fn month_name(month: u8) -> String {
    MONTHS
        .get(usize::from(month.max(1)) - 1)
        .map_or_else(|| month.to_string(), |name| name.to_string())
}

fn weekday_name(day: u8) -> String {
    WEEKDAYS
        .get(usize::from(day))
        .map_or_else(|| day.to_string(), |name| name.to_string())
}

/// ", in Jan and Mar": the months a schedule is kept to, or nothing for every month.
fn months_suffix(months: &[u8]) -> String {
    let mut sorted = months.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.is_empty() || sorted.len() == 12 {
        return String::new();
    }
    format!(", in {}", named_runs(&sorted, month_name))
}

/// The days of the week a schedule runs on, as the start of its summary.
fn weekday_phrase(days: &[u8]) -> String {
    let mut sorted = days.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    match sorted.as_slice() {
        [] => "No day of the week".into(),
        [1, 2, 3, 4, 5] => "Weekdays".into(),
        [0, 6] => "Weekends".into(),
        [0, 1, 2, 3, 4, 5, 6] => "Every day".into(),
        [one] => format!("Every {}", WEEKDAYS_IN_FULL[usize::from(*one).min(6)]),
        many => named_runs(many, weekday_name),
    }
}

/// `Every N minutes/hours/days`, in words that are true of the line it becomes. A step that does
/// not divide its field starts again at the field's start: every 45 minutes is :00 and :45 of
/// each hour, and saying "every 45 minutes" alone would promise a run at 1:30 there is not.
fn interval_label(every: u32, unit: ScheduleUnit) -> String {
    let noun = |one: &str, many: &str| if every == 1 { one } else { many }.to_string();
    match unit {
        ScheduleUnit::Minutes => {
            let base = format!("Every {every} {}", noun("minute", "minutes"));
            if every == 0 || every > 59 || 60 % every == 0 {
                base
            } else {
                format!("{base}, starting again at :00 each hour")
            }
        }
        ScheduleUnit::Hours => {
            let base = format!("Every {every} {}", noun("hour", "hours"));
            if every == 0 || every > 23 || 24 % every == 0 {
                base
            } else {
                format!("{base}, starting again at 12 AM each day")
            }
        }
        ScheduleUnit::Days => {
            let base = format!("Every {every} {} at 12:00 AM", noun("day", "days"));
            if every <= 1 {
                base
            } else {
                format!("{base}, starting again on the 1st of each month")
            }
        }
    }
}

fn advanced_label(spec: &ScheduleSpec) -> String {
    let times: Vec<String> = spec
        .times
        .iter()
        .map(|(hour, minute)| format_clock(*hour, *minute))
        .collect();
    let time = if times.is_empty() {
        "no time of day".to_string()
    } else {
        join_and(&times)
    };
    let days = match spec.day_kind {
        ScheduleDayKind::EveryDay => format!("Every day at {time}"),
        ScheduleDayKind::Weekdays => format!("{} at {time}", weekday_phrase(&spec.weekdays)),
        ScheduleDayKind::DaysOfMonth => {
            if spec.month_days.is_empty() {
                format!("Monthly on no date at {time}")
            } else {
                format!(
                    "Monthly on the {} at {time}",
                    named_runs(&spec.month_days, ordinal)
                )
            }
        }
    };
    format!("{days}{}", months_suffix(&spec.months))
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
            ScheduleUiMode::Custom => five_fields(&self.expr),
            ScheduleUiMode::Advanced => advanced_cron(self),
        }
    }

    /// The editor's schedule for a line the server sent back.
    ///
    /// Only the shapes the tabs draw are read back; anything else is `Custom`, the Cron tab,
    /// which shows the line as written. That is the honest reading — an approximation would let
    /// somebody save the editor's idea of their line over their own.
    pub fn from_cron(line: &str) -> Self {
        parse_cron(line).unwrap_or_else(|| Self::custom(line.trim()))
    }

    /// A line as opengrok-server keeps it: six fields, a seconds field of `0` first
    /// (`0 0 9 * * MON`). Read the way the server shows it (`display_cron` in opengrok-core
    /// `schedule.rs`), with that `0` dropped, so a routine opens on the tab that drew it and
    /// saving it unchanged sends nothing.
    pub fn from_server_cron(line: &str) -> Self {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() == 6 && fields[0] == "0" {
            Self::from_cron(&fields[1..].join(" "))
        } else {
            Self::from_cron(line)
        }
    }
}

/// The Cron tab's line, as typed, when it is five fields: minute, hour, day of the month, month
/// and day of the week. The server is the judge of what is in them; what this refuses is a line
/// that is not one of its own shape at all, `@every` among them, which the server refuses too.
fn five_fields(line: &str) -> Result<String, ScheduleNotCron> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields.len() {
        0 => Err(ScheduleNotCron::new(
            "A cron line with nothing written in it is not a schedule: type one, like \
             `0 9 * * MON-FRI`.",
        )),
        5 => Ok(fields.join(" ")),
        _ => Err(ScheduleNotCron::new(
            "A cron line here has five fields: minute, hour, day of the month, month and day of \
             the week, like `0 9 * * MON-FRI`.",
        )),
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
             Choose a larger unit, or write the line on the Cron tab."
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
            ("*".to_string(), weekday_field(&spec.weekdays))
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

/// Days of the week as a written line names them: `MON-FRI`, `MON,WED,FRI`. Names and not
/// numbers, because the server counts Sunday as 1 where most people's cron counts it as 0, and
/// a name means the same day to both.
fn weekday_field(days: &[u8]) -> String {
    let mut sorted: Vec<u8> = days.iter().copied().filter(|day| *day < 7).collect();
    sorted.sort_unstable();
    sorted.dedup();
    runs(&sorted)
        .into_iter()
        .flat_map(|(first, last)| {
            let name = |day: u8| CRON_WEEKDAYS[usize::from(day)];
            if last >= first + 2 {
                vec![format!("{}-{}", name(first), name(last))]
            } else {
                (first..=last).map(|day| name(day).to_string()).collect()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
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

/// One field of a cron line, read the way the server reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CronField {
    /// `*` (or `?`, for the two day fields): every one.
    Any,
    /// `*/n`: every nth, counted from the start of the field.
    Every(u32),
    /// The ones named, sorted: numbers, names, ranges, and lists of them. Days of the week are
    /// kept the schedule's way, 0 for Sunday.
    List(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Minute,
    Hour,
    DayOfMonth,
    Month,
    DayOfWeek,
}

impl Field {
    /// The numbers the field takes, the server's way: days of the week are 1 (Sunday) to 7.
    fn bounds(self) -> (u8, u8) {
        match self {
            Self::Minute => (0, 59),
            Self::Hour => (0, 23),
            Self::DayOfMonth => (1, 31),
            Self::Month => (1, 12),
            Self::DayOfWeek => (1, 7),
        }
    }

    /// A name in the field, as the server takes it: three letters or the whole word, in any
    /// case. Days of the week by the server's numbers.
    fn named(self, word: &str) -> Option<u8> {
        let word = word.to_ascii_lowercase();
        let find = |names: &[&str]| {
            names.iter().position(|name| {
                let name = name.to_ascii_lowercase();
                word == name[..3] || word == name
            })
        };
        match self {
            Self::Month => find(&[
                "january",
                "february",
                "march",
                "april",
                "may",
                "june",
                "july",
                "august",
                "september",
                "october",
                "november",
                "december",
            ])
            .map(|at| at as u8 + 1),
            // "tues" and "thurs" are the server's too.
            Self::DayOfWeek => match word.as_str() {
                "tues" => Some(3),
                "thurs" => Some(5),
                _ => find(&WEEKDAYS_IN_FULL).map(|at| at as u8 + 1),
            },
            _ => None,
        }
    }

    fn number(self, raw: &str) -> Option<u8> {
        let (low, high) = self.bounds();
        let number = match raw.parse::<u8>() {
            Ok(number) => number,
            Err(_) => self.named(raw)?,
        };
        (low..=high).contains(&number).then_some(number)
    }
}

/// One field read, or `None` for one the server would refuse or this module cannot follow.
fn read_field(raw: &str, field: Field) -> Option<CronField> {
    let any_day = matches!(field, Field::DayOfMonth | Field::DayOfWeek);
    if raw == "*" || (raw == "?" && any_day) {
        return Some(CronField::Any);
    }
    let (low, high) = field.bounds();
    if let Some(step) = raw.strip_prefix("*/") {
        let step: u32 = step.parse().ok()?;
        if step == 0 {
            return None;
        }
        // A stepped month or day of the week is a list of them; a stepped minute, hour or date
        // is an interval, kept while it fits the field (`*/90` minutes does not, and is read as
        // the Cron tab's line, not as an interval the editor would then refuse to save).
        if matches!(field, Field::Month | Field::DayOfWeek) {
            let numbers = (u32::from(low)..=u32::from(high))
                .step_by(step as usize)
                .map(|n| n as u8)
                .collect::<Vec<_>>();
            return Some(CronField::List(server_days(field, numbers)));
        }
        let ceiling = match field {
            Field::Minute => 59,
            Field::Hour => 23,
            _ => 31,
        };
        return (1..=ceiling)
            .contains(&step)
            .then_some(CronField::Every(step));
    }
    let mut numbers = Vec::new();
    for part in raw.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (range, Some(step.parse::<usize>().ok().filter(|s| *s > 0)?)),
            None => (part, None),
        };
        let (first, last) = match range.split_once('-') {
            Some((first, last)) => (field.number(first)?, field.number(last)?),
            None => {
                let only = field.number(range)?;
                (only, if step.is_some() { high } else { only })
            }
        };
        if first > last {
            return None;
        }
        numbers.extend((first..=last).step_by(step.unwrap_or(1)));
    }
    numbers.sort_unstable();
    numbers.dedup();
    (!numbers.is_empty()).then(|| CronField::List(server_days(field, numbers)))
}

/// Days of the week from the server's numbers (1 for Sunday) to the schedule's (0 for Sunday);
/// any other field as it is.
fn server_days(field: Field, numbers: Vec<u8>) -> Vec<u8> {
    if field == Field::DayOfWeek {
        numbers.into_iter().map(|day| day - 1).collect()
    } else {
        numbers
    }
}

/// The five fields of a line, each read, or `None` for a line that is not five readable fields.
fn read_line(line: &str) -> Option<[CronField; 5]> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let [minute, hour, day_of_month, month, day_of_week] = fields[..] else {
        return None;
    };
    Some([
        read_field(minute, Field::Minute)?,
        read_field(hour, Field::Hour)?,
        read_field(day_of_month, Field::DayOfMonth)?,
        read_field(month, Field::Month)?,
        read_field(day_of_week, Field::DayOfWeek)?,
    ])
}

fn parse_cron(line: &str) -> Option<ScheduleSpec> {
    let [minute, hour, day_of_month, month, day_of_week] = read_line(line)?;
    let every_day = day_of_month == CronField::Any && day_of_week == CronField::Any;

    // The interval readings first: a `*` or a `*/n` where a time of day would be is the shape
    // the Every tab draws, and reading those back as a time of day at midnight would turn
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

    // Everything else the tabs draw is one time of day, on days that are named rather than
    // stepped. Two times of day are the Cron tab's: the tabs draw one.
    let (CronField::List(minutes), CronField::List(hours)) = (&minute, &hour) else {
        return None;
    };
    let (&[minute], &[hour]) = (minutes.as_slice(), hours.as_slice()) else {
        return None;
    };
    let mut spec = ScheduleSpec::advanced_daily(hour, minute);
    spec.months = match month {
        CronField::Any => Vec::new(),
        CronField::List(months) => months,
        CronField::Every(_) => return None,
    };
    match (day_of_month, day_of_week) {
        (CronField::Any, CronField::Any) => spec.day_kind = ScheduleDayKind::EveryDay,
        (CronField::Any, CronField::List(days)) if days.len() == 7 => {
            spec.day_kind = ScheduleDayKind::EveryDay;
        }
        (CronField::Any, CronField::List(days)) => {
            spec.day_kind = ScheduleDayKind::Weekdays;
            spec.weekdays = days;
        }
        (CronField::List(days), CronField::Any) => {
            spec.day_kind = ScheduleDayKind::DaysOfMonth;
            spec.month_days = days;
        }
        // The server runs a line with both a date and a weekday only when both hold, which no
        // tab draws. The line stays as written, and its words say so.
        _ => return None,
    }
    Some(spec)
}

/// Whether a line gives its days of the week by number, which the server counts from Sunday as
/// 1: the Cron tab says so beside it, since most people's cron counts from Sunday as 0.
pub fn numbered_weekdays(line: &str) -> bool {
    line.split_whitespace()
        .nth(4)
        .is_some_and(|days| days.chars().any(|c| c.is_ascii_digit()))
}

/// What a five-field line means in words, read the way the server reads it: the schedule's own
/// words where one of the editor's tabs draws it, and otherwise field by field ("Every 15 minutes
/// from 9:00 AM to 5:45 PM, on weekdays"). `None` for a line that is not five fields the server
/// would take.
pub fn describe_cron(line: &str) -> Option<String> {
    let line = line.trim();
    if let Some(spec) = parse_cron(line) {
        return Some(spec.label());
    }
    let [minute, hour, day_of_month, month, day_of_week] = read_line(line)?;
    let mut parts = vec![time_phrase(&minute, &hour)];
    if let Some(days) = day_phrase(&day_of_month, &day_of_week) {
        parts.push(days);
    }
    if let CronField::List(months) = &month
        && months.len() < 12
    {
        parts.push(format!("in {}", named_runs(months, month_name)));
    }
    let words = parts.join(", ");
    let mut chars = words.chars();
    Some(chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    }))
}

/// When in the day a line fires.
fn time_phrase(minute: &CronField, hour: &CronField) -> String {
    let minutes_of = |field: &CronField| match field {
        CronField::Any => (0..60).collect::<Vec<u8>>(),
        CronField::Every(step) => (0..60).step_by(*step as usize).collect(),
        CronField::List(minutes) => minutes.clone(),
    };
    let minute_marks = |minutes: &[u8]| {
        join_and(
            &minutes
                .iter()
                .map(|m| format!(":{m:02}"))
                .collect::<Vec<_>>(),
        )
    };
    let how_often = match minute {
        CronField::Any => Some("every minute".to_string()),
        CronField::Every(step) => Some(if 60 % step == 0 {
            format!("every {step} minutes")
        } else {
            format!("every {step} minutes starting again at :00 each hour")
        }),
        CronField::List(_) => None,
    };
    match hour {
        CronField::Any => how_often
            .unwrap_or_else(|| format!("at {} past every hour", minute_marks(&minutes_of(minute)))),
        CronField::Every(step) => {
            let hours = format!("every {step} hours from 12 AM");
            match how_often {
                Some(often) => format!("{often}, in {hours}"),
                None => format!("at {} past {hours}", minute_marks(&minutes_of(minute))),
            }
        }
        CronField::List(hours) => {
            let minutes = minutes_of(minute);
            if let (Some(often), [(first, last)]) = (&how_often, runs(hours).as_slice()) {
                let (Some(&start), Some(&end)) = (minutes.first(), minutes.last()) else {
                    return String::new();
                };
                return format!(
                    "{often} from {} to {}",
                    format_clock(*first, start),
                    format_clock(*last, end)
                );
            }
            if how_often.is_none() && minutes.len() == 1 && hours.len() <= 6 {
                let times: Vec<String> = hours
                    .iter()
                    .map(|hour| format_clock(*hour, minutes[0]))
                    .collect();
                return format!("at {}", join_and(&times));
            }
            let hours = named_runs(hours, format_hour);
            match how_often {
                Some(often) => format!("{often} in the hours of {hours}"),
                None => format!("at {} past {hours}", minute_marks(&minutes)),
            }
        }
    }
}

/// Which days a line fires on.
fn day_phrase(day_of_month: &CronField, day_of_week: &CronField) -> Option<String> {
    let dates = match day_of_month {
        CronField::Any => None,
        CronField::Every(step) => Some(format!(
            "every {step} days starting again on the 1st of each month"
        )),
        CronField::List(days) => Some(format!("on the {}", named_runs(days, ordinal))),
    };
    let weekdays = match day_of_week {
        CronField::List(days) if days.len() < 7 => Some(days.as_slice()),
        _ => None,
    };
    Some(match (dates, weekdays) {
        (None, None) => "every day".to_string(),
        (Some(dates), None) => dates,
        (None, Some(days)) => match weekday_phrase(days).as_str() {
            "Weekdays" => "on weekdays".to_string(),
            "Weekends" => "on weekends".to_string(),
            phrase => match phrase.strip_prefix("Every ") {
                Some(day) => format!("on {day}s"),
                None => format!("on {phrase}"),
            },
        },
        // Both must hold, on this server: a date that is also one of these days.
        (Some(dates), Some(days)) => format!("{dates}, when it is {}", a_weekday(days)),
    })
}

/// "a Monday", "a weekday", "a Mon, Wed or Fri": one day of those given, as a date can be.
fn a_weekday(days: &[u8]) -> String {
    match weekday_phrase(days).as_str() {
        "Weekdays" => "a weekday".to_string(),
        "Weekends" => "a Saturday or Sunday".to_string(),
        phrase => match phrase.strip_prefix("Every ") {
            Some(day) => format!("a {day}"),
            None => {
                let names: Vec<String> = days.iter().map(|day| weekday_name(*day)).collect();
                match names.as_slice() {
                    [rest @ .., last] if !rest.is_empty() => {
                        format!("a {} or {last}", rest.join(", "))
                    }
                    _ => format!("a {phrase}"),
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line each tab writes for what it shows, which is the line the server keeps and runs.
    /// Days of the week are named, months and dates are numbers, and a time is minute then hour.
    #[test]
    fn every_tab_writes_a_line_the_server_takes() {
        let cron = |spec: ScheduleSpec| spec.to_cron().unwrap();
        // Every
        assert_eq!(
            cron(ScheduleSpec::interval(1, ScheduleUnit::Minutes)),
            "* * * * *"
        );
        assert_eq!(
            cron(ScheduleSpec::interval(15, ScheduleUnit::Minutes)),
            "*/15 * * * *"
        );
        assert_eq!(
            cron(ScheduleSpec::interval(1, ScheduleUnit::Hours)),
            "0 * * * *"
        );
        assert_eq!(
            cron(ScheduleSpec::interval(6, ScheduleUnit::Hours)),
            "0 */6 * * *"
        );
        assert_eq!(
            cron(ScheduleSpec::interval(1, ScheduleUnit::Days)),
            "0 0 * * *"
        );
        assert_eq!(
            cron(ScheduleSpec::interval(2, ScheduleUnit::Days)),
            "0 0 */2 * *"
        );
        // Daily
        let mut daily = ScheduleSpec::advanced_daily(9, 30);
        assert_eq!(cron(daily.clone()), "30 9 * * *");
        daily.months = vec![3, 1];
        assert_eq!(cron(daily), "30 9 * 1,3 *");
        // Weekly
        let weekly = |days: &[u8]| {
            let mut spec = ScheduleSpec::advanced_daily(9, 0).on_tab(WakeTab::Weekly);
            spec.weekdays = days.to_vec();
            cron(spec)
        };
        assert_eq!(weekly(&[1, 2, 3, 4, 5]), "0 9 * * MON-FRI");
        assert_eq!(weekly(&[1, 3, 5]), "0 9 * * MON,WED,FRI");
        assert_eq!(weekly(&[0, 6]), "0 9 * * SUN,SAT");
        assert_eq!(weekly(&[0, 1, 2, 4]), "0 9 * * SUN-TUE,THU");
        // Monthly
        let mut monthly = ScheduleSpec::advanced_daily(8, 0).on_tab(WakeTab::Monthly);
        monthly.month_days = vec![15, 1];
        assert_eq!(cron(monthly), "0 8 1,15 * *");
        // Cron
        assert_eq!(
            cron(ScheduleSpec::custom("  0  9 * *   MON-FRI ")),
            "0 9 * * MON-FRI"
        );
    }

    /// A weekly schedule names its days, because the server numbers them from Sunday as 1 and
    /// most people's cron from Sunday as 0: the editor's "Weekdays" used to write `1-5`, which
    /// the server runs Sunday to Thursday. A number in a line read back is read the server's
    /// way, so a routine already saved like that says what it really does.
    #[test]
    fn weekdays_are_named_and_a_numbered_one_is_read_the_servers_way() {
        let label = |line: &str| ScheduleSpec::from_cron(line).label();
        assert_eq!(label("0 9 * * MON-FRI"), "Weekdays at 9:00 AM");
        assert_eq!(label("0 9 * * mon,tue,wed,thu,fri"), "Weekdays at 9:00 AM");
        assert_eq!(
            label("0 9 * * 2-6"),
            "Weekdays at 9:00 AM",
            "2 is Monday there"
        );
        assert_eq!(label("0 9 * * 1-5"), "Sun to Thu at 9:00 AM");
        assert_eq!(label("0 9 * * 1,2,3,4,5"), "Sun to Thu at 9:00 AM");
        assert_eq!(label("0 9 * * 1"), "Every Sunday at 9:00 AM");
        assert_eq!(label("0 9 * * SAT,SUN"), "Weekends at 9:00 AM");
        assert_eq!(label("0 9 * * 1-7"), "Every day at 9:00 AM");
        assert_eq!(
            ScheduleSpec::from_cron("0 9 * * 0").tab(),
            WakeTab::Cron,
            "the server refuses day 0, so no tab draws it"
        );
        assert!(numbered_weekdays("0 9 * * 1-5"));
        assert!(!numbered_weekdays("0 9 * * MON-FRI"));
        assert!(!numbered_weekdays("0 9 1 * *"));
    }

    /// What each tab says under the editor, and in the list of when a routine runs: what was
    /// picked, in words, the months by name and never "Selected months", and the time on the
    /// routine's own clock, whose zone the words do not name: the editor names it beside them
    /// where it is not this computer's.
    #[test]
    fn a_summary_says_exactly_what_was_picked() {
        let mut spec = ScheduleSpec::advanced_daily(9, 0).on_tab(WakeTab::Weekly);
        spec.months = vec![1, 3];
        assert_eq!(spec.label(), "Weekdays at 9:00 AM, in Jan and Mar");
        spec.months = vec![3, 4, 5, 6, 12];
        assert_eq!(spec.label(), "Weekdays at 9:00 AM, in Mar to Jun and Dec");
        spec.months = (1..=12).collect();
        assert_eq!(
            spec.label(),
            "Weekdays at 9:00 AM",
            "every month is no month named"
        );
        spec.weekdays = vec![1, 3, 5];
        spec.times = vec![(14, 15)];
        assert_eq!(spec.label(), "Mon, Wed and Fri at 2:15 PM");
        spec.weekdays = vec![2];
        assert_eq!(spec.label(), "Every Tuesday at 2:15 PM");

        let mut monthly = ScheduleSpec::advanced_daily(8, 0).on_tab(WakeTab::Monthly);
        monthly.month_days = vec![1, 15];
        assert_eq!(monthly.label(), "Monthly on the 1st and 15th at 8:00 AM");
        monthly.month_days = vec![1, 2, 3, 22];
        assert_eq!(
            monthly.label(),
            "Monthly on the 1st to 3rd and 22nd at 8:00 AM"
        );

        assert_eq!(
            ScheduleSpec::advanced_daily(0, 5).label(),
            "Every day at 12:05 AM"
        );
        let every = |n, unit| ScheduleSpec::interval(n, unit).label();
        assert_eq!(every(1, ScheduleUnit::Hours), "Every 1 hour");
        assert_eq!(every(30, ScheduleUnit::Minutes), "Every 30 minutes");
        assert_eq!(every(1, ScheduleUnit::Minutes), "Every 1 minute");
        assert_eq!(every(1, ScheduleUnit::Days), "Every 1 day at 12:00 AM");
    }

    /// A step that does not divide its field starts again at the field's start, on the server
    /// as in any cron: the words say so, rather than promise runs that never come.
    #[test]
    fn an_uneven_step_says_it_starts_again() {
        let every = |n, unit| ScheduleSpec::interval(n, unit).label();
        assert_eq!(
            every(45, ScheduleUnit::Minutes),
            "Every 45 minutes, starting again at :00 each hour"
        );
        assert_eq!(
            every(5, ScheduleUnit::Hours),
            "Every 5 hours, starting again at 12 AM each day"
        );
        assert_eq!(every(8, ScheduleUnit::Hours), "Every 8 hours");
        assert_eq!(
            every(2, ScheduleUnit::Days),
            "Every 2 days at 12:00 AM, starting again on the 1st of each month"
        );
    }

    /// The Cron tab takes five fields, as written, and nothing else: not `@every`, which the
    /// server refuses, and not the `@daily` kind, which it takes but the tab does not offer.
    #[test]
    fn the_cron_tab_takes_five_fields_and_nothing_else() {
        assert_eq!(
            ScheduleSpec::custom("0 9 * * MON-FRI").to_cron().unwrap(),
            "0 9 * * MON-FRI"
        );
        for line in ["@every 1h", "@daily", "0 9 * *", "0 0 9 * * 1", "   "] {
            assert!(ScheduleSpec::custom(line).to_cron().is_err(), "{line:?}");
        }
        assert!(
            ScheduleSpec::custom("@every 1h")
                .to_cron()
                .unwrap_err()
                .sentence()
                .contains("five fields")
        );
    }

    /// Every schedule a tab draws comes back off its own line on the same tab, the same, which is
    /// what lets the editor open a routine the server sent on the tab and words it was chosen by.
    #[test]
    fn every_tab_reads_its_own_line_back() {
        let mut weekly = ScheduleSpec::advanced_daily(7, 45).on_tab(WakeTab::Weekly);
        weekly.weekdays = vec![0, 2, 4];
        weekly.months = vec![6, 7, 8];
        let mut monthly = ScheduleSpec::advanced_daily(23, 0).on_tab(WakeTab::Monthly);
        monthly.month_days = vec![1, 10, 31];
        for spec in [
            ScheduleSpec::interval(10, ScheduleUnit::Minutes),
            ScheduleSpec::interval(3, ScheduleUnit::Hours),
            ScheduleSpec::interval(2, ScheduleUnit::Days),
            ScheduleSpec::advanced_daily(9, 0),
            weekly,
            monthly,
            ScheduleSpec::from_preset("Weekdays"),
            ScheduleSpec::from_preset("Every week"),
            ScheduleSpec::from_preset("Every month"),
        ] {
            let line = spec.to_cron().unwrap();
            let back = ScheduleSpec::from_cron(&line);
            assert_eq!(back, spec, "`{line}` came back as something else");
            assert_eq!(back.tab(), spec.tab(), "`{line}`");
        }
    }

    /// A tab switched to keeps what it can: the time and the months carry over, the weekly tab
    /// starts on Monday to Friday and the monthly on the 1st, and the Cron tab on the line the
    /// schedule was.
    #[test]
    fn a_tab_switched_to_keeps_the_time_and_the_months() {
        let mut daily = ScheduleSpec::advanced_daily(18, 30);
        daily.months = vec![12];
        let weekly = daily.on_tab(WakeTab::Weekly);
        assert_eq!(weekly.tab(), WakeTab::Weekly);
        assert_eq!(weekly.label(), "Weekdays at 6:30 PM, in Dec");
        let monthly = weekly.on_tab(WakeTab::Monthly);
        assert_eq!(monthly.label(), "Monthly on the 1st at 6:30 PM, in Dec");
        let cron = monthly.on_tab(WakeTab::Cron);
        assert_eq!(cron.tab(), WakeTab::Cron);
        assert_eq!(cron.expr, "30 18 1 12 *");
        let every = cron.on_tab(WakeTab::Every);
        assert_eq!(every.label(), "Every 1 hour");
        assert_eq!(
            every.on_tab(WakeTab::Daily).label(),
            "Every day at 6:30 PM, in Dec"
        );
    }

    /// A line no tab draws is described field by field, the server's way.
    #[test]
    fn a_line_no_tab_draws_is_said_in_words_the_servers_way() {
        let words = |line: &str| describe_cron(line).unwrap();
        assert_eq!(words("0 9,17 * * *"), "At 9:00 AM and 5:00 PM, every day");
        assert_eq!(
            words("*/15 9-17 * * MON-FRI"),
            "Every 15 minutes from 9:00 AM to 5:45 PM, on weekdays"
        );
        assert_eq!(
            words("0 9 1 * MON"),
            "At 9:00 AM, on the 1st, when it is a Monday"
        );
        assert_eq!(
            words("0 9 1,15 * MON-FRI"),
            "At 9:00 AM, on the 1st and 15th, when it is a weekday"
        );
        assert_eq!(words("30 * * * *"), "At :30 past every hour, every day");
        assert_eq!(
            words("0 12 * JAN-MAR SAT"),
            "Every Saturday at 12:00 PM, in Jan to Mar"
        );
        assert_eq!(describe_cron("0 9 * * 0"), None, "the server refuses day 0");
        assert_eq!(describe_cron("@daily"), None, "not five fields");
    }

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

    /// Every interval the Every tab can write reads back as a schedule that saves the same line,
    /// for every step it allows and a few past them. (Every 1 day reads back as every day at
    /// 12:00 AM: the same schedule, drawn by the Daily tab.)
    #[test]
    fn every_interval_the_editor_writes_reads_back_the_same() {
        for (unit, ceiling) in [
            (ScheduleUnit::Minutes, 59),
            (ScheduleUnit::Hours, 23),
            (ScheduleUnit::Days, 31),
        ] {
            for every in 0..=ceiling + 10 {
                let spec = ScheduleSpec::interval(every, unit);
                match spec.to_cron() {
                    Ok(line) => {
                        assert!((1..=ceiling).contains(&every), "{every} {unit:?} -> {line}");
                        let again = ScheduleSpec::from_cron(&line).to_cron();
                        assert_eq!(again.as_deref(), Ok(line.as_str()), "{every} {unit:?}");
                    }
                    Err(_) => assert!(every == 0 || every > ceiling, "{every} {unit:?} refused"),
                }
            }
        }
    }

    /// Whatever step the server sends back, the editor can save it again: a step its field can
    /// hold is an interval, and one it cannot (`*/90` minutes, `*/48` hours) is the Cron tab's
    /// line, kept as written rather than read as an interval the editor would then refuse.
    #[test]
    fn every_stepped_line_from_the_server_saves_again() {
        for step in 1..=200u32 {
            for (line, ceiling) in [
                (format!("*/{step} * * * *"), 59),
                (format!("0 */{step} * * *"), 23),
                (format!("0 0 */{step} * *"), 31),
            ] {
                let read = ScheduleSpec::from_cron(&line);
                let saved = read.to_cron().unwrap_or_else(|e| {
                    panic!("{line} read back as a schedule that will not save: {e}")
                });
                if step > ceiling {
                    assert_eq!(read.mode, ScheduleUiMode::Custom, "{line}");
                    assert_eq!(saved, line);
                } else if step > 1 {
                    assert_eq!(saved, line);
                }
            }
        }
    }

    /// Two times of day are one line while they share the minute, and two routines when they
    /// do not: cron has one minute field for the whole line. Read back, two times are the Cron
    /// tab's, which says both.
    #[test]
    fn two_times_of_day_are_one_line_only_when_the_minute_agrees() {
        let mut spec = ScheduleSpec::advanced_daily(9, 0);
        spec.times = vec![(9, 0), (17, 0)];
        assert_eq!(spec.to_cron().unwrap(), "0 9,17 * * *");
        assert_eq!(spec.tab(), WakeTab::Cron);
        assert_eq!(ScheduleSpec::from_cron("0 9,17 * * *").tab(), WakeTab::Cron);

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

    /// A line with a shape the tabs cannot draw is kept, not approximated: two times, a date and
    /// a weekday together, steps the tabs do not offer, and lines that are not five fields.
    #[test]
    fn a_line_the_tabs_cannot_draw_stays_the_line() {
        for line in [
            "0 9,17 * * MON-FRI",
            "0 9 1 * MON",
            "*/15 9-17 * * *",
            "0 9 * *",
            "",
            "0 9 * * * *",
            "@daily",
        ] {
            let spec = ScheduleSpec::from_cron(line);
            assert_eq!(spec.mode, ScheduleUiMode::Custom, "{line}");
            assert_eq!(spec.expr, line.trim(), "{line}");
        }
    }

    /// An hourly line with an offset is not the Every tab's `every hour`, which fires on the
    /// hour: reading it back as one would move somebody's schedule by half an hour.
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
    /// line was read back into, or off the line itself where no tab draws it.
    #[test]
    fn a_line_read_back_is_labelled_the_way_it_was_chosen() {
        let label = |line: &str| ScheduleSpec::from_cron(line).label();
        assert_eq!(label("0 * * * *"), "Every 1 hour");
        assert_eq!(label("*/30 * * * *"), "Every 30 minutes");
        assert_eq!(label("0 9 * * *"), "Every day at 9:00 AM");
        assert_eq!(label("0 8 1 * *"), "Monthly on the 1st at 8:00 AM");
        assert_eq!(label("0 9,17 * * *"), "At 9:00 AM and 5:00 PM, every day");
        assert_eq!(
            label("@daily"),
            "@daily",
            "a line with no words for it is itself"
        );
    }
}
