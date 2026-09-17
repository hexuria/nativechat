use crate::actions::TtsSource;
use crate::audio::AudioInput;
use crate::chrome::{
    ResponsiveCollapse, SIDEBAR_EXPANDED, SidebarChrome, collapse_for_width, remember_choice,
    sidebar_from_resize,
};
use crate::config::Config;
use crate::opengrok::{
    Account, ActivityTick, AguiMessage, ApprovalSpec, BotActivity, ChatPart, ConnectedComputer,
    Coworker, CoworkerComputer, CoworkerPatch, FormSpec, LocalExecMode, LocalExecResolution,
    ModelCatalogue, OpenGrokClient, OpenGrokError, ProfileUpdate, QueuedApproval, RecipeDetail,
    RecipeRunResult, RecipeShareTarget, RecipeStep, RecipeSummary, ReplyQuote, ToolCallTracker,
    TurnAssembler, activity_from_replay, command_from_args, command_from_replay_events,
    deeds_from_replay, enrol_this_machine, local_exec_outcome, policy_answer, serve_local_exec,
    stored_machine_id, tool_standin, visible_bot_status,
};
use crate::services::database::{DatabaseService, MessagePart, ReplyRef};
use crate::services::tts_service::TtsService;
use chrono::{DateTime, Local, NaiveDateTime, Timelike};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub content: String,
    pub sent_at: SystemTime,
    pub is_me: bool,
    pub reply_preview: Option<String>,
    /// The message this one answers. The preview is what the bubble paints; this is what the
    /// quote sent to the coworker is built from, so a reply reaches it as more than a bubble.
    pub reply_to_id: Option<String>,
    /// The person wrote the quoted message, rather than the coworker.
    pub reply_is_me: bool,
    pub parts: Vec<ChatPart>,
}

impl Message {
    pub fn has_visible_body(&self) -> bool {
        !self.content.trim().is_empty()
            || self.parts.iter().any(|part| match part {
                ChatPart::Text(text) => !text.trim().is_empty(),
                ChatPart::Ui(_) | ChatPart::Approval(_) | ChatPart::Screenshot(_) => true,
            })
    }

    /// Words, as opposed to a card or a picture. A run that ends without any is a turn the
    /// transcript has to speak for: with the error, or with what the tools did.
    pub fn has_text_body(&self) -> bool {
        !self.content.trim().is_empty()
            || self.parts.iter().any(|part| match part {
                ChatPart::Text(text) => !text.trim().is_empty(),
                ChatPart::Ui(_) | ChatPart::Approval(_) | ChatPart::Screenshot(_) => false,
            })
    }

    /// Clock time on the message row, matching Grok's `12:14 PM` column.
    pub fn formatted_time(&self) -> String {
        let dt = DateTime::<Local>::from(self.sent_at);
        let (pm, hour) = dt.hour12();
        let hour = if hour == 0 { 12 } else { hour };
        format!(
            "{}:{:02} {}",
            hour,
            dt.minute(),
            if pm { "PM" } else { "AM" }
        )
    }
}

/// What is worth keeping of a message the person watched arrive: its words and the pictures of
/// the box's screen, in the order they appeared.
///
/// A turn that was only words keeps nothing here — `content` already holds them, and a second
/// copy would double every thread on disk. Cards are left out on purpose; see `MessagePart`.
/// Because a card is dropped, the words on either side of one are kept apart by a blank line,
/// the same break `content` gets, rather than running together into one sentence. A chart, which
/// is cut out of the middle of a sentence, leaves that sentence whole.
fn saved_parts(parts: &[ChatPart]) -> Vec<MessagePart> {
    let mut saved: Vec<MessagePart> = Vec::new();
    let mut words = String::new();
    for part in parts {
        match part {
            ChatPart::Text(text) => words.push_str(text),
            ChatPart::Screenshot(spec) => {
                close_text_run(&mut words, &mut saved);
                saved.push(MessagePart::Screenshot {
                    call_id: spec.call_id.clone(),
                    caption: spec.caption.clone(),
                    image: spec.image.bytes.clone(),
                    width: spec.width,
                    height: spec.height,
                });
            }
            ChatPart::Ui(_) => {}
            ChatPart::Approval(_) => break_paragraph(&mut words),
        }
    }
    close_text_run(&mut words, &mut saved);
    if saved
        .iter()
        .all(|part| matches!(part, MessagePart::Text(_)))
    {
        return Vec::new();
    }
    saved
}

/// The words so far become a bubble of their own. Whitespace is not a bubble, so a run of it is
/// dropped; the edges are trimmed because a run boundary is where one bubble ends and the next
/// begins, and a blank first line there is only noise.
fn close_text_run(words: &mut String, saved: &mut Vec<MessagePart>) {
    let text = std::mem::take(words);
    let text = text.trim();
    if !text.is_empty() {
        saved.push(MessagePart::Text(text.to_string()));
    }
}

fn break_paragraph(words: &mut String) {
    if words.trim().is_empty() || words.ends_with("\n\n") {
        return;
    }
    words.truncate(words.trim_end().len());
    words.push_str("\n\n");
}

/// A saved message as the feed draws it.
///
/// A row with no pieces — every row an older build wrote, and every turn that was only words —
/// is the single bubble its words already were, so old threads read exactly as they did.
fn restored_parts(content: &str, saved: Vec<MessagePart>) -> Vec<ChatPart> {
    if saved.is_empty() {
        if content.trim().is_empty() {
            return Vec::new();
        }
        return vec![ChatPart::Text(content.to_string())];
    }
    saved
        .into_iter()
        .map(|part| match part {
            MessagePart::Text(text) => ChatPart::Text(text),
            MessagePart::Screenshot {
                call_id,
                caption,
                image,
                width,
                height,
            } => ChatPart::Screenshot(crate::opengrok::ScreenshotSpec {
                call_id,
                caption,
                image: Arc::new(gpui_kit::Image::from_bytes(
                    gpui_kit::ImageFormat::Png,
                    image,
                )),
                width,
                height,
            }),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyTo {
    pub message_id: String,
    pub preview: String,
    pub is_me: bool,
}

/// What the feed says when a run came back with nothing at all.
pub const EMPTY_TURN_NOTE: &str = "(OpenGrok returned no assistant text.)";

/// How a run's failure is spelled in the feed.
pub const RUN_ERROR_PREFIX: &str = "OpenGrok: ";

/// How much of a quoted message the coworker is shown: a reply to a long answer names it, it
/// does not replay it. The server's `reply_context` caps the same way.
const REPLY_QUOTE_CHARS: usize = 600;

/// The app talking about a turn — the empty-turn note, a run's failure — rather than anything the
/// coworker said. These are painted as a status line, never saved and never sent back: a line the
/// app wrote is not a turn the coworker took, and the model would answer to it as if it were.
///
/// The test is the content itself so that rows an older build saved are read the same way.
pub fn is_status_line(content: &str) -> bool {
    let text = content.trim();
    text == EMPTY_TURN_NOTE || text.starts_with(RUN_ERROR_PREFIX)
}

/// The stand-in a turn that acted but said nothing leaves behind, e.g.
/// "[took a screenshot of my screen]". It is the coworker's own content — saved, and sent on
/// later turns so it remembers what it did — but it is not speech, so the feed dims it.
pub fn is_tool_standin(content: &str) -> bool {
    let text = content.trim();
    text.starts_with('[') && text.ends_with(']') && !text.contains('\n')
}

/// The bracketed line the coworker reads ahead of a reply's own words, in the sentence the
/// server's `reply_context` already writes, so a reply reads the same whichever path carried it.
fn reply_quote_line(quote: &ReplyQuote) -> String {
    let who = if quote.is_me {
        "their own earlier message"
    } else {
        "your earlier message"
    };
    format!("[Replying to {who}: \"{}\"]", quote.preview)
}

/// The quote a message carries: the words of the message it answers when that one is still in the
/// thread — all of them, not the bubble's short preview — and the preview saved with the reply
/// when it is not.
fn reply_quote(messages: &[Message], message: &Message) -> Option<ReplyQuote> {
    let message_id = message.reply_to_id.clone()?;
    let quoted = messages.iter().find(|m| m.id == message_id);
    let text = match quoted {
        Some(quoted) => quoted.content.clone(),
        None => message.reply_preview.clone()?,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let preview = if text.chars().count() > REPLY_QUOTE_CHARS {
        let head: String = text.chars().take(REPLY_QUOTE_CHARS).collect();
        format!("{head}…")
    } else {
        text.to_string()
    };
    Some(ReplyQuote {
        message_id,
        preview,
        is_me: quoted.map_or(message.reply_is_me, |quoted| quoted.is_me),
    })
}

/// The thread as the coworker should see it.
///
/// The app's own status lines are left out, and a reply carries its quote twice: in `content`,
/// because that is all today's server reads, and in `replyTo` for a server that would rather
/// find the quoted message and word the context itself.
pub fn agui_messages(messages: &[Message]) -> Vec<AguiMessage> {
    messages
        .iter()
        .filter(|m| m.is_me || (!m.content.trim().is_empty() && !is_status_line(&m.content)))
        .map(|m| {
            let reply_to = reply_quote(messages, m);
            AguiMessage {
                id: m.id.clone(),
                role: if m.is_me { "user" } else { "assistant" }.to_string(),
                content: match &reply_to {
                    Some(quote) => format!("{}\n\n{}", reply_quote_line(quote), m.content),
                    None => m.content.clone(),
                },
                tool_call_id: None,
                reply_to,
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct EmojiPickerOpen {
    pub message_id: String,
    pub bounds: Bounds<Pixels>,
}

#[derive(Clone, Debug)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<Message>,
    pub unread_count: usize,
}

impl Conversation {
    pub fn relative_time(&self) -> String {
        let now = SystemTime::now();

        // Parse the ISO 8601 string or fallback to now
        let created_at = NaiveDateTime::parse_from_str(&self.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| SystemTime::from(dt.and_utc()))
            .unwrap_or(SystemTime::now());

        let duration = now.duration_since(created_at).unwrap_or_default();
        let secs = duration.as_secs();

        if secs < 60 {
            "Just now".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("{}m ago", mins)
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!("{}h ago", hours)
        } else if secs < 604800 {
            let days = secs / 86400;
            format!("{}d ago", days)
        } else if secs < 2592000 {
            let weeks = secs / 604800;
            format!("{}w ago", weeks)
        } else if secs < 31536000 {
            let months = secs / 2592000;
            format!("{}mo ago", months)
        } else {
            let years = secs / 31536000;
            format!("{}y ago", years)
        }
    }
}

fn parse_sql_time(value: &str) -> Option<SystemTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| SystemTime::from(dt.and_utc()))
}

fn system_time_ms(at: SystemTime) -> u128 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum VoiceStatus {
    Ready,
    Connecting,
    Connected,
    Disconnected,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SubmitChord {
    /// Enter sends; Shift+Enter inserts a newline.
    #[default]
    Enter,
    /// ⌘Enter sends; Enter inserts a newline.
    CommandEnter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RightPane {
    #[default]
    Closed,
    Settings,
    Computer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputerView {
    Overview,
    Editor { id: Option<String> },
}

impl Default for ComputerView {
    fn default() -> Self {
        Self::Overview
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRoutine {
    pub id: String,
    pub name: String,
    pub instruction: String,
    pub active: bool,
    pub triggers: Vec<RoutineTrigger>,
    pub runs: Vec<RoutineRun>,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutineTrigger {
    Schedule {
        id: String,
        spec: ScheduleSpec,
    },
    Event {
        id: String,
        kind: &'static str,
        label: String,
    },
    Webhook {
        id: String,
        url: String,
        key: String,
        header: String,
    },
}

impl RoutineTrigger {
    pub fn id(&self) -> &str {
        match self {
            Self::Schedule { id, .. } | Self::Event { id, .. } | Self::Webhook { id, .. } => id,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Schedule { spec, .. } => spec.label(),
            Self::Event { label, .. } => label.clone(),
            Self::Webhook { .. } => "When a webhook fires".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutineRun {
    pub at: String,
    pub ok: bool,
}

/// What the confirm dialog over the app is asking about the active bot's computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputerAction {
    /// Rebuild on the newest image; files and logins stay.
    Update,
    /// Start fresh; everything on it is lost.
    Reset,
}

/// What fills the main slot beside the sidebar: the chat, or a page reached from the dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MainPage {
    #[default]
    Chat,
    Recipes,
}

/// Which recipes the list asks the server for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RecipeFilter {
    #[default]
    Mine,
    Shared,
    Org,
}

impl RecipeFilter {
    pub const ALL: [Self; 3] = [Self::Mine, Self::Shared, Self::Org];

    /// The `?filter=` word.
    pub fn query(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Shared => "shared",
            Self::Org => "org",
        }
    }

    pub fn from_query(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|filter| filter.query() == word)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mine => "Mine",
            Self::Shared => "Shared with me",
            Self::Org => "Org",
        }
    }

    /// The chip's element id.
    pub fn element_id(self) -> &'static str {
        match self {
            Self::Mine => "recipes-filter-mine",
            Self::Shared => "recipes-filter-shared",
            Self::Org => "recipes-filter-org",
        }
    }
}

/// What a recipe's newest run came to, as far as this session has been told. A detail carries
/// a recipe's runs and the list's summaries carry none, so a row says what the app has already
/// been shown and nothing where it has not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeRunNote {
    pub ok: bool,
    pub version: u32,
    /// The step the run stopped at, when it did not finish.
    pub stopped_at: Option<u64>,
    pub at_ms: i64,
}

impl RecipeRunNote {
    /// "last run ok · v3", "last run stopped at step 7 · v3".
    pub fn label(&self) -> String {
        let version = self.version;
        if self.ok {
            format!("last run ok · v{version}")
        } else {
            match self.stopped_at {
                Some(step) => format!("last run stopped at step {step} · v{version}"),
                None => format!("last run stopped · v{version}"),
            }
        }
    }
}

/// What the last Run on… came back with, decoded for the page.
#[derive(Clone)]
pub struct RecipeRunOutcome {
    pub coworker_id: String,
    pub version: u32,
    pub ok: bool,
    pub ran: Option<u64>,
    pub stopped_at: Option<u64>,
    pub error: Option<String>,
    /// The screen after the run, with its size.
    pub image: Option<(Arc<gpui_kit::Image>, u32, u32)>,
}

impl RecipeRunOutcome {
    fn from_result(coworker_id: String, result: RecipeRunResult) -> Self {
        let image = result
            .image
            .as_ref()
            .and_then(|image| crate::opengrok::ScreenshotSpec::from_frame("recipe-run", "", image))
            .map(|spec| (spec.image, spec.width, spec.height));
        Self {
            coworker_id,
            version: result.version,
            ok: result.ok,
            ran: result.ran_count(),
            stopped_at: result.stopped_at,
            error: result.error,
            image,
        }
    }

    /// The one line the page shows for the outcome.
    pub fn headline(&self) -> String {
        let steps = match self.ran {
            Some(1) => "1 step".to_string(),
            Some(count) => format!("{count} steps"),
            None => "the steps".to_string(),
        };
        if self.ok {
            return format!("Ran {steps} of v{}", self.version);
        }
        let stopped = match self.stopped_at {
            Some(step) => format!("Stopped at step {step}"),
            None => "Stopped".to_string(),
        };
        match &self.error {
            Some(error) => format!("{stopped}: {error}"),
            None => stopped,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppSettingsTab {
    #[default]
    General,
    Profile,
    Appearance,
    Shortcuts,
    Computer,
    Updates,
}

/// One frame of in-app navigation. GPUI has no browser history; we keep this stack
/// so ⌘[ / ⌘] can walk agents, the right pane, and Settings the way macOS apps do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavLocation {
    pub coworker_id: Option<String>,
    pub page: MainPage,
    pub right_pane: RightPane,
    pub computer_view: ComputerView,
    pub app_settings_open: bool,
    pub app_settings_tab: AppSettingsTab,
}

impl NavLocation {
    fn is_blank(&self) -> bool {
        self.coworker_id.is_none()
            && self.right_pane == RightPane::Closed
            && !self.app_settings_open
    }
}

#[derive(Clone, Debug, Default)]
pub struct NavHistory {
    back: Vec<NavLocation>,
    forward: Vec<NavLocation>,
    current: Option<NavLocation>,
    applying: bool,
}

impl NavHistory {
    fn record(&mut self, loc: NavLocation) {
        if self.applying {
            return;
        }
        match &self.current {
            Some(cur) if cur == &loc => {}
            Some(cur) if cur.is_blank() => self.current = Some(loc),
            Some(cur) => {
                self.back.push(cur.clone());
                self.forward.clear();
                self.current = Some(loc);
            }
            None => self.current = Some(loc),
        }
    }

    fn go_back(&mut self) -> Option<NavLocation> {
        let prev = self.back.pop()?;
        if let Some(cur) = self.current.take() {
            self.forward.push(cur);
        }
        self.current = Some(prev.clone());
        Some(prev)
    }

    fn go_forward(&mut self) -> Option<NavLocation> {
        let next = self.forward.pop()?;
        if let Some(cur) = self.current.take() {
            self.back.push(cur);
        }
        self.current = Some(next.clone());
        Some(next)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AuthStatus {
    #[default]
    SignedOut,
    SigningIn,
    SignedIn,
}

pub struct AppState {
    pub conversations: Vec<Conversation>,
    /// Last send/receive per coworker. Beats an unopened session's empty `messages`.
    pub last_active_at: HashMap<String, SystemTime>,
    pub active_conversation_id: Option<String>,
    pub theme_mode: String,
    pub amplitude: Arc<AtomicU32>,
    pub ai_amplitude: Arc<AtomicU32>, // New field for AI voice viz
    pub is_ai_speaking: Arc<AtomicBool>,
    pub is_voice_mode_open: bool,
    pub is_sidebar_open: bool,
    pub is_voice_muted: bool,
    pub voice_status: VoiceStatus,
    pub more_menu_open: bool,
    /// The composer's "+" picker: what it can offer, what is picked, and what each entry is.
    /// Tools named for the next message. They show as chips beside the composer's "+".
    pub picked_tools: Vec<PickedTool>,
    pub is_app_settings_open: bool,
    pub bot_finder_open: bool,
    pub command_palette_open: bool,
    nav: NavHistory,
    pub app_settings_tab: AppSettingsTab,
    pub submit_chord: SubmitChord,
    pub audio_input: Option<AudioInput>,
    pub sidebar_collapsed: bool,
    pub sidebar_hidden: bool,
    pub sidebar_expanded_width: f32,
    pub sidebar_responsive: ResponsiveCollapse,
    pub auto_collapsed: bool,
    pub database_service: Option<DatabaseService>,
    pub config: Option<Config>,
    pub is_ai_responding: bool,
    pub debug_markdown_disabled: bool,
    pub tts_service: Option<TtsService>,
    tts_initing: bool,
    pending_read_aloud: Option<(String, String)>,
    pub native_tts: SourceTtsState,
    pub opengrok: Option<OpenGrokClient>,
    pub account: Option<Account>,
    pub auth_status: AuthStatus,
    pub auth_error: Option<String>,
    login_epoch: u64,
    pub login_email: String,
    pub login_password: String,
    pub coworkers: Vec<Coworker>,
    pub active_coworker_id: Option<String>,
    pub bot_status: Option<String>,
    /// Coworker whose turn owns `bot_status` / `is_ai_responding`.
    responding_coworker_id: Option<String>,
    pub model_catalogue: ModelCatalogue,
    pub right_pane: RightPane,
    pub computer_view: ComputerView,
    pub routines: HashMap<String, Vec<AgentRoutine>>,
    pub model_picker_open: bool,
    pub avatar_editor_open: bool,
    pub hiring: bool,
    pub pinned_coworker_ids: HashSet<String>,
    pub hidden_coworker_ids: HashSet<String>,
    pub renaming_coworker_id: Option<String>,
    pub hidden_bots_open: bool,
    pub reply_to: Option<ReplyTo>,
    pub message_reactions: HashMap<String, String>,
    pub emoji_picker: Option<EmojiPickerOpen>,
    pub form_picks: HashMap<String, HashMap<String, String>>,
    pub approval_decisions: HashMap<String, ApprovalDecision>,
    pub local_exec_machine_id: Option<String>,
    local_exec_cancel: Option<Arc<AtomicBool>>,
    pub expanded_shell_output: HashSet<String>,
    pub computers: Vec<ConnectedComputer>,
    /// The active coworker's computer, as last polled. Cleared on a switch so a
    /// bot never shows the previous one's screen.
    pub coworker_computer: Option<CoworkerComputer>,
    /// This server answered 404 to `/coworkers/{id}/computer`: it has no such
    /// endpoint, so polling stops until the roster reloads.
    pub computer_endpoint_missing: bool,
    /// The active coworker's screen as last fetched, painted in the Computer
    /// pane's tile. Polled with the status, only while the box has a screen.
    pub coworker_screen: Option<std::sync::Arc<gpui_kit::Image>>,
    /// Runs while the Computer pane is open; dropped when it closes.
    computer_poll: Option<Task<()>>,
    /// One screen window per coworker: Open brings the existing one forward rather than
    /// stacking another.
    #[cfg(target_os = "macos")]
    computer_windows: std::collections::HashMap<
        String,
        WindowHandle<crate::components::computer_screen::ComputerScreen>,
    >,
    /// Update / Reset ask first: the dialog over the app, until Confirm or Cancel.
    pub computer_confirm: Option<ComputerAction>,
    /// What the last Update / Reset request said when it was refused; shown under the buttons.
    pub computer_action_error: Option<String>,
    /// The coworker whose absent computer we already asked the server to (re)provision, so a
    /// status of `absent` heals once per visit rather than on every poll.
    computer_heal_requested: Option<String>,
    /// What the main slot shows: the chat, or the Recipes page.
    pub page: MainPage,
    pub recipes: Vec<RecipeSummary>,
    pub recipes_filter: RecipeFilter,
    pub recipes_loading: bool,
    pub recipes_error: Option<String>,
    /// Bumped per list request, so a late answer for an earlier filter is dropped.
    recipes_epoch: u64,
    /// The recipe the detail view shows, once it has loaded.
    pub recipe_open: Option<RecipeDetail>,
    /// The recipe the detail view is on, from the moment it is asked for; a late answer for
    /// another one is dropped.
    pub recipe_open_id: Option<String>,
    pub recipe_loading: bool,
    /// What the detail view is doing right now ("Saving…", "Running…"), while it does it.
    pub recipe_busy: Option<String>,
    /// What the last recipe request said when it was refused.
    pub recipe_error: Option<String>,
    pub recipe_run_result: Option<RecipeRunOutcome>,
    /// Delete asks first: the dialog over the app, until Delete or Cancel.
    pub recipe_delete_confirm: bool,
    /// What each recipe's newest run came to, kept as details are read, so a row in the list
    /// can say what became of that recipe last time.
    pub recipe_last_runs: HashMap<String, RecipeRunNote>,
    /// The window the app's pages live in. A second window — a coworker's screen — has no
    /// page of its own: it asks this one to show the Recipes page and brings it forward.
    main_window: Option<AnyWindowHandle>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Pending,
    Sending,
    AllowOnce,
    Always,
    Denied,
    Never,
    Failed(String),
}

impl ApprovalDecision {
    pub fn is_settled(&self) -> bool {
        !matches!(self, Self::Pending | Self::Sending)
    }

    /// The person (or the policy) has answered; a click still in flight counts.
    pub fn is_answered(&self) -> bool {
        !matches!(self, Self::Pending)
    }

    /// `place` is [`ApprovalSpec::place`].
    pub fn outcome_line(&self, bot: &str, place: &str) -> Option<String> {
        let resolution = match self {
            Self::AllowOnce => LocalExecResolution::AllowOnce,
            Self::Always => LocalExecResolution::Always,
            Self::Denied => LocalExecResolution::DenyOnce,
            Self::Never => LocalExecResolution::Never,
            _ => return None,
        };
        Some(local_exec_outcome(bot, resolution, place))
    }
}

#[derive(Clone, Debug, Default)]
pub struct SourceTtsState {
    pub message_id: Option<String>,
    pub is_paused: bool,
    pub is_loading: bool,
}

/// A tool the person named for the next message, by typing `@` in the composer.
///
/// The kind travels with it. The chip row used to decide Tools from Apps by matching the name
/// against a hardcoded list, which only worked while the names came from one hardcoded menu;
/// a real tool's name comes from the server and matches nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedTool {
    /// What the server calls it: `shell`, or a plugin's qualified `plugin.server.tool`.
    pub id: String,
    /// What the chip reads.
    pub label: String,
    pub kind: PickedKind,
}

/// Which group a chip sits in. A bare name is one of the server's built-in tools; a qualified
/// one belongs to a plugin, which is what the person means by an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickedKind {
    Tool,
    App,
}

impl PickedKind {
    /// Read the kind off the name, the same rule the server's tool listing uses.
    pub fn of(id: &str) -> Self {
        if id.contains('.') {
            Self::App
        } else {
            Self::Tool
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// What a caller hears once a patch is over: nothing when the server took it, the server's
/// message when it refused.
pub type PatchDone = Box<dyn FnOnce(Option<String>, &mut App)>;

impl AppState {
    pub fn new() -> Self {
        let mut state = Self {
            conversations: Vec::new(),
            last_active_at: HashMap::new(),
            active_conversation_id: None,
            theme_mode: "light".to_string(),
            amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            ai_amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            is_ai_speaking: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            is_voice_mode_open: false,
            is_voice_muted: false,
            is_sidebar_open: true,
            voice_status: VoiceStatus::Ready,
            more_menu_open: false,
            picked_tools: Vec::new(),
            is_app_settings_open: false,
            bot_finder_open: false,
            command_palette_open: false,
            nav: NavHistory::default(),
            app_settings_tab: AppSettingsTab::General,
            submit_chord: SubmitChord::Enter,
            audio_input: None,
            sidebar_collapsed: false,
            sidebar_hidden: false,
            sidebar_expanded_width: SIDEBAR_EXPANDED,
            sidebar_responsive: ResponsiveCollapse::default(),
            auto_collapsed: false,
            database_service: None,
            config: None,
            is_ai_responding: false,
            debug_markdown_disabled: false,
            tts_service: None,
            tts_initing: false,
            pending_read_aloud: None,
            native_tts: SourceTtsState::default(),
            opengrok: None,
            account: None,
            auth_status: AuthStatus::SignedOut,
            auth_error: None,
            login_epoch: 0,
            login_email: String::new(),
            login_password: String::new(),
            coworkers: Vec::new(),
            active_coworker_id: None,
            bot_status: None,
            responding_coworker_id: None,
            model_catalogue: ModelCatalogue::default(),
            right_pane: RightPane::Closed,
            computer_view: ComputerView::Overview,
            routines: HashMap::new(),
            model_picker_open: false,
            avatar_editor_open: false,
            hiring: false,
            pinned_coworker_ids: HashSet::new(),
            hidden_coworker_ids: HashSet::new(),
            renaming_coworker_id: None,
            hidden_bots_open: false,
            reply_to: None,
            message_reactions: HashMap::new(),
            emoji_picker: None,
            form_picks: HashMap::new(),
            approval_decisions: HashMap::new(),
            local_exec_machine_id: None,
            local_exec_cancel: None,
            expanded_shell_output: HashSet::new(),
            computers: Vec::new(),
            coworker_computer: None,
            coworker_screen: None,
            computer_confirm: None,
            computer_action_error: None,
            computer_heal_requested: None,
            computer_endpoint_missing: false,
            computer_poll: None,
            page: MainPage::Chat,
            recipes: Vec::new(),
            recipes_filter: RecipeFilter::Mine,
            recipes_loading: false,
            recipes_error: None,
            recipes_epoch: 0,
            recipe_open: None,
            recipe_open_id: None,
            recipe_loading: false,
            recipe_busy: None,
            recipe_error: None,
            recipe_run_result: None,
            recipe_delete_confirm: false,
            recipe_last_runs: HashMap::new(),
            main_window: None,
            #[cfg(target_os = "macos")]
            computer_windows: std::collections::HashMap::new(),
        };

        state
    }

    pub fn set_config(&mut self, config: Config, cx: &mut Context<Self>) {
        match OpenGrokClient::new(&config.opengrok_base_url) {
            Ok(client) => {
                let client =
                    client.with_session_file(config.data_dir.join("opengrok-session.json"));
                self.opengrok = Some(client);
                self.restore_session(cx);
            }
            Err(error) => {
                self.auth_error = Some(error.message);
                self.opengrok = None;
            }
        }
        self.config = Some(config);
        cx.notify();
    }

    fn restore_session(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if !client.load_session() {
            return;
        }
        self.login_epoch += 1;
        let epoch = self.login_epoch;
        self.auth_status = AuthStatus::SigningIn;
        self.auth_error = None;
        cx.spawn(async move |this, cx| {
            let account = match client.me().await {
                Ok(account) => Ok(account),
                Err(error) if error.is_unauthorized() => match client.refresh().await {
                    Ok(()) => client.me().await,
                    Err(_) => {
                        client.clear_session();
                        Err(error)
                    }
                },
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                if state.login_epoch != epoch {
                    return;
                }
                match account {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_status = AuthStatus::SignedIn;
                        state.auth_error = None;
                        state.start_local_exec(cx);
                        state.refresh_coworkers(cx);
                        state.refresh_computers(cx);
                        state.sync_pending_approvals(cx);
                    }
                    Err(_) => {
                        state.account = None;
                        state.auth_status = AuthStatus::SignedOut;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn is_signed_in(&self) -> bool {
        self.auth_status == AuthStatus::SignedIn && self.account.is_some()
    }

    pub fn visible_bot_status(&self) -> Option<String> {
        visible_bot_status(
            self.active_coworker_id.as_deref(),
            self.responding_coworker_id.as_deref(),
            self.bot_status.as_deref(),
        )
    }

    /// The selected coworker's name, for copy that addresses it.
    pub fn active_bot_name(&self) -> String {
        self.active_coworker_id
            .as_ref()
            .and_then(|id| self.coworkers.iter().find(|c| &c.id == id))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "this agent".to_string())
    }

    pub fn is_active_bot_responding(&self) -> bool {
        self.is_ai_responding
            && self.active_coworker_id.is_some()
            && self.active_coworker_id == self.responding_coworker_id
    }

    fn begin_responding(&mut self, coworker_id: Option<&str>, status: &str) {
        self.responding_coworker_id = coworker_id.map(str::to_string);
        self.is_ai_responding = true;
        self.bot_status = Some(status.to_string());
    }

    fn apply_turn_status(&mut self, coworker_id: Option<&str>, tick: ActivityTick) {
        if coworker_id.is_some() && coworker_id != self.responding_coworker_id.as_deref() {
            return;
        }
        match tick {
            ActivityTick::Keep => {}
            ActivityTick::Clear => self.bot_status = None,
            ActivityTick::Set(activity) => self.bot_status = Some(activity.label),
        }
    }

    fn this_machine_mode(&self) -> Option<LocalExecMode> {
        self.computers
            .iter()
            .find(|computer| {
                computer.this_machine
                    || self.local_exec_machine_id.as_ref() == Some(&computer.machine_id)
            })
            .map(|computer| computer.mode)
    }

    /// What this Mac's policy answers for `spec` without a card, if anything.
    fn auto_resolve_local_exec(&self, spec: &ApprovalSpec) -> Option<LocalExecResolution> {
        policy_answer(spec, self.this_machine_mode())
    }

    pub fn approval_answered(&self, call_id: &str) -> bool {
        self.approval_decisions
            .get(call_id)
            .is_some_and(ApprovalDecision::is_answered)
    }

    /// Cards in the open conversation still waiting on the person.
    pub fn open_approvals(&self) -> Vec<ApprovalSpec> {
        self.active_conversation_id
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
            .into_iter()
            .flat_map(|c| c.messages.iter().flat_map(|m| m.parts.iter()))
            .filter_map(|part| match part {
                ChatPart::Approval(spec) if !self.approval_answered(&spec.call_id) => {
                    Some(spec.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// Answer a card by id. False when no such card is open.
    pub fn answer_approval_by_id(
        &mut self,
        call_id: &str,
        resolution: LocalExecResolution,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(spec) = self
            .open_approvals()
            .into_iter()
            .find(|spec| spec.call_id == call_id)
        else {
            return false;
        };
        self.answer_approval(spec, resolution, cx);
        true
    }

    pub fn approval_status_line(&self, spec: &ApprovalSpec, bot: &str) -> Option<String> {
        if let Some(line) = self
            .approval_decisions
            .get(&spec.call_id)
            .and_then(|decision| decision.outcome_line(bot, spec.place()))
        {
            return Some(line);
        }
        self.auto_resolve_local_exec(spec)
            .map(|resolution| local_exec_outcome(bot, resolution, spec.place()))
    }

    fn finish_responding(&mut self, coworker_id: Option<&str>, waiting_approval: bool) {
        if coworker_id.is_some()
            && self.responding_coworker_id.is_some()
            && coworker_id != self.responding_coworker_id.as_deref()
        {
            return;
        }
        self.is_ai_responding = false;
        if waiting_approval {
            self.bot_status = Some("Waiting for approval".into());
        } else {
            self.bot_status = None;
            self.responding_coworker_id = None;
        }
    }

    pub fn login(&mut self, email: String, password: String, cx: &mut Context<Self>) {
        if self.auth_status == AuthStatus::SigningIn {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        self.login_epoch += 1;
        let epoch = self.login_epoch;
        self.auth_status = AuthStatus::SigningIn;
        self.auth_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match client.login(&email, &password).await {
                Ok(()) => client.me().await,
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                if state.login_epoch != epoch {
                    return;
                }
                match result {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_status = AuthStatus::SignedIn;
                        state.auth_error = None;
                        state.start_local_exec(cx);
                        state.refresh_coworkers(cx);
                        state.refresh_computers(cx);
                        state.sync_pending_approvals(cx);
                    }
                    Err(error) => {
                        state.account = None;
                        state.auth_status = AuthStatus::SignedOut;
                        state.auth_error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn logout(&mut self, cx: &mut Context<Self>) {
        let client = self.opengrok.clone();
        self.account = None;
        self.auth_status = AuthStatus::SignedOut;
        self.auth_error = None;
        self.coworkers.clear();
        self.last_active_at.clear();
        self.active_coworker_id = None;
        self.bot_status = None;
        self.responding_coworker_id = None;
        self.is_ai_responding = false;
        self.is_app_settings_open = false;
        self.bot_finder_open = false;
        self.command_palette_open = false;
        self.close_right_pane(cx);
        self.stop_local_exec();
        self.approval_decisions.clear();
        self.computers.clear();
        cx.notify();
        if let Some(client) = client {
            cx.spawn(async move |_, _| {
                let _ = client.logout().await;
            })
            .detach();
        }
    }

    pub fn refresh_coworkers(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.list_coworkers().await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(list) => {
                        state.coworkers = list;
                        state.hidden_coworker_ids = state
                            .coworkers
                            .iter()
                            .filter(|c| c.hidden_from_sidebar)
                            .map(|c| c.id.clone())
                            .collect();
                        if state
                            .active_coworker_id
                            .as_ref()
                            .is_none_or(|id| !state.coworkers.iter().any(|c| &c.id == id))
                        {
                            if let Some(first) = state
                                .ranked_coworkers()
                                .into_iter()
                                .find(|c| !state.hidden_coworker_ids.contains(&c.id))
                            {
                                state.select_coworker(first.id, cx);
                            }
                        }
                    }
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        self.refresh_models(cx);
    }

    pub fn refresh_models(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.list_models().await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(catalogue) => state.model_catalogue = catalogue,
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn is_right_pane_open(&self) -> bool {
        self.right_pane != RightPane::Closed
    }

    pub fn is_agent_settings_open(&self) -> bool {
        self.right_pane == RightPane::Settings
    }

    /// How often the open Computer pane asks after the coworker's box.
    const COMPUTER_POLL: Duration = Duration::from_secs(2);

    /// Every change of the right pane goes through here, so the Computer poll
    /// runs exactly while that pane is open.
    fn set_right_pane(&mut self, pane: RightPane, cx: &mut Context<Self>) {
        let computer = pane == RightPane::Computer;
        self.right_pane = pane;
        self.computer_confirm = None;
        self.computer_action_error = None;
        if computer {
            self.refresh_coworker_computer(cx);
            self.start_computer_poll(cx);
        } else {
            self.computer_poll = None;
        }
    }

    pub fn close_right_pane(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Closed {
            return;
        }
        self.set_right_pane(RightPane::Closed, cx);
        self.computer_view = ComputerView::Overview;
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.record_nav();
        cx.notify();
    }

    pub fn show_agent_settings(&mut self, cx: &mut Context<Self>) {
        self.ensure_active_coworker(cx);
        self.set_right_pane(RightPane::Settings, cx);
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn show_computer_pane(&mut self, cx: &mut Context<Self>) {
        self.ensure_active_coworker(cx);
        self.set_right_pane(RightPane::Computer, cx);
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_agent_settings(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Settings {
            self.close_right_pane(cx);
            return;
        }
        self.set_right_pane(RightPane::Settings, cx);
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_computer_pane(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Computer {
            self.close_right_pane(cx);
            return;
        }
        self.set_right_pane(RightPane::Computer, cx);
        self.computer_view = ComputerView::Overview;
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.record_nav();
        cx.notify();
    }

    /// Keep the Computer pane's tile current while it is open. Idempotent; the
    /// task is dropped by `set_right_pane` when the pane closes.
    fn start_computer_poll(&mut self, cx: &mut Context<Self>) {
        if self.computer_poll.is_some() {
            return;
        }
        self.computer_poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Self::COMPUTER_POLL).await;
                let alive = this.update(cx, |state, cx| {
                    if state.right_pane == RightPane::Computer {
                        state.refresh_coworker_computer(cx);
                        state.refresh_coworker_screen(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    pub fn refresh_coworker_computer(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            self.coworker_computer = None;
            self.coworker_screen = None;
            return;
        };
        if self.computer_endpoint_missing {
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = client.coworker_computer(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                // A late answer for a bot the person has since left is stale.
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(status) => {
                        // A bot with no computer gets one: ask once per visit, and let the
                        // next poll pick up the answer. A recorded error is the server saying
                        // it cannot, so that is left alone.
                        let absent = status.state == "absent" && !status.updating();
                        if state.coworker_computer.as_ref() != Some(&status) {
                            state.coworker_computer = Some(status);
                            cx.notify();
                        }
                        if absent
                            && state.computer_heal_requested.as_deref()
                                != Some(coworker_id.as_str())
                        {
                            state.computer_heal_requested = Some(coworker_id.clone());
                            state.ensure_coworker_computer(cx);
                        }
                    }
                    Err(error) if error.status == Some(404) => {
                        eprintln!(
                            "NativeChat computer: this server has no /coworkers/{{id}}/computer; not polling"
                        );
                        state.computer_endpoint_missing = true;
                        state.computer_poll = None;
                    }
                    Err(error) => {
                        // Say it once, when the status is lost, not every two seconds.
                        if state.coworker_computer.take().is_some() {
                            eprintln!("NativeChat computer: {}", error.message);
                            cx.notify();
                        }
                    }
                }
            });
        })
        .detach();
    }

    /// Fetch the screen for the tile. Only while the status says there is one — a headless or
    /// stopped box is not asked, and a 404 (no screen after all) just clears the picture.
    pub fn refresh_coworker_screen(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        let has_screen = self
            .coworker_computer
            .as_ref()
            .and_then(|status| status.vnc_url())
            .is_some();
        if !has_screen {
            if self.coworker_screen.take().is_some() {
                cx.notify();
            }
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = client.coworker_screen(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(frame) => {
                        state.coworker_screen =
                            crate::opengrok::ScreenshotSpec::from_frame("screen", "", &frame)
                                .map(|spec| spec.image);
                        cx.notify();
                    }
                    Err(_) => {
                        if state.coworker_screen.take().is_some() {
                            cx.notify();
                        }
                    }
                }
            });
        })
        .detach();
    }

    /// Ask the server to (re)provision the active coworker's computer, and take the answer as
    /// the current status.
    pub fn ensure_coworker_computer(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.ensure_coworker_computer(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(status) => {
                        state.coworker_computer = Some(status);
                        cx.notify();
                    }
                    Err(error) => eprintln!(
                        "NativeChat computer: could not provision the computer of {coworker_id}: {}",
                        error.message
                    ),
                }
            });
        })
        .detach();
    }

    /// Ask before acting on the active bot's computer: the dialog over the app.
    pub fn open_computer_confirm(&mut self, action: ComputerAction, cx: &mut Context<Self>) {
        if self.active_coworker_id.is_none() {
            return;
        }
        self.computer_action_error = None;
        self.computer_confirm = Some(action);
        cx.notify();
    }

    pub fn close_computer_confirm(&mut self, cx: &mut Context<Self>) {
        if self.computer_confirm.take().is_some() {
            cx.notify();
        }
    }

    /// The dialog's Confirm: do what it asked, then close it.
    pub fn confirm_computer_action(&mut self, cx: &mut Context<Self>) {
        let Some(action) = self.computer_confirm.take() else {
            return;
        };
        match action {
            ComputerAction::Update => self.start_computer_update(cx),
            ComputerAction::Reset => self.start_computer_reset(cx),
        }
        cx.notify();
    }

    fn start_computer_update(&mut self, cx: &mut Context<Self>) {
        self.computer_action(cx, |client, id| {
            Box::pin(async move { client.update_coworker_computer(&id).await })
        });
    }

    fn start_computer_reset(&mut self, cx: &mut Context<Self>) {
        self.computer_action(cx, |client, id| {
            Box::pin(async move { client.reset_coworker_computer(&id).await })
        });
    }

    /// Run one computer action for the active coworker and take its answer as the status; a
    /// refusal is shown under the buttons. The poll carries the phases after that.
    fn computer_action(
        &mut self,
        cx: &mut Context<Self>,
        action: impl FnOnce(
            OpenGrokClient,
            String,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<CoworkerComputer, OpenGrokError>> + Send>,
        > + 'static,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        let future = action(client, coworker_id.clone());
        cx.spawn(async move |this, cx| {
            let result = future.await;
            let _ = this.update(cx, |state, cx| {
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(status) => {
                        state.coworker_computer = Some(status);
                        state.coworker_screen = None;
                    }
                    Err(error) => state.computer_action_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The banner over the app while the active coworker's computer is being updated: the
    /// title and what is happening now. `None` when nothing is.
    pub fn computer_banner(&self) -> Option<(String, String)> {
        let update = self.coworker_computer.as_ref()?.update.as_ref()?;
        let name = self.active_bot_name();
        if update.in_flight() {
            Some((format!("Updating {name}'s computer"), update.detail()))
        } else {
            Some((
                format!("Could not update {name}'s computer"),
                update.detail(),
            ))
        }
    }

    /// The Recipes page in the main slot, with the list for the current filter. A docked pane
    /// steps aside: a table of steps wants the whole width left of the sidebar, and the pane
    /// is one click away in the title bar.
    pub fn open_recipes(&mut self, cx: &mut Context<Self>) {
        self.dismiss_popovers(cx);
        self.close_right_pane(cx);
        if self.page != MainPage::Recipes {
            self.page = MainPage::Recipes;
            self.record_nav();
        }
        self.refresh_recipes(cx);
        cx.notify();
    }

    /// The window that draws the pages, for a second window to hand work to.
    pub fn set_main_window(&mut self, window: AnyWindowHandle) {
        self.main_window = Some(window);
    }

    /// The Recipes page, asked for from another window (a coworker's screen). The page is
    /// drawn in the main window, so that window comes forward with it; `recipe` opens one
    /// recipe's own page rather than the list.
    pub fn show_recipes_in_main_window(&mut self, recipe: Option<String>, cx: &mut Context<Self>) {
        cx.activate(true);
        if let Some(window) = self.main_window {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        self.open_recipes(cx);
        if let Some(id) = recipe {
            self.open_recipe(id, cx);
        }
    }

    /// Back to the chat from a page.
    pub fn show_chat(&mut self, cx: &mut Context<Self>) {
        if self.page == MainPage::Chat {
            return;
        }
        self.page = MainPage::Chat;
        self.record_nav();
        cx.notify();
    }

    pub fn set_recipes_filter(&mut self, filter: RecipeFilter, cx: &mut Context<Self>) {
        if self.recipes_filter == filter {
            return;
        }
        self.recipes_filter = filter;
        self.refresh_recipes(cx);
        cx.notify();
    }

    /// Load the list for the current filter. A late answer for an earlier request is dropped,
    /// so switching chips quickly never shows the wrong list.
    pub fn refresh_recipes(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        self.recipes_epoch += 1;
        let epoch = self.recipes_epoch;
        let filter = self.recipes_filter;
        self.recipes_loading = true;
        self.recipes_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.list_recipes(Some(filter.query())).await;
            let _ = this.update(cx, |state, cx| {
                if state.recipes_epoch != epoch {
                    return;
                }
                state.recipes_loading = false;
                match result {
                    Ok(recipes) => state.recipes = recipes,
                    Err(error) => state.recipes_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The detail view for one recipe.
    pub fn open_recipe(&mut self, id: String, cx: &mut Context<Self>) {
        self.recipe_open = None;
        self.recipe_open_id = Some(id);
        self.recipe_busy = None;
        self.recipe_error = None;
        self.recipe_run_result = None;
        self.recipe_delete_confirm = false;
        self.load_open_recipe(cx);
        cx.notify();
    }

    /// Fetch the open recipe, keeping whatever is shown until the answer lands.
    fn load_open_recipe(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_loading = true;
        cx.spawn(async move |this, cx| {
            let result = client.recipe(&id).await;
            let _ = this.update(cx, |state, cx| {
                // A late answer for a recipe the person has since left is stale.
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_loading = false;
                match result {
                    Ok(detail) => state.set_open_recipe(detail),
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Take a detail as the open recipe, keeping what its newest run came to: the list is told
    /// nothing about runs, so this is the only place the app learns it.
    fn set_open_recipe(&mut self, detail: RecipeDetail) {
        if let Some(run) = detail.runs.first() {
            self.recipe_last_runs.insert(
                detail.recipe.id.clone(),
                RecipeRunNote {
                    ok: run.ok,
                    version: run.version,
                    stopped_at: run.stopped_at,
                    at_ms: run.at_ms,
                },
            );
        }
        self.recipe_open = Some(detail);
    }

    pub fn close_recipe(&mut self, cx: &mut Context<Self>) {
        self.recipe_open = None;
        self.recipe_open_id = None;
        self.recipe_loading = false;
        self.recipe_busy = None;
        self.recipe_error = None;
        self.recipe_run_result = None;
        self.recipe_delete_confirm = false;
        cx.notify();
    }

    /// Run one request on the open recipe and take its answer as the detail; a refusal is
    /// shown on the page. The list is reloaded too, since names and versions show there.
    fn recipe_action(
        &mut self,
        busy: &str,
        cx: &mut Context<Self>,
        action: impl FnOnce(
            OpenGrokClient,
            String,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<RecipeDetail, OpenGrokError>> + Send>,
        > + 'static,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_busy = Some(busy.to_string());
        self.recipe_error = None;
        cx.notify();
        let future = action(client, id.clone());
        cx.spawn(async move |this, cx| {
            let result = future.await;
            let _ = this.update(cx, |state, cx| {
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_busy = None;
                match result {
                    Ok(detail) => {
                        state.set_open_recipe(detail);
                        state.refresh_recipes(cx);
                    }
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn rename_open_recipe(
        &mut self,
        name: String,
        description: String,
        cx: &mut Context<Self>,
    ) {
        self.recipe_action("Saving…", cx, move |client, id| {
            Box::pin(async move { client.rename_recipe(&id, &name, &description).await })
        });
    }

    /// The edited steps as the open recipe's next version.
    pub fn add_recipe_version(
        &mut self,
        steps: Vec<RecipeStep>,
        note: String,
        cx: &mut Context<Self>,
    ) {
        self.recipe_action("Saving…", cx, move |client, id| {
            Box::pin(async move { client.add_recipe_version(&id, &steps, &note).await })
        });
    }

    /// Remove one edited version of the open recipe. The server answers the delete with
    /// nothing in particular, so the detail is fetched again to see what is left; a refusal —
    /// the raw and the filtered version cannot go — lands on the page's error line.
    pub fn delete_recipe_version(&mut self, version: u32, cx: &mut Context<Self>) {
        self.recipe_action("Deleting version…", cx, move |client, id| {
            Box::pin(async move {
                client.delete_recipe_version(&id, version).await?;
                client.recipe(&id).await
            })
        });
    }

    pub fn share_open_recipe(&mut self, target: RecipeShareTarget, cx: &mut Context<Self>) {
        self.recipe_action("Sharing…", cx, move |client, id| {
            Box::pin(async move { client.share_recipe(&id, &target).await })
        });
    }

    pub fn unshare_open_recipe(&mut self, scope: String, scope_id: String, cx: &mut Context<Self>) {
        self.recipe_action("Unsharing…", cx, move |client, id| {
            Box::pin(async move { client.unshare_recipe(&id, &scope, &scope_id).await })
        });
    }

    /// Let one of the person's bots run the open recipe, or take that back.
    pub fn set_recipe_grant(&mut self, coworker_id: String, granted: bool, cx: &mut Context<Self>) {
        self.recipe_action("Updating bots…", cx, move |client, id| {
            Box::pin(async move {
                if granted {
                    client.grant_recipe(&id, &coworker_id).await
                } else {
                    client.revoke_recipe_grant(&id, &coworker_id).await
                }
            })
        });
    }

    /// Select all and Deselect all in the Bots picker: the same grant or revoke for several
    /// bots. The requests go one after another rather than together, because each answers with
    /// the whole detail and answers that raced would leave the page showing an older set.
    pub fn set_recipe_grants(
        &mut self,
        coworker_ids: Vec<String>,
        granted: bool,
        cx: &mut Context<Self>,
    ) {
        if coworker_ids.is_empty() {
            return;
        }
        self.recipe_action("Updating bots…", cx, move |client, id| {
            Box::pin(async move {
                let mut last = None;
                for coworker_id in coworker_ids {
                    let result = if granted {
                        client.grant_recipe(&id, &coworker_id).await
                    } else {
                        client.revoke_recipe_grant(&id, &coworker_id).await
                    };
                    // One refusal stops the rest: the detail it would have answered with is
                    // no longer the truth, and the reason belongs on the page.
                    last = Some(result?);
                }
                match last {
                    Some(detail) => Ok(detail),
                    None => client.recipe(&id).await,
                }
            })
        });
    }

    /// Accept or decline a recipe shared with the person, from the list or from its detail.
    pub fn answer_recipe_share(&mut self, id: String, accept: bool, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        self.recipes_error = None;
        if self.recipe_open_id.as_deref() == Some(id.as_str()) {
            self.recipe_busy = Some(
                if accept {
                    "Accepting…"
                } else {
                    "Declining…"
                }
                .to_string(),
            );
            self.recipe_error = None;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = if accept {
                client.accept_recipe(&id).await
            } else {
                client.decline_recipe(&id).await
            };
            let _ = this.update(cx, |state, cx| {
                let open = state.recipe_open_id.as_deref() == Some(id.as_str());
                if open {
                    state.recipe_busy = None;
                }
                match result {
                    Ok(detail) => {
                        if open {
                            state.set_open_recipe(detail);
                        }
                        state.refresh_recipes(cx);
                    }
                    Err(error) if open => state.recipe_error = Some(error.message),
                    Err(error) => state.recipes_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Play the open recipe on one of the person's bots. The outcome and the screen after it
    /// show on the page, and the run joins the history.
    pub fn run_open_recipe(&mut self, coworker_id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_busy = Some("Running…".to_string());
        self.recipe_error = None;
        self.recipe_run_result = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.run_recipe(&id, &coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_busy = None;
                match result {
                    Ok(result) => {
                        state.recipe_run_result =
                            Some(RecipeRunOutcome::from_result(coworker_id, result));
                        state.load_open_recipe(cx);
                    }
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Ask before deleting the open recipe: the dialog over the app.
    pub fn open_recipe_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if self.recipe_open.is_none() {
            return;
        }
        self.recipe_delete_confirm = true;
        cx.notify();
    }

    pub fn close_recipe_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if self.recipe_delete_confirm {
            self.recipe_delete_confirm = false;
            cx.notify();
        }
    }

    /// The dialog's Delete: remove the recipe, then leave its detail for the list.
    pub fn confirm_recipe_delete(&mut self, cx: &mut Context<Self>) {
        if !self.recipe_delete_confirm {
            return;
        }
        self.recipe_delete_confirm = false;
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_busy = Some("Deleting…".to_string());
        self.recipe_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.delete_recipe(&id).await;
            let _ = this.update(cx, |state, cx| {
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_busy = None;
                match result {
                    Ok(()) => {
                        state.close_recipe(cx);
                        state.refresh_recipes(cx);
                    }
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The name of the recipe the delete dialog asks about, while it is open.
    pub fn recipe_delete_prompt(&self) -> Option<String> {
        if !self.recipe_delete_confirm {
            return None;
        }
        self.recipe_open
            .as_ref()
            .map(|detail| detail.recipe.name.clone())
    }

    /// Teach the active bot a task: its screen, with a tape already running. The same thing the
    /// screen window's own Teach a task button does, asked for from the composer, opening the
    /// window first when there is not one yet.
    pub fn teach_task(&mut self, cx: &mut Context<Self>) {
        self.show_coworker_screen(true, cx);
    }

    pub fn open_coworker_screen(&mut self, cx: &mut Context<Self>) {
        self.show_coworker_screen(false, cx);
    }

    /// The active coworker's screen. `teach` carries the ask for a tape all the way to the
    /// window, which may be several awaits away: the box has to exist before it has a screen.
    fn show_coworker_screen(&mut self, teach: bool, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        if let Some(url) = self
            .coworker_computer
            .as_ref()
            .and_then(CoworkerComputer::vnc_url)
            .map(str::to_string)
        {
            self.open_computer_window(&coworker_id, &url, teach, cx);
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = client.ensure_coworker_computer(&coworker_id).await;
            let _ = this.update(cx, |state, cx| match result {
                Ok(status) => {
                    if let Some(url) = status.vnc_url().map(str::to_string) {
                        state.open_computer_window(&coworker_id, &url, teach, cx);
                    }
                    if state.active_coworker_id.as_deref() == Some(coworker_id.as_str()) {
                        state.coworker_computer = Some(status);
                        cx.notify();
                    }
                }
                Err(error) => eprintln!(
                    "NativeChat computer: could not open the computer of {coworker_id}: {}",
                    error.message
                ),
            });
        })
        .detach();
    }

    /// The screen of `coworker_id`, in its own window. The title names that
    /// coworker, not whichever one is active by the time the answer lands.
    fn open_computer_window(
        &mut self,
        coworker_id: &str,
        url: &str,
        teach: bool,
        cx: &mut Context<Self>,
    ) {
        let title = self
            .coworkers
            .iter()
            .find(|coworker| coworker.id == coworker_id)
            .map(|coworker| format!("{}'s Computer", coworker.name))
            .unwrap_or_else(|| "Computer".into());
        #[cfg(target_os = "macos")]
        {
            // Already open: bring it forward. A handle whose window was closed fails to
            // update, and that is the cue to open a fresh one.
            if let Some(existing) = self.computer_windows.get(coworker_id)
                && existing
                    .update(cx, |screen, window, cx| {
                        window.activate_window();
                        if teach {
                            screen.start_teaching(window, cx);
                        }
                    })
                    .is_ok()
            {
                return;
            }
            let url = url.to_string();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(72.), px(72.)),
                    size: size(px(1100.), px(760.)),
                })),
                window_min_size: Some(size(px(640.), px(480.))),
                // The title bar is ours: transparent, with the traffic lights left where they
                // are, so the strip the window paints (name, Teach a task) IS the title bar and
                // follows the app's theme rather than the system's.
                titlebar: Some(TitlebarOptions {
                    title: Some(title.clone().into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.), px(14.))),
                }),
                ..WindowOptions::default()
            };
            let coworker = coworker_id.to_string();
            let app = cx.entity();
            let opened = cx.open_window(options, move |window, cx| {
                cx.new(|cx| {
                    crate::components::computer_screen::ComputerScreen::new(
                        &url, &coworker, &title, app, window, cx,
                    )
                })
            });
            match opened {
                Ok(handle) => {
                    self.computer_windows
                        .insert(coworker_id.to_string(), handle);
                    if teach {
                        // The page is not up yet, and teaching is a flag set on it; wait the
                        // same moment the window itself waits before painting the page.
                        cx.spawn(async move |_, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(1500))
                                .await;
                            let _ = handle.update(cx, |screen, window, cx| {
                                screen.start_teaching(window, cx);
                            });
                        })
                        .detach();
                    }
                }
                Err(error) => eprintln!("NativeChat computer: could not open a window: {error}"),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = teach;
            eprintln!("NativeChat computer: {title} is at {url}; opening it in-app is macOS-only");
        }
    }

    pub fn open_routine_editor(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        let id = match id {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                self.routines.entry(coworker_id).or_default().insert(
                    0,
                    AgentRoutine {
                        id: id.clone(),
                        name: String::new(),
                        instruction: String::new(),
                        active: true,
                        triggers: Vec::new(),
                        runs: Vec::new(),
                    },
                );
                id
            }
        };
        self.set_right_pane(RightPane::Computer, cx);
        self.computer_view = ComputerView::Editor { id: Some(id) };
        self.record_nav();
        cx.notify();
    }

    pub fn back_to_computer(&mut self, cx: &mut Context<Self>) {
        if let (Some(coworker_id), ComputerView::Editor { id: Some(rid) }) =
            (self.active_coworker_id.clone(), self.computer_view.clone())
        {
            let empty = self
                .routines
                .get(&coworker_id)
                .and_then(|rows| rows.iter().find(|row| row.id == rid))
                .is_some_and(|row| {
                    row.name.trim().is_empty()
                        && row.instruction.trim().is_empty()
                        && row.triggers.is_empty()
                        && row.runs.is_empty()
                });
            if empty {
                self.delete_routine(&coworker_id, &rid, cx);
                return;
            }
        }
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn coworker_routines(&self, coworker_id: &str) -> &[AgentRoutine] {
        self.routines
            .get(coworker_id)
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
    }

    pub fn save_routine_fields(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        name: String,
        instruction: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.routine_mut(coworker_id, routine_id) {
            row.name = name;
            row.instruction = instruction;
        }
        cx.notify();
    }

    pub fn routine_mut(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
    ) -> Option<&mut AgentRoutine> {
        self.routines
            .get_mut(coworker_id)?
            .iter_mut()
            .find(|row| row.id == routine_id)
    }

    pub fn add_routine_trigger(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        trigger: RoutineTrigger,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.routine_mut(coworker_id, routine_id) {
            row.triggers.push(trigger);
        }
        cx.notify();
    }

    pub fn update_webhook(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        trigger_id: &str,
        url: String,
        key: String,
        header: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.routine_mut(coworker_id, routine_id)
            && let Some(RoutineTrigger::Webhook {
                url: u,
                key: k,
                header: h,
                ..
            }) = row.triggers.iter_mut().find(|t| t.id() == trigger_id)
        {
            *u = url;
            *k = key;
            *h = header;
        }
        cx.notify();
    }

    pub fn update_schedule_spec(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        trigger_id: &str,
        spec: ScheduleSpec,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.routine_mut(coworker_id, routine_id)
            && let Some(RoutineTrigger::Schedule { spec: current, .. }) =
                row.triggers.iter_mut().find(|t| t.id() == trigger_id)
        {
            *current = spec;
        }
        cx.notify();
    }

    pub fn record_routine_run(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        cx: &mut Context<Self>,
    ) {
        let stamp = chrono::Local::now()
            .format("%b %d at %I:%M %p")
            .to_string()
            .replace(" 0", " ");
        if let Some(row) = self.routine_mut(coworker_id, routine_id) {
            row.runs.insert(
                0,
                RoutineRun {
                    at: stamp,
                    ok: true,
                },
            );
        }
        cx.notify();
    }

    pub fn delete_routine(&mut self, coworker_id: &str, routine_id: &str, cx: &mut Context<Self>) {
        if let Some(rows) = self.routines.get_mut(coworker_id) {
            rows.retain(|row| row.id != routine_id);
        }
        self.computer_view = ComputerView::Overview;
        cx.notify();
    }

    pub fn set_routine_active(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(rows) = self.routines.get_mut(coworker_id)
            && let Some(row) = rows.iter_mut().find(|row| row.id == routine_id)
        {
            row.active = active;
        }
        cx.notify();
    }

    pub fn dismiss_popovers(&mut self, cx: &mut Context<Self>) {
        if !self.model_picker_open && !self.avatar_editor_open && self.emoji_picker.is_none() {
            return;
        }
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.emoji_picker = None;
        self.hidden_bots_open = false;
        cx.notify();
    }

    pub fn set_reply_to(&mut self, reply: ReplyTo, cx: &mut Context<Self>) {
        self.reply_to = Some(reply);
        cx.notify();
    }

    pub fn clear_reply_to(&mut self, cx: &mut Context<Self>) {
        if self.reply_to.take().is_some() {
            cx.notify();
        }
    }

    pub fn toggle_reaction(&mut self, message_id: String, emoji: String, cx: &mut Context<Self>) {
        match self.message_reactions.get(&message_id) {
            Some(current) if current == &emoji => {
                self.message_reactions.remove(&message_id);
            }
            _ => {
                self.message_reactions.insert(message_id, emoji);
            }
        }
        self.emoji_picker = None;
        cx.notify();
    }

    pub fn open_emoji_picker(
        &mut self,
        message_id: String,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.emoji_picker = Some(EmojiPickerOpen { message_id, bounds });
        cx.notify();
    }

    /// Pick one of the composer's capabilities. The picked ones show as chips beside the field.
    /// Name a tool for the next message. Naming the same one twice is one chip, not two.
    pub fn pick_tool(&mut self, id: String, label: String, cx: &mut Context<Self>) {
        if self.picked_tools.iter().any(|picked| picked.id == id) {
            return;
        }
        let kind = PickedKind::of(&id);
        self.picked_tools.push(PickedTool { id, label, kind });
        cx.notify();
    }

    /// Take a named tool back.
    pub fn unpick_tool(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.picked_tools.iter().position(|picked| picked.id == id) {
            self.picked_tools.remove(index);
            cx.notify();
        }
    }

    /// The names to send with the next message, in the order they were named.
    pub fn picked_tool_ids(&self) -> Vec<String> {
        self.picked_tools
            .iter()
            .map(|picked| picked.id.clone())
            .collect()
    }

    pub fn close_emoji_picker(&mut self, cx: &mut Context<Self>) {
        if self.emoji_picker.take().is_some() {
            cx.notify();
        }
    }

    pub fn delete_message(&mut self, message_id: &str, cx: &mut Context<Self>) {
        if self.native_tts.message_id.as_deref() == Some(message_id) {
            if let Some(service) = &self.tts_service {
                service.stop_native();
            }
            self.native_tts = SourceTtsState::default();
        }
        if let Some(id) = &self.active_conversation_id {
            if let Some(conversation) = self.conversations.iter_mut().find(|c| &c.id == id) {
                conversation.messages.retain(|m| m.id != message_id);
            }
        }
        self.message_reactions.remove(message_id);
        if self
            .reply_to
            .as_ref()
            .is_some_and(|r| r.message_id == message_id)
        {
            self.reply_to = None;
        }
        if self
            .emoji_picker
            .as_ref()
            .is_some_and(|p| p.message_id == message_id)
        {
            self.emoji_picker = None;
        }
        if let Some(db) = self.database_service.clone() {
            let id = message_id.to_string();
            cx.spawn(async move |_, _| {
                if let Err(error) = db.delete_message(&id).await {
                    eprintln!("Failed to delete message: {error}");
                }
            })
            .detach();
        }
        cx.notify();
    }

    pub fn set_model_picker_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.model_picker_open == open && (!open || !self.avatar_editor_open) {
            return;
        }
        self.model_picker_open = open;
        if open {
            self.avatar_editor_open = false;
        }
        cx.notify();
    }

    pub fn set_avatar_editor_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.avatar_editor_open == open && (!open || !self.model_picker_open) {
            return;
        }
        self.avatar_editor_open = open;
        if open {
            self.model_picker_open = false;
        }
        cx.notify();
    }

    pub fn patch_active_coworker(
        &mut self,
        model: Option<String>,
        role: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.patch_active_agent(
            CoworkerPatch {
                model,
                role,
                ..Default::default()
            },
            cx,
        );
    }

    pub fn patch_active_agent(&mut self, patch: CoworkerPatch, cx: &mut Context<Self>) {
        self.patch_active_agent_then(patch, None, cx);
    }

    /// The same patch, with `done` called once the request is over, carrying the server's
    /// message when it refused. A control that was put out of the person's hands for the
    /// length of the request has no other way of hearing that the request has ended.
    pub fn patch_active_agent_then(
        &mut self,
        patch: CoworkerPatch,
        done: Option<PatchDone>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.refuse_patch("OpenGrok is not configured", done, cx);
            return;
        };
        let Some(id) = self.active_coworker_id.clone() else {
            self.refuse_patch("No agent selected", done, cx);
            return;
        };
        // The roster takes the patch before the server has seen it so the pane answers the
        // click at once, and keeps what it held before it so that guess can be taken back:
        // `settle_patch` replaces it with the server's word either way.
        let before = self.coworkers.iter().find(|c| c.id == id).cloned();
        if let Some(existing) = self.coworkers.iter_mut().find(|c| c.id == id) {
            apply_patch(existing, &patch);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.patch_coworker(&id, &patch).await;
            let error = result.as_ref().err().map(|error| error.message.clone());
            let _ = this.update(cx, |state, cx| {
                if let Some(before) = before.as_ref()
                    && let Some(existing) = state.coworkers.iter_mut().find(|c| c.id == id)
                {
                    settle_patch(existing, &patch, result.as_ref().ok(), before);
                }
                state.auth_error = error.clone();
                cx.notify();
            });
            if let Some(done) = done {
                // Outside the roster's own update, so that whoever was waiting on the patch is
                // free to reach for anything the app holds, this roster included.
                cx.update(|cx| done(error, cx));
            }
        })
        .detach();
    }

    /// A patch that never left the app. The settings pane paints the reason, and whoever is
    /// waiting on the request still has to hear that it is over.
    fn refuse_patch(&mut self, reason: &str, done: Option<PatchDone>, cx: &mut Context<Self>) {
        let reason = reason.to_string();
        self.auth_error = Some(reason.clone());
        cx.notify();
        if let Some(done) = done {
            // This one is answered without the server, so the answer would land while the
            // click that asked for the patch is still being handled and the asking view is
            // still on the stack. It waits for the end of the effect cycle instead.
            cx.defer(move |cx| done(Some(reason), cx));
        }
    }

    pub fn update_opengrok_profile(
        &mut self,
        first_name: String,
        last_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .update_profile(&ProfileUpdate {
                    first_name: Some(first_name),
                    last_name: Some(last_name),
                    avatar_url: None,
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_error = None;
                    }
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn change_opengrok_password(
        &mut self,
        current: String,
        new_password: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.change_password(&current, &new_password).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(()) => state.auth_error = None,
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn set_database_service(&mut self, service: DatabaseService, cx: &mut Context<Self>) {
        self.database_service = Some(service.clone());
        cx.notify();

        // Load sessions when DB service is set
        self.load_sessions(cx);
    }

    pub fn load_sessions(&mut self, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            cx.spawn(async move |this, cx| {
                match db.get_sessions().await {
                    Ok(sessions) => {
                        this.update(cx, |state, cx| {
                            state.conversations = sessions
                                .into_iter()
                                .map(|s| Conversation {
                                    id: s.id,
                                    title: s.title,
                                    created_at: s.created_at,
                                    updated_at: s.updated_at,
                                    messages: Vec::new(),
                                    unread_count: 0,
                                })
                                .collect();

                            // If no active conversation, select the most recent one
                            if state.active_conversation_id.is_none() {
                                if let Some(first) = state.conversations.first() {
                                    let id = first.id.clone();
                                    state.select_conversation(id, cx);
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                    }
                    Err(e) => eprintln!("Failed to load sessions: {}", e),
                }
            })
            .detach();
        }
    }

    pub fn ensure_active_coworker(&mut self, cx: &mut Context<Self>) {
        if self.active_coworker_id.is_some() {
            return;
        }
        if let Some(first) = self.ranked_coworkers().into_iter().next() {
            self.select_coworker(first.id, cx);
        }
    }

    pub fn open_bot_finder(&mut self, cx: &mut Context<Self>) {
        self.command_palette_open = false;
        self.bot_finder_open = true;
        self.dismiss_popovers(cx);
        cx.notify();
    }

    pub fn close_bot_finder(&mut self, cx: &mut Context<Self>) {
        if self.bot_finder_open {
            self.bot_finder_open = false;
            cx.notify();
        }
    }

    pub fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.bot_finder_open = false;
        self.command_palette_open = true;
        self.dismiss_popovers(cx);
        cx.notify();
    }

    pub fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_open {
            self.command_palette_open = false;
            cx.notify();
        }
    }

    pub fn create_agent(&mut self, cx: &mut Context<Self>) {
        self.hire_agent("New Bot", cx);
    }

    pub fn hire_agent(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.hiring {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        if !self.is_signed_in() {
            self.auth_error = Some("Sign in first".to_string());
            cx.notify();
            return;
        }
        self.hiring = true;
        self.auth_error = None;
        self.bot_finder_open = false;
        self.command_palette_open = false;
        cx.notify();
        let name = name.to_string();
        cx.spawn(async move |this, cx| {
            let result = client.hire(&name, None).await;
            let _ = this.update(cx, |state, cx| {
                state.hiring = false;
                match result {
                    Ok(hired) => {
                        let id = hired.id.clone();
                        state.coworkers.insert(0, hired);
                        state.select_coworker(id, cx);
                    }
                    Err(error) => {
                        state.auth_error = Some(format!(
                            "Could not create agent: {} (is OpenGrok running at {}?)",
                            error.message,
                            state
                                .config
                                .as_ref()
                                .map(|c| c.opengrok_base_url.as_str())
                                .unwrap_or("http://127.0.0.1:1447")
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn select_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(coworker) = self.coworkers.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        self.active_coworker_id = Some(id.clone());
        // A bot chosen is a chat: the main slot leaves whatever page it was on.
        self.page = MainPage::Chat;
        // The previous bot's screen must not show under this bot's name.
        self.coworker_computer = None;
        self.coworker_screen = None;
        self.computer_confirm = None;
        self.computer_action_error = None;
        if !self.conversations.iter().any(|c| c.id == id) {
            self.conversations.insert(
                0,
                Conversation {
                    id: id.clone(),
                    title: coworker.name.clone(),
                    created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    updated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    messages: Vec::new(),
                    unread_count: 0,
                },
            );
        }
        self.select_conversation(id, cx);
        self.record_nav();
    }

    fn nav_location(&self) -> NavLocation {
        NavLocation {
            coworker_id: self.active_coworker_id.clone(),
            page: self.page,
            right_pane: self.right_pane,
            computer_view: self.computer_view.clone(),
            app_settings_open: self.is_app_settings_open,
            app_settings_tab: self.app_settings_tab,
        }
    }

    fn record_nav(&mut self) {
        self.nav.record(self.nav_location());
    }

    pub fn nav_back(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_open || self.bot_finder_open {
            self.command_palette_open = false;
            self.bot_finder_open = false;
            cx.notify();
            return;
        }
        let Some(loc) = self.nav.go_back() else {
            return;
        };
        self.apply_nav(loc, cx);
    }

    pub fn nav_forward(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_open || self.bot_finder_open {
            self.command_palette_open = false;
            self.bot_finder_open = false;
            cx.notify();
            return;
        }
        let Some(loc) = self.nav.go_forward() else {
            return;
        };
        self.apply_nav(loc, cx);
    }

    fn apply_nav(&mut self, loc: NavLocation, cx: &mut Context<Self>) {
        self.nav.applying = true;
        self.bot_finder_open = false;
        self.command_palette_open = false;
        if let Some(id) = loc.coworker_id.clone() {
            if self.active_coworker_id.as_ref() != Some(&id) {
                self.select_coworker(id, cx);
            }
        } else {
            self.active_coworker_id = None;
        }
        self.set_right_pane(loc.right_pane, cx);
        self.computer_view = loc.computer_view;
        self.is_app_settings_open = loc.app_settings_open;
        self.app_settings_tab = loc.app_settings_tab;
        // After `select_coworker`, which lands on the chat: the page is where the person was.
        self.page = loc.page;
        if self.page == MainPage::Recipes {
            self.refresh_recipes(cx);
        }
        self.nav.applying = false;
        cx.notify();
    }

    pub fn touch_coworker_activity(&mut self, id: &str) {
        self.last_active_at
            .insert(id.to_string(), SystemTime::now());
        if let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) {
            conversation.updated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Most recent message first; idle bots (no messages) by created date, newest first.
    pub fn ranked_coworkers(&self) -> Vec<Coworker> {
        let mut list = self.coworkers.clone();
        list.sort_by(|a, b| self.coworker_rank(b).cmp(&self.coworker_rank(a)));
        list
    }

    fn coworker_rank(&self, coworker: &Coworker) -> (u8, u128, i64) {
        match self.coworker_activity_ms(coworker) {
            Some(ms) => (1, ms, self.coworker_created_ms(coworker)),
            None => (0, 0, self.coworker_created_ms(coworker)),
        }
    }

    fn coworker_activity_ms(&self, coworker: &Coworker) -> Option<u128> {
        if let Some(at) = self.last_active_at.get(&coworker.id) {
            return Some(system_time_ms(*at));
        }
        let conversation = self.conversations.iter().find(|c| c.id == coworker.id)?;
        if let Some(message) = conversation
            .messages
            .iter()
            .rev()
            .find(|m| !m.content.trim().is_empty())
        {
            return Some(system_time_ms(message.sent_at));
        }
        let updated = parse_sql_time(&conversation.updated_at)?;
        let created = parse_sql_time(&conversation.created_at);
        if created.is_some_and(|c| updated > c) {
            Some(system_time_ms(updated))
        } else {
            None
        }
    }

    fn coworker_created_ms(&self, coworker: &Coworker) -> i64 {
        if coworker.updated_at_ms > 0 {
            return coworker.updated_at_ms;
        }
        self.conversations
            .iter()
            .find(|c| c.id == coworker.id)
            .and_then(|c| parse_sql_time(&c.created_at))
            .map(|t| system_time_ms(t) as i64)
            .unwrap_or(0)
    }

    pub fn toggle_pin_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.pinned_coworker_ids.remove(&id) {
            self.pinned_coworker_ids.insert(id);
        }
        cx.notify();
    }

    pub fn hide_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        self.hidden_coworker_ids.insert(id.clone());
        if let Some(coworker) = self.coworkers.iter_mut().find(|c| c.id == id) {
            coworker.hidden_from_sidebar = true;
        }
        self.persist_hidden(&id, true, cx);
        if self.active_coworker_id.as_ref() == Some(&id) {
            let next = self
                .ranked_coworkers()
                .into_iter()
                .map(|c| c.id)
                .find(|other| other != &id && !self.hidden_coworker_ids.contains(other));
            if let Some(next) = next {
                self.select_coworker(next, cx);
                return;
            }
            self.active_coworker_id = None;
        }
        cx.notify();
    }

    pub fn unhide_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        self.hidden_coworker_ids.remove(&id);
        if let Some(coworker) = self.coworkers.iter_mut().find(|c| c.id == id) {
            coworker.hidden_from_sidebar = false;
        }
        self.persist_hidden(&id, false, cx);
        cx.notify();
    }

    pub fn open_hidden_bots(&mut self, cx: &mut Context<Self>) {
        self.hidden_bots_open = true;
        cx.notify();
    }

    pub fn close_hidden_bots(&mut self, cx: &mut Context<Self>) {
        if self.hidden_bots_open {
            self.hidden_bots_open = false;
            cx.notify();
        }
    }

    fn persist_hidden(&mut self, id: &str, hidden: bool, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let id = id.to_string();
        let patch = CoworkerPatch {
            hidden_from_sidebar: Some(hidden),
            ..Default::default()
        };
        cx.spawn(async move |this, cx| {
            let result = client.patch_coworker(&id, &patch).await;
            let _ = this.update(cx, |state, cx| {
                if let Err(error) = result {
                    eprintln!("hide coworker: {}", error.message);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn mark_coworker_read(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) {
            conversation.unread_count = 0;
        }
        cx.notify();
    }

    pub fn begin_rename_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        self.renaming_coworker_id = Some(id);
        cx.notify();
    }

    pub fn cancel_rename_coworker(&mut self, cx: &mut Context<Self>) {
        if self.renaming_coworker_id.take().is_some() {
            cx.notify();
        }
    }

    pub fn commit_rename_coworker(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(id) = self.renaming_coworker_id.take() else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            cx.notify();
            return;
        }
        self.select_coworker(id, cx);
        self.patch_active_agent(
            CoworkerPatch {
                name: Some(name),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn open_agent_profile(&mut self, id: String, cx: &mut Context<Self>) {
        self.select_coworker(id, cx);
        self.set_right_pane(RightPane::Settings, cx);
        self.computer_view = ComputerView::Overview;
        cx.notify();
    }

    pub fn duplicate_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        if self.hiring {
            return;
        }
        let Some(source) = self.coworkers.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        self.hiring = true;
        self.auth_error = None;
        cx.notify();
        let name = if source.name.trim().is_empty() {
            "New Bot".to_string()
        } else {
            format!("{} copy", source.name.trim())
        };
        let model = if source.model.is_empty() {
            None
        } else {
            Some(source.model.clone())
        };
        let patch = CoworkerPatch {
            model: model.clone(),
            role: source.role.clone(),
            title: source.title.clone(),
            avatar_shape: source.avatar_shape.clone(),
            avatar_color: source.avatar_color.clone(),
            notify_on_updates: source.notify_on_updates,
            ..Default::default()
        };
        cx.spawn(async move |this, cx| {
            let hired = client.hire(&name, model.as_deref()).await;
            let _ = this.update(cx, |state, cx| {
                state.hiring = false;
                match hired {
                    Ok(hired) => {
                        let new_id = hired.id.clone();
                        state.coworkers.insert(0, hired);
                        if !patch.is_empty() {
                            state.active_coworker_id = Some(new_id.clone());
                            state.patch_active_agent(patch, cx);
                        }
                        state.select_coworker(new_id, cx);
                    }
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        self.coworkers.retain(|c| c.id != id);
        self.pinned_coworker_ids.remove(&id);
        self.hidden_coworker_ids.remove(&id);
        if self.renaming_coworker_id.as_ref() == Some(&id) {
            self.renaming_coworker_id = None;
        }
        if self.active_coworker_id.as_ref() == Some(&id) {
            self.active_coworker_id = self.coworkers.first().map(|c| c.id.clone());
            if let Some(next) = self.active_coworker_id.clone() {
                self.select_coworker(next, cx);
            } else {
                self.close_right_pane(cx);
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.delete_coworker(&id).await;
            let _ = this.update(cx, |state, cx| {
                if let Err(error) = result {
                    state.auth_error = Some(error.message);
                    state.refresh_coworkers(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn create_new_session(&mut self, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            cx.spawn(
                async move |this, cx| match db.create_session("New Chat").await {
                    Ok(id) => {
                        this.update(cx, |state, cx| {
                            state.conversations.insert(
                                0,
                                Conversation {
                                    id: id.clone(),
                                    title: "New Chat".to_string(),
                                    created_at: chrono::Local::now()
                                        .format("%Y-%m-%d %H:%M:%S")
                                        .to_string(),
                                    updated_at: chrono::Local::now()
                                        .format("%Y-%m-%d %H:%M:%S")
                                        .to_string(),
                                    messages: Vec::new(),
                                    unread_count: 0,
                                },
                            );
                            state.select_conversation(id, cx);
                        })
                        .ok();
                    }
                    Err(e) => eprintln!("Failed to create session: {}", e),
                },
            )
            .detach();
        }
    }

    pub fn rename_session(&mut self, id: String, new_title: String, cx: &mut Context<Self>) {
        if let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) {
            conversation.title = new_title.clone();
            cx.notify();

            if let Some(db) = self.database_service.clone() {
                cx.spawn(async move |_this, _cx| {
                    if let Err(e) = db.update_session_title(&id, &new_title).await {
                        eprintln!("Failed to rename session: {}", e);
                    }
                })
                .detach();
            }
        }
    }

    pub fn delete_session(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(index) = self.conversations.iter().position(|c| c.id == id) {
            self.conversations.remove(index);

            // If we deleted the active conversation, select another one
            if self.active_conversation_id.as_ref() == Some(&id) {
                self.active_conversation_id = self.conversations.first().map(|c| c.id.clone());
                if let Some(new_id) = self.active_conversation_id.clone() {
                    self.load_session_messages(new_id, cx);
                }
            }

            cx.notify();

            if let Some(db) = self.database_service.clone() {
                cx.spawn(async move |_this, _cx| {
                    if let Err(e) = db.delete_session(&id).await {
                        eprintln!("Failed to delete session: {}", e);
                    }
                })
                .detach();
            }
        }
    }

    pub fn load_session_messages(&mut self, session_id: String, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            let session_id_clone = session_id.clone();
            cx.spawn(
                async move |this, cx| match db.get_messages(&session_id_clone).await {
                    Ok(db_messages) => {
                        this.update(cx, |state, cx| {
                            if let Some(conversation) = state
                                .conversations
                                .iter_mut()
                                .find(|c| c.id == session_id_clone)
                            {
                                conversation.messages = db_messages
                                    .into_iter()
                                    .map(|m| {
                                        let sent_at = NaiveDateTime::parse_from_str(
                                            &m.created_at,
                                            "%Y-%m-%d %H:%M:%S",
                                        )
                                        .map(|dt| SystemTime::from(dt.and_utc()))
                                        .unwrap_or(SystemTime::now());

                                        // The pieces the turn was made of, so a thread reopened
                                        // shows the bubbles and the pictures it showed live.
                                        let content = m.content;
                                        let parts = restored_parts(&content, m.parts);
                                        Message {
                                            id: m.id,
                                            sender: if m.role == "user" {
                                                "Me".to_string()
                                            } else {
                                                "AI".to_string()
                                            },
                                            content,
                                            sent_at,
                                            is_me: m.role == "user",
                                            reply_preview: m.reply_preview,
                                            reply_to_id: m.reply_to_id,
                                            reply_is_me: m.reply_is_me.unwrap_or(0) != 0,
                                            parts,
                                        }
                                    })
                                    .collect();
                                state.sync_pending_approvals(cx);
                                cx.notify();
                            }
                        })
                        .ok();
                    }
                    Err(e) => {
                        eprintln!("Failed to load messages: {}", e);
                        let _ = this.update(cx, |state, cx| {
                            state.sync_pending_approvals(cx);
                        });
                    }
                },
            )
            .detach();
        } else {
            self.sync_pending_approvals(cx);
        }
    }

    pub fn select_conversation(&mut self, conversation_id: String, cx: &mut Context<Self>) {
        self.active_conversation_id = Some(conversation_id.clone());
        self.load_session_messages(conversation_id, cx);
        self.sync_pending_approvals(cx);
        cx.notify();
    }

    fn conversation_title(&self, id: &str) -> String {
        self.conversations
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// Keep the coworker's reply so the thread survives a relaunch.
    ///
    /// Only what the coworker actually said: a status line is the app's own words about the turn,
    /// and saving it would put a line nobody spoke into the history every later turn is sent.
    ///
    /// The pieces go with it. A recipe run is several bubbles with pictures of the box's screen
    /// between them, and a reply flattened to its text would come back as one long paragraph.
    fn persist_assistant_reply(
        &self,
        conversation_id: &str,
        content: String,
        parts: &[ChatPart],
        cx: &mut Context<Self>,
    ) {
        if content.trim().is_empty() || is_status_line(&content) {
            return;
        }
        let Some(db) = self.database_service.clone() else {
            return;
        };
        let title = self.conversation_title(conversation_id);
        let conversation_id = conversation_id.to_string();
        let parts = saved_parts(parts);
        cx.spawn(async move |_this, _cx| {
            let saved = match db.ensure_session(&conversation_id, &title).await {
                Ok(()) => db
                    .save_message(
                        &conversation_id,
                        "assistant",
                        &content,
                        None,
                        None,
                        None,
                        &parts,
                    )
                    .await
                    .map(|_| ()),
                Err(error) => Err(error),
            };
            if let Err(error) = saved {
                eprintln!("Failed to save assistant message: {error}");
            }
        })
        .detach();
    }

    fn send_opengrok_turn(
        &mut self,
        conversation_id: String,
        _content: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        let coworker_id = self.active_coworker_id.clone();
        let history: Vec<AguiMessage> = self
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .map(|c| agui_messages(&c.messages))
            .unwrap_or_default();

        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            conversation.messages.push(Message {
                id: uuid::Uuid::now_v7().to_string(),
                sender: "AI".to_string(),
                content: String::new(),
                sent_at: SystemTime::now(),
                is_me: false,
                reply_preview: None,
                reply_to_id: None,
                reply_is_me: false,
                parts: Vec::new(),
            });
        }
        if let Some(id) = self.active_coworker_id.clone() {
            self.touch_coworker_activity(&id);
        }
        self.begin_responding(coworker_id.as_deref(), "Thinking");
        cx.notify();

        cx.spawn(async move |this, cx| {
            let coworker = match coworker_id.clone() {
                Some(id) => Ok(id),
                None => match client.hire("NativeChat", None).await {
                    Ok(hired) => {
                        let id = hired.id.clone();
                        let _ = this.update(cx, |state, _| {
                            state.active_coworker_id = Some(id.clone());
                            state.coworkers = vec![hired];
                        });
                        Ok(id)
                    }
                    Err(error) => Err(error),
                },
            };
            let (result, waiting_approval, turn_id, deeds) = match coworker {
                Ok(id) => {
                    let mut tracker = ToolCallTracker::default();
                    let mut assembler = TurnAssembler::default();
                    let result = client
                        .run_turn(&id, &conversation_id, &history, |event| {
                            match tracker.tick(event) {
                                ActivityTick::Keep => {}
                                tick => {
                                    let turn_id = id.clone();
                                    let _ = this.update(cx, |state, cx| {
                                        let before = state.bot_status.clone();
                                        state.apply_turn_status(Some(&turn_id), tick);
                                        if state.bot_status != before {
                                            cx.notify();
                                        }
                                    });
                                }
                            }
                            assembler.push_event(&event);
                            let (plain, parts) = assembler.snapshot();
                            let _ = this.update(cx, |state, cx| {
                                if let Some(conversation) = state
                                    .conversations
                                    .iter_mut()
                                    .find(|c| c.id == conversation_id)
                                {
                                    if let Some(last) = conversation.messages.last_mut() {
                                        if !last.is_me {
                                            last.content = plain.clone();
                                            last.parts = parts.clone();
                                            cx.notify();
                                        }
                                    }
                                }
                                if assembler.waiting_approval() {
                                    let open = parts.iter().rev().find_map(|part| match part {
                                        ChatPart::Approval(spec) => Some(spec.clone()),
                                        _ => None,
                                    });
                                    if let Some(spec) = open {
                                        if let Some(resolution) =
                                            state.auto_resolve_local_exec(&spec)
                                        {
                                            state.answer_approval(spec, resolution, cx);
                                        }
                                    }
                                }
                            });
                        })
                        .await;
                    assembler.finish();
                    let waiting_approval = assembler.waiting_approval();
                    // What the tools did, in case the turn ends without a word about it.
                    let deeds = tracker.deeds();
                    let (plain, parts) = assembler.snapshot();
                    let _ = this.update(cx, |state, cx| {
                        if let Some(conversation) = state
                            .conversations
                            .iter_mut()
                            .find(|c| c.id == conversation_id)
                        {
                            if let Some(last) = conversation.messages.last_mut() {
                                if !last.is_me {
                                    last.content = plain;
                                    last.parts = parts;
                                }
                            }
                        }
                        cx.notify();
                    });
                    (result, waiting_approval, Some(id), deeds)
                }
                Err(error) => (Err(error), false, coworker_id.clone(), Vec::new()),
            };
            let _ = this.update(cx, |state, cx| {
                if let Some(conversation) = state
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    if let Some(last) = conversation.messages.last_mut() {
                        // A turn that ends without words is spoken for by the app: why it
                        // failed, what its tools did, or the note that it said nothing at all.
                        // A picture is not an answer, so a failed run says so even when the
                        // turn left a screenshot behind.
                        if !last.is_me && !last.has_text_body() {
                            match &result {
                                Ok(text) if !text.is_empty() => last.content = text.clone(),
                                // Parked on a permission card: the turn is not over yet.
                                Ok(_) if waiting_approval => {}
                                Ok(_) => {
                                    last.content = tool_standin(&deeds)
                                        .unwrap_or_else(|| EMPTY_TURN_NOTE.to_string())
                                }
                                Err(error) => {
                                    last.content = format!("{RUN_ERROR_PREFIX}{}", error.message)
                                }
                            }
                        }
                    }
                }
                if !waiting_approval && result.is_ok() {
                    // The run is final; a run parked on a card is saved when it finishes.
                    // A status line is painted, never saved: `persist_assistant_reply` refuses
                    // it, so it cannot become history the model is shown next turn.
                    let reply = state
                        .conversations
                        .iter()
                        .find(|c| c.id == conversation_id)
                        .and_then(|c| c.messages.last())
                        .filter(|m| !m.is_me)
                        .map(|m| (m.content.clone(), m.parts.clone()));
                    if let Some((content, parts)) = reply {
                        state.persist_assistant_reply(&conversation_id, content, &parts, cx);
                    }
                }
                if waiting_approval {
                    let open = state
                        .conversations
                        .iter()
                        .find(|c| c.id == conversation_id)
                        .and_then(|c| c.messages.iter().rev().find(|m| !m.is_me))
                        .and_then(|m| {
                            m.parts.iter().rev().find_map(|part| match part {
                                ChatPart::Approval(spec) => Some(spec.clone()),
                                _ => None,
                            })
                        });
                    let auto = open
                        .as_ref()
                        .and_then(|spec| state.auto_resolve_local_exec(spec))
                        .zip(open);
                    if let Some((resolution, spec)) = auto {
                        state.answer_approval(spec, resolution, cx);
                        state.finish_responding(turn_id.as_deref(), false);
                    } else {
                        state.finish_responding(turn_id.as_deref(), true);
                        state.fill_open_approval_commands(cx);
                        state.sync_pending_approvals(cx);
                    }
                } else {
                    state.finish_responding(turn_id.as_deref(), false);
                }
                // A failed run already ended the last assistant row with "OpenGrok: <why>";
                // `auth_error` is the sign-in / settings error and the settings pane paints it,
                // so a run's failure must not land there too.
                if let Err(error) = result {
                    eprintln!("NativeChat: the turn failed: {}", error.message);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn answer_approval(
        &mut self,
        spec: ApprovalSpec,
        resolution: LocalExecResolution,
        cx: &mut Context<Self>,
    ) {
        if self.approval_answered(&spec.call_id) {
            return;
        }
        self.approval_decisions
            .insert(spec.call_id.clone(), ApprovalDecision::Sending);
        self.drop_other_pending_approvals(&spec.call_id);
        cx.notify();
        let Some(client) = self.opengrok.clone() else {
            self.approval_decisions.insert(
                spec.call_id.clone(),
                ApprovalDecision::Failed("OpenGrok is not configured".into()),
            );
            return;
        };
        let machine_id = self
            .local_exec_machine_id
            .clone()
            .or_else(|| {
                self.computers
                    .iter()
                    .find(|computer| computer.this_machine)
                    .map(|computer| computer.machine_id.clone())
            })
            .or_else(|| {
                self.config
                    .as_ref()
                    .and_then(|config| stored_machine_id(&config.data_dir))
            })
            .or_else(|| {
                self.computers
                    .first()
                    .map(|computer| computer.machine_id.clone())
            });
        let conversation_id = self.active_conversation_id.clone();
        let (approved, decision, mode) = match resolution {
            LocalExecResolution::Always => (true, ApprovalDecision::Always, Some("bypass")),
            LocalExecResolution::AllowOnce => (true, ApprovalDecision::AllowOnce, None),
            LocalExecResolution::Never => (false, ApprovalDecision::Never, Some("never")),
            LocalExecResolution::DenyOnce => (false, ApprovalDecision::Denied, None),
        };
        // Only the local-shell tool can move this Mac's policy.
        let mode = mode.filter(|_| spec.runs_on_this_mac());
        if let (Some(machine_id), Some(stored)) = (machine_id.as_ref(), mode) {
            if let Some(computer) = self
                .computers
                .iter_mut()
                .find(|computer| &computer.machine_id == machine_id)
            {
                computer.mode = LocalExecMode::from_stored(stored);
            }
            cx.notify();
        }
        let run_id_empty = spec.run_id.trim().is_empty();
        cx.spawn(async move |this, cx| {
            if let (Some(machine_id), Some(mode)) = (machine_id.as_deref(), mode) {
                let _ = client.set_local_exec_mode(machine_id, mode).await;
                let _ = this.update(cx, |state, cx| {
                    state.refresh_computers(cx);
                });
            }
            if run_id_empty {
                let _ = this.update(cx, |state, cx| {
                    state.drop_dead_approval(&spec.call_id);
                    state
                        .approval_decisions
                        .insert(spec.call_id.clone(), decision);
                    cx.notify();
                });
                return;
            }
            let mut run_id = spec.run_id.clone();
            if let Ok(queue) = client.list_approvals().await
                && let Some(item) = queue.iter().find(|item| item.call_id == spec.call_id)
            {
                run_id = item.run_id.clone();
            }
            let result = client.answer_run(&run_id, &spec.call_id, approved).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(_) => {
                        state
                            .approval_decisions
                            .insert(spec.call_id.clone(), decision);
                        let coworker_id = state.active_coworker_id.clone();
                        if approved {
                            state.begin_responding(coworker_id.as_deref(), "Running commands");
                            state.follow_answered_run(
                                run_id.clone(),
                                conversation_id,
                                coworker_id,
                                cx,
                            );
                        } else {
                            state.finish_responding(coworker_id.as_deref(), false);
                        }
                    }
                    Err(error) => {
                        if error.message.contains("no such run") {
                            state.drop_dead_approval(&spec.call_id);
                        } else {
                            state.approval_decisions.insert(
                                spec.call_id.clone(),
                                ApprovalDecision::Failed(error.message),
                            );
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn follow_answered_run(
        &mut self,
        run_id: String,
        conversation_id: Option<String>,
        coworker_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let mut last_len = 0usize;
            for _ in 0..400 {
                match client.replay_run(&run_id).await {
                    Ok(replay) => {
                        if replay.events.len() != last_len {
                            last_len = replay.events.len();
                            let mut assembler = TurnAssembler::default();
                            for event in &replay.events {
                                assembler.push_event(event);
                            }
                            assembler.finish();
                            let (plain, parts) = assembler.snapshot();
                            let status = replay.status.clone();
                            // A resumed turn that only ran tools says what it did, so the
                            // thread keeps a memory of the run the person allowed.
                            let plain = if status == "finished" && plain.trim().is_empty() {
                                tool_standin(&deeds_from_replay(&replay.events)).unwrap_or_default()
                            } else {
                                plain
                            };
                            // The journal says what the run is doing now; "Working" only
                            // when no frame has said.
                            let activity =
                                activity_from_replay(&replay.events).unwrap_or(BotActivity {
                                    label: "Working".into(),
                                });
                            let _ = this.update(cx, |state, cx| {
                                if let Some(conversation_id) = conversation_id.as_ref()
                                    && let Some(conversation) = state
                                        .conversations
                                        .iter_mut()
                                        .find(|c| &c.id == conversation_id)
                                    && let Some(last) =
                                        conversation.messages.iter_mut().rev().find(|m| !m.is_me)
                                {
                                    last.content = plain.clone();
                                    last.parts = parts.clone();
                                    for part in &parts {
                                        if let ChatPart::Approval(spec) = part
                                            && spec.output.is_some()
                                        {
                                            state.approval_decisions.insert(
                                                spec.call_id.clone(),
                                                ApprovalDecision::AllowOnce,
                                            );
                                        }
                                    }
                                }
                                match status.as_str() {
                                    "awaiting-approval" => {
                                        state.finish_responding(coworker_id.as_deref(), true);
                                    }
                                    "running" => {
                                        state.apply_turn_status(
                                            coworker_id.as_deref(),
                                            ActivityTick::Set(activity.clone()),
                                        );
                                    }
                                    "finished" => {
                                        state.finish_responding(coworker_id.as_deref(), false);
                                        if let Some(id) = conversation_id.as_ref() {
                                            state.persist_assistant_reply(
                                                id,
                                                plain.clone(),
                                                &parts,
                                                cx,
                                            );
                                        }
                                    }
                                    "failed" => {
                                        state.finish_responding(coworker_id.as_deref(), false);
                                    }
                                    _ => {}
                                }
                                cx.notify();
                            });
                        }
                        match replay.status.as_str() {
                            "finished" | "failed" => break,
                            "awaiting-approval" => {
                                let _ = this.update(cx, |state, cx| {
                                    state.finish_responding(coworker_id.as_deref(), true);
                                    cx.notify();
                                });
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(_) => break,
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            let _ = this.update(cx, |state, cx| {
                if !matches!(state.bot_status.as_deref(), Some("Waiting for approval")) {
                    state.finish_responding(coworker_id.as_deref(), false);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn fill_approval_command(&mut self, call_id: &str, command: String) {
        if command.trim().is_empty() {
            return;
        }
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                for part in &mut message.parts {
                    if let ChatPart::Approval(spec) = part
                        && spec.call_id == call_id
                        && spec.command.is_empty()
                    {
                        spec.command = command.clone();
                    }
                }
            }
        }
    }

    fn fill_open_approval_commands(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let mut jobs = Vec::new();
        for conversation in &self.conversations {
            for message in &conversation.messages {
                for part in &message.parts {
                    if let ChatPart::Approval(spec) = part
                        && spec.command.is_empty()
                        && !spec.run_id.is_empty()
                    {
                        jobs.push((spec.run_id.clone(), spec.call_id.clone()));
                    }
                }
            }
        }
        if jobs.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            for (run_id, call_id) in jobs {
                let mut command = String::new();
                if let Ok(replay) = client.replay_run(&run_id).await {
                    command = command_from_replay_events(&replay.events, &call_id);
                    if command.is_empty()
                        && let Some(pending) = replay.pending
                    {
                        let pending_id = pending
                            .get("call_id")
                            .or_else(|| pending.get("callId"))
                            .and_then(serde_json::Value::as_str);
                        if pending_id.is_none_or(|id| id == call_id) {
                            command = command_from_args(
                                pending.get("arguments").unwrap_or(&serde_json::Value::Null),
                            );
                        }
                    }
                }
                if command.is_empty()
                    && let Ok(queue) = client.list_approvals().await
                    && let Some(item) = queue.iter().find(|item| item.call_id == call_id)
                {
                    command = command_from_args(&item.arguments);
                }
                if command.is_empty() {
                    continue;
                }
                let _ = this.update(cx, |state, cx| {
                    state.fill_approval_command(&call_id, command);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn sync_pending_approvals(&mut self, cx: &mut Context<Self>) {
        if self.is_ai_responding {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let thread_id = self.active_conversation_id.clone();
        cx.spawn(async move |this, cx| {
            let Ok(queue) = client.list_approvals().await else {
                return;
            };
            let _ = this.update(cx, |state, cx| {
                // The policy answers what it covers; the rest wait for a card.
                let mut needs_card = Vec::new();
                for item in queue {
                    if state.approval_answered(&item.call_id) {
                        continue;
                    }
                    let spec = spec_from_queued(&item);
                    match state.auto_resolve_local_exec(&spec) {
                        Some(resolution) => state.answer_approval(spec, resolution, cx),
                        None => needs_card.push(item),
                    }
                }
                if state.is_ai_responding {
                    return;
                }
                let Some(thread_id) = thread_id.as_deref() else {
                    return;
                };
                let Some(item) = QueuedApproval::latest_for_thread(&needs_card, thread_id) else {
                    return;
                };
                if item.run_id.trim().is_empty() {
                    return;
                }
                state.attach_queued_approval(item.clone());
                state.responding_coworker_id = Some(thread_id.to_string());
                state.bot_status = Some("Waiting for approval".into());
                cx.notify();
            });
        })
        .detach();
    }

    fn drop_dead_approval(&mut self, call_id: &str) {
        self.approval_decisions.remove(call_id);
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                message.parts.retain(
                    |part| !matches!(part, ChatPart::Approval(spec) if spec.call_id == call_id),
                );
            }
        }
    }

    fn drop_other_pending_approvals(&mut self, keep_call_id: &str) {
        let decisions = &self.approval_decisions;
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                message.parts.retain(|part| match part {
                    ChatPart::Approval(spec) => {
                        spec.call_id == keep_call_id
                            || decisions
                                .get(&spec.call_id)
                                .is_some_and(ApprovalDecision::is_answered)
                    }
                    _ => true,
                });
            }
        }
    }

    fn attach_queued_approval(&mut self, item: QueuedApproval) {
        let spec = spec_from_queued(&item);
        self.approval_decisions
            .entry(spec.call_id.clone())
            .or_insert(ApprovalDecision::Pending);
        let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == item.thread_id)
        else {
            return;
        };
        if let Some(last) = conversation.messages.iter_mut().rev().find(|m| !m.is_me) {
            if let Some(ChatPart::Approval(existing)) = last.parts.iter_mut().find(|part| {
                matches!(part, ChatPart::Approval(existing) if existing.call_id == spec.call_id
                    || existing.run_id == spec.run_id)
            }) {
                if existing.command.is_empty() && !spec.command.is_empty() {
                    existing.command = spec.command;
                }
                if existing.run_id.is_empty() {
                    existing.run_id = spec.run_id;
                }
                return;
            }
            if last
                .parts
                .iter()
                .any(|part| matches!(part, ChatPart::Approval(_)))
            {
                return;
            }
            last.parts.push(ChatPart::Approval(spec));
            return;
        }
        conversation.messages.push(Message {
            id: uuid::Uuid::now_v7().to_string(),
            sender: "AI".to_string(),
            content: String::new(),
            sent_at: SystemTime::now(),
            is_me: false,
            reply_preview: None,
            reply_to_id: None,
            reply_is_me: false,
            parts: vec![ChatPart::Approval(spec)],
        });
    }

    fn start_local_exec(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(config) = self.config.clone() else {
            return;
        };
        self.stop_local_exec();
        let cancel = Arc::new(AtomicBool::new(false));
        self.local_exec_cancel = Some(cancel.clone());
        cx.spawn(async move |this, cx| {
            match enrol_this_machine(&client, &config.data_dir).await {
                Ok(machine_id) => {
                    let _ = this.update(cx, |state, cx| {
                        state.local_exec_machine_id = Some(machine_id);
                        state.refresh_computers(cx);
                        cx.notify();
                    });
                }
                Err(error) => {
                    eprintln!("NativeChat local-exec: {error}");
                }
            }
            serve_local_exec(client, config.data_dir, cancel).await;
        })
        .detach();
    }

    fn stop_local_exec(&mut self) {
        if let Some(cancel) = &self.local_exec_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.local_exec_cancel = None;
        self.local_exec_machine_id = None;
    }

    pub fn toggle_shell_output(&mut self, call_id: String, cx: &mut Context<Self>) {
        if !self.expanded_shell_output.remove(&call_id) {
            self.expanded_shell_output.insert(call_id);
        }
        cx.notify();
    }

    pub fn pick_form_option(
        &mut self,
        message_id: String,
        field_id: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.form_picks
            .entry(message_id)
            .or_default()
            .insert(field_id, value);
        cx.notify();
    }

    pub fn submit_form(&mut self, message_id: String, spec: FormSpec, cx: &mut Context<Self>) {
        let picks = self
            .form_picks
            .get(&message_id)
            .cloned()
            .unwrap_or_default();
        let mut lines = Vec::new();
        if let Some(title) = &spec.title {
            lines.push(title.clone());
        }
        for field in &spec.fields {
            if let Some(value) = picks.get(&field.id) {
                lines.push(format!("{}: {value}", field.label));
            }
        }
        let body = lines.join("\n");
        if body.trim().is_empty() {
            return;
        }
        self.send_message(body, cx);
    }

    pub fn send_message(&mut self, content: String, cx: &mut Context<Self>) {
        if !self.is_signed_in() {
            self.auth_error = Some("Sign in first".to_string());
            cx.notify();
            return;
        }
        if self.active_coworker_id.is_none() {
            self.auth_error = Some("Create a bot first".to_string());
            cx.notify();
            return;
        }
        let conversation_id = match &self.active_conversation_id {
            Some(id) => id.clone(),
            None => return,
        };

        let local_id = uuid::Uuid::now_v7().to_string();
        // The whole reply, not just its preview: the bubble paints the preview, and the quote
        // the coworker is sent is built from the message this one points at.
        let reply = self.reply_to.take();
        // Add user message to UI immediately
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            let message = Message {
                id: local_id.clone(),
                sender: "Me".to_string(),
                content: content.clone(),
                sent_at: SystemTime::now(),
                is_me: true,
                reply_preview: reply.as_ref().map(|r| r.preview.clone()),
                reply_to_id: reply.as_ref().map(|r| r.message_id.clone()),
                reply_is_me: reply.as_ref().is_some_and(|r| r.is_me),
                parts: Vec::new(),
            };
            conversation.messages.push(message);
        }
        if let Some(id) = self.active_coworker_id.clone() {
            self.touch_coworker_activity(&id);
        }
        cx.notify();

        // Save to DB
        if let Some(db) = self.database_service.clone() {
            let content_clone = content.clone();
            let conversation_id_clone = conversation_id.clone();
            let title = self.conversation_title(&conversation_id);
            let local_id = local_id.clone();
            let reply = reply.map(|reply| ReplyRef {
                message_id: reply.message_id,
                preview: reply.preview,
                is_me: reply.is_me,
            });
            cx.spawn(async move |this, cx| {
                // A coworker thread has no session row until it first speaks.
                if let Err(e) = db.ensure_session(&conversation_id_clone, &title).await {
                    eprintln!("Failed to save user message: {}", e);
                    return;
                }
                match db
                    .save_message(
                        &conversation_id_clone,
                        "user",
                        &content_clone,
                        None,
                        None,
                        reply,
                        &[],
                    )
                    .await
                {
                    Ok(id) => {
                        this.update(cx, |state, cx| {
                            if let Some(conversation) = state
                                .conversations
                                .iter_mut()
                                .find(|c| c.id == conversation_id_clone)
                            {
                                if let Some(msg) = conversation
                                    .messages
                                    .iter_mut()
                                    .rev()
                                    .find(|m| m.id == local_id)
                                {
                                    msg.id = id;
                                }
                            }
                        })
                        .ok();
                    }
                    Err(e) => eprintln!("Failed to save user message: {}", e),
                }
            })
            .detach();
        }

        if self.has_open_approval(&conversation_id) {
            self.bot_status = Some("Waiting for approval".into());
            cx.notify();
            return;
        }
        if self.is_active_bot_responding() {
            cx.notify();
            return;
        }
        self.send_opengrok_turn(conversation_id, content, cx);
    }

    fn has_open_approval(&self, conversation_id: &str) -> bool {
        let Some(conversation) = self
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        conversation.messages.iter().rev().any(|message| {
            !message.is_me
                && message.parts.iter().any(|part| match part {
                    ChatPart::Approval(spec) => {
                        !spec.run_id.is_empty()
                            && !self.approval_answered(&spec.call_id)
                            && self.auto_resolve_local_exec(spec).is_none()
                    }
                    _ => false,
                })
        })
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_hidden = !self.sidebar_hidden;
        cx.notify();
    }

    pub fn toggle_mini_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_hidden {
            self.sidebar_hidden = false;
            self.sidebar_collapsed = false;
        } else {
            self.sidebar_collapsed = !self.sidebar_collapsed;
        }
        self.auto_collapsed = false;
        self.sidebar_responsive = remember_choice(self.sidebar_responsive, self.sidebar_collapsed);
        cx.notify();
    }

    pub fn resize_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        let next = sidebar_from_resize(
            SidebarChrome {
                hidden: self.sidebar_hidden,
                collapsed: self.sidebar_collapsed,
                expanded_width: self.sidebar_expanded_width,
            },
            width,
        );
        self.sidebar_hidden = next.hidden;
        self.sidebar_collapsed = next.collapsed;
        self.sidebar_expanded_width = next.expanded_width;
        self.auto_collapsed = false;
        if !next.hidden {
            self.sidebar_responsive = remember_choice(self.sidebar_responsive, next.collapsed);
        }
        cx.notify();
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool, auto: bool, cx: &mut Context<Self>) {
        self.sidebar_collapsed = collapsed;
        self.auto_collapsed = auto;
        self.sidebar_responsive = remember_choice(self.sidebar_responsive, collapsed);
        cx.notify();
    }

    pub fn apply_responsive_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        let result = collapse_for_width(self.sidebar_responsive, width, self.sidebar_collapsed);
        self.sidebar_responsive = result.next;
        if let Some(apply) = result.apply {
            if self.sidebar_collapsed != apply {
                self.sidebar_collapsed = apply;
                self.auto_collapsed = true;
                cx.notify();
            }
        }
    }

    pub fn toggle_debug_markdown(&mut self, cx: &mut Context<Self>) {
        self.debug_markdown_disabled = !self.debug_markdown_disabled;
        println!(
            "[DEBUG] Markdown rendering: {}",
            if self.debug_markdown_disabled {
                "DISABLED (plain text)"
            } else {
                "ENABLED"
            }
        );
        cx.notify();
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        let next = match self.theme_mode.as_str() {
            "light" => "dark",
            "dark" => "system",
            _ => "light",
        };
        self.set_theme_mode(next, cx);
    }

    pub fn set_theme_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        use gpui_kit::component::{Theme, ThemeRegistry};

        self.theme_mode = mode.to_string();
        let theme_name = match self.theme_mode.as_str() {
            "light" => "macOS Classic Light",
            "dark" => "macOS Classic Dark",
            "system" => "macOS Classic Dark",
            _ => "macOS Classic Light",
        };
        if let Some(theme) = ThemeRegistry::global(cx)
            .themes()
            .get(&SharedString::from(theme_name))
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&theme);
            Theme::sync_base(cx);
        }
        cx.notify();
    }

    pub fn toggle_app_settings(&mut self, cx: &mut Context<Self>) {
        self.is_app_settings_open = !self.is_app_settings_open;
        if self.is_app_settings_open {
            self.dismiss_popovers(cx);
        }
        self.record_nav();
        cx.notify();
    }

    pub fn set_app_settings_tab(&mut self, tab: AppSettingsTab, cx: &mut Context<Self>) {
        if self.app_settings_tab != tab {
            self.app_settings_tab = tab;
            self.record_nav();
            if tab == AppSettingsTab::Computer {
                self.refresh_computers(cx);
            }
            cx.notify();
        }
    }

    pub fn refresh_computers(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        // A roster reload is the moment a server upgrade would show; ask again.
        self.computer_endpoint_missing = false;
        let this_id = self.local_exec_machine_id.clone();
        cx.spawn(async move |this, cx| {
            let Ok(mut computers) = client.list_computers().await else {
                return;
            };
            for computer in &mut computers {
                computer.this_machine = this_id.as_ref() == Some(&computer.machine_id);
                if computer.this_machine {
                    computer.online = true;
                }
            }
            computers.sort_by_key(|computer| !computer.this_machine);
            let _ = this.update(cx, |state, cx| {
                state.computers = computers;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn set_computer_exec_mode(
        &mut self,
        machine_id: String,
        mode: LocalExecMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(computer) = self
            .computers
            .iter_mut()
            .find(|computer| computer.machine_id == machine_id)
        {
            computer.mode = mode;
        }
        cx.notify();
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let stored = mode.as_stored();
            let ok = client
                .set_local_exec_mode(&machine_id, stored)
                .await
                .is_ok();
            let confirmed = if ok {
                client
                    .local_exec_mode(&machine_id)
                    .await
                    .ok()
                    .map(|mode| LocalExecMode::from_stored(&mode))
            } else {
                None
            };
            let _ = this.update(cx, |state, cx| {
                if confirmed != Some(mode) {
                    state.refresh_computers(cx);
                    return;
                }
                if mode == LocalExecMode::Always {
                    state.sync_pending_approvals(cx);
                }
            });
        })
        .detach();
    }

    pub fn set_submit_chord(&mut self, chord: SubmitChord, cx: &mut Context<Self>) {
        if self.submit_chord != chord {
            self.submit_chord = chord;
            cx.notify();
        }
    }

    pub fn set_voice_mode(&mut self, open: bool, cx: &mut Context<Self>) {
        self.is_voice_mode_open = open;
        if !open {
            self.stop_voice_mode(cx);
        }
        cx.notify();
    }

    pub fn toggle_voice_mute(&mut self, cx: &mut Context<Self>) {
        self.is_voice_muted = !self.is_voice_muted;
        cx.notify();
    }

    pub fn open_app_settings(&mut self, tab: AppSettingsTab, cx: &mut Context<Self>) {
        self.app_settings_tab = tab;
        if !self.is_app_settings_open {
            self.is_app_settings_open = true;
            self.dismiss_popovers(cx);
        }
        if tab == AppSettingsTab::Computer {
            self.refresh_computers(cx);
        }
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_account_settings(&mut self, cx: &mut Context<Self>) {
        self.open_app_settings(AppSettingsTab::Profile, cx);
    }

    pub fn start_voice_mode(&mut self, cx: &mut Context<Self>) {
        self.is_voice_mode_open = true;
        self.voice_status = VoiceStatus::Connecting;
        cx.notify();

        match AudioInput::new(self.amplitude.clone()) {
            Ok(input) => {
                self.audio_input = Some(input);
                self.voice_status = VoiceStatus::Connected;
            }
            Err(e) => {
                eprintln!("Failed to start local audio input: {}", e);
                self.voice_status = VoiceStatus::Error("Mic Error".to_string());
            }
        }
        cx.notify();
    }

    pub fn stop_voice_mode(&mut self, cx: &mut Context<Self>) {
        self.is_voice_mode_open = false;
        self.voice_status = VoiceStatus::Disconnected;
        self.audio_input = None;
        cx.notify();
    }

    pub fn warm_tts(&mut self, cx: &mut Context<Self>) {
        self.ensure_tts_service(cx);
    }

    fn ensure_tts_service(&mut self, cx: &mut Context<Self>) {
        if self.tts_service.is_some() || self.tts_initing {
            return;
        }
        self.tts_initing = true;
        cx.spawn(async move |this, cx| {
            let service = cx
                .background_executor()
                .spawn(async move {
                    let service = TtsService::new();
                    service.warm_native();
                    service
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                state.tts_initing = false;
                state.tts_service = Some(service);
                if let Some((text, message_id)) = state.pending_read_aloud.take() {
                    state.read_aloud(text, message_id, TtsSource::Native, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn read_aloud(
        &mut self,
        text: String,
        message_id: String,
        source: TtsSource,
        cx: &mut Context<Self>,
    ) {
        let _ = source;
        if self.tts_service.is_none() {
            self.pending_read_aloud = Some((text.clone(), message_id.clone()));
            self.native_tts.message_id = Some(message_id);
            self.native_tts.is_loading = true;
            self.native_tts.is_paused = false;
            self.ensure_tts_service(cx);
            cx.notify();
            return;
        }

        if let Some(service) = &self.tts_service {
            self.native_tts.message_id = Some(message_id.clone());
            self.native_tts.is_paused = false;
            self.native_tts.is_loading = false;
            cx.notify();

            let service = service.clone();
            let message_id = message_id.clone();
            let text = text.clone();

            if service.start_speaking_native(&text, &message_id) {
                cx.spawn(async move |this, cx| {
                    service.wait_until_finished_native().await;
                    if let Some(this) = this.upgrade() {
                        let _ = this.update(cx, |state, cx| {
                            if state.native_tts.message_id.as_ref() == Some(&message_id) {
                                state.native_tts.message_id = None;
                                cx.notify();
                            }
                        });
                    }
                })
                .detach();
            }
        }
    }

    pub fn stop_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            service.stop_native();
            self.native_tts = SourceTtsState::default();
            cx.notify();
        }
    }

    pub fn pause_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            if self.native_tts.message_id.is_some() && !self.native_tts.is_paused {
                service.pause_native();
                self.native_tts.is_paused = service
                    .native_paused
                    .load(std::sync::atomic::Ordering::SeqCst);
            }
            cx.notify();
        }
    }

    pub fn resume_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            if self.native_tts.message_id.is_some() && self.native_tts.is_paused {
                service.resume_native();
                self.native_tts.is_paused = false;
            }
            cx.notify();
        }
    }

    pub fn active_highlight_range(&self) -> Option<std::ops::Range<usize>> {
        self.tts_service
            .as_ref()
            .and_then(|s| s.get_active_word_range())
    }

    /// The one entry the menu and F8 use: reads a message; on the message being read, pauses;
    /// on a paused one, resumes. A message still loading its voice is left alone, so a double
    /// click cannot pause a synthesizer that has not started (which would wedge the next start).
    pub fn toggle_read_aloud(
        &mut self,
        message_id: String,
        text: String,
        mode: TtsSource,
        cx: &mut Context<Self>,
    ) {
        let _ = mode;
        if self.native_tts.message_id.as_ref() == Some(&message_id) {
            if self.native_tts.is_loading {
                return;
            }
            if self.native_tts.is_paused {
                self.resume_read_aloud(cx);
            } else {
                self.pause_read_aloud(cx);
            }
            return;
        }
        self.read_aloud(text, message_id, TtsSource::Native, cx);
    }

    /// Whether this message is the one being read, and whether it is paused.
    pub fn read_aloud_state(&self, message_id: &str) -> (bool, bool) {
        let reading = self.native_tts.message_id.as_deref() == Some(message_id);
        (reading, reading && self.native_tts.is_paused)
    }
}

fn spec_from_queued(item: &QueuedApproval) -> ApprovalSpec {
    ApprovalSpec {
        run_id: item.run_id.clone(),
        call_id: item.call_id.clone(),
        tool: item.tool.clone(),
        command: command_from_args(&item.arguments),
        why: "your machine's owner must approve this command".into(),
        reason: "exec-consent".into(),
        output: None,
        ok: None,
    }
}

/// One of a coworker's optional fields as the roster keeps it. The app asks for such a field
/// to be cleared by patching it with nothing in it.
fn some_unless_blank(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

/// The roster's copy of a coworker brought up to what a patch asks for, before the server has
/// been asked at all.
fn apply_patch(coworker: &mut Coworker, patch: &CoworkerPatch) {
    if let Some(name) = patch.name.as_ref() {
        coworker.name = name.clone();
    }
    if let Some(model) = patch.model.as_ref() {
        coworker.model = model.clone();
    }
    if let Some(role) = patch.role.as_deref() {
        coworker.role = some_unless_blank(role);
    }
    if let Some(title) = patch.title.as_deref() {
        coworker.title = some_unless_blank(title);
    }
    if let Some(shape) = patch.avatar_shape.as_deref() {
        coworker.avatar_shape = some_unless_blank(shape);
    }
    if let Some(color) = patch.avatar_color.as_deref() {
        coworker.avatar_color = some_unless_blank(color);
    }
    if let Some(notify) = patch.notify_on_updates {
        coworker.notify_on_updates = Some(notify);
    }
}

/// The roster's copy of a coworker once the server has answered, for the fields the patch
/// asked about and no others.
///
/// What was written optimistically is not evidence of anything: the server may have refused
/// the patch, or taken the request and stored none of it, which is what this one still does
/// with several of these fields. So a field the server echoed back is the server's, and a
/// field it said nothing about goes back to what the roster held before the request. The value
/// the app guessed is kept nowhere. `echo` is `None` when the request failed, which leaves
/// every patched field as it was.
///
/// A patch that asks for a field to be cleared is the one place where a silent answer is
/// taken for agreement: there is no value to be wrong about, and a server that leaves empty
/// fields out of its answer would otherwise make clearing an avatar impossible. A refused
/// request is not a silent answer but no answer at all, and takes the clearing back with
/// everything else.
fn settle_patch(
    coworker: &mut Coworker,
    patch: &CoworkerPatch,
    echo: Option<&Coworker>,
    before: &Coworker,
) {
    let answered = echo.is_some();
    if patch.name.is_some() {
        coworker.name = settled_text(echo.map(|c| c.name.as_str()), &before.name);
    }
    if patch.model.is_some() {
        coworker.model = settled_text(echo.map(|c| c.model.as_str()), &before.model);
    }
    if let Some(role) = patch.role.as_deref() {
        coworker.role = settled_option(
            echo.and_then(|c| c.role.clone()),
            answered && some_unless_blank(role).is_none(),
            before.role.clone(),
        );
    }
    if let Some(title) = patch.title.as_deref() {
        coworker.title = settled_option(
            echo.and_then(|c| c.title.clone()),
            answered && some_unless_blank(title).is_none(),
            before.title.clone(),
        );
    }
    if let Some(shape) = patch.avatar_shape.as_deref() {
        coworker.avatar_shape = settled_option(
            echo.and_then(|c| c.avatar_shape.clone()),
            answered && some_unless_blank(shape).is_none(),
            before.avatar_shape.clone(),
        );
    }
    if let Some(color) = patch.avatar_color.as_deref() {
        coworker.avatar_color = settled_option(
            echo.and_then(|c| c.avatar_color.clone()),
            answered && some_unless_blank(color).is_none(),
            before.avatar_color.clone(),
        );
    }
    if patch.notify_on_updates.is_some() {
        coworker.notify_on_updates = echo
            .and_then(|c| c.notify_on_updates)
            .or(before.notify_on_updates);
    }
}

/// A field a coworker always has, after the server has answered. An answer that left the field
/// out says nothing about it, and nothing is not a name or a model.
fn settled_text(echo: Option<&str>, before: &str) -> String {
    echo.filter(|text| !text.is_empty())
        .unwrap_or(before)
        .to_string()
}

/// A field a coworker may not have, after the server has answered.
fn settled_option(echo: Option<String>, cleared: bool, before: Option<String>) -> Option<String> {
    match echo {
        Some(value) => Some(value),
        None if cleared => None,
        None => before,
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_picked_tool_knows_whether_it_is_a_tool_or_an_app() {
        // The chip row used to read the kind off a hardcoded list of menu names. A real tool's
        // name comes from the server, so the rule is the one the server itself uses: a
        // qualified name belongs to a plugin, which is what a person means by an app.
        assert_eq!(PickedKind::of("shell"), PickedKind::Tool);
        assert_eq!(PickedKind::of("run_recipe"), PickedKind::Tool);
        assert_eq!(PickedKind::of("gmail.api.send"), PickedKind::App);
    }
    // Item by item rather than a glob: `use super::*` would drag in gpui_kit's own `test`.
    use super::{
        ChatPart, DatabaseService, EMPTY_TURN_NOTE, Message, PickedKind, REPLY_QUOTE_CHARS,
        agui_messages, is_status_line, is_tool_standin, restored_parts, saved_parts,
    };
    use std::str::FromStr;
    use std::sync::Arc;
    use std::time::SystemTime;

    fn message(id: &str, is_me: bool, content: &str) -> Message {
        Message {
            id: id.to_string(),
            sender: if is_me { "Me" } else { "AI" }.to_string(),
            content: content.to_string(),
            sent_at: SystemTime::UNIX_EPOCH,
            is_me,
            reply_preview: None,
            reply_to_id: None,
            reply_is_me: false,
            parts: Vec::new(),
        }
    }

    fn replying_to(mut message: Message, quoted: &Message) -> Message {
        message.reply_to_id = Some(quoted.id.clone());
        message.reply_preview = Some(quoted.content.clone());
        message.reply_is_me = quoted.is_me;
        message
    }

    /// The whole point: "what am I replying to?" must arrive with the quote, because the server
    /// answers from the array the app sends and nothing else.
    #[test]
    fn a_reply_reaches_the_coworker_as_a_quote_ahead_of_its_own_words() {
        let bot = message("m1", false, "The build is green.");
        let reply = replying_to(message("m2", true, "what am I replying to?"), &bot);
        let sent = agui_messages(&[bot, reply]);
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent[1].content,
            "[Replying to your earlier message: \"The build is green.\"]\n\nwhat am I replying to?"
        );
        let quote = sent[1].reply_to.as_ref().expect("the field is filled too");
        assert_eq!(quote.message_id, "m1");
        assert_eq!(quote.preview, "The build is green.");
        assert!(!quote.is_me);
    }

    #[test]
    fn replying_to_your_own_message_is_told_apart_from_replying_to_the_bot() {
        let mine = message("m1", true, "remind me at five");
        let reply = replying_to(message("m2", true, "make that six"), &mine);
        let sent = agui_messages(&[mine, reply]);
        assert_eq!(
            sent[1].content,
            "[Replying to their own earlier message: \"remind me at five\"]\n\nmake that six"
        );
        assert!(sent[1].reply_to.as_ref().expect("a quote").is_me);
    }

    /// A reply to a long answer names it, it does not replay it.
    #[test]
    fn a_long_quote_is_clipped() {
        let bot = message("m1", false, &"x".repeat(REPLY_QUOTE_CHARS + 50));
        let reply = replying_to(message("m2", true, "go on"), &bot);
        let sent = agui_messages(&[bot, reply]);
        let quote = sent[1].reply_to.as_ref().expect("a quote");
        assert_eq!(quote.preview.chars().count(), REPLY_QUOTE_CHARS + 1);
        assert!(quote.preview.ends_with('…'));
    }

    /// A message can be deleted after it was answered; the preview saved with the reply is what
    /// is left of it.
    #[test]
    fn a_quote_whose_message_is_gone_falls_back_to_the_saved_preview() {
        let mut reply = message("m2", true, "why?");
        reply.reply_to_id = Some("deleted".to_string());
        reply.reply_preview = Some("The build is green.".to_string());
        let sent = agui_messages(&[reply]);
        assert_eq!(
            sent[0].content,
            "[Replying to your earlier message: \"The build is green.\"]\n\nwhy?"
        );
    }

    /// The app's own words about a turn are not a turn: sending them back would have the model
    /// answering a line nobody said.
    #[test]
    fn the_apps_status_lines_are_never_sent_back_as_the_coworkers_words() {
        let thread = vec![
            message("m1", true, "hi"),
            message("m2", false, EMPTY_TURN_NOTE),
            message("m3", true, "still there?"),
            message("m4", false, "OpenGrok: the model returned no text"),
            message("m5", false, "[took a screenshot of my screen]"),
        ];
        let sent = agui_messages(&thread);
        let kept: Vec<&str> = sent.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(
            kept,
            vec!["hi", "still there?", "[took a screenshot of my screen]"]
        );
    }

    /// The app's own schema on a database that lives for the length of the test. One connection:
    /// a second connection to `:memory:` would open a second, empty database.
    async fn test_db() -> DatabaseService {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("an in-memory database")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("a pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("the app's schema");
        DatabaseService::new(pool)
    }

    /// Bytes stand in for a PNG: nothing in this path decodes one, and distinct bytes prove the
    /// right picture came back under the right caption.
    fn screenshot(call_id: &str, caption: &str, bytes: &[u8], size: (u32, u32)) -> ChatPart {
        ChatPart::Screenshot(crate::opengrok::ScreenshotSpec {
            call_id: call_id.to_string(),
            caption: caption.to_string(),
            image: Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Png,
                bytes.to_vec(),
            )),
            width: size.0,
            height: size.1,
        })
    }

    /// Order, kind and contents in one line per part. `ChatPart`'s own `PartialEq` takes one
    /// screenshot per call id on trust and never looks at the bytes, so the bytes are spelled
    /// out here instead.
    fn shape(parts: &[ChatPart]) -> Vec<String> {
        parts
            .iter()
            .map(|part| match part {
                ChatPart::Text(text) => format!("text {text}"),
                ChatPart::Screenshot(spec) => format!(
                    "shot {} {}x{} {:?} {}",
                    spec.call_id, spec.width, spec.height, spec.image.bytes, spec.caption
                ),
                ChatPart::Ui(_) => "ui".to_string(),
                ChatPart::Approval(_) => "approval".to_string(),
            })
            .collect()
    }

    /// The bug the person reported: a recipe run is several bubbles with pictures of the box's
    /// screen between them, and switching to another bot and back brought it all back as one
    /// paragraph with every picture gone.
    #[tokio::test]
    async fn a_turn_of_words_and_pictures_comes_back_the_way_it_was_seen() {
        let db = test_db().await;
        db.ensure_session("s1", "Recipes").await.expect("a session");
        let live = vec![
            ChatPart::Text("I'll open YouTube on my box using the taught recipe.".to_string()),
            screenshot(
                "c1",
                "ran recipe \"youtube\" (v2): 6 steps; screenshot of the screen afterwards attached",
                b"png-one",
                (1280, 800),
            ),
            ChatPart::Text("YouTube is open on my box (not your Mac).".to_string()),
            screenshot(
                "c2",
                "clicking at 175,705; screenshot of the 1280x800 screen attached",
                b"png-two",
                (640, 480),
            ),
        ];
        let content = "I'll open YouTube on my box using the taught recipe.\n\nYouTube is open on my box (not your Mac).";

        db.save_message(
            "s1",
            "assistant",
            content,
            None,
            None,
            None,
            &saved_parts(&live),
        )
        .await
        .expect("the turn is saved");

        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(rows.len(), 1);
        let restored = restored_parts(&rows[0].content, rows[0].parts.clone());
        assert_eq!(shape(&restored), shape(&live));
    }

    /// A row written before pieces were kept has none of them — which is also every row the
    /// build in the person's hands is writing right now. It must still open, as the one bubble
    /// its words always were.
    #[tokio::test]
    async fn a_row_with_only_words_still_loads_as_a_single_bubble() {
        let db = test_db().await;
        db.ensure_session("s1", "Old").await.expect("a session");
        db.save_message(
            "s1",
            "assistant",
            "The build is green.",
            None,
            None,
            None,
            &[],
        )
        .await
        .expect("the message is saved");

        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert!(rows[0].parts.is_empty(), "no pieces were written");
        assert_eq!(
            shape(&restored_parts(&rows[0].content, rows[0].parts.clone())),
            vec!["text The build is green.".to_string()]
        );
    }

    /// Words alone are already in `content`. Writing them a second time would double every
    /// thread on disk for nothing.
    #[test]
    fn a_turn_that_was_only_words_saves_no_pieces() {
        let live = vec![
            ChatPart::Text("The build ".to_string()),
            ChatPart::Text("is green.".to_string()),
        ];
        assert!(saved_parts(&live).is_empty());
    }

    /// Both are the app talking, whether painted now or read back from an older build's rows.
    #[test]
    fn a_status_line_is_told_from_something_the_coworker_said() {
        assert!(is_status_line(EMPTY_TURN_NOTE));
        assert!(is_status_line("OpenGrok: the run failed"));
        assert!(!is_status_line("OpenGrok is a server."));
        assert!(!is_status_line("[took a screenshot of my screen]"));
        assert!(is_tool_standin("[took a screenshot of my screen]"));
        assert!(!is_tool_standin("The build is green."));
    }

    // The patch reconciliation, apart from the app: what the roster holds for a coworker once
    // the server has answered.
    use super::{Coworker, CoworkerPatch, apply_patch, settle_patch};

    fn bob() -> Coworker {
        Coworker {
            id: "cw_1".to_string(),
            name: "Bob".to_string(),
            model: "xai/grok-4.6@sub".to_string(),
            role: Some("Research".to_string()),
            title: Some("Analyst".to_string()),
            avatar_shape: Some("circle".to_string()),
            avatar_color: Some("blue".to_string()),
            notify_on_updates: Some(false),
            updated_at_ms: 17,
            hidden_from_sidebar: false,
            box_id: None,
        }
    }

    /// The roster took the new name before the server was asked. The server answered without a
    /// word about the name — which is what a route that quietly drops the field does — so the
    /// new name was never stored, and the roster must not go on showing it.
    #[test]
    fn a_rename_the_server_never_echoed_does_not_stay_on_the_roster() {
        let before = bob();
        let patch = CoworkerPatch {
            name: Some("Roberta".to_string()),
            title: Some("Analyst".to_string()),
            role: Some("Research".to_string()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        assert_eq!(roster.name, "Roberta", "the click is answered at once");

        let echo = Coworker {
            name: String::new(),
            ..before.clone()
        };
        settle_patch(&mut roster, &patch, Some(&echo), &before);
        assert_eq!(
            roster.name, "Bob",
            "an answer that says nothing about the name leaves the stored name standing"
        );
    }

    /// The same, for a server that answers with the name it kept rather than with no name at
    /// all: the echo is the truth even when it is the old value.
    #[test]
    fn the_servers_echo_wins_over_what_the_app_sent() {
        let before = bob();
        let patch = CoworkerPatch {
            name: Some("Roberta".to_string()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        settle_patch(&mut roster, &patch, Some(&before), &before);
        assert_eq!(roster.name, "Bob");
    }

    /// Nothing was stored, so nothing the patch asked for may be left behind — including the
    /// fields the app had already written into the roster to look quick.
    #[test]
    fn a_refused_patch_leaves_the_roster_as_it_was() {
        let before = bob();
        let patch = CoworkerPatch {
            name: Some("Roberta".to_string()),
            title: Some(String::new()),
            role: Some("Marketing".to_string()),
            notify_on_updates: Some(true),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        assert_eq!(roster.title, None, "the click is answered at once");

        settle_patch(&mut roster, &patch, None, &before);
        assert_eq!(roster, before);
    }

    /// A field the patch never mentioned is none of the reconciliation's business, which is
    /// what keeps a server that answers with half a coworker from blanking the other half.
    #[test]
    fn a_field_outside_the_patch_is_left_alone() {
        let before = bob();
        let patch = CoworkerPatch {
            model: Some("xai/grok-4.7@sub".to_string()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        let echo = Coworker {
            id: "cw_1".to_string(),
            name: String::new(),
            model: "xai/grok-4.7@sub".to_string(),
            role: None,
            title: None,
            avatar_shape: None,
            avatar_color: None,
            notify_on_updates: None,
            updated_at_ms: 0,
            hidden_from_sidebar: false,
            box_id: None,
        };
        settle_patch(&mut roster, &patch, Some(&echo), &before);
        assert_eq!(roster.model, "xai/grok-4.7@sub");
        assert_eq!(roster.name, "Bob", "a partial answer blanks nothing else");
        assert_eq!(roster.title.as_deref(), Some("Analyst"));
        assert_eq!(roster.updated_at_ms, 17);
    }

    /// Clearing is the one thing an answer with nothing in it confirms: the avatar the person
    /// reset stays reset rather than coming back on the next frame.
    #[test]
    fn a_cleared_field_stays_cleared_when_the_server_echoes_nothing() {
        let before = bob();
        let patch = CoworkerPatch {
            avatar_shape: Some(String::new()),
            avatar_color: Some(String::new()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        let echo = Coworker {
            avatar_shape: None,
            avatar_color: None,
            ..before.clone()
        };
        settle_patch(&mut roster, &patch, Some(&echo), &before);
        assert_eq!(roster.avatar_shape, None);
        assert_eq!(roster.avatar_color, None);
    }
}
