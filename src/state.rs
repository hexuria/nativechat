use crate::actions::TtsSource;
use crate::audio::AudioInput;
use crate::chrome::{
    ResponsiveCollapse, SIDEBAR_EXPANDED, SidebarChrome, collapse_for_width, remember_choice,
    sidebar_from_resize,
};
use crate::config::Config;
use crate::opengrok::{
    Account, ActivityTick, AguiMessage, ApprovalSpec, ChatPart, ConnectedComputer, Coworker,
    CoworkerPatch, FormSpec, LocalExecMode, LocalExecResolution, ModelCatalogue, OpenGrokClient,
    ProfileUpdate, QueuedApproval, TurnAssembler, activity_from_agui, command_from_args,
    command_from_replay_events, enrol_this_machine, local_exec_outcome, policy_answer,
    serve_local_exec, stored_machine_id, visible_bot_status,
};
use crate::services::database::DatabaseService;
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
    pub parts: Vec<ChatPart>,
}

impl Message {
    pub fn has_visible_body(&self) -> bool {
        !self.content.trim().is_empty()
            || self.parts.iter().any(|part| match part {
                ChatPart::Text(text) => !text.trim().is_empty(),
                ChatPart::Ui(_) | ChatPart::Approval(_) => true,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyTo {
    pub message_id: String,
    pub preview: String,
    pub is_me: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppSettingsTab {
    #[default]
    General,
    Profile,
    Appearance,
    Shortcuts,
    Computer,
}

/// One frame of in-app navigation. GPUI has no browser history; we keep this stack
/// so ⌘[ / ⌘] can walk agents, the right pane, and Settings the way macOS apps do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavLocation {
    pub coworker_id: Option<String>,
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

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

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

    /// True once the person (or the policy) has answered; a click still in
    /// flight counts.
    pub fn approval_answered(&self, call_id: &str) -> bool {
        self.approval_decisions
            .get(call_id)
            .is_some_and(|decision| !matches!(decision, ApprovalDecision::Pending))
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

    pub fn close_right_pane(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Closed {
            return;
        }
        self.right_pane = RightPane::Closed;
        self.computer_view = ComputerView::Overview;
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.record_nav();
        cx.notify();
    }

    pub fn show_agent_settings(&mut self, cx: &mut Context<Self>) {
        self.ensure_active_coworker(cx);
        self.right_pane = RightPane::Settings;
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn show_computer_pane(&mut self, cx: &mut Context<Self>) {
        self.ensure_active_coworker(cx);
        self.right_pane = RightPane::Computer;
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_agent_settings(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Settings {
            self.close_right_pane(cx);
            return;
        }
        self.right_pane = RightPane::Settings;
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_computer_pane(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Computer {
            self.close_right_pane(cx);
            return;
        }
        self.right_pane = RightPane::Computer;
        self.computer_view = ComputerView::Overview;
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.record_nav();
        cx.notify();
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
        self.right_pane = RightPane::Computer;
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
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".into());
            cx.notify();
            return;
        };
        let Some(id) = self.active_coworker_id.clone() else {
            self.auth_error = Some("No agent selected".into());
            cx.notify();
            return;
        };
        if let Some(existing) = self.coworkers.iter_mut().find(|c| c.id == id) {
            if let Some(name) = patch.name.clone() {
                existing.name = name;
            }
            if let Some(model) = patch.model.clone() {
                existing.model = model;
            }
            if let Some(role) = patch.role.clone() {
                existing.role = if role.trim().is_empty() {
                    None
                } else {
                    Some(role)
                };
            }
            if let Some(title) = patch.title.clone() {
                existing.title = if title.trim().is_empty() {
                    None
                } else {
                    Some(title)
                };
            }
            if let Some(shape) = patch.avatar_shape.clone() {
                existing.avatar_shape = if shape.is_empty() { None } else { Some(shape) };
            }
            if let Some(color) = patch.avatar_color.clone() {
                existing.avatar_color = if color.is_empty() { None } else { Some(color) };
            }
            if let Some(notify) = patch.notify_on_updates {
                existing.notify_on_updates = Some(notify);
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.patch_coworker(&id, &patch).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(updated) => {
                        if let Some(existing) = state.coworkers.iter_mut().find(|c| c.id == id) {
                            if !updated.name.is_empty() {
                                existing.name = updated.name;
                            }
                            if !updated.model.is_empty() {
                                existing.model = updated.model;
                            }
                            if updated.role.is_some() {
                                existing.role = updated.role;
                            }
                            if updated.title.is_some() {
                                existing.title = updated.title;
                            }
                            if updated.avatar_shape.is_some() {
                                existing.avatar_shape = updated.avatar_shape;
                            }
                            if updated.avatar_color.is_some() {
                                existing.avatar_color = updated.avatar_color;
                            }
                            if updated.notify_on_updates.is_some() {
                                existing.notify_on_updates = updated.notify_on_updates;
                            }
                        }
                        state.auth_error = None;
                    }
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
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
        self.right_pane = loc.right_pane;
        self.computer_view = loc.computer_view;
        self.is_app_settings_open = loc.app_settings_open;
        self.app_settings_tab = loc.app_settings_tab;
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
        self.right_pane = RightPane::Settings;
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

                                        Message {
                                            id: m.id,
                                            sender: if m.role == "user" {
                                                "Me".to_string()
                                            } else {
                                                "AI".to_string()
                                            },
                                            content: m.content,
                                            sent_at,
                                            is_me: m.role == "user",
                                            reply_preview: None,
                                            parts: Vec::new(),
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
            .map(|c| {
                c.messages
                    .iter()
                    .filter(|m| !m.content.trim().is_empty() || m.is_me)
                    .map(|m| AguiMessage {
                        id: m.id.clone(),
                        role: if m.is_me {
                            "user".to_string()
                        } else {
                            "assistant".to_string()
                        },
                        content: m.content.clone(),
                        tool_call_id: None,
                    })
                    .collect()
            })
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
            let (result, waiting_approval, turn_id) = match coworker {
                Ok(id) => {
                    let mut args_by_call: std::collections::HashMap<String, String> =
                        std::collections::HashMap::new();
                    let mut names_by_call: std::collections::HashMap<String, String> =
                        std::collections::HashMap::new();
                    let mut assembler = TurnAssembler::default();
                    let result = client
                        .run_turn(&id, &conversation_id, &history, |event| {
                            if let Some(call_id) = event.get("toolCallId").and_then(|v| v.as_str())
                            {
                                if let Some(name) =
                                    event.get("toolCallName").and_then(|v| v.as_str())
                                {
                                    names_by_call.insert(call_id.to_string(), name.to_string());
                                }
                                if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                                    args_by_call
                                        .entry(call_id.to_string())
                                        .or_default()
                                        .push_str(delta);
                                }
                            }
                            let call_id = event.get("toolCallId").and_then(|v| v.as_str());
                            let args = call_id
                                .and_then(|id| args_by_call.get(id))
                                .map(String::as_str);
                            let mut event = event.clone();
                            if event.get("toolCallName").is_none() {
                                if let Some(name) = call_id.and_then(|id| names_by_call.get(id)) {
                                    event.as_object_mut().map(|o| {
                                        o.insert("toolCallName".into(), name.clone().into())
                                    });
                                }
                            }
                            match activity_from_agui(&event, args) {
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
                    (result, waiting_approval, Some(id))
                }
                Err(error) => (Err(error), false, coworker_id.clone()),
            };
            let _ = this.update(cx, |state, cx| {
                if let Some(conversation) = state
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    if let Some(last) = conversation.messages.last_mut() {
                        if !last.is_me && !last.has_visible_body() {
                            match &result {
                                Ok(text) if !text.is_empty() => last.content = text.clone(),
                                Ok(_) => {
                                    last.content =
                                        "(OpenGrok returned no assistant text.)".to_string()
                                }
                                Err(error) => last.content = format!("OpenGrok: {}", error.message),
                            }
                        }
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
                if let Err(error) = result {
                    state.auth_error = Some(error.message);
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
                                            ActivityTick::Set(crate::opengrok::BotActivity {
                                                label: "Working".into(),
                                            }),
                                        );
                                    }
                                    "finished" | "failed" => {
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
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                message.parts.retain(|part| match part {
                    ChatPart::Approval(spec) => {
                        spec.call_id == keep_call_id
                            || self.approval_decisions.get(&spec.call_id).is_some_and(|d| {
                                d.is_settled() || matches!(d, ApprovalDecision::Sending)
                            })
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
        // Add user message to UI immediately
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            let reply_preview = self.reply_to.take().map(|r| r.preview);
            let message = Message {
                id: local_id.clone(),
                sender: "Me".to_string(),
                content: content.clone(),
                sent_at: SystemTime::now(),
                is_me: true,
                reply_preview,
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
            let local_id = local_id.clone();
            cx.spawn(async move |this, cx| {
                match db
                    .save_message(&conversation_id_clone, "user", &content_clone, None, None)
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
                self.native_tts.is_paused = true;
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

    pub fn toggle_read_aloud(
        &mut self,
        message_id: String,
        text: String,
        mode: TtsSource,
        cx: &mut Context<Self>,
    ) {
        let _ = mode;
        if let Some(service) = &self.tts_service {
            if self.native_tts.message_id.as_ref() == Some(&message_id) {
                if self.native_tts.is_paused {
                    service.resume_native();
                    self.native_tts.is_paused = false;
                } else {
                    service.pause_native();
                    self.native_tts.is_paused = true;
                }
                cx.notify();
            } else {
                self.read_aloud(text, message_id, TtsSource::Native, cx);
            }
        } else {
            self.read_aloud(text, message_id, TtsSource::Native, cx);
        }
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
