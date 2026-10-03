use gpui_agent::prelude::*;
use gpui_agent::{DispatchResult, virtual_unavailable};

use crate::components::agent_settings::{
    CeilingCardLine, SHARED_BOT_READS_SKILLS, SWITCHED_OFF_IN_SETTINGS, ShownCeilingRow,
    ShownSkillRow, SkillsCardLine, ceiling_card_lines, ceiling_line, offered_without_switches,
    shown_ceiling_rows, shown_skill_rows, skills_card_lines, skills_summary, tools_summary,
};
use crate::components::app_settings::{
    NO_LOCAL_RULES, not_in_effect_line, remove_label, rule_list_title,
};
use crate::components::chat_input::PanelMode;
use crate::components::chat_input::sources::{
    ParameterSource, SkillLibrary, SlashSource, ToolSource, ValueSource,
};
use crate::components::composer_panel::ComposerPanelRow;
use crate::components::computers::{self, ComputerCard};
use crate::components::connections::{self, ConnectOffer};
use crate::components::default_models;
use crate::components::model_picker;
use crate::components::reply_source;
use crate::components::skills::{
    NEVER_UPDATED, NOT_YET_RECORDING, NOT_YET_WITH_BOT, NOTHING_WRITTEN_YET, empty_line,
    short_relative_time, skill_matches, waiting_to_be_read,
};
use crate::opengrok::{
    BoxHandoffResolution, ChatPart, ChoiceCard, ComputerHandoffStatus, CoworkerPatch,
    LocalExecResolution, RecipeKind, RecipeSummary, ScreenshotSpec, UserFormDismissMode,
    UserFormFieldKind, choice_letter, computer_attention_done_id, computer_attention_id,
    computer_attention_skip_id, computer_handoff_card_id, computer_handoff_done_id,
    computer_handoff_skip_id, computer_handoff_takeover_id, computer_window_attention_done_id,
    computer_window_attention_id, computer_window_attention_skip_id, save_login_card_id,
    save_login_save_id, save_login_skip_id, user_form_card_id, user_form_continue_id,
    user_form_dismiss_id, user_form_field_id, user_form_pill_id, user_form_saved_clear_id,
    user_form_saved_note_id, user_form_screen_id, user_form_use_saved_id,
};
use crate::site_login::{SiteLoginRecord, grouped_logins, login_title};
use crate::state::{
    ActiveRecipe, AfterRefusal, AppSettingsTab, AppState, BotSkills, CeilingCard, ConnectionList,
    LocalRuleRow, LocalRules, PickerFor, PickerView, RuleKind, SWITCH_IN_FLIGHT, SkillScope,
    SkillsCard, TaughtSkill, ToolCeiling, ToolList, WRITING_A_LESSON,
};

pub mod ids {
    use crate::components::{computers, connections, default_models, model_picker, reply_source};
    use crate::opengrok::InferenceKind;
    use crate::state::RuleKind;

    pub const WINDOW: &str = "app-window";
    pub const PAGE: &str = "page-chat";
    pub const SIDEBAR: &str = "sidebar";
    pub const SIDEBAR_LIST: &str = "sidebar-chat-list";
    pub const NAV_NEW_CHAT: &str = "nav-new-chat";
    pub const NAV_SEARCH: &str = "nav-search";
    pub const NAV_LIBRARY: &str = "nav-library";
    pub const NAV_PROJECTS: &str = "nav-projects";
    pub const NAV_RECIPES: &str = "nav-recipes";
    pub const PAGE_RECIPES: &str = "page-recipes";
    pub const NAV_TOGGLE: &str = "nav-toggle-sidebar";
    pub const FOOTER_THEME: &str = "footer-theme";
    pub const FOOTER_ACCOUNT: &str = "footer-account";
    pub const FOOTER_SIGN_OUT: &str = "footer-sign-out";
    pub const COMPOSER: &str = "composer";
    /// The one button at the right of the composer: the send arrow, or the stop square while a
    /// turn is running. One id, because it is one button in one place.
    pub const COMPOSER_SEND: &str = "composer-send";
    /// Messages the open thread is holding until it is idle; in the tree only while there
    /// are any, so `assert --exists false` is "nothing queued".
    pub const COMPOSER_QUEUED: &str = "composer-queued";
    /// The one wide list `+`, `@` and `/` all open above the composer.
    pub const COMPOSER_PANEL: &str = "composer-panel";
    /// The field inside that list, which takes the caret the moment the list opens.
    pub const COMPOSER_PANEL_SEARCH: &str = "composer-panel-search";
    /// The line above the field saying which recipe the next message runs.
    pub const COMPOSER_RECIPE_BAR: &str = "composer-recipe-bar";
    /// The skill the next message is sent with, named by the chip in the draft; in the tree
    /// only while one is on the draft, so `assert --exists false` is "this message carries no
    /// skill". Its value is the id the turn will name, because two skills may share a name.
    pub const COMPOSER_SKILL: &str = "composer-skill";
    /// The chat's transcript, a scroll node whose value says whether it keeps the newest row in
    /// view: `following`, or `detached` once the person has scrolled back up the thread. A step
    /// toward older messages, however small, makes it `detached`, and nothing but the person
    /// makes it `following` again.
    pub const TRANSCRIPT: &str = "transcript";
    pub const LIGHTBOX: &str = "lightbox";
    pub const PAGE_LOGIN: &str = "page-login";
    pub const LOGIN_EMAIL: &str = "login-email";
    pub const LOGIN_PASSWORD: &str = "login-password";
    pub const LOGIN_SUBMIT: &str = "login-submit";
    pub const LOGIN_ERROR: &str = "login-error";
    pub const DIALOG_ACCOUNT: &str = "dialog-account";
    pub const DIALOG_VOICE: &str = "dialog-voice";
    pub const HEADER_SETTINGS: &str = "header-settings";
    pub const HEADER_LEFT_SIDEBAR: &str = "header-left-sidebar";
    pub const HEADER_RIGHT_SIDEBAR: &str = "header-right-sidebar";
    pub const HEADER_MONITOR: &str = "header-monitor";
    /// The chip at the top of the chat that names the open Bot. A press opens its settings, or,
    /// in one of its routines' threads, goes back to the Bot's own chat: state `goes-home` says
    /// which it will do.
    pub const HEADER_COWORKER: &str = "header-coworker";
    /// The open recipe's Run, the outcome of the run it started (value `running`, `ok`,
    /// `failed` or `interrupted`), and its run history.
    pub const RECIPE_RUN: &str = "recipe-run";
    pub const RECIPE_RUN_RESULT: &str = "recipe-run-result";
    pub const RECIPE_HISTORY_RUNS: &str = "recipe-history-runs";
    pub const RECIPE_ERROR: &str = "recipe-error";
    /// On a routine's thread: which routine (label) and what fires it (value, `schedule` or
    /// `webhook`), the line centred under the Bot chip. The way back to the bot's own chat is
    /// the chip itself, `header-coworker`, which says `goes-home` there.
    pub const CHAT_ROUTINE_THREAD: &str = "chat-routine-thread";
    /// On a routine's thread: how many bubbles are labelled as its instruction (value).
    pub const CHAT_ROUTINE_INSTRUCTIONS: &str = "chat-routine-instructions";
    pub const AGENT_SETTINGS: &str = "agent-settings";
    pub const AGENT_SAVE: &str = "agent-save";
    /// The one control that opens a blank routine, whichever of its two shapes the Computer
    /// pane is drawing: the "Create routine" card when the bot has none, the `+` when it has.
    pub const ROUTINE_NEW: &str = "routine-new";
    /// The open routine editor's red line: the last refusal, in the server's words, unless a
    /// note of the routine's already says it. In the tree only while the editor shows one.
    pub const ROUTINE_ERROR: &str = "routine-error";
    /// The open routine's four header icons, in the tree while a routine is open in the
    /// Computer pane: its Run history in place of its fields (state `selected` while shown, when
    /// it reads "Back to the routine"), Open in thread, Run it now, and Delete. Each acts on the
    /// open routine, and one that cannot is disabled with the reason as its value.
    pub const ROUTINE_HISTORY_TOGGLE: &str = "routine-history-toggle";
    pub const ROUTINE_OPEN_THREAD: &str = "routine-open-thread";
    pub const ROUTINE_RUN_NOW: &str = "routine-run-now";
    pub const ROUTINE_DELETE: &str = "routine-delete";
    /// "When to run" on the open routine: + (dead, with the reason as its value, while the
    /// routine has its one wake), and the wake editor's Save and Cancel.
    pub const ROUTINE_WAKE_ADD: &str = "routine-wake-add";
    pub const ROUTINE_WAKE_EDITOR: &str = "routine-wake-editor";
    pub const ROUTINE_WAKE_SUMMARY: &str = "routine-wake-summary";
    pub const ROUTINE_WAKE_NEXT: &str = "routine-wake-next";
    pub const ROUTINE_WAKE_ERROR: &str = "routine-wake-error";
    pub const ROUTINE_WAKE_CRON_NOTE: &str = "routine-wake-cron-note";
    /// Under what the wake editor picked, the zone its times are in (label `<zone> time`, value
    /// the IANA name), only where that is not this computer's zone.
    pub const ROUTINE_WAKE_ZONE: &str = "routine-wake-zone";
    pub const ROUTINE_WAKE_SAVE: &str = "routine-wake-save";
    pub const ROUTINE_WAKE_CANCEL: &str = "routine-wake-cancel";
    pub const ROUTINE_WAKE_AM: &str = "routine-wake-am";
    pub const ROUTINE_WAKE_PM: &str = "routine-wake-pm";

    /// One of the open routine's wakes (label = what sets it off, in words), and its ✎ and 🗑.
    pub fn routine_wake(at: usize) -> String {
        format!("routine-wake-{at}")
    }

    /// Under a schedule's wake, the zone its times are in (label = the IANA name), only where
    /// that is not this computer's zone.
    pub fn routine_wake_zone(at: usize) -> String {
        format!("routine-wake-{at}-zone")
    }

    pub fn routine_wake_edit(at: usize) -> String {
        format!("routine-wake-edit-{at}")
    }

    pub fn routine_wake_delete(at: usize) -> String {
        format!("routine-wake-delete-{at}")
    }

    /// A tab of the wake editor, by its word: `every`, `daily`, `weekly`, `monthly`, `webhook`,
    /// `cron`.
    pub fn routine_wake_tab(tab: crate::state::WakeTab) -> String {
        format!("routine-wake-tab-{}", tab.word())
    }

    /// One of the wake editor's typed boxes, and its ▲ and ▼.
    pub fn routine_wake_box(which: crate::state::WakeBox) -> &'static str {
        match which {
            crate::state::WakeBox::Every => "routine-wake-every",
            crate::state::WakeBox::Hour => "routine-wake-hour",
            crate::state::WakeBox::Minute => "routine-wake-minute",
            crate::state::WakeBox::Cron => "routine-wake-cron",
        }
    }

    /// The Every tab's unit, by its word: `minutes`, `hours`, `days`.
    pub fn routine_wake_unit(unit: crate::state::ScheduleUnit) -> &'static str {
        match unit {
            crate::state::ScheduleUnit::Minutes => "routine-wake-unit-minutes",
            crate::state::ScheduleUnit::Hours => "routine-wake-unit-hours",
            crate::state::ScheduleUnit::Days => "routine-wake-unit-days",
        }
    }

    /// The Weekly tab's day chips (0 is Sunday), the Monthly tab's dates (1 to 31) and the month
    /// chips (1 is January).
    pub fn routine_wake_day(day: u8) -> String {
        format!("routine-wake-day-{day}")
    }

    pub fn routine_wake_date(date: u8) -> String {
        format!("routine-wake-date-{date}")
    }

    pub fn routine_wake_month(month: u8) -> String {
        format!("routine-wake-month-{month}")
    }

    /// The question Delete asks over the whole window (label = `Delete "<name>"?`, value = what
    /// deleting it does), and its two answers. In the tree only while it asks.
    pub const ROUTINE_DELETE_PROMPT: &str = "routine-delete-prompt";
    pub const ROUTINE_DELETE_CANCEL: &str = "routine-delete-cancel";
    pub const ROUTINE_DELETE_CONFIRM: &str = "routine-delete-confirm";
    /// Under `computer-status`, while the Computer pane says why the server could not give the
    /// bot a computer: the server's words as the pane shows them, with its code as the value.
    pub const COMPUTER_ERROR: &str = "computer-error";
    /// Get a computer, beside that reason: asks the server again, and is dead while it is asked.
    pub const COMPUTER_GET: &str = "computer-get";
    /// The newest coworker reply's steps (value = how many), in the tree only while it has
    /// any, and its Thought rows (value = how many), only while it has any.
    pub const REPLY_STEPS: &str = "reply-steps";
    pub const REPLY_REASONING: &str = "reply-reasoning";
    /// The newest coworker reply's Timing row (value = what it says shut, `10s total`), in the
    /// tree only while the feed draws one.
    pub const REPLY_TIMING: &str = "reply-timing";

    /// One step of the newest reply, by the call it was.
    pub fn step(call_id: &str) -> String {
        format!("step-{call_id}")
    }

    pub fn session(id: &str) -> String {
        format!("session-{id}")
    }

    pub fn coworker(id: &str) -> String {
        format!("coworker-{id}")
    }

    /// One picture of the newest set in the transcript, named as the feed names its tiles.
    pub fn image_thumb(index: usize) -> String {
        format!("image-thumb-{index}")
    }

    /// One parameter's chip on the recipe bar. The bar's chips and the `@` panel's rows are two
    /// different things on screen, so they keep the two different ids the composer gives them:
    /// `composer-recipe-param-<name>` here, `composer-param-<name>` in the panel.
    pub fn recipe_param(name: &str) -> String {
        format!("composer-recipe-param-{name}")
    }

    /// Settings → Skills. A SKILL is prose the model reads before it works; a RECIPE is a
    /// taped replay of clicks. The two are never named by each other's word.
    pub const SETTINGS_SKILLS: &str = "settings-tab-skills";
    pub const SKILLS_SEARCH: &str = "settings-skills-search";
    pub const SKILLS_EMPTY: &str = "settings-skills-empty";
    pub const SKILLS_ERROR: &str = "settings-skills-error";
    /// An upload, while it is reading a folder and sending it. The sheet says so itself when it
    /// is the sheet that is saving, so this is on the list and only for an upload.
    pub const SKILLS_SAVING: &str = "settings-skills-saving";
    /// The one button that opens the four ways of making a skill. The four are in the tree
    /// themselves, so this is not what a driver clicks.
    pub const SKILL_ADD: &str = "settings-skill-add";
    pub const SKILL_UPLOAD: &str = "settings-skill-upload";
    pub const SKILL_WRITE: &str = "settings-skill-write";
    /// The two nobody has built. They carry the sentence saying what would have to be built, so
    /// a driver reads the same answer the person reads rather than finding a dead control.
    pub const SKILL_WITH_BOT: &str = "settings-skill-with-bot";
    pub const SKILL_RECORD: &str = "settings-skill-record";
    /// Create a skill: the sheet and its fields.
    pub const SKILL_SHEET: &str = "settings-skill-add-sheet";
    pub const SKILL_SHEET_NAME: &str = "settings-skill-add-name";
    pub const SKILL_SHEET_DESCRIPTION: &str = "settings-skill-add-description";
    pub const SKILL_SHEET_BODY: &str = "settings-skill-add-body";
    pub const SKILL_SHEET_CANCEL: &str = "settings-skill-add-cancel";
    pub const SKILL_SHEET_SAVE: &str = "settings-skill-add-save";
    pub const SKILL_ADD_ERROR: &str = "settings-skill-add-error";
    /// Delete asks first: the dialog, and its two answers.
    pub const SKILL_DELETE_SHEET: &str = "settings-skill-delete-sheet";
    pub const SKILL_DELETE_CONFIRM: &str = "settings-skill-delete-confirm";
    pub const SKILL_DELETE_CANCEL: &str = "settings-skill-delete-cancel";
    /// A task taught on a coworker's screen, being written up as a skill.
    ///
    /// The sheet this happens on is in that screen's own window, which has no tree: these four
    /// are what a driver has of it. The two that are clicked carry the ids the sheet's own
    /// controls carry, so a driver presses the button the person presses; the two statuses are
    /// here only — the screen says the same things as a label and a red line.
    pub const TEACH_SKILL_WRITING: &str = "teach-skill-writing";
    pub const TEACH_SKILL_ERROR: &str = "teach-skill-error";
    /// The lesson that was written, named by the name it must be invoked by, carrying its id and
    /// — while nobody has read it — the state `off`. Clicking it opens it to be read.
    pub const TEACH_SAVED_SKILL: &str = "teach-saved-skill";
    /// Send the refused tape again. The server keeps no tape, so this is the only thing that
    /// can: the recording is still in the window that made it. Only where sending it again
    /// could come out differently, and only while that window is open.
    pub const TEACH_RETRY: &str = "teach-retry";
    /// The way to the library for the one refusal nobody can answer: the connection dropped, so
    /// the skill may have been written and may not, and looking is the only way to find out.
    pub const TEACH_CHECK_LIBRARY: &str = "teach-check-library";

    /// The way back from an open skill to the whole-width list.
    pub const SKILL_CLOSE: &str = "settings-skill-close";
    /// The open skill's pane while its prose is on its way, and where it says the fetch was
    /// refused. One or the other, never both, and neither once the pane has what it asked for.
    ///
    /// "Has what it asked for" is not "has prose": a skill can arrive with none, which is what a
    /// draft is. Reading the empty body as "still coming" left a driver waiting at a pane the
    /// person could see was finished.
    pub const SKILL_LOADING: &str = "settings-skill-loading";
    pub const SKILL_ERROR: &str = "settings-skill-error";

    /// One skill's row. The id is the server's, so a driver that made a skill through
    /// `skill.create` can address the thing it made.
    pub fn skill(id: &str) -> String {
        format!("settings-skill-row-{id}")
    }

    /// Open, on that row's own menu. A second way in to the same pane as the row itself, and
    /// both are on screen.
    pub fn skill_open(id: &str) -> String {
        format!("settings-skill-open-{id}")
    }

    /// Delete on that row's menu, and Delete on the open skill's pane. Two controls in two
    /// places, so two ids: one name for both is one name that cannot say which was meant.
    pub fn skill_delete(id: &str) -> String {
        format!("settings-skill-delete-{id}")
    }

    pub fn skill_detail_delete(id: &str) -> String {
        format!("settings-skill-detail-delete-{id}")
    }

    /// The switch on the open skill's pane: on, and a turn may read it; off, and nothing may
    /// run it. It carries `checked` so a driver reads where it is rather than what it says.
    pub fn skill_enabled(id: &str) -> String {
        format!("settings-skill-enabled-{id}")
    }

    /// The prose itself, on the open skill's pane. It is what `skill.create` wrote down, so it
    /// is the one thing a driver has to be able to read back — and on a draft it is the sentence
    /// the pane shows in its place, with an empty value under it.
    pub fn skill_body(id: &str) -> String {
        format!("settings-skill-body-{id}")
    }

    pub fn skill_detail(id: &str) -> String {
        format!("settings-skill-detail-{id}")
    }

    pub fn skill_updated(id: &str) -> String {
        format!("settings-skill-updated-{id}")
    }

    /// One routine's row. The id is the schedule's, which is the server's, so a driver that
    /// made a routine through `routine.create` can address the thing it made.
    pub fn routine(id: &str) -> String {
        format!("routine-{id}")
    }

    /// The two triggers the editor offers, on a routine that has none. Gone once it has one:
    /// a routine is one schedule, so the second would be a second routine.
    pub fn routine_trigger_schedule(id: &str) -> String {
        format!("routine-{id}-trigger-schedule")
    }

    pub fn routine_trigger_webhook(id: &str) -> String {
        format!("routine-{id}-trigger-webhook")
    }

    /// What the webhook popover shows, and only while there is a webhook to show.
    pub fn routine_webhook_url(id: &str) -> String {
        format!("routine-{id}-webhook-url")
    }

    pub fn routine_webhook_key(id: &str) -> String {
        format!("routine-{id}-webhook-key")
    }

    pub fn routine_rotate(id: &str) -> String {
        format!("routine-{id}-rotate")
    }

    /// The Active switch on the routine's editor: checked while the routine runs on its own,
    /// labelled `Active`, or `Paused` while it does not. A click asks for the other way.
    pub fn routine_active(id: &str) -> String {
        format!("routine-{id}-active")
    }

    /// Test run: the server starts the routine now. Only on a routine the server has; dead on a
    /// server that cannot run a routine on demand.
    pub fn routine_test(id: &str) -> String {
        format!("routine-{id}-test")
    }

    /// What the editor says its server cannot do with the routine, beside the controls it leaves
    /// dead, by the line's word: `cant-change` (it cannot change a routine once it is made) and
    /// `cant-run` (it cannot run one on demand). In the tree only while it is so.
    pub fn routine_note(id: &str, word: &str) -> String {
        format!("routine-{id}-{word}")
    }

    /// In the Run history's place, on a server that cannot list a routine's runs.
    pub fn routine_runs_unavailable(id: &str) -> String {
        format!("routine-{id}-runs-unavailable")
    }

    /// What the person typed that a server unable to change the routine did not keep.
    pub fn routine_unsaved(id: &str) -> String {
        format!("routine-{id}-unsaved")
    }

    /// One line of the routine's Run history, by the server's run id. Its label is what set
    /// the run off and its value where the run got to, in the history's own words.
    pub fn routine_run(id: &str, run_id: &str) -> String {
        format!("routine-{id}-run-{run_id}")
    }

    /// A line of the routine's Run history that is a firing the server skipped (opengrok-server
    /// #316), by when it was due, in the server's milliseconds: it has no run id. Its label is
    /// what set it off and its value the server's sentence for why. It opens nothing.
    pub fn routine_skipped(id: &str, at_ms: i64) -> String {
        format!("routine-{id}-skipped-{at_ms}")
    }

    /// Open the thread the routine runs in. Only on a routine the server has.
    pub fn routine_thread(id: &str) -> String {
        format!("routine-{id}-thread")
    }

    /// Settings → Computer: one connected computer's local-exec mode (value the stored word,
    /// `ask` / `bypass` / `never`), and a button per mode under it.
    pub fn computer_exec(machine_id: &str) -> String {
        format!("settings-computer-{machine_id}-exec")
    }

    pub fn computer_exec_mode(machine_id: &str, mode: crate::opengrok::LocalExecMode) -> String {
        format!("settings-computer-{machine_id}-exec-{}", mode.as_stored())
    }

    /// One line of the open recipe's run history, by the run's id (`rrun_…`).
    pub fn recipe_history_run(run_id: &str) -> String {
        format!("recipe-history-run-{run_id}")
    }

    pub fn routine_delete(id: &str) -> String {
        format!("routine-{id}-delete")
    }

    /// This Mac's standing rules on Settings → Computer, drawn under its mode. The line saying
    /// there are none is in the tree only while there are none, and the one saying the lists
    /// could not be read only while they could not.
    pub const LOCAL_RULES_EMPTY: &str = "settings-local-rules-empty";
    pub const LOCAL_RULES_ERROR: &str = "settings-local-rules-error";

    /// Why this Mac stopped running commands for the server by itself, on Settings → Computer
    /// under This Mac. In the tree only while it has, until the next sign-in.
    pub const LOCAL_EXEC_STOPPED: &str = "settings-local-exec-stopped";

    /// One of the two lists, `allow` or `deny`, with its count as the value. In the tree only
    /// while it has a rule on it, as on screen.
    pub fn local_rules(kind: RuleKind) -> String {
        format!("settings-local-rules-{}", kind.word())
    }

    /// The `n`th rule on that list, counted from 0 in the server's order. The value is the
    /// command exactly as the server keeps it, which is how a driver says which rule it meant.
    pub fn local_rule(kind: RuleKind, n: usize) -> String {
        format!("settings-local-rule-{}-{n}", kind.word())
    }

    /// That rule's Remove: the id the button on screen carries.
    pub fn local_rule_remove(kind: RuleKind, n: usize) -> String {
        crate::components::app_settings::local_rule_remove_id(kind, n)
    }

    /// Why that allow is never read, while the server names it in `inert`.
    pub fn local_rule_inert(kind: RuleKind, n: usize) -> String {
        format!("settings-local-rule-inert-{}-{n}", kind.word())
    }

    /// Why that rule's last Remove did not go through.
    pub fn local_rule_error(kind: RuleKind, n: usize) -> String {
        format!("settings-local-rule-error-{}-{n}", kind.word())
    }

    // Connections (#2). Each is the id the window's own element carries, from the module that
    // draws it, so the control a driver presses is the control a person presses.
    pub const SETTINGS_CONNECTIONS: &str = connections::SETTINGS_TAB;
    pub const CONNECTIONS_REFRESH: &str = connections::REFRESH;
    pub const CONNECTIONS: &str = connections::LIST;
    pub const CONNECTIONS_EMPTY: &str = connections::LIST_EMPTY;
    pub const CONNECTIONS_ERROR: &str = connections::LIST_ERROR;
    pub const CONNECTORS: &str = connections::OFFERED;
    pub const CONNECTORS_EMPTY: &str = connections::OFFERED_EMPTY;
    pub const CONNECTORS_ERROR: &str = connections::OFFERED_ERROR;
    pub const AGENT_CONNECTIONS: &str = connections::AGENT_CARD;
    pub const AGENT_CONNECTIONS_NOTE: &str = connections::AGENT_NOTE;

    /// Settings → General, and its first section, Default models, which holds Default for new
    /// Bots.
    pub const SETTINGS_GENERAL: &str = "settings-tab-general";
    pub const DEFAULT_MODELS: &str = default_models::SECTION;

    /// Settings → Computer's "Your computers": one card to each enrolled computer, by the server's
    /// machine id: the card, its Relay your plan switch, where its relay stands, and what became of
    /// a switch that did not go as asked.
    pub fn computer_card(machine_id: &str) -> String {
        computers::card_id(machine_id)
    }

    pub fn computer_relay(machine_id: &str) -> String {
        computers::relay_switch_id(machine_id)
    }

    pub fn computer_status(machine_id: &str) -> String {
        computers::status_id(machine_id)
    }

    pub fn computer_error(machine_id: &str) -> String {
        computers::error_id(machine_id)
    }

    /// What this computer's card alone holds of its relay: opencodex's address and key, Save, and
    /// the lines about them, which were Settings → Relay's and keep their ids.
    pub const REPLY_SOURCE_SAVE: &str = computers::SAVE;
    pub const REPLY_SOURCE_ERROR: &str = computers::ERROR;
    pub const REPLY_SOURCE_UNAVAILABLE: &str = computers::UNAVAILABLE;
    pub const REPLY_SOURCE_HINT: &str = computers::HINT;
    /// In a Bot's Usage card, while its replies go through the person's own plan.
    pub const AGENT_USAGE_PLAN: &str = reply_source::BOT_USAGE_PLAN;

    /// The Bot's model picker: the Model card in the Bot's settings, the one place a Bot's model
    /// is picked. The parts of the popover it opens are `model_picker`'s own ids, all under
    /// `agent-model-`.
    pub const AGENT_MODEL_CARD: &str = model_picker::CARD;

    /// One model in the popover's list, by its door's wire word and the id a pick of it pins.
    pub fn model_row(source: InferenceKind, base_id: &str) -> String {
        model_picker::row_id(source, base_id)
    }
    /// Under this computer's status line: why it is not relaying, in the relay's own words.
    pub const RELAY_DETAIL: &str = computers::RELAY_DETAIL;
    pub const RELAY_ADDR: &str = computers::RELAY_ADDR;
    pub const RELAY_KEY: &str = computers::RELAY_KEY;
    pub const RELAY_KEY_REMOVE: &str = computers::RELAY_KEY_REMOVE;
    pub const RELAY_UNAVAILABLE: &str = computers::RELAY_UNAVAILABLE;
    /// Default for new Bots, on Settings → General: the section, the line saying it is coming,
    /// and the picker's card in it, which is dead until the server keeps such a default.
    pub const NEW_BOTS: &str = default_models::NEW_BOTS;
    pub const NEW_BOTS_UNAVAILABLE: &str = default_models::NEW_BOTS_UNAVAILABLE;
    pub const NEW_BOTS_CARD: &str = default_models::NEW_BOTS_CARD;
    /// Shuts Default for new Bots' popover, as `agent-model-dismiss` shuts the Bot's.
    pub const NEW_BOTS_DISMISS: &str = "settings-new-bots-dismiss";
    /// The Relay-off fallback, on Settings → General: the section, the line saying it is coming,
    /// and the picker's card in it, which is dead until the server keeps such a fallback; and the
    /// dismiss that shuts its popover.
    pub const PLAN_FALLBACK: &str = default_models::PLAN_FALLBACK;
    pub const PLAN_FALLBACK_UNAVAILABLE: &str = default_models::PLAN_FALLBACK_UNAVAILABLE;
    pub const PLAN_FALLBACK_CARD: &str = default_models::PLAN_FALLBACK_CARD;
    pub const PLAN_FALLBACK_DISMISS: &str = "settings-plan-fallback-dismiss";

    /// One model in the Relay-off fallback's list, by the id a pick of it pins: a Gateway row,
    /// the Bot's row's id under `settings-plan-fallback-`.
    pub fn plan_fallback_row(base_id: &str) -> String {
        model_picker::PLAN_FALLBACK_IDS.row_id(InferenceKind::Gateway, base_id)
    }

    /// One model in Default for new Bots' list, by its door's wire word and the id a pick of it
    /// pins: the Bot's row's id under `settings-new-bots-`.
    pub fn new_bots_row(source: InferenceKind, base_id: &str) -> String {
        model_picker::NEW_BOTS_IDS.row_id(source, base_id)
    }

    /// Beside the line of a turn the person's plan could not answer: the turn again, on the
    /// server's paid keys.
    pub const RUN_ERROR_SEND_ON_SERVER: &str = crate::components::chat::SEND_ON_SERVER;

    /// A held message the server holds until a Mac holds the relay again, by its bubble's id,
    /// under `composer-queued`.
    pub fn queued_waiting(message_id: &str) -> String {
        format!("queued-waiting-{message_id}")
    }

    /// A reply's badge, by the reply's message id.
    pub fn reply_badge(message_id: &str) -> String {
        reply_source::badge_id(message_id)
    }

    /// The model a reply's badge names on hover, under the badge.
    pub fn reply_badge_model(message_id: &str) -> String {
        reply_source::badge_model_id(message_id)
    }

    /// One connected service on Settings → Connections, by the server's connection id.
    pub fn connection(id: &str) -> String {
        connections::row_id(id)
    }

    pub fn connection_disconnect(id: &str) -> String {
        connections::disconnect_id(id)
    }

    pub fn connection_error(id: &str) -> String {
        connections::row_error_id(id)
    }

    /// Connect, for a service on offer, by the name the server lists it under.
    pub fn connect(connector: &str) -> String {
        connections::connect_id(connector)
    }

    pub fn connect_error(connector: &str) -> String {
        connections::connect_error_id(connector)
    }

    /// A connection's switch on the open bot's Connections card.
    pub fn connection_lend(id: &str) -> String {
        connections::lend_id(id)
    }

    pub fn connection_lend_error(id: &str) -> String {
        connections::lend_error_id(id)
    }

    /// The Tools card's line about the open Bot's ceiling: how many rows are allowed, or why
    /// there are no switches. In the tree only once the server has answered, as on screen.
    pub const AGENT_CEILING: &str = "agent-ceiling";
    /// Why every switch on the card is dead for good: the server's words for a 403.
    pub const AGENT_CEILING_READ_ONLY: &str = "agent-ceiling-read-only";
    /// Why every switch on the card is dead for now: another Bot's switch, or a read of this
    /// Bot's ceiling, is with the server.
    pub const AGENT_CEILING_WAIT: &str = "agent-ceiling-wait";
    /// What the server said about the last switch, when no row on the card is the one it is
    /// about.
    pub const AGENT_CEILING_NOTE: &str = "agent-ceiling-note";

    /// Every id under a row of the card is `agent-ceiling-` and one of these words, then the
    /// row's name, and none of the card's own ids has one of them there. So no name a plugin can
    /// have (`read-only`, `why-x`, `switch-x`) makes one id another's, and an id is read back
    /// into its row by cutting the prefix off.
    const CEILING_SWITCH: &str = "agent-ceiling-switch-";
    const CEILING_ROW_LINES: [&str; 3] = [
        "agent-ceiling-why-",
        "agent-ceiling-connector-",
        "agent-ceiling-error-",
    ];

    /// One row's switch on the Tools card, by the server's name for the row, which is what a
    /// `PUT` of the ceiling names it by.
    pub fn ceiling_switch(name: &str) -> String {
        format!("{CEILING_SWITCH}{name}")
    }

    /// The row a switch's id is for.
    pub fn ceiling_switch_row(id: &str) -> Option<&str> {
        id.strip_prefix(CEILING_SWITCH)
    }

    /// Under a row the server could not offer: why.
    pub fn ceiling_why(name: &str) -> String {
        format!("{}{name}", CEILING_ROW_LINES[0])
    }

    /// Under a plugin that works through a connection: which.
    pub fn ceiling_connector(name: &str) -> String {
        format!("{}{name}", CEILING_ROW_LINES[1])
    }

    /// Under a row: what the card says about its last switch.
    pub fn ceiling_error(name: &str) -> String {
        format!("{}{name}", CEILING_ROW_LINES[2])
    }

    /// A line on the Tools card rather than a switch: one of the card's own, or one under a row.
    pub fn is_ceiling_line(id: &str) -> bool {
        [
            AGENT_CEILING,
            AGENT_CEILING_READ_ONLY,
            AGENT_CEILING_WAIT,
            AGENT_CEILING_NOTE,
        ]
        .contains(&id)
            || CEILING_ROW_LINES
                .iter()
                .any(|prefix| id.starts_with(prefix))
    }

    /// A row of the read-only list of what the next turn is offered, which the Tools card shows
    /// where it has no switches.
    pub fn offered_tool(name: &str) -> String {
        format!("agent-tool-{name}")
    }

    /// The Skills card in a Bot's settings (opengrok-server#270): its value is the card's line,
    /// how many skills are attached, or why there are no switches.
    pub const AGENT_SKILLS: &str = "agent-skills";
    /// Show / Hide on the Skills card, while it has skills to show.
    pub const AGENT_SKILLS_TOGGLE: &str = "agent-skills-toggle";
    /// Why every skill switch is dead for good: the server's words for a 403.
    pub const AGENT_SKILLS_READ_ONLY: &str = "agent-skills-read-only";
    /// Why every skill switch is dead for now: another Bot's skill switch, or a read of this Bot's
    /// skills, is with the server.
    pub const AGENT_SKILLS_WAIT: &str = "agent-skills-wait";
    /// What the server said about the last skill switch, when no row on the card is the one it is
    /// about: a stale version, or the cap on attached skills.
    pub const AGENT_SKILLS_NOTE: &str = "agent-skills-note";
    /// On a shared Bot's card: the people who use it can read its attached skills.
    pub const AGENT_SKILLS_SHARED: &str = "agent-skills-shared";

    /// Every id under a skill on the card is `agent-skills-` and one of these words, then the
    /// skill's id, and none of the card's own ids has one of them there. So no id a skill can have
    /// (`toggle`, `off-x`, `switch-x`) makes one id another's, and an id is read back into its
    /// skill by cutting the prefix off. The skill's id and never its name: one of the person's
    /// own skills and a colleague's can share a name.
    const SKILL_SWITCH: &str = "agent-skills-switch-";
    const SKILL_ROW_LINES: [&str; 2] = ["agent-skills-off-", "agent-skills-error-"];

    /// One skill's switch on the Skills card, by the skill's id, which is what a `PUT` of the
    /// Bot's skills names it by.
    pub fn skill_switch(id: &str) -> String {
        format!("{SKILL_SWITCH}{id}")
    }

    /// The skill a switch's id is for.
    pub fn skill_switch_row(id: &str) -> Option<&str> {
        id.strip_prefix(SKILL_SWITCH)
    }

    /// Under a skill switched off in Settings → Skills: that it is.
    pub fn skill_off(id: &str) -> String {
        format!("{}{id}", SKILL_ROW_LINES[0])
    }

    /// Under a skill: what the card says about its last switch.
    pub fn skill_error(id: &str) -> String {
        format!("{}{id}", SKILL_ROW_LINES[1])
    }

    /// A line on the Skills card rather than a switch: one of the card's own, or one under a
    /// skill.
    pub fn is_skills_line(id: &str) -> bool {
        [
            AGENT_SKILLS,
            AGENT_SKILLS_READ_ONLY,
            AGENT_SKILLS_WAIT,
            AGENT_SKILLS_NOTE,
            AGENT_SKILLS_SHARED,
        ]
        .contains(&id)
            || SKILL_ROW_LINES.iter().any(|prefix| id.starts_with(prefix))
    }
}

/// A value the driver hands the app that must not show up in any `{:?}` of a command.
#[derive(Clone, PartialEq, Eq)]
pub struct RedactedSecret(pub String);

impl std::fmt::Debug for RedactedSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    NewChat,
    ToggleSidebar,
    ToggleMiniSidebar,
    ToggleTheme,
    ToggleAccount,
    ToggleAgentSettings,
    /// The model picker's card: its popover opens, or shuts.
    ToggleModelPicker,
    /// The popover opened, or shut.
    SetModelPicker(bool),
    /// The model's name in the popover, which opens the list, or the list's heading back.
    ToggleModelList,
    /// What the list's search box holds, as typing it there would leave it.
    SetModelSearch(String),
    /// A model in the list, by its group and its id: the Bot goes onto it at once.
    PickModel {
        source: crate::opengrok::InferenceKind,
        base_id: String,
    },
    /// ⚡, on or off, saved at once.
    SetModelFast(bool),
    /// A level of the effort slider, by the server's word, saved at once.
    SetModelEffort(String),
    /// ↺: the effort back to `inherit`, and ⚡ off.
    ResetModelPick,
    /// Default for new Bots' picker, on Settings → General: the same changes as the Bot's, each
    /// kept on the account at once.
    NewBots(PickerCommand),
    /// None, in Default for new Bots' list: no default, which leaves a new Bot to the server's.
    ClearNewBotsDefault,
    /// The Relay-off fallback's picker, on Settings → General: the same changes, each kept on
    /// the account at once.
    PlanFallback(PickerCommand),
    /// None, in the Relay-off fallback's list: no fallback.
    ClearPlanFallback,
    ToggleAvatarEditor,
    SetAvatarEditor(bool),
    SetAvatarColor(String),
    SelectSession(String),
    SelectCoworker(String),
    SendMessage(String),
    /// ⌘⇧↩: send now, over a running turn.
    SendMessageSteer(String),
    /// Stop the turn the open thread has in flight, which is what the composer's button does
    /// while it is a stop button.
    StopTurn,
    /// Send the open thread's last turn again, which is what "Try again" does on the row of a
    /// turn that never left.
    RetryTurn,
    ToggleComputerPane,
    OpenCoworkerScreen,
    /// Get a computer: ask the server again for the open bot's computer, after it said why it
    /// could not give one.
    GetComputer,
    /// Update / Reset the active bot's computer: open the confirm dialog, then answer it.
    OpenComputerConfirm(crate::state::ComputerAction),
    ConfirmComputerAction,
    CancelComputerConfirm,
    SetEgressTunnelEnabled(bool),
    /// The open bot's computer's standing answer to the tunnel's card.
    SetEgressPolicy(crate::opengrok::LocalExecMode),
    /// The shield badge's dialog on the Computer pane: open it, close it.
    OpenNetworkPolicy,
    CloseNetworkPolicy,
    /// The Recipes page: open it, filter it, open one recipe, answer a share, go back.
    OpenRecipes,
    SetRecipesFilter(crate::state::RecipeFilter),
    OpenRecipe(String),
    CloseRecipe,
    AnswerRecipeShare {
        id: String,
        accept: bool,
    },
    AnswerApproval {
        call_id: String,
        resolution: LocalExecResolution,
    },
    /// Open the newest set of pictures in the transcript at one of them, which is what a click
    /// on a tile does.
    OpenLightbox {
        index: usize,
    },
    Login {
        email: String,
        password: String,
    },
    SetLoginDraft {
        email: Option<String>,
        password: Option<String>,
    },
    Logout,
    /// The signed-out banner's button: go to the sign-in page, which is the only way out of
    /// that state and the reason the banner is a banner rather than a line in the transcript.
    SignInAgain,
    /// Idle user-form Continue. Values come from typed/picks on AppState.
    UserFormContinue {
        card_key: String,
    },
    /// "Use saved login" on an idle card: Touch ID, then the keychain, then the fill.
    UserFormUseSaved {
        card_key: String,
        login_id: String,
    },
    /// "Change" on a locked card: the held password is dropped.
    UserFormClearSaved {
        card_key: String,
    },
    /// A register-mode passkey card: the person confirms with Touch ID that the site may
    /// make a passkey.
    UserFormRegisterPasskey {
        card_key: String,
    },
    /// Settings → Logins → Add, with the values the driver gives.
    AddSiteLogin {
        origin: String,
        username: String,
        password: RedactedSecret,
        label: String,
        notes: String,
    },
    /// Settings → Logins → Import, from a file path the driver gives (no picker).
    ImportSiteLogins {
        path: String,
    },
    /// Settings → Skills: open the page, pick a side of the toggle, search it, open one skill
    /// or leave it, write one down, take an upload, drop one.
    OpenSkills,
    SetSkillsScope(crate::state::SkillScope),
    SetSkillsQuery(String),
    OpenSkill(String),
    CloseSkill,
    OpenSkillAdd,
    CloseSkillAdd,
    CreateSkill {
        name: String,
        description: String,
        body: String,
    },
    /// A `SKILL.md`, or the folder one lives in, by a path the driver gives (no picker).
    UploadSkill {
        path: String,
    },
    /// Delete asks first, as it does for a person: this opens the dialog, and the two below
    /// answer it.
    AskSkillDelete {
        id: String,
    },
    ConfirmSkillDelete,
    CloseSkillDeleteConfirm,
    /// The switch on the open skill's pane, both ways: on is the review a lesson a model wrote
    /// is waiting for, off is what somebody does after reading one they do not want used.
    SetSkillEnabled {
        id: String,
        enabled: bool,
    },
    /// A tape taught on a coworker's screen and written up as a skill: read the lesson, which
    /// is what has to happen before anything may use it, or send a refused tape again. `None`
    /// opens the library itself, for the refusal where nobody knows whether a skill was written.
    ///
    /// Both belong to the screen window's sheet and are reached through the app, because that
    /// window has no tree of its own for a driver to work.
    OpenTaughtSkill {
        id: Option<String>,
    },
    RetryTaughtSkill,
    /// Settings → Logins: the search field's text, the picked row, the Add sheet, and the
    /// picked row's notes.
    SetSiteLoginQuery(String),
    SelectSiteLogin(Option<String>),
    OpenSiteLoginAdd,
    CloseSiteLoginAdd,
    SetSiteLoginNotes {
        id: String,
        notes: String,
    },
    /// A choice picked on a choice card: see [`AppState::choose`].
    Choose {
        message_id: String,
        field_index: usize,
        option_index: usize,
    },
    ChoiceSubmit {
        message_id: String,
    },
    ChoiceDismiss {
        message_id: String,
    },
    ToggleAgentTools,
    ToggleAgentUsage,
    /// A switch on the Tools card: one row of the open Bot's ceiling on or off, sent at once.
    SetCeilingTool {
        name: String,
        enabled: bool,
    },
    /// The bot settings' Save.
    SaveAgentSettings,
    ToggleAgentSkills,
    /// A switch on the Skills card: one skill attached to the open Bot or detached from it, by
    /// the skill's id, sent at once.
    SetBotSkill {
        skill_id: String,
        attached: bool,
    },
    /// Attach a file to the draft by path, as the + would (#90).
    AttachFile(std::path::PathBuf),
    /// Take a file off the draft by its place, as its ✕ would.
    DetachFile(usize),
    UserFormDismiss {
        card_key: String,
    },
    UserFormOpenScreen {
        card_key: String,
    },
    UserFormSetField {
        card_key: String,
        field_id: String,
        value: String,
    },
    ComputerHandoffTakeOver {
        card_key: String,
    },
    ComputerHandoffDone {
        card_key: String,
    },
    ComputerHandoffSkip {
        card_key: String,
    },
    SaveLogin {
        form_entry_id: String,
    },
    SkipSaveLogin {
        form_entry_id: String,
    },
    DeleteSiteLogin {
        id: String,
    },
    SetAppSettingsTab(crate::state::AppSettingsTab),
    CloseAppSettings,
    /// The Computer pane's routines: open one (or a blank one), give a draft its trigger, ask
    /// for a new webhook key, drop one.
    OpenRoutineEditor(Option<String>),
    AddRoutineTrigger {
        routine_id: String,
        trigger: crate::state::NewTrigger,
    },
    /// A routine made outright, with no draft in between: the prompt and how it fires.
    CreateRoutine {
        kind: crate::opengrok::ScheduleKind,
        prompt: String,
        cron: Option<String>,
    },
    RotateRoutineWebhook {
        routine_id: String,
    },
    RunRoutineNow {
        routine_id: String,
    },
    /// The Active switch: pause the routine, or let it run on its own again.
    SetRoutineActive {
        routine_id: String,
        active: bool,
    },
    OpenRoutineThread {
        routine_id: String,
    },
    /// A line of a routine's Run history: its thread, at that run.
    OpenRoutineRun {
        routine_id: String,
        run_id: String,
    },
    /// The open routine's history icon: its Run history in place of its fields, or back.
    ToggleRoutineHistory,
    /// ✎ on one of the open routine's wakes, or + on one with none.
    OpenWakeEditor {
        index: Option<usize>,
    },
    CloseWakeEditor,
    PickWakeTab(crate::state::WakeTab),
    /// A typed box of the wake editor, set the way a driver sets a field.
    SetWakeBox {
        which: crate::state::WakeBox,
        text: String,
    },
    StepWakeBox {
        which: crate::state::WakeBox,
        up: bool,
    },
    SetWakePm(bool),
    SetWakeUnit(crate::state::ScheduleUnit),
    ToggleWakeDay(u8),
    ToggleWakeDate(u8),
    ToggleWakeMonth(u8),
    /// The wake editor's Save, with the routine's name and instruction as they stand.
    SaveWake,
    /// The Bot chip, pressed: the Bot's settings, or its own chat from a routine's thread.
    PressBotChip,
    /// Run the open recipe on this bot.
    RunOpenRecipe(String),
    SetComputerExecMode {
        machine_id: String,
        mode: crate::opengrok::LocalExecMode,
    },
    /// A routine's name or instruction changed the way the editor saves them, which is how a
    /// driver edits one: the editor's fields are the pane's own text, not the app's.
    EditRoutine {
        routine_id: String,
        name: Option<String>,
        prompt: Option<String>,
    },
    DeleteRoutine {
        routine_id: String,
    },
    /// Delete, clicked: the question first, as a person's click asks it.
    AskDeleteRoutine {
        routine_id: String,
    },
    ConfirmRoutineDelete,
    CancelRoutineDelete,
    /// Settings → Computer: take one of this Mac's standing rules off, named by its command.
    RemoveLocalRule {
        kind: RuleKind,
        pattern: String,
    },
    /// Settings → Connections' Refresh: read the connections and the services on offer again.
    RefreshConnections,
    /// Settings → Connections' Connect: ask for the service's sign-in page and open it in the
    /// person's browser, which is what a person's click does too.
    ConnectService(String),
    /// Settings → Connections' Disconnect, by the server's connection id.
    DisconnectConnection(String),
    /// Disconnect pressed: the row asks "Disconnect …?" first.
    AskDisconnect(String),
    /// No, on that question.
    KeepConnection,
    /// The open bot's Connections switch: lend it the connection, or take it back.
    SetConnectionLent {
        connection_id: String,
        lent: bool,
    },
    /// Open or shut rows of what the coworker did and thought, by their keys, which is what a
    /// click on a step row or a Thought row does.
    SetStepsOpen {
        keys: Vec<String>,
        open: bool,
    },
    /// Settings → Computer's Save, on this computer's card, which keeps this computer's half of the
    /// relay.
    SaveReplySource,
    /// Send this reply on Server instead: the turn the person's plan could not answer, again, on
    /// the server's paid keys.
    SendOnServer,
    /// A computer's Relay your plan switch, which acts at once: `PATCH /local-exec/daemon/{id}`
    /// for that computer, and on also points the account's way at the relay once. And opencodex's
    /// address and key on this computer, each waiting for Save as on its card.
    SetComputerRelay {
        machine_id: String,
        on: bool,
    },
    SetRelayAddress(String),
    SetRelayKey(RedactedSecret),
    /// Remove key for this computer's opencodex, or Keep key to take it back.
    ToggleRemoveRelayKey,
    Shutdown,
}

impl Command {
    pub fn apply(self, state: &mut AppState, cx: &mut gpui_kit::Context<AppState>) {
        match self {
            Self::NewChat => state.create_agent(cx),
            Self::ToggleSidebar => state.toggle_sidebar(cx),
            Self::ToggleMiniSidebar => state.toggle_mini_sidebar(cx),
            Self::ToggleTheme => state.toggle_theme(cx),
            Self::ToggleAccount => state.toggle_account_settings(cx),
            Self::ToggleAgentSettings => state.toggle_agent_settings(cx),
            Self::ToggleModelPicker => state.toggle_picker(PickerFor::Bot, cx),
            Self::SetModelPicker(open) => state.set_picker_open(PickerFor::Bot, open, cx),
            Self::ToggleModelList => state.toggle_picker_list(PickerFor::Bot, cx),
            Self::SetModelSearch(query) => state.set_picker_search(PickerFor::Bot, query, cx),
            Self::PickModel { source, base_id } => {
                state.pick_model(PickerFor::Bot, source, &base_id, cx)
            }
            Self::SetModelFast(on) => state.set_model_fast(PickerFor::Bot, on, cx),
            Self::SetModelEffort(word) => state.pick_model_effort(PickerFor::Bot, &word, cx),
            Self::ResetModelPick => state.reset_model_pick(PickerFor::Bot, cx),
            Self::NewBots(command) => command.apply(PickerFor::NewBots, state, cx),
            Self::ClearNewBotsDefault => state.clear_new_bots_default(cx),
            Self::PlanFallback(command) => command.apply(PickerFor::PlanFallback, state, cx),
            Self::ClearPlanFallback => state.clear_plan_fallback(cx),
            Self::ToggleAvatarEditor => {
                let open = !state.avatar_editor_open;
                state.set_avatar_editor_open(open, cx);
            }
            Self::SetAvatarEditor(open) => state.set_avatar_editor_open(open, cx),
            Self::SetAvatarColor(id) => state.patch_active_agent(
                CoworkerPatch {
                    avatar_color: Some(id),
                    ..Default::default()
                },
                cx,
            ),
            Self::SelectSession(id) => state.select_conversation(id, cx),
            Self::SelectCoworker(id) => state.select_coworker(id, cx),
            Self::SendMessage(text) => state.send_message(text, cx),
            // Words of the driver's own, not the composer's: `chat.send` is given its text and
            // never reads the draft, so nothing off the draft rides out on it. A driver that
            // wants the draft's skill types into the composer and presses Enter, which is the
            // person's path and goes through the composer.
            Self::SendMessageSteer(text) => state.send_message_with(text, true, None, cx),
            Self::StopTurn => state.stop_turn(cx),
            Self::RetryTurn => state.retry_turn(cx),
            Self::ToggleComputerPane => state.toggle_computer_pane(cx),
            Self::OpenCoworkerScreen => state.open_coworker_screen(cx),
            Self::GetComputer => state.ensure_coworker_computer(cx),
            Self::OpenComputerConfirm(action) => state.open_computer_confirm(action, cx),
            Self::ConfirmComputerAction => state.confirm_computer_action(cx),
            Self::CancelComputerConfirm => state.close_computer_confirm(cx),
            Self::SetEgressTunnelEnabled(enabled) => state.set_egress_tunnel_enabled(enabled, cx),
            Self::SetEgressPolicy(mode) => state.pick_network_policy(mode, cx),
            Self::OpenNetworkPolicy => state.open_network_policy(cx),
            Self::CloseNetworkPolicy => state.close_network_policy(cx),
            Self::OpenRecipes => state.open_recipes(cx),
            Self::SetRecipesFilter(filter) => state.set_recipes_filter(filter, cx),
            Self::OpenRecipe(id) => state.open_recipe(id, cx),
            Self::CloseRecipe => state.close_recipe(cx),
            Self::AnswerRecipeShare { id, accept } => state.answer_recipe_share(id, accept, cx),
            Self::AnswerApproval {
                call_id,
                resolution,
            } => {
                state.answer_approval_by_id(&call_id, resolution, cx);
            }
            Self::OpenLightbox { index } => {
                let shots = last_screenshot_set(state);
                state.open_lightbox(shots, index, cx);
            }
            Self::Login { email, password } => state.login(email, password, cx),
            Self::SetLoginDraft { email, password } => {
                if let Some(email) = email {
                    state.login_email = email;
                }
                if let Some(password) = password {
                    state.login_password = password;
                }
            }
            Self::Logout => state.logout(cx),
            Self::SignInAgain => state.sign_in_again(cx),
            Self::UserFormContinue { card_key } => state.submit_open_user_form(card_key, cx),
            Self::UserFormUseSaved { card_key, login_id } => {
                state.pick_saved_login(card_key, login_id, cx)
            }
            Self::UserFormClearSaved { card_key } => state.clear_saved_login_pick(card_key, cx),
            Self::UserFormRegisterPasskey { card_key } => {
                state.confirm_passkey_register(card_key, cx)
            }
            Self::AddSiteLogin {
                origin,
                username,
                password,
                label,
                notes,
            } => {
                state.add_site_login(origin, username, password.0, label, notes, cx);
            }
            Self::ImportSiteLogins { path } => {
                state.import_site_logins(std::path::PathBuf::from(path), cx)
            }
            Self::OpenSkills => state.open_skills(cx),
            Self::SetSkillsScope(scope) => state.set_skills_scope(scope, cx),
            Self::SetSkillsQuery(query) => state.set_skills_query(query, cx),
            Self::OpenSkill(id) => state.open_skill(id, cx),
            Self::CloseSkill => state.close_skill(cx),
            Self::OpenSkillAdd => state.open_skill_add(cx),
            Self::CloseSkillAdd => state.close_skill_add(cx),
            Self::CreateSkill {
                name,
                description,
                body,
            } => state.create_skill(name, description, body, cx),
            Self::UploadSkill { path } => state.upload_skill(std::path::PathBuf::from(path), cx),
            Self::AskSkillDelete { id } => state.ask_skill_delete(id, cx),
            Self::ConfirmSkillDelete => state.confirm_skill_delete(cx),
            Self::CloseSkillDeleteConfirm => state.close_skill_delete_confirm(cx),
            Self::SetSkillEnabled { id, enabled } => state.set_skill_enabled(id, enabled, cx),
            Self::OpenTaughtSkill { id } => state.show_skill_in_main_window(id, cx),
            Self::RetryTaughtSkill => state.retry_taught_skill(cx),
            Self::SetSiteLoginQuery(query) => state.set_site_login_query(query, cx),
            Self::SelectSiteLogin(id) => state.select_site_login(id, cx),
            Self::OpenSiteLoginAdd => state.open_site_login_add(cx),
            Self::CloseSiteLoginAdd => state.close_site_login_add(cx),
            Self::SetSiteLoginNotes { id, notes } => state.update_site_login_notes(id, notes, cx),
            Self::Choose {
                message_id,
                field_index,
                option_index,
            } => state.choose(message_id, field_index, option_index, cx),
            Self::ChoiceSubmit { message_id } => {
                if let Some(spec) = state.choice_spec(&message_id) {
                    state.submit_form(message_id, spec, cx);
                }
            }
            Self::ChoiceDismiss { message_id } => state.dismiss_choice(message_id, cx),
            Self::ToggleAgentTools => state.toggle_agent_tools(cx),
            Self::ToggleAgentUsage => state.toggle_agent_usage(cx),
            Self::SetCeilingTool { name, enabled } => state.switch_ceiling_tool(name, enabled, cx),
            Self::SaveAgentSettings => state.request_agent_save(cx),
            Self::ToggleAgentSkills => state.toggle_agent_skills(cx),
            Self::SetBotSkill { skill_id, attached } => {
                state.switch_bot_skill(skill_id, attached, cx)
            }
            Self::AttachFile(path) => state.request_attach(path, cx),
            Self::DetachFile(index) => state.request_detach(index, cx),
            Self::UserFormDismiss { card_key } => {
                state.dismiss_user_form(card_key, UserFormDismissMode::Dismissed, cx)
            }
            Self::UserFormOpenScreen { card_key } => {
                state.dismiss_user_form(card_key, UserFormDismissMode::Escalated, cx)
            }
            Self::UserFormSetField {
                card_key,
                field_id,
                value,
            } => state.set_user_form_typed_field(card_key, field_id, value, cx),
            Self::ComputerHandoffTakeOver { .. } => state.take_over_computer(cx),
            Self::ComputerHandoffDone { card_key } => {
                state.resolve_user_form_handoff(card_key, BoxHandoffResolution::HandedBack, cx)
            }
            Self::ComputerHandoffSkip { card_key } => {
                state.resolve_user_form_handoff(card_key, BoxHandoffResolution::Declined, cx)
            }
            Self::SaveLogin { form_entry_id } => state.save_offered_login(form_entry_id, cx),
            Self::SkipSaveLogin { form_entry_id } => state.skip_save_login(form_entry_id, cx),
            Self::DeleteSiteLogin { id } => state.delete_site_login(id, cx),
            Self::SetAppSettingsTab(tab) => state.set_app_settings_tab(tab, cx),
            Self::CloseAppSettings => {
                if state.is_app_settings_open {
                    state.toggle_app_settings(cx);
                }
            }
            Self::OpenRoutineEditor(id) => state.open_routine_editor(id, cx),
            Self::AddRoutineTrigger {
                routine_id,
                trigger,
            } => {
                if let Some(coworker_id) = state.active_coworker_id.clone() {
                    state.add_routine_trigger(&coworker_id, &routine_id, trigger, cx);
                }
            }
            Self::CreateRoutine { kind, prompt, cron } => {
                state.create_routine(kind, prompt, cron, cx)
            }
            Self::RotateRoutineWebhook { routine_id } => {
                if let Some(coworker_id) = state.active_coworker_id.clone() {
                    state.rotate_routine_webhook(&coworker_id, &routine_id, cx);
                }
            }
            Self::OpenRoutineThread { routine_id } => state.open_routine_thread(&routine_id, cx),
            Self::OpenRoutineRun { routine_id, run_id } => {
                state.open_routine_run(&routine_id, &run_id, cx)
            }
            Self::ToggleRoutineHistory => state.toggle_routine_history(cx),
            Self::OpenWakeEditor { index } => {
                if let crate::state::ComputerView::Editor { id: Some(open) } =
                    state.computer_view.clone()
                {
                    state.open_wake_editor(&open, index, cx);
                }
            }
            Self::CloseWakeEditor => state.close_wake_editor(cx),
            Self::PickWakeTab(tab) => state.pick_wake_tab(tab, cx),
            Self::SetWakeBox { which, text } => state.set_wake_box(which, text, false, cx),
            Self::StepWakeBox { which, up } => state.step_wake_box(which, up, cx),
            Self::SetWakePm(pm) => state.set_wake_pm(pm, cx),
            Self::SetWakeUnit(unit) => state.set_wake_unit(unit, cx),
            Self::ToggleWakeDay(day) => state.toggle_wake_weekday(day, cx),
            Self::ToggleWakeDate(date) => state.toggle_wake_date(date, cx),
            Self::ToggleWakeMonth(month) => state.toggle_wake_month(month, cx),
            Self::SaveWake => {
                let fields = state.routine_wake_editor.as_ref().and_then(|editor| {
                    let bot = state.active_coworker_id.as_deref()?;
                    state
                        .coworker_routines(bot)
                        .iter()
                        .find(|row| row.id == editor.routine_id)
                        .map(|row| (row.name.clone(), row.instruction.clone()))
                });
                if let Some((name, instruction)) = fields {
                    state.save_wake(name, instruction, cx);
                }
            }
            Self::PressBotChip => state.press_bot_chip(cx),
            Self::RunOpenRecipe(coworker_id) => state.run_open_recipe(coworker_id, cx),
            Self::SetComputerExecMode { machine_id, mode } => {
                state.set_computer_exec_mode(machine_id, mode, cx)
            }
            Self::RunRoutineNow { routine_id } => {
                if let Some(coworker_id) = state.active_coworker_id.clone() {
                    state.run_routine_now(&coworker_id, &routine_id, cx);
                }
            }
            Self::SetRoutineActive { routine_id, active } => {
                if let Some(coworker_id) = state.active_coworker_id.clone() {
                    state.set_routine_active(&coworker_id, &routine_id, active, cx);
                }
            }
            Self::EditRoutine {
                routine_id,
                name,
                prompt,
            } => {
                if let Some(coworker_id) = state.active_coworker_id.clone()
                    && let Some(row) = state.routine_mut(&coworker_id, &routine_id)
                {
                    let name = name.unwrap_or_else(|| row.name.clone());
                    let prompt = prompt.unwrap_or_else(|| row.instruction.clone());
                    state.save_routine_fields(&coworker_id, &routine_id, name, prompt, cx);
                }
            }
            Self::DeleteRoutine { routine_id } => {
                if let Some(coworker_id) = state.active_coworker_id.clone() {
                    state.delete_routine(&coworker_id, &routine_id, cx);
                }
            }
            Self::AskDeleteRoutine { routine_id } => state.ask_delete_routine(&routine_id, cx),
            Self::ConfirmRoutineDelete => state.confirm_routine_delete(cx),
            Self::CancelRoutineDelete => state.cancel_routine_delete(cx),
            Self::RemoveLocalRule { kind, pattern } => state.remove_local_rule(kind, pattern, cx),
            Self::RefreshConnections => state.refresh_connections(cx),
            Self::ConnectService(connector) => state.connect_service(connector, cx),
            Self::DisconnectConnection(id) => state.disconnect_connection(id, cx),
            Self::AskDisconnect(id) => state.ask_to_disconnect(id, cx),
            Self::KeepConnection => state.keep_connection(cx),
            Self::SetConnectionLent {
                connection_id,
                lent,
            } => state.set_connection_lent(connection_id, lent, cx),
            Self::SetStepsOpen { keys, open } => state.set_steps_open(&keys, open, cx),
            Self::SaveReplySource => state.save_reply_source(cx),
            Self::SendOnServer => state.send_on_server(cx),
            Self::SetComputerRelay { machine_id, on } => {
                state.set_computer_relay(machine_id, on, cx)
            }
            Self::SetRelayAddress(address) => state.set_relay_address(address, cx),
            Self::SetRelayKey(key) => state.set_relay_key(&key.0, cx),
            Self::ToggleRemoveRelayKey => state.toggle_remove_relay_key(cx),
            Self::Shutdown => {}
        }
    }
}

/// One of a picker's changes, as a click on its popover makes it: the Bot's are sent as the
/// commands above that it has always had, Default for new Bots' as [`Command::NewBots`], and the
/// Relay-off fallback's as [`Command::PlanFallback`].
#[derive(Debug, Clone)]
pub enum PickerCommand {
    /// The card: the popover opens, or shuts.
    Toggle,
    /// The popover opened, or shut.
    SetOpen(bool),
    /// The model's name in the popover, which opens the list, or the list's heading back.
    ToggleList,
    /// What the list's search box holds, as typing it there would leave it.
    SetSearch(String),
    /// A model in the list, by its group and its id.
    Pick {
        source: crate::opengrok::InferenceKind,
        base_id: String,
    },
    /// ⚡, on or off.
    SetFast(bool),
    /// A level of the effort slider, by the server's word.
    SetEffort(String),
    /// ↺: the effort back to `inherit`, and ⚡ off.
    Reset,
}

impl PickerCommand {
    fn apply(self, which: PickerFor, state: &mut AppState, cx: &mut gpui_kit::Context<AppState>) {
        match self {
            Self::Toggle => state.toggle_picker(which, cx),
            Self::SetOpen(open) => state.set_picker_open(which, open, cx),
            Self::ToggleList => state.toggle_picker_list(which, cx),
            Self::SetSearch(query) => state.set_picker_search(which, query, cx),
            Self::Pick { source, base_id } => state.pick_model(which, source, &base_id, cx),
            Self::SetFast(on) => state.set_model_fast(which, on, cx),
            Self::SetEffort(word) => state.pick_model_effort(which, &word, cx),
            Self::Reset => state.reset_model_pick(which, cx),
        }
    }

    /// The command this change is sent as for `which` picker.
    fn sent_to(self, which: PickerFor) -> Command {
        match which {
            PickerFor::NewBots => Command::NewBots(self),
            PickerFor::PlanFallback => Command::PlanFallback(self),
            PickerFor::Bot => match self {
                Self::Toggle => Command::ToggleModelPicker,
                Self::SetOpen(open) => Command::SetModelPicker(open),
                Self::ToggleList => Command::ToggleModelList,
                Self::SetSearch(query) => Command::SetModelSearch(query),
                Self::Pick { source, base_id } => Command::PickModel { source, base_id },
                Self::SetFast(on) => Command::SetModelFast(on),
                Self::SetEffort(word) => Command::SetModelEffort(word),
                Self::Reset => Command::ResetModelPick,
            },
        }
    }
}

/// The keys one typing op asks the window for.
///
/// Typing is not a write to a field. `/` and `@` never reach the composer's text at all — the
/// composer takes them in the capture phase and opens its panel instead (see
/// [`crate::components::chat_input`]) — so a driver that set the draft would leave the panel
/// shut while the test said the text was there. The host therefore plans keystrokes and
/// [`crate::root::RootView`], which has the window, presses them one painted frame at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposePlan {
    /// Put the caret in the composer before pressing anything. `false` means "whatever holds
    /// the caret now", which is how the panel's own search field is reached.
    pub focus_composer: bool,
    /// Take the caret out of every field first, the way a click on a choice card does, so
    /// the keys reach the window rather than a field's text. See [`NativeChatHost::key`].
    pub release_caret: bool,
    /// GPUI keystroke tokens, in order: `a`, `space`, `enter`, `escape`, `up`, `cmd-a`.
    pub keys: Vec<String>,
}

/// The chord the text field binds to `SelectAll`, which is how a person replaces what is in it.
///
/// `set_value` means "replace", and the only honest way to replace text in a field that is
/// driven by keys is to take all of it and delete it first. The chord is the field's own
/// (gpui-base binds it in the `Input` context), not something invented here.
fn select_all_chord() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    }
}

/// A protocol key name in GPUI's spelling.
///
/// The arrows are named here; everything else goes to [`gpui_agent::keystroke_token`], so the
/// modifier-free rule the protocol promises is kept by the crate that promises it. A chord
/// belongs on `op keybinding`, not here.
fn key_token(key: &str) -> Result<String, String> {
    match key.trim().to_ascii_lowercase().as_str() {
        "up" | "arrowup" => Ok("up".into()),
        "down" | "arrowdown" => Ok("down".into()),
        "left" | "arrowleft" => Ok("left".into()),
        "right" | "arrowright" => Ok("right".into()),
        _ => gpui_agent::keystroke_token(key).map_err(plain),
    }
}

/// One keystroke per character, the way a person would type the text.
fn text_tokens(text: &str) -> Result<Vec<String>, String> {
    gpui_agent::text_keystrokes(text).map_err(plain)
}

/// Drop the `virtual_unavailable:` label off a message from the keystroke tables.
///
/// The label is about the virtual delivery mode, and these ops are answered semantically; the
/// half of the sentence that says which key could not be spelled is the useful half.
fn plain(error: String) -> String {
    error
        .strip_prefix(gpui_agent::VIRTUAL_UNAVAILABLE)
        .and_then(|rest| rest.strip_prefix(": "))
        .map(str::to_string)
        .unwrap_or(error)
}

/// Where a typing op is aimed, or the error saying that nothing there takes text.
fn compose_plan(target: &str, keys: Vec<String>) -> Result<ComposePlan, String> {
    match target {
        ids::COMPOSER => Ok(ComposePlan {
            focus_composer: true,
            release_caret: false,
            keys,
        }),
        // The protocol's own "the focused editable widget". It is how everything the composer
        // opens is worked: a panel's search field takes the caret as it opens, and the keys
        // that filter and pick a row are meant for that field rather than for the message.
        "" | "focused" => Ok(ComposePlan {
            focus_composer: false,
            release_caret: false,
            keys,
        }),
        other => Err(not_editable(other)),
    }
}

fn not_editable(target: &str) -> String {
    format!(
        "`{target}` is not editable (composer, login-email, login-password, \
         user-form-field-*, settings-logins-search, settings-skills-search, \
         settings-login-notes-*, settings-relay-addr, settings-relay-key, agent-model-search, \
         settings-new-bots-search, settings-plan-fallback-search, or \"\" for whatever holds the \
         caret)"
    )
}

/// The login page's two fields. The page keeps its own text; what the host keeps is the draft
/// the rest of the app reads, which is what `click login-submit` signs in with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginField {
    Email,
    Password,
}

/// Whether a target is one of the Bot's model picker's ids: the card, a part of its popover, or a
/// heading or a row of its list.
fn is_picker_part(target: &str) -> bool {
    model_picker::BOT_IDS.holds(target)
}

/// The picker whose search box or slider a target is, if it is one.
fn picker_field(
    target: &str,
    field: fn(&model_picker::PickerIds) -> &'static str,
) -> Option<PickerFor> {
    [PickerFor::Bot, PickerFor::NewBots, PickerFor::PlanFallback]
        .into_iter()
        .find(|which| field(model_picker::ids(*which)) == target)
}

/// The words a driver is refused in where a picker's slider is not on screen: no model is
/// picked, or the model on the card lists no levels of effort.
fn no_slider_refusal(pick: &crate::opengrok::ModelPick, target: &str) -> String {
    let why = if pick.model.is_some() {
        format!("{} lists no levels of effort", pick.model_label())
    } else {
        "no model is picked".to_string()
    };
    format!("`{target}` is not on screen: {why}, so there is no slider")
}

/// The levels the model on the card lists, by the server's words, low to high, as a refusal names
/// them.
fn levels_in_words(pick: &crate::opengrok::ModelPick) -> String {
    pick.levels()
        .iter()
        .map(|level| level.value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn login_field(target: &str) -> Option<LoginField> {
    match target {
        ids::LOGIN_EMAIL => Some(LoginField::Email),
        ids::LOGIN_PASSWORD => Some(LoginField::Password),
        _ => None,
    }
}

/// `image-thumb-3` → 3.
fn thumb_target(target: &str) -> Option<usize> {
    gpui_agent::parse_numbered_id("image-thumb-", target).map(|index| index as usize)
}

/// The pictures of the newest set in the transcript, which is the set `image-thumb-<n>` names.
///
/// The feed numbers each strip from zero and starts again at the next one, so only one strip
/// can be named without ambiguity — and the newest is the one a driver has just caused. Words
/// between two pictures end a strip in the feed, so they end it here too.
fn last_screenshot_set(state: &AppState) -> Vec<ScreenshotSpec> {
    let Some(conversation) = state
        .conversations
        .iter()
        .find(|conversation| Some(&conversation.id) == state.active_conversation_id.as_ref())
    else {
        return Vec::new();
    };
    for message in conversation.messages.iter().rev().filter(|m| !m.hidden) {
        let mut set: Vec<ScreenshotSpec> = Vec::new();
        for part in message.parts.iter().rev() {
            match part {
                ChatPart::Screenshot(spec) => set.push(spec.clone()),
                ChatPart::Text(text) if text.trim().is_empty() => {}
                _ if !set.is_empty() => break,
                _ => {}
            }
        }
        if !set.is_empty() {
            set.reverse();
            return set;
        }
    }
    state.last_box_shot.clone().into_iter().collect()
}

/// The rows of the composer's open panel, taken from the sources the panel itself draws from.
///
/// Rebuilt rather than copied out of the composer: the lists move under an open panel — recipes
/// land after `/` was pressed, a value fills in while its parameter's list is up — and a copy
/// taken when the panel opened would name rows that are no longer the rows on screen.
fn panel_rows(
    mode: PanelMode,
    recipes: &[RecipeSummary],
    skills: &SkillLibrary<'_>,
    active: Option<&ActiveRecipe>,
) -> Vec<PanelRow> {
    let rows = match mode {
        // The "+" list is the composer's own two fixed rows rather than a source, so they are
        // named here by the element ids the composer gives them.
        PanelMode::Plus => {
            return vec![
                PanelRow::fixed("composer-attach", "Attach files"),
                PanelRow::fixed("composer-teach", "Teach a task"),
            ];
        }
        PanelMode::Tools => ToolSource.rows(),
        PanelMode::Slash => SlashSource.rows(recipes, skills),
        PanelMode::Parameters => match active {
            Some(recipe) => ParameterSource.rows(recipe),
            None => Vec::new(),
        },
        PanelMode::Value { parameter } => match active
            .and_then(|recipe| recipe.parameters.get(parameter).map(|p| (recipe, p)))
        {
            Some((recipe, declared)) => ValueSource.rows(declared, recipe.value(&declared.name)),
            None => Vec::new(),
        },
    };
    rows.into_iter()
        .map(|(row, _)| PanelRow {
            id: row_id(&row),
            title: row.title.to_string(),
            label: row.label.as_ref().map(ToString::to_string),
            note: !row.selectable,
        })
        .collect()
}

/// The element id a row answers to on screen: the one it asked for, or the one built from its
/// key — the same fallback [`crate::components::composer_panel`] uses when it draws the row.
fn row_id(row: &ComposerPanelRow) -> String {
    row.element_id
        .as_ref()
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("composer-panel-row-{}", row.id))
}

/// What the open panel is called, which is what the driver reads in the tree.
fn panel_name(mode: PanelMode, recipe: Option<&RecipeBarSnap>) -> String {
    match mode {
        PanelMode::Plus => "Attach or teach".to_string(),
        PanelMode::Tools => "Tools".to_string(),
        PanelMode::Slash => "Recipes, workflows, skills and actions".to_string(),
        PanelMode::Parameters => match recipe {
            Some(recipe) => format!("What {} needs told", recipe.name),
            None => "What the recipe needs told".to_string(),
        },
        PanelMode::Value { parameter } => {
            match recipe.and_then(|recipe| recipe.parameters.get(parameter)) {
                Some(parameter) => format!("Value for {}", parameter.name),
                None => "Value".to_string(),
            }
        }
    }
}

#[derive(Clone)]
struct SessionSnap {
    id: String,
    title: String,
    active: bool,
    /// What the sidebar row says under the name (`rail_preview`): the last thing said in the
    /// bot's thread, or "No messages yet".
    preview: Option<String>,
    /// The thread is known only from the server's list (`GET /ag-ui/threads`): this Mac never
    /// held it, and the list is what put it here.
    listed: bool,
}

/// One row of the composer's open panel, by the id the panel gives it on screen.
#[derive(Clone)]
struct PanelRow {
    id: String,
    title: String,
    /// Which kind of thing this row is — "Recipe", "Workflow", "Skill", "Action", "Tool" — the
    /// word the panel prints down the right-hand side of it.
    ///
    /// It rides in the node's `value` because `value` is the one field a driver can assert on,
    /// and telling a tape from a tree is the whole point of the list: two rows with the same
    /// shape and the same id prefix are otherwise indistinguishable to anything but an eye.
    label: Option<String>,
    /// A row that only says something: dimmed, stepped over by the arrows, never picked.
    note: bool,
}

impl PanelRow {
    fn fixed(id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            label: None,
            note: false,
        }
    }
}

/// The recipe or workflow on the draft, as the composer's bar shows it.
#[derive(Clone)]
struct RecipeBarSnap {
    name: String,
    /// Which of the two it is. The driver has to be able to tell a tape from a tree without
    /// looking at pixels, because the two rows in `/` are otherwise the same shape.
    kind: RecipeKind,
    parameters: Vec<ParamSnap>,
}

/// One of that recipe's parameters: its name, what it has been told, and whether it must be.
#[derive(Clone)]
struct ParamSnap {
    name: String,
    value: Option<String>,
    required: bool,
}

/// The picture overlay, while it is open.
#[derive(Clone)]
struct LightboxSnap {
    index: usize,
    total: usize,
    caption: String,
}

/// One row of the Recipes page.
#[derive(Clone)]
struct RecipeSnap {
    id: String,
    name: String,
    /// A share waiting on Accept or Decline.
    pending: bool,
}

/// One row of Settings → Skills. A skill is prose the model reads before it works, which is
/// why the row carries its description: that sentence is what the model chooses by, and a
/// driver checking that a skill was written down checks the words, not a count.
#[derive(Clone)]
struct SkillSnap {
    id: String,
    name: String,
    /// `authored`, `uploaded` or `taught` — where the prose came from, as a state on the row so
    /// an assert does not have to match a sentence.
    source: &'static str,
    description: String,
    /// Named and kept, with no prose in it yet, so there is nothing to invoke.
    draft: bool,
    /// The switch. Off is drawn on the row as a chip, so it is a state here: after this branch
    /// most rows in a library are off, because every lesson a model writes starts that way.
    enabled: bool,
}

// How stale a skill is has no field here. The row shows it, but as a bare line with no id of
// its own; where the screen gives that fact an element to point at is the open skill's pane,
// and it is read off [`OpenSkillSnap`] there.

/// The skill on the pane, from the moment it was asked for.
///
/// Not read off the list: the list holds one side of the toggle and the pane keeps whatever was
/// opened, including a skill from the side that is no longer shown. Reading it off the rows put
/// the driver and the screen out of step exactly there — the person saw the pane, the driver saw
/// nothing to assert on.
#[derive(Clone)]
struct OpenSkillSnap {
    id: String,
    name: String,
    /// The prose. Empty while it is coming, and empty for a draft — which is why `arrived` is
    /// its own bit rather than something to be worked out from this being empty.
    body: String,
    /// The fetch has answered. Until it has, the pane is the close button and a line saying so.
    arrived: bool,
    updated_at_ms: i64,
    /// Where the switch is. Off means nothing may run it — the server refuses a switched-off
    /// skill even to its owner — and off is where every lesson a model wrote starts.
    enabled: bool,
    /// When somebody first switched this skill on, and `None` while nobody has. The raw fact,
    /// not the answer: whether it is waiting to be read is worked out by the screen's own
    /// function, so the tree and the pane cannot say different things about it.
    approved_at_ms: Option<i64>,
    /// The switch is with the server. It is dead while it is, both on screen and here.
    switching: bool,
    /// What the fetch said when it was refused, or what the switch was refused with. The pane
    /// says this instead of "Loading…" while there is nothing to draw, and beside the switch
    /// once there is.
    error: Option<String>,
}

/// One routine on the open bot's Computer pane, which is one schedule on the server.
#[derive(Clone)]
struct RoutineSnap {
    id: String,
    name: String,
    /// `cron`, `webhook`, or `draft` for one nobody has given a trigger yet — the only kind
    /// that is not on the server at all.
    kind: &'static str,
    /// The line the server keeps, on a cron routine.
    cron: Option<String>,
    active: bool,
    webhook_url: Option<String>,
    webhook_key: Option<String>,
    /// Run history, newest first.
    runs: Vec<RunLineSnap>,
    /// What the editor says the server cannot do with this routine (`state::routine_notes`).
    notes: Vec<(&'static str, &'static str)>,
    /// The server cannot list this routine's runs, so the history says so in their place.
    runs_unavailable: bool,
    /// What the person typed that the server did not keep, line by line.
    unsaved: Vec<(&'static str, String)>,
    /// Its wakes, in order: what sets each off in words, and whether it is a webhook.
    wakes: Vec<(String, bool)>,
    /// The zone its times are in, where that is not this computer's
    /// ([`crate::state::RoutineZone::note`]).
    zone: Option<String>,
}

/// One line of a routine's Run history as the driver sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RunLineSnap {
    /// A run, by its id: what set it off, and where it got to.
    Ran {
        run_id: String,
        cause: String,
        status: &'static str,
    },
    /// A firing the server skipped, by when it was due: what set it off, and the server's
    /// sentence for why.
    Skipped {
        at_ms: i64,
        cause: String,
        reason: String,
    },
}

impl RoutineSnap {
    /// Test run is dead: the server cannot run a routine on demand.
    fn run_dead(&self) -> bool {
        self.notes.iter().any(|(word, _)| *word == "cant-run")
    }

    /// The routine's fields are dead: the server cannot change it once it is made.
    fn cant_change(&self) -> bool {
        self.notes.iter().any(|(word, _)| *word == "cant-change")
    }
}

/// The open recipe as the driver needs it: whether Run can be pressed and which bot it plays on,
/// how the run it started came out, and the history.
#[derive(Clone, Default)]
struct RecipeDetailSnap {
    /// The bot Run plays on when nobody picked one: the page's own default, the first bot the
    /// recipe is granted to or else the person's first bot. `None` when they have no bot.
    run_bot: Option<String>,
    runnable: bool,
    running: bool,
    /// `ok`, `failed` or `interrupted`, for the run this page started.
    result: Option<&'static str>,
    /// What the page says went wrong, in its words: a Run the server refused, among others.
    error: Option<String>,
    /// Run id and its state word (`running`, `finished`, `interrupted`), plus whether it
    /// succeeded, newest first as the server lists them.
    runs: Vec<(String, String, bool)>,
}

/// What the newest coworker reply did and thought, as its rows show it.
#[derive(Clone, Default)]
struct ReplyRunSnap {
    steps: Vec<StepSnap>,
    /// Each Thought row's key, and whether it is open.
    thoughts: Vec<(String, bool, Option<String>)>,
    /// Its Timing row, from the function that builds the feed's (`steps::timing_row`), while
    /// the feed draws one.
    timing: Option<crate::components::steps::RunRow>,
}

/// One step of that reply.
#[derive(Clone)]
struct StepSnap {
    call_id: String,
    /// The row's own words, from the function that draws them (`StepSpec::label`).
    label: String,
    status: crate::opengrok::StepStatus,
    /// Its key in `AppState::expanded_steps`, and whether its row is open.
    key: String,
    open: bool,
    /// How long the call took, as the end of its row says it (`StepSpec::took`), while the row
    /// says it.
    took: Option<String>,
}

impl ReplyRunSnap {
    /// `show_timing` is Settings → Show turn timing, which puts each call's time on its row and
    /// the Timing row under the reply.
    fn from_message(
        message: &crate::state::Message,
        open: &std::collections::HashSet<String>,
        show_timing: bool,
    ) -> Self {
        let message_id = message.id.as_str();
        let mut run = Self {
            timing: message
                .run_timing
                .as_ref()
                .filter(|_| show_timing)
                .and_then(|frame| crate::components::steps::timing_row(message_id, frame, open)),
            ..Self::default()
        };
        for part in &message.parts {
            match part {
                ChatPart::Step(step) => {
                    let key = crate::components::steps::step_key(message_id, &step.call_id);
                    run.steps.push(StepSnap {
                        call_id: step.call_id.clone(),
                        label: step.label(),
                        status: step.status(),
                        open: open.contains(&key),
                        key,
                        took: show_timing.then(|| step.took()).flatten(),
                    });
                }
                ChatPart::Reasoning(thought) => {
                    let key = crate::components::steps::thought_key(message_id, run.thoughts.len());
                    run.thoughts.push((
                        key.clone(),
                        open.contains(&key),
                        show_timing.then(|| thought.took()).flatten(),
                    ));
                }
                _ => {}
            }
        }
        run
    }
}

/// An approval card still waiting on the person.
#[derive(Clone)]
struct ApprovalSnap {
    call_id: String,
    /// The card's own title, from the function that draws it (`gen_ui::approval_title`), so a
    /// snapshot catches the title a person would read and not one rebuilt here.
    title: String,
    tool: String,
    local: bool,
    review: bool,
    /// The server's word for what suspended the run, and the thread it filed
    /// the card under. Both on the card as states, so a driver can tell an
    /// MCP card from a shell's without reading the title.
    reason: String,
    thread_id: String,
    /// The egress tunnel's own card: its Always and Never set the computer's standing choice,
    /// so Never is on offer here and nowhere else among review cards.
    tunnel: bool,
}

/// User-form card in the open thread. Idle cards expose fields + Continue /
/// Open the screen / Dismiss. Settled cards expose a pill so Open the screen
/// cannot drop `user-form-*` from the tree.
#[derive(Clone)]
struct UserFormSnap {
    card_key: String,
    title: String,
    fields: Vec<UserFormFieldSnap>,
    /// None = idle (fields still on screen).
    pill: Option<String>,
    /// What the primary button says: "Log in" on a one-page login, else "Continue".
    continue_label: &'static str,
    /// The accounts saved for this site: (login id, username, origin), listed under the
    /// name field.
    saved_logins: Vec<(String, String, String)>,
    /// The line under the name field while a pick is under way, or after it was not.
    saved_login_note: Option<String>,
    /// A picked password is held: the fields are locked and Change frees them.
    saved_login_held: bool,
    /// A passkey card in register mode: one row to confirm instead of a list.
    passkey_register: bool,
}

#[derive(Clone)]
struct UserFormFieldSnap {
    id: String,
    label: String,
    kind: UserFormFieldKind,
    masked: bool,
    /// Typed value, including secrets. The tree omits masked values.
    value: String,
}

#[derive(Clone)]
struct ComputerHandoffSnap {
    card_key: String,
    instruction: String,
    status: ComputerHandoffStatus,
}

impl Default for ComputerHandoffSnap {
    fn default() -> Self {
        Self {
            card_key: String::new(),
            instruction: String::new(),
            status: ComputerHandoffStatus::ActionNeeded,
        }
    }
}

#[derive(Clone, Default)]
struct SaveLoginSnap {
    form_entry_id: String,
    origin: String,
    username: String,
}

/// One saved login as Settings → Logins lists it: the row (id, origin, username, label,
/// kind, notes, last use — never a password) and where its password is.
#[derive(Clone, Default)]
struct SiteLoginSnap {
    row: SiteLoginRecord,
    /// The password is in this Mac's keychain (else on the server only).
    on_this_mac: bool,
    /// An authenticator-code seed is on this Mac: the pane shows a live code.
    has_code: bool,
}

/// A choice card in the open thread, as [`crate::components::gen_ui`] draws it.
#[derive(Clone, Default)]
struct ChoiceSnap {
    message_id: String,
    title: String,
    /// `open`, `answered`, `not-answered` (the thread moved past it) or `dismissed`.
    state: &'static str,
    /// One question: a pick answers it, and its choices wear letters.
    one_question: bool,
    /// The newest open one-question card, which a letter key answers.
    keyed: bool,
    fields: Vec<ChoiceFieldSnap>,
    /// What the answer said, `Label: choice` per line, once there is one.
    answer: Option<String>,
}

#[derive(Clone, Default)]
struct ChoiceFieldSnap {
    label: String,
    options: Vec<String>,
    picked: Option<String>,
}

fn choice_card_id(message_id: &str) -> String {
    format!("choice-{message_id}")
}

fn choice_option_id(message_id: &str, field_index: usize, option_index: usize) -> String {
    format!("choice-{message_id}-{field_index}-{option_index}")
}

fn choice_dismiss_id(message_id: &str) -> String {
    format!("choice-{message_id}-dismiss")
}

fn choice_submit_id(message_id: &str) -> String {
    format!("choice-{message_id}-submit")
}

fn choice_snaps(state: &AppState) -> Vec<ChoiceSnap> {
    let keyed = state.keyed_choice();
    let mut snaps = Vec::new();
    for message in state.active_thread_messages() {
        let Some(spec) = message.parts.iter().rev().find_map(|part| match part {
            ChatPart::Ui(crate::opengrok::UiSpec::Form(spec)) => Some(spec),
            _ => None,
        }) else {
            continue;
        };
        let card = state.choice_card(&message.id, spec);
        let picks = state.form_picks.get(&message.id);
        let (label, answer) = match &card {
            ChoiceCard::Open => ("open", None),
            ChoiceCard::Answered(answer) => ("answered", Some(answer)),
            ChoiceCard::MovedOn => ("not-answered", None),
            ChoiceCard::Dismissed => ("dismissed", None),
        };
        snaps.push(ChoiceSnap {
            message_id: message.id.clone(),
            title: spec
                .title
                .clone()
                .or_else(|| spec.prompt.clone())
                .unwrap_or_default(),
            state: label,
            one_question: spec.answers_on_pick(),
            keyed: keyed.as_deref() == Some(message.id.as_str()),
            fields: spec
                .fields
                .iter()
                .map(|field| ChoiceFieldSnap {
                    label: field.label.clone(),
                    options: field.options.clone(),
                    picked: picks.and_then(|picks| picks.get(&field.id)).cloned(),
                })
                .collect(),
            answer: answer.map(|picks| {
                let mut only = spec.clone();
                only.title = None;
                only.answer_text(picks)
            }),
        });
    }
    snaps
}

/// `choice-{messageId}` (value = its state; state `keyboard` on the card a letter answers),
/// with `choice-{messageId}-{field}-{option}` per choice (value = its letter on a one-question
/// card; state `selected` when picked), `choice-{messageId}-dismiss`, and on a card of several
/// questions `choice-{messageId}-submit`, while it is open; `choice-{messageId}-answer` once
/// answered.
fn choice_node(choice: &ChoiceSnap) -> UiNode {
    let id = choice_card_id(&choice.message_id);
    let mut card = UiNode::dialog(id, choice.title.clone()).with_value(choice.state);
    if choice.keyed {
        card.states.push("keyboard".into());
    }
    if let Some(answer) = &choice.answer {
        return card.with_child(UiNode::status(
            format!("choice-{}-answer", choice.message_id),
            answer.clone(),
        ));
    }
    if choice.state != "open" {
        return card;
    }
    for (field_index, field) in choice.fields.iter().enumerate() {
        for (option_index, option) in field.options.iter().enumerate() {
            let mut node = UiNode::button(
                choice_option_id(&choice.message_id, field_index, option_index),
                if choice.one_question {
                    option.clone()
                } else {
                    format!("{}: {option}", field.label)
                },
            );
            if let Some(letter) = choice
                .one_question
                .then(|| choice_letter(option_index))
                .flatten()
            {
                node = node.with_value(letter.to_string());
            }
            if field.picked.as_deref() == Some(option.as_str()) {
                node.states.push("selected".into());
            }
            card = card.with_child(node);
        }
    }
    if !choice.one_question {
        card = card.with_child(UiNode::button(choice_submit_id(&choice.message_id), "Send"));
    }
    card.with_child(UiNode::button(
        choice_dismiss_id(&choice.message_id),
        "Dismiss",
    ))
}

/// The open Bot's Tools card as its settings draw it: what the next turn is offered, and the
/// switches of its ceiling (opengrok-server#268).
struct ToolsCardSnap<'a> {
    tools: Option<&'a ToolList>,
    ceiling: Option<&'a CeilingCard>,
    open: bool,
}

/// `agent-tools` (value = the card's first line: what the next turn is offered, `Asking the
/// server…`, or why there is no list), `agent-ceiling` (value = `3 of 8 allowed`, or why there
/// are no switches; in the tree once the server has answered, as on screen), and while there is
/// anything to show, `agent-tools-toggle`, with under it, visible while the card is open:
///
/// - the card's own lines, as on screen above the switches: `agent-ceiling-read-only` (the
///   server's words for a 403), `agent-ceiling-wait` (another Bot's switch, or a read of this
///   one's, is with the server) and `agent-ceiling-note` (the server's words about the last
///   switch when no row is the one they are about);
/// - one `agent-ceiling-switch-{name}` per row. A row's node is its switch: named by its heading,
///   value `builtin` / `plugin`, `checked` where it stands (where it was asked to go while that is
///   with the server), enabled only while a click would send it, with the states `switching` and
///   `unavailable`. Under it, as under the row on screen: `agent-ceiling-why-{name}`,
///   `agent-ceiling-connector-{name}` and `agent-ceiling-error-{name}`;
/// - where the ceiling could not be read, `agent-tool-{name}` per tool the next turn is offered
///   instead (value `builtin` / `plugin`), read-only.
///
/// Every one of them is made from what the screen draws ([`shown_ceiling_rows`],
/// [`ceiling_card_lines`], [`offered_without_switches`]), so the two cannot disagree.
fn agent_tools_node(card: &ToolsCardSnap<'_>) -> UiNode {
    let mut node = UiNode::new("agent-tools", "list", "Tools");
    if let Some(tools) = card.tools {
        node = node.with_value(tools_summary(tools));
    }
    if let Some(line) = card
        .ceiling
        .and_then(|ceiling| ceiling_line(&ceiling.ceiling, ceiling.pending.as_ref()))
    {
        node = node.with_child(UiNode::status(ids::AGENT_CEILING, "Allowed").with_value(line));
    }
    let rows = card.ceiling.map(shown_ceiling_rows).unwrap_or_default();
    let offered = offered_without_switches(card.ceiling, card.tools);
    if rows.is_empty() && offered.is_empty() {
        return node;
    }
    node = node.with_child(UiNode::button(
        "agent-tools-toggle",
        if card.open { "Hide" } else { "Show" },
    ));
    for line in card.ceiling.map(ceiling_card_lines).unwrap_or_default() {
        let (id, words) = match line {
            CeilingCardLine::ReadOnly(words) => (ids::AGENT_CEILING_READ_ONLY, words),
            CeilingCardLine::Wait(words) => (ids::AGENT_CEILING_WAIT, words),
            CeilingCardLine::Note(words) => (ids::AGENT_CEILING_NOTE, words),
        };
        node = node.with_child(UiNode::status(id, words).with_visible(card.open));
    }
    for row in &rows {
        node = node.with_child(ceiling_switch_node(row).with_visible_deep(card.open));
    }
    for tool in offered {
        node = node.with_child(
            UiNode::listitem(ids::offered_tool(&tool.name), tool.name.clone())
                .with_value(if tool.is_builtin() {
                    "builtin"
                } else {
                    "plugin"
                })
                .with_visible(card.open),
        );
    }
    node
}

fn ceiling_switch_node(row: &ShownCeilingRow) -> UiNode {
    let mut node = UiNode::new(ids::ceiling_switch(&row.name), "switch", row.title.clone())
        .with_value(if row.builtin { "builtin" } else { "plugin" })
        .with_checked(row.on)
        .with_enabled(row.live);
    if row.switching {
        node.states.push("switching".to_string());
    }
    if let Some(why) = row.unavailable {
        node.states.push("unavailable".to_string());
        node = node.with_child(UiNode::status(ids::ceiling_why(&row.name), why));
    }
    if let Some(note) = &row.connector {
        node = node.with_child(UiNode::note(
            ids::ceiling_connector(&row.name),
            note.clone(),
        ));
    }
    if let Some(words) = &row.note {
        node = node.with_child(UiNode::status(ids::ceiling_error(&row.name), words.clone()));
    }
    node
}

/// `agent-skills` (value = the Skills card's line: `2 attached · 1 switched off`, `Asking the
/// server…`, or why there are no switches), with `agent-skills-toggle` while the card has skills
/// to show, and under it, visible while the card is open:
///
/// - the card's own lines, as on screen above the switches: `agent-skills-read-only` (the
///   server's words for a 403), `agent-skills-wait` (another Bot's skill switch, or a read of this
///   one's skills, is with the server) and `agent-skills-note` (the server's words about the last
///   switch when no skill is the one they are about);
/// - one `agent-skills-switch-{id}` per skill, by the skill's id. A skill's node is its switch:
///   named by the skill's name, value `mine` / `org`, `checked` where it stands (where it was
///   asked to go while that is with the server), enabled only while a click would send it, with
///   the states `switching` and `switched-off`. Under it, as under the row on screen:
///   `agent-skills-off-{id}` and `agent-skills-error-{id}`;
/// - on a Bot the roster says is shared, `agent-skills-shared`.
///
/// Every one of them is made from what the screen draws ([`shown_skill_rows`],
/// [`skills_card_lines`], [`SkillsCard::shared`]), so the two cannot disagree.
fn agent_skills_node(card: &SkillsCard, open: bool) -> UiNode {
    let mut node = UiNode::new(ids::AGENT_SKILLS, "list", "Skills")
        .with_value(skills_summary(&card.skills, card.pending.as_ref()));
    let rows = shown_skill_rows(card);
    if rows.is_empty() {
        return node;
    }
    node = node.with_child(UiNode::button(
        ids::AGENT_SKILLS_TOGGLE,
        if open { "Hide" } else { "Show" },
    ));
    for line in skills_card_lines(card) {
        let (id, words) = match line {
            SkillsCardLine::ReadOnly(words) => (ids::AGENT_SKILLS_READ_ONLY, words),
            SkillsCardLine::Wait(words) => (ids::AGENT_SKILLS_WAIT, words),
            SkillsCardLine::Note(words) => (ids::AGENT_SKILLS_NOTE, words),
        };
        node = node.with_child(UiNode::status(id, words).with_visible(open));
    }
    for row in &rows {
        node = node.with_child(skill_switch_node(row).with_visible_deep(open));
    }
    if card.shared {
        node = node.with_child(
            UiNode::status(ids::AGENT_SKILLS_SHARED, SHARED_BOT_READS_SKILLS).with_visible(open),
        );
    }
    node
}

fn skill_switch_node(row: &ShownSkillRow) -> UiNode {
    let mut node = UiNode::new(ids::skill_switch(&row.id), "switch", row.title.clone())
        .with_value(if row.mine { "mine" } else { "org" })
        .with_checked(row.on)
        .with_enabled(row.live);
    if row.switching {
        node.states.push("switching".to_string());
    }
    if row.switched_off {
        node.states.push("switched-off".to_string());
        node = node.with_child(UiNode::status(
            ids::skill_off(&row.id),
            SWITCHED_OFF_IN_SETTINGS,
        ));
    }
    if let Some(words) = &row.note {
        node = node.with_child(UiNode::status(ids::skill_error(&row.id), words.clone()));
    }
    node
}

fn user_form_node(form: &UserFormSnap) -> UiNode {
    let key = &form.card_key;
    let mut card = UiNode::dialog(user_form_card_id(key), form.title.clone());
    if let Some(pill) = &form.pill {
        return card.with_child(UiNode::status(user_form_pill_id(key), pill.clone()));
    }
    for field in &form.fields {
        let id = user_form_field_id(key, &field.id);
        let node = if field.kind == UserFormFieldKind::Checkbox {
            UiNode::checkbox(id, field.label.clone()).with_checked(field.value == "true")
        } else {
            let mut box_ = UiNode::textbox(id, field.label.clone());
            if !field.masked && !field.value.is_empty() {
                box_ = box_.with_value(field.value.clone());
            }
            box_
        };
        card = card.with_child(node);
    }
    for (login_id, username, origin) in &form.saved_logins {
        card = card.with_child(UiNode::listitem(
            user_form_use_saved_id(key, login_id),
            format!("{username} · {origin}"),
        ));
    }
    if let Some(note) = &form.saved_login_note {
        card = card.with_child(UiNode::status(user_form_saved_note_id(key), note.clone()));
    }
    if form.saved_login_held {
        card = card.with_child(UiNode::button(user_form_saved_clear_id(key), "Change"));
    }
    if form.passkey_register && !form.saved_login_held {
        card = card.with_child(UiNode::button(
            format!("user-form-passkey-register-{key}"),
            "Create a passkey",
        ));
    }
    card.with_child(UiNode::button(
        user_form_continue_id(key),
        form.continue_label,
    ))
    .with_child(UiNode::button(user_form_screen_id(key), "Open the screen"))
    .with_child(UiNode::button(user_form_dismiss_id(key), "Dismiss"))
}

fn computer_handoff_node(handoff: &ComputerHandoffSnap) -> UiNode {
    let key = &handoff.card_key;
    let mut card =
        UiNode::dialog(computer_handoff_card_id(key), "Computer").with_child(UiNode::status(
            format!("computer-handoff-badge-{key}"),
            handoff.status.pill(),
        ));
    if !handoff.status.is_live() {
        return card;
    }
    card = card
        .with_child(UiNode::status(
            format!("computer-handoff-instruction-{key}"),
            handoff.instruction.clone(),
        ))
        .with_child(UiNode::button(
            computer_handoff_takeover_id(key),
            "Take over",
        ))
        .with_child(UiNode::button(computer_handoff_done_id(key), "I'm done"))
        .with_child(UiNode::button(computer_handoff_skip_id(key), "Skip"));
    card
}

/// One routine as the driver sees it.
///
/// A routine is one schedule, so the trigger says what kind it is and carries the one fact
/// worth asserting on: the cron line the server keeps, or the URL it minted.
fn routine_snap(routine: &crate::state::AgentRoutine, state: &AppState) -> RoutineSnap {
    let on_the_server = routine.saved.is_some();
    let mut snap = RoutineSnap {
        id: routine.id.clone(),
        name: if routine.name.trim().is_empty() {
            "Untitled routine".to_string()
        } else {
            routine.name.clone()
        },
        kind: "draft",
        cron: None,
        active: routine.active,
        webhook_url: None,
        webhook_key: None,
        runs: routine
            .runs
            .iter()
            .map(|run| match &run.outcome {
                crate::state::RunOutcome::Ran { run_id, status } => RunLineSnap::Ran {
                    run_id: run_id.clone(),
                    cause: run.cause_label(),
                    status: match status {
                        crate::opengrok::ScheduleRunStatus::Running => "running",
                        crate::opengrok::ScheduleRunStatus::Waiting => "waiting",
                        crate::opengrok::ScheduleRunStatus::Ok => "ok",
                        crate::opengrok::ScheduleRunStatus::Error => "error",
                        crate::opengrok::ScheduleRunStatus::Other => "other",
                    },
                },
                crate::state::RunOutcome::Skipped { reason } => RunLineSnap::Skipped {
                    at_ms: run.started_at_ms,
                    cause: run.cause_label(),
                    reason: reason.clone(),
                },
            })
            .collect(),
        notes: crate::state::routine_notes(state.routine_routes_missing, on_the_server),
        runs_unavailable: on_the_server && state.routine_routes_missing.runs,
        unsaved: state
            .routine_unsaved
            .get(&routine.id)
            .map(crate::state::unsaved_lines)
            .unwrap_or_default(),
        wakes: routine
            .triggers
            .iter()
            .map(|trigger| {
                (
                    trigger.label(),
                    matches!(trigger, crate::state::RoutineTrigger::Webhook { .. }),
                )
            })
            .collect(),
        zone: state.routine_zone(routine).note().map(str::to_string),
    };
    match routine.triggers.first() {
        Some(crate::state::RoutineTrigger::Schedule { spec, .. }) => {
            snap.kind = "cron";
            snap.cron = spec.to_cron().ok();
        }
        Some(crate::state::RoutineTrigger::Webhook { url, key, .. }) => {
            snap.kind = "webhook";
            snap.webhook_url = Some(url.clone());
            snap.webhook_key = Some(key.clone());
        }
        Some(crate::state::RoutineTrigger::Event { .. }) | None => {}
    }
    snap
}

/// The open routine's four header icons, as the panel draws them: each enabled while it can
/// act, with the reason as the value of one that cannot.
fn routine_header_nodes(routine: &RoutineSnap, history_open: bool) -> Vec<UiNode> {
    let on_the_server = routine.kind != "draft";
    let mut history = UiNode::button(
        ids::ROUTINE_HISTORY_TOGGLE,
        if history_open {
            "Back to the routine"
        } else {
            "Run history"
        },
    );
    if history_open {
        history.states.push("selected".into());
    }
    let mut thread =
        UiNode::button(ids::ROUTINE_OPEN_THREAD, "Open in thread").with_enabled(on_the_server);
    if !on_the_server {
        thread = thread.with_value("Choose when it runs first: its thread is on the server");
    }
    let mut run = UiNode::button(ids::ROUTINE_RUN_NOW, "Run it now")
        .with_enabled(on_the_server && !routine.run_dead());
    if routine.run_dead() {
        run = run.with_value(crate::state::ROUTINE_RUN_UNAVAILABLE);
    } else if !on_the_server {
        run = run.with_value("Choose when it runs first, then run it");
    }
    vec![
        history,
        thread,
        run,
        UiNode::button(ids::ROUTINE_DELETE, "Delete"),
    ]
}

/// "When to run" on the open routine: +, a node per wake with its ✎ and 🗑, and the wake editor
/// while it is open, each enabled while a click would act and with the reason as the value of
/// one that would not.
fn routine_wake_nodes(
    open: &RoutineSnap,
    editor: Option<&(crate::state::WakeEditor, crate::state::WakeStatus)>,
) -> Vec<UiNode> {
    use crate::state::{LAST_WAKE_STAYS, ONE_WAKE_PER_ROUTINE, ROUTINE_EDIT_UNAVAILABLE};
    let can_add = open.wakes.is_empty();
    let mut add = UiNode::button(ids::ROUTINE_WAKE_ADD, "Add a schedule")
        .with_enabled(can_add && editor.is_none());
    if !can_add {
        add = add.with_value(ONE_WAKE_PER_ROUTINE);
    }
    let mut nodes = vec![add];
    for (at, (summary, webhook)) in open.wakes.iter().enumerate() {
        let can_edit = *webhook || !open.cant_change();
        let mut edit = UiNode::button(ids::routine_wake_edit(at), "Edit").with_enabled(can_edit);
        if !can_edit {
            edit = edit.with_value(ROUTINE_EDIT_UNAVAILABLE);
        }
        let mut wake = UiNode::status(ids::routine_wake(at), summary.clone())
            .with_child(edit)
            .with_child(
                UiNode::button(ids::routine_wake_delete(at), "Delete")
                    .with_enabled(false)
                    .with_value(LAST_WAKE_STAYS),
            );
        if let Some(zone) = open.zone.as_ref().filter(|_| !*webhook) {
            wake = wake.with_child(UiNode::status(ids::routine_wake_zone(at), zone.clone()));
        }
        nodes.push(wake);
    }
    if let Some((editor, status)) = editor {
        nodes.push(wake_editor_node(editor, status));
    }
    nodes
}

/// The wake editor as the panel draws it: the tabs, what the open tab picks with, and under
/// them what was picked, when it would next run or why it cannot be saved, and the two buttons.
fn wake_editor_node(
    editor: &crate::state::WakeEditor,
    status: &crate::state::WakeStatus,
) -> UiNode {
    use crate::state::{WakeBox, WakeTab};
    let mut node =
        UiNode::dialog(ids::ROUTINE_WAKE_EDITOR, "When to run").with_value(editor.tab.word());
    for tab in WakeTab::ALL {
        let mut button = UiNode::button(ids::routine_wake_tab(tab), tab.label())
            .with_enabled(editor.tab_open(tab));
        if editor.tab == tab {
            button.states.push("selected".into());
        }
        node = node.with_child(button);
    }
    let typed = |which: WakeBox, label: &str, value: &str| {
        UiNode::textbox(ids::routine_wake_box(which), label).with_value(value)
    };
    let steppers = |which: WakeBox| {
        [true, false].map(|up| {
            UiNode::button(
                format!(
                    "{}-{}",
                    ids::routine_wake_box(which),
                    if up { "up" } else { "down" }
                ),
                if up { "Up" } else { "Down" },
            )
        })
    };
    let chip =
        |id: String, label: String, on: bool| UiNode::new(id, "checkbox", label).with_checked(on);
    let time_tab = matches!(
        editor.tab,
        WakeTab::Daily | WakeTab::Weekly | WakeTab::Monthly
    );
    match editor.tab {
        WakeTab::Every => {
            node = node.with_child(typed(WakeBox::Every, "Every", &editor.every));
            for stepper in steppers(WakeBox::Every) {
                node = node.with_child(stepper);
            }
            for unit in [
                crate::state::ScheduleUnit::Minutes,
                crate::state::ScheduleUnit::Hours,
                crate::state::ScheduleUnit::Days,
            ] {
                let mut button = UiNode::button(ids::routine_wake_unit(unit), format!("{unit:?}"));
                if editor.spec.unit == unit {
                    button.states.push("selected".into());
                }
                node = node.with_child(button);
            }
        }
        WakeTab::Weekly => {
            for day in 0u8..7 {
                node = node.with_child(chip(
                    ids::routine_wake_day(day),
                    crate::cron_spec::WEEKDAYS[usize::from(day)].to_string(),
                    editor.spec.weekdays.contains(&day),
                ));
            }
        }
        WakeTab::Monthly => {
            for date in 1u8..=31 {
                node = node.with_child(chip(
                    ids::routine_wake_date(date),
                    date.to_string(),
                    editor.spec.month_days.contains(&date),
                ));
            }
        }
        WakeTab::Cron => {
            node = node.with_child(typed(WakeBox::Cron, "Cron line", &editor.spec.expr));
            if status.numbered_weekdays {
                node = node.with_child(UiNode::status(
                    ids::ROUTINE_WAKE_CRON_NOTE,
                    crate::state::NUMBERED_WEEKDAYS,
                ));
            }
        }
        WakeTab::Daily | WakeTab::Webhook => {}
    }
    if time_tab {
        node = node.with_child(typed(WakeBox::Hour, "Hour", &editor.hour));
        for stepper in steppers(WakeBox::Hour) {
            node = node.with_child(stepper);
        }
        node = node.with_child(typed(WakeBox::Minute, "Minute", &editor.minute));
        for stepper in steppers(WakeBox::Minute) {
            node = node.with_child(stepper);
        }
        for (id, label, pm) in [
            (ids::ROUTINE_WAKE_AM, "AM", false),
            (ids::ROUTINE_WAKE_PM, "PM", true),
        ] {
            let mut button = UiNode::button(id, label);
            if editor.pm == pm {
                button.states.push("selected".into());
            }
            node = node.with_child(button);
        }
        for month in 1u8..=12 {
            node = node.with_child(chip(
                ids::routine_wake_month(month),
                crate::cron_spec::MONTHS[usize::from(month) - 1].to_string(),
                editor.spec.months.contains(&month),
            ));
        }
    }
    node = node.with_child(UiNode::status(
        ids::ROUTINE_WAKE_SUMMARY,
        status.summary.clone(),
    ));
    if let Some(zone) = editor
        .zone
        .note()
        .filter(|_| editor.tab != WakeTab::Webhook)
    {
        node = node.with_child(
            UiNode::status(
                ids::ROUTINE_WAKE_ZONE,
                crate::components::computer::zone_words(zone),
            )
            .with_value(zone),
        );
    }
    if let Some(next) = &status.next {
        node = node.with_child(UiNode::status(ids::ROUTINE_WAKE_NEXT, next.clone()));
    }
    if let Some(error) = &status.error {
        node = node.with_child(UiNode::status(ids::ROUTINE_WAKE_ERROR, error.clone()));
    }
    let done = if editor.kind == Some(crate::opengrok::ScheduleKind::Webhook) {
        "Done"
    } else {
        "Save"
    };
    node.with_child(UiNode::button(ids::ROUTINE_WAKE_SAVE, done).with_enabled(status.can_save()))
        .with_child(UiNode::button(ids::ROUTINE_WAKE_CANCEL, "Cancel"))
}

/// The routine's row and everything reachable from it.
///
/// The two triggers are here only while the routine has none, and the webhook's three only
/// while it has one: `assert --exists false` on either is then the whole question — "this
/// routine already has its trigger", "this one is not a webhook" — without reading a word.
fn routine_node(routine: &RoutineSnap) -> UiNode {
    let mut node = UiNode::listitem(ids::routine(&routine.id), routine.name.clone());
    node.states.push(routine.kind.to_string());
    node.states
        .push(if routine.active { "active" } else { "paused" }.to_string());
    if let Some(cron) = &routine.cron {
        node = node.with_value(cron.clone());
    }
    node = node.with_child(
        UiNode::new(
            ids::routine_active(&routine.id),
            "switch",
            if routine.active { "Active" } else { "Paused" },
        )
        .with_checked(routine.active),
    );
    if routine.kind == "draft" {
        node = node
            .with_child(UiNode::button(
                ids::routine_trigger_schedule(&routine.id),
                "On a schedule",
            ))
            .with_child(UiNode::button(
                ids::routine_trigger_webhook(&routine.id),
                "Webhook",
            ));
    }
    // Tied to the kind and not to whether the server filled either in: a webhook whose key
    // came back empty is a fact worth reading off an empty value, not one worth hiding the
    // URL over.
    if routine.kind == "webhook" {
        node = node
            .with_child(
                UiNode::status(ids::routine_webhook_url(&routine.id), "POST to")
                    .with_value(routine.webhook_url.clone().unwrap_or_default()),
            )
            .with_child(
                UiNode::status(ids::routine_webhook_key(&routine.id), "key")
                    .with_value(routine.webhook_key.clone().unwrap_or_default()),
            )
            .with_child(UiNode::button(
                ids::routine_rotate(&routine.id),
                "Rotate key",
            ));
    }
    if routine.kind != "draft" {
        node = node
            .with_child(
                UiNode::button(ids::routine_test(&routine.id), "Test run")
                    .with_enabled(!routine.run_dead()),
            )
            .with_child(UiNode::button(
                ids::routine_thread(&routine.id),
                "Open thread",
            ));
    }
    for (word, words) in &routine.notes {
        node = node.with_child(UiNode::status(ids::routine_note(&routine.id, word), *words));
    }
    if !routine.unsaved.is_empty() {
        node = node.with_child(
            UiNode::status(ids::routine_unsaved(&routine.id), "Not saved").with_value(
                routine
                    .unsaved
                    .iter()
                    .map(|(label, value)| format!("{label}: {value}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        );
    }
    if routine.runs_unavailable {
        node = node.with_child(UiNode::status(
            ids::routine_runs_unavailable(&routine.id),
            crate::state::ROUTINE_RUNS_UNAVAILABLE,
        ));
    }
    for line in &routine.runs {
        node = node.with_child(match line {
            RunLineSnap::Ran {
                run_id,
                cause,
                status,
            } => UiNode::status(ids::routine_run(&routine.id, run_id), cause.clone())
                .with_value(status.to_string()),
            RunLineSnap::Skipped {
                at_ms,
                cause,
                reason,
            } => {
                let mut skipped =
                    UiNode::status(ids::routine_skipped(&routine.id, *at_ms), cause.clone())
                        .with_value(reason.clone());
                skipped.states.push("skipped".into());
                skipped
            }
        });
    }
    node.with_child(UiNode::button(ids::routine_delete(&routine.id), "Delete"))
}

fn save_login_node(offer: &SaveLoginSnap) -> UiNode {
    UiNode::dialog(
        save_login_card_id(&offer.form_entry_id),
        format!("Save login for {} as {}?", offer.origin, offer.username),
    )
    .with_child(UiNode::button(
        save_login_save_id(&offer.form_entry_id),
        "Save",
    ))
    .with_child(UiNode::button(
        save_login_skip_id(&offer.form_entry_id),
        "Not now",
    ))
}

/// One row of the list: its title and the name under it. The picked one says so.
fn site_login_node(row: &SiteLoginRecord, selected: bool) -> UiNode {
    let mut node = UiNode::listitem(
        format!("settings-login-row-{}", row.id),
        format!("{} · {}", login_title(row), row.username),
    );
    if selected {
        node.states.push("selected".to_string());
    }
    node
}

/// The detail pane for the picked row: where the password is, the notes as a field the
/// driver can read, the last use, and Delete. Never the password.
fn site_login_detail_node(login: &SiteLoginSnap) -> UiNode {
    let row = &login.row;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let last_used = row
        .last_used_at_ms
        .map(|at| crate::site_login::relative_time(at, now_ms))
        .unwrap_or_else(|| "Never".to_string());
    UiNode::dialog(
        format!("settings-login-detail-{}", row.id),
        login_title(row).to_string(),
    )
    .with_child(
        UiNode::status(format!("settings-login-username-{}", row.id), "User Name")
            .with_value(row.username.clone()),
    )
    .with_child(
        UiNode::status(format!("settings-login-website-{}", row.id), "Website")
            .with_value(row.origin.clone()),
    )
    .with_child(UiNode::status(
        format!("settings-login-where-{}", row.id),
        crate::site_login::where_the_secret_is(&row.kind, login.on_this_mac),
    ))
    .with_child(
        UiNode::textbox(format!("settings-login-notes-{}", row.id), "Notes")
            .with_value(row.notes.clone()),
    )
    .with_child(
        UiNode::status(format!("settings-login-last-used-{}", row.id), "Last used")
            .with_value(last_used),
    )
    .with_child(UiNode::status(
        format!("settings-login-code-{}", row.id),
        if login.has_code {
            "A code is minted here from the seed on this Mac"
        } else {
            "No authenticator code for this login"
        },
    ))
    .with_child(UiNode::button(
        format!("settings-login-delete-{}", row.id),
        "Delete",
    ))
}

/// One row of the list: what the skill is called and what it is FOR — its description, which is
/// the sentence the model chooses by. What it SAYS is not on the row; that is fetched when the
/// skill is opened and read off its pane, at [`ids::skill_body`].
///
/// Where the prose came from rides as a state, and a draft says so: a skill with no prose in it
/// cannot be invoked, which is the one thing worth knowing about it.
fn skill_node(skill: &SkillSnap, open: bool) -> UiNode {
    let mut node = UiNode::listitem(ids::skill(&skill.id), skill.name.clone())
        .with_value(skill.description.clone());
    node.states.push(skill.source.to_string());
    if skill.draft {
        node.states.push("draft".to_string());
    }
    if !skill.enabled {
        node.states.push("off".to_string());
    }
    if open {
        node.states.push("selected".to_string());
    }
    node.with_child(UiNode::button(ids::skill_delete(&skill.id), "Delete"))
}

/// The pane for the open skill: the prose, how stale it is, and the way out of it.
///
/// The prose is here because this is the only place it is. A row carries what a skill is FOR;
/// what it SAYS is fetched when it is opened, and a driver that had just written one down could
/// otherwise not check that a word of it was kept.
fn skill_detail_node(skill: &OpenSkillSnap) -> UiNode {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut node = UiNode::dialog(ids::skill_detail(&skill.id), skill.name.clone());
    // Refused, still coming, or here — three states the pane has and a driver has to be able to
    // tell apart. "Here" is not "has prose": a draft arrives with none, and reading the empty
    // body as "still coming" left a driver waiting at a pane that was already finished.
    //
    // A refusal with no skill under it is the whole pane. One WITH a skill under it came from
    // the switch, and goes beside the switch, because that is what it is about.
    if !skill.arrived {
        let said = match &skill.error {
            Some(error) => UiNode::status(ids::SKILL_ERROR, error.clone()),
            None => UiNode::status(ids::SKILL_LOADING, "Loading…"),
        };
        return node
            .with_child(said)
            .with_child(UiNode::button(ids::SKILL_CLOSE, "Close"));
    }
    let updated = if skill.updated_at_ms > 0 {
        short_relative_time(skill.updated_at_ms, now_ms)
    } else {
        NEVER_UPDATED.to_string()
    };
    // The one control on this pane that changes what the skill can do: off means nothing may
    // run it, and a lesson a model wrote from a recording is born off. Where it stands rides as
    // `checked`, so an assert does not have to match the sentence beside it, and it is not
    // enabled while the server is being told — a switch pressed twice sends two answers about
    // one flag.
    node = node.with_child(
        UiNode::new(
            ids::skill_enabled(&skill.id),
            "switch",
            if skill.enabled {
                "Switched on"
            } else {
                "Switched off"
            },
        )
        .with_checked(skill.enabled)
        .with_enabled(!skill.switching),
    );
    // Off and never switched on by anybody, which is not the same as off: a lesson a model
    // wrote and nobody has read, as against one somebody read and switched off again. The
    // screen's own function answers it, so the tree and the pane cannot disagree — they did,
    // because each worked it out for itself and the tree called every skill on a server that
    // sends no stamp unread, switched on or not.
    if waiting_to_be_read(skill.enabled, skill.approved_at_ms)
        && let Some(switch) = node.children.last_mut()
    {
        switch.states.push("unread".to_string());
    }
    if let Some(error) = &skill.error {
        node = node.with_child(UiNode::status(ids::SKILL_ERROR, error.clone()));
    }
    node = node.with_child(
        // The sentence the pane draws where the prose would be, with nothing under it: a draft
        // is a skill that has none, not a skill whose prose could not be read.
        UiNode::status(
            ids::skill_body(&skill.id),
            if skill.body.is_empty() {
                NOTHING_WRITTEN_YET
            } else {
                "Instructions"
            },
        )
        .with_value(skill.body.clone()),
    );
    node.with_child(UiNode::status(ids::skill_updated(&skill.id), "Updated").with_value(updated))
        .with_child(UiNode::button(
            ids::skill_detail_delete(&skill.id),
            "Delete",
        ))
        .with_child(UiNode::button(ids::SKILL_CLOSE, "Close"))
}

/// What a second create is told while the first is still going.
///
/// The person cannot ask twice — Save goes dead and the sheet closes on the way out — so this is
/// for the driver, which would otherwise be told nothing and conclude its skill was taken.
const ALREADY_SAVING: &str = "a skill is already being saved";

/// What a control on the Skills page says when the Skills page is not what is on screen.
fn skills_off_screen(target: &str) -> String {
    format!(
        "`{target}` is on Settings → Skills, which is not what is on screen: open it with \
         `skills.open`"
    )
}

/// What to use instead of typing into the Create sheet. One sentence, because the three fields
/// and the Save button are refused for the same reason, and a driver has to be told the same
/// thing whichever of the four it reached for.
const SHEET_IS_NOT_TYPED_INTO: &str = "the New skill sheet's fields belong to the window and the driver cannot type into them: \
     use `skill.create --arg name= [--arg description= --arg body=]`";

/// A recipe row's id, and only a row's. Every control on the recipe page is named
/// `recipe-<something>` too, so a bare prefix match turned a click on a version tab into a
/// fetch of a recipe called "version-1" — the page then said "no such recipe" and the driver
/// could not work the page at all. A recipe's id is what the server mints, `rcp_…`.
fn recipe_row_target(target: &str) -> Option<String> {
    let rest = target.strip_prefix("recipe-")?;
    rest.starts_with("rcp_").then(|| rest.to_string())
}

/// The three-way choice as the driver sees it: the current word, and one button per word.
fn egress_policy_node(current: crate::opengrok::LocalExecMode) -> UiNode {
    UiNode::new("egress-policy-menu", "menu", current.label())
        .with_child(UiNode::button("egress-policy-bypass", "Always allow"))
        .with_child(UiNode::button("egress-policy-ask", "Ask every time"))
        .with_child(UiNode::button("egress-policy-never", "Never allow"))
}

/// This Mac's standing rules as Settings → Computer draws them under its mode: why they could
/// not be read, the line saying there are none, and each list that has a rule on it.
fn local_rules_nodes(rules: &LocalRules) -> Vec<UiNode> {
    let mut nodes = Vec::new();
    if let Some(error) = &rules.error {
        nodes.push(UiNode::status(ids::LOCAL_RULES_ERROR, error.clone()));
    }
    if rules.is_empty() {
        // A list that could not be read is not a list with nothing on it.
        if rules.listed {
            nodes.push(UiNode::status(ids::LOCAL_RULES_EMPTY, NO_LOCAL_RULES));
        }
        return nodes;
    }
    for kind in RuleKind::ALL {
        let rows = rules.rows(kind);
        if rows.is_empty() {
            continue;
        }
        let mut list = UiNode::list(ids::local_rules(kind), rule_list_title(kind))
            .with_value(rows.len().to_string());
        for (n, row) in rows.iter().enumerate() {
            list = list.with_child(local_rule_node(kind, n, row, rules));
        }
        nodes.push(list);
    }
    nodes
}

/// One rule, named and valued by its command. An allow the gate never reads carries `inert`
/// as a state, so an assert does not have to match the sentence, and the sentence under it with
/// the server's reason as its value. One whose Remove is with the server carries `removing`,
/// and its button is dead, as on screen.
fn local_rule_node(kind: RuleKind, n: usize, row: &LocalRuleRow, rules: &LocalRules) -> UiNode {
    let removing = rules.is_removing(kind, &row.pattern);
    let mut node = UiNode::listitem(ids::local_rule(kind, n), row.pattern.clone())
        .with_value(row.pattern.clone());
    if let Some(reason) = &row.inert {
        node.states.push("inert".to_string());
        node = node.with_child(
            UiNode::status(ids::local_rule_inert(kind, n), not_in_effect_line(reason))
                .with_value(reason.clone()),
        );
    }
    if removing {
        node.states.push("removing".to_string());
    }
    if let Some(why) = rules.not_removed(kind, &row.pattern) {
        node = node.with_child(UiNode::status(ids::local_rule_error(kind, n), why));
    }
    node.with_child(
        UiNode::button(ids::local_rule_remove(kind, n), remove_label(removing))
            .with_enabled(!removing),
    )
}

/// `approval-<call_id>-<verb>` → the answer it stands for.
fn approval_target(target: &str) -> Option<(String, LocalExecResolution)> {
    let rest = target.strip_prefix("approval-")?;
    let verbs = [
        ("-allow-once", LocalExecResolution::AllowOnce),
        ("-deny-once", LocalExecResolution::DenyOnce),
        ("-always", LocalExecResolution::Always),
        ("-never", LocalExecResolution::Never),
    ];
    verbs.iter().find_map(|(suffix, resolution)| {
        rest.strip_suffix(suffix)
            .filter(|call_id| !call_id.is_empty())
            .map(|call_id| (call_id.to_string(), *resolution))
    })
}

fn invoke_arg_str(args: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        args.get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// The id Settings → Relay's tab had, which answers nothing now and says where the relay moved.
const RETIRED_RELAY_TAB: &str = "settings-tab-reply-source";

/// The state on a reply's badge while the door is the person's plan through their computer, so an
/// assert need not match the words. A driver's word, kept as it was when the app said "Mac".
const VIA_MAC: &str = "via-mac";

/// This computer's card's opencodex fields as the card draws them, and General's two pickers'
/// kept settings. The typed key is not copied here: the tree says only that one is waiting for
/// Save, never what it is.
#[derive(Default)]
struct ReplySourceSnap {
    /// The setting and what the card has changed of it, with no key in them.
    settings: crate::state::ReplySourceSettings,
    unsaved: bool,
    can_save: bool,
    /// The line beside Save (`AppState::reply_source_hint`).
    hint: Option<&'static str>,
    /// This computer's opencodex address and key as the card draws them, without the typed key.
    relay: RelaySnap,
    /// Default for new Bots: whether the server keeps one, and what.
    new_bots: crate::state::DefaultForNewBots,
    /// The Relay-off fallback: whether the server keeps one, and what.
    plan_fallback: crate::state::RelayOffFallback,
}

impl ReplySourceSnap {
    fn from_state(state: &AppState) -> Self {
        let settings = &state.reply_source;
        Self {
            settings: settings.without_key(),
            unsaved: settings.is_unsaved(),
            can_save: state.reply_source_can_save(),
            hint: state.reply_source_hint(),
            relay: RelaySnap {
                address: settings
                    .relay_address_draft
                    .clone()
                    .unwrap_or_else(|| state.relay_mac.shown_address()),
                has_key: state.relay_mac.has_key,
                key_typed: settings.relay_key_draft.is_some(),
            },
            new_bots: state.default_for_new_bots(),
            plan_fallback: state.relay_off_fallback(),
        }
    }
}

/// This computer's opencodex fields as its card draws them. The key is never here: the tree says
/// only that the Keychain holds one, or that one waits for Save.
#[derive(Default)]
struct RelaySnap {
    /// opencodex's address as the field shows it.
    address: String,
    has_key: bool,
    key_typed: bool,
}

/// `Default` is the host with nothing in it — signed out, no sessions, no panel. Typing ops
/// are planned without reading the app at all, so that empty host is what the tests plan
/// against; everything else comes through [`NativeChatHost::from_app`].
#[derive(Default)]
pub struct NativeChatHost {
    ready: bool,
    sidebar_hidden: bool,
    theme_mode: String,
    sessions: Vec<SessionSnap>,
    account_open: bool,
    voice_open: bool,
    signed_in: bool,
    account_label: String,
    auth_error: Option<String>,
    login_email: String,
    login_password: String,
    last_assistant: String,
    /// The transcript keeps its newest row in view, as the transcript published it.
    transcript_following: bool,
    /// The steps and thoughts of that same reply.
    reply_run: ReplyRunSnap,
    /// The open thread's working line, which is that thread's own: it is kept per thread, like
    /// the live turn below, so a bot working next door cannot put a line here and cannot take
    /// this one away.
    bot_status: Option<String>,
    /// The open thread has a turn in flight. It is what the composer's button is showing, and
    /// the two now answer the same question the same way — the button has always read the live
    /// turn, which is kept per thread, and the line beside it was once one label for the whole
    /// app.
    turn_in_flight: bool,
    /// Messages the open thread is holding until it is idle.
    queued_sends: usize,
    agent_settings_open: bool,
    /// The open bot's tools as its settings list them, and whether the card is open to them.
    agent_tools: Option<crate::state::ToolList>,
    /// The open Bot's ceiling as its Tools card draws it: see [`AppState::ceiling_card`].
    agent_ceiling: Option<CeilingCard>,
    /// The open Bot's skills as its Skills card draws them: see [`AppState::skills_card`].
    agent_skills: Option<SkillsCard>,
    agent_skills_open: bool,
    /// The open bot's usage this month, as its settings' Usage card shows it (#138).
    agent_usage: Option<crate::state::UsageReport>,
    agent_usage_open: bool,
    agent_tools_open: bool,
    avatar_editor_open: bool,
    approvals: Vec<ApprovalSnap>,
    computer_open: bool,
    /// The coworker's computer as the pane sees it: "<state>; screen: yes|no",
    /// "endpoint missing", or "unknown".
    computer_status: String,
    /// Why the server could not give the open bot a computer, as the pane shows it in place of
    /// "No computer yet", and the server's code for it.
    computer_error: Option<(String, String)>,
    /// Get a computer is with the server for the open bot.
    computer_asking: bool,
    computer_update_label: String,
    computer_reset_label: String,
    /// The confirm dialog's question, when it is open.
    computer_confirm: Option<String>,
    /// The update banner's two lines, when one is showing.
    update_banner: Option<String>,
    /// The reconnecting pill's two lines and the machine it names, while something cannot be
    /// reached. Absent is the whole assertion for "it went away", which is the half of this that
    /// the transcript line it replaced could never be checked for.
    reconnect: Option<(String, &'static str)>,
    /// The signed-out banner's two lines, while the server does not know who the app is.
    ///
    /// Separate from `reconnect` on purpose, and never both in one field: the two look alike on
    /// screen and are opposite in the one way that matters, which is what makes them go away.
    /// A driver that could not tell them apart is a driver that would have passed the bug.
    signed_out: Option<String>,
    /// The open thread's last turn did not go through, and the feed is offering it again.
    can_retry_turn: bool,
    /// The open thread's last turn is one the person's plan could not answer, and the feed offers
    /// it again on the server's keys.
    can_send_on_server: bool,
    /// The open thread's held messages the server holds for the person's Mac, by bubble id.
    waiting_for_mac: Vec<String>,
    /// The open Bot's replies go through the person's own plan, which the server meters none of:
    /// the Usage card says so (`AppState::replies_on_plan`).
    replies_on_plan: bool,
    /// The open Bot's model picker, as both of its places draw it (`AppState::model_pick`).
    model_pick: Option<crate::opengrok::ModelPick>,
    /// Its popover: open or shut, its list or its controls, its search and its window.
    model_picker: PickerView,
    /// The server's words for the picker's last change that did not go through.
    picker_note: Option<String>,
    /// Default for new Bots' picker, while the server keeps such a default
    /// (`AppState::new_bots_pick`), its popover, what its last change that did not go as asked
    /// came to, and whether a change is with the server.
    new_bots_pick: Option<crate::opengrok::ModelPick>,
    new_bots_picker: PickerView,
    new_bots_note: Option<String>,
    new_bots_busy: bool,
    /// The same of the Relay-off fallback's picker (`AppState::plan_fallback_pick`).
    plan_fallback_pick: Option<crate::opengrok::ModelPick>,
    plan_fallback_picker: PickerView,
    plan_fallback_note: Option<String>,
    plan_fallback_busy: bool,
    /// `GET /models`' word on why its list is not fuller, which the popover's list shows.
    model_note: Option<String>,
    /// The Recipes page, when it fills the main slot: its rows, and the recipe open in it.
    recipes_open: bool,
    recipes_filter: &'static str,
    recipes: Vec<RecipeSnap>,
    recipe_open: Option<String>,
    /// The open recipe's Run and what it has run, when its detail has come back.
    recipe_detail: Option<RecipeDetailSnap>,
    /// Settings → Computer's cards, one to each enrolled computer, as the window draws them.
    computers: Vec<ComputerCard>,
    /// The open bot's routines, as the Computer pane lists them.
    routines: Vec<RoutineSnap>,
    /// The red line of the routine editor, while one is open and shows one.
    routine_error: Option<String>,
    /// The routine open in the Computer pane, by id.
    routine_editor: Option<String>,
    /// The open routine's panel shows its Run history in place of its fields.
    routine_history_open: bool,
    /// The question Delete is asking about a routine: its title and what deleting it does.
    routine_delete_question: Option<(String, String)>,
    /// The wake editor while it is open, and what it says under what is picked.
    routine_wake_editor: Option<(crate::state::WakeEditor, crate::state::WakeStatus)>,
    /// The open thread's routine, when it is one of the bot's routines' threads: its name and
    /// the server's word for what fires it.
    routine_thread: Option<(String, String)>,
    /// A press of the Bot chip goes back to the Bot's own chat (`AppState::bot_chip_goes_home`).
    bot_chip_goes_home: bool,
    /// How many of the open routine thread's bubbles carry a routine's instruction caption.
    routine_instructions: usize,
    /// The composer's panel, when one is open: which list it is, and the rows in it.
    composer_panel: Option<PanelMode>,
    /// The draft's chips, in order: (kind, label).
    composer_chips: Vec<(crate::components::chat_input::TokenKind, String)>,
    /// The draft's files: (name, `uploading` / `ready` / `failed`).
    composer_files: Vec<(String, &'static str)>,
    /// The files the open thread's messages carried, in order: (art id, filename, message id).
    sent_files: Vec<(String, String, String)>,
    panel_rows: Vec<PanelRow>,
    /// The recipe the next message runs, as the composer's bar shows it.
    recipe_bar: Option<RecipeBarSnap>,
    /// The skill the next message is sent with, as the chip in the draft names it.
    composer_skill: Option<crate::state::ActiveSkill>,
    /// The captions of the newest set of pictures in the transcript, in tile order.
    thumbs: Vec<String>,
    /// The picture overlay, while it is open.
    lightbox: Option<LightboxSnap>,
    /// The choice cards in the open thread, oldest first.
    choices: Vec<ChoiceSnap>,
    /// Idle user-form cards in the open thread.
    user_forms: Vec<UserFormSnap>,
    /// Open the screen → Grok Computer chrome (Take over / I'm done / Skip).
    computer_handoffs: Vec<ComputerHandoffSnap>,
    save_logins: Vec<SaveLoginSnap>,
    site_logins: Vec<SiteLoginSnap>,
    logins_tab: bool,
    /// What the last Add / Import / sync said on Settings → Logins.
    site_login_notice: Option<String>,
    site_login_error: Option<String>,
    /// The search field's text, the picked row and the Add sheet on Settings → Logins, as
    /// the page has them.
    site_login_query: String,
    site_login_selected: Option<String>,
    site_login_add_open: bool,
    /// Settings → Skills, as that page has it: the library for the open side of the toggle, a
    /// count for each side, the search, the skill on the pane and the Create sheet.
    skills: Vec<SkillSnap>,
    skills_tab: bool,
    skills_query: String,
    skills_scope: crate::state::SkillScope,
    skills_counts: crate::state::SkillCounts,
    skills_loading: bool,
    /// The three error slots the page has, kept apart here because they are apart on screen: a
    /// driver that read the list's slot for the sheet's would read a refusal nobody was shown.
    skills_error: Option<String>,
    skill_open: Option<OpenSkillSnap>,
    skill_add_open: bool,
    skill_add_error: Option<String>,
    skill_saving: bool,
    /// The skill the delete dialog is about, by name, while it is up.
    skill_delete_confirm: Option<String>,
    /// A tape being written up as a skill on a coworker's screen, and what became of it. That
    /// window draws its own sheet and this host cannot see into it; the app carries the fact.
    taught_skill: Option<TaughtSkill>,
    /// The window holding a refused tape is still open, so an offer to send it again is an
    /// offer something can honour. Closing that window takes the tape with it.
    taught_tape_in_hand: bool,
    computer_tab: bool,
    updates_tab: bool,
    /// Dedicated provisioned box: Route traffic icon on the Computer pane.
    route_traffic_on_bot_pane: bool,
    /// User-scope / shared box: Route traffic on Settings → Computer.
    route_traffic_in_user_settings: bool,
    egress_tunnel_enabled: bool,
    /// Box `egress_tunnel.ready` when the computer JSON exposed it.
    egress_tunnel_ready: Option<bool>,
    /// The computer's standing answer to the tunnel's card, when the server carries one. The
    /// control sits where Route traffic sits, and is absent (not merely hidden) without it.
    egress_policy: Option<crate::opengrok::LocalExecMode>,
    /// The permission dialog the shield badge opens, on screen.
    network_policy_open: bool,
    /// This Mac's standing rules, where Settings → Computer draws them: see
    /// [`AppState::this_mac_rules`]. `None` where it draws none.
    local_rules: Option<LocalRules>,
    /// Why local-exec stopped by itself, in the words Settings → Computer shows:
    /// [`AppState::local_exec_stopped`].
    local_exec_stopped: Option<String>,
    /// The person's connections and the services on offer, as Settings → Connections and the
    /// open bot's Connections card draw them (#2). Bot names come from `sessions`, which is the
    /// roster while signed in.
    connections: crate::state::AccountConnections,
    connections_tab: bool,
    /// This computer's relay fields on Settings → Computer, and General's two kept settings, as
    /// the window draws them, without the typed key.
    reply_source: ReplySourceSnap,
    /// Settings is on General, whose first section is Default models.
    general_tab: bool,
    /// The badges on the open thread's replies, oldest first: (message id, source).
    reply_sources: Vec<(String, crate::opengrok::ReplySource)>,
    pending: Option<Command>,
    /// Keys the last op asked the window for. The host has no window; the root view presses
    /// them (see [`Self::take_compose`]).
    compose: Option<ComposePlan>,
}

impl NativeChatHost {
    pub fn from_app(state: &AppState) -> Self {
        let active = state.active_conversation_id.clone();
        let sessions = if state.is_signed_in() {
            state
                .coworkers
                .iter()
                .map(|c| {
                    let thread = state.conversations.iter().find(|conv| conv.id == c.id);
                    SessionSnap {
                        active: state.active_coworker_id.as_ref() == Some(&c.id),
                        id: c.id.clone(),
                        title: c.name.clone(),
                        // The hover card's own line, so the tree says what the row says.
                        preview: Some(crate::components::sidebar::rail_preview(thread).0),
                        listed: thread.is_some_and(|conv| conv.known_only_from_list()),
                    }
                })
                .collect()
        } else {
            state
                .conversations
                .iter()
                .map(|c| SessionSnap {
                    active: active.as_ref() == Some(&c.id),
                    id: c.id.clone(),
                    title: c.title.clone(),
                    preview: None,
                    listed: false,
                })
                .collect()
        };
        Self {
            ready: true,
            theme_mode: state.theme_mode.clone(),
            sidebar_hidden: state.sidebar_hidden,
            sessions,
            account_open: state.is_app_settings_open,
            voice_open: state.is_voice_mode_open,
            signed_in: state.is_signed_in(),
            account_label: state
                .account
                .as_ref()
                .map(|a| a.display_name())
                .unwrap_or_else(|| "Sign in".into()),
            auth_error: state.auth_error.clone(),
            login_email: state.login_email.clone(),
            login_password: state.login_password.clone(),
            last_assistant: state
                .conversations
                .iter()
                .find(|c| Some(&c.id) == state.active_conversation_id.as_ref())
                .and_then(|c| c.messages.iter().rev().find(|m| !m.is_me && !m.hidden))
                .map(|m| m.content.clone())
                .unwrap_or_default(),
            transcript_following: state.transcript_following,
            reply_run: state
                .conversations
                .iter()
                .find(|c| Some(&c.id) == state.active_conversation_id.as_ref())
                .and_then(|c| c.messages.iter().rev().find(|m| !m.is_me && !m.hidden))
                .map(|m| {
                    ReplyRunSnap::from_message(m, &state.expanded_steps, state.show_turn_timing)
                })
                .unwrap_or_default(),
            bot_status: state.visible_bot_status(),
            turn_in_flight: state.is_turn_in_flight(),
            queued_sends: state.queued_send_count(),
            agent_settings_open: state.is_agent_settings_open(),
            agent_tools: state
                .coworker_tools
                .as_ref()
                .filter(|(owner, _)| state.active_coworker_id.as_deref() == Some(owner.as_str()))
                .map(|(_, list)| list.clone()),
            agent_ceiling: state.ceiling_card(),
            agent_skills: state.skills_card(),
            agent_skills_open: state.agent_skills_open,
            agent_tools_open: state.agent_tools_open,
            agent_usage_open: state.agent_usage_open,
            agent_usage: state
                .coworker_usage
                .as_ref()
                .filter(|(owner, _)| state.active_coworker_id.as_deref() == Some(owner.as_str()))
                .map(|(_, report)| report.clone()),
            avatar_editor_open: state.avatar_editor_open,
            approvals: state
                .open_approvals()
                .into_iter()
                .map(|spec| ApprovalSnap {
                    title: crate::components::gen_ui::approval_title(
                        &spec,
                        &state.active_bot_name(),
                        spec.is_review_an_action() && state.egress_tunnel_available(),
                    ),
                    local: spec.runs_on_this_mac(),
                    review: spec.is_review_an_action() && state.egress_tunnel_available(),
                    tunnel: spec.is_egress_tunnel(),
                    reason: spec.reason,
                    thread_id: spec.thread_id.unwrap_or_default(),
                    call_id: spec.call_id,
                    tool: spec.tool,
                })
                .collect(),
            computer_open: state.right_pane == crate::state::RightPane::Computer,
            computer_confirm: state.computer_confirm.map(|action| match action {
                crate::state::ComputerAction::Update => "Update this computer?".to_string(),
                crate::state::ComputerAction::Reset => "Reset this computer?".to_string(),
            }),
            computer_update_label: {
                let status = state.coworker_computer.as_ref();
                crate::components::computer::confirm_label(
                    status.is_some_and(|s| s.updating()),
                    crate::components::computer::update_rest_label(
                        status.is_some_and(|s| s.image_stale()),
                        status
                            .and_then(|s| s.image.as_ref())
                            .is_some_and(|image| !image.stale),
                    ),
                )
                .to_string()
            },
            computer_reset_label: crate::components::computer::confirm_label(
                state
                    .coworker_computer
                    .as_ref()
                    .is_some_and(|s| s.updating()),
                "Reset",
            )
            .to_string(),
            update_banner: state
                .computer_banner()
                .map(|(title, detail)| format!("{title} — {detail}")),
            reconnect: state.reachability_indicator().map(|(title, detail)| {
                (
                    format!("{title} — {detail}"),
                    state
                        .unreachable()
                        .map_or("", crate::opengrok::Unreachable::as_str),
                )
            }),
            signed_out: state
                .session_banner()
                .map(|(title, detail)| format!("{title} — {detail}")),
            can_retry_turn: state.retryable_turn().is_some(),
            can_send_on_server: state.plan_failed_turn().is_some(),
            waiting_for_mac: state.sends_waiting_for_mac(),
            replies_on_plan: state.replies_on_plan(),
            model_pick: state.model_pick(),
            model_picker: state.model_picker.clone(),
            picker_note: state.picker_note(PickerFor::Bot).map(str::to_string),
            new_bots_pick: state.new_bots_pick(),
            new_bots_picker: state.new_bots_picker.clone(),
            new_bots_note: state.picker_note(PickerFor::NewBots).map(str::to_string),
            new_bots_busy: state.picker_busy(PickerFor::NewBots),
            plan_fallback_pick: state.plan_fallback_pick(),
            plan_fallback_picker: state.plan_fallback_picker.clone(),
            plan_fallback_note: state
                .picker_note(PickerFor::PlanFallback)
                .map(str::to_string),
            plan_fallback_busy: state.picker_busy(PickerFor::PlanFallback),
            model_note: state.model_catalogue.note.clone(),
            computer_status: if state.computer_endpoint_missing {
                "endpoint missing".to_string()
            } else {
                state
                    .coworker_computer
                    .as_ref()
                    .map(|computer| {
                        format!(
                            "{}; screen: {}",
                            computer.state,
                            if computer.vnc_url().is_some() {
                                "yes"
                            } else {
                                "no"
                            }
                        )
                    })
                    .unwrap_or_else(|| "unknown".to_string())
            },
            // Read the way the pane reads it (`ComputerControls::from_state`), so the tree says
            // what the pane says.
            computer_error: state
                .coworker_computer
                .as_ref()
                .and_then(crate::opengrok::CoworkerComputer::why_no_computer)
                .map(|error| {
                    (
                        crate::components::computer::no_computer_line(error),
                        error.code.clone(),
                    )
                }),
            computer_asking: state.asking_for_computer(),
            recipes_open: state.page == crate::state::MainPage::Recipes,
            recipes_filter: state.recipes_filter.query(),
            recipes: state
                .recipes
                .iter()
                // The same rows the page draws, and the listing holds workflows too: a tree is
                // not on that page, so a driver must not be told there is a row there to click.
                .filter(|recipe| !recipe.is_workflow())
                .map(|recipe| RecipeSnap {
                    id: recipe.id.clone(),
                    name: recipe.name.clone(),
                    pending: recipe.is_pending_invite(),
                })
                .collect(),
            recipe_open: state.recipe_open_id.clone(),
            recipe_detail: state.recipe_open.as_ref().map(|detail| {
                // The page's default when nobody has picked: the first bot the recipe is granted
                // to, else the person's first bot (`RecipesView::picked_bot` without its first
                // clause). A bot picked from the chevron beside Run is the view's own and out of
                // this state's reach, so a driver runs the default; `recipe.run {bot}` names
                // another.
                let run_bot = detail
                    .my_bots
                    .iter()
                    .find(|bot| detail.is_granted(&bot.id))
                    .or_else(|| detail.my_bots.first())
                    .map(|bot| bot.id.clone());
                RecipeDetailSnap {
                    // What the toolbar asks before it enables Run: a bot, a version to play, and
                    // nothing else under way on the page. A save, delete or accept in flight
                    // clears the page's busy line when it lands, whatever started under it.
                    runnable: run_bot.is_some()
                        && detail.runnable_version().is_some()
                        && state.recipe_busy.is_none(),
                    run_bot,
                    running: state.recipe_busy.as_deref() == Some(crate::state::RECIPE_RUNNING),
                    result: state.recipe_run_result.as_ref().map(|run| {
                        if run.interrupted {
                            "interrupted"
                        } else if run.ok {
                            "ok"
                        } else {
                            "failed"
                        }
                    }),
                    error: state.recipe_error.clone(),
                    runs: detail
                        .runs
                        .iter()
                        .map(|run| (run.id.clone(), run.state.clone(), run.ok))
                        .collect(),
                }
            }),
            computers: computers::cards(state),
            routine_instructions: state
                .active_conversation_id
                .as_ref()
                .and_then(|id| state.conversations.iter().find(|c| &c.id == id))
                .map_or(0, |conv| {
                    conv.messages
                        .iter()
                        .filter(|m| {
                            !m.hidden
                                && crate::state::routine_instruction_caption(conv, m).is_some()
                        })
                        .count()
                }),
            bot_chip_goes_home: state.bot_chip_goes_home(),
            routine_editor: match (&state.computer_view, state.right_pane) {
                (
                    crate::state::ComputerView::Editor { id: Some(open) },
                    crate::state::RightPane::Computer,
                ) => Some(open.clone()),
                _ => None,
            },
            routine_history_open: state.routine_history_open,
            routine_delete_question: state
                .routine_delete_question()
                .map(|(title, what)| (title, what.to_string())),
            routine_wake_editor: state.routine_wake_editor.clone().map(|editor| {
                let status = editor.status(chrono::Utc::now());
                (editor, status)
            }),
            routine_error: match (&state.computer_view, state.right_pane) {
                (
                    crate::state::ComputerView::Editor { id: Some(open) },
                    crate::state::RightPane::Computer,
                ) => {
                    let on_the_server = state.active_coworker_id.as_deref().is_some_and(|bot| {
                        state
                            .coworker_routines(bot)
                            .iter()
                            .any(|routine| &routine.id == open && routine.saved.is_some())
                    });
                    crate::state::routine_trouble_line(
                        state.computer_action_error.as_deref(),
                        &crate::state::routine_notes(state.routine_routes_missing, on_the_server),
                    )
                }
                _ => None,
            },
            routine_thread: state
                .active_thread_origin()
                .map(|origin| (origin.routine_name.clone(), origin.word.clone())),
            routines: state
                .active_coworker_id
                .as_deref()
                .map(|id| state.coworker_routines(id))
                .unwrap_or_default()
                .iter()
                .map(|routine| routine_snap(routine, state))
                .collect(),
            composer_panel: state.composer_panel,
            composer_chips: state.composer_chips.clone(),
            composer_files: state.composer_files.clone(),
            sent_files: state
                .active_thread_messages()
                .iter()
                .filter_map(|message| {
                    state
                        .message_files
                        .get(&message.id)
                        .map(|files| (message.id.clone(), files))
                })
                .flat_map(|(message, files)| {
                    files
                        .iter()
                        .map(move |file| (file.id.clone(), file.filename.clone(), message.clone()))
                })
                .collect(),
            panel_rows: state
                .composer_panel
                .map(|mode| {
                    panel_rows(
                        mode,
                        &state.recipes,
                        &SkillLibrary {
                            skills: &state.your_skills,
                            loading: state.your_skills_loading,
                            error: state.your_skills_error.as_deref(),
                        },
                        state.active_recipe.as_ref(),
                    )
                })
                .unwrap_or_default(),
            recipe_bar: state.active_recipe.as_ref().map(|recipe| RecipeBarSnap {
                name: recipe.name.clone(),
                kind: recipe.kind,
                parameters: recipe
                    .parameters
                    .iter()
                    .map(|parameter| ParamSnap {
                        value: recipe.value(&parameter.name).map(str::to_string),
                        name: parameter.name.clone(),
                        required: parameter.required,
                    })
                    .collect(),
            }),
            composer_skill: state.active_skill.clone(),
            thumbs: last_screenshot_set(state)
                .iter()
                .map(|shot| shot.caption.clone())
                .collect(),
            lightbox: state.lightbox.as_ref().map(|open| LightboxSnap {
                index: open.index,
                total: open.shots.len(),
                caption: open
                    .current()
                    .map(|shot| shot.caption.clone())
                    .unwrap_or_default(),
            }),
            choices: choice_snaps(state),
            user_forms: state
                .visible_user_forms()
                .into_iter()
                .map(|spec| {
                    let key = spec.card_key().to_string();
                    let pill = spec.effective_resolution().map(|resolution| {
                        if resolution == crate::opengrok::FormResolution::Escalated {
                            crate::opengrok::FormResolution::Dismissed
                                .pill()
                                .to_string()
                        } else {
                            resolution.pill().to_string()
                        }
                    });
                    let fields = if pill.is_some() {
                        Vec::new()
                    } else {
                        let typed = state.user_form_typed.get(&key);
                        let picks = state.user_form_picks.get(&key);
                        spec.fields
                            .iter()
                            .map(|field| {
                                let raw = typed
                                    .and_then(|map| map.get(&field.id))
                                    .or_else(|| picks.and_then(|map| map.get(&field.id)))
                                    .cloned()
                                    .unwrap_or_default();
                                UserFormFieldSnap {
                                    id: field.id.clone(),
                                    label: if field.label.is_empty() {
                                        field.id.clone()
                                    } else {
                                        field.label.clone()
                                    },
                                    kind: field.kind,
                                    masked: field.masked(),
                                    value: raw,
                                }
                            })
                            .collect()
                    };
                    let current = state.saved_login_use.get(&key);
                    // The list shows until a pick is under way or held, as on screen.
                    let list_shows =
                        !current.is_some_and(|use_| use_.is_busy() || use_.ready().is_some());
                    let saved_logins = if pill.is_some() || !list_shows {
                        Vec::new()
                    } else {
                        state
                            .saved_logins_for_form(&spec)
                            .into_iter()
                            .map(|row| (row.id, row.username, row.origin))
                            .collect()
                    };
                    let saved_login_note = current.map(crate::site_login::SavedLoginUse::note);
                    let saved_login_held = current.is_some_and(|use_| use_.ready().is_some());
                    let passkey_register = spec.challenge_kind.as_deref() == Some("passkey")
                        && spec.passkey_mode.as_deref() == Some("register");
                    UserFormSnap {
                        title: if spec.title.is_empty() {
                            "Form".into()
                        } else {
                            spec.title.clone()
                        },
                        fields,
                        saved_logins,
                        saved_login_note,
                        saved_login_held,
                        passkey_register,
                        card_key: key,
                        pill,
                        continue_label: spec.continue_label(),
                    }
                })
                .collect(),
            computer_handoffs: state
                .visible_computer_handoffs()
                .into_iter()
                .map(|spec| ComputerHandoffSnap {
                    instruction: spec.handoff_prompt(),
                    card_key: spec.card_key().to_string(),
                    status: spec
                        .computer_handoff
                        .unwrap_or(ComputerHandoffStatus::ActionNeeded),
                })
                .collect(),
            save_logins: state
                .conversations
                .iter()
                .find(|conversation| {
                    Some(&conversation.id) == state.active_conversation_id.as_ref()
                })
                .map(|conversation| {
                    conversation
                        .messages
                        .iter()
                        .filter(|message| !message.hidden)
                        .flat_map(|message| message.parts.iter())
                        .filter_map(|part| match part {
                            ChatPart::SaveLogin(spec) => Some(SaveLoginSnap {
                                form_entry_id: spec.form_entry_id.clone(),
                                origin: spec.origin.clone(),
                                username: spec.username.clone(),
                            }),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            site_logins: state
                .site_logins
                .iter()
                .map(|row| SiteLoginSnap {
                    on_this_mac: state.site_logins_on_this_mac.contains(&row.id),
                    has_code: state.site_logins_with_code.contains(&row.id),
                    row: row.clone(),
                })
                .collect(),
            skills: state
                .skills
                .iter()
                .map(|skill| SkillSnap {
                    id: skill.id.clone(),
                    name: skill.name.clone(),
                    source: skill.source.word(),
                    description: skill.description.clone(),
                    draft: skill.draft,
                    enabled: skill.enabled,
                })
                .collect(),
            skills_tab: state.app_settings_tab == AppSettingsTab::Skills,
            skills_query: state.skills_query.clone(),
            skills_scope: state.skills_scope,
            skills_counts: state.skills_counts,
            skills_loading: state.skills_loading,
            skills_error: state.skills_error.clone(),
            skill_open: state.skill_open_id.clone().map(|id| {
                let detail = state.skill_open.as_ref().filter(|open| open.skill.id == id);
                let row = state.skills.iter().find(|skill| skill.id == id);
                OpenSkillSnap {
                    name: detail
                        .map(|open| open.skill.name.clone())
                        .or_else(|| row.map(|row| row.name.clone()))
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| id.clone()),
                    body: detail.map(|open| open.body.clone()).unwrap_or_default(),
                    arrived: detail.is_some(),
                    // The detail's and not the row's: the pane draws this row only once the
                    // fetch has answered, and a timestamp taken off the list would be a fact
                    // advertised for an element nobody can point at yet.
                    updated_at_ms: detail
                        .map(|open| open.skill.updated_at_ms)
                        .unwrap_or_default(),
                    enabled: detail.map(|open| open.skill.enabled).unwrap_or(true),
                    approved_at_ms: detail.and_then(|open| open.skill.approved_at_ms),
                    // Any switch, as on screen: one at a time, whichever skill.
                    switching: state.skill_enabling.is_some(),
                    error: state.skill_error.clone(),
                    id,
                }
            }),
            skill_add_open: state.skill_add_open,
            skill_add_error: state.skill_add_error.clone(),
            skill_saving: state.skill_saving,
            skill_delete_confirm: state.skill_delete_prompt(),
            taught_skill: state.taught_skill.clone(),
            taught_tape_in_hand: state.taught_tape_is_in_hand(),
            logins_tab: state.app_settings_tab == AppSettingsTab::Logins,
            site_login_notice: state.site_login_notice.clone(),
            site_login_error: state.site_login_error.clone(),
            site_login_query: state.site_login_query.clone(),
            site_login_selected: state.site_login_selected.clone(),
            site_login_add_open: state.site_login_add_open,
            computer_tab: state.app_settings_tab == AppSettingsTab::Computer,
            updates_tab: state.app_settings_tab == AppSettingsTab::Updates,
            route_traffic_on_bot_pane: state.show_route_traffic_on_bot_pane(),
            route_traffic_in_user_settings: state.show_route_traffic_in_user_settings(),
            egress_tunnel_enabled: state.egress_tunnel_enabled,
            egress_tunnel_ready: state
                .coworker_computer
                .as_ref()
                .and_then(|computer| computer.box_egress_ready()),
            // Only where a person could see it: a group's or an org's computer has no surface
            // for the choice yet, so the driver must not be able to set what nobody can.
            egress_policy: (state.show_egress_policy_on_bot_pane()
                || state.show_egress_policy_in_user_settings())
            .then(|| state.egress_policy())
            .flatten(),
            network_policy_open: state.network_policy_open,
            local_rules: state.this_mac_rules().cloned(),
            local_exec_stopped: state
                .local_exec_stopped
                .as_ref()
                .map(crate::opengrok::LocalExecStopped::sentence),
            connections: state.connections.clone(),
            reply_source: ReplySourceSnap::from_state(state),
            general_tab: state.app_settings_tab == AppSettingsTab::General,
            reply_sources: crate::components::chat::reply_badges(state),
            connections_tab: state.app_settings_tab == AppSettingsTab::Connections,
            pending: None,
            compose: None,
        }
    }

    pub fn take_command(&mut self) -> Option<Command> {
        self.pending.take()
    }

    /// The keys the last op asked for, if it asked for any.
    ///
    /// They are not a command because they are not a change to the state: they are keys, and
    /// only the window can press them. [`crate::root::RootView`] takes them from here.
    pub fn take_compose(&mut self) -> Option<ComposePlan> {
        self.compose.take()
    }

    fn tree(&self) -> UiTree {
        if !self.signed_in {
            let mut login = UiNode::page(ids::PAGE_LOGIN, "Sign in to OpenGrok")
                .with_child(UiNode::textbox(ids::LOGIN_EMAIL, "Email"))
                .with_child(UiNode::textbox(ids::LOGIN_PASSWORD, "Password"))
                .with_child(UiNode::button(ids::LOGIN_SUBMIT, "Sign in"));
            if let Some(error) = &self.auth_error {
                login = login.with_child(UiNode::new(ids::LOGIN_ERROR, "status", error.clone()));
            }
            // The sign-in page shows the pill too, and so does the tree: a password typed at a
            // server that is not answering fails for a reason that has nothing to do with it.
            if let Some(node) = self.reconnect_node() {
                login = login.with_child(node);
            }
            return UiTree {
                app: "nativechat".into(),
                platform: PlatformKind::Desktop,
                ready: self.ready,
                nodes: vec![UiNode::window(ids::WINDOW, "NativeChat").with_child(login)],
            };
        }

        if self.sessions.is_empty() {
            let mut empty = UiNode::page("empty-roster", "Create your first Bot")
                .with_child(UiNode::button("create-first-bot", "New Bot"));
            if let Some(error) = &self.auth_error {
                empty =
                    empty.with_child(UiNode::new("empty-roster-error", "status", error.clone()));
            }
            return UiTree {
                app: "nativechat".into(),
                platform: PlatformKind::Desktop,
                ready: self.ready,
                nodes: vec![
                    UiNode::window(ids::WINDOW, "NativeChat")
                        .with_child(if self.sidebar_hidden {
                            UiNode::button(ids::HEADER_LEFT_SIDEBAR, "Show sidebar")
                        } else {
                            self.sidebar_toggle_node()
                        })
                        .with_child(UiNode::button(
                            ids::HEADER_RIGHT_SIDEBAR,
                            "Toggle Bot details",
                        ))
                        .with_child(UiNode::button(ids::NAV_NEW_CHAT, "New Bot"))
                        .with_child(empty),
                ],
            };
        }

        let sessions: Vec<UiNode> = self
            .sessions
            .iter()
            .map(|s| {
                let item_id = if self.signed_in {
                    ids::coworker(&s.id)
                } else {
                    ids::session(&s.id)
                };
                let mut item = UiNode::listitem(item_id, s.title.clone());
                if s.active {
                    item.states.push("selected".into());
                }
                if let Some(preview) = &s.preview {
                    item = item.with_value(preview.clone());
                }
                if s.listed {
                    item.states.push("listed".into());
                }
                item
            })
            .collect();

        let sidebar =
            UiNode::navigation(ids::SIDEBAR, "Sidebar")
                .with_child(self.sidebar_toggle_node())
                .with_child(UiNode::button(ids::NAV_NEW_CHAT, "New Bot"))
                .with_child(UiNode::button(ids::NAV_SEARCH, "Search"))
                .with_child(UiNode::button(ids::NAV_LIBRARY, "Library"))
                .with_child(UiNode::button(ids::NAV_PROJECTS, "Projects"))
                .with_child(UiNode::button(ids::NAV_RECIPES, "Recipes"))
                .with_child(UiNode::scroll(ids::SIDEBAR_LIST, "Chats").with_child(
                    UiNode::list("sidebar-sessions", "Sessions").with_children(sessions),
                ))
                .with_child(UiNode::button(
                    ids::FOOTER_THEME,
                    format!("Theme: {}", self.theme_mode),
                ))
                .with_child(UiNode::button(
                    ids::FOOTER_ACCOUNT,
                    self.account_label.clone(),
                ))
                .with_child(UiNode::button(ids::FOOTER_SIGN_OUT, "Sign Out"));

        let mut page = UiNode::page(ids::PAGE, "Chat")
            .with_child(self.composer_node())
            .with_child(self.composer_send_node())
            .with_child(UiNode::new(
                "transcript-tail",
                "status",
                if self.last_assistant.is_empty() {
                    "(empty)".to_string()
                } else {
                    self.last_assistant.chars().take(400).collect()
                },
            ))
            .with_child(UiNode::scroll(ids::TRANSCRIPT, "Transcript").with_value(
                if self.transcript_following {
                    "following"
                } else {
                    "detached"
                },
            ));
        if self.signed_in {
            if self.sidebar_hidden {
                page = page.with_child(UiNode::button(ids::HEADER_LEFT_SIDEBAR, "Show sidebar"));
            }
            page = page.with_child(UiNode::button(
                ids::HEADER_RIGHT_SIDEBAR,
                "Toggle right sidebar",
            ));
            if self.sessions.iter().any(|session| session.active) {
                page = page.with_child(UiNode::button(ids::HEADER_MONITOR, "Toggle computer pane"));
            }
            if let Some(bot) = self.sessions.iter().find(|session| session.active) {
                let mut chip = UiNode::button(ids::HEADER_COWORKER, bot.title.clone());
                if self.bot_chip_goes_home {
                    chip.states.push("goes-home".into());
                }
                page = page.with_child(chip);
            }
        }
        if let Some(status) = &self.bot_status {
            page = page.with_child(UiNode::new("bot-status", "status", status.clone()));
        }
        for (id, filename, message) in &self.sent_files {
            page = page.with_child(
                UiNode::status(format!("message-file-{id}"), filename.clone())
                    .with_value(message.clone()),
            );
        }
        for node in self.reply_run_nodes() {
            page = page.with_child(node);
        }
        // Each reply's badge, as the feed draws it: label as it reads, value the door's wire
        // word, state `via-mac` on a reply the person's Mac answered, and under it the model the
        // server named, which the badge shows on hover: words the window draws, so a node of
        // their own rather than a state.
        for (message_id, source) in &self.reply_sources {
            let mut node = UiNode::status(
                ids::reply_badge(message_id),
                reply_source::badge_words(source),
            )
            .with_value(source.kind.word());
            if source.via == Some(crate::opengrok::Via::Mac) {
                node.states.push(VIA_MAC.into());
            }
            // The server's keys answered a Bot on the person's plan because the relay is off.
            if source.relay_off() {
                node.states.push("relay-off".into());
            }
            if let Some(model) = &source.model {
                node = node.with_child(UiNode::status(
                    ids::reply_badge_model(message_id),
                    model.clone(),
                ));
            }
            page = page.with_child(node);
        }
        if let Some((name, word)) = &self.routine_thread {
            page = page
                .with_child(
                    UiNode::status(ids::CHAT_ROUTINE_INSTRUCTIONS, "Instructions")
                        .with_value(self.routine_instructions.to_string()),
                )
                .with_child(
                    UiNode::status(ids::CHAT_ROUTINE_THREAD, name.clone()).with_value(word.clone()),
                );
        }
        if self.queued_sends > 0 {
            // Each held message the server holds for the person's Mac says so under it, as its
            // bubble does.
            let queued = self.waiting_for_mac.iter().fold(
                UiNode::new(
                    ids::COMPOSER_QUEUED,
                    "status",
                    format!("{} queued", self.queued_sends),
                ),
                |queued, message_id| {
                    queued.with_child(UiNode::status(
                        ids::queued_waiting(message_id),
                        crate::components::message::WAITING_FOR_YOUR_MAC,
                    ))
                },
            );
            page = page.with_child(queued);
        }
        // What typing produces: the list `/` or `@` opened, the recipe that picking one put on
        // the draft, and the pictures a turn came back with. Each is in the tree only while it
        // is on screen, so `assert --exists false` is the way to say a panel is shut.
        if let Some(panel) = self.composer_panel_node() {
            page = page.with_child(panel);
        }
        if let Some(bar) = self.recipe_bar_node() {
            page = page.with_child(bar);
        }
        if let Some(skill) = self.composer_skill_node() {
            page = page.with_child(skill);
        }
        for (index, caption) in self.thumbs.iter().enumerate() {
            page = page.with_child(UiNode::button(ids::image_thumb(index), caption.clone()));
        }
        if let Some(lightbox) = self.lightbox_node() {
            page = page.with_child(lightbox);
        }
        if let Some((title, what)) = &self.routine_delete_question {
            page = page.with_child(
                UiNode::dialog(ids::ROUTINE_DELETE_PROMPT, title.clone())
                    .with_value(what.clone())
                    .with_child(UiNode::button(ids::ROUTINE_DELETE_CANCEL, "Cancel"))
                    .with_child(UiNode::button(ids::ROUTINE_DELETE_CONFIRM, "Delete")),
            );
        }
        for approval in &self.approvals {
            let id = format!("approval-{}", approval.call_id);
            let mut card = UiNode::new(id.clone(), "dialog", approval.title.clone())
                .with_child(UiNode::button(format!("{id}-allow-once"), "Allow once"))
                .with_child(UiNode::button(
                    format!("{id}-deny-once"),
                    if approval.review { "Deny" } else { "Deny once" },
                ));
            // Bare facts as states, so an assert does not have to match a sentence: the reason,
            // the thread, and the tool the call would use.
            for state in [&approval.reason, &approval.thread_id, &approval.tool] {
                if !state.is_empty() {
                    card.states.push(state.clone());
                }
            }
            if approval.tunnel {
                card.states.push("egress-tunnel".to_string());
            }
            if approval.local || approval.review {
                card = card.with_child(UiNode::button(format!("{id}-always"), "Always allow"));
            }
            if (approval.local && !approval.review) || (approval.review && approval.tunnel) {
                card = card.with_child(UiNode::button(format!("{id}-never"), "Never"));
            }
            page = page.with_child(card);
        }
        for choice in &self.choices {
            page = page.with_child(choice_node(choice));
        }
        for form in &self.user_forms {
            page = page.with_child(user_form_node(form));
        }
        for handoff in &self.computer_handoffs {
            page = page.with_child(computer_handoff_node(handoff));
        }
        for offer in &self.save_logins {
            page = page.with_child(save_login_node(offer));
        }
        let mut status = UiNode::new("computer-status", "status", self.computer_status.clone());
        if let Some((line, code)) = &self.computer_error {
            status = status.with_child(
                UiNode::status(ids::COMPUTER_ERROR, line.clone()).with_value(code.clone()),
            );
        }
        let mut computer = UiNode::new("computer-pane", "dialog", "Computer")
            .with_visible(self.computer_open)
            .with_child(status);
        if self.computer_error.is_some() {
            computer = computer.with_child(
                UiNode::button(
                    ids::COMPUTER_GET,
                    crate::components::computer::GET_A_COMPUTER,
                )
                .with_enabled(!self.computer_asking),
            );
        }
        computer = computer
            .with_child(UiNode::button(
                "computer-update",
                self.computer_update_label.clone(),
            ))
            .with_child(UiNode::button(
                "computer-reset",
                self.computer_reset_label.clone(),
            ));
        if self.computer_open && self.route_traffic_on_bot_pane {
            computer = computer.with_child(UiNode::new(
                "route-traffic-this-computer",
                "switch",
                "Route traffic through this computer",
            ));
            if let Some(current) = self.egress_policy {
                computer = computer.with_child(UiNode::button(
                    "network-policy",
                    format!("Use your network: {}", current.label()),
                ));
            }
        }
        if self.network_policy_open
            && let Some(current) = self.egress_policy
        {
            computer = computer.with_child(
                egress_policy_node(current)
                    .with_child(UiNode::button("network-policy-close", "Close")),
            );
        }
        computer = computer.with_child(UiNode::button(ids::ROUTINE_NEW, "Create routine"));
        if let Some(line) = &self.routine_error {
            computer = computer.with_child(UiNode::status(ids::ROUTINE_ERROR, line.clone()));
        }
        if let Some(open) = self.open_routine() {
            for node in routine_header_nodes(open, self.routine_history_open) {
                computer = computer.with_child(node);
            }
            for node in routine_wake_nodes(open, self.routine_wake_editor.as_ref()) {
                computer = computer.with_child(node);
            }
        }
        for routine in &self.routines {
            computer = computer.with_child(routine_node(routine));
        }
        if let Some(handoff) = self
            .computer_handoffs
            .iter()
            .rev()
            .find(|handoff| handoff.status.is_live())
        {
            computer = computer.with_child(
                UiNode::new(computer_attention_id(), "dialog", "Needs your attention")
                    .with_child(UiNode::button(
                        computer_attention_skip_id(&handoff.card_key),
                        "Skip this step",
                    ))
                    .with_child(UiNode::button(
                        computer_attention_done_id(&handoff.card_key),
                        "I'm done, continue",
                    )),
            );
            page = page.with_child(
                UiNode::new(
                    computer_window_attention_id(),
                    "dialog",
                    "Needs your attention",
                )
                .with_child(UiNode::button(
                    computer_window_attention_skip_id(&handoff.card_key),
                    "Skip this step",
                ))
                .with_child(UiNode::button(
                    computer_window_attention_done_id(&handoff.card_key),
                    "I'm done, continue",
                )),
            );
        }
        page = page.with_child(computer);
        for node in self.taught_skill_nodes() {
            page = page.with_child(node);
        }
        if let Some(ready) = self.egress_tunnel_ready {
            page = page.with_child(UiNode::status(
                "egress_tunnel.ready",
                if ready { "ready" } else { "not-ready" },
            ));
        }
        if let Some(policy) = self.egress_policy {
            page = page.with_child(UiNode::status("egress_policy", policy.as_stored()));
        }
        if let Some(banner) = &self.update_banner {
            page = page.with_child(UiNode::new("update-banner", "status", banner.clone()));
        }
        if let Some(node) = self.reconnect_node() {
            page = page.with_child(node);
        }
        for node in self.signed_out_nodes() {
            page = page.with_child(node);
        }
        if self.can_retry_turn {
            page = page.with_child(UiNode::button("retry-turn", "Try again"));
        }
        if self.can_send_on_server {
            page = page.with_child(UiNode::button(
                ids::RUN_ERROR_SEND_ON_SERVER,
                crate::components::chat::SEND_ON_SERVER_LABEL,
            ));
        }
        if let Some(question) = &self.computer_confirm {
            page = page.with_child(
                UiNode::new("computer-confirm", "dialog", question.clone())
                    .with_child(UiNode::button("computer-confirm-yes", "Confirm"))
                    .with_child(UiNode::button("computer-confirm-cancel", "Cancel")),
            );
        }

        let mut settings = UiNode::dialog(ids::AGENT_SETTINGS, "Agent Settings")
            .with_visible(self.agent_settings_open)
            .with_child(UiNode::button("avatar-trigger", "Edit avatar"))
            .with_child(
                UiNode::new("avatar-editor", "dialog", "Avatar editor")
                    .with_visible(self.avatar_editor_open),
            );
        // The Model card, the one place a Bot's model is picked: see `picker_node`.
        if let Some(card) = self.picker_node(PickerFor::Bot) {
            settings = settings.with_child(card);
        }
        if self.agent_tools.is_some() || self.agent_ceiling.is_some() {
            settings = settings.with_child(agent_tools_node(&ToolsCardSnap {
                tools: self.agent_tools.as_ref(),
                ceiling: self.agent_ceiling.as_ref(),
                open: self.agent_tools_open,
            }));
        }
        if let Some(card) = &self.agent_skills {
            settings = settings.with_child(agent_skills_node(card, self.agent_skills_open));
        }
        if let Some(usage) = &self.agent_usage {
            // `agent-usage` (value = the card's second line), with `agent-usage-toggle` while
            // there are models to show and one `agent-usage-model-{i}` per model the server
            // reported, label = that model's line, visible while the card is open (#138).
            let mut node = UiNode::new("agent-usage", "status", "Usage")
                .with_value(crate::components::agent_settings::usage_summary(usage));
            if let crate::state::UsageReport::Read(read) = usage
                && !read.models.is_empty()
            {
                node = node.with_child(UiNode::button(
                    "agent-usage-toggle",
                    if self.agent_usage_open {
                        "Hide"
                    } else {
                        "Show"
                    },
                ));
                for (i, model) in read.models.iter().enumerate() {
                    node = node.with_child(
                        UiNode::listitem(
                            format!("agent-usage-model-{i}"),
                            crate::components::agent_settings::model_line(model),
                        )
                        .with_visible(self.agent_usage_open),
                    );
                }
            }
            settings = settings.with_child(node);
        }
        if self.agent_settings_open
            && let Some(bot) = self.sessions.iter().find(|session| session.active)
        {
            settings = settings.with_child(self.agent_connections_node(&bot.id));
        }
        // While the Bot's replies go through the person's own plan: in the Usage card, which
        // does not count them.
        if self.replies_on_plan {
            settings = settings.with_child(UiNode::status(
                ids::AGENT_USAGE_PLAN,
                reply_source::PLAN_USAGE_NOTE,
            ));
        }
        // The pane's red line over Save, where a refused Save says why in the server's words.
        if let Some(error) = &self.auth_error {
            settings = settings.with_child(UiNode::status("agent-settings-error", error.clone()));
        }
        settings = settings.with_child(UiNode::button(ids::AGENT_SAVE, "Save"));

        UiTree {
            app: "nativechat".into(),
            platform: PlatformKind::Desktop,
            ready: self.ready,
            nodes: vec![
                UiNode::window(ids::WINDOW, "NativeChat")
                    .with_children(
                        (!self.sidebar_hidden)
                            .then_some(sidebar)
                            .into_iter()
                            .collect(),
                    )
                    .with_child(page)
                    .with_child(self.recipes_node())
                    .with_child({
                        let mut settings = UiNode::dialog(ids::DIALOG_ACCOUNT, "Settings")
                            .with_visible(self.account_open)
                            .with_child(UiNode::button("app-settings-back", "← Back to app"))
                            .with_child(UiNode::button(ids::SETTINGS_GENERAL, "General"))
                            .with_child(UiNode::button("settings-tab-computer", "Computer"))
                            .with_child(UiNode::button("settings-tab-updates", "Updates"))
                            .with_child(UiNode::button("settings-tab-logins", "Logins"))
                            .with_child(UiNode::button(ids::SETTINGS_CONNECTIONS, "Connections"))
                            .with_child(UiNode::button(ids::SETTINGS_SKILLS, "Skills"));
                        if self.logins_tab {
                            settings = self.logins_nodes(settings);
                        }
                        // Only while the dialog is open on Connections, as with this Mac's rules
                        // below: a closed dialog's children are still found by id.
                        if self.account_open && self.connections_tab {
                            for node in self.connections_nodes() {
                                settings = settings.with_child(node);
                            }
                        }
                        // The same for General's Default models.
                        if self.account_open && self.general_tab {
                            settings = settings.with_child(self.default_models_node());
                        }
                        if self.skills_tab {
                            settings = self.skills_nodes(settings);
                        }
                        if self.updates_tab {
                            settings = settings.with_child(UiNode::button(
                                "settings-computer-update",
                                self.computer_update_label.clone(),
                            ));
                        }
                        // Why this Mac stopped running commands, over its computers as on the
                        // page, then each computer's card, its relay switch and where its relay
                        // stands, and its local-exec mode, while Settings is open on Computer:
                        // the choice a check of this Mac's Ask / Always / Never sets.
                        if self.account_open && self.computer_tab {
                            if let Some(why) = &self.local_exec_stopped {
                                settings = settings.with_child(UiNode::status(
                                    ids::LOCAL_EXEC_STOPPED,
                                    why.clone(),
                                ));
                            }
                            for card in &self.computers {
                                settings = settings.with_child(self.computer_card_node(card));
                                let (machine, mode) = (&card.machine_id, card.mode);
                                let mut menu = UiNode::new(
                                    ids::computer_exec(machine),
                                    "menu",
                                    card.label.clone(),
                                )
                                .with_value(mode.as_stored());
                                if card.this_computer {
                                    menu.states.push("this-mac".into());
                                }
                                for choice in [
                                    crate::opengrok::LocalExecMode::Always,
                                    crate::opengrok::LocalExecMode::Ask,
                                    crate::opengrok::LocalExecMode::Never,
                                ] {
                                    let mut button = UiNode::button(
                                        ids::computer_exec_mode(machine, choice),
                                        choice.label(),
                                    );
                                    if choice == mode {
                                        button.states.push("selected".into());
                                    }
                                    menu = menu.with_child(button);
                                }
                                settings = settings.with_child(menu);
                            }
                        }
                        if self.computer_tab && self.route_traffic_in_user_settings {
                            settings = settings.with_child(UiNode::new(
                                "route-traffic-this-computer",
                                "switch",
                                "Route traffic through this computer",
                            ));
                            if let Some(current) = self.egress_policy {
                                settings = settings.with_child(egress_policy_node(current));
                            }
                        }
                        // Only while the dialog is open on Computer: the tab stays selected after
                        // Settings closes, and a closed dialog's children are still found by id, so
                        // a snapshot would otherwise keep listing the commands after the person left.
                        if self.account_open
                            && self.computer_tab
                            && let Some(rules) = &self.local_rules
                        {
                            for node in local_rules_nodes(rules) {
                                settings = settings.with_child(node);
                            }
                        }
                        settings
                    })
                    .with_child(
                        UiNode::dialog(ids::DIALOG_VOICE, "Voice Mode")
                            .with_visible(self.voice_open),
                    )
                    .with_child(settings)
                    .with_child(UiNode::button(
                        "agent-model-dismiss",
                        "Dismiss model picker",
                    ))
                    .with_child(UiNode::button(
                        ids::NEW_BOTS_DISMISS,
                        "Dismiss the default for new Bots' picker",
                    ))
                    .with_child(UiNode::button(
                        ids::PLAN_FALLBACK_DISMISS,
                        "Dismiss the Relay-off fallback's picker",
                    ))
                    .with_child(UiNode::button(
                        "avatar-editor-dismiss",
                        "Dismiss avatar editor",
                    )),
            ],
        }
    }

    /// The newest reply's steps, each with its row's words and how it came out, and a count of
    /// its Thought rows. Each is in the tree only while the reply has some, so `assert --exists
    /// false` is "this reply did nothing but talk". An open row has state `expanded`; for the
    /// Thought rows that is all of them open, which is what a click on `reply-reasoning` asks for.
    /// A step whose row ends with how long the call took has that as a state, `took-1s`; its
    /// value stays how it came out. The Timing row under the reply is `reply-timing`, with its
    /// lines under it while it is open.
    fn reply_run_nodes(&self) -> Vec<UiNode> {
        let run = &self.reply_run;
        let mut nodes = Vec::new();
        if !run.steps.is_empty() {
            nodes.push(
                UiNode::list(ids::REPLY_STEPS, "Steps")
                    .with_value(run.steps.len().to_string())
                    .with_children(
                        run.steps
                            .iter()
                            .map(|step| {
                                let mut node =
                                    UiNode::status(ids::step(&step.call_id), step.label.clone())
                                        .with_value(step.status.word());
                                if step.open {
                                    node.states.push("expanded".into());
                                }
                                if let Some(took) = &step.took {
                                    node.states.push(format!("took-{took}"));
                                }
                                node
                            })
                            .collect(),
                    ),
            );
        }
        if !run.thoughts.is_empty() {
            let mut node = UiNode::status(ids::REPLY_REASONING, "Thought")
                .with_value(run.thoughts.len().to_string());
            if run.thoughts.iter().all(|(_, open, _)| *open) {
                node.states.push("expanded".into());
            }
            node = node.with_children(
                run.thoughts
                    .iter()
                    .enumerate()
                    .map(|(n, (_, open, took))| {
                        let mut thought = UiNode::status(format!("reply-thought-{n}"), "Thinking");
                        if *open {
                            thought.states.push("expanded".into());
                        }
                        if let Some(took) = took {
                            thought.states.push(format!("took-{took}"));
                        }
                        thought
                    })
                    .collect(),
            );
            nodes.push(node);
        }
        if let Some(crate::components::steps::RunRow::Timing { total, .. }) = &run.timing {
            nodes.push(UiNode::status(ids::REPLY_TIMING, "Timing").with_value(total.clone()));
        }
        nodes
    }

    /// A click on one of the newest reply's steps opens it, or shuts it if it is open. What
    /// holds it opens with it: an open step keeps its "N steps" line open (see
    /// `components::steps`). A click on `reply-reasoning` opens every Thought row, or shuts
    /// them all once they all are. The `reply-timing` footer is plain text and refuses clicks.
    fn reply_run_command(&self, target: &str) -> Option<Result<Command, String>> {
        if target == ids::REPLY_TIMING {
            return Some(match &self.reply_run.timing {
                Some(_) => {
                    Err("the newest reply's Timing row is its total and does not open".to_string())
                }
                None => Err("the newest reply has no Timing row".to_string()),
            });
        }
        if target == ids::REPLY_REASONING {
            let thoughts = &self.reply_run.thoughts;
            if thoughts.is_empty() {
                return Some(Err("the newest reply has no Thought row".to_string()));
            }
            return Some(Ok(Command::SetStepsOpen {
                keys: thoughts.iter().map(|(key, _, _)| key.clone()).collect(),
                open: !thoughts.iter().all(|(_, open, _)| *open),
            }));
        }
        let call_id = target.strip_prefix("step-")?;
        Some(
            self.reply_run
                .steps
                .iter()
                .find(|step| step.call_id == call_id)
                .map(|step| Command::SetStepsOpen {
                    keys: vec![step.key.clone()],
                    open: !step.open,
                })
                .ok_or_else(|| format!("no step `{target}` in the newest reply")),
        )
    }

    /// The reconnecting pill, while something cannot be reached.
    ///
    /// In the tree only while it is on screen, so `assert --exists false` is how a driver says
    /// the app has reconnected — which is the half of this the red transcript line it replaced
    /// could never be checked for, because that line never went away.
    fn reconnect_node(&self) -> Option<UiNode> {
        let (banner, machine) = self.reconnect.as_ref()?;
        let mut node = UiNode::status("reconnect-banner", banner.clone());
        // The machine as a state as well as in the copy, for an assert that would rather not
        // match on a sentence.
        node.states.push((*machine).to_string());
        Some(node)
    }

    /// The signed-out banner and its button, while the server does not know who the app is.
    ///
    /// Both go in and out together, so `assert --exists false` on either says the app has a
    /// session again — and the button is in the tree because it is the whole of the recovery:
    /// a driver, like a person, has to be able to get out of this state without a relaunch.
    fn signed_out_nodes(&self) -> Vec<UiNode> {
        let Some(banner) = &self.signed_out else {
            return Vec::new();
        };
        let mut node = UiNode::status("signed-out-banner", banner.clone());
        // The bare fact as a state, so an assert does not have to match the copy — and so that
        // it is plainly not the reconnect pill, which is the confusion that made the bug.
        node.states.push("signed-out".to_string());
        vec![node, UiNode::button("signed-out-sign-in", "Sign in again")]
    }

    /// A task taught on a coworker's screen while it is being written up as a skill, and what
    /// came of it. Nothing at all until somebody teaches one.
    ///
    /// The sheet is in that screen's own window, which draws no tree: without these a driver
    /// watching a model write a lesson sees an app doing nothing for a minute, and then an app
    /// that has done nothing. Three states and they are three nodes, because they are three
    /// different things to wait for.
    fn taught_skill_nodes(&self) -> Vec<UiNode> {
        match &self.taught_skill {
            None => Vec::new(),
            Some(TaughtSkill::Writing) => {
                vec![UiNode::status(ids::TEACH_SKILL_WRITING, WRITING_A_LESSON)]
            }
            Some(TaughtSkill::Written { id, name, enabled }) => {
                // Named by what has to be typed after a slash, carrying the id to open it by,
                // and switched off as a STATE — off is the fact, and a driver checking that a
                // lesson has to be read before anything uses it should not be matching copy.
                let mut node =
                    UiNode::button(ids::TEACH_SAVED_SKILL, name.clone()).with_value(id.clone());
                if !enabled {
                    node.states.push("off".to_string());
                }
                vec![node]
            }
            Some(TaughtSkill::Refused { why, next, .. }) => {
                // The server's sentence, which names which of the things went wrong, and then
                // whichever of the two controls the sheet is drawing beside it. A driver
                // offered a retry for a spend cap would sit in a loop the sentence above it
                // has already said will never end; one offered a retry for an answer that
                // never arrived would write a second skill from one recording.
                let mut nodes = vec![UiNode::status(ids::TEACH_SKILL_ERROR, why.clone())];
                match next {
                    AfterRefusal::SendAgain if self.taught_tape_in_hand => {
                        nodes.push(UiNode::button(ids::TEACH_RETRY, "Try again"));
                    }
                    AfterRefusal::LookFirst => {
                        nodes.push(UiNode::button(ids::TEACH_CHECK_LIBRARY, "Check Skills"));
                    }
                    AfterRefusal::SendAgain | AfterRefusal::Nothing => {}
                }
                nodes
            }
        }
    }

    /// The composer's one action button, in whichever of its two states it is in.
    ///
    /// Always in the tree, because it is always on screen: a driver reads what it says now
    /// rather than testing whether it is there at all. The label is the button's own word and
    /// the `running` state is the same fact for an assert that would rather not match on copy.
    fn composer_send_node(&self) -> UiNode {
        let mut node = UiNode::button(
            ids::COMPOSER_SEND,
            if self.turn_in_flight {
                "Stop"
            } else {
                "Send message"
            },
        );
        if self.turn_in_flight {
            node.states.push("running".into());
        }
        node
    }

    /// The composer's panel, while one is open: the field the caret is in, and every row by the
    /// id the panel gives it on screen.
    ///
    /// A row is not clicked. The panel is a keyboard list — typing filters it, the arrows move
    /// the highlight and Enter takes the row — and that is the path `op type` and `op key` now
    /// take. The rows are here to be read and asserted on, not as click targets the host would
    /// have to serve by another route than the person's.
    fn composer_panel_node(&self) -> Option<UiNode> {
        let mode = self.composer_panel?;
        let mut panel = UiNode::list(
            ids::COMPOSER_PANEL,
            panel_name(mode, self.recipe_bar.as_ref()),
        )
        .with_child(
            UiNode::textbox(ids::COMPOSER_PANEL_SEARCH, "Search")
                // The panel takes the caret as it opens, which is why typing with no target
                // goes here and not into the message.
                .with_focused(true),
        );
        for row in &self.panel_rows {
            let mut item = UiNode::listitem(row.id.clone(), row.title.clone());
            if let Some(label) = &row.label {
                item = item.with_value(label.clone());
            }
            if row.note {
                item.states.push("note".into());
            }
            panel = panel.with_child(item);
        }
        Some(panel)
    }

    /// The bar above the field: what the next message runs, and what it has been told.
    ///
    /// The line is the bar's own, word for word, so a driver that has just picked a row from `/`
    /// can assert which of the two it got — "Workflow · Search and retry" is the only place the
    /// app says that out loud.
    fn recipe_bar_node(&self) -> Option<UiNode> {
        let bar = self.recipe_bar.as_ref()?;
        let mut node = UiNode::new(
            ids::COMPOSER_RECIPE_BAR,
            role::STATUS,
            format!("{} · {}", bar.kind.label(), bar.name),
        );
        for parameter in &bar.parameters {
            let mut chip = UiNode::note(ids::recipe_param(&parameter.name), parameter.name.clone());
            match &parameter.value {
                Some(value) => {
                    chip = chip.with_value(value.clone());
                    chip.states.push("filled".into());
                }
                None if parameter.required => chip.states.push("needed".into()),
                None => {}
            }
            node = node.with_child(chip);
        }
        Some(node)
    }

    /// The skill the next message is sent with, while one is on the draft.
    ///
    /// There is no bar for a skill: the chip in the message is the whole of what is on screen,
    /// so the node is named after what the chip reads, and the id the turn will carry rides in
    /// `value` — which is the field a driver can assert on, and the only one of the two that
    /// tells two skills of one name apart.
    fn composer_skill_node(&self) -> Option<UiNode> {
        let skill = self.composer_skill.as_ref()?;
        Some(UiNode::note(ids::COMPOSER_SKILL, skill.name.clone()).with_value(skill.id.clone()))
    }

    /// The picture overlay, while it is open, with the same line the overlay itself prints.
    fn lightbox_node(&self) -> Option<UiNode> {
        let open = self.lightbox.as_ref()?;
        Some(UiNode::dialog(
            ids::LIGHTBOX,
            crate::components::lightbox::caption_line(&open.caption, open.index, open.total),
        ))
    }

    /// The Recipes page as the driver sees it: the filter chips, the rows with their Accept
    /// and Decline, and the recipe open in it.
    fn recipes_node(&self) -> UiNode {
        let mut page = UiNode::page(ids::PAGE_RECIPES, "Recipes").with_visible(self.recipes_open);
        for filter in crate::state::RecipeFilter::ALL {
            let mut chip = UiNode::button(filter.element_id(), filter.label());
            if filter.query() == self.recipes_filter {
                chip.states.push("selected".into());
            }
            page = page.with_child(chip);
        }
        let pending = self.recipes.iter().filter(|recipe| recipe.pending).count();
        let mut list = UiNode::list("recipes-list", "Recipes");
        for recipe in &self.recipes {
            let mut item = UiNode::listitem(format!("recipe-{}", recipe.id), recipe.name.clone());
            if recipe.pending {
                item.states.push("pending".into());
                // The one pending share answers to the plain ids; several need the suffix.
                let suffix = if pending == 1 {
                    String::new()
                } else {
                    format!("-{}", recipe.id)
                };
                item = item
                    .with_child(UiNode::button(format!("recipe-accept{suffix}"), "Accept"))
                    .with_child(UiNode::button(format!("recipe-decline{suffix}"), "Decline"));
            }
            list = list.with_child(item);
        }
        page = page.with_child(list);
        if let Some(id) = &self.recipe_open {
            let mut detail = UiNode::new("recipe-detail", "dialog", format!("Recipe {id}"))
                .with_child(UiNode::button("recipe-back", "Back"));
            if let Some(open) = &self.recipe_detail {
                let mut run = UiNode::button(
                    ids::RECIPE_RUN,
                    if open.running {
                        crate::state::RECIPE_RUNNING
                    } else {
                        "Run"
                    },
                )
                .with_enabled(open.runnable);
                if let Some(bot) = &open.run_bot {
                    run = run.with_value(bot.clone());
                }
                detail = detail.with_child(run);
                let outcome = if open.running {
                    Some("running")
                } else {
                    open.result
                };
                if let Some(outcome) = outcome {
                    detail = detail.with_child(
                        UiNode::status(ids::RECIPE_RUN_RESULT, "Run").with_value(outcome),
                    );
                }
                if let Some(error) = &open.error {
                    detail = detail.with_child(UiNode::status(ids::RECIPE_ERROR, error.clone()));
                }
                let mut history = UiNode::list(ids::RECIPE_HISTORY_RUNS, "Runs")
                    .with_value(open.runs.len().to_string());
                for (run_id, state, ok) in &open.runs {
                    let mut line = UiNode::status(ids::recipe_history_run(run_id), run_id.clone())
                        .with_value(state.clone());
                    if *ok {
                        line.states.push("ok".into());
                    }
                    history = history.with_child(line);
                }
                detail = detail.with_child(history);
            }
            page = page.with_child(detail);
        }
        page
    }

    fn choice_command(&self, target: &str) -> Option<Command> {
        // Every id is compared whole. A message id may carry dashes, so matching the card by
        // its prefix could take `choice-m` for the card of `choice-m-1-0-0` and lose the click.
        for choice in self.choices.iter().filter(|choice| choice.state == "open") {
            let message_id = choice.message_id.clone();
            if target == choice_dismiss_id(&message_id) {
                return Some(Command::ChoiceDismiss { message_id });
            }
            if !choice.one_question && target == choice_submit_id(&message_id) {
                return Some(Command::ChoiceSubmit { message_id });
            }
            for (field_index, field) in choice.fields.iter().enumerate() {
                for option_index in 0..field.options.len() {
                    if target == choice_option_id(&message_id, field_index, option_index) {
                        return Some(Command::Choose {
                            message_id,
                            field_index,
                            option_index,
                        });
                    }
                }
            }
        }
        None
    }

    /// `composer`, with a `composer-chip-{i}` per chip in the draft (label = what it reads as,
    /// value = its kind: `tool` / `recipe` / `workflow` / `skill`), so a driver sees a chip as
    /// the one object it is.
    fn composer_node(&self) -> UiNode {
        let mut node = UiNode::textbox(ids::COMPOSER, "Type a message...");
        for (i, (kind, label)) in self.composer_chips.iter().enumerate() {
            let kind = match kind {
                crate::components::chat_input::TokenKind::Tool => "tool",
                crate::components::chat_input::TokenKind::Recipe => "recipe",
                crate::components::chat_input::TokenKind::Workflow => "workflow",
                crate::components::chat_input::TokenKind::Skill => "skill",
            };
            node = node.with_child(
                UiNode::status(format!("composer-chip-{i}"), label.clone()).with_value(kind),
            );
        }
        for (i, (name, state)) in self.composer_files.iter().enumerate() {
            node = node.with_child(
                UiNode::status(format!("composer-file-{i}"), name.clone()).with_value(*state),
            );
        }
        node
    }

    fn user_form_command(&self, target: &str) -> Option<Command> {
        for form in &self.user_forms {
            let key = &form.card_key;
            if target == user_form_continue_id(key) {
                return Some(Command::UserFormContinue {
                    card_key: key.clone(),
                });
            }
            for (login_id, _, _) in &form.saved_logins {
                if target == user_form_use_saved_id(key, login_id) {
                    return Some(Command::UserFormUseSaved {
                        card_key: key.clone(),
                        login_id: login_id.clone(),
                    });
                }
            }
            if form.saved_login_held && target == user_form_saved_clear_id(key) {
                return Some(Command::UserFormClearSaved {
                    card_key: key.clone(),
                });
            }
            if form.passkey_register && target == format!("user-form-passkey-register-{key}") {
                return Some(Command::UserFormRegisterPasskey {
                    card_key: key.clone(),
                });
            }
            if target == user_form_dismiss_id(key) {
                return Some(Command::UserFormDismiss {
                    card_key: key.clone(),
                });
            }
            if target == user_form_screen_id(key) {
                return Some(Command::UserFormOpenScreen {
                    card_key: key.clone(),
                });
            }
        }
        None
    }

    fn computer_handoff_command(&self, target: &str) -> Option<Command> {
        for handoff in &self.computer_handoffs {
            let key = &handoff.card_key;
            if target == computer_handoff_takeover_id(key) {
                return Some(Command::ComputerHandoffTakeOver {
                    card_key: key.clone(),
                });
            }
            if target == computer_handoff_done_id(key)
                || target == computer_attention_done_id(key)
                || target == computer_window_attention_done_id(key)
            {
                return Some(Command::ComputerHandoffDone {
                    card_key: key.clone(),
                });
            }
            if target == computer_handoff_skip_id(key)
                || target == computer_attention_skip_id(key)
                || target == computer_window_attention_skip_id(key)
            {
                return Some(Command::ComputerHandoffSkip {
                    card_key: key.clone(),
                });
            }
        }
        None
    }

    fn save_login_command(&self, target: &str) -> Option<Command> {
        for offer in &self.save_logins {
            if target == save_login_save_id(&offer.form_entry_id) {
                return Some(Command::SaveLogin {
                    form_entry_id: offer.form_entry_id.clone(),
                });
            }
            if target == save_login_skip_id(&offer.form_entry_id) {
                return Some(Command::SkipSaveLogin {
                    form_entry_id: offer.form_entry_id.clone(),
                });
            }
        }
        None
    }

    /// A Bot's name by its id, from the roster (`sessions` is the roster while signed in).
    fn bot_name(&self, coworker_id: &str) -> Option<String> {
        self.sessions
            .iter()
            .find(|session| session.id == coworker_id)
            .map(|session| session.title.clone())
    }

    /// Settings → Connections as the page draws it (#2): Refresh; each of the person's own
    /// connections, named by its label, valued by the line under it (the service and who it is
    /// lent to), with its Disconnect and why its last Disconnect did not go through; and a
    /// Connect for each service on offer that is not connected. A list still being asked for has
    /// no node at all, so a driver waits for `settings-connections` or one of the lines that
    /// stand in for it.
    fn connections_nodes(&self) -> Vec<UiNode> {
        let connections = &self.connections;
        let own = connections.own_rows();
        let mut nodes = vec![UiNode::button(ids::CONNECTIONS_REFRESH, "Refresh")];
        match &connections.list {
            None | Some(ConnectionList::Loading) => {}
            Some(ConnectionList::Unavailable(why)) => {
                nodes.push(UiNode::status(ids::CONNECTIONS_ERROR, why.clone()));
            }
            Some(ConnectionList::Listed(_)) if own.is_empty() => {
                nodes.push(UiNode::status(
                    ids::CONNECTIONS_EMPTY,
                    connections::NOTHING_CONNECTED,
                ));
            }
            Some(ConnectionList::Listed(_)) => {
                let mut list =
                    UiNode::list(ids::CONNECTIONS, "Connected").with_value(own.len().to_string());
                for row in own {
                    let changing = connections.is_changing(&row.id);
                    let mut node = UiNode::listitem(ids::connection(&row.id), row.label.clone())
                        .with_value(connections::row_detail(connections, row, |id| {
                            self.bot_name(id)
                        }));
                    if changing {
                        node.states.push("changing".into());
                    }
                    if connections.is_confirming_disconnect(&row.id) {
                        // Asked first: the question, and its Yes and No, in place of Disconnect.
                        node = node
                            .with_child(UiNode::status(
                                connections::confirm_question_id(&row.id),
                                connections::confirm_question(connections, row, |id| {
                                    self.bot_name(id)
                                }),
                            ))
                            .with_child(UiNode::button(
                                connections::confirm_yes_id(&row.id),
                                "Yes, disconnect",
                            ))
                            .with_child(UiNode::button(connections::confirm_no_id(&row.id), "No"));
                    } else {
                        node = node.with_child(
                            UiNode::button(
                                ids::connection_disconnect(&row.id),
                                connections::disconnect_label(connections, &row.id),
                            )
                            .with_enabled(!changing),
                        );
                    }
                    if let Some(why) = connections.disconnect_refusal(&row.id) {
                        node = node.with_child(UiNode::status(ids::connection_error(&row.id), why));
                    }
                    list = list.with_child(node);
                }
                nodes.push(list);
            }
        }
        match connections::connect_offer(connections) {
            ConnectOffer::Asking | ConnectOffer::Unknown => {}
            ConnectOffer::Unavailable(why) => {
                nodes.push(UiNode::status(ids::CONNECTORS_ERROR, why));
            }
            ConnectOffer::Nothing(line) => nodes.push(UiNode::status(ids::CONNECTORS_EMPTY, line)),
            ConnectOffer::Offered(open) => {
                let mut list = UiNode::list(ids::CONNECTORS, "Connect a service")
                    .with_value(open.len().to_string());
                for connector in open {
                    let name = connector.name.as_str();
                    let mut button = UiNode::button(
                        ids::connect(name),
                        connections::connect_label(connections, connector),
                    )
                    .with_enabled(connections.opening.is_none());
                    if connections.opening.as_deref() == Some(name) {
                        button.states.push("opening".into());
                    }
                    if connections.is_waiting(name, std::time::Instant::now()) {
                        button.states.push("waiting".into());
                    }
                    list = list.with_child(button);
                    if let Some((_, why)) = connections
                        .connect_refused
                        .as_ref()
                        .filter(|(refused, _)| refused == name)
                    {
                        list =
                            list.with_child(UiNode::status(ids::connect_error(name), why.clone()));
                    }
                }
                nodes.push(list);
            }
        }
        nodes
    }

    /// The open bot's Connections card (#2): its second line, a switch per connection of the
    /// person's own (named by its label, valued by its service, `checked` while it shows as lent
    /// to this bot, dead and `changing` while a change to it is with the server), why a lend or
    /// a revoke of it to this bot did not go through, and the card's sentence about what lending
    /// does not do yet.
    fn agent_connections_node(&self, coworker_id: &str) -> UiNode {
        let connections = &self.connections;
        let mut card = UiNode::list(ids::AGENT_CONNECTIONS, "Connections")
            .with_value(connections::agent_summary(connections, coworker_id));
        for row in connections.own_rows() {
            let changing = connections.is_changing(&row.id);
            let mut switch =
                UiNode::new(ids::connection_lend(&row.id), "switch", row.label.clone())
                    .with_value(connections.connector_label(&row.connector))
                    .with_checked(connections.shows_lent(row, coworker_id))
                    .with_enabled(!changing);
            if changing {
                switch.states.push("changing".into());
            }
            card = card.with_child(switch);
            if let Some(why) = connections.lend_refusal(&row.id, coworker_id) {
                card = card.with_child(UiNode::status(ids::connection_lend_error(&row.id), why));
            }
        }
        card.with_child(UiNode::status(
            ids::AGENT_CONNECTIONS_NOTE,
            connections::LEND_NOTE,
        ))
    }

    /// One of the Connections controls, on Settings → Connections or the open bot's card, or
    /// `None` for a target that is not one.
    ///
    /// The tab answers from anywhere in Settings, and not while Settings is shut: arriving on it
    /// reads the connections, and nobody can press it there. Every other control is refused while
    /// its surface is not on screen, for a row or a service that surface is not showing, and
    /// while it is dead on screen: a connection whose change is with the server, or a Connect
    /// while a sign-in page is being asked for.
    fn connection_command(&self, target: &str) -> Option<Result<Command, String>> {
        if target == ids::SETTINGS_CONNECTIONS {
            return Some(if self.account_open {
                Ok(Command::SetAppSettingsTab(AppSettingsTab::Connections))
            } else {
                Err(format!(
                    "`{target}` is in Settings, which is shut: open it with `{}`",
                    ids::FOOTER_ACCOUNT
                ))
            });
        }
        let connections = &self.connections;
        let on_tab = self.account_open && self.connections_tab;
        let off_tab = || {
            Err(format!(
                "`{target}` is on Settings → Connections, which is not what is on screen: open \
                 it with `{}`",
                ids::SETTINGS_CONNECTIONS
            ))
        };
        let busy = |label: &str| {
            Err(format!(
                "`{target}` is dead: a change to {label} is with the server"
            ))
        };
        if target == ids::CONNECTIONS_REFRESH {
            return Some(if on_tab {
                Ok(Command::RefreshConnections)
            } else {
                off_tab()
            });
        }
        let rows = connections.own_rows();
        if let Some(row) = rows
            .iter()
            .find(|row| target == ids::connection_disconnect(&row.id))
        {
            return Some(if !on_tab {
                off_tab()
            } else if connections.is_changing(&row.id) {
                busy(&row.label)
            } else if connections.is_confirming_disconnect(&row.id) {
                Err(format!(
                    "`{target}` is asking first: click `{}` or `{}`",
                    connections::confirm_yes_id(&row.id),
                    connections::confirm_no_id(&row.id)
                ))
            } else {
                Ok(Command::AskDisconnect(row.id.clone()))
            });
        }
        if let Some(row) = rows.iter().find(|row| {
            target == connections::confirm_yes_id(&row.id)
                || target == connections::confirm_no_id(&row.id)
        }) {
            return Some(if !on_tab {
                off_tab()
            } else if !connections.is_confirming_disconnect(&row.id) {
                Err(format!(
                    "`{target}` is only there while the row asks \"Disconnect …?\": click `{}` first",
                    ids::connection_disconnect(&row.id)
                ))
            } else if target == connections::confirm_yes_id(&row.id) {
                Ok(Command::DisconnectConnection(row.id.clone()))
            } else {
                Ok(Command::KeepConnection)
            });
        }
        if let Some(row) = rows
            .iter()
            .find(|row| target == ids::connection_lend(&row.id))
        {
            let bot = self
                .sessions
                .iter()
                .find(|session| session.active)
                .filter(|_| self.agent_settings_open);
            return Some(match bot {
                None => Err(format!(
                    "`{target}` is on a bot's settings, which are closed"
                )),
                Some(_) if connections.is_changing(&row.id) => busy(&row.label),
                Some(bot) => Ok(Command::SetConnectionLent {
                    connection_id: row.id.clone(),
                    lent: !connections.shows_lent(row, &bot.id),
                }),
            });
        }
        if let ConnectOffer::Offered(open) = connections::connect_offer(connections)
            && let Some(connector) = open
                .iter()
                .find(|connector| target == ids::connect(&connector.name))
        {
            return Some(if !on_tab {
                off_tab()
            } else if connections.opening.is_some() {
                Err(format!(
                    "`{target}` is dead: a sign-in page is being asked for"
                ))
            } else {
                Ok(Command::ConnectService(connector.name.clone()))
            });
        }
        // Shaped like one of these controls, and naming nothing the surfaces show: a wrong
        // address rather than an unknown control. A Connect's refusal line shares its prefix
        // and is no control at all.
        let shaped = target.starts_with("settings-connection-disconnect-")
            || target.starts_with("settings-connection-confirm-")
            || target.starts_with("settings-connection-keep-")
            || target.starts_with("agent-connection-lend-")
            || (target.starts_with("settings-connect-")
                && !target.starts_with("settings-connect-error-"));
        shaped.then(|| {
            Err(format!(
                "no `{target}` on screen: it names no connection or service the page is showing"
            ))
        })
    }

    /// A picker's card and popover, what each picker has, and whether it takes a change now.
    fn picker_snap(
        &self,
        which: PickerFor,
    ) -> (
        Option<&crate::opengrok::ModelPick>,
        &PickerView,
        Option<&String>,
        bool,
    ) {
        match which {
            PickerFor::Bot => (
                self.model_pick.as_ref(),
                &self.model_picker,
                self.picker_note.as_ref(),
                false,
            ),
            PickerFor::NewBots => (
                self.new_bots_pick.as_ref(),
                &self.new_bots_picker,
                self.new_bots_note.as_ref(),
                self.new_bots_busy,
            ),
            PickerFor::PlanFallback => (
                self.plan_fallback_pick.as_ref(),
                &self.plan_fallback_picker,
                self.plan_fallback_note.as_ref(),
                self.plan_fallback_busy,
            ),
        }
    }

    /// A model picker as the window draws it: the Bot's while a Bot is open, `agent-model-card`
    /// in the Bot's settings, the one place a Bot's model is picked; and Default for new Bots'
    /// while the server keeps one, `settings-new-bots-card`, with every part below under
    /// `settings-new-bots-` for `agent-model-`. The card is a button named as the picker reads in
    /// a line ("GPT-6 Luna · Medium ⚡") and valued by the model, with its door's wire word as a
    /// state, `fast` while ⚡ is on, `expanded` while its popover is open, and `saving` while a
    /// change is with the server. Under it the popover, `agent-model-pop`, visible while open,
    /// holds while it shows its controls `agent-model-fast` (a switch, checked while on, dead
    /// with why as its value), `agent-model-open-list` (named by the model) and
    /// `agent-model-reset` (live while there is something to put back), then
    /// `agent-model-effort`, a slider that is there only where the model lists levels of effort
    /// (named by the level it is on, the model's own while the Bot chose none, and valued by that
    /// level's own word, so a driver can write back what it reads; `Effort` and with no value
    /// where no level is lit, a model that names none as its own and none chosen; dead with why
    /// as its own words from a server that keeps no effort; `set_value` takes one of the model's
    /// own levels by its word and refuses any other in words that name them), and
    /// `agent-model-effort-note`, the line a pick left when it put the effort back on the
    /// model's own level, while the model it is about is the one on the card; and while it shows
    /// its
    /// list, `agent-model-open-list` as the heading back (state `expanded`) and
    /// `agent-model-search`, the search box (valued by what is typed). `agent-model-list` is
    /// always there, valued by how many models the search leaves and visible while shown, and
    /// holds while shown `settings-new-bots-none` (new Bots only: `None`, valued
    /// `the server's default`, `selected` while none is set) and the window the list draws: at
    /// most five models, an `agent-model-row-{source}-{id}` each (valued by its door's word,
    /// `selected` on the one that answers, `fast` where it has a fast version), with an
    /// `agent-model-group-{source}` heading ("Subscription", "Gateway") over each group's first
    /// model in view; `agent-model-plan` where a server without per-Bot doors has the account's
    /// plan model answer, `agent-model-no-match` where the search leaves nothing of a list that
    /// has some, `agent-model-routines` where the Bot's own door is the person's plan, where its
    /// routines run, and `agent-model-note`, the server's word on why the list is not
    /// fuller. `agent-model-error`, while open: the server's words for the last change it refused,
    /// or for new Bots that nobody knows whether it was kept.
    fn picker_node(&self, which: PickerFor) -> Option<UiNode> {
        use crate::opengrok::{ListLine, group_title, list_window, row_count};
        use model_picker::{MODELS_TITLE, NO_MODEL_MATCHES, SEARCH_PLACEHOLDER};
        let ids = model_picker::ids(which);
        let (pick, view, note, busy) = self.picker_snap(which);
        let pick = pick?;
        let open = view.open;
        let list_open = open && view.list_open;
        let mut trigger = UiNode::button(ids.card, pick.summary())
            .with_value(pick.model.clone().unwrap_or_default());
        if let Some(door) = pick.door {
            trigger.states.push(door.word().into());
        }
        if pick.is_fast() {
            trigger.states.push("fast".into());
        }
        if open {
            trigger.states.push("expanded".into());
        }
        if busy {
            trigger.states.push("saving".into());
        }
        let mut pop = UiNode::dialog(ids.pop, "Model").with_visible(open);
        if open && !list_open {
            let mut fast = UiNode::new(ids.fast, "switch", model_picker::FAST_LABEL)
                .with_checked(pick.is_fast())
                .with_enabled(pick.fast_blocked.is_none() && !busy);
            if let Some(why) = pick.fast_blocked {
                fast = fast.with_value(why);
            }
            // The window's own order: ⚡, the model's name and ↺ in a row, then the slider under
            // them, which is not there at all where the model lists no levels, and the line a
            // pick may have left.
            pop = pop
                .with_child(fast)
                .with_child(UiNode::button(ids.open_list, pick.model_label()))
                .with_child(
                    UiNode::button(ids.reset, model_picker::RESET_LABEL)
                        .with_enabled(pick.can_reset() && !busy),
                );
            if pick.has_slider() {
                // Valued by the word of the level it is lit on, the model's own where the Bot
                // chose none, so what a driver reads is what it can write back; a slider with
                // no level lit has no word to say.
                let mut slider = UiNode::new(
                    ids.effort,
                    "slider",
                    pick.effort_name()
                        .unwrap_or_else(|| model_picker::EFFORT_LABEL.to_string()),
                )
                .with_enabled(pick.effort_dead.is_none() && !busy);
                if let Some(level) = pick.lit_level() {
                    slider = slider.with_value(level.value.clone());
                }
                pop = pop.with_child(slider);
            }
            if let Some(line) = view.effort_note_for(pick) {
                pop = pop.with_child(UiNode::status(ids.effort_note, line));
            }
        }
        // What the search leaves: the whole list while nothing is typed, which it always is while
        // the list is shut.
        let groups = pick.search(&view.search);
        let mut list = UiNode::list(ids.list, MODELS_TITLE)
            .with_value(row_count(&groups).to_string())
            .with_visible(list_open);
        if list_open {
            let mut back = UiNode::button(ids.open_list, MODELS_TITLE);
            back.states.push("expanded".into());
            pop = pop.with_child(back).with_child(
                UiNode::textbox(ids.search, SEARCH_PLACEHOLDER).with_value(view.search.clone()),
            );
            let none = ids
                .none
                .filter(|_| model_picker::none_shows(&view.search, ids.none_hint));
            if let Some(id) = none {
                let mut row = UiNode::listitem(id, crate::opengrok::NEW_BOTS_NONE)
                    .with_value(ids.none_hint)
                    .with_enabled(!busy);
                if pick.model.is_none() {
                    row.states.push("selected".into());
                }
                list = list.with_child(row);
            }
            let plan = pick.plan_line(&view.search);
            if let Some(plan) = plan {
                let model = plan.model.as_deref().map_or_else(
                    || crate::opengrok::NO_MODEL.to_string(),
                    crate::opengrok::base_label,
                );
                list = list.with_child(
                    UiNode::status(ids.plan, model)
                        .with_value(plan.model.clone().unwrap_or_default()),
                );
            }
            for line in list_window(&groups, view.list_start) {
                list = list.with_child(match line {
                    ListLine::Heading(source) => {
                        UiNode::new(ids.group_id(source), "heading", group_title(source))
                    }
                    ListLine::Row(row) => {
                        let mut item = UiNode::listitem(
                            ids.row_id(row.source, &row.base_id),
                            row.label.clone(),
                        )
                        .with_value(row.source.word())
                        .with_enabled(!busy);
                        if pick.is_current(row) {
                            item.states.push("selected".into());
                        }
                        if row.has_fast {
                            item.states.push("fast".into());
                        }
                        item
                    }
                });
            }
            let offers_nothing = pick.groups.is_empty() && pick.account_plan.is_none();
            if !offers_nothing && groups.is_empty() && plan.is_none() && none.is_none() {
                list = list.with_child(UiNode::status(ids.no_match, NO_MODEL_MATCHES));
            }
            if let Some(line) = &pick.routines {
                list = list.with_child(UiNode::status(ids.routines, line.clone()));
            }
            if let Some(note) = &self.model_note {
                list = list.with_child(UiNode::status(ids.note, note.clone()));
            }
        }
        pop = pop.with_child(list);
        if open && let Some(note) = note {
            pop = pop.with_child(UiNode::status(ids.error, note.clone()));
        }
        Some(trigger.with_child(pop))
    }

    /// One computer's card as the window draws it: `settings-computer-{id}` (named by the
    /// computer's label; states `online` or `offline`, `this-computer` on the one the app is
    /// running on, and `unsaved` on that one while a change to its opencodex fields waits for
    /// Save) holding its Relay your plan switch, `settings-computer-{id}-relay` (checked while the
    /// computer's relay is on, or where a click is taking it while the server is asked; disabled
    /// then), where its relay stands, `settings-computer-{id}-status` (`Relaying`, `Not
    /// relaying` or `On, but asleep: it can't answer right now`; value `relaying`,
    /// `not-relaying` or `asleep`), and under the switch `settings-computer-{id}-error` while a
    /// switch did not go as asked (the server's words, or that nobody knows whether it was kept).
    /// The card of this computer alone also holds why its relay is not relaying,
    /// `settings-relay-detail`, and what its relay needs of it ([`Self::opencodex_nodes`]).
    fn computer_card_node(&self, card: &ComputerCard) -> UiNode {
        let machine = &card.machine_id;
        let mut node = UiNode::new(ids::computer_card(machine), "group", card.label.clone());
        node.states
            .push(if card.online { "online" } else { "offline" }.into());
        if card.this_computer {
            node.states.push("this-computer".into());
            if self.reply_source.unsaved {
                node.states.push("unsaved".into());
            }
        }
        node = node
            .with_child(
                UiNode::new(
                    ids::computer_relay(machine),
                    "switch",
                    computers::RELAY_LABEL,
                )
                .with_checked(card.relay_on)
                .with_enabled(!card.switching),
            )
            .with_child(
                UiNode::status(ids::computer_status(machine), card.status_words())
                    .with_value(card.state.word()),
            );
        if let Some(note) = &card.note {
            node = node.with_child(UiNode::status(ids::computer_error(machine), note.clone()));
        }
        if let Some(detail) = &card.detail {
            let mut line = UiNode::status(ids::RELAY_DETAIL, detail.words.clone());
            if detail.trouble {
                line.states.push("trouble".into());
            }
            node = node.with_child(line);
        }
        if card.this_computer {
            for child in self.opencodex_nodes() {
                node = node.with_child(child);
            }
        }
        node
    }

    /// This computer's opencodex address and key, and Save, as its card draws them: until the
    /// setting has been read, on a server without reply sources, or when it could not be read,
    /// only the line the card draws in place of them (`settings-reply-source-unavailable`, state
    /// `asking` while that is the server being asked); from a server without the relay,
    /// `settings-relay-unavailable` (that this server can't take replies from a computer yet) and
    /// the line under Save; and from one with it, opencodex's address, the key field (never its
    /// text: states `set` while the Keychain holds a key, `typed` while one waits for Save) with
    /// Remove key, `settings-reply-source-error` (a read that failed, state `trouble`, or what
    /// the Keychain said when it did not keep a key), `settings-reply-source-hint` (what Save
    /// waits for) and Save. Nothing of the plan on the server's own machine is on it, nor a model
    /// of the relay's own.
    fn opencodex_nodes(&self) -> Vec<UiNode> {
        let snap = &self.reply_source;
        let settings = &snap.settings;
        let relay = &snap.relay;
        let Some(_) = settings.kept_source() else {
            let line = computers::unavailable_line(settings).unwrap_or(computers::ASKING);
            let mut node = UiNode::status(ids::REPLY_SOURCE_UNAVAILABLE, line);
            if line == computers::ASKING {
                node.states.push("asking".into());
            }
            return vec![node];
        };
        let mut nodes = Vec::new();
        // The line under Save: state `trouble` while it is drawn in the danger colour, a failed
        // read; what the Keychain said is drawn muted.
        let error = computers::error_line(settings).map(|line| {
            let mut error = UiNode::status(ids::REPLY_SOURCE_ERROR, line);
            if computers::error_is_trouble(settings) {
                error.states.push("trouble".into());
            }
            error
        });
        // From a server before the relay: the line saying so in the fields' place, and nothing
        // to save.
        if !settings.knows_relay() {
            nodes.push(UiNode::status(
                ids::RELAY_UNAVAILABLE,
                computers::RELAY_NOT_ON_SERVER,
            ));
            nodes.extend(error);
            return nodes;
        }
        nodes.push(
            UiNode::textbox(ids::RELAY_ADDR, computers::RELAY_ADDR_LABEL)
                .with_value(relay.address.clone())
                .with_enabled(settings.relay_editable()),
        );
        // The window draws a key waiting for Save as masked dots in the field, a driver's too.
        let mut key = UiNode::textbox(ids::RELAY_KEY, computers::RELAY_KEY_LABEL)
            .with_enabled(settings.relay_key_editable());
        if relay.has_key {
            key.states.push("set".into());
        }
        if relay.key_typed {
            key.states.push("typed".into());
        }
        // A key typed and left behind when the page was left: the window asks for it again.
        if settings.relay_retype_key {
            key.states.push("retype".into());
        }
        nodes.push(key);
        if relay.has_key {
            let mut remove = UiNode::button(
                ids::RELAY_KEY_REMOVE,
                if settings.relay_remove_key {
                    computers::KEEP_KEY_LABEL
                } else {
                    computers::REMOVE_KEY_LABEL
                },
            )
            .with_enabled(settings.relay_editable());
            if settings.relay_remove_key {
                remove.states.push("picked".into());
            }
            nodes.push(remove);
        }
        nodes.extend(error);
        if let Some(hint) = snap.hint {
            nodes.push(UiNode::status(ids::REPLY_SOURCE_HINT, hint));
        }
        nodes.push(
            UiNode::button(ids::REPLY_SOURCE_SAVE, computers::SAVE_LABEL)
                .with_enabled(snap.can_save),
        );
        nodes
    }

    /// Settings → General's first section as the page draws it: `settings-default-models`
    /// (named `Default models`), holding Default for new Bots and the Relay-off fallback.
    fn default_models_node(&self) -> UiNode {
        UiNode::new(ids::DEFAULT_MODELS, "group", default_models::TITLE)
            .with_child(self.new_bots_node())
            .with_child(self.plan_fallback_node())
    }

    /// The Relay-off fallback as the page draws it: `settings-plan-fallback`, as Default for new
    /// Bots is ([`Self::new_bots_node`]). While the server keeps no such fallback (state
    /// `unavailable`) it holds `settings-plan-fallback-unavailable`, the line saying it is coming,
    /// and `settings-plan-fallback-card`, disabled; while it keeps one, the live card and its
    /// popover over the Gateway group alone, and while the popover is shut
    /// `settings-plan-fallback-error`.
    fn plan_fallback_node(&self) -> UiNode {
        let mut section = UiNode::new(
            ids::PLAN_FALLBACK,
            "group",
            default_models::PLAN_FALLBACK_TITLE,
        );
        match &self.reply_source.plan_fallback {
            crate::state::RelayOffFallback::NotOnServer => {
                section = section
                    .with_child(UiNode::status(
                        ids::PLAN_FALLBACK_UNAVAILABLE,
                        default_models::PLAN_FALLBACK_COMING_SOON,
                    ))
                    .with_child(
                        UiNode::button(ids::PLAN_FALLBACK_CARD, model_picker::dead_card_model())
                            .with_enabled(false),
                    );
                section.states.push("unavailable".into());
            }
            crate::state::RelayOffFallback::Kept(_) => {
                if let Some(card) = self.picker_node(PickerFor::PlanFallback) {
                    section = section.with_child(card);
                }
                if !self.plan_fallback_picker.open
                    && let Some(note) = &self.plan_fallback_note
                {
                    section = section.with_child(UiNode::status(
                        model_picker::PLAN_FALLBACK_IDS.error,
                        note.clone(),
                    ));
                }
            }
        }
        section
    }

    /// Default for new Bots as the page draws it: `settings-new-bots`. While the server keeps no
    /// default for new Bots (state `unavailable`) it holds `settings-new-bots-unavailable`, the
    /// line saying it is coming, and `settings-new-bots-card`, the picker's card, named as it
    /// reads in a line and disabled, as the window dims it. While the server keeps one it holds
    /// the live card and its popover ([`Self::picker_node`]), and while the popover is shut,
    /// `settings-new-bots-error`: what the last change that did not go as asked came to.
    fn new_bots_node(&self) -> UiNode {
        let section = UiNode::new(ids::NEW_BOTS, "group", default_models::NEW_BOTS_TITLE);
        match &self.reply_source.new_bots {
            crate::state::DefaultForNewBots::NotOnServer => {
                let mut section = section
                    .with_child(UiNode::status(
                        ids::NEW_BOTS_UNAVAILABLE,
                        default_models::NEW_BOTS_COMING_SOON,
                    ))
                    .with_child(
                        UiNode::button(ids::NEW_BOTS_CARD, model_picker::dead_card_model())
                            .with_enabled(false),
                    );
                section.states.push("unavailable".into());
                section
            }
            crate::state::DefaultForNewBots::Kept(_) => {
                let mut section = section;
                if let Some(card) = self.picker_node(PickerFor::NewBots) {
                    section = section.with_child(card);
                }
                if !self.new_bots_picker.open
                    && let Some(note) = &self.new_bots_note
                {
                    section = section.with_child(UiNode::status(
                        model_picker::NEW_BOTS_IDS.error,
                        note.clone(),
                    ));
                }
                section
            }
        }
    }

    /// One of the Bot's model picker's controls on its card, or `None` for a target that is none
    /// of them ([`is_picker_part`]).
    fn model_picker_command(&self, target: &str) -> Option<Result<Command, String>> {
        is_picker_part(target).then(|| self.picker_control(PickerFor::Bot, target))
    }

    /// Why `which` picker cannot be worked at all now, or `None` while it can: no Bot open for
    /// the Bot's; and for Default for new Bots', its page not on screen, or a server that keeps no
    /// default for new Bots, where its card is dead.
    fn picker_absent(&self, which: PickerFor, target: &str) -> Option<String> {
        if which != PickerFor::Bot
            && let Some(off) = self.off_the_general_page(target)
        {
            return Some(off);
        }
        if self.picker_snap(which).0.is_some() {
            return None;
        }
        Some(match which {
            PickerFor::Bot => format!("`{target}` is not on screen: no Bot is open"),
            PickerFor::NewBots => {
                format!(
                    "`{target}` is dead: {}",
                    default_models::NEW_BOTS_COMING_SOON
                )
            }
            PickerFor::PlanFallback => {
                format!(
                    "`{target}` is dead: {}",
                    default_models::PLAN_FALLBACK_COMING_SOON
                )
            }
        })
    }

    /// A click on one of a picker's parts, refused while it is not on screen: the Bot's card
    /// while the Bot's settings are shut, a part of the popover while it is shut, its controls
    /// while it shows its list, and its list's parts while it does not. ⚡ where it is dead is
    /// refused with why, and ↺ with nothing to put back; the slider and the search box are set
    /// with `set_value`, not clicked. A model is picked only while it is in the list's window, as
    /// a person can click only what is in view: one further down is out of view until the search
    /// box finds it. Default for new Bots' controls are dead while a change is with the server.
    fn picker_control(&self, which: PickerFor, target: &str) -> Result<Command, String> {
        let ids = model_picker::ids(which);
        if let Some(absent) = self.picker_absent(which, target) {
            return Err(absent);
        }
        let (pick, view, _, busy) = self.picker_snap(which);
        let Some(pick) = pick else {
            return Err(format!("`{target}` is not on screen"));
        };
        if target == ids.card {
            return if which == PickerFor::Bot && !self.agent_settings_open {
                Err(format!(
                    "`{target}` is in the Bot's settings, which are closed"
                ))
            } else {
                Ok(PickerCommand::Toggle.sent_to(which))
            };
        }
        if !view.open {
            return Err(format!(
                "`{target}` is in the model picker's popover, which is shut: open it with `{}`",
                ids.card
            ));
        }
        let list_open = view.list_open;
        let open_list = ids.open_list;
        let dead = || Err(format!("`{target}` is dead: a change is with the server"));
        if target == ids.pop {
            return Err(format!(
                "`{target}` is the popover: click one of its controls"
            ));
        }
        if target == open_list {
            return Ok(PickerCommand::ToggleList.sent_to(which));
        }
        if [
            ids.plan,
            ids.note,
            ids.routines,
            ids.error,
            ids.no_match,
            ids.effort_note,
        ]
        .contains(&target)
        {
            return Err(format!("`{target}` is a line, not a control"));
        }
        if [ids.fast, ids.effort, ids.reset].contains(&target) && list_open {
            return Err(format!(
                "`{target}` is not on screen: the popover shows its list, and `{open_list}` goes \
                 back"
            ));
        }
        if target == ids.fast {
            return match pick.fast_blocked {
                Some(why) => Err(format!("`{target}` is dead: {why}")),
                None if busy => dead(),
                None => Ok(PickerCommand::SetFast(!pick.is_fast()).sent_to(which)),
            };
        }
        if target == ids.effort {
            return Err(if pick.has_slider() {
                format!(
                    "`{target}` is a slider: set_value it to one of {}",
                    levels_in_words(pick)
                )
            } else {
                no_slider_refusal(pick, target)
            });
        }
        if target == ids.reset {
            return if !pick.can_reset() {
                Err(format!(
                    "`{target}` has nothing to put back: the effort is the model's own, and ⚡ is \
                     off or cannot be switched"
                ))
            } else if busy {
                dead()
            } else {
                Ok(PickerCommand::Reset.sent_to(which))
            };
        }
        if target == ids.list {
            return Err(format!(
                "`{target}` is the list: click one of its models, `{}{{source}}-{{id}}`",
                ids.row
            ));
        }
        if !list_open {
            return Err(format!(
                "`{target}` is not on screen: the popover shows its controls, and `{open_list}` \
                 opens the list"
            ));
        }
        if target == ids.search {
            return Err(format!("`{target}` is a field: use set_value"));
        }
        let query = view.search.as_str();
        let search = ids.search;
        if ids.none == Some(target) {
            return if !model_picker::none_shows(query, ids.none_hint) {
                Err(format!(
                    "`{target}` is not on screen: the search `{query}` in `{search}` leaves it out"
                ))
            } else if busy {
                dead()
            } else if which == PickerFor::PlanFallback {
                Ok(Command::ClearPlanFallback)
            } else {
                Ok(Command::ClearNewBotsDefault)
            };
        }
        if target.starts_with(ids.group) {
            return Err(format!("`{target}` is a group's heading, not a model"));
        }
        use crate::opengrok::{LIST_ROWS, ListLine, list_window};
        let rest = target.strip_prefix(ids.row).unwrap_or(target);
        let found = crate::opengrok::InferenceKind::ALL
            .into_iter()
            .find_map(|source| {
                let base_id = rest.strip_prefix(source.word())?.strip_prefix('-')?;
                pick.row(source, base_id)
            });
        let Some(found) = found else {
            let offered: Vec<String> = pick
                .rows()
                .map(|row| ids.row_id(row.source, &row.base_id))
                .collect();
            return Err(format!(
                "no `{target}` in the list, which offers {}",
                if offered.is_empty() {
                    "nothing".to_string()
                } else {
                    offered.join(", ")
                }
            ));
        };
        if !found.matches(query) {
            return Err(format!(
                "`{target}` is not on screen: the search `{query}` in `{search}` leaves it out"
            ));
        }
        let groups = pick.search(query);
        let in_view = list_window(&groups, view.list_start)
            .into_iter()
            .any(|line| line == ListLine::Row(found));
        if !in_view {
            return Err(format!(
                "`{target}` is out of view: the list shows {LIST_ROWS} models at a time, and \
                 `{search}` finds the rest"
            ));
        }
        if busy {
            return dead();
        }
        Ok(PickerCommand::Pick {
            source: found.source,
            base_id: found.base_id.clone(),
        }
        .sent_to(which))
    }

    /// `set_value` on a list's search box, as typing it there would leave it: the list shows
    /// only the models it leaves, from the top. Refused while the box is not on screen: the
    /// popover shut, or showing its controls.
    fn set_picker_search(
        &mut self,
        which: PickerFor,
        target: &str,
        value: &str,
    ) -> Result<DispatchResult, String> {
        if let Some(closed) = self.picker_search_closed(which, target) {
            return Err(closed);
        }
        let view = match which {
            PickerFor::Bot => &mut self.model_picker,
            PickerFor::NewBots => &mut self.new_bots_picker,
            PickerFor::PlanFallback => &mut self.plan_fallback_picker,
        };
        view.search = value.to_string();
        view.list_start = 0;
        self.pending = Some(PickerCommand::SetSearch(value.to_string()).sent_to(which));
        Ok(DispatchResult::empty())
    }

    /// Why a list's search box takes no typing now, or `None` while it does.
    fn picker_search_closed(&self, which: PickerFor, target: &str) -> Option<String> {
        if let Some(absent) = self.picker_absent(which, target) {
            return Some(absent);
        }
        let ids = model_picker::ids(which);
        let view = self.picker_snap(which).1;
        if !view.open {
            return Some(format!(
                "`{target}` is in the model picker's popover, which is shut: open it with `{}`",
                ids.card
            ));
        }
        if !view.list_open {
            return Some(format!(
                "`{target}` is not on screen: the popover shows its controls, and `{}` opens the \
                 list",
                ids.open_list
            ));
        }
        None
    }

    /// `set_value` on a picker's slider: one of the model's own levels by the server's word,
    /// saved at once, as letting go of the thumb there is. Refused off screen, as a click is,
    /// where the model lists no levels and so the slider is not there, where it is dead (a server
    /// that keeps no effort), while a change is with the server, and for a word that is none of
    /// the model's levels, in words that name them (`inherit` is ↺'s).
    fn set_picker_effort(
        &mut self,
        which: PickerFor,
        target: &str,
        value: &str,
    ) -> Result<DispatchResult, String> {
        if let Some(absent) = self.picker_absent(which, target) {
            return Err(absent);
        }
        let ids = model_picker::ids(which);
        let (pick, view, _, busy) = self.picker_snap(which);
        let Some(pick) = pick else {
            return Err(format!("`{target}` is not on screen"));
        };
        if !view.open {
            return Err(format!(
                "`{target}` is in the model picker's popover, which is shut: open it with `{}`",
                ids.card
            ));
        }
        if view.list_open {
            return Err(format!(
                "`{target}` is not on screen: the popover shows its list, and `{}` goes back",
                ids.open_list
            ));
        }
        if !pick.has_slider() {
            return Err(no_slider_refusal(pick, target));
        }
        let word = value.trim();
        pick.effort_patch(word)
            .map_err(|why| format!("`{target}`: {why}"))?;
        if busy {
            return Err(format!("`{target}` is dead: a change is with the server"));
        }
        self.pending = Some(PickerCommand::SetEffort(word.to_string()).sent_to(which));
        Ok(DispatchResult::empty())
    }

    /// One of a computer's card's controls on Settings → Computer, or `None` for a target that is
    /// none of them: the Relay your plan switch of a computer on the roster, which acts at once and
    /// asks for the other way from where it is drawn; the card and its lines, which are lines and
    /// not controls; and this computer's opencodex fields, key and Save
    /// ([`Self::opencodex_command`]). Each is refused while Settings is not open on Computer.
    fn computer_card_command(&self, target: &str) -> Option<Result<Command, String>> {
        if let Some(command) = self.opencodex_command(target) {
            return Some(command);
        }
        let card = self.computers.iter().find(|card| {
            let machine = &card.machine_id;
            [
                ids::computer_card(machine),
                ids::computer_relay(machine),
                ids::computer_status(machine),
                ids::computer_error(machine),
            ]
            .iter()
            .any(|id| id == target)
        })?;
        if let Some(off) = self.off_the_computer_page(target) {
            return Some(Err(off));
        }
        if target != ids::computer_relay(&card.machine_id) {
            return Some(Err(format!(
                "`{target}` is a card or a line on it, not a control: its Relay your plan switch is \
                 `{}`",
                ids::computer_relay(&card.machine_id)
            )));
        }
        Some(if card.switching {
            Err(format!(
                "`{target}` is dead: the switch is with the server, which has not answered"
            ))
        } else {
            Ok(Command::SetComputerRelay {
                machine_id: card.machine_id.clone(),
                on: !card.relay_on,
            })
        })
    }

    /// One of Settings → General's Default models, or `None` for a target that is none of them.
    /// The tab answers from anywhere in Settings, and not while Settings is shut; the section is
    /// a line; Default for new Bots' parts are its own ([`Self::new_bots_control`]).
    fn default_models_command(&self, target: &str) -> Option<Result<Command, String>> {
        if target == ids::SETTINGS_GENERAL {
            return Some(if self.account_open {
                Ok(Command::SetAppSettingsTab(AppSettingsTab::General))
            } else {
                Err(format!(
                    "`{target}` is in Settings, which is shut: open it with `{}`",
                    ids::FOOTER_ACCOUNT
                ))
            });
        }
        if target == ids::DEFAULT_MODELS {
            return Some(Err(self.off_the_general_page(target).unwrap_or_else(
                || format!("`{target}` is a section of the page, not a control"),
            )));
        }
        if target.starts_with(ids::NEW_BOTS) {
            return Some(self.new_bots_control(target));
        }
        if target.starts_with(ids::PLAN_FALLBACK) {
            return Some(self.plan_fallback_control(target));
        }
        None
    }

    /// The Relay-off fallback's parts, as Default for new Bots' are ([`Self::new_bots_control`]):
    /// its popover's dismiss answers anywhere; the rest are refused off General; while the server
    /// keeps no such fallback the card is dead and says why, and the rest are lines; while it
    /// keeps one, the card and its popover are the Bot's ([`Self::picker_control`]).
    fn plan_fallback_control(&self, target: &str) -> Result<Command, String> {
        if target == ids::PLAN_FALLBACK_DISMISS {
            return Ok(PickerCommand::SetOpen(false).sent_to(PickerFor::PlanFallback));
        }
        if let Some(off) = self.off_the_general_page(target) {
            return Err(off);
        }
        let parts = &model_picker::PLAN_FALLBACK_IDS;
        match &self.reply_source.plan_fallback {
            crate::state::RelayOffFallback::NotOnServer => Err(if target == parts.card {
                format!(
                    "`{target}` is dead: {}",
                    default_models::PLAN_FALLBACK_COMING_SOON
                )
            } else {
                format!("`{target}` is a line on the page, not a control")
            }),
            crate::state::RelayOffFallback::Kept(_) => {
                if target == parts.error || target == ids::PLAN_FALLBACK {
                    Err(format!("`{target}` is a line on the page, not a control"))
                } else if parts.holds(target) {
                    self.picker_control(PickerFor::PlanFallback, target)
                } else {
                    Err(format!("no `{target}` on the page"))
                }
            }
        }
    }

    /// Why a control of Settings → General is not on screen: Settings is not open on the page.
    fn off_the_general_page(&self, target: &str) -> Option<String> {
        (!(self.account_open && self.general_tab)).then(|| {
            format!(
                "`{target}` is on Settings → General, which is not what is on screen: open it \
                 with `{}`",
                ids::SETTINGS_GENERAL
            )
        })
    }

    /// Why a control of Settings → Computer is not on screen: Settings is not open on the page.
    fn off_the_computer_page(&self, target: &str) -> Option<String> {
        (!(self.account_open && self.computer_tab)).then(|| {
            format!(
                "`{target}` is on Settings → Computer, which is not what is on screen: open it \
                 with `settings-tab-computer`"
            )
        })
    }

    /// Default for new Bots' parts. Its popover's dismiss answers anywhere, as the Bot's does;
    /// the rest are refused off the page, as the page's are. While the server keeps no default
    /// for new Bots, the card is dead and says why, since a pick in it would change nothing
    /// there, and the rest are lines; while it keeps one, the card and its popover are the Bot's
    /// ([`Self::picker_control`]).
    fn new_bots_control(&self, target: &str) -> Result<Command, String> {
        if target == ids::NEW_BOTS_DISMISS {
            return Ok(PickerCommand::SetOpen(false).sent_to(PickerFor::NewBots));
        }
        if let Some(off) = self.off_the_general_page(target) {
            return Err(off);
        }
        match &self.reply_source.new_bots {
            crate::state::DefaultForNewBots::NotOnServer => Err(if target == ids::NEW_BOTS_CARD {
                format!(
                    "`{target}` is dead: {}",
                    default_models::NEW_BOTS_COMING_SOON
                )
            } else {
                format!("`{target}` is a line on the page, not a control")
            }),
            crate::state::DefaultForNewBots::Kept(_) => {
                if target == model_picker::NEW_BOTS_IDS.error || target == ids::NEW_BOTS {
                    Err(format!("`{target}` is a line on the page, not a control"))
                } else if model_picker::NEW_BOTS_IDS.holds(target) {
                    self.picker_control(PickerFor::NewBots, target)
                } else {
                    Err(format!("no `{target}` on the page"))
                }
            }
        }
    }

    /// One of this computer's opencodex controls, or `None` for a target that is none of them:
    /// the lines about them are lines, the address and the key are fields, Remove key only while
    /// the Keychain holds one, and Save keeps this computer's half of the relay and is refused with
    /// what it waits for. Refused off the page, while this computer is not on the roster (it has no
    /// card), before the setting is read, and from a server without the relay, which draws none of
    /// them. An id the card does not draw, the plan on the server's own machine's among them, is
    /// no control of it either.
    fn opencodex_command(&self, target: &str) -> Option<Result<Command, String>> {
        let ours = [
            ids::RELAY_DETAIL,
            ids::RELAY_ADDR,
            ids::RELAY_KEY,
            ids::RELAY_KEY_REMOVE,
            ids::RELAY_UNAVAILABLE,
            ids::REPLY_SOURCE_SAVE,
            ids::REPLY_SOURCE_ERROR,
            ids::REPLY_SOURCE_HINT,
            ids::REPLY_SOURCE_UNAVAILABLE,
        ];
        if ours.contains(&target) {
            return Some(self.opencodex_control(target));
        }
        // Everything else of Settings → Relay is gone with the page: its tab, its section, its
        // card, its switch and its lines, and the plan on the server's own machine that it no
        // longer sets up. A driver that still asks for one is told where the relay is.
        let retired = target == RETIRED_RELAY_TAB
            || target.starts_with("settings-relay")
            || target.starts_with("settings-reply-source");
        retired.then(|| {
            Err(format!(
                "no `{target}` on screen: Settings → Relay is gone, and each computer has its own \
                 Relay your plan switch on its card on Settings → Computer \
                 (`settings-computer-{{id}}-relay`)"
            ))
        })
    }

    fn opencodex_control(&self, target: &str) -> Result<Command, String> {
        if let Some(off) = self.off_the_computer_page(target) {
            return Err(off);
        }
        if !self.computers.iter().any(|card| card.this_computer) {
            return Err(format!(
                "no `{target}` on screen: this computer is not among the computers listed yet"
            ));
        }
        let snap = &self.reply_source;
        let settings = &snap.settings;
        if settings.kept_source().is_none() {
            let line = computers::unavailable_line(settings).unwrap_or(computers::ASKING);
            return if target == ids::REPLY_SOURCE_UNAVAILABLE {
                Err(format!("`{target}` is a line on the card, not a control"))
            } else {
                Err(format!("no `{target}` on screen: {line}"))
            };
        }
        if !settings.knows_relay() {
            return Err(
                if target == ids::RELAY_UNAVAILABLE || target == ids::REPLY_SOURCE_ERROR {
                    format!("`{target}` is a line on the card, not a control")
                } else {
                    format!(
                        "no `{target}` on screen: {}",
                        computers::RELAY_NOT_ON_SERVER
                    )
                },
            );
        }
        if target == ids::RELAY_ADDR || target == ids::RELAY_KEY {
            return Err(format!("`{target}` is a field: use set_value"));
        }
        if target == ids::REPLY_SOURCE_SAVE {
            return if snap.can_save {
                Ok(Command::SaveReplySource)
            } else if settings.reading.is_some() {
                Err(format!(
                    "`{target}` is dead: the setting is being read, and Save waits for what it \
                     brings"
                ))
            } else if let Some(hint) = snap.hint.filter(|_| snap.unsaved) {
                Err(format!("`{target}` is dead: {hint}"))
            } else {
                Err(format!(
                    "`{target}` is dead: nothing on the card differs from what this computer \
                     keeps"
                ))
            };
        }
        if target == ids::RELAY_KEY_REMOVE {
            return if snap.relay.has_key {
                Ok(Command::ToggleRemoveRelayKey)
            } else {
                Err(format!(
                    "no `{target}` on screen: this computer's secure storage holds no key"
                ))
            };
        }
        Err(format!("`{target}` is a line on the card, not a control"))
    }

    /// Why the relay's address or key field takes no typing now, or `None` while it does: the
    /// page's own reasons for drawing it read-only. Off the page, before the setting is read,
    /// from a server without the relay, and the key while Remove key is picked. Every way a
    /// driver types asks it first (`set_value`, `type`, `key`), so none of them reaches a draft
    /// the window would not let a person change.
    fn relay_field_closed(&self, target: &str) -> Option<String> {
        if let Some(off) = self.off_the_computer_page(target) {
            return Some(off);
        }
        if !self.computers.iter().any(|card| card.this_computer) {
            return Some(format!(
                "no `{target}` on screen: this computer is not among the computers listed yet"
            ));
        }
        let settings = &self.reply_source.settings;
        if settings.kept_source().is_none() {
            let line = computers::unavailable_line(settings).unwrap_or(computers::ASKING);
            return Some(format!("no `{target}` on screen: {line}"));
        }
        if !settings.knows_relay() {
            return Some(format!(
                "no `{target}` on screen: {}",
                computers::RELAY_NOT_ON_SERVER
            ));
        }
        if target == ids::RELAY_KEY && !settings.relay_key_editable() {
            return Some(format!(
                "`{target}` is dead: Remove key waits for Save; `{}`, now Keep key, takes it back",
                ids::RELAY_KEY_REMOVE
            ));
        }
        None
    }

    /// `set_value` on the relay's address or key field, as typing it there would. The key goes
    /// to the page, drawn masked, and never into a tree.
    fn set_relay_field(&mut self, target: &str, value: &str) -> Result<DispatchResult, String> {
        if let Some(closed) = self.relay_field_closed(target) {
            return Err(closed);
        }
        self.pending = Some(if target == ids::RELAY_ADDR {
            Command::SetRelayAddress(value.to_string())
        } else {
            Command::SetRelayKey(RedactedSecret(value.to_string()))
        });
        Ok(DispatchResult::empty())
    }

    /// Keys aimed at the relay's fields one by one: `key` on either, `type` on the key. They are
    /// refused, and say what writes the field instead, or first why the page draws it read-only.
    /// What the key field holds is never copied here, so there is nothing to add a key to or
    /// take one off: the key is written whole. The address is written by `set_value`, and
    /// `type` adds to it.
    fn relay_field_keys(&self, target: &str) -> String {
        if let Some(closed) = self.relay_field_closed(target) {
            return closed;
        }
        if target == ids::RELAY_KEY {
            format!("`{target}` takes the whole key: use set_value")
        } else {
            format!("`{target}` takes text, not single keys: use set_value, or type to add to it")
        }
    }

    /// Settings → Logins as the page draws it: the search field with Add beside it, Import…,
    /// the notice and error lines, the sections the search leaves with their rows under
    /// them, the picked row's pane, and the Add sheet while it is up.
    fn logins_nodes(&self, mut settings: UiNode) -> UiNode {
        let rows: Vec<SiteLoginRecord> = self
            .site_logins
            .iter()
            .map(|login| login.row.clone())
            .collect();
        settings = settings
            .with_child(
                UiNode::textbox("settings-logins-search", "Search")
                    .with_value(self.site_login_query.clone()),
            )
            .with_child(UiNode::button("settings-login-add", "Add"))
            .with_child(UiNode::button("settings-login-import", "Import…"));
        if let Some(notice) = &self.site_login_notice {
            settings =
                settings.with_child(UiNode::status("settings-logins-notice", notice.clone()));
        }
        if let Some(error) = &self.site_login_error {
            settings = settings.with_child(UiNode::status("settings-logins-error", error.clone()));
        }
        let with_code: std::collections::HashSet<String> = self
            .site_logins
            .iter()
            .filter(|login| login.has_code)
            .map(|login| login.row.id.clone())
            .collect();
        let groups = grouped_logins(&rows, &self.site_login_query, &with_code);
        if rows.is_empty() {
            settings = settings.with_child(UiNode::status(
                "settings-logins-empty",
                "No saved logins yet.",
            ));
        } else if groups.is_empty() {
            settings =
                settings.with_child(UiNode::status("settings-logins-empty", "No logins match."));
        }
        // A row with a `Security:` note is under its kind and under Security: one id, twice,
        // as on the screen.
        for (group, members) in groups {
            let mut node = UiNode::new(
                format!("settings-logins-group-{}", group.id()),
                "group",
                group.title(),
            )
            .with_value(members.len().to_string());
            for row in members {
                let selected = self.site_login_selected.as_deref() == Some(row.id.as_str());
                node = node.with_child(site_login_node(row, selected));
            }
            settings = settings.with_child(node);
        }
        // The pick stays on the pane even when a search hides its row.
        if let Some(picked) = self
            .site_login_selected
            .as_ref()
            .and_then(|id| self.site_logins.iter().find(|login| &login.row.id == id))
        {
            settings = settings.with_child(site_login_detail_node(picked));
        }
        if self.site_login_add_open {
            settings = settings.with_child(
                UiNode::dialog("settings-login-add-sheet", "New Login")
                    .with_child(UiNode::textbox("settings-login-add-title", "Title"))
                    .with_child(UiNode::textbox("settings-login-add-username", "User Name"))
                    .with_child(UiNode::textbox("settings-login-add-password", "Password"))
                    .with_child(UiNode::textbox("settings-login-add-website", "Website"))
                    .with_child(UiNode::textbox("settings-login-add-notes", "Notes"))
                    .with_child(UiNode::button("settings-login-add-cancel", "Cancel"))
                    .with_child(UiNode::button("settings-login-add-save", "Save")),
            );
        }
        settings
    }

    /// Settings → Skills as the page draws it: the toggle with a count on each side, the
    /// search field, Add and the four things it offers, the rows the search leaves, the open
    /// skill's pane, and the Create sheet while it is up.
    fn skills_nodes(&self, mut settings: UiNode) -> UiNode {
        for side in SkillScope::ALL {
            let mut chip = UiNode::button(
                side.element_id(),
                format!("{} {}", side.label(), self.skills_counts.of(side)),
            );
            if side == self.skills_scope {
                chip.states.push("selected".to_string());
            }
            settings = settings.with_child(chip);
        }
        settings = settings
            .with_child(
                UiNode::textbox(ids::SKILLS_SEARCH, "Search skills")
                    .with_value(self.skills_query.clone()),
            )
            .with_child(UiNode::button(ids::SKILL_ADD, "Add"))
            .with_child(UiNode::button(ids::SKILL_UPLOAD, "Upload skill"))
            .with_child(UiNode::button(ids::SKILL_WRITE, "Create a skill"))
            // The two nobody has built are statuses carrying their own sentence, not buttons:
            // what a driver needs from them is the answer, which is the same answer the person
            // reads in the menu.
            .with_child(UiNode::status(ids::SKILL_WITH_BOT, NOT_YET_WITH_BOT))
            .with_child(UiNode::status(ids::SKILL_RECORD, NOT_YET_RECORDING));
        if let Some(error) = &self.skills_error {
            settings = settings.with_child(UiNode::status(ids::SKILLS_ERROR, error.clone()));
        }
        // Saving with no sheet up is an upload, which is the one long thing this page does with
        // nothing else on screen to say it is going.
        if self.skill_saving && !self.skill_add_open {
            settings = settings.with_child(UiNode::status(ids::SKILLS_SAVING, "Uploading…"));
        }
        let rows: Vec<&SkillSnap> = self
            .skills
            .iter()
            .filter(|skill| skill_matches(&skill.name, &skill.description, &self.skills_query))
            .collect();
        if rows.is_empty()
            && let Some(line) = empty_line(
                self.skills_loading,
                self.skills.len(),
                self.skills_scope,
                self.skills_error.is_some(),
            )
        {
            settings = settings.with_child(UiNode::status(ids::SKILLS_EMPTY, line));
        }
        let mut list = UiNode::list("settings-skills-rows", "Skills");
        for skill in &rows {
            list = list.with_child(skill_node(
                skill,
                self.skill_open.as_ref().map(|open| open.id.as_str()) == Some(skill.id.as_str()),
            ));
        }
        settings = settings.with_child(list);
        // The pane stays on the open skill whatever the list is showing: a search that hides
        // its row, or a toggle moved to the other side. It is not read off the rows for exactly
        // that reason.
        if let Some(open) = &self.skill_open {
            settings = settings.with_child(skill_detail_node(open));
        }
        if self.skill_add_open {
            let mut sheet = UiNode::dialog(ids::SKILL_SHEET, "New skill")
                .with_child(UiNode::textbox(ids::SKILL_SHEET_NAME, "Name"))
                .with_child(UiNode::textbox(ids::SKILL_SHEET_DESCRIPTION, "Description"))
                .with_child(UiNode::textbox(ids::SKILL_SHEET_BODY, "Instructions"))
                .with_child(UiNode::button(ids::SKILL_SHEET_CANCEL, "Cancel"))
                .with_child(UiNode::button(
                    ids::SKILL_SHEET_SAVE,
                    // Dead while the create is in flight, and saying so: a Save that is still
                    // working is not a Save that can be pressed again.
                    if self.skill_saving {
                        "Saving…"
                    } else {
                        "Save"
                    },
                ));
            // The sheet's own slot, over the fields it is about. What the list was refused is
            // said in the list's slot, and a driver reading one for the other would read a
            // refusal from a place nobody was shown one.
            if let Some(error) = &self.skill_add_error {
                sheet = sheet.with_child(UiNode::status(ids::SKILL_ADD_ERROR, error.clone()));
            }
            settings = settings.with_child(sheet);
        }
        if let Some(name) = &self.skill_delete_confirm {
            settings = settings.with_child(
                UiNode::dialog(ids::SKILL_DELETE_SHEET, format!("Delete {name}?"))
                    .with_child(UiNode::button(ids::SKILL_DELETE_CONFIRM, "Delete"))
                    .with_child(UiNode::button(ids::SKILL_DELETE_CANCEL, "Cancel")),
            );
        }
        settings
    }

    /// The skill an invoke names, checked against the ones the page has: an id nobody is showing
    /// is a wrong address, and saying so is better than a call going out under it.
    fn skill_id(&self, id: &str) -> Option<String> {
        self.skills
            .iter()
            .find(|skill| skill.id == id)
            .map(|skill| skill.id.clone())
    }

    fn invoke_skill_id(&self, args: &serde_json::Value, invoke: &str) -> Result<String, String> {
        let id = invoke_arg_str(args, &["id", "skill_id", "skillId"])
            .ok_or_else(|| format!("{invoke} requires arg id"))?;
        self.skill_id(&id)
            .ok_or_else(|| format!("no skill `{id}` on Settings → Skills"))
    }

    /// Typing into the search field, which is only there to be typed into while the page it
    /// belongs to is on screen.
    ///
    /// The verb is not held to this: `skills.search` says what the page's search is, the way
    /// `skills.scope` says which side is open, and neither is a claim about a keystroke landing
    /// somewhere. A `type` or a `set_value` IS that claim, and this field is not in the tree for
    /// it to land in until Settings is open on Skills.
    fn skills_search(&mut self, value: String) -> Result<DispatchResult, String> {
        if !(self.account_open && self.skills_tab) {
            return Err(skills_off_screen(ids::SKILLS_SEARCH));
        }
        self.set_skills_query(value)
    }

    /// The search field on Settings → Skills: the host keeps the copy the list filters by.
    fn set_skills_query(&mut self, value: String) -> Result<DispatchResult, String> {
        self.skills_query = value.clone();
        self.pending = Some(Command::SetSkillsQuery(value));
        Ok(DispatchResult::empty())
    }

    /// One of the Skills page's controls, or `None` for a target that is not one.
    ///
    /// The tab itself answers from anywhere in Settings — it is how the page is reached. Every
    /// other control here is refused while the page is not the one on screen: these ids sit in
    /// the tree under a dialog that may be shut, and one of them answering while the person is
    /// on Settings → Updates would be a click nobody could have made.
    fn skill_command(&self, target: &str) -> Option<Result<Command, String>> {
        if target == ids::SETTINGS_SKILLS {
            return Some(Ok(Command::SetAppSettingsTab(AppSettingsTab::Skills)));
        }
        let found = self.skill_target(target)?;
        if !(self.account_open && self.skills_tab) {
            return Some(Err(skills_off_screen(target)));
        }
        Some(found)
    }

    /// What one of the page's controls does, leaving aside whether the page is on screen.
    ///
    /// A ROW IS A SKILL THIS PAGE IS SHOWING, and that is tested before anything else is read
    /// off the target. Stripping `settings-skill-delete-` and calling what is left an id sent
    /// `settings-skill-delete-confirm` looking for a skill called `confirm`, and the delete
    /// dialog could not be answered at all; it did the same to `-delete-sheet`, the dialog's own
    /// container. Membership closes both directions at once.
    ///
    /// An id that is shaped like a row's and names nothing on the page still answers with "no
    /// skill", because that is a wrong address rather than an unknown control — and it is told
    /// apart by the `skl_` the server mints, the way a recipe row's id is (see
    /// [`recipe_row_target`]).
    #[allow(clippy::type_complexity)]
    fn skill_target(&self, target: &str) -> Option<Result<Command, String>> {
        for side in SkillScope::ALL {
            if target == side.element_id() {
                return Some(Ok(Command::SetSkillsScope(side)));
            }
        }
        let row: [(&str, fn(String) -> Command); 4] = [
            ("settings-skill-row-", Command::OpenSkill),
            ("settings-skill-open-", Command::OpenSkill),
            ("settings-skill-detail-delete-", |id| {
                Command::AskSkillDelete { id }
            }),
            ("settings-skill-delete-", |id| Command::AskSkillDelete {
                id,
            }),
        ];
        for (prefix, make) in row {
            if let Some(id) = target.strip_prefix(prefix).and_then(|id| self.skill_id(id)) {
                return Some(Ok(make(id)));
            }
        }
        // The switch, which is the open skill's and nobody else's: it toggles, the way it does
        // under a finger, so what it is now has to be read off the pane rather than guessed.
        if let Some(id) = target.strip_prefix("settings-skill-enabled-") {
            return Some(
                match self.skill_open.as_ref().filter(|open| open.id == id) {
                    Some(open) if !open.arrived => Err(format!(
                        "`{target}` is not on the pane yet: skill `{id}` is still being fetched"
                    )),
                    // One at a time, whichever skill, and in the words the app answers with:
                    // the driver and the person are told the same thing about the same fact.
                    Some(open) if open.switching => Err(SWITCH_IN_FLIGHT.to_string()),
                    Some(open) => Ok(Command::SetSkillEnabled {
                        id: id.to_string(),
                        enabled: !open.enabled,
                    }),
                    None => Err(format!("no skill `{id}` is open to switch")),
                },
            );
        }
        let fixed = match target {
            ids::SKILL_WRITE => Some(Ok(Command::OpenSkillAdd)),
            ids::SKILL_SHEET_CANCEL => Some(Ok(Command::CloseSkillAdd)),
            ids::SKILL_CLOSE => Some(Ok(Command::CloseSkill)),
            ids::SKILL_DELETE_CONFIRM => Some(if self.skill_delete_confirm.is_some() {
                Ok(Command::ConfirmSkillDelete)
            } else {
                Err("no delete is waiting to be answered".to_string())
            }),
            ids::SKILL_DELETE_CANCEL => Some(Ok(Command::CloseSkillDeleteConfirm)),
            ids::SKILL_ADD => Some(Err(
                "`settings-skill-add` opens a menu: click what you want from it                  (`settings-skill-write`), or use `skill.create` / `skill.upload`"
                    .to_string(),
            )),
            ids::SKILL_UPLOAD => Some(Err(
                "`settings-skill-upload` opens a file picker: use `skill.upload --arg path=`                  with a SKILL.md or the folder one lives in"
                    .to_string(),
            )),
            // The sheet's fields are the window's own; the driver hands the values over.
            ids::SKILL_SHEET_SAVE => Some(Err(SHEET_IS_NOT_TYPED_INTO.to_string())),
            ids::SKILL_WITH_BOT => Some(Err(NOT_YET_WITH_BOT.to_string())),
            ids::SKILL_RECORD => Some(Err(NOT_YET_RECORDING.to_string())),
            _ => None,
        };
        if fixed.is_some() {
            return fixed;
        }
        for (prefix, _) in row {
            if let Some(id) = target
                .strip_prefix(prefix)
                .filter(|id| id.starts_with("skl_"))
            {
                return Some(Err(format!("no skill `{id}`")));
            }
        }
        if let Some(id) = target
            .strip_prefix("settings-skill-menu-")
            .filter(|id| id.starts_with("skl_"))
        {
            return Some(Err(format!(
                "`{target}` opens that row's menu: click `{}` to open the skill or `{}` to                  delete it",
                ids::skill_open(id),
                ids::skill_delete(id)
            )));
        }
        None
    }

    /// The two controls a taught skill has, or `None` for a target that is neither.
    ///
    /// Both are refused when there is nothing for them to be about, rather than doing nothing:
    /// a click on a line naming a skill nobody has taught, or a Try again with no refused tape
    /// behind it, is a driver working from a tree it read before something changed.
    fn taught_skill_command(&self, target: &str) -> Option<Result<Command, String>> {
        match target {
            ids::TEACH_SAVED_SKILL => Some(match &self.taught_skill {
                Some(TaughtSkill::Written { id, .. }) => Ok(Command::OpenTaughtSkill {
                    id: Some(id.clone()),
                }),
                _ => Err("no taught skill is waiting to be read".to_string()),
            }),
            ids::TEACH_RETRY => Some(match &self.taught_skill {
                Some(TaughtSkill::Refused {
                    next: AfterRefusal::SendAgain,
                    ..
                }) if self.taught_tape_in_hand => Ok(Command::RetryTaughtSkill),
                // The window that was holding those bytes has been closed, and the sheet went
                // with it. There is nothing left to send.
                Some(TaughtSkill::Refused {
                    next: AfterRefusal::SendAgain,
                    ..
                }) => Err(
                    "the screen window holding that recording has been closed, and the \
                     recording went with it"
                        .to_string(),
                ),
                // Nothing answered, so the skill may already exist. Look before sending.
                Some(TaughtSkill::Refused {
                    next: AfterRefusal::LookFirst,
                    ..
                }) => Err(format!(
                    "nobody knows whether that recording was written: open the library with \
                     `{}` and look before sending it again",
                    ids::TEACH_CHECK_LIBRARY
                )),
                // Refused for a reason a second identical send cannot change: a spend cap, a
                // recording nothing can be written from, a name already taken, or a skill that
                // was written and kept before the failure — where sending again would make a
                // second one.
                Some(TaughtSkill::Refused { why, .. }) => Err(format!(
                    "sending that recording again cannot change the answer: {why}"
                )),
                _ => Err("no refused recording is waiting to be sent again".to_string()),
            }),
            ids::TEACH_CHECK_LIBRARY => Some(match &self.taught_skill {
                Some(TaughtSkill::Refused { .. }) => Ok(Command::OpenTaughtSkill { id: None }),
                _ => Err("nothing is waiting to be looked for".to_string()),
            }),
            _ => None,
        }
    }

    /// The Create sheet's three fields take no keystrokes from here: they are the window's, and
    /// what would be typed into them is what `skill.create` takes as arguments.
    fn skill_sheet_field(&self, target: &str) -> Option<Result<DispatchResult, String>> {
        [
            ids::SKILL_SHEET_NAME,
            ids::SKILL_SHEET_DESCRIPTION,
            ids::SKILL_SHEET_BODY,
        ]
        .contains(&target)
        .then(|| Err(SHEET_IS_NOT_TYPED_INTO.to_string()))
    }

    fn site_login_id(&self, id: &str) -> Option<String> {
        self.site_logins
            .iter()
            .find(|login| login.row.id == id)
            .map(|login| login.row.id.clone())
    }

    fn site_login_delete_target(&self, target: &str) -> Option<String> {
        self.site_login_id(target.strip_prefix("settings-login-delete-")?)
    }

    /// A row's id or one of the sheet's two buttons, from what was clicked.
    fn site_login_command(&self, target: &str) -> Option<Result<Command, String>> {
        if let Some(id) = target.strip_prefix("settings-login-row-") {
            return Some(
                self.site_login_id(id)
                    .map(|id| Command::SelectSiteLogin(Some(id)))
                    .ok_or_else(|| format!("no saved login `{id}`")),
            );
        }
        match target {
            "settings-login-add" => Some(Ok(Command::OpenSiteLoginAdd)),
            "settings-login-add-cancel" => Some(Ok(Command::CloseSiteLoginAdd)),
            // The sheet's fields are the window's own; the driver hands the values over.
            "settings-login-add-save" => Some(Err(
                "`settings-login-add-save` reads the sheet's fields, which the driver cannot \
                 type into: use `logins.add --arg origin= --arg username= --arg password= \
                 [--arg label= --arg notes=]`"
                    .to_string(),
            )),
            "settings-login-import" => Some(Err(
                "`settings-login-import` opens a file picker: use `logins.import --arg path=`"
                    .to_string(),
            )),
            _ => None,
        }
    }

    /// The search field on Settings → Logins: the host keeps the copy the list filters by.
    fn set_site_login_query(&mut self, value: String) -> Result<DispatchResult, String> {
        self.site_login_query = value.clone();
        self.pending = Some(Command::SetSiteLoginQuery(value));
        Ok(DispatchResult::empty())
    }

    fn set_site_login_notes(
        &mut self,
        target: &str,
        value: &str,
    ) -> Option<Result<DispatchResult, String>> {
        let id = target.strip_prefix("settings-login-notes-")?;
        Some(match self.site_login_id(id) {
            Some(id) => {
                self.pending = Some(Command::SetSiteLoginNotes {
                    id,
                    notes: value.to_string(),
                });
                Ok(DispatchResult::empty())
            }
            None => Err(format!("no saved login `{id}`")),
        })
    }

    fn user_form_field(&self, target: &str) -> Option<(String, String, UserFormFieldKind, String)> {
        for form in &self.user_forms {
            for field in &form.fields {
                if target == user_form_field_id(&form.card_key, &field.id) {
                    return Some((
                        form.card_key.clone(),
                        field.id.clone(),
                        field.kind,
                        field.value.clone(),
                    ));
                }
            }
        }
        None
    }

    fn set_user_form_field(
        &mut self,
        card_key: String,
        field_id: String,
        value: String,
    ) -> Result<DispatchResult, String> {
        if let Some(form) = self
            .user_forms
            .iter_mut()
            .find(|form| form.card_key == card_key)
            && let Some(field) = form.fields.iter_mut().find(|field| field.id == field_id)
        {
            field.value = value.clone();
        }
        self.pending = Some(Command::UserFormSetField {
            card_key,
            field_id,
            value,
        });
        Ok(DispatchResult::empty())
    }

    /// `recipe-accept` / `recipe-decline` (the one pending share) or the same with `-<id>`.
    fn recipe_answer_target(&self, target: &str) -> Option<(String, bool)> {
        let (rest, accept) = if let Some(rest) = target.strip_prefix("recipe-accept") {
            (rest, true)
        } else {
            (target.strip_prefix("recipe-decline")?, false)
        };
        let id = match rest.strip_prefix('-') {
            Some(id) if !id.is_empty() => id.to_string(),
            Some(_) => return None,
            None if rest.is_empty() => {
                let mut pending = self.recipes.iter().filter(|recipe| recipe.pending);
                let first = pending.next()?;
                if pending.next().is_some() {
                    return None;
                }
                first.id.clone()
            }
            None => return None,
        };
        Some((id, accept))
    }

    /// Remove on one of this Mac's rules, or `None` for a target that is not one.
    ///
    /// The rule is the one at that place on the page as it is now, and the command sent is
    /// that row's own. Refused while the page is not on screen, for a place with no rule on
    /// it, and for a rule already on its way off: its button is dead, and a second press would
    /// be a second request about one rule.
    fn local_rule_command(&self, target: &str) -> Option<Result<Command, String>> {
        let rest = target.strip_prefix("settings-local-rule-remove-")?;
        let (kind, n) = RuleKind::ALL.into_iter().find_map(|kind| {
            let n = rest.strip_prefix(kind.word())?.strip_prefix('-')?;
            n.parse::<usize>().ok().map(|n| (kind, n))
        })?;
        if !(self.account_open && self.computer_tab) {
            return Some(Err(format!(
                "`{target}` is on Settings → Computer, which is not what is on screen: open it \
                 with `settings-tab-computer`"
            )));
        }
        let Some(rules) = &self.local_rules else {
            return Some(Err(format!(
                "`{target}` is not on screen: this Mac's rules have not been read"
            )));
        };
        let rows = rules.rows(kind);
        let Some(row) = rows.get(n) else {
            return Some(Err(format!(
                "no rule `{target}`: {} lists {}",
                rule_list_title(kind),
                rows.len()
            )));
        };
        if rules.is_removing(kind, &row.pattern) {
            return Some(Err(format!("`{}` is already being removed", row.pattern)));
        }
        Some(Ok(Command::RemoveLocalRule {
            kind,
            pattern: row.pattern.clone(),
        }))
    }

    /// Whether the Tools card has anything to show when it is opened: switches, or the read-only
    /// list of what the next turn is offered where the ceiling could not be read.
    fn tools_card_has_rows(&self) -> bool {
        self.agent_ceiling
            .as_ref()
            .is_some_and(|card| !shown_ceiling_rows(card).is_empty())
            || !offered_without_switches(self.agent_ceiling.as_ref(), self.agent_tools.as_ref())
                .is_empty()
    }

    /// A click on `agent-ceiling-switch-{name}`: that row's switch, moved the other way, which is
    /// what a person's click does. Only where a person could click it — the settings open, the
    /// card open to its rows — and only while the screen draws it live: not while the card is
    /// blocked (a switch with the server, whichever Bot's; a read of this Bot's ceiling; the
    /// server's 403), and never to switch back on a plugin the server no longer loads. Each of
    /// those says which it was, and a card with no switches says why it has none.
    fn ceiling_command(&self, target: &str) -> Result<Command, String> {
        if !self.agent_settings_open {
            return Err(format!(
                "`{target}` is in the bot's settings, which are closed"
            ));
        }
        let Some(name) = ids::ceiling_switch_row(target) else {
            return Err(if ids::is_ceiling_line(target) {
                format!("`{target}` is a line on the Tools card, not a switch")
            } else {
                format!("no switch `{target}` on this Bot's Tools card")
            });
        };
        let Some(card) = &self.agent_ceiling else {
            return Err(format!(
                "`{target}` is not on screen: this Bot's tools have not been asked for"
            ));
        };
        let read = match &card.ceiling {
            ToolCeiling::Read(read) => read,
            ToolCeiling::Loading => {
                return Err(format!(
                    "`{target}` is not on screen yet: this Bot's tools are still being read"
                ));
            }
            ToolCeiling::Unavailable(why) => {
                return Err(format!(
                    "`{target}` is not on screen: the Tools card has no switches, because: {why}"
                ));
            }
        };
        let rows = shown_ceiling_rows(card);
        let Some(row) = rows.iter().find(|row| row.name == name) else {
            return Err(format!("no switch `{target}` on this Bot's Tools card"));
        };
        if !self.agent_tools_open {
            return Err(format!(
                "`{target}` is on the Tools card, which is shut: open it with \
                 `agent-tools-toggle`"
            ));
        }
        let enabled = !row.on;
        read.may_switch(&row.name, enabled, card.blocked.as_ref())
            .map_err(|why| {
                format!(
                    "`{target}` cannot be switched {}: {why}",
                    if enabled { "on" } else { "off" }
                )
            })?;
        Ok(Command::SetCeilingTool {
            name: row.name.clone(),
            enabled,
        })
    }

    /// Whether the Skills card has any skill to show when it is opened.
    fn skills_card_has_rows(&self) -> bool {
        self.agent_skills
            .as_ref()
            .is_some_and(|card| !shown_skill_rows(card).is_empty())
    }

    /// A click on `agent-skills-switch-{id}`: that skill attached or detached, the other way from
    /// where its switch stands, which is what a person's click does. Only where a person could
    /// click it — the settings open, the card open to its skills — and only while the screen draws
    /// it live: not while the card is blocked (a skill switch with the server, whichever Bot's; a
    /// read of this Bot's skills; the server's 403), and never to attach a skill switched off in
    /// Settings → Skills, which can only be detached. Each of those says which it was, and a card
    /// with no switches says why it has none.
    fn skills_command(&self, target: &str) -> Result<Command, String> {
        if !self.agent_settings_open {
            return Err(format!(
                "`{target}` is in the bot's settings, which are closed"
            ));
        }
        let Some(skill_id) = ids::skill_switch_row(target) else {
            return Err(if ids::is_skills_line(target) {
                format!("`{target}` is a line on the Skills card, not a switch")
            } else {
                format!("no switch `{target}` on this Bot's Skills card")
            });
        };
        let Some(card) = &self.agent_skills else {
            return Err(format!(
                "`{target}` is not on screen: this Bot's skills have not been asked for"
            ));
        };
        let read = match &card.skills {
            BotSkills::Read(read) => read,
            BotSkills::Loading => {
                return Err(format!(
                    "`{target}` is not on screen yet: this Bot's skills are still being read"
                ));
            }
            BotSkills::Unavailable(why) => {
                return Err(format!(
                    "`{target}` is not on screen: the Skills card has no switches, because: {why}"
                ));
            }
        };
        let rows = shown_skill_rows(card);
        let Some(row) = rows.iter().find(|row| row.id == skill_id) else {
            return Err(format!("no switch `{target}` on this Bot's Skills card"));
        };
        if !self.agent_skills_open {
            return Err(format!(
                "`{target}` is on the Skills card, which is shut: open it with \
                 `{}`",
                ids::AGENT_SKILLS_TOGGLE
            ));
        }
        let attached = !row.on;
        read.may_switch(&row.id, attached, card.blocked.as_ref())
            .map_err(|why| {
                format!(
                    "`{target}` cannot be {}: {why}",
                    if attached { "attached" } else { "detached" }
                )
            })?;
        Ok(Command::SetBotSkill {
            skill_id: row.id.clone(),
            attached,
        })
    }

    fn sidebar_toggle_node(&self) -> UiNode {
        UiNode::button(ids::NAV_TOGGLE, "Hide sidebar")
    }

    fn click(&mut self, target: &str) -> Result<DispatchResult, String> {
        let cmd = if target == ids::NAV_NEW_CHAT || target == "create-first-bot" {
            Command::NewChat
        } else if target == ids::HEADER_LEFT_SIDEBAR {
            if !self.sidebar_hidden {
                return Err("the sidebar is already visible".into());
            }
            Command::ToggleSidebar
        } else if target == ids::NAV_TOGGLE {
            if self.sidebar_hidden {
                return Err("the sidebar control is hidden".into());
            }
            Command::ToggleSidebar
        } else if target == ids::HEADER_MONITOR {
            Command::ToggleComputerPane
        } else if target == ids::HEADER_COWORKER {
            if !self.sessions.iter().any(|session| session.active) {
                return Err("the Bot chip is there only while a Bot is open".into());
            }
            Command::PressBotChip
        } else if target == ids::HEADER_RIGHT_SIDEBAR {
            if self.computer_open {
                Command::ToggleComputerPane
            } else {
                Command::ToggleAgentSettings
            }
        } else if target == ids::FOOTER_THEME {
            Command::ToggleTheme
        } else if target == ids::FOOTER_ACCOUNT {
            Command::ToggleAccount
        } else if target == ids::HEADER_SETTINGS || target == ids::AGENT_SETTINGS {
            Command::ToggleAgentSettings
        } else if target == "agent-tools-toggle" {
            if !self.agent_settings_open {
                return Err(
                    "`agent-tools-toggle` is in the bot's settings, which are closed".into(),
                );
            }
            if !self.tools_card_has_rows() {
                return Err(
                    "`agent-tools-toggle` is only there while the bot has tools to show".into(),
                );
            }
            Command::ToggleAgentTools
        } else if target.starts_with(ids::AGENT_CEILING) {
            self.ceiling_command(target)?
        } else if target == ids::AGENT_SKILLS_TOGGLE {
            if !self.agent_settings_open {
                return Err(format!(
                    "`{target}` is in the bot's settings, which are closed"
                ));
            }
            if !self.skills_card_has_rows() {
                return Err(format!(
                    "`{target}` is only there while the Bot has skills to show"
                ));
            }
            Command::ToggleAgentSkills
        } else if target.starts_with(ids::AGENT_SKILLS) {
            self.skills_command(target)?
        } else if target == "agent-usage-toggle" {
            if !self.agent_settings_open {
                return Err(
                    "`agent-usage-toggle` is in the bot's settings, which are closed".into(),
                );
            }
            if !matches!(&self.agent_usage, Some(crate::state::UsageReport::Read(read)) if !read.models.is_empty())
            {
                return Err(
                    "`agent-usage-toggle` is only there while the server reported models".into(),
                );
            }
            Command::ToggleAgentUsage
        } else if target == ids::AGENT_SAVE {
            if !self.agent_settings_open {
                return Err("`agent-save` is in the bot's settings, which are closed".into());
            }
            Command::SaveAgentSettings
        } else if target == "agent-model-dismiss" {
            Command::SetModelPicker(false)
        } else if target == "avatar-trigger" || target == "avatar-editor-dismiss" {
            Command::ToggleAvatarEditor
        } else if target == ids::LOGIN_SUBMIT {
            Command::Login {
                email: self.login_email.clone(),
                password: self.login_password.clone(),
            }
        } else if target == ids::FOOTER_SIGN_OUT {
            Command::Logout
        } else if target == "signed-out-sign-in" {
            if self.signed_out.is_none() {
                return Err(
                    "there is nothing to sign in again for: the app has a session".to_string(),
                );
            }
            Command::SignInAgain
        } else if target == ids::NAV_SEARCH
            || target == ids::NAV_LIBRARY
            || target == ids::NAV_PROJECTS
        {
            return Ok(DispatchResult::empty());
        } else if target == ids::NAV_RECIPES {
            Command::OpenRecipes
        } else if target == ids::RECIPE_RUN {
            let open = self
                .recipe_detail
                .as_ref()
                .ok_or_else(|| "no recipe is open".to_string())?;
            let bot = open
                .run_bot
                .clone()
                .filter(|_| open.runnable)
                .ok_or_else(|| {
                    "Run is not on offer: no bot, no version to play, or the page is busy"
                        .to_string()
                })?;
            Command::RunOpenRecipe(bot)
        } else if let Some(command) = self.computer_exec_command(target) {
            command
        } else if target == "recipe-back" {
            Command::CloseRecipe
        } else if let Some(word) = target.strip_prefix("recipes-filter-") {
            Command::SetRecipesFilter(
                crate::state::RecipeFilter::from_query(word)
                    .ok_or_else(|| format!("unknown recipes filter `{word}`"))?,
            )
        } else if let Some((id, accept)) = self.recipe_answer_target(target) {
            Command::AnswerRecipeShare { id, accept }
        } else if let Some(id) = recipe_row_target(target) {
            Command::OpenRecipe(id)
        } else if let Some(id) = target.strip_prefix("coworker-") {
            Command::SelectCoworker(id.to_string())
        } else if let Some(id) = target.strip_prefix("session-") {
            Command::SelectSession(id.to_string())
        } else if let Some((call_id, resolution)) = approval_target(target) {
            Command::AnswerApproval {
                call_id,
                resolution,
            }
        } else if let Some(index) = thumb_target(target) {
            if index >= self.thumbs.len() {
                return Err(format!(
                    "no picture `{target}` in the newest set ({} there)",
                    self.thumbs.len()
                ));
            }
            Command::OpenLightbox { index }
        } else if target == "retry-turn" {
            if !self.can_retry_turn {
                return Err(
                    "there is no turn to send again: the open thread's last turn is not one \
                     that failed to go out"
                        .to_string(),
                );
            }
            Command::RetryTurn
        } else if target == ids::RUN_ERROR_SEND_ON_SERVER {
            if !self.can_send_on_server {
                return Err(
                    "there is no turn to send on the server's keys: the open thread's last turn \
                     is not one the person's plan could not answer"
                        .to_string(),
                );
            }
            Command::SendOnServer
        } else if let Some(cmd) = self.reply_run_command(target) {
            cmd?
        } else if let Some(cmd) = self.computer_card_command(target) {
            cmd?
        } else if let Some(cmd) = self.default_models_command(target) {
            cmd?
        } else if let Some(cmd) = self.model_picker_command(target) {
            cmd?
        } else if target == ids::COMPOSER_SEND {
            if !self.turn_in_flight {
                // Sending belongs to the keyboard like the rest of the composer: `key composer
                // Enter` runs what a person's Enter runs, chips and recipe and all. The button
                // is a click target only while it is the stop button, which no key is bound to.
                return Err(format!(
                    "`{target}` reads \"Send message\": there is no turn to stop. Send with \
                     `key composer Enter`; this is clicked while it reads \"Stop\"."
                ));
            }
            Command::StopTurn
        } else if target.starts_with("composer-") {
            // The composer's panel and its bar are worked from the keyboard, the way a person
            // works them, because that is the only path that runs what the composer runs. Say
            // so rather than let this fall through to "unknown target": these ids are in the
            // tree now, so a driver will reasonably try to click one.
            return Err(format!(
                "`{target}` is not clicked: the composer is worked from the keyboard \
                 (`type composer /`, then `type \"\" <words>` to filter and `key \"\" Enter` \
                 to take the row)"
            ));
        } else if let Some(cmd) = self.choice_command(target) {
            cmd
        } else if let Some(cmd) = self.user_form_command(target) {
            cmd
        } else if let Some(cmd) = self.computer_handoff_command(target) {
            cmd
        } else if let Some(cmd) = self.save_login_command(target) {
            cmd
        } else if let Some(cmd) = self.skill_command(target) {
            cmd?
        } else if let Some(cmd) = self.taught_skill_command(target) {
            cmd?
        } else if let Some(cmd) = self.local_rule_command(target) {
            cmd?
        } else if let Some(cmd) = self.connection_command(target) {
            cmd?
        } else if target == "settings-tab-logins" {
            Command::SetAppSettingsTab(AppSettingsTab::Logins)
        } else if target == "settings-tab-computer" {
            Command::SetAppSettingsTab(AppSettingsTab::Computer)
        } else if target == "settings-tab-updates" {
            Command::SetAppSettingsTab(AppSettingsTab::Updates)
        } else if target == "app-settings-back" {
            Command::CloseAppSettings
        } else if target == ids::COMPUTER_GET {
            if !self.computer_open || self.computer_error.is_none() {
                return Err(format!(
                    "`{target}` is on the Computer pane only while it says why the bot has no \
                     computer"
                ));
            }
            if self.computer_asking {
                return Err(format!(
                    "`{target}` waits while the server is being asked for a computer"
                ));
            }
            Command::GetComputer
        } else if target == "computer-update" || target == "settings-computer-update" {
            Command::OpenComputerConfirm(crate::state::ComputerAction::Update)
        } else if target == "computer-reset" {
            Command::OpenComputerConfirm(crate::state::ComputerAction::Reset)
        } else if target == "route-traffic-this-computer" || target == "egress-tunnel-enabled" {
            Command::SetEgressTunnelEnabled(!self.egress_tunnel_enabled)
        } else if let Some(mode) = target
            .strip_prefix("egress-policy-")
            .and_then(crate::opengrok::LocalExecMode::parse)
        {
            let on_screen = self.egress_policy.is_some()
                && ((self.network_policy_open && self.route_traffic_on_bot_pane)
                    || (self.account_open
                        && self.computer_tab
                        && self.route_traffic_in_user_settings));
            if !on_screen {
                return Err("no network choice is on screen to click".to_string());
            }
            Command::SetEgressPolicy(mode)
        } else if target == "network-policy" {
            if !(self.computer_open
                && self.route_traffic_on_bot_pane
                && self.egress_policy.is_some())
            {
                return Err("no network badge is on screen to click".to_string());
            }
            Command::OpenNetworkPolicy
        } else if target == "network-policy-close" {
            Command::CloseNetworkPolicy
        } else if target == ids::ROUTINE_NEW {
            Command::OpenRoutineEditor(None)
        } else if target == ids::ROUTINE_DELETE_CONFIRM || target == ids::ROUTINE_DELETE_CANCEL {
            if self.routine_delete_question.is_none() {
                return Err(format!(
                    "`{target}` answers Delete's question, which is not asked"
                ));
            }
            if target == ids::ROUTINE_DELETE_CONFIRM {
                Command::ConfirmRoutineDelete
            } else {
                Command::CancelRoutineDelete
            }
        } else if let Some(cmd) = self.wake_command(target) {
            cmd?
        } else if [
            ids::ROUTINE_HISTORY_TOGGLE,
            ids::ROUTINE_OPEN_THREAD,
            ids::ROUTINE_RUN_NOW,
            ids::ROUTINE_DELETE,
        ]
        .contains(&target)
        {
            self.routine_header_command(target)?
        } else if let Some(why) = self.skipped_line_refusal(target) {
            return Err(why);
        } else if let Some(cmd) = self.routine_command(target) {
            if let Command::RunRoutineNow { routine_id } = &cmd {
                self.refuse_dead_test_run(target, routine_id)?;
            }
            cmd
        } else if let Some(id) = self.site_login_delete_target(target) {
            Command::DeleteSiteLogin { id }
        } else if let Some(cmd) = self.site_login_command(target) {
            cmd?
        } else if let Some((card_key, field_id, kind, value)) = self.user_form_field(target) {
            if kind == UserFormFieldKind::Checkbox {
                let next = if value == "true" { "false" } else { "true" };
                return self.set_user_form_field(card_key, field_id, next.to_string());
            }
            return Err(format!(
                "`{target}` is a user-form field: use set_value, not click"
            ));
        } else {
            return Err(format!("unknown click target `{target}`"));
        };
        self.pending = Some(cmd);
        Ok(DispatchResult::empty())
    }

    /// Replace what is in a field.
    ///
    /// On the composer that is select-all, delete, then type it — the three things a person
    /// does — so a value beginning with `/` or `@` opens the panel exactly as typing it would.
    /// A set-value that wrote the draft behind the field's back would leave the panel shut.
    fn set_value(&mut self, target: &str, value: &str) -> Result<DispatchResult, String> {
        if let Some(field) = login_field(target) {
            return self.set_login(field, value.to_string());
        }
        if let Some((card_key, field_id, _, _)) = self.user_form_field(target) {
            return self.set_user_form_field(card_key, field_id, value.to_string());
        }
        if target == "settings-logins-search" {
            return self.set_site_login_query(value.to_string());
        }
        if target == ids::SKILLS_SEARCH {
            return self.skills_search(value.to_string());
        }
        if target == ids::RELAY_ADDR || target == ids::RELAY_KEY {
            return self.set_relay_field(target, value);
        }
        if let Some(which) = picker_field(target, |ids| ids.effort) {
            return self.set_picker_effort(which, target, value);
        }
        if let Some(which) = picker_field(target, |ids| ids.search) {
            return self.set_picker_search(which, target, value);
        }
        if let Some(which) = [
            crate::state::WakeBox::Every,
            crate::state::WakeBox::Hour,
            crate::state::WakeBox::Minute,
            crate::state::WakeBox::Cron,
        ]
        .into_iter()
        .find(|which| target == ids::routine_wake_box(*which))
        {
            if self.routine_wake_editor.is_none() {
                return Err(format!("`{target}` is in the wake editor, which is shut"));
            }
            self.pending = Some(Command::SetWakeBox {
                which,
                text: value.to_string(),
            });
            return Ok(DispatchResult::empty());
        }
        if let Some(refusal) = self.skill_sheet_field(target) {
            return refusal;
        }
        if let Some(result) = self.set_site_login_notes(target, value) {
            return result;
        }
        // The target is read before the text, here and in the two below: a wrong address is
        // worth saying before anything about what was going to be typed into it.
        let plan = compose_plan(target, Vec::new())?;
        let mut keys = vec![select_all_chord().to_string(), "backspace".to_string()];
        keys.extend(text_tokens(value)?);
        self.plan(ComposePlan { keys, ..plan })
    }

    /// Add text to the end of what a field holds, one keystroke per character.
    fn type_into(&mut self, target: &str, text: &str) -> Result<DispatchResult, String> {
        if let Some(field) = login_field(target) {
            let value = match field {
                LoginField::Email => format!("{}{text}", self.login_email),
                LoginField::Password => format!("{}{text}", self.login_password),
            };
            return self.set_login(field, value);
        }
        if let Some((card_key, field_id, _, current)) = self.user_form_field(target) {
            return self.set_user_form_field(card_key, field_id, format!("{current}{text}"));
        }
        if target == "settings-logins-search" {
            return self.set_site_login_query(format!("{}{text}", self.site_login_query));
        }
        if target == ids::SKILLS_SEARCH {
            return self.skills_search(format!("{}{text}", self.skills_query));
        }
        if target == ids::RELAY_ADDR {
            let address = format!("{}{text}", self.reply_source.relay.address);
            return self.set_relay_field(target, &address);
        }
        if target == ids::RELAY_KEY {
            return Err(self.relay_field_keys(target));
        }
        if let Some(which) = picker_field(target, |ids| ids.search) {
            let query = format!("{}{text}", self.picker_snap(which).1.search);
            return self.set_picker_search(which, target, &query);
        }
        if let Some(refusal) = self.skill_sheet_field(target) {
            return refusal;
        }
        let plan = compose_plan(target, Vec::new())?;
        self.plan(ComposePlan {
            keys: text_tokens(text)?,
            ..plan
        })
    }

    /// Press one key. No modifiers: a chord is `op keybinding`'s business, not this one's.
    fn key(&mut self, target: &str, key: &str) -> Result<DispatchResult, String> {
        if let Some(field) = login_field(target) {
            return self.login_key(field, target, key);
        }
        if let Some(refusal) = self.skill_sheet_field(target) {
            return refusal;
        }
        if target == ids::RELAY_ADDR || target == ids::RELAY_KEY {
            return Err(self.relay_field_keys(target));
        }
        if target == ids::SKILLS_SEARCH {
            return match key_token(key)?.as_str() {
                "backspace" => {
                    let mut value = self.skills_query.clone();
                    value.pop();
                    self.skills_search(value)
                }
                // The list filters as the text changes; Enter has nothing left to do — but a
                // key pressed at a field that is not on screen did not land, and saying it did
                // would be the same lie as taking the text.
                "enter" if self.account_open && self.skills_tab => Ok(DispatchResult::empty()),
                "enter" => Err(skills_off_screen(target)),
                other => Err(format!(
                    "unhandled key `{other}` on `{target}` (Enter, Backspace)"
                )),
            };
        }
        if let Some(which) = picker_field(target, |ids| ids.search) {
            if let Some(closed) = self.picker_search_closed(which, target) {
                return Err(closed);
            }
            return match key_token(key)?.as_str() {
                "backspace" => {
                    let mut query = self.picker_snap(which).1.search.clone();
                    query.pop();
                    self.set_picker_search(which, target, &query)
                }
                // The list filters as the text changes; Enter has nothing left to do.
                "enter" => Ok(DispatchResult::empty()),
                other => Err(format!(
                    "unhandled key `{other}` on `{target}` (Enter, Backspace)"
                )),
            };
        }
        if target == "settings-logins-search" {
            return match key_token(key)?.as_str() {
                "backspace" => {
                    let mut value = self.site_login_query.clone();
                    value.pop();
                    self.set_site_login_query(value)
                }
                // The list filters as the text changes; Enter has nothing left to do.
                "enter" => Ok(DispatchResult::empty()),
                other => Err(format!(
                    "unhandled key `{other}` on `{target}` (Enter, Backspace)"
                )),
            };
        }
        if let Some((card_key, field_id, _, current)) = self.user_form_field(target) {
            match key_token(key)?.as_str() {
                "enter" => {
                    self.pending = Some(Command::UserFormContinue { card_key });
                    return Ok(DispatchResult::empty());
                }
                "backspace" => {
                    let mut value = current;
                    value.pop();
                    return self.set_user_form_field(card_key, field_id, value);
                }
                other => {
                    return Err(format!(
                        "unhandled key `{other}` on `{target}` (Enter, Backspace)"
                    ));
                }
            }
        }
        // A letter pressed at a choice card is the letter on its keycap. It goes to the window
        // the way a person's does: the caret leaves the composer (a click on the card does
        // that) and the key goes down the window's own path, where the root answers the card
        // a letter answers. That is only ever one card, so a key aimed at any other is refused
        // rather than pressed: pressed, it would answer the other card.
        if let Some(choice) = self
            .choices
            .iter()
            .find(|choice| choice_card_id(&choice.message_id) == target)
        {
            if !choice.keyed {
                return Err(format!(
                    "`{target}` takes no letter: a letter answers only the newest open card \
                     with one question"
                ));
            }
            return self.plan(ComposePlan {
                focus_composer: false,
                release_caret: true,
                keys: vec![key_token(key)?],
            });
        }
        let plan = compose_plan(target, Vec::new())?;
        self.plan(ComposePlan {
            keys: vec![key_token(key)?],
            ..plan
        })
    }

    /// Hold the keys for the window and tell the driver what was planned.
    fn plan(&mut self, plan: ComposePlan) -> Result<DispatchResult, String> {
        let result = DispatchResult::json(serde_json::json!({
            "target": if plan.focus_composer { ids::COMPOSER } else { "focused" },
            "keys": plan.keys,
            "path": "gpui.dispatch_keystroke",
        }));
        self.compose = Some(plan);
        Ok(result)
    }

    /// Write one of the login drafts. The page draws its own fields; this is the copy the rest
    /// of the app reads, and the one `click login-submit` signs in with.
    fn set_login(&mut self, field: LoginField, value: String) -> Result<DispatchResult, String> {
        let (email, password) = match field {
            LoginField::Email => {
                self.login_email = value.clone();
                (Some(value), None)
            }
            LoginField::Password => {
                self.login_password = value.clone();
                (None, Some(value))
            }
        };
        self.pending = Some(Command::SetLoginDraft { email, password });
        Ok(DispatchResult::empty())
    }

    fn login_key(
        &mut self,
        field: LoginField,
        target: &str,
        key: &str,
    ) -> Result<DispatchResult, String> {
        match key_token(key)?.as_str() {
            // What the page does with Enter on either field: it tries to sign in.
            "enter" => {
                self.pending = Some(Command::Login {
                    email: self.login_email.clone(),
                    password: self.login_password.clone(),
                });
                Ok(DispatchResult::empty())
            }
            "backspace" => {
                let mut value = match field {
                    LoginField::Email => self.login_email.clone(),
                    LoginField::Password => self.login_password.clone(),
                };
                value.pop();
                self.set_login(field, value)
            }
            other => Err(format!(
                "unhandled key `{other}` on `{target}` (Enter, Backspace)"
            )),
        }
    }

    fn invoke_user_form_card_key(&self, args: &serde_json::Value) -> Result<String, String> {
        if let Some(key) = invoke_arg_str(args, &["card_key", "cardKey", "id"]) {
            if self.user_forms.iter().any(|form| form.card_key == key) {
                return Ok(key);
            }
            return Err(format!("no user-form `{key}`"));
        }
        let idle: Vec<&UserFormSnap> = self
            .user_forms
            .iter()
            .filter(|form| form.pill.is_none())
            .collect();
        match idle.as_slice() {
            [one] => Ok(one.card_key.clone()),
            [] => Err("no idle user-form".into()),
            _ => Err("user-form invoke requires arg card_key".into()),
        }
    }

    /// A button under one connected computer's local-exec mode.
    fn computer_exec_command(&self, target: &str) -> Option<Command> {
        use crate::opengrok::LocalExecMode;
        // Only what is on screen: the buttons are in the tree while Settings is open on
        // Computer, and a click on one that is not there is a wrong address.
        if !(self.account_open && self.computer_tab) {
            return None;
        }
        self.computers.iter().find_map(|card| {
            let machine = &card.machine_id;
            [
                LocalExecMode::Always,
                LocalExecMode::Ask,
                LocalExecMode::Never,
            ]
            .into_iter()
            .find(|mode| target == ids::computer_exec_mode(machine, *mode))
            .map(|mode| Command::SetComputerExecMode {
                machine_id: machine.clone(),
                mode,
            })
        })
    }

    /// Why a click on a skipped line of a routine's Run history does nothing, or `None` for a
    /// target that is not one: the firing started no run, so there is no thread to open at it.
    fn skipped_line_refusal(&self, target: &str) -> Option<String> {
        let rest = target.strip_prefix("routine-")?;
        self.routines.iter().find_map(|routine| {
            let at = rest
                .strip_prefix(routine.id.as_str())?
                .strip_prefix("-skipped-")?;
            routine
                .runs
                .iter()
                .any(|line| {
                    matches!(line, RunLineSnap::Skipped { at_ms, .. } if at_ms.to_string() == at)
                })
                .then(|| {
                    format!(
                        "`{target}` is a firing the server skipped: it started no run, so there \
                         is nothing to open"
                    )
                })
        })
    }

    /// One of a routine's controls, or `None` for a target that is not a routine's at all.
    ///
    /// The id is the server's and can hold anything, dashes included, so this reads the tail
    /// first and takes what is left as the id — and then only if that id is a routine the open
    /// bot has. An id nobody is showing is a wrong address, not a click.
    #[allow(clippy::type_complexity)]
    fn routine_command(&self, target: &str) -> Option<Command> {
        let rest = target.strip_prefix("routine-")?;
        // A line of a routine's history opens its thread, as it does in the editor. Matched
        // against the routines and runs on screen, because both ids can hold dashes and no
        // tail can tell where one ends.
        for routine in &self.routines {
            if let Some(run) = rest
                .strip_prefix(routine.id.as_str())
                .and_then(|tail| tail.strip_prefix("-run-"))
                && routine
                    .runs
                    .iter()
                    .any(|line| matches!(line, RunLineSnap::Ran { run_id, .. } if run_id == run))
            {
                return Some(Command::OpenRoutineRun {
                    routine_id: routine.id.clone(),
                    run_id: run.to_string(),
                });
            }
        }
        // The switch asks for the other way from where it stands, so it is read off the row
        // and not off the id alone.
        if let Some(routine) = rest
            .strip_suffix("-active")
            .and_then(|id| self.routines.iter().find(|routine| routine.id == id))
        {
            return Some(Command::SetRoutineActive {
                routine_id: routine.id.clone(),
                active: !routine.active,
            });
        }
        let mut cmd: Option<(&str, fn(String) -> Command)> = None;
        for (tail, make) in [
            (
                "-trigger-schedule",
                (|id| Command::AddRoutineTrigger {
                    routine_id: id,
                    // The menu's own first offer, so a driver that asks for "a schedule" gets
                    // the one a person clicking the same row would get.
                    trigger: crate::state::NewTrigger::Schedule(
                        crate::state::ScheduleSpec::from_preset("Every day"),
                    ),
                }) as fn(String) -> Command,
            ),
            (
                "-trigger-webhook",
                (|id| Command::AddRoutineTrigger {
                    routine_id: id,
                    trigger: crate::state::NewTrigger::Webhook,
                }) as fn(String) -> Command,
            ),
            (
                "-rotate",
                (|id| Command::RotateRoutineWebhook { routine_id: id }) as fn(String) -> Command,
            ),
            (
                "-test",
                (|id| Command::RunRoutineNow { routine_id: id }) as fn(String) -> Command,
            ),
            (
                "-thread",
                (|id| Command::OpenRoutineThread { routine_id: id }) as fn(String) -> Command,
            ),
            (
                "-delete",
                (|id| Command::AskDeleteRoutine { routine_id: id }) as fn(String) -> Command,
            ),
        ] {
            if let Some(id) = rest.strip_suffix(tail) {
                cmd = Some((id, make));
                break;
            }
        }
        let (id, make) = match cmd {
            Some((id, make)) => (id, Some(make)),
            None => (rest, None),
        };
        if !self.routines.iter().any(|routine| routine.id == id) {
            return None;
        }
        Some(match make {
            Some(make) => make(id.to_string()),
            None => Command::OpenRoutineEditor(Some(id.to_string())),
        })
    }

    fn invoke(&mut self, name: &str, args: &serde_json::Value) -> Result<DispatchResult, String> {
        let cmd = match name {
            "chat.new" => Command::NewChat,
            "sidebar.toggle" => Command::ToggleSidebar,
            "sidebar.mini" => Command::ToggleMiniSidebar,
            // The Model card's popover in the Bot's settings, as these once opened its model list.
            "model.picker" => Command::ToggleModelPicker,
            "model.picker.open" => Command::SetModelPicker(true),
            "model.picker.close" => Command::SetModelPicker(false),
            "avatar.editor" => Command::ToggleAvatarEditor,
            "avatar.editor.open" => Command::SetAvatarEditor(true),
            "avatar.editor.close" => Command::SetAvatarEditor(false),
            "avatar.color" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "avatar.color requires arg id".to_string())?
                    .to_string();
                Command::SetAvatarColor(id)
            }
            "theme.toggle" => Command::ToggleTheme,
            "composer.attach" => {
                let path = std::path::PathBuf::from(
                    invoke_arg_str(args, &["path"]).ok_or("composer.attach requires arg path")?,
                );
                // Said here rather than only on the composer's notice, which is not on the tree:
                // a driver learns at once that the path is not one the app can attach.
                if !path.is_absolute() {
                    return Err(format!(
                        "composer.attach needs an absolute path: {} would resolve against the app's \
                         directory, not yours",
                        path.display()
                    ));
                }
                if crate::components::chat_input::file_mime(&path).is_none() {
                    return Err(format!(
                        "{} cannot be attached: images, videos, PDFs and text files can",
                        path.display()
                    ));
                }
                Command::AttachFile(path)
            }
            "composer.detach" => {
                let index = invoke_arg_str(args, &["index"])
                    .and_then(|index| index.parse::<usize>().ok())
                    .ok_or("composer.detach requires arg index (the N of composer-file-N)")?;
                if index >= self.composer_files.len() {
                    return Err(format!("no composer-file-{index} on the draft"));
                }
                Command::DetachFile(index)
            }
            "computer.toggle" => Command::ToggleComputerPane,
            "computer.open" => Command::OpenCoworkerScreen,
            "computer.update" => Command::OpenComputerConfirm(crate::state::ComputerAction::Update),
            "computer.reset" => Command::OpenComputerConfirm(crate::state::ComputerAction::Reset),
            "computer.confirm" => Command::ConfirmComputerAction,
            "computer.cancel" => Command::CancelComputerConfirm,
            "computer.network_policy" => Command::OpenNetworkPolicy,
            "computer.egress_policy" => {
                let word = args
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "computer.egress_policy requires arg mode".to_string())?;
                let mode = crate::opengrok::LocalExecMode::parse(word)
                    .ok_or_else(|| format!("unknown mode `{word}` (bypass, ask, never)"))?;
                // A no-op on a hidden control is an error, not a silent success: the driver
                // must be able to tell "set" from "nothing to set".
                if self.egress_policy.is_none() {
                    return Err(
                        "this bot's computer carries no network policy to set (no computer, or an older server)"
                            .to_string(),
                    );
                }
                Command::SetEgressPolicy(mode)
            }
            "settings.account" => Command::ToggleAccount,
            "recipes.open" => Command::OpenRecipes,
            "recipes.filter" => {
                let word = args
                    .get("filter")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "recipes.filter requires arg filter".to_string())?;
                Command::SetRecipesFilter(crate::state::RecipeFilter::from_query(word).ok_or_else(
                    || format!("unknown recipes filter `{word}` (mine, shared, org)"),
                )?)
            }
            "recipe.open" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "recipe.open requires arg id".to_string())?;
                Command::OpenRecipe(id.to_string())
            }
            "recipe.close" => Command::CloseRecipe,
            "auth.login" => {
                let email = args
                    .get("email")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "auth.login requires arg email".to_string())?
                    .to_string();
                let password = args
                    .get("password")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "auth.login requires arg password".to_string())?
                    .to_string();
                Command::Login { email, password }
            }
            "auth.logout" => Command::Logout,
            // What the signed-out banner's button does, under a name, so a driver can take the
            // way out without having to find the button first.
            "auth.sign-in-again" => Command::SignInAgain,
            "chat.send" => {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "chat.send requires arg text".to_string())?
                    .to_string();
                Command::SendMessage(text)
            }
            // The forced send and the stop, under names, so a driver can exercise the
            // queue: send while a turn runs, then send now, then stop.
            "chat.send-steer" => {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "chat.send-steer requires arg text".to_string())?
                    .to_string();
                Command::SendMessageSteer(text)
            }
            "turn.stop" => Command::StopTurn,
            "session.select" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "session.select requires arg id".to_string())?;
                Command::SelectSession(id.to_string())
            }
            "approval.answer" => {
                let call_id = args
                    .get("call_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "approval.answer requires arg call_id".to_string())?;
                let answer = args
                    .get("answer")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "approval.answer requires arg answer".to_string())?;
                let (call_id, resolution) =
                    approval_target(&format!("approval-{call_id}-{answer}")).ok_or_else(|| {
                        format!("unknown answer `{answer}` (allow-once, deny-once, always, never)")
                    })?;
                Command::AnswerApproval {
                    call_id,
                    resolution,
                }
            }
            "UserFormContinue" | "user-form.continue" => Command::UserFormContinue {
                card_key: self.invoke_user_form_card_key(args)?,
            },
            "AddSiteLogin" | "logins.add" => Command::AddSiteLogin {
                origin: invoke_arg_str(args, &["origin", "site", "website"])
                    .ok_or_else(|| "logins.add requires arg origin".to_string())?,
                username: invoke_arg_str(args, &["username", "user"])
                    .ok_or_else(|| "logins.add requires arg username".to_string())?,
                password: RedactedSecret(
                    invoke_arg_str(args, &["password"])
                        .ok_or_else(|| "logins.add requires arg password".to_string())?,
                ),
                label: invoke_arg_str(args, &["label", "title"]).unwrap_or_default(),
                notes: invoke_arg_str(args, &["notes"]).unwrap_or_default(),
            },
            // The saved logins as the page lists them, so a driver can find an id without
            // reading the tree. Never a password: the host never holds one.
            "logins.list" => {
                return Ok(DispatchResult::json(serde_json::json!({
                    "logins": self
                        .site_logins
                        .iter()
                        .map(|login| serde_json::json!({
                            "id": login.row.id,
                            "kind": login.row.kind,
                            "label": login.row.label,
                            "origin": login.row.origin,
                            "username": login.row.username,
                            "on_this_mac": login.on_this_mac,
                            "last_used_at_ms": login.row.last_used_at_ms,
                        }))
                        .collect::<Vec<_>>()
                })));
            }
            // No `q` clears the search.
            "logins.search" => {
                let query = invoke_arg_str(args, &["q", "query"]).unwrap_or_default();
                return self.set_site_login_query(query);
            }
            // No `id` clears the pick.
            "logins.select" => match invoke_arg_str(args, &["id", "login_id"]) {
                Some(id) => Command::SelectSiteLogin(Some(
                    self.site_login_id(&id)
                        .ok_or_else(|| format!("no saved login `{id}`"))?,
                )),
                None => Command::SelectSiteLogin(None),
            },
            // No `notes` empties them.
            "logins.notes" => {
                let id = invoke_arg_str(args, &["id", "login_id"])
                    .ok_or_else(|| "logins.notes requires arg id".to_string())?;
                let id = self
                    .site_login_id(&id)
                    .ok_or_else(|| format!("no saved login `{id}`"))?;
                Command::SetSiteLoginNotes {
                    id,
                    notes: invoke_arg_str(args, &["notes"]).unwrap_or_default(),
                }
            }
            "UserFormClearSaved" | "user-form.clear-saved" => Command::UserFormClearSaved {
                card_key: self.invoke_user_form_card_key(args)?,
            },
            "UserFormRegisterPasskey" | "user-form.register-passkey" => {
                Command::UserFormRegisterPasskey {
                    card_key: self.invoke_user_form_card_key(args)?,
                }
            }
            "ImportSiteLogins" | "logins.import" => Command::ImportSiteLogins {
                path: invoke_arg_str(args, &["path", "file"])
                    .ok_or_else(|| "logins.import requires arg path".to_string())?,
            },
            "UserFormUseSaved" | "user-form.use-saved" => {
                let card_key = self.invoke_user_form_card_key(args)?;
                let form = self
                    .user_forms
                    .iter()
                    .find(|form| form.card_key == card_key)
                    .ok_or_else(|| format!("no user-form `{card_key}`"))?;
                let login_id = match invoke_arg_str(args, &["login_id", "loginId", "id"]) {
                    Some(id) => id,
                    None => match form.saved_logins.as_slice() {
                        [(one, _, _)] => one.clone(),
                        [] => return Err("that card offers no saved login".into()),
                        _ => return Err("user-form.use-saved requires arg login_id".into()),
                    },
                };
                Command::UserFormUseSaved { card_key, login_id }
            }
            "UserFormDismiss" | "user-form.dismiss" => Command::UserFormDismiss {
                card_key: self.invoke_user_form_card_key(args)?,
            },
            "UserFormOpenScreen" | "user-form.screen" => Command::UserFormOpenScreen {
                card_key: self.invoke_user_form_card_key(args)?,
            },
            // The open bot's routines as the pane lists them, so a driver can find the id of
            // the one it just made without reading the tree.
            "routine.list" => {
                return Ok(DispatchResult::json(serde_json::json!({
                    "routines": self
                        .routines
                        .iter()
                        .map(|routine| serde_json::json!({
                            "id": routine.id,
                            "name": routine.name,
                            "kind": routine.kind,
                            "cron": routine.cron,
                            "active": routine.active,
                            "webhook_url": routine.webhook_url,
                            // The key rides with the row: firing the hook needs it, and so
                            // does telling a rotated key from the one it replaced.
                            "webhook_key": routine.webhook_key,
                        }))
                        .collect::<Vec<_>>(),
                })));
            }
            "routine.create" => {
                let prompt = invoke_arg_str(args, &["prompt", "instruction"])
                    .ok_or_else(|| "routine.create requires arg prompt".to_string())?;
                let cron = invoke_arg_str(args, &["cron"]);
                let kind = match invoke_arg_str(args, &["kind"]).as_deref() {
                    Some("webhook") => crate::opengrok::ScheduleKind::Webhook,
                    None | Some("cron") => crate::opengrok::ScheduleKind::Cron,
                    Some(other) => {
                        return Err(format!("unknown routine kind `{other}` (cron, webhook)"));
                    }
                };
                if kind == crate::opengrok::ScheduleKind::Cron && cron.is_none() {
                    return Err(
                        "routine.create with kind=cron requires arg cron (a line like \
                         `0 9 * * 1-5`)"
                            .to_string(),
                    );
                }
                Command::CreateRoutine {
                    kind,
                    prompt,
                    // A webhook has no clock, and sending one a line would be asking the server
                    // for something it has no way to honour.
                    cron: cron.filter(|_| kind == crate::opengrok::ScheduleKind::Cron),
                }
            }
            // Settings → Skills. A skill is prose the model reads before it works; `recipe.*`
            // is the taped replay of clicks, and the two verbs never cross.
            "skills.open" => Command::OpenSkills,
            "skills.scope" => {
                let word = invoke_arg_str(args, &["scope", "side"])
                    .ok_or_else(|| "skills.scope requires arg scope".to_string())?;
                Command::SetSkillsScope(
                    SkillScope::from_word(&word).ok_or_else(|| {
                        format!("unknown skills scope `{word}` (yours, discover)")
                    })?,
                )
            }
            // No `q` clears the search.
            "skills.search" => {
                let query = invoke_arg_str(args, &["q", "query"]).unwrap_or_default();
                return self.set_skills_query(query);
            }
            "skill.open" => Command::OpenSkill(self.invoke_skill_id(args, "skill.open")?),
            "skill.create" if self.skill_saving => return Err(ALREADY_SAVING.to_string()),
            "skill.create" => Command::CreateSkill {
                name: invoke_arg_str(args, &["name"])
                    .ok_or_else(|| "skill.create requires arg name".to_string())?,
                description: invoke_arg_str(args, &["description"]).unwrap_or_default(),
                // No body writes the row and leaves it a draft, which is what the sheet does
                // with the instructions left empty.
                body: invoke_arg_str(args, &["body", "instructions"]).unwrap_or_default(),
            },
            "skill.upload" if self.skill_saving => return Err(ALREADY_SAVING.to_string()),
            "skill.upload" => Command::UploadSkill {
                path: invoke_arg_str(args, &["path", "file", "folder"])
                    .ok_or_else(|| "skill.upload requires arg path".to_string())?,
            },
            // Delete asks first, as it does for a person: this opens the dialog and the two
            // below answer it, the way `computer.update` and `computer.confirm` do.
            "skill.delete" => Command::AskSkillDelete {
                id: self.invoke_skill_id(args, "skill.delete")?,
            },
            "skill.delete.confirm" => {
                if self.skill_delete_confirm.is_none() {
                    return Err("no delete is waiting to be answered".to_string());
                }
                Command::ConfirmSkillDelete
            }
            "skill.delete.cancel" => Command::CloseSkillDeleteConfirm,
            "routine.rotate" => Command::RotateRoutineWebhook {
                routine_id: self.invoke_routine_id(args, "routine.rotate")?,
            },
            "routine.delete" => Command::DeleteRoutine {
                routine_id: self.invoke_routine_id(args, "routine.delete")?,
            },
            "routine.thread" => Command::OpenRoutineThread {
                routine_id: self.invoke_routine_id(args, "routine.thread")?,
            },
            "recipe.run" => {
                let open = self
                    .recipe_detail
                    .as_ref()
                    .ok_or_else(|| "recipe.run needs an open recipe".to_string())?;
                // The same gate as the button: a second start would drop the run the page is
                // following, and one under a save would lose its busy line when the save lands.
                if !open.runnable {
                    return Err(
                        "recipe.run: Run is not on offer (no version to play, or the page is busy)"
                            .to_string(),
                    );
                }
                let bot = invoke_arg_str(args, &["bot", "coworker", "coworker_id"])
                    .or_else(|| open.run_bot.clone())
                    .ok_or_else(|| "recipe.run: no bot is granted this recipe".to_string())?;
                Command::RunOpenRecipe(bot)
            }
            "routine.run" => {
                let routine_id = self.invoke_routine_id(args, "routine.run")?;
                self.refuse_dead_test_run("routine.run", &routine_id)?;
                Command::RunRoutineNow { routine_id }
            }
            "routine.edit" => {
                let routine_id = self.invoke_routine_id(args, "routine.edit")?;
                // The fields are dead on a server that cannot change a routine; an edit by name
                // is refused in the same words, as the app would refuse it.
                if self
                    .routines
                    .iter()
                    .any(|routine| routine.id == routine_id && routine.cant_change())
                {
                    return Err(format!(
                        "routine.edit is refused: {}",
                        crate::state::ROUTINE_EDIT_UNAVAILABLE
                    ));
                }
                let name = invoke_arg_str(args, &["name"]);
                let prompt = invoke_arg_str(args, &["prompt", "instruction"]);
                if name.is_none() && prompt.is_none() {
                    return Err("routine.edit requires arg name or prompt".to_string());
                }
                Command::EditRoutine {
                    routine_id,
                    name,
                    prompt,
                }
            }
            other => return Err(format!("unknown invoke `{other}`")),
        };
        self.pending = Some(cmd);
        Ok(DispatchResult::empty())
    }

    /// One of the open routine's header icons, refused when no routine is open or when the icon
    /// is dead, in the words its tooltip says it in.
    fn routine_header_command(&self, target: &str) -> Result<Command, String> {
        let open = self
            .open_routine()
            .ok_or_else(|| format!("`{target}` is in a routine's header, and none is open"))?;
        let routine_id = open.id.clone();
        let on_the_server = open.kind != "draft";
        Ok(match target {
            ids::ROUTINE_HISTORY_TOGGLE => Command::ToggleRoutineHistory,
            ids::ROUTINE_OPEN_THREAD if !on_the_server => {
                return Err(format!(
                    "`{target}` is dead: choose when it runs first; its thread is on the server"
                ));
            }
            ids::ROUTINE_OPEN_THREAD => Command::OpenRoutineThread { routine_id },
            ids::ROUTINE_RUN_NOW if !on_the_server => {
                return Err(format!(
                    "`{target}` is dead: choose when it runs first, then run it"
                ));
            }
            ids::ROUTINE_RUN_NOW => {
                self.refuse_dead_test_run(target, &routine_id)?;
                Command::RunRoutineNow { routine_id }
            }
            _ => Command::AskDeleteRoutine { routine_id },
        })
    }

    /// A click on "When to run" or in the wake editor, refused where the panel's control is dead,
    /// in the words it says it in. `None` for a target that is not one of theirs.
    fn wake_command(&self, target: &str) -> Option<Result<Command, String>> {
        use crate::state::{WakeBox, WakeTab};
        let rest = target.strip_prefix("routine-wake-")?;
        let Some(open) = self.open_routine() else {
            return Some(Err(format!(
                "`{target}` is in a routine's panel, and none is open"
            )));
        };
        let editor = self.routine_wake_editor.as_ref();
        let dead = |why: &str| Some(Err(format!("`{target}` is dead: {why}")));
        if target == ids::ROUTINE_WAKE_ADD {
            if !open.wakes.is_empty() {
                return dead(crate::state::ONE_WAKE_PER_ROUTINE);
            }
            if editor.is_some() {
                return dead("the wake editor is open");
            }
            return Some(Ok(Command::OpenWakeEditor { index: None }));
        }
        if let Some(at) = rest
            .strip_prefix("edit-")
            .and_then(|at| at.parse::<usize>().ok())
        {
            let Some((_, webhook)) = open.wakes.get(at) else {
                return Some(Err(format!("the open routine has no wake {at}")));
            };
            if !webhook && open.cant_change() {
                return dead(crate::state::ROUTINE_EDIT_UNAVAILABLE);
            }
            return Some(Ok(Command::OpenWakeEditor { index: Some(at) }));
        }
        if rest.starts_with("delete-") {
            return dead(crate::state::LAST_WAKE_STAYS);
        }
        let Some((editor, status)) = editor else {
            return Some(Err(format!(
                "`{target}` is in the wake editor, which is shut"
            )));
        };
        let cmd = if let Some(word) = rest.strip_prefix("tab-") {
            let tab = WakeTab::ALL.into_iter().find(|tab| tab.word() == word)?;
            if !editor.tab_open(tab) {
                return dead("a routine stays the kind it was made");
            }
            Command::PickWakeTab(tab)
        } else if let Some((which, up)) = [WakeBox::Every, WakeBox::Hour, WakeBox::Minute]
            .into_iter()
            .flat_map(|which| [(which, true), (which, false)])
            .find(|(which, up)| {
                target
                    == format!(
                        "{}-{}",
                        ids::routine_wake_box(*which),
                        if *up { "up" } else { "down" }
                    )
            })
        {
            Command::StepWakeBox { which, up }
        } else if let Some(unit) = [
            crate::state::ScheduleUnit::Minutes,
            crate::state::ScheduleUnit::Hours,
            crate::state::ScheduleUnit::Days,
        ]
        .into_iter()
        .find(|unit| target == ids::routine_wake_unit(*unit))
        {
            Command::SetWakeUnit(unit)
        } else if target == ids::ROUTINE_WAKE_AM || target == ids::ROUTINE_WAKE_PM {
            Command::SetWakePm(target == ids::ROUTINE_WAKE_PM)
        } else if let Some(day) = rest.strip_prefix("day-").and_then(|d| d.parse::<u8>().ok()) {
            Command::ToggleWakeDay(day)
        } else if let Some(date) = rest
            .strip_prefix("date-")
            .and_then(|d| d.parse::<u8>().ok())
        {
            Command::ToggleWakeDate(date)
        } else if let Some(month) = rest
            .strip_prefix("month-")
            .and_then(|m| m.parse::<u8>().ok())
        {
            Command::ToggleWakeMonth(month)
        } else if target == ids::ROUTINE_WAKE_SAVE {
            if let Some(why) = &status.error {
                return dead(why);
            }
            Command::SaveWake
        } else if target == ids::ROUTINE_WAKE_CANCEL {
            Command::CloseWakeEditor
        } else {
            return None;
        };
        Some(Ok(cmd))
    }

    /// The routine open in the Computer pane, as the tree lists it.
    fn open_routine(&self) -> Option<&RoutineSnap> {
        let open = self.routine_editor.as_deref()?;
        self.routines.iter().find(|routine| routine.id == open)
    }

    /// Test run on a server that cannot run a routine on demand is dead on screen, and refused
    /// here in the words the editor says it in.
    fn refuse_dead_test_run(&self, target: &str, routine_id: &str) -> Result<(), String> {
        if self
            .routines
            .iter()
            .any(|routine| routine.id == routine_id && routine.run_dead())
        {
            return Err(format!(
                "`{target}` is dead: {}",
                crate::state::ROUTINE_RUN_UNAVAILABLE
            ));
        }
        Ok(())
    }

    /// The routine an invoke names, checked against the ones on screen: an id nobody is
    /// showing is worth saying so about before a call goes out under it.
    fn invoke_routine_id(&self, args: &serde_json::Value, invoke: &str) -> Result<String, String> {
        let id = invoke_arg_str(args, &["id", "routine_id", "routineId"])
            .ok_or_else(|| format!("{invoke} requires arg id"))?;
        if !self.routines.iter().any(|routine| routine.id == id) {
            return Err(format!("no routine `{id}` on the open bot"));
        }
        Ok(id)
    }
}

impl AgentHost for NativeChatHost {
    fn hello(&self) -> HelloInfo {
        HelloInfo {
            protocol: PROTOCOL_VERSION,
            app: "nativechat".into(),
            platform: PlatformKind::Desktop,
            os: gpui_agent::protocol::host_os(),
            ready: self.ready,
            deliveries: vec![DeliveryMode::Semantic],
            auth: HelloAuth::None,
        }
    }

    fn snapshot(&self) -> UiTree {
        self.tree()
    }

    fn dispatch(&mut self, op: &Op) -> Result<DispatchResult, String> {
        if op.is_virtual_input() {
            return Err(virtual_unavailable(
                "virtual delivery is not wired: a semantic type or key already goes in as a \
                 GPUI keystroke, and a virtual click would need pointer synthesis this host \
                 does not do. Use delivery=semantic.",
            ));
        }
        match op {
            Op::Click { target, .. } => self.click(target),
            Op::Invoke { name, args } => self.invoke(name, args),
            Op::Shutdown => {
                self.pending = Some(Command::Shutdown);
                Ok(DispatchResult::empty())
            }
            Op::SetValue { target, value, .. } => self.set_value(target, value),
            Op::Type { target, text, .. } => self.type_into(target, text),
            Op::Key { target, key, .. } => self.key(target, key),
            _ => Ok(DispatchResult::empty()),
        }
    }

    fn screenshot(
        &self,
        _spec: gpui_agent::scroll_capture::ScreenshotSpec<'_>,
    ) -> Result<DispatchResult, String> {
        Err(gpui_agent::screenshot_unavailable(
            "screenshot is intercepted on the UI thread",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opengrok::{RecipeParameter, RecipeParameterKind};

    /// The same bar, for a decision tree. A driver that has just picked from `/` reads this
    /// line to know which of the two it got, so the noun has to be the thing's own.
    #[test]
    fn the_bar_says_workflow_when_the_draft_is_a_tree() {
        let mut host = host();
        let mut active = recipe(&[]);
        active.kind = RecipeKind::Workflow;
        active.name = "Search and retry".into();
        host.recipe_bar = Some(RecipeBarSnap {
            name: active.name.clone(),
            kind: active.kind,
            parameters: Vec::new(),
        });
        assert_eq!(
            host.snapshot().find(ids::COMPOSER_RECIPE_BAR).unwrap().name,
            "Workflow · Search and retry"
        );
    }

    fn routine(id: &str, kind: &'static str) -> RoutineSnap {
        RoutineSnap {
            id: id.into(),
            name: "Morning post".into(),
            kind,
            cron: (kind == "cron").then(|| "0 9 * * *".to_string()),
            active: true,
            webhook_url: (kind == "webhook").then(|| "https://og.example/hooks/sch_2".to_string()),
            webhook_key: (kind == "webhook").then(|| "og_live_abc".to_string()),
            runs: Vec::new(),
            notes: Vec::new(),
            runs_unavailable: false,
            unsaved: Vec::new(),
            wakes: match kind {
                "cron" => vec![("Every day at 9:00 AM".into(), false)],
                "webhook" => vec![("When a webhook fires".into(), true)],
                _ => Vec::new(),
            },
            zone: None,
        }
    }

    /// A run's line of a routine's Run history, as the tree is given it.
    fn ran(run_id: &str, cause: &'static str, status: &'static str) -> RunLineSnap {
        RunLineSnap::Ran {
            run_id: run_id.into(),
            cause: cause.into(),
            status,
        }
    }

    /// A run a Bot started with its `run_routine` tool (opengrok-server #337, built in #342) is on
    /// the tree as the window says it, `Run by Luna`, for the run and for a firing its press set off
    /// that the plan could not answer; the person's own Test run says what it always said, and
    /// nothing of any Bot.
    #[test]
    fn a_run_a_bot_started_is_on_the_tree_as_the_window_says_it() {
        use crate::opengrok::{RunCause, ScheduleRunStatus};
        use crate::state::{AgentRoutine, RoutineRun, RunOutcome};
        let line = |cause, by: Option<&str>, outcome| RoutineRun {
            at: String::new(),
            started_at_ms: 1_790_000_000_000,
            cause,
            by: by.map(str::to_string),
            outcome,
        };
        let row = AgentRoutine {
            id: "sch-1-3".into(),
            name: "Standup".into(),
            instruction: "post the standup".into(),
            active: true,
            triggers: Vec::new(),
            runs: vec![
                line(
                    RunCause::Bot,
                    Some("Luna"),
                    RunOutcome::Ran {
                        run_id: "run_b".into(),
                        status: ScheduleRunStatus::Ok,
                    },
                ),
                line(
                    RunCause::Bot,
                    Some("Luna"),
                    RunOutcome::Skipped {
                        reason: "Skipped: Relay is off for your plan".into(),
                    },
                ),
                line(
                    RunCause::Manual,
                    None,
                    RunOutcome::Ran {
                        run_id: "run_m".into(),
                        status: ScheduleRunStatus::Ok,
                    },
                ),
            ],
            saved: None,
            tz: None,
        };
        let mut fired = routine("sch-1-3", "cron");
        fired.runs = routine_snap(&row, &AppState::new()).runs;
        let mut host = host();
        host.computer_open = true;
        host.routines = vec![fired];
        let tree = host.snapshot();
        let said = |id: String| tree.find(&id).expect("the line").name.clone();
        assert_eq!(said(ids::routine_run("sch-1-3", "run_b")), "Run by Luna");
        assert_eq!(
            said(ids::routine_skipped("sch-1-3", 1_790_000_000_000)),
            "Run by Luna"
        );
        assert_eq!(said(ids::routine_run("sch-1-3", "run_m")), "Test run");
    }

    /// A firing the server skipped is on the tree among the runs, by when it was due, with what
    /// set it off and the server's sentence for why, and state `skipped`. It started no run, so
    /// a click on it is refused in words rather than sent as an open of a run that is not there,
    /// and the run beside it still opens.
    #[test]
    fn a_skipped_line_of_the_history_says_why_and_opens_nothing() {
        let said = "Skipped: your computer was off, so your plan couldn't answer";
        let mut host = host();
        host.computer_open = true;
        let mut fired = routine("sch-1-2", "cron");
        fired.runs = vec![
            RunLineSnap::Skipped {
                at_ms: 1_790_000_000_000,
                cause: "Schedule".into(),
                reason: said.into(),
            },
            ran("run_a", "Test run", "ok"),
        ];
        host.routines = vec![fired];
        let tree = host.snapshot();
        let skipped = tree
            .find(&ids::routine_skipped("sch-1-2", 1_790_000_000_000))
            .expect("the skipped line");
        assert_eq!(skipped.name, "Schedule");
        assert_eq!(skipped.value.as_deref(), Some(said));
        assert!(skipped.states.iter().any(|state| state == "skipped"));

        let refused = host
            .click(&ids::routine_skipped("sch-1-2", 1_790_000_000_000))
            .unwrap_err();
        assert!(refused.contains("started no run"), "{refused}");
        assert!(host.take_command().is_none(), "nothing was opened");
        host.click(&ids::routine_run("sch-1-2", "run_a")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::OpenRoutineRun { run_id, .. } if run_id == "run_a"
        ));
    }

    /// A routine the server has can be test-run, and its history is on the tree line by line:
    /// what set each run off, and where it got to. A draft has neither.
    #[test]
    fn a_routine_shows_its_history_and_can_be_test_run() {
        let mut host = host();
        host.computer_open = true;
        let mut fired = routine("sch-1-2", "webhook");
        fired.runs = vec![
            ran("run_b", "Test run", "running"),
            ran("run_a", "Webhook", "ok"),
        ];
        host.routines = vec![fired, routine("draft-1", "draft")];
        let tree = host.snapshot();
        let newest = tree.find(&ids::routine_run("sch-1-2", "run_b")).unwrap();
        assert_eq!(newest.name, "Test run");
        assert_eq!(newest.value.as_deref(), Some("running"));
        let hooked = tree.find(&ids::routine_run("sch-1-2", "run_a")).unwrap();
        assert_eq!(hooked.name, "Webhook");
        assert_eq!(hooked.value.as_deref(), Some("ok"));
        assert!(tree.find(&ids::routine_test("draft-1")).is_none());

        host.click(&ids::routine_test("sch-1-2")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::RunRoutineNow { routine_id } if routine_id == "sch-1-2"
        ));
    }

    /// The Active switch is on the tree where it stands, labelled as the editor labels it, and a
    /// click asks for the other way: a paused routine is switched back on, an active one paused.
    #[test]
    fn a_routines_active_switch_reads_where_it_stands_and_flips() {
        let mut host = host();
        host.computer_open = true;
        let mut paused = routine("sch-1-2", "cron");
        paused.active = false;
        host.routines = vec![routine("sch_1", "cron"), paused];
        let tree = host.snapshot();
        let on = tree.find(&ids::routine_active("sch_1")).unwrap();
        assert_eq!(on.name, "Active");
        assert!(on.checked == Some(true), "{on:?}");
        let off = tree.find(&ids::routine_active("sch-1-2")).unwrap();
        assert_eq!(off.name, "Paused");
        assert!(off.checked == Some(false), "{off:?}");

        host.click(&ids::routine_active("sch-1-2")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetRoutineActive { routine_id, active: true } if routine_id == "sch-1-2"
        ));
        host.click(&ids::routine_active("sch_1")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetRoutineActive { routine_id, active: false } if routine_id == "sch_1"
        ));
    }

    /// On a server that cannot change a routine, run one on demand or list what one ran, the
    /// routine says so where the editor does: a line for each, the history's place, what was
    /// typed and not kept, and a Test run that is dead and refused in the editor's words, by
    /// click and by name, as an edit by name is. The editor's red line is there while it shows.
    #[test]
    fn a_routine_says_what_its_server_cannot_do_with_it() {
        let mut host = host();
        host.computer_open = true;
        let mut old = routine("sch_1", "cron");
        old.notes = vec![
            ("cant-change", crate::state::ROUTINE_EDIT_UNAVAILABLE),
            ("cant-run", crate::state::ROUTINE_RUN_UNAVAILABLE),
        ];
        old.runs_unavailable = true;
        old.unsaved = vec![("Name", "Daily".into()), ("When", "Every 1 minute".into())];
        host.routines = vec![old, routine("sch_2", "cron")];
        host.routine_error = Some("No such schedule".into());
        let tree = host.snapshot();
        let line = |id: String| tree.find(&id).map(|node| node.name.clone());
        assert_eq!(
            line(ids::routine_note("sch_1", "cant-change")).as_deref(),
            Some(crate::state::ROUTINE_EDIT_UNAVAILABLE)
        );
        assert_eq!(
            line(ids::routine_note("sch_1", "cant-run")).as_deref(),
            Some(crate::state::ROUTINE_RUN_UNAVAILABLE)
        );
        assert_eq!(
            line(ids::routine_runs_unavailable("sch_1")).as_deref(),
            Some(crate::state::ROUTINE_RUNS_UNAVAILABLE)
        );
        let unsaved = tree.find(&ids::routine_unsaved("sch_1")).unwrap();
        assert_eq!(
            unsaved.value.as_deref(),
            Some("Name: Daily\nWhen: Every 1 minute")
        );
        assert!(!tree.find(&ids::routine_test("sch_1")).unwrap().enabled);
        assert!(tree.find(&ids::routine_test("sch_2")).unwrap().enabled);
        assert!(tree.find(&ids::routine_runs_unavailable("sch_2")).is_none());
        assert_eq!(
            tree.find(ids::ROUTINE_ERROR).map(|node| node.name.as_str()),
            Some("No such schedule")
        );

        let refused = host.click(&ids::routine_test("sch_1")).unwrap_err();
        assert!(
            refused.contains(crate::state::ROUTINE_RUN_UNAVAILABLE),
            "{refused}"
        );
        assert!(
            host.invoke("routine.run", &serde_json::json!({ "id": "sch_1" }))
                .is_err()
        );
        let refused = host
            .invoke(
                "routine.edit",
                &serde_json::json!({ "id": "sch_1", "name": "Daily" }),
            )
            .unwrap_err();
        assert!(
            refused.contains(crate::state::ROUTINE_EDIT_UNAVAILABLE),
            "{refused}"
        );
        assert!(host.take_command().is_none(), "nothing was sent");
        host.click(&ids::routine_test("sch_2")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::RunRoutineNow { routine_id }) if routine_id == "sch_2"
        ));
    }

    /// The open routine's header carries its four icons, each acting on that routine: the
    /// history toggle says which view it leads to and is `selected` while the history shows, and
    /// Open in thread and Run it now are dead on a draft, Run it now also where the server cannot
    /// run a routine on demand, each refused with its reason. With no routine open there are none.
    #[test]
    fn the_open_routines_header_icons_act_on_it() {
        let mut host = host();
        host.computer_open = true;
        host.routines = vec![routine("sch_1", "cron"), routine("draft-1", "draft")];
        assert!(host.snapshot().find(ids::ROUTINE_HISTORY_TOGGLE).is_none());
        assert!(
            host.click(ids::ROUTINE_RUN_NOW).is_err(),
            "no routine is open"
        );

        host.routine_editor = Some("sch_1".into());
        let tree = host.snapshot();
        for id in [
            ids::ROUTINE_HISTORY_TOGGLE,
            ids::ROUTINE_OPEN_THREAD,
            ids::ROUTINE_RUN_NOW,
            ids::ROUTINE_DELETE,
        ] {
            assert!(tree.find(id).is_some_and(|node| node.enabled), "{id}");
        }
        assert_eq!(
            tree.find(ids::ROUTINE_HISTORY_TOGGLE).unwrap().name,
            "Run history"
        );
        let clicked = |host: &mut NativeChatHost, id: &str| {
            host.click(id).unwrap();
            host.take_command().unwrap()
        };
        assert!(matches!(
            clicked(&mut host, ids::ROUTINE_HISTORY_TOGGLE),
            Command::ToggleRoutineHistory
        ));
        assert!(matches!(
            clicked(&mut host, ids::ROUTINE_OPEN_THREAD),
            Command::OpenRoutineThread { routine_id } if routine_id == "sch_1"
        ));
        assert!(matches!(
            clicked(&mut host, ids::ROUTINE_RUN_NOW),
            Command::RunRoutineNow { routine_id } if routine_id == "sch_1"
        ));
        assert!(matches!(
            clicked(&mut host, ids::ROUTINE_DELETE),
            Command::AskDeleteRoutine { routine_id } if routine_id == "sch_1"
        ));

        host.routine_history_open = true;
        let toggle = host
            .snapshot()
            .find(ids::ROUTINE_HISTORY_TOGGLE)
            .cloned()
            .unwrap();
        assert_eq!(toggle.name, "Back to the routine");
        assert!(toggle.states.iter().any(|state| state == "selected"));

        host.routine_editor = Some("draft-1".into());
        let tree = host.snapshot();
        assert!(!tree.find(ids::ROUTINE_OPEN_THREAD).unwrap().enabled);
        assert!(!tree.find(ids::ROUTINE_RUN_NOW).unwrap().enabled);
        assert!(host.click(ids::ROUTINE_RUN_NOW).is_err());
        assert!(host.click(ids::ROUTINE_OPEN_THREAD).is_err());

        host.routine_editor = Some("sch_1".into());
        host.routines[0].notes = vec![("cant-run", crate::state::ROUTINE_RUN_UNAVAILABLE)];
        let run = host.snapshot().find(ids::ROUTINE_RUN_NOW).cloned().unwrap();
        assert!(!run.enabled);
        assert_eq!(
            run.value.as_deref(),
            Some(crate::state::ROUTINE_RUN_UNAVAILABLE)
        );
        assert!(host.click(ids::ROUTINE_RUN_NOW).is_err());
    }

    /// Delete asks first, by click as by hand: the question is on the tree while it asks, with
    /// its two answers, and an answer with no question asked is refused.
    #[test]
    fn delete_asks_first_and_the_question_is_answered_by_id() {
        let mut host = host();
        host.routines = vec![routine("sch_1", "cron")];
        assert!(
            host.click(ids::ROUTINE_DELETE_CONFIRM).is_err(),
            "nothing asked"
        );
        host.routine_delete_question = Some((
            "Delete \"Morning post\"?".into(),
            crate::state::ROUTINE_DELETE_KEEPS.into(),
        ));
        let tree = host.snapshot();
        let prompt = tree.find(ids::ROUTINE_DELETE_PROMPT).unwrap();
        assert_eq!(prompt.name, "Delete \"Morning post\"?");
        assert_eq!(
            prompt.value.as_deref(),
            Some("It stops running. Its past runs and conversation stay.")
        );
        host.click(ids::ROUTINE_DELETE_CANCEL).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::CancelRoutineDelete)
        ));
        host.click(ids::ROUTINE_DELETE_CONFIRM).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ConfirmRoutineDelete)
        ));
    }

    /// A routine whose times are in another zone than this computer's names it under its wake
    /// and under what the wake editor picked (opengrok-server #316: the server reads the line in
    /// the routine's zone); in this computer's own zone nothing is named, nor on a webhook, which
    /// has no times. The editor leaves the next run unsaid in a zone it cannot read the line in,
    /// and says it in UTC, which it can.
    #[test]
    fn a_routines_zone_is_named_where_it_is_not_this_computers() {
        use crate::state::{RoutineZone, ScheduleSpec, WakeEditor, WakeTab};
        let mut host = host();
        host.computer_open = true;
        let mut away = routine("sch_1", "cron");
        away.zone = Some("Europe/London".into());
        let mut hook = routine("sch_2", "webhook");
        hook.zone = Some("Europe/London".into());
        host.routines = vec![away, hook, routine("sch_3", "cron")];
        host.routine_editor = Some("sch_1".into());
        let tree = host.snapshot();
        assert_eq!(
            tree.find(&ids::routine_wake_zone(0))
                .expect("the zone under the wake")
                .name,
            "Europe/London"
        );
        host.routine_editor = Some("sch_2".into());
        assert!(
            host.snapshot().find(&ids::routine_wake_zone(0)).is_none(),
            "a webhook has no times"
        );
        host.routine_editor = Some("sch_3".into());
        assert!(
            host.snapshot().find(&ids::routine_wake_zone(0)).is_none(),
            "this computer's own zone"
        );

        host.routine_editor = Some("sch_1".into());
        let editor = |name: &str, here: bool| WakeEditor {
            routine_id: "sch_1".into(),
            index: Some(0),
            kind: Some(crate::opengrok::ScheduleKind::Cron),
            tab: WakeTab::Daily,
            spec: ScheduleSpec::advanced_daily(9, 0),
            every: "1".into(),
            hour: "9".into(),
            minute: "00".into(),
            pm: false,
            opened: 1,
            resync: 0,
            zone: RoutineZone {
                name: name.into(),
                here,
            },
        };
        let open = |host: &mut NativeChatHost, editor: WakeEditor| {
            let status = editor.status(chrono::Utc::now());
            host.routine_wake_editor = Some((editor, status));
            host.snapshot()
        };
        let tree = open(&mut host, editor("Europe/London", false));
        let zone = tree.find(ids::ROUTINE_WAKE_ZONE).expect("the zone");
        assert_eq!(zone.name, "Europe/London time");
        assert_eq!(zone.value.as_deref(), Some("Europe/London"));
        assert_eq!(
            tree.find(ids::ROUTINE_WAKE_SUMMARY).unwrap().name,
            "Every day at 9:00 AM"
        );
        assert!(
            tree.find(ids::ROUTINE_WAKE_NEXT).is_none(),
            "no clock here for Europe/London"
        );
        assert!(tree.find(ids::ROUTINE_WAKE_SAVE).unwrap().enabled);
        let tree = open(&mut host, editor("UTC", false));
        assert_eq!(tree.find(ids::ROUTINE_WAKE_ZONE).unwrap().name, "UTC time");
        assert!(tree.find(ids::ROUTINE_WAKE_NEXT).is_some(), "UTC is read");
        let tree = open(&mut host, editor("Asia/Manila", true));
        assert!(tree.find(ids::ROUTINE_WAKE_ZONE).is_none());
        assert!(
            tree.find(ids::ROUTINE_WAKE_NEXT).is_some(),
            "this computer's own"
        );
    }

    /// "When to run" on the open routine: + is dead while it has its one wake, saying why; the
    /// wake is listed in words with ✎ live and 🗑 dead; ✎ is dead too where the server cannot
    /// change a routine. The wake editor, while open, carries its tabs and the open tab's
    /// controls, what was picked and Save, and clicks and `set_value` go to it, a dead one
    /// refused in the words the panel says it in.
    #[test]
    fn when_to_run_lists_the_wake_and_the_editor_is_driven_by_id() {
        use crate::state::{
            LAST_WAKE_STAYS, ONE_WAKE_PER_ROUTINE, ROUTINE_EDIT_UNAVAILABLE, ScheduleSpec, WakeBox,
            WakeEditor, WakeTab,
        };
        let mut host = host();
        host.computer_open = true;
        host.routines = vec![routine("sch_1", "cron"), routine("draft-1", "draft")];
        host.routine_editor = Some("sch_1".into());
        let tree = host.snapshot();
        let add = tree.find(ids::ROUTINE_WAKE_ADD).unwrap();
        assert!(!add.enabled);
        assert_eq!(add.value.as_deref(), Some(ONE_WAKE_PER_ROUTINE));
        assert_eq!(
            tree.find(&ids::routine_wake(0)).unwrap().name,
            "Every day at 9:00 AM"
        );
        assert!(tree.find(&ids::routine_wake_edit(0)).unwrap().enabled);
        let delete = tree.find(&ids::routine_wake_delete(0)).unwrap();
        assert!(!delete.enabled);
        assert_eq!(delete.value.as_deref(), Some(LAST_WAKE_STAYS));
        assert!(host.click(ids::ROUTINE_WAKE_ADD).is_err());
        assert!(host.click(&ids::routine_wake_delete(0)).is_err());
        host.click(&ids::routine_wake_edit(0)).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::OpenWakeEditor { index: Some(0) })
        ));

        host.routine_editor = Some("draft-1".into());
        assert!(host.snapshot().find(ids::ROUTINE_WAKE_ADD).unwrap().enabled);
        host.click(ids::ROUTINE_WAKE_ADD).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::OpenWakeEditor { index: None })
        ));

        host.routine_editor = Some("sch_1".into());
        host.routines[0].notes = vec![("cant-change", ROUTINE_EDIT_UNAVAILABLE)];
        let edit = host
            .snapshot()
            .find(&ids::routine_wake_edit(0))
            .cloned()
            .unwrap();
        assert!(!edit.enabled);
        assert_eq!(edit.value.as_deref(), Some(ROUTINE_EDIT_UNAVAILABLE));
        assert!(host.click(&ids::routine_wake_edit(0)).is_err());
        host.routines[0].notes.clear();

        assert!(
            host.click(&ids::routine_wake_day(1)).is_err(),
            "the editor is shut"
        );
        let mut spec = ScheduleSpec::advanced_daily(9, 0).on_tab(WakeTab::Weekly);
        spec.weekdays = vec![1, 2, 3, 4, 5];
        let editor = WakeEditor {
            routine_id: "sch_1".into(),
            index: Some(0),
            kind: Some(crate::opengrok::ScheduleKind::Cron),
            tab: WakeTab::Weekly,
            spec,
            every: "1".into(),
            hour: "9".into(),
            minute: "00".into(),
            pm: false,
            opened: 1,
            resync: 0,
            zone: crate::state::RoutineZone {
                name: "UTC".into(),
                here: false,
            },
        };
        let status = editor.status(chrono::Utc::now());
        host.routine_wake_editor = Some((editor, status));
        let tree = host.snapshot();
        let weekly = tree.find(&ids::routine_wake_tab(WakeTab::Weekly)).unwrap();
        assert!(weekly.states.iter().any(|state| state == "selected"));
        assert!(
            !tree
                .find(&ids::routine_wake_tab(WakeTab::Webhook))
                .unwrap()
                .enabled
        );
        assert_eq!(
            tree.find(&ids::routine_wake_day(1)).unwrap().checked,
            Some(true)
        );
        assert_eq!(
            tree.find(&ids::routine_wake_day(0)).unwrap().checked,
            Some(false)
        );
        assert_eq!(
            tree.find(ids::routine_wake_box(WakeBox::Hour))
                .unwrap()
                .value
                .as_deref(),
            Some("9")
        );
        assert_eq!(
            tree.find(ids::ROUTINE_WAKE_SUMMARY).unwrap().name,
            "Weekdays at 9:00 AM"
        );
        assert!(tree.find(ids::ROUTINE_WAKE_NEXT).is_some());
        assert!(tree.find(ids::ROUTINE_WAKE_SAVE).unwrap().enabled);
        assert!(
            tree.find(&ids::routine_wake_date(1)).is_none(),
            "the Monthly tab's"
        );

        let clicked = |host: &mut NativeChatHost, id: &str| {
            host.click(id).unwrap();
            host.take_command().unwrap()
        };
        assert!(matches!(
            clicked(&mut host, &ids::routine_wake_tab(WakeTab::Daily)),
            Command::PickWakeTab(WakeTab::Daily)
        ));
        assert!(
            host.click(&ids::routine_wake_tab(WakeTab::Webhook))
                .is_err()
        );
        assert!(matches!(
            clicked(&mut host, &ids::routine_wake_day(6)),
            Command::ToggleWakeDay(6)
        ));
        assert!(matches!(
            clicked(&mut host, &ids::routine_wake_month(3)),
            Command::ToggleWakeMonth(3)
        ));
        assert!(matches!(
            clicked(&mut host, "routine-wake-hour-up"),
            Command::StepWakeBox {
                which: WakeBox::Hour,
                up: true
            }
        ));
        assert!(matches!(
            clicked(&mut host, ids::ROUTINE_WAKE_PM),
            Command::SetWakePm(true)
        ));
        assert!(matches!(
            clicked(&mut host, ids::ROUTINE_WAKE_SAVE),
            Command::SaveWake
        ));
        assert!(matches!(
            clicked(&mut host, ids::ROUTINE_WAKE_CANCEL),
            Command::CloseWakeEditor
        ));
        host.dispatch(&Op::SetValue {
            target: ids::routine_wake_box(WakeBox::Hour).into(),
            value: "7".into(),
        })
        .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetWakeBox { which: WakeBox::Hour, text }) if text == "7"
        ));

        // A pick that is not a schedule: Save is dead, with the reason.
        let (editor, _) = host.routine_wake_editor.take().unwrap();
        let mut empty = editor.clone();
        empty.spec.weekdays.clear();
        let status = empty.status(chrono::Utc::now());
        host.routine_wake_editor = Some((empty, status));
        assert!(
            !host
                .snapshot()
                .find(ids::ROUTINE_WAKE_SAVE)
                .unwrap()
                .enabled
        );
        let refused = host.click(ids::ROUTINE_WAKE_SAVE).unwrap_err();
        assert!(refused.contains("day of the week"), "{refused}");
    }

    /// The driver's two routine verbs: Test run by name, and an edit saved the way the editor
    /// saves one. An edit that names nothing to change is refused before it reaches the app.
    #[test]
    fn a_driver_can_test_run_and_edit_a_routine_by_name() {
        let mut host = host();
        host.routines = vec![routine("sch_1", "cron")];
        host.invoke("routine.run", &serde_json::json!({ "id": "sch_1" }))
            .unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::RunRoutineNow { routine_id } if routine_id == "sch_1"
        ));
        host.invoke(
            "routine.edit",
            &serde_json::json!({ "id": "sch_1", "prompt": "summarise the standup notes" }),
        )
        .unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::EditRoutine { routine_id, name: None, prompt: Some(prompt) }
                if routine_id == "sch_1" && prompt == "summarise the standup notes"
        ));
        assert!(
            host.invoke("routine.edit", &serde_json::json!({ "id": "sch_1" }))
                .is_err()
        );
        assert!(
            host.invoke("routine.run", &serde_json::json!({ "id": "sch_nope" }))
                .is_err(),
            "a routine the open bot does not have"
        );
    }

    /// A routine the server has opens its thread, by its button or by name. The chat then says
    /// which routine it is and what fires it, and the way back is the Bot chip, which says it
    /// goes home only while the thread is open. There is no Back link of its own any more.
    #[test]
    fn a_routines_thread_is_opened_and_left_by_id() {
        let mut host = host();
        host.computer_open = true;
        host.routines = vec![routine("sch-1-2", "cron"), routine("draft-1", "draft")];
        assert!(
            host.snapshot()
                .find(&ids::routine_thread("draft-1"))
                .is_none()
        );
        assert!(host.snapshot().find(ids::CHAT_ROUTINE_THREAD).is_none());

        host.click(&ids::routine_thread("sch-1-2")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::OpenRoutineThread { routine_id } if routine_id == "sch-1-2"
        ));
        // A line of its history opens the same thread at that run, dashes in both ids and all.
        host.routines[0].runs = vec![ran("run-a-1", "Test run", "ok")];
        host.click(&ids::routine_run("sch-1-2", "run-a-1")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::OpenRoutineRun { routine_id, run_id }
                if routine_id == "sch-1-2" && run_id == "run-a-1"
        ));
        host.invoke("routine.thread", &serde_json::json!({ "id": "sch-1-2" }))
            .unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::OpenRoutineThread { .. }
        ));

        host.routine_thread = Some(("Morning post".into(), "schedule".into()));
        host.bot_chip_goes_home = true;
        let tree = host.snapshot();
        let badge = tree.find(ids::CHAT_ROUTINE_THREAD).unwrap();
        assert_eq!(badge.name, "Morning post");
        assert_eq!(badge.value.as_deref(), Some("schedule"));
        assert!(
            tree.find("chat-routine-back").is_none(),
            "the Back link is gone"
        );
        assert!(host.click("chat-routine-back").is_err());
        let chip = tree.find(ids::HEADER_COWORKER).unwrap();
        assert!(chip.states.iter().any(|state| state == "goes-home"));
    }

    /// The chip at the top of the chat names the open Bot and says when a press of it goes home:
    /// in a routine's thread it goes back to the Bot's own chat (`goes-home`), and elsewhere it
    /// opens the settings. Either way the click is the chip's own press, decided by the app.
    #[test]
    fn the_bot_chip_says_when_it_goes_home() {
        let mut host = host();
        let chip = host.snapshot().find(ids::HEADER_COWORKER).cloned().unwrap();
        assert_eq!(chip.name, "Ada");
        assert!(!chip.states.iter().any(|state| state == "goes-home"));

        host.routine_thread = Some(("Morning post".into(), "schedule".into()));
        host.bot_chip_goes_home = true;
        let chip = host.snapshot().find(ids::HEADER_COWORKER).cloned().unwrap();
        assert!(chip.states.iter().any(|state| state == "goes-home"));
        host.click(ids::HEADER_COWORKER).unwrap();
        assert!(matches!(host.take_command(), Some(Command::PressBotChip)));

        host.sessions.clear();
        assert!(host.click(ids::HEADER_COWORKER).is_err(), "no Bot, no chip");
    }

    /// A cron routine carries the line the server keeps, and nothing about a webhook it has
    /// not got.
    #[test]
    fn a_cron_routine_is_its_line_and_the_webhook_ids_are_not_there() {
        let mut host = host();
        host.computer_open = true;
        host.routines = vec![routine("sch_1", "cron")];
        let tree = host.snapshot();
        let node = tree.find(&ids::routine("sch_1")).unwrap();
        assert_eq!(node.value.as_deref(), Some("0 9 * * *"));
        assert!(node.states.contains(&"cron".to_string()));
        assert!(node.states.contains(&"active".to_string()));
        assert!(tree.find(&ids::routine_webhook_url("sch_1")).is_none());
        assert!(tree.find(&ids::routine_rotate("sch_1")).is_none());
        assert!(
            tree.find(&ids::routine_trigger_schedule("sch_1")).is_none(),
            "a routine with a trigger is not offered another"
        );
        assert!(tree.find(&ids::routine_delete("sch_1")).is_some());
    }

    /// The URL and the key are on the tree because they are the two things a person copies,
    /// and a test that fires the hook needs both.
    #[test]
    fn a_webhook_routine_shows_the_url_and_the_key_it_was_given() {
        let mut host = host();
        host.computer_open = true;
        host.routines = vec![routine("sch_2", "webhook")];
        let tree = host.snapshot();
        assert_eq!(
            tree.find(&ids::routine_webhook_url("sch_2"))
                .unwrap()
                .value
                .as_deref(),
            Some("https://og.example/hooks/sch_2")
        );
        assert_eq!(
            tree.find(&ids::routine_webhook_key("sch_2"))
                .unwrap()
                .value
                .as_deref(),
            Some("og_live_abc")
        );
        assert!(tree.find(&ids::routine_rotate("sch_2")).is_some());

        // A key the server did not send back reads as an empty value rather than as a webhook
        // with no URL: the row is still there to fire, and the emptiness is the news.
        let mut keyless = routine("sch_3", "webhook");
        keyless.webhook_key = None;
        host.routines = vec![keyless];
        let tree = host.snapshot();
        assert_eq!(
            tree.find(&ids::routine_webhook_url("sch_3"))
                .unwrap()
                .value
                .as_deref(),
            Some("https://og.example/hooks/sch_2")
        );
        assert_eq!(
            tree.find(&ids::routine_webhook_key("sch_3"))
                .unwrap()
                .value
                .as_deref(),
            Some("")
        );
    }

    /// A draft is the one routine with a trigger to offer, and the only one not on the server.
    #[test]
    fn a_draft_offers_the_two_triggers() {
        let mut host = host();
        host.computer_open = true;
        host.routines = vec![routine("draft-1", "draft")];
        let tree = host.snapshot();
        assert!(
            tree.find(&ids::routine_trigger_schedule("draft-1"))
                .is_some()
        );
        assert!(
            tree.find(&ids::routine_trigger_webhook("draft-1"))
                .is_some()
        );
        assert!(tree.find(&ids::routine_webhook_url("draft-1")).is_none());
    }

    /// A server id can hold dashes, and three of this routine's controls end in one. The tail
    /// is read first so `sch-1-2` stays `sch-1-2` and does not lose its last two characters to
    /// a suffix that was never there.
    #[test]
    fn a_routines_controls_are_told_apart_from_an_id_with_dashes_in_it() {
        let mut host = host();
        host.routines = vec![routine("sch-1-2", "webhook")];
        let opened = |host: &mut NativeChatHost, target: &str| {
            host.click(target).unwrap();
            host.take_command().unwrap()
        };
        assert!(matches!(
            opened(&mut host, &ids::routine("sch-1-2")),
            Command::OpenRoutineEditor(Some(id)) if id == "sch-1-2"
        ));
        assert!(matches!(
            opened(&mut host, &ids::routine_delete("sch-1-2")),
            Command::AskDeleteRoutine { routine_id } if routine_id == "sch-1-2"
        ));
        assert!(matches!(
            opened(&mut host, &ids::routine_rotate("sch-1-2")),
            Command::RotateRoutineWebhook { routine_id } if routine_id == "sch-1-2"
        ));
        assert!(matches!(
            opened(&mut host, &ids::routine_trigger_webhook("sch-1-2")),
            Command::AddRoutineTrigger { routine_id, trigger }
                if routine_id == "sch-1-2" && trigger == crate::state::NewTrigger::Webhook
        ));
    }

    /// An id nobody is showing is a wrong address, and saying so beats sending a call under it.
    #[test]
    fn a_routine_the_open_bot_does_not_have_is_not_a_target() {
        let mut host = host();
        host.routines = vec![routine("sch_1", "cron")];
        assert!(host.click("routine-sch_9").is_err());
        assert!(
            host.invoke("routine.delete", &serde_json::json!({ "id": "sch_9" }))
                .is_err()
        );
    }

    /// `routine.list` answers with the rows themselves, so a driver that has just made one can
    /// find its id without reading the tree.
    #[test]
    fn routine_list_answers_with_the_rows() {
        let mut host = host();
        host.routines = vec![routine("sch_1", "cron"), routine("sch_2", "webhook")];
        let answer = host
            .invoke("routine.list", &serde_json::json!({}))
            .unwrap()
            .value
            .unwrap();
        let rows = answer["routines"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], "sch_1");
        assert_eq!(rows[0]["cron"], "0 9 * * *");
        assert_eq!(rows[1]["kind"], "webhook");
        assert_eq!(rows[1]["webhook_url"], "https://og.example/hooks/sch_2");
        assert_eq!(
            rows[1]["webhook_key"], "og_live_abc",
            "a driver that cannot read the key cannot fire the hook or tell a rotation happened"
        );
        assert_eq!(rows[0]["webhook_key"], serde_json::Value::Null);
        assert!(host.take_command().is_none(), "a listing changes nothing");
    }

    /// A cron routine with no line is a routine that never fires, which is worth refusing
    /// before it is made rather than reading off the listing afterwards.
    #[test]
    fn routine_create_wants_a_line_for_a_cron_routine() {
        let mut host = host();
        let error = host
            .invoke(
                "routine.create",
                &serde_json::json!({ "kind": "cron", "prompt": "Read the inbox" }),
            )
            .unwrap_err();
        assert!(error.contains("cron"), "{error}");

        host.invoke(
            "routine.create",
            &serde_json::json!({ "kind": "cron", "prompt": "Read the inbox", "cron": "0 9 * * *" }),
        )
        .unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::CreateRoutine { kind, prompt, cron }
                if kind == crate::opengrok::ScheduleKind::Cron
                    && prompt == "Read the inbox"
                    && cron.as_deref() == Some("0 9 * * *")
        ));
    }

    /// A webhook has no clock, so a line sent with one is dropped rather than sent to a server
    /// with no way to honour it.
    #[test]
    fn a_webhook_routine_is_created_without_a_line() {
        let mut host = host();
        host.invoke(
            "routine.create",
            &serde_json::json!({ "kind": "webhook", "prompt": "Deal with it", "cron": "0 9 * * *" }),
        )
        .unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::CreateRoutine { kind, cron, .. }
                if kind == crate::opengrok::ScheduleKind::Webhook && cron.is_none()
        ));
    }

    fn skill(id: &str, name: &str, description: &str) -> SkillSnap {
        SkillSnap {
            id: id.into(),
            name: name.into(),
            source: "authored",
            description: description.into(),
            draft: false,
            enabled: true,
        }
    }

    fn open_skill(id: &str, name: &str) -> OpenSkillSnap {
        OpenSkillSnap {
            id: id.into(),
            name: name.into(),
            body: "Ask for the receipt first.".into(),
            arrived: true,
            updated_at_ms: 1_700_000_000_000,
            enabled: true,
            approved_at_ms: Some(1_700_000_000_000),
            switching: false,
            error: None,
        }
    }

    /// A host with Settings open on Skills and two of them listed.
    fn skills_host() -> NativeChatHost {
        let mut host = host();
        host.account_open = true;
        host.skills_tab = true;
        host.skills = vec![
            skill("skl_1", "expense-report", "How we file expenses"),
            skill(
                "skl_2",
                "new-hire",
                "First week for somebody who just joined",
            ),
        ];
        host.skills_counts = crate::state::SkillCounts {
            yours: 2,
            discover: 5,
        };
        host
    }

    /// The tree is the page: both sides of the toggle with their counts, the search field with
    /// what is in it, and a row per skill carrying what that skill is for.
    #[test]
    fn the_skills_tab_reads_as_the_page_reads() {
        let host = skills_host();
        let tree = host.snapshot();
        assert_eq!(
            tree.find(SkillScope::Yours.element_id()).unwrap().name,
            "Yours 2"
        );
        let discover = tree.find(SkillScope::Discover.element_id()).unwrap();
        assert_eq!(discover.name, "Discover 5");
        assert!(
            !discover.states.contains(&"selected".to_string()),
            "the side that is not open is not the one marked"
        );
        assert!(
            tree.find(SkillScope::Yours.element_id())
                .unwrap()
                .states
                .contains(&"selected".to_string())
        );
        let row = tree.find(&ids::skill("skl_1")).unwrap();
        assert_eq!(row.name, "expense-report");
        assert_eq!(
            row.value.as_deref(),
            Some("How we file expenses"),
            "the description is what the model chooses by, so it is what a driver can assert on"
        );
        assert!(row.states.contains(&"authored".to_string()));
        assert!(tree.find(&ids::skill_delete("skl_1")).is_some());
        assert!(tree.find(ids::SKILLS_EMPTY).is_none());
    }

    /// A skill with no prose in it cannot be invoked, and that is the one thing worth knowing
    /// about it — so it is a state, not something to be worked out from a count.
    #[test]
    fn a_draft_says_so_on_its_row() {
        let mut host = skills_host();
        host.skills[1].draft = true;
        host.skills[1].source = "uploaded";
        let tree = host.snapshot();
        assert!(
            !tree
                .find(&ids::skill("skl_1"))
                .unwrap()
                .states
                .contains(&"draft".to_string())
        );
        let draft = tree.find(&ids::skill("skl_2")).unwrap();
        assert!(draft.states.contains(&"draft".to_string()));
        assert!(draft.states.contains(&"uploaded".to_string()));

        // And the switch, which the row wears as a chip: after this branch most rows in a
        // library are off, because every lesson a model writes from a recording starts off.
        assert!(
            !draft.states.contains(&"off".to_string()),
            "a draft is not a skill that was switched off"
        );
        host.skills[1].enabled = false;
        assert!(
            host.snapshot()
                .find(&ids::skill("skl_2"))
                .unwrap()
                .states
                .contains(&"off".to_string())
        );
    }

    /// The tree lists the rows the search leaves, exactly the rows the screen has: a driver
    /// shown a row nobody can see would click something that is not there.
    #[test]
    fn the_search_leaves_the_driver_the_same_rows_it_leaves_the_screen() {
        let mut host = skills_host();
        host.dispatch(&Op::Invoke {
            name: "skills.search".into(),
            args: serde_json::json!({ "q": "joined" }),
        })
        .unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetSkillsQuery(query) if query == "joined"
        ));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::SKILLS_SEARCH).unwrap().value.as_deref(),
            Some("joined")
        );
        assert!(tree.find(&ids::skill("skl_2")).is_some());
        assert!(tree.find(&ids::skill("skl_1")).is_none());
        // Nothing left is a sentence about the search, not about the library.
        host.skills_query = "payroll".into();
        assert_eq!(
            host.snapshot().find(ids::SKILLS_EMPTY).unwrap().name,
            "No skills match."
        );
    }

    /// The pane stays on the open skill whatever the list is showing — a search that hides its
    /// row, and a toggle moved to the side the skill is not on. The screen keeps it on the open
    /// id alone, and a tree that read it off the rows would go blank where the person sees a
    /// pane.
    #[test]
    fn the_open_skill_keeps_its_pane_whatever_the_list_shows() {
        let mut host = skills_host();
        host.skill_open = Some(open_skill("skl_1", "expense-report"));
        host.skills_query = "joined".into();
        let tree = host.snapshot();
        assert!(tree.find(&ids::skill("skl_1")).is_none());
        let pane = tree.find(&ids::skill_detail("skl_1")).unwrap();
        assert_eq!(pane.name, "expense-report");
        assert!(tree.find(&ids::skill_updated("skl_1")).is_some());
        assert!(tree.find(ids::SKILL_CLOSE).is_some());

        // The other side of the toggle: the open skill is not among the rows at all.
        host.skills_query = String::new();
        host.skills.clear();
        host.skills_scope = SkillScope::Discover;
        assert!(
            host.snapshot().find(&ids::skill_detail("skl_1")).is_some(),
            "the pane is on screen, so it is in the tree"
        );
    }

    /// What a skill SAYS is on its pane and nowhere else — a row carries what it is FOR. A
    /// driver that has just written one down reads it back here or not at all.
    #[test]
    fn the_pane_carries_the_prose_and_tells_waiting_from_refused() {
        let mut host = skills_host();
        let mut open = open_skill("skl_1", "expense-report");
        open.body = String::new();
        open.arrived = false;
        host.skill_open = Some(open);
        let tree = host.snapshot();
        assert_eq!(tree.find(ids::SKILL_LOADING).unwrap().name, "Loading…");
        assert!(tree.find(&ids::skill_body("skl_1")).is_none());
        assert!(tree.find(ids::SKILL_ERROR).is_none());
        assert!(
            tree.find(&ids::skill_updated("skl_1")).is_none(),
            "the pane draws that row once the fetch has answered, and not before"
        );

        host.skill_open = Some(open_skill("skl_1", "expense-report"));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(&ids::skill_body("skl_1"))
                .unwrap()
                .value
                .as_deref(),
            Some("Ask for the receipt first.")
        );
        assert!(tree.find(ids::SKILL_LOADING).is_none());

        // A skill that arrived with no prose in it is a draft, not a fetch that is still
        // going: the pane says the sentence a person reads, and says it is not waiting.
        let mut draft = open_skill("skl_1", "expense-report");
        draft.body = String::new();
        host.skill_open = Some(draft);
        let tree = host.snapshot();
        assert!(tree.find(ids::SKILL_LOADING).is_none());
        let body = tree.find(&ids::skill_body("skl_1")).unwrap();
        assert_eq!(body.name, NOTHING_WRITTEN_YET);
        assert_eq!(body.value.as_deref(), Some(""));

        let mut refused = open_skill("skl_1", "expense-report");
        refused.body = String::new();
        refused.arrived = false;
        refused.error = Some("no such skill".into());
        host.skill_open = Some(refused);
        let tree = host.snapshot();
        assert_eq!(tree.find(ids::SKILL_ERROR).unwrap().name, "no such skill");
        assert!(
            tree.find(ids::SKILL_LOADING).is_none(),
            "a pane that was refused is not a pane that is still coming"
        );
        assert!(
            tree.find(ids::SKILLS_ERROR).is_none(),
            "the pane's refusal is the pane's; the list was refused nothing"
        );
    }

    /// The sheet's refusal is the sheet's, over the fields it is about. The list's slot says
    /// nothing about a create, and the three fields point at the verb that takes their values.
    #[test]
    fn the_sheet_carries_its_own_refusal_and_its_fields_name_the_verb() {
        let mut host = skills_host();
        host.skill_add_open = true;
        host.skill_add_error = Some("that name is taken".into());
        host.skills_error = Some("the library would not load".into());
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::SKILL_ADD_ERROR).unwrap().name,
            "that name is taken"
        );
        assert_eq!(
            tree.find(ids::SKILLS_ERROR).unwrap().name,
            "the library would not load",
            "two slots, because they are two places on screen"
        );
        assert_eq!(tree.find(ids::SKILL_SHEET_SAVE).unwrap().name, "Save");

        for field in [
            ids::SKILL_SHEET_NAME,
            ids::SKILL_SHEET_DESCRIPTION,
            ids::SKILL_SHEET_BODY,
        ] {
            let refused = host
                .dispatch(&Op::type_text(field, "expense-report"))
                .unwrap_err();
            assert!(refused.contains("skill.create"), "{refused}");
        }

        host.skill_saving = true;
        assert_eq!(
            host.snapshot().find(ids::SKILL_SHEET_SAVE).unwrap().name,
            "Saving…",
            "a Save that is still working is not a Save that can be pressed again"
        );
    }

    /// These ids sit in the tree under a dialog that may be shut. One of them answering while
    /// the person is on another tab is a click nobody could have made.
    #[test]
    fn a_skills_control_off_screen_is_refused() {
        let mut host = skills_host();
        host.skills_tab = false;
        host.updates_tab = true;
        let refused = host.click(ids::SKILL_RECORD).unwrap_err();
        assert!(refused.contains("skills.open"), "{refused}");
        assert!(
            host.click(&ids::skill("skl_1"))
                .unwrap_err()
                .contains("skills.open")
        );
        // A click is not the only thing that claims to have landed on an element. The search
        // field is not in the tree while the page is shut, and typing into what is not there
        // must not quietly become a change to the page.
        for op in [
            Op::type_text(ids::SKILLS_SEARCH, "expense"),
            Op::key(ids::SKILLS_SEARCH, "Backspace"),
            Op::key(ids::SKILLS_SEARCH, "Enter"),
            Op::SetValue {
                target: ids::SKILLS_SEARCH.into(),
                value: "expense".into(),
            },
        ] {
            let refused = host.dispatch(&op).unwrap_err();
            assert!(refused.contains("skills.open"), "{refused}");
        }
        assert!(host.skills_query.is_empty(), "nothing was typed anywhere");
        assert!(host.take_command().is_none());
        // The tab itself is how the page is reached, so it answers from anywhere in Settings.
        host.click(ids::SETTINGS_SKILLS).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetAppSettingsTab(AppSettingsTab::Skills)
        ));
    }

    /// Delete asks first, as every recipe does. The menu it is chosen from opens under the
    /// pointer with Delete one row under Open.
    #[test]
    fn deleting_a_skill_asks_before_it_deletes() {
        let mut host = skills_host();
        host.click(&ids::skill_delete("skl_1")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::AskSkillDelete { id } if id == "skl_1"
        ));
        // The pane's Delete is its own control with its own id, and asks the same question.
        host.click(&ids::skill_detail_delete("skl_1")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::AskSkillDelete { id } if id == "skl_1"
        ));
        assert_ne!(
            ids::skill_delete("skl_1"),
            ids::skill_detail_delete("skl_1")
        );

        // Nothing is deleted until the dialog is answered, and there is no answering a dialog
        // that is not up.
        assert!(host.click(ids::SKILL_DELETE_CONFIRM).is_err());
        assert!(
            host.invoke("skill.delete.confirm", &serde_json::json!({}))
                .is_err()
        );
        host.skill_delete_confirm = Some("expense-report".into());
        assert_eq!(
            host.snapshot().find(ids::SKILL_DELETE_SHEET).unwrap().name,
            "Delete expense-report?"
        );
        host.click(ids::SKILL_DELETE_CONFIRM).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::ConfirmSkillDelete
        ));
        host.click(ids::SKILL_DELETE_CANCEL).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::CloseSkillDeleteConfirm
        ));
    }

    /// A container is not a row. Stripping `settings-skill-delete-` and calling what is left an
    /// id sent the delete dialog's own two ids looking for skills called `confirm` and `sheet`,
    /// and the dialog could not be answered at all.
    #[test]
    fn a_dialogs_own_id_is_never_read_as_a_rows() {
        let mut host = skills_host();
        for container in [ids::SKILL_DELETE_SHEET, ids::SKILL_SHEET] {
            let refused = host.click(container).unwrap_err();
            assert!(
                !refused.contains("no skill"),
                "`{container}` is a dialog, not a row: {refused}"
            );
        }
        // And an id that is shaped like a row's and names nothing is still a wrong address,
        // which is worth saying plainly.
        assert!(
            host.click(&ids::skill_delete("skl_9"))
                .unwrap_err()
                .contains("no skill `skl_9`")
        );
    }

    /// A second create while the first is still going is told so. The person cannot ask twice —
    /// Save goes dead — but a driver told nothing would conclude its skill was taken.
    #[test]
    fn a_create_while_one_is_saving_is_refused() {
        let mut host = skills_host();
        host.skill_saving = true;
        for (verb, args) in [
            (
                "skill.create",
                serde_json::json!({ "name": "expense-report" }),
            ),
            ("skill.upload", serde_json::json!({ "path": "/tmp/skill" })),
        ] {
            let refused = host.invoke(verb, &args).unwrap_err();
            assert_eq!(refused, "a skill is already being saved");
        }
        assert!(host.take_command().is_none());
        // And the page says it is going, where an upload has no sheet to say it in.
        host.skill_add_open = false;
        assert_eq!(
            host.snapshot().find(ids::SKILLS_SAVING).unwrap().name,
            "Uploading…"
        );
        host.skill_add_open = true;
        assert!(
            host.snapshot().find(ids::SKILLS_SAVING).is_none(),
            "the sheet's own Save says so while it is the sheet that is saving"
        );
    }

    /// The "…" is not in the tree — the row publishes itself and its Delete, and the menu's two
    /// rows are what a driver clicks. But it is on screen, and an id read off the screen is an
    /// id a driver will try, so it answers with the two it should have used.
    #[test]
    fn a_rows_menu_says_what_to_click_instead() {
        let mut host = skills_host();
        let refused = host.click("settings-skill-menu-skl_1").unwrap_err();
        assert!(refused.contains(&ids::skill_open("skl_1")), "{refused}");
        assert!(refused.contains(&ids::skill_delete("skl_1")), "{refused}");
    }

    /// Each control goes where it says it goes, and the two that open something the driver
    /// cannot work say what to use instead rather than opening it.
    #[test]
    fn the_skills_controls_go_where_they_say() {
        let mut host = skills_host();
        let opened = |host: &mut NativeChatHost, target: &str| {
            host.click(target).unwrap();
            host.take_command().unwrap()
        };
        assert!(matches!(
            opened(&mut host, &ids::skill("skl_2")),
            Command::OpenSkill(id) if id == "skl_2"
        ));
        assert!(matches!(
            opened(&mut host, &ids::skill_open("skl_2")),
            Command::OpenSkill(id) if id == "skl_2"
        ));
        assert!(matches!(
            opened(&mut host, &ids::skill_delete("skl_1")),
            Command::AskSkillDelete { id } if id == "skl_1"
        ));
        assert!(matches!(
            opened(&mut host, SkillScope::Discover.element_id()),
            Command::SetSkillsScope(SkillScope::Discover)
        ));
        assert!(matches!(
            opened(&mut host, ids::SKILL_WRITE),
            Command::OpenSkillAdd
        ));
        assert!(matches!(
            opened(&mut host, ids::SKILL_CLOSE),
            Command::CloseSkill
        ));
        assert!(matches!(
            opened(&mut host, ids::SETTINGS_SKILLS),
            Command::SetAppSettingsTab(AppSettingsTab::Skills)
        ));
        // The picker and the sheet's fields belong to the window; the verbs take the values.
        assert!(
            host.click(ids::SKILL_UPLOAD)
                .unwrap_err()
                .contains("skill.upload")
        );
        assert!(
            host.click(ids::SKILL_SHEET_SAVE)
                .unwrap_err()
                .contains("skill.create")
        );
        assert!(host.click(ids::SKILL_ADD).unwrap_err().contains("menu"));
    }

    /// The two ways nobody has built are in the tree with the sentence saying what would have
    /// to be built, and clicking one answers with that sentence rather than doing nothing.
    #[test]
    fn the_ways_nobody_built_say_what_is_missing() {
        let mut host = skills_host();
        let tree = host.snapshot();
        assert!(
            tree.find(ids::SKILL_WITH_BOT)
                .unwrap()
                .name
                .starts_with("Not yet")
        );
        // The half of Record that was missing is built, and the sentence says where it is
        // rather than asking for a model that now exists: what this page still cannot do is
        // record the Mac somebody is sitting in front of.
        let record = &tree.find(ids::SKILL_RECORD).unwrap().name;
        assert!(record.contains("Teach a task"), "{record}");
        assert!(record.contains("this Mac"), "{record}");
        assert!(
            host.click(ids::SKILL_RECORD)
                .unwrap_err()
                .starts_with("Not yet")
        );
        assert!(
            host.take_command().is_none(),
            "nothing was asked of the app"
        );
    }

    /// Every verb parses its args, and a missing one is named rather than guessed at.
    #[test]
    fn the_skills_verbs_take_what_they_need() {
        let mut host = skills_host();
        let done = |host: &mut NativeChatHost, name: &str, args: serde_json::Value| {
            host.invoke(name, &args).unwrap();
            host.take_command().unwrap()
        };
        assert!(matches!(
            done(&mut host, "skills.open", serde_json::json!({})),
            Command::OpenSkills
        ));
        assert!(matches!(
            done(
                &mut host,
                "skills.scope",
                serde_json::json!({"scope": "discover"})
            ),
            Command::SetSkillsScope(SkillScope::Discover)
        ));
        assert!(matches!(
            done(&mut host, "skill.open", serde_json::json!({"id": "skl_1"})),
            Command::OpenSkill(id) if id == "skl_1"
        ));
        assert!(matches!(
            done(
                &mut host,
                "skill.create",
                serde_json::json!({"name": "expense-report", "body": "Ask for the receipt."})
            ),
            Command::CreateSkill { name, description, body }
                if name == "expense-report"
                    && description.is_empty()
                    && body == "Ask for the receipt."
        ));
        assert!(matches!(
            done(&mut host, "skill.upload", serde_json::json!({"path": "/tmp/skill"})),
            Command::UploadSkill { path } if path == "/tmp/skill"
        ));
        // Delete asks: the verb opens the dialog, and answering it is its own step.
        assert!(matches!(
            done(&mut host, "skill.delete", serde_json::json!({"id": "skl_2"})),
            Command::AskSkillDelete { id } if id == "skl_2"
        ));

        assert_eq!(
            host.invoke("skill.create", &serde_json::json!({}))
                .unwrap_err(),
            "skill.create requires arg name"
        );
        assert_eq!(
            host.invoke("skill.upload", &serde_json::json!({}))
                .unwrap_err(),
            "skill.upload requires arg path"
        );
        assert_eq!(
            host.invoke("skill.delete", &serde_json::json!({}))
                .unwrap_err(),
            "skill.delete requires arg id"
        );
        assert!(
            host.invoke("skills.scope", &serde_json::json!({"scope": "mine"}))
                .unwrap_err()
                .contains("yours, discover"),
            "the toggle is named by what it says, not by the word on the wire"
        );
    }

    /// An id nobody is showing is a wrong address, and saying so beats sending a delete under
    /// it.
    #[test]
    fn a_skill_the_page_does_not_have_is_not_a_target() {
        let mut host = skills_host();
        assert!(host.click(&ids::skill("skl_9")).is_err());
        assert!(host.click(&ids::skill_delete("skl_9")).is_err());
        assert!(
            host.invoke("skill.delete", &serde_json::json!({"id": "skl_9"}))
                .unwrap_err()
                .contains("no skill `skl_9`")
        );
    }

    /// The one control that makes the rest of this mean anything: a lesson a model wrote is off
    /// until somebody reads it, and this is what they press when they have. It works both ways —
    /// the server keeps one flag and somebody who reads a lesson they do not want puts it back.
    #[test]
    fn the_switch_says_where_it_is_and_turns_a_taught_skill_on() {
        let mut host = skills_host();
        let mut open = open_skill("skl_1", "invoice-lookup");
        open.enabled = false;
        open.approved_at_ms = None;
        host.skill_open = Some(open);

        let switch = host
            .snapshot()
            .find(&ids::skill_enabled("skl_1"))
            .cloned()
            .expect("the pane draws it, so the tree carries it");
        assert_eq!(switch.name, "Switched off");
        assert_eq!(switch.checked, Some(false));
        assert!(
            switch.states.contains(&"unread".to_string()),
            "nobody has ever switched it on: {:?}",
            switch.states
        );

        // On. The click toggles, the way a finger does, so what it is now is read off the pane.
        host.click(&ids::skill_enabled("skl_1")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetSkillEnabled { id, enabled } if id == "skl_1" && enabled
        ));

        // And back off, for somebody who read it and did not want it. Once it has been switched
        // on the stamp stands, so it is no longer waiting to be read.
        let mut read = open_skill("skl_1", "invoice-lookup");
        read.enabled = true;
        read.approved_at_ms = Some(1_758_000_000_000);
        host.skill_open = Some(read);
        let switch = host
            .snapshot()
            .find(&ids::skill_enabled("skl_1"))
            .cloned()
            .expect("still there");
        assert_eq!(switch.name, "Switched on");
        assert_eq!(switch.checked, Some(true));
        assert!(!switch.states.contains(&"unread".to_string()));
        host.click(&ids::skill_enabled("skl_1")).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetSkillEnabled { id, enabled } if id == "skl_1" && !enabled
        ));
    }

    /// The pane and the tree draw one fact from one function. They used to work it out
    /// separately and disagreed: the screen only tells the two kinds of off apart while a skill
    /// IS off, and the tree called every skill on a server that sends no stamp unread —
    /// including the ones somebody had switched on.
    #[test]
    fn the_tree_and_the_pane_agree_about_which_skills_are_waiting_to_be_read() {
        let mut host = skills_host();
        for (enabled, approved_at_ms) in [
            (false, None),
            (false, Some(1_758_000_000_000)),
            (true, None),
            (true, Some(1_758_000_000_000)),
        ] {
            let mut open = open_skill("skl_1", "invoice-lookup");
            open.enabled = enabled;
            open.approved_at_ms = approved_at_ms;
            host.skill_open = Some(open);
            let unread = host
                .snapshot()
                .find(&ids::skill_enabled("skl_1"))
                .expect("the switch is on the pane")
                .states
                .contains(&"unread".to_string());
            let said =
                crate::components::skills::use_line("invoice-lookup", enabled, approved_at_ms)
                    .unwrap_or_default();
            assert_eq!(
                unread,
                said.contains("until somebody reads it"),
                "enabled={enabled} approved={approved_at_ms:?}: the tree says {unread} and the \
                 pane says {said:?}"
            );
        }
    }

    /// The switch is the open skill's and nobody else's, it is dead while the server is being
    /// told, and a refusal is drawn beside it rather than in place of the pane — the pane is
    /// still there, and so is the skill it is about.
    #[test]
    fn the_switch_is_refused_when_there_is_nothing_for_it_to_be_about() {
        let mut host = skills_host();
        assert!(
            host.click(&ids::skill_enabled("skl_1"))
                .unwrap_err()
                .contains("no skill `skl_1` is open to switch"),
            "a pane nobody opened has no switch on it"
        );

        // Still being fetched: the pane is a line saying so, and there is nothing to press.
        let mut coming = open_skill("skl_1", "invoice-lookup");
        coming.arrived = false;
        host.skill_open = Some(coming);
        assert!(host.snapshot().find(&ids::skill_enabled("skl_1")).is_none());
        assert!(
            host.click(&ids::skill_enabled("skl_1"))
                .unwrap_err()
                .contains("still being fetched")
        );

        // With the server. Pressed twice, two answers about one flag are in the air.
        let mut switching = open_skill("skl_1", "invoice-lookup");
        switching.switching = true;
        host.skill_open = Some(switching);
        assert!(
            !host
                .snapshot()
                .find(&ids::skill_enabled("skl_1"))
                .unwrap()
                .enabled
        );
        assert_eq!(
            host.click(&ids::skill_enabled("skl_1")).unwrap_err(),
            SWITCH_IN_FLIGHT,
            "and in the same words the app refuses a second switch with"
        );

        // Refused: the switch has already gone back, and the reason is on the pane with it.
        let mut refused = open_skill("skl_1", "invoice-lookup");
        refused.error = Some("Only the owner can switch a skill on.".into());
        host.skill_open = Some(refused);
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::SKILL_ERROR).unwrap().name,
            "Only the owner can switch a skill on.",
            "the server's sentence, beside the control it is about"
        );
        assert!(
            tree.find(&ids::skill_enabled("skl_1")).is_some(),
            "and the pane is still a pane: the skill did not go anywhere"
        );
        assert!(tree.find(&ids::skill_body("skl_1")).is_some());
        assert!(host.take_command().is_none());
    }

    /// A lesson being written from a tape takes a model call and a wait, and it happens in the
    /// coworker's screen window — which has no tree. Without these three nodes a driver sees an
    /// app that does nothing for a minute and then an app that has done nothing.
    #[test]
    fn the_driver_sees_a_lesson_being_written_and_what_came_of_it() {
        let mut host = host();
        assert!(
            host.snapshot().find(ids::TEACH_SKILL_WRITING).is_none(),
            "nothing at all until somebody teaches one"
        );

        host.taught_skill = Some(TaughtSkill::Writing);
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::TEACH_SKILL_WRITING).unwrap().name,
            WRITING_A_LESSON,
            "the sentence the sheet's own button is showing in the other window"
        );
        assert!(tree.find(ids::TEACH_SAVED_SKILL).is_none());
        assert!(tree.find(ids::TEACH_RETRY).is_none());

        host.taught_skill = Some(TaughtSkill::Written {
            id: "skl_7".into(),
            name: "invoice-lookup".into(),
            enabled: false,
        });
        let tree = host.snapshot();
        let written = tree.find(ids::TEACH_SAVED_SKILL).unwrap();
        assert_eq!(
            written.name, "invoice-lookup",
            "named by the word that has to be typed after a slash"
        );
        assert_eq!(written.value.as_deref(), Some("skl_7"));
        assert!(
            written.states.contains(&"off".to_string()),
            "born switched off, because nobody has read it: {:?}",
            written.states
        );
        assert!(tree.find(ids::TEACH_SKILL_WRITING).is_none());

        host.taught_skill = Some(TaughtSkill::Written {
            id: "skl_7".into(),
            name: "invoice-lookup".into(),
            enabled: true,
        });
        assert!(
            !host
                .snapshot()
                .find(ids::TEACH_SAVED_SKILL)
                .unwrap()
                .states
                .contains(&"off".to_string()),
            "and not off once it is on"
        );
    }

    /// A refused upload is the server's sentence and, where there is any point, one thing to do
    /// about it. The tape is not gone — the server keeps none — so Try again sends the same
    /// bytes rather than asking somebody to record minutes of work a second time.
    #[test]
    fn a_refused_lesson_keeps_the_sentence_and_offers_the_tape_again() {
        let mut host = host();
        host.taught_skill = Some(TaughtSkill::Refused {
            why: "Your bot hung up on the way back. Nothing was stored.".into(),
            next: AfterRefusal::SendAgain,
            coworker: "cw_1".into(),
        });
        host.taught_tape_in_hand = true;
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::TEACH_SKILL_ERROR).unwrap().name,
            "Your bot hung up on the way back. Nothing was stored.",
            "the server's own words, which name which of the things went wrong"
        );
        assert_eq!(tree.find(ids::TEACH_RETRY).unwrap().name, "Try again");

        host.click(ids::TEACH_RETRY).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::RetryTaughtSkill
        ));
    }

    /// A refusal a second identical send cannot change — a spend cap, a recording nothing can be
    /// written from — keeps its sentence and loses the button. A driver offered a retry there
    /// would loop against a machine whose answer is already written above it.
    #[test]
    fn a_refusal_a_retry_cannot_change_offers_no_retry() {
        let mut host = host();
        host.taught_skill = Some(TaughtSkill::Refused {
            why: "This coworker is over its spend limit for the month.".into(),
            next: AfterRefusal::Nothing,
            coworker: "cw_1".into(),
        });
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::TEACH_SKILL_ERROR).unwrap().name,
            "This coworker is over its spend limit for the month.",
            "the sentence is the server's either way, and it is not softened"
        );
        assert!(
            tree.find(ids::TEACH_RETRY).is_none(),
            "no button under a sentence that says a second try is pointless"
        );
        let refused = host.click(ids::TEACH_RETRY).unwrap_err();
        assert!(
            refused.contains("cannot change the answer"),
            "and a driver that clicks it anyway is told why: {refused}"
        );
        assert!(
            refused.contains("spend limit"),
            "with the server's own reason in it: {refused}"
        );
        assert!(host.take_command().is_none());
    }

    /// The refusal nobody can answer: the connection dropped, so the skill may have been
    /// written and may not. A Try again there would write a second skill from one recording —
    /// the route has no idempotency key — so what is offered is the library, and the driver is
    /// told the same thing in the same words if it reaches for the retry anyway.
    #[test]
    fn an_answer_that_never_arrived_offers_the_library_and_not_the_tape() {
        let mut host = host();
        host.taught_skill = Some(TaughtSkill::Refused {
            why: "The connection dropped before the answer came back, and your bot may have \
                  written the skill anyway."
                .into(),
            next: AfterRefusal::LookFirst,
            coworker: "cw_1".into(),
        });
        host.taught_tape_in_hand = true;
        let tree = host.snapshot();
        assert!(
            tree.find(ids::TEACH_SKILL_ERROR)
                .unwrap()
                .name
                .contains("may have written the skill")
        );
        assert!(
            tree.find(ids::TEACH_RETRY).is_none(),
            "the tape is in hand and must still not be sent: one recording, one skill"
        );
        assert_eq!(
            tree.find(ids::TEACH_CHECK_LIBRARY).unwrap().name,
            "Check Skills"
        );

        let refused = host.click(ids::TEACH_RETRY).unwrap_err();
        assert!(refused.contains("nobody knows"), "{refused}");
        assert!(
            refused.contains(ids::TEACH_CHECK_LIBRARY),
            "and what to press instead: {refused}"
        );
        assert!(host.take_command().is_none());

        host.click(ids::TEACH_CHECK_LIBRARY).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::OpenTaughtSkill { id } if id.is_none()
        ));
    }

    /// The tape lives in the screen window, and closing that window takes it with it. The offer
    /// goes when the tape does: a driver told its click worked, on a window that is not there,
    /// would wait for a skill nothing is writing.
    #[test]
    fn a_retry_goes_when_the_window_holding_the_tape_does() {
        let mut host = host();
        host.taught_skill = Some(TaughtSkill::Refused {
            why: "Your bot hung up on the way back.".into(),
            next: AfterRefusal::SendAgain,
            coworker: "cw_1".into(),
        });
        host.taught_tape_in_hand = false;
        assert!(
            host.snapshot().find(ids::TEACH_RETRY).is_none(),
            "no window, no bytes, no button"
        );
        let refused = host.click(ids::TEACH_RETRY).unwrap_err();
        assert!(refused.contains("has been closed"), "{refused}");
        assert!(host.take_command().is_none());

        host.taught_tape_in_hand = true;
        assert!(host.snapshot().find(ids::TEACH_RETRY).is_some());
        host.click(ids::TEACH_RETRY).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::RetryTaughtSkill
        ));
    }

    /// Both controls are refused when there is nothing for them to be about: a driver working
    /// from a tree it read a moment ago must not press a button that has since gone.
    #[test]
    fn a_taught_skills_controls_are_refused_when_there_is_no_tape() {
        let mut host = host();
        assert_eq!(
            host.click(ids::TEACH_RETRY).unwrap_err(),
            "no refused recording is waiting to be sent again"
        );
        assert_eq!(
            host.click(ids::TEACH_SAVED_SKILL).unwrap_err(),
            "no taught skill is waiting to be read"
        );
        host.taught_skill = Some(TaughtSkill::Writing);
        assert!(
            host.click(ids::TEACH_RETRY).is_err(),
            "one that is still being written has not been refused"
        );
        assert!(
            host.take_command().is_none(),
            "nothing was asked of the app"
        );

        // Written, and the line is the way to the reading it is waiting for.
        host.taught_skill = Some(TaughtSkill::Written {
            id: "skl_7".into(),
            name: "invoice-lookup".into(),
            enabled: false,
        });
        host.click(ids::TEACH_SAVED_SKILL).unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::OpenTaughtSkill { id } if id.as_deref() == Some("skl_7")
        ));
        assert!(host.click(ids::TEACH_RETRY).is_err());
    }

    /// Typing into the field is typing into the field: the host keeps the copy the list filters
    /// by, so the tree answers with what was typed without a round trip through the app.
    #[test]
    fn the_skills_search_takes_keys_the_way_the_logins_one_does() {
        let mut host = skills_host();
        host.dispatch(&Op::type_text(ids::SKILLS_SEARCH, "expen"))
            .unwrap();
        assert_eq!(host.skills_query, "expen");
        host.dispatch(&Op::key(ids::SKILLS_SEARCH, "Backspace"))
            .unwrap();
        assert_eq!(host.skills_query, "expe");
        host.dispatch(&Op::SetValue {
            target: ids::SKILLS_SEARCH.into(),
            value: "new".into(),
        })
        .unwrap();
        assert_eq!(host.skills_query, "new");
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetSkillsQuery(query) if query == "new"
        ));
    }

    /// A host with a bot and a session, which is what the chat tree is drawn for.
    fn host() -> NativeChatHost {
        NativeChatHost {
            ready: true,
            signed_in: true,
            sessions: vec![SessionSnap {
                id: "bot-1".into(),
                title: "Ada".into(),
                active: true,
                preview: None,
                listed: false,
            }],
            ..Default::default()
        }
    }

    /// A computer's card as Settings → Computer draws it: the computer `id`, with its relay on, not
    /// relaying and awake, and asking each time. A test changes what it is about.
    fn a_card(id: &str, this_computer: bool) -> ComputerCard {
        ComputerCard {
            machine_id: id.into(),
            label: format!("NativeChat on {id}"),
            this_computer,
            online: true,
            mode: crate::opengrok::LocalExecMode::Ask,
            relay_on: true,
            switching: false,
            state: computers::RelayState::NotRelaying,
            tried: None,
            detail: None,
            note: None,
        }
    }

    #[test]
    fn floating_header_controls_match_their_visible_actions() {
        let mut host = host();
        host.sidebar_hidden = true;
        let tree = host.snapshot();
        for id in [
            ids::HEADER_LEFT_SIDEBAR,
            ids::HEADER_RIGHT_SIDEBAR,
            ids::HEADER_MONITOR,
        ] {
            assert!(tree.find(id).is_some(), "missing {id}");
        }
        host.dispatch(&Op::click(ids::HEADER_LEFT_SIDEBAR)).unwrap();
        assert!(matches!(host.take_command(), Some(Command::ToggleSidebar)));
        host.dispatch(&Op::click(ids::HEADER_MONITOR)).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleComputerPane)
        ));
        for computer_open in [false, true] {
            host.computer_open = computer_open;
            host.dispatch(&Op::click(ids::HEADER_RIGHT_SIDEBAR))
                .unwrap();
            let command = host.take_command();
            assert!(if computer_open {
                matches!(command, Some(Command::ToggleComputerPane))
            } else {
                matches!(command, Some(Command::ToggleAgentSettings))
            });
        }
        host.sessions.clear();
        let empty_tree = host.snapshot();
        assert!(empty_tree.find(ids::HEADER_MONITOR).is_none());
        for id in [ids::HEADER_LEFT_SIDEBAR, ids::HEADER_RIGHT_SIDEBAR] {
            assert!(empty_tree.find(id).is_some(), "missing {id} without a Bot");
        }
    }

    #[test]
    fn sidebar_has_one_visible_control_and_no_size_menu() {
        for empty in [false, true] {
            for hidden in [false, true] {
                let mut host = host();
                if empty {
                    host.sessions.clear();
                }
                host.sidebar_hidden = hidden;
                let tree = host.snapshot();
                assert_eq!(tree.find(ids::HEADER_LEFT_SIDEBAR).is_some(), hidden);
                assert_eq!(tree.find(ids::NAV_TOGGLE).is_some(), !hidden);
                for removed in [
                    "sidebar-mode-menu",
                    "sidebar-mode-expanded",
                    "sidebar-mode-mini",
                    "sidebar-mode-hide",
                ] {
                    assert!(tree.find(removed).is_none());
                    assert!(host.dispatch(&Op::click(removed)).is_err());
                }
                let (visible, absent) = if hidden {
                    (ids::HEADER_LEFT_SIDEBAR, ids::NAV_TOGGLE)
                } else {
                    (ids::NAV_TOGGLE, ids::HEADER_LEFT_SIDEBAR)
                };
                assert!(host.dispatch(&Op::click(absent)).is_err());
                host.dispatch(&Op::click(visible)).unwrap();
                assert!(matches!(host.take_command(), Some(Command::ToggleSidebar)));
            }
        }
    }

    fn recipe(values: &[(&str, &str)]) -> ActiveRecipe {
        ActiveRecipe {
            id: "rcp_1".into(),
            name: "Weekly report".into(),
            kind: RecipeKind::Recipe,
            parameters: vec![
                RecipeParameter {
                    name: "city".into(),
                    description: "Where".into(),
                    required: true,
                    kind: RecipeParameterKind::Text,
                    default: None,
                    values: None,
                },
                RecipeParameter {
                    name: "shorts".into(),
                    description: "Short ones only".into(),
                    required: false,
                    kind: RecipeParameterKind::Boolean,
                    default: None,
                    values: None,
                },
            ],
            values: values
                .iter()
                .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
                .collect(),
        }
    }

    fn keys(host: &mut NativeChatHost, op: Op) -> ComposePlan {
        host.dispatch(&op).unwrap();
        host.take_compose().unwrap()
    }

    #[test]
    fn a_trigger_is_typed_as_a_key_and_not_written_into_the_draft() {
        let mut host = host();
        let plan = keys(&mut host, Op::type_text(ids::COMPOSER, "/hi"));
        // `/` is the whole point: the composer takes it before the field does and opens its
        // panel, which only happens if it arrives as a key.
        assert_eq!(plan.keys, ["/", "h", "i"]);
        assert!(plan.focus_composer);
        assert!(host.take_command().is_none());
    }

    #[test]
    fn a_space_and_a_newline_are_the_keys_a_person_presses_for_them() {
        let mut host = host();
        let plan = keys(&mut host, Op::type_text(ids::COMPOSER, "a b\n"));
        assert_eq!(plan.keys, ["a", "space", "b", "enter"]);
    }

    #[test]
    fn set_value_on_the_composer_leaves_the_draft_that_typing_it_would() {
        let mut host = host();
        let replaced = keys(
            &mut host,
            Op::SetValue {
                target: ids::COMPOSER.into(),
                value: "/weekly".into(),
            },
        );
        let typed = keys(&mut host, Op::type_text(ids::COMPOSER, "/weekly"));
        // Replace is take-it-all, delete, then type: after the clearing the two press exactly
        // the same keys, so the field ends up holding the same thing either way.
        assert_eq!(replaced.keys[..2], [select_all_chord(), "backspace"]);
        assert_eq!(replaced.keys[2..], typed.keys[..]);
        assert!(replaced.focus_composer);
    }

    #[test]
    fn typing_with_no_target_goes_where_the_caret_is() {
        let mut host = host();
        // This is how the panel `/` opened is worked: its search field has the caret.
        for target in ["", "focused"] {
            let plan = keys(&mut host, Op::type_text(target, "we"));
            assert!(!plan.focus_composer, "{target}");
            assert_eq!(plan.keys, ["w", "e"]);
        }
        assert!(!keys(&mut host, Op::key("", "Enter")).focus_composer);
    }

    #[test]
    fn every_key_the_protocol_names_is_spelled_for_gpui() {
        let mut host = host();
        for (asked, token) in [
            ("Enter", "enter"),
            ("Backspace", "backspace"),
            ("Escape", "escape"),
            ("Tab", "tab"),
            ("Up", "up"),
            ("Down", "down"),
            ("Left", "left"),
            ("Right", "right"),
            ("ArrowUp", "up"),
            ("space", "space"),
        ] {
            let plan = keys(&mut host, Op::key(ids::COMPOSER, asked));
            assert_eq!(plan.keys, [token], "{asked}");
            assert!(plan.focus_composer, "{asked}");
        }
    }

    #[test]
    fn a_key_nobody_can_press_is_refused() {
        let mut host = host();
        let unknown = host.dispatch(&Op::key(ids::COMPOSER, "F13")).unwrap_err();
        assert!(unknown.to_lowercase().contains("f13"), "{unknown}");
        // A chord is `op keybinding`'s business: free-form keys stay modifier-free.
        let chord = host.dispatch(&Op::key(ids::COMPOSER, "cmd-q")).unwrap_err();
        assert!(chord.contains("keybinding"), "{chord}");
        // Neither left any keys behind for the window to press.
        assert!(host.take_compose().is_none());
    }

    #[test]
    fn a_target_that_holds_no_text_is_refused() {
        let mut host = host();
        for op in [
            Op::type_text(ids::SIDEBAR, "hi"),
            Op::key(ids::SIDEBAR, "Enter"),
            Op::SetValue {
                target: ids::SIDEBAR.into(),
                value: "hi".into(),
            },
        ] {
            let error = host.dispatch(&op).unwrap_err();
            assert!(error.contains("is not editable"), "{error}");
        }
        assert!(host.take_compose().is_none());
        assert!(host.take_command().is_none());
    }

    #[test]
    fn the_login_drafts_take_text_and_enter_signs_in() {
        let mut host = host();
        host.dispatch(&Op::type_text(ids::LOGIN_EMAIL, "ada@"))
            .unwrap();
        host.dispatch(&Op::type_text(ids::LOGIN_EMAIL, "example.com"))
            .unwrap();
        assert_eq!(host.login_email, "ada@example.com");
        // The page draws its own fields; this draft is the one `click login-submit` reads, so
        // typing and setting have to leave it saying the same thing.
        host.dispatch(&Op::SetValue {
            target: ids::LOGIN_PASSWORD.into(),
            value: "hunter2".into(),
        })
        .unwrap();
        host.dispatch(&Op::key(ids::LOGIN_PASSWORD, "Backspace"))
            .unwrap();
        assert_eq!(host.login_password, "hunter");
        host.dispatch(&Op::key(ids::LOGIN_EMAIL, "Enter")).unwrap();
        match host.take_command() {
            Some(Command::Login { email, password }) => {
                assert_eq!(email, "ada@example.com");
                assert_eq!(password, "hunter");
            }
            other => panic!("expected a login, got {other:?}"),
        }
        let refused = host
            .dispatch(&Op::key(ids::LOGIN_EMAIL, "Escape"))
            .unwrap_err();
        assert!(refused.contains("unhandled key"), "{refused}");
    }

    #[test]
    fn the_open_panel_and_its_rows_are_in_the_tree() {
        let mut host = host();
        assert!(host.snapshot().find(ids::COMPOSER_PANEL).is_none());

        // Nothing told yet: the list is everything the recipe needs.
        let untold = recipe(&[]);
        host.composer_panel = Some(PanelMode::Parameters);
        host.panel_rows = panel_rows(PanelMode::Parameters, &[], &listed(&[]), Some(&untold));
        let tree = host.snapshot();
        let panel = tree.find(ids::COMPOSER_PANEL).unwrap();
        assert!(tree.find(ids::COMPOSER_PANEL_SEARCH).unwrap().focused);
        // The ids are the composer's own, so a driver reads back what the person sees.
        let rows: Vec<&str> = panel
            .children
            .iter()
            .map(|child| child.id.as_str())
            .collect();
        assert!(rows.contains(&"composer-param-city"), "{rows:?}");
        assert!(rows.contains(&"composer-param-shorts"), "{rows:?}");

        // THE LIST IS WHAT IS LEFT TO TELL, NOT WHAT THE RECIPE HAS. Once a value is in, the
        // parameter leaves this list and lives in the bar above the composer, where it can still
        // be changed. A driver asserting on the panel must read it as the outstanding work.
        let told = recipe(&[("city", "London")]);
        host.panel_rows = panel_rows(PanelMode::Parameters, &[], &listed(&[]), Some(&told));
        let tree = host.snapshot();
        let rows: Vec<&str> = tree
            .find(ids::COMPOSER_PANEL)
            .unwrap()
            .children
            .iter()
            .map(|child| child.id.as_str())
            .collect();
        assert!(!rows.contains(&"composer-param-city"), "{rows:?}");
        assert!(rows.contains(&"composer-param-shorts"), "{rows:?}");
    }

    /// The whole point of the pill over the red line it replaced: it goes away, and a driver
    /// can say so. A transcript message is in the tree forever, because a transcript message is
    /// a thing that happened.
    #[test]
    fn the_reconnecting_pill_names_its_machine_and_leaves_the_tree_when_it_clears() {
        let mut host = host();
        assert!(host.snapshot().find("reconnect-banner").is_none());

        host.reconnect = Some((
            "Waiting for the model gateway… — OpenGrok is answering; the model gateway behind \
             it is not."
                .to_string(),
            "gateway",
        ));
        let node = host
            .snapshot()
            .find("reconnect-banner")
            .cloned()
            .expect("the pill is on screen, so it is in the tree");
        assert_eq!(node.states, vec!["gateway".to_string()]);
        assert!(node.name.contains("OpenGrok is answering"), "{}", node.name);

        host.reconnect = None;
        assert!(
            host.snapshot().find("reconnect-banner").is_none(),
            "`assert --exists false` is how a driver says the app reconnected"
        );
    }

    /// While the open Bot's replies go through the person's own plan, the Usage card says it
    /// does not count them (the server meters no turn on the person's plan): the Bot's own door,
    /// or the account's that it follows. Not on the server's keys, and not for a Bot whose door
    /// is the server's keys while the account is on the plan.
    #[test]
    fn the_usage_card_says_when_the_bots_replies_are_on_the_plan() {
        use crate::opengrok::{CoworkerSource, InferenceKind, InferenceSource};
        use crate::state::ReplySourceRead;
        let usage_line = |state: &AppState| {
            NativeChatHost::from_app(state)
                .snapshot()
                .find(ids::AGENT_USAGE_PLAN)
                .map(|node| node.name.clone())
        };
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(
                serde_json::json!({ "id": "cw_1", "name": "Ada", "source": null }),
            )
            .unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        assert_eq!(usage_line(&state), None, "nothing read");
        let read = |kind| {
            Some(ReplySourceRead::Read(InferenceSource {
                kind,
                base_url: Some("http://127.0.0.1:8080".into()),
                local_model: Some("gpt-5-codex".into()),
                healthy: true,
                has_api_key: false,
                via: None,
                relay: None,
                new_bot_default: None,
                relay_enabled: None,
                plan_fallback: None,
            }))
        };
        state.reply_source.kept = read(InferenceKind::Gateway);
        assert_eq!(usage_line(&state), None, "the server's keys");
        state.reply_source.kept = read(InferenceKind::LocalProxy);
        assert_eq!(
            usage_line(&state).as_deref(),
            Some(reply_source::PLAN_USAGE_NOTE),
            "the account's plan, which the Bot follows"
        );
        state.coworkers[0].source = CoworkerSource::Kind(InferenceKind::Gateway);
        assert_eq!(usage_line(&state), None, "the Bot's own door is the keys");
        state.reply_source.kept = read(InferenceKind::Gateway);
        state.coworkers[0].source = CoworkerSource::Kind(InferenceKind::LocalProxy);
        assert_eq!(
            usage_line(&state).as_deref(),
            Some(reply_source::PLAN_USAGE_NOTE),
            "the Bot's own plan"
        );
    }

    /// The signed-out banner appears, carries its own way out, and leaves when there is a
    /// session again.
    ///
    /// It is a separate node from the reconnecting pill and never the same one, because the two
    /// are opposite in the way that matters: the pill goes when the wire returns, and this goes
    /// only when somebody signs in. A driver that could not tell them apart is a driver that
    /// would have watched the bug happen and reported the app as reconnecting.
    #[test]
    fn the_signed_out_banner_carries_its_own_way_out_and_is_not_the_reconnecting_pill() {
        let mut host = host();
        assert!(host.snapshot().find("signed-out-banner").is_none());
        assert!(host.snapshot().find("signed-out-sign-in").is_none());
        let refused = host.dispatch(&Op::click("signed-out-sign-in")).unwrap_err();
        assert!(refused.contains("has a session"), "{refused}");

        host.signed_out = Some(
            "You are signed out. — OpenGrok no longer recognises this app, so nothing is being \
             sent. Sign in again to carry on."
                .to_string(),
        );
        let tree = host.snapshot();
        let node = tree
            .find("signed-out-banner")
            .expect("the banner is on screen, so it is in the tree");
        assert_eq!(node.states, vec!["signed-out".to_string()]);
        assert!(
            tree.find("reconnect-banner").is_none(),
            "nothing here is reconnecting, and saying so would send somebody to the wrong fix"
        );

        // The way out, which is the half a relaunch used to be.
        host.dispatch(&Op::click("signed-out-sign-in")).unwrap();
        assert!(matches!(host.take_command(), Some(Command::SignInAgain)));

        host.signed_out = None;
        let tree = host.snapshot();
        assert!(
            tree.find("signed-out-banner").is_none(),
            "`assert --exists false` is how a driver says the app has a session again"
        );
        assert!(tree.find("signed-out-sign-in").is_none());
    }

    /// A turn the person's Mac could not answer is offered again on the server's keys, and the
    /// offer is a click a driver can make, only while there is one; each held message the server
    /// holds for the Mac says so under the queue.
    #[test]
    fn a_turn_the_mac_could_not_answer_is_offered_on_the_server_and_a_wait_for_it_is_said() {
        let mut host = host();
        assert!(
            host.snapshot()
                .find(ids::RUN_ERROR_SEND_ON_SERVER)
                .is_none()
        );
        let refused = host.click(ids::RUN_ERROR_SEND_ON_SERVER).unwrap_err();
        assert!(refused.contains("person's plan"), "{refused}");
        host.can_send_on_server = true;
        assert_eq!(
            host.snapshot()
                .find(ids::RUN_ERROR_SEND_ON_SERVER)
                .map(|node| node.name.clone())
                .as_deref(),
            Some("Send this reply on Server instead")
        );
        host.click(ids::RUN_ERROR_SEND_ON_SERVER).unwrap();
        assert!(matches!(host.take_command(), Some(Command::SendOnServer)));

        host.queued_sends = 2;
        host.waiting_for_mac = vec!["m_1".into()];
        let tree = host.snapshot();
        let queued = tree.find(ids::COMPOSER_QUEUED).unwrap();
        assert_eq!(queued.name, "2 queued");
        assert_eq!(
            tree.find(&ids::queued_waiting("m_1"))
                .map(|node| node.name.as_str()),
            Some("Waiting for your Mac")
        );
        assert!(tree.find(&ids::queued_waiting("m_2")).is_none());
    }

    /// A turn that never left is offered again, and the offer is a click a driver can make.
    #[test]
    fn the_turn_that_never_left_is_offered_again_and_only_while_there_is_one() {
        let mut host = host();
        assert!(host.snapshot().find("retry-turn").is_none());
        let refused = host.dispatch(&Op::click("retry-turn")).unwrap_err();
        assert!(refused.contains("no turn to send again"), "{refused}");

        host.can_retry_turn = true;
        assert!(host.snapshot().find("retry-turn").is_some());
        host.dispatch(&Op::click("retry-turn")).unwrap();
        assert!(matches!(host.take_command(), Some(Command::RetryTurn)));
    }

    /// The button a driver has to be able to see change, and to press once it has. The person
    /// asked for two things — to know whether the bot is still doing something, and to be able
    /// to stop it — and this is both of them in one node.
    #[test]
    fn the_composers_button_says_which_of_its_two_it_is_and_is_clicked_only_as_the_stop() {
        let mut host = host();
        let idle = host.snapshot().find(ids::COMPOSER_SEND).cloned().unwrap();
        assert_eq!(idle.name, "Send message");
        assert!(idle.states.is_empty());
        let refused = host.dispatch(&Op::click(ids::COMPOSER_SEND)).unwrap_err();
        assert!(refused.contains("no turn to stop"), "{refused}");
        assert!(host.take_command().is_none());

        host.turn_in_flight = true;
        let running = host.snapshot().find(ids::COMPOSER_SEND).cloned().unwrap();
        assert_eq!(running.name, "Stop");
        assert!(running.states.contains(&"running".to_string()));
        host.dispatch(&Op::click(ids::COMPOSER_SEND)).unwrap();
        assert!(matches!(host.take_command(), Some(Command::StopTurn)));
    }

    #[test]
    fn a_recipe_row_carries_the_id_the_panel_gives_it() {
        let row = ComposerPanelRow::new("recipe:rcp_1", "icons/record.svg", "Weekly", "A task");
        assert_eq!(row_id(&row), "composer-panel-row-recipe:rcp_1");
        assert_eq!(
            row_id(&row.element_id("composer-param-city")),
            "composer-param-city"
        );
    }

    use crate::opengrok::SkillSummary;

    /// A library that has arrived, for a panel that is not waiting on anything.
    fn listed(skills: &[SkillSummary]) -> SkillLibrary<'_> {
        SkillLibrary {
            skills,
            loading: false,
            error: None,
        }
    }

    /// The `/` listing as a driver sees it: two recipes and two skills.
    fn slash_listing() -> (Vec<RecipeSummary>, Vec<SkillSummary>) {
        (
            serde_json::from_value(serde_json::json!([
                { "id": "rcp_tape", "name": "Weekly report", "kind": "recipe" },
                { "id": "rcp_tree", "name": "Search and retry", "kind": "workflow" }
            ]))
            .unwrap(),
            serde_json::from_value(serde_json::json!([
                {
                    "id": "skl_1", "name": "expense-report",
                    "description": "File a receipt the way the firm wants it"
                },
                { "id": "skl_2", "name": "half-written", "draft": true }
            ]))
            .unwrap(),
        )
    }

    /// What a driver needs to work `/`: the rows are there while the panel is open, and each
    /// one says which of the four kinds it is. Without the word, a workflow row and a recipe
    /// row are the same node under the same id prefix, and "pick the workflow" is a guess.
    #[test]
    fn the_slash_list_tells_a_driver_which_rows_are_workflows() {
        let mut host = host();
        assert!(host.snapshot().find(ids::COMPOSER_PANEL).is_none());

        let (listing, skills) = slash_listing();
        host.composer_panel = Some(PanelMode::Slash);
        host.panel_rows = panel_rows(PanelMode::Slash, &listing, &listed(&skills), None);

        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::COMPOSER_PANEL).unwrap().name,
            "Recipes, workflows, skills and actions",
            "the panel says what is in it, and the four words are kept apart in the saying"
        );
        let tape = tree.find("composer-panel-row-recipe:rcp_tape").unwrap();
        assert_eq!(tape.name, "Weekly report");
        assert_eq!(tape.value.as_deref(), Some("Recipe"));
        let workflow = tree.find("composer-panel-row-recipe:rcp_tree").unwrap();
        assert_eq!(workflow.name, "Search and retry");
        assert_eq!(
            workflow.value.as_deref(),
            Some("Workflow"),
            "the one assertable field a driver has is where the kind has to be"
        );
    }

    /// A skill is a row like any other: it is in the list under its own name, which is what a
    /// driver types into the panel's field to pick it, and it says it is a Skill rather than a
    /// Recipe — the two are a tape and a lesson, and nothing about the node shape says which.
    #[test]
    fn the_slash_list_offers_a_skill_by_name_and_will_not_offer_a_draft() {
        let mut host = host();
        let (listing, skills) = slash_listing();
        host.composer_panel = Some(PanelMode::Slash);
        host.panel_rows = panel_rows(PanelMode::Slash, &listing, &listed(&skills), None);

        let tree = host.snapshot();
        let skill = tree.find("composer-panel-row-skill:skl_1").unwrap();
        assert_eq!(skill.name, "expense-report");
        assert_eq!(skill.value.as_deref(), Some("Skill"));
        assert!(
            !skill.states.contains(&"note".to_string()),
            "this one can be taken, so the arrows must land on it"
        );
        // A draft has no prose in it for the bot to read, so the server would refuse it: the
        // row is shown and stepped over rather than offered and refused a round trip later.
        let draft = tree.find("composer-panel-row-skill:skl_2").unwrap();
        assert!(draft.states.contains(&"note".to_string()));

        // And a library with nothing in it says so under the id that row has always had.
        host.panel_rows = panel_rows(PanelMode::Slash, &listing, &listed(&[]), None);
        let empty = host.snapshot();
        let none = empty.find("composer-skills-none").unwrap();
        assert!(none.states.contains(&"note".to_string()));
        assert!(
            empty.find("composer-panel-row-skill:skl_1").is_none(),
            "no skill is listed while none is loaded"
        );
    }

    /// Which skill this message will be sent with, for a driver that has just picked one. The
    /// name is what the chip in the draft reads; the id is what the turn names, and it is the
    /// id that tells two skills of one name apart.
    #[test]
    fn the_composer_says_which_skill_the_next_message_carries() {
        let mut host = host();
        assert!(
            host.snapshot().find(ids::COMPOSER_SKILL).is_none(),
            "nothing is attached, and the way to say so is not to be there at all"
        );

        host.composer_skill = Some(crate::state::ActiveSkill {
            id: "skl_1".into(),
            name: "expense-report".into(),
        });
        let tree = host.snapshot();
        let attached = tree.find(ids::COMPOSER_SKILL).unwrap();
        assert_eq!(attached.name, "expense-report");
        assert_eq!(attached.value.as_deref(), Some("skl_1"));
    }

    #[test]
    fn the_recipe_bar_says_what_is_filled_in_and_what_is_still_needed() {
        let mut host = host();
        assert!(host.snapshot().find(ids::COMPOSER_RECIPE_BAR).is_none());

        let active = recipe(&[("city", "London")]);
        host.recipe_bar = Some(RecipeBarSnap {
            name: active.name.clone(),
            kind: active.kind,
            parameters: active
                .parameters
                .iter()
                .map(|parameter| ParamSnap {
                    value: active.value(&parameter.name).map(str::to_string),
                    name: parameter.name.clone(),
                    required: parameter.required,
                })
                .collect(),
        });
        let tree = host.snapshot();
        let bar = tree.find(ids::COMPOSER_RECIPE_BAR).unwrap();
        assert!(bar.name.starts_with("Recipe · "), "{}", bar.name);
        assert!(bar.name.contains("Weekly report"));
        let city = tree.find(&ids::recipe_param("city")).unwrap();
        assert_eq!(city.value.as_deref(), Some("London"));
        assert!(city.states.contains(&"filled".to_string()));
        let shorts = tree.find(&ids::recipe_param("shorts")).unwrap();
        assert!(shorts.value.is_none());
        // Not required, so nothing is missing from it.
        assert!(shorts.states.is_empty());
    }

    #[test]
    fn a_picture_is_a_node_and_a_click_opens_the_overlay() {
        let mut host = host();
        assert!(host.snapshot().find(&ids::image_thumb(0)).is_none());

        host.thumbs = vec!["the 1280x800 screen".into(), "after the click".into()];
        let tree = host.snapshot();
        assert_eq!(
            tree.find(&ids::image_thumb(1)).unwrap().name,
            "after the click"
        );
        host.dispatch(&Op::click(ids::image_thumb(1))).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::OpenLightbox { index: 1 })
        ));
        let missing = host.dispatch(&Op::click(ids::image_thumb(7))).unwrap_err();
        assert!(missing.contains("no picture"), "{missing}");
    }

    #[test]
    fn the_overlay_is_in_the_tree_only_while_it_is_open() {
        let mut host = host();
        assert!(host.snapshot().find(ids::LIGHTBOX).is_none());
        host.lightbox = Some(LightboxSnap {
            index: 1,
            total: 3,
            caption: "after the click".into(),
        });
        assert_eq!(
            host.snapshot().find(ids::LIGHTBOX).unwrap().name,
            "after the click · 2 / 3"
        );
    }

    #[test]
    fn the_composer_is_not_clicked_and_says_so() {
        let mut host = host();
        for target in [
            "composer-panel-row-recipe:rcp_1",
            "composer-param-city",
            ids::COMPOSER_RECIPE_BAR,
        ] {
            let error = host.dispatch(&Op::click(target)).unwrap_err();
            assert!(error.contains("keyboard"), "{target}: {error}");
        }
    }

    #[test]
    fn virtual_delivery_is_still_refused_rather_than_faked() {
        let mut host = host();
        let error = host
            .dispatch(&Op::type_text_virtual(ids::COMPOSER, "hi"))
            .unwrap_err();
        assert!(
            error.starts_with(gpui_agent::VIRTUAL_UNAVAILABLE),
            "{error}"
        );
        assert!(host.take_compose().is_none());
    }

    #[test]
    fn only_a_recipe_row_opens_a_recipe() {
        assert_eq!(
            recipe_row_target("recipe-rcp_01a0a97d"),
            Some("rcp_01a0a97d".to_string())
        );
        // The page's controls are all named `recipe-…` too; a bare prefix match sent a click on
        // a version tab off to fetch a recipe called "version-1", and the page said there was
        // no such recipe.
        for control in [
            "recipe-version-1",
            "recipe-step-2",
            "recipe-run",
            "recipe-delete",
            "recipe-history",
            "recipe-bots",
            "recipe-add-step",
        ] {
            assert_eq!(recipe_row_target(control), None, "{control}");
        }
        assert_eq!(recipe_row_target("recipes-filter-mine"), None);
    }

    /// The saved accounts for the card's site are rows under the name field, one per
    /// login; a click or the invoke picks one, and the pick names the login id.
    #[test]
    fn the_account_list_is_in_the_tree_and_a_row_picks_it() {
        let mut host = host();
        let mut form = google_login_form();
        form.saved_logins = vec![
            ("sl_1".into(), "ada@example.com".into(), "google.com".into()),
            ("sl_2".into(), "bea@example.com".into(), "google.com".into()),
        ];
        form.saved_login_note = Some("Confirm with Touch ID to fill in ada@example.com.".into());
        host.user_forms = vec![form];
        let tree = host.snapshot();
        let row = tree
            .find("user-form-use-saved-e_form-sl_2")
            .expect("second row");
        assert_eq!(row.name, "bea@example.com · google.com");
        assert!(tree.find("user-form-saved-note-e_form").is_some());

        host.dispatch(&Op::click("user-form-use-saved-e_form-sl_1"))
            .unwrap();
        match host.take_command() {
            Some(Command::UserFormUseSaved { card_key, login_id }) => {
                assert_eq!(card_key, "e_form");
                assert_eq!(login_id, "sl_1");
            }
            other => panic!("expected a pick, got {other:?}"),
        }
        // Two rows: the invoke needs to be told which.
        assert!(
            host.invoke("user-form.use-saved", &serde_json::json!({}))
                .is_err()
        );
        host.invoke(
            "user-form.use-saved",
            &serde_json::json!({ "login_id": "sl_2" }),
        )
        .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::UserFormUseSaved { login_id, .. }) if login_id == "sl_2"
        ));
        // A held password: the rows are gone, Change is there, and a click frees the card.
        host.user_forms[0].saved_logins.clear();
        host.user_forms[0].saved_login_held = true;
        let tree = host.snapshot();
        assert!(tree.find("user-form-use-saved-e_form-sl_1").is_none());
        assert!(tree.find("user-form-saved-clear-e_form").is_some());
        host.dispatch(&Op::click("user-form-saved-clear-e_form"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::UserFormClearSaved { card_key }) if card_key == "e_form"
        ));
        // The password the driver hands over never shows in a command's Debug.
        host.invoke(
            "logins.add",
            &serde_json::json!({ "origin": "x.com", "username": "a", "password": "hunter2" }),
        )
        .unwrap();
        let shown = format!("{:?}", host.take_command());
        assert!(
            shown.contains("AddSiteLogin") && !shown.contains("hunter2"),
            "{shown}"
        );
    }

    fn google_login_form() -> UserFormSnap {
        UserFormSnap {
            card_key: "e_form".into(),
            continue_label: "Continue",
            saved_logins: Vec::new(),
            saved_login_note: None,
            saved_login_held: false,
            passkey_register: false,
            title: "Google account".into(),
            fields: vec![
                UserFormFieldSnap {
                    id: "email".into(),
                    label: "Email".into(),
                    kind: UserFormFieldKind::Email,
                    masked: false,
                    value: String::new(),
                },
                UserFormFieldSnap {
                    id: "password".into(),
                    label: "Password".into(),
                    kind: UserFormFieldKind::Password,
                    masked: true,
                    value: String::new(),
                },
            ],
            pill: None,
        }
    }

    #[test]
    fn idle_user_form_fields_and_buttons_are_in_the_tree() {
        let mut host = host();
        assert!(host.snapshot().find("user-form-e_form").is_none());
        host.user_forms = vec![google_login_form()];
        let tree = host.snapshot();
        assert!(tree.find("user-form-e_form").is_some());
        assert_eq!(
            tree.find("user-form-field-e_form-email").unwrap().name,
            "Email"
        );
        assert_eq!(
            tree.find("user-form-field-e_form-password").unwrap().name,
            "Password"
        );
        assert!(tree.find("user-form-continue-e_form").is_some());
        assert!(tree.find("user-form-dismiss-e_form").is_some());
        assert_eq!(
            tree.find("user-form-screen-e_form").unwrap().name,
            "Open the screen"
        );
        host.dispatch(&Op::SetValue {
            target: "user-form-field-e_form-email".into(),
            value: "ada@example.com".into(),
        })
        .unwrap();
        match host.take_command() {
            Some(Command::UserFormSetField {
                card_key,
                field_id,
                value,
            }) => {
                assert_eq!(card_key, "e_form");
                assert_eq!(field_id, "email");
                assert_eq!(value, "ada@example.com");
            }
            other => panic!("expected set field, got {other:?}"),
        }
        host.dispatch(&Op::SetValue {
            target: "user-form-field-e_form-password".into(),
            value: "s3cret".into(),
        })
        .unwrap();
        assert!(
            host.snapshot()
                .find("user-form-field-e_form-password")
                .unwrap()
                .value
                .is_none(),
            "secrets stay off the tree"
        );
        host.dispatch(&Op::click("user-form-continue-e_form"))
            .unwrap();
        match host.take_command() {
            Some(Command::UserFormContinue { card_key }) => assert_eq!(card_key, "e_form"),
            other => panic!("expected continue, got {other:?}"),
        }
        host.dispatch(&Op::click("user-form-dismiss-e_form"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::UserFormDismiss { .. })
        ));
        host.dispatch(&Op::click("user-form-screen-e_form"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::UserFormOpenScreen { .. })
        ));
    }

    #[test]
    fn call_keyed_user_form_dismiss_is_in_the_tree() {
        let mut host = host();
        host.user_forms = vec![UserFormSnap {
            card_key: "call-9".into(),
            continue_label: "Continue",
            saved_logins: Vec::new(),
            saved_login_note: None,
            saved_login_held: false,
            passkey_register: false,
            title: "Website login".into(),
            fields: vec![UserFormFieldSnap {
                id: "email".into(),
                label: "Email".into(),
                kind: UserFormFieldKind::Email,
                masked: false,
                value: String::new(),
            }],
            pill: None,
        }];
        let tree = host.snapshot();
        assert!(tree.find("user-form-call-9").is_some());
        assert!(tree.find("user-form-dismiss-call-9").is_some());
        assert!(tree.find("user-form-screen-call-9").is_some());
        host.dispatch(&Op::click("user-form-dismiss-call-9"))
            .unwrap();
        match host.take_command() {
            Some(Command::UserFormDismiss { card_key }) => assert_eq!(card_key, "call-9"),
            other => panic!("expected dismiss call-9, got {other:?}"),
        }
        host.dispatch(&Op::click("user-form-screen-call-9"))
            .unwrap();
        match host.take_command() {
            Some(Command::UserFormOpenScreen { card_key }) => assert_eq!(card_key, "call-9"),
            other => panic!("expected open screen call-9, got {other:?}"),
        }
    }

    #[test]
    fn computer_handoff_chrome_is_in_the_tree() {
        let mut host = host();
        host.computer_handoffs = vec![ComputerHandoffSnap {
            card_key: "e_form".into(),
            instruction: "Sign in on the computer.".into(),
            status: ComputerHandoffStatus::ActionNeeded,
        }];
        host.computer_open = true;
        let tree = host.snapshot();
        assert!(tree.find("computer-handoff-e_form").is_some());
        assert_eq!(
            tree.find("computer-handoff-takeover-e_form").unwrap().name,
            "Take over"
        );
        assert_eq!(
            tree.find("computer-handoff-done-e_form").unwrap().name,
            "I'm done"
        );
        assert_eq!(
            tree.find("computer-handoff-skip-e_form").unwrap().name,
            "Skip"
        );
        assert_eq!(
            tree.find("computer-attention").unwrap().name,
            "Needs your attention"
        );
        assert_eq!(
            tree.find("computer-attention-skip-e_form").unwrap().name,
            "Skip this step"
        );
        assert_eq!(
            tree.find("computer-attention-done-e_form").unwrap().name,
            "I'm done, continue"
        );
        assert_eq!(
            tree.find("computer-window-attention").unwrap().name,
            "Needs your attention"
        );
        assert_eq!(
            tree.find("computer-window-attention-skip-e_form")
                .unwrap()
                .name,
            "Skip this step"
        );
        assert_eq!(
            tree.find("computer-window-attention-done-e_form")
                .unwrap()
                .name,
            "I'm done, continue"
        );
        host.dispatch(&Op::click("computer-handoff-takeover-e_form"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ComputerHandoffTakeOver { card_key }) if card_key == "e_form"
        ));
        host.dispatch(&Op::click("computer-handoff-done-e_form"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ComputerHandoffDone { .. })
        ));
        host.dispatch(&Op::click("computer-attention-skip-e_form"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ComputerHandoffSkip { .. })
        ));
        host.dispatch(&Op::click("computer-window-attention-done-e_form"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ComputerHandoffDone { .. })
        ));
    }

    #[test]
    fn form_and_computer_stay_in_the_tree_across_handoff() {
        let mut host = host();
        host.user_forms = vec![google_login_form()];
        host.computer_handoffs = vec![ComputerHandoffSnap {
            card_key: "e_form".into(),
            instruction: "Sign in on the computer.".into(),
            status: ComputerHandoffStatus::ActionNeeded,
        }];
        let open = host.snapshot();
        assert!(
            open.find("user-form-e_form").is_some(),
            "Open the screen must not rip user-form-* out"
        );
        assert!(open.find("user-form-screen-e_form").is_some());
        assert!(open.find("computer-handoff-e_form").is_some());
        assert_eq!(
            open.find("computer-handoff-badge-e_form").unwrap().name,
            "Action needed"
        );

        host.user_forms = vec![UserFormSnap {
            card_key: "e_form".into(),
            continue_label: "Continue",
            saved_logins: Vec::new(),
            saved_login_note: None,
            saved_login_held: false,
            passkey_register: false,
            title: "Google account".into(),
            fields: Vec::new(),
            pill: Some("Dismissed".into()),
        }];
        host.computer_handoffs[0].status = ComputerHandoffStatus::Done;
        let done = host.snapshot();
        assert!(done.find("user-form-e_form").is_some());
        assert_eq!(
            done.find("user-form-pill-e_form").unwrap().name,
            "Dismissed"
        );
        assert!(done.find("user-form-screen-e_form").is_none());
        assert!(done.find("computer-handoff-e_form").is_some());
        assert_eq!(
            done.find("computer-handoff-badge-e_form").unwrap().name,
            "Done"
        );
        assert!(done.find("computer-handoff-takeover-e_form").is_none());
        assert!(done.find("computer-attention").is_none());

        host.user_forms[0].pill = Some("Skipped".into());
        host.computer_handoffs[0].status = ComputerHandoffStatus::Skipped;
        let skipped = host.snapshot();
        assert_eq!(
            skipped.find("user-form-pill-e_form").unwrap().name,
            "Skipped"
        );
        assert_eq!(
            skipped.find("computer-handoff-badge-e_form").unwrap().name,
            "Skipped"
        );
        assert!(skipped.find("computer-handoff-e_form").is_some());
    }

    #[test]
    fn settled_user_form_has_no_field_nodes() {
        let host = host();
        let tree = host.snapshot();
        assert!(
            tree.find("user-form-field-e_form-email").is_none(),
            "after Continue the snapshot must not list idle fields"
        );
        assert!(tree.find("user-form-continue-e_form").is_none());
    }

    #[test]
    fn save_login_is_in_the_tree_without_a_password() {
        let mut host = host();
        host.save_logins = vec![SaveLoginSnap {
            form_entry_id: "e_form".into(),
            origin: "google.com".into(),
            username: "ada@example.com".into(),
        }];
        let tree = host.snapshot();
        assert!(tree.find("save-login-e_form").is_some());
        assert!(tree.find("save-login-save-e_form").is_some());
        assert!(tree.find("save-login-skip-e_form").is_some());
        let dump = format!("{tree:?}");
        assert!(!dump.contains("s3cret"));
        host.dispatch(&Op::click("save-login-save-e_form")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SaveLogin { .. })
        ));
    }

    #[test]
    fn an_unknown_invoke_fails_closed() {
        let mut host = host();
        let unknown = host
            .dispatch(&Op::Invoke {
                name: "NotACommand".into(),
                args: serde_json::json!({}),
            })
            .unwrap_err();
        assert!(
            unknown.contains("unknown invoke"),
            "stray names still fail closed: {unknown}"
        );
    }

    #[test]
    fn user_form_continue_and_dismiss_are_named_invokes() {
        let mut host = host();
        host.user_forms = vec![google_login_form()];
        host.dispatch(&Op::Invoke {
            name: "UserFormContinue".into(),
            args: serde_json::json!({ "card_key": "e_form" }),
        })
        .unwrap();
        match host.take_command() {
            Some(Command::UserFormContinue { card_key }) => assert_eq!(card_key, "e_form"),
            other => panic!("expected Continue invoke, got {other:?}"),
        }
        host.dispatch(&Op::Invoke {
            name: "UserFormDismiss".into(),
            args: serde_json::json!({}),
        })
        .unwrap();
        match host.take_command() {
            Some(Command::UserFormDismiss { card_key }) => assert_eq!(card_key, "e_form"),
            other => panic!("expected Dismiss invoke, got {other:?}"),
        }
    }

    /// Settings → Logins in the tree: the search field with Add beside it, the sections
    /// with their counts and the rows the search leaves, the picked row's pane with its
    /// notes, and the Add sheet while it is up. A click, a set-value or an invoke each name
    /// the command the page would run, and nothing in the tree or in `logins.list` is a
    /// password.
    #[test]
    fn settings_logins_tree_has_search_sections_rows_and_the_picked_pane() {
        let mut host = host();
        host.account_open = true;
        host.logins_tab = true;
        host.site_logins = vec![
            SiteLoginSnap {
                row: SiteLoginRecord {
                    id: "cred-1".into(),
                    origin: "google.com".into(),
                    username: "ada@example.com".into(),
                    kind: "password".into(),
                    ..Default::default()
                },
                on_this_mac: true,
                has_code: false,
            },
            SiteLoginSnap {
                row: SiteLoginRecord {
                    id: "cred-2".into(),
                    origin: "github.com".into(),
                    username: "ada".into(),
                    label: "Work GitHub".into(),
                    kind: "passkey".into(),
                    notes: "Security: hardware key".into(),
                    ..Default::default()
                },
                on_this_mac: false,
                has_code: false,
            },
        ];
        let tree = host.snapshot();
        let count = |id: &str| tree.find(id).unwrap().value.clone().unwrap();
        assert_eq!(count("settings-logins-group-passwords"), "1");
        assert_eq!(count("settings-logins-group-passkeys"), "1");
        assert_eq!(count("settings-logins-group-codes"), "0");
        assert_eq!(count("settings-logins-group-security"), "1");
        assert_eq!(
            tree.find("settings-logins-search")
                .unwrap()
                .value
                .as_deref(),
            Some("")
        );
        assert!(tree.find("settings-login-add").is_some());
        assert!(tree.find("settings-login-import").is_some());
        let row = tree.find("settings-login-row-cred-1").unwrap();
        assert!(row.name.contains("ada@example.com") && row.name.contains("google.com"));
        assert!(row.value.is_none());
        assert_eq!(
            tree.find("settings-login-row-cred-2").unwrap().name,
            "Work GitHub · ada"
        );
        assert_eq!(
            tree.find_all("settings-login-row-cred-2").len(),
            2,
            "a row with a Security: note is under its kind and under Security"
        );
        assert!(
            tree.find("settings-login-detail-cred-1").is_none(),
            "nothing picked yet"
        );

        // A row picks; the pane follows with the notes as a field and where the password is.
        host.dispatch(&Op::click("settings-login-row-cred-2"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SelectSiteLogin(Some(id))) if id == "cred-2"
        ));
        host.site_login_selected = Some("cred-2".into());
        let tree = host.snapshot();
        assert_eq!(
            tree.find("settings-login-detail-cred-2").unwrap().name,
            "Work GitHub"
        );
        assert_eq!(
            tree.find("settings-login-notes-cred-2")
                .unwrap()
                .value
                .as_deref(),
            Some("Security: hardware key")
        );
        assert!(
            tree.find("settings-login-where-cred-2")
                .unwrap()
                .name
                .contains("server")
        );
        assert_eq!(
            tree.find("settings-login-last-used-cred-2")
                .unwrap()
                .value
                .as_deref(),
            Some("Never")
        );
        assert!(
            tree.find_all("settings-login-row-cred-2")
                .iter()
                .all(|row| row.states.contains(&"selected".to_string()))
        );

        // The search, by invoke or by set-value, is one command; the tree shows the sections
        // it leaves, and the pick stays on the pane.
        host.invoke("logins.search", &serde_json::json!({ "q": "google" }))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetSiteLoginQuery(q)) if q == "google"
        ));
        let tree = host.snapshot();
        assert_eq!(count_in(&tree, "settings-logins-group-passwords"), "1");
        assert!(tree.find("settings-logins-group-passkeys").is_none());
        assert!(tree.find("settings-logins-group-codes").is_none());
        assert!(tree.find("settings-logins-group-security").is_none());
        assert!(tree.find("settings-login-row-cred-2").is_none());
        assert!(tree.find("settings-login-row-cred-1").is_some());
        assert!(tree.find("settings-login-detail-cred-2").is_some());
        assert_eq!(
            tree.find("settings-logins-search")
                .unwrap()
                .value
                .as_deref(),
            Some("google")
        );
        host.dispatch(&Op::SetValue {
            target: "settings-logins-search".into(),
            value: "nothing here".into(),
        })
        .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetSiteLoginQuery(q)) if q == "nothing here"
        ));
        let tree = host.snapshot();
        assert_eq!(
            tree.find("settings-logins-empty").unwrap().name,
            "No logins match."
        );
        assert!(tree.find("settings-logins-group-passwords").is_none());
        host.invoke("logins.search", &serde_json::json!({}))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetSiteLoginQuery(q)) if q.is_empty()
        ));
        assert!(
            host.snapshot()
                .find("settings-logins-group-codes")
                .is_some()
        );

        // The notes, by invoke or by set-value on the pane's field; an unknown row is refused.
        host.invoke(
            "logins.notes",
            &serde_json::json!({ "id": "cred-2", "notes": "Security: yubikey" }),
        )
        .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetSiteLoginNotes { id, notes })
                if id == "cred-2" && notes == "Security: yubikey"
        ));
        host.dispatch(&Op::SetValue {
            target: "settings-login-notes-cred-1".into(),
            value: "plain words".into(),
        })
        .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetSiteLoginNotes { id, notes }) if id == "cred-1" && notes == "plain words"
        ));
        assert!(
            host.invoke(
                "logins.notes",
                &serde_json::json!({ "id": "nope", "notes": "x" })
            )
            .is_err()
        );
        assert!(
            host.invoke("logins.select", &serde_json::json!({ "id": "nope" }))
                .is_err()
        );

        // The list answers the rows and never a password.
        let listed = host
            .invoke("logins.list", &serde_json::json!({}))
            .unwrap()
            .value
            .unwrap();
        let logins = listed["logins"].as_array().unwrap();
        assert_eq!(logins.len(), 2);
        assert_eq!(logins[1]["id"], "cred-2");
        assert_eq!(logins[1]["kind"], "passkey");
        assert_eq!(logins[1]["label"], "Work GitHub");
        assert_eq!(logins[1]["on_this_mac"], false);
        assert_eq!(logins[0]["on_this_mac"], true);
        assert!(logins[0]["last_used_at_ms"].is_null());
        assert!(
            logins.iter().all(|login| login.get("password").is_none()),
            "a kind may be `password`; a field never is"
        );

        // Add opens the sheet; its fields and buttons are in the tree; Cancel closes it. Save
        // cannot be clicked from here, because its fields are the window's: the invoke is
        // the way, and it carries the title and the notes.
        host.dispatch(&Op::click("settings-login-add")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::OpenSiteLoginAdd)
        ));
        assert!(host.snapshot().find("settings-login-add-sheet").is_none());
        host.site_login_add_open = true;
        let tree = host.snapshot();
        for id in [
            "settings-login-add-title",
            "settings-login-add-username",
            "settings-login-add-password",
            "settings-login-add-website",
            "settings-login-add-notes",
            "settings-login-add-save",
            "settings-login-add-cancel",
        ] {
            assert!(tree.find(id).is_some(), "{id}");
        }
        assert!(
            host.dispatch(&Op::click("settings-login-add-save"))
                .is_err()
        );
        host.dispatch(&Op::click("settings-login-add-cancel"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::CloseSiteLoginAdd)
        ));
        host.invoke(
            "logins.add",
            &serde_json::json!({
                "origin": "x.com", "username": "a", "password": "hunter2",
                "label": "X", "notes": "Security: none"
            }),
        )
        .unwrap();
        match host.take_command() {
            Some(Command::AddSiteLogin { label, notes, .. }) => {
                assert_eq!(label, "X");
                assert_eq!(notes, "Security: none");
            }
            other => panic!("expected an add, got {other:?}"),
        }

        // Delete is still by id, whether or not the row is picked.
        host.dispatch(&Op::click("settings-login-delete-cred-1"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::DeleteSiteLogin { id }) if id == "cred-1"
        ));
    }

    fn count_in(tree: &UiTree, id: &str) -> String {
        tree.find(id).unwrap().value.clone().unwrap()
    }

    #[test]
    fn dedicated_route_traffic_is_computer_pane_not_settings() {
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.route_traffic_on_bot_pane = true;
        assert!(
            host.snapshot()
                .find("route-traffic-this-computer")
                .is_none(),
            "Settings must not host dedicated Route traffic"
        );
        host.computer_open = true;
        assert!(
            host.snapshot()
                .find("route-traffic-this-computer")
                .is_some()
        );
        host.dispatch(&Op::click("route-traffic-this-computer"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetEgressTunnelEnabled(true))
        ));
    }

    /// The network choice sits with Route traffic on either surface, only when the server
    /// carries one, and a click on a word is that word — not a toggle.
    #[test]
    fn egress_policy_sits_with_route_traffic_and_clicks_are_words() {
        use crate::opengrok::LocalExecMode;
        let mut host = host();
        host.agent_settings_open = true;
        host.route_traffic_on_bot_pane = true;
        assert!(
            host.snapshot().find("egress-policy-menu").is_none(),
            "no policy from the server: no control"
        );
        assert!(
            host.invoke(
                "computer.egress_policy",
                &serde_json::json!({ "mode": "never" })
            )
            .is_err(),
            "setting a policy the server does not carry must fail loudly"
        );
        assert!(
            host.dispatch(&Op::click("egress-policy-never")).is_err(),
            "a click on a control that is not there must fail loudly too"
        );
        host.egress_policy = Some(LocalExecMode::Ask);
        // A dedicated box: a shield badge on the Computer pane opens the dialog; the words are
        // clickable only while it is open.
        host.agent_settings_open = false;
        host.computer_open = true;
        let tree = host.snapshot();
        assert!(tree.find("network-policy").is_some(), "the badge");
        assert!(tree.find("egress-policy-menu").is_none(), "closed dialog");
        assert!(host.dispatch(&Op::click("egress-policy-never")).is_err());
        host.dispatch(&Op::click("network-policy")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::OpenNetworkPolicy)
        ));
        host.network_policy_open = true;
        let tree = host.snapshot();
        assert!(tree.find("egress-policy-menu").is_some());
        assert!(tree.find("network-policy-close").is_some());
        assert_eq!(
            tree.find("egress_policy").map(|node| node.name.clone()),
            Some("ask".to_string())
        );
        host.dispatch(&Op::click("egress-policy-never")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetEgressPolicy(LocalExecMode::Never))
        ));
        host.invoke(
            "computer.egress_policy",
            &serde_json::json!({ "mode": "bypass" }),
        )
        .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetEgressPolicy(LocalExecMode::Always))
        ));
        assert!(
            host.invoke(
                "computer.egress_policy",
                &serde_json::json!({ "mode": "sometimes" })
            )
            .is_err()
        );

        // A shared box: Settings → Computer, not the bot's sidebar — the surface flag is what
        // moves it, with both places open.
        let mut host = self::host();
        host.egress_policy = Some(LocalExecMode::Never);
        host.computer_open = true;
        host.account_open = true;
        host.computer_tab = true;
        host.route_traffic_on_bot_pane = false;
        host.route_traffic_in_user_settings = false;
        assert!(host.snapshot().find("egress-policy-menu").is_none());
        assert!(host.snapshot().find("network-policy").is_none());
        host.route_traffic_in_user_settings = true;
        let tree = host.snapshot();
        let menu = tree
            .find("egress-policy-menu")
            .expect("in Settings → Computer");
        assert_eq!(menu.name, "Never allow");
        host.route_traffic_in_user_settings = false;
        host.route_traffic_on_bot_pane = true;
        assert!(
            host.snapshot().find("network-policy").is_some(),
            "the badge instead"
        );
    }

    #[test]
    fn unprovisioned_hides_route_traffic() {
        let mut host = host();
        host.computer_open = true;
        host.account_open = true;
        host.computer_tab = true;
        assert!(
            host.snapshot()
                .find("route-traffic-this-computer")
                .is_none()
        );
    }

    #[test]
    fn user_scope_route_traffic_is_settings_computer_not_bot_pane() {
        let mut host = host();
        host.computer_open = true;
        host.route_traffic_in_user_settings = true;
        assert!(
            host.snapshot()
                .find("route-traffic-this-computer")
                .is_none(),
            "shared/user-scope Route traffic must not duplicate on every bot pane"
        );
        host.account_open = true;
        host.computer_tab = true;
        assert!(
            host.snapshot()
                .find("route-traffic-this-computer")
                .is_some()
        );
    }

    /// Why the server could not give the bot a computer is on the tree under `computer-status`,
    /// in the words the pane shows and with the server's code, and Get a computer beside it asks
    /// again. The first status that names a box takes both away, and a click on Get a computer
    /// then is refused.
    #[test]
    fn the_computer_pane_s_reason_for_no_computer_is_on_the_tree() {
        let recorded = |recording: &str| -> crate::opengrok::CoworkerComputer {
            let fixture: serde_json::Value = serde_json::from_str(recording).unwrap();
            serde_json::from_value(fixture["body"].clone()).unwrap()
        };
        let mut state = AppState::new();
        // Signed in with a bot on the roster, or the tree is the sign-in or empty-roster page
        // and has no Computer pane in it.
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.right_pane = crate::state::RightPane::Computer;
        state.coworker_computer = Some(recorded(include_str!(
            "../../fixtures/wire/rest/GET__coworkers__coworker_id__computer/200-a_hosted_hire_whose_ascii_create_fails_records_why_and_makes_no_box.json"
        )));
        let mut host = NativeChatHost::from_app(&state);
        let tree = host.snapshot();
        let status = tree.find("computer-status").unwrap();
        let error = status
            .children
            .iter()
            .find(|node| node.id == ids::COMPUTER_ERROR)
            .expect("the reason, under the status");
        assert_eq!(
            error.name,
            "The box refused: 429 {\"error\":\"box creation rate limit reached: \
             https://api.box.ascii.dev/v1/boxes?key=«redacted»\"}"
        );
        assert_eq!(error.value.as_deref(), Some("quota_exceeded"));
        let get = tree.find(ids::COMPUTER_GET).expect("Get a computer");
        assert_eq!(get.name, "Get a computer");
        assert!(get.enabled);
        host.dispatch(&Op::click(ids::COMPUTER_GET)).unwrap();
        assert!(matches!(host.take_command(), Some(Command::GetComputer)));

        state.coworker_computer = Some(recorded(include_str!(
            "../../fixtures/wire/rest/GET__coworkers__coworker_id__computer/200-a_healthy_local_vm_does_not_wear_another_scopes_failure.json"
        )));
        let mut host = NativeChatHost::from_app(&state);
        let tree = host.snapshot();
        assert!(
            tree.find("computer-status").is_some(),
            "the pane is still drawn"
        );
        assert!(tree.find(ids::COMPUTER_ERROR).is_none());
        assert!(tree.find(ids::COMPUTER_GET).is_none());
        assert!(host.dispatch(&Op::click(ids::COMPUTER_GET)).is_err());
    }

    #[test]
    fn settings_updates_has_update_and_no_reset() {
        let mut host = host();
        host.account_open = true;
        host.updates_tab = true;
        let tree = host.snapshot();
        assert!(tree.find("settings-tab-updates").is_some());
        assert!(tree.find("settings-computer-update").is_some());
        assert!(
            tree.find("settings-computer-reset").is_none(),
            "Reset lives on the Computer pane, not Settings → Updates"
        );
        assert!(tree.find("computer-reset").is_some());
        host.dispatch(&Op::click("settings-tab-updates")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetAppSettingsTab(AppSettingsTab::Updates))
        ));
    }

    /// A card the MCP door raised: two answers, because Always and Never write
    /// a policy for this Mac and there is none behind an MCP call. The reason
    /// and the thread ride along as states so a driver can say which card this
    /// is without reading the title.
    #[test]
    fn an_mcp_card_offers_two_answers_and_says_where_it_came_from() {
        let mut host = host();
        host.approvals = vec![ApprovalSnap {
            call_id: "call_9".into(),
            tool: "read_file".into(),
            title: "Allow Ada to read a file on its computer?".into(),
            local: false,
            review: false,
            reason: "policy-approval".into(),
            thread_id: "mcp-cw_1".into(),
            tunnel: false,
        }];
        let tree = host.snapshot();
        let card = tree.find("approval-call_9").unwrap();
        assert_eq!(
            card.children
                .iter()
                .map(|child| child.id.as_str())
                .collect::<Vec<_>>(),
            vec!["approval-call_9-allow-once", "approval-call_9-deny-once"]
        );
        assert_eq!(
            card.states,
            vec![
                "policy-approval".to_string(),
                "mcp-cw_1".to_string(),
                "read_file".to_string()
            ]
        );
    }

    /// The approval card's title in the tree is the one the card draws: the same function, the
    /// same bot name, so a snapshot catches a title a person would read wrong.
    #[test]
    fn an_approval_card_is_titled_as_the_person_sees_it() {
        let spec = crate::opengrok::approval_from_event(&serde_json::json!({
            "type": "CUSTOM", "name": "run-awaiting-approval", "threadId": "cw_1",
            "runId": "run_1", "callId": "call_1", "tool": "shell",
            "arguments": { "command": "ls" }, "reason": "exec-consent"
        }))
        .expect("a card");
        let mut state = AppState::new();
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Hex" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.active_conversation_id = Some("cw_1".into());
        state.conversations.push(crate::state::Conversation {
            id: "cw_1".into(),
            title: "Hex".into(),
            created_at: String::new(),
            updated_at: String::new(),
            messages: vec![crate::state::Message {
                id: "m_1".into(),
                sender: "AI".into(),
                content: String::new(),
                sent_at: std::time::SystemTime::UNIX_EPOCH,
                finished_at: None,
                run_timing: None,
                reply_source: None,
                is_me: false,
                reply_preview: None,
                reply_to_id: None,
                reply_is_me: false,
                parts: vec![crate::opengrok::ChatPart::Approval(spec.clone())],
                run_id: None,
                hidden: false,
            }],
            unread_count: 0,
            origin: None,
        });
        let mut host = NativeChatHost::from_app(&state);
        // Signed in draws the chat; a state with no account would draw the sign-in page.
        host.signed_in = true;
        let card = host.snapshot().find("approval-call_1").cloned().unwrap();
        assert_eq!(
            card.name,
            crate::components::gen_ui::approval_title(&spec, "Hex", false)
        );
        assert_eq!(card.name, "Allow Hex to run a command on its computer?");
        assert!(card.states.contains(&"shell".to_string()));
    }

    /// The newest reply's steps are on the tree with their rows' own words and how each came
    /// out, and its thoughts are counted; an older reply's are not. A click opens a step, a
    /// second shuts it, and `reply-reasoning` opens the thoughts.
    #[test]
    fn a_reply_s_steps_are_on_the_tree_and_open_on_click() {
        use crate::opengrok::StepSpec;
        let step = |call_id: &str, tool: &str, arguments: &str, result: Option<(&str, bool)>| {
            ChatPart::Step(StepSpec {
                call_id: call_id.into(),
                tool: tool.into(),
                arguments: arguments.into(),
                result: result.map(|(content, _)| content.to_string()),
                ok: result.map(|(_, ok)| ok),
                took_ms: None,
            })
        };
        let reply = |id: &str, is_me: bool, parts: Vec<ChatPart>| crate::state::Message {
            id: id.into(),
            sender: if is_me { "Me" } else { "AI" }.into(),
            content: "words".into(),
            sent_at: std::time::SystemTime::UNIX_EPOCH,
            finished_at: None,
            run_timing: None,
            reply_source: None,
            is_me,
            reply_preview: None,
            reply_to_id: None,
            reply_is_me: false,
            parts,
            run_id: None,
            hidden: false,
        };
        let mut state = AppState::new();
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Hex" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.active_conversation_id = Some("cw_1".into());
        state.conversations.push(crate::state::Conversation {
            id: "cw_1".into(),
            title: "Hex".into(),
            created_at: String::new(),
            updated_at: String::new(),
            messages: vec![
                reply(
                    "m_old",
                    false,
                    vec![step(
                        "c9",
                        "shell",
                        "{\"command\":\"pwd\"}",
                        Some(("/", true)),
                    )],
                ),
                reply("m_ask", true, Vec::new()),
                reply(
                    "m_new",
                    false,
                    vec![
                        ChatPart::Reasoning("List it, then read it.".into()),
                        ChatPart::Text("Let me look.".into()),
                        step("c1", "shell", "{\"command\":\"ls\"}", Some(("a.txt", true))),
                        step("c2", "read_file", "{\"path\":\"a.txt\"}", None),
                        step(
                            "c3",
                            "open_url",
                            "{\"url\":\"https://example.com/x\"}",
                            Some(("refused", false)),
                        ),
                        ChatPart::Text("Done.".into()),
                    ],
                ),
            ],
            unread_count: 0,
            origin: None,
        });
        let host = |state: &AppState| {
            let mut host = NativeChatHost::from_app(state);
            host.signed_in = true;
            host
        };

        let tree = host(&state).snapshot();
        assert_eq!(
            tree.find(ids::REPLY_STEPS).unwrap().value.as_deref(),
            Some("3")
        );
        let said = |id: &str| {
            let node = tree.find(&ids::step(id)).unwrap();
            (node.name.clone(), node.value.clone().unwrap_or_default())
        };
        assert_eq!(said("c1"), ("Running `ls`".into(), "ok".into()));
        assert_eq!(said("c2"), ("Reading a.txt".into(), "running".into()));
        assert_eq!(said("c3"), ("Opening example.com".into(), "failed".into()));
        assert!(tree.find(&ids::step("c9")).is_none(), "an older reply's");
        assert!(
            !tree
                .find(&ids::step("c1"))
                .unwrap()
                .states
                .contains(&"expanded".to_string())
        );
        let thoughts = tree.find(ids::REPLY_REASONING).unwrap();
        assert_eq!(thoughts.value.as_deref(), Some("1"));
        assert!(tree.ids_are_unique());

        let click = |state: &mut AppState, target: &str| {
            let mut driver = host(state);
            driver.click(target).unwrap();
            let Some(Command::SetStepsOpen { keys, open }) = driver.take_command() else {
                panic!("a click on {target} opens or shuts rows");
            };
            state.mark_steps_open(&keys, open);
        };
        let expanded = |state: &AppState, id: &str| {
            host(state)
                .snapshot()
                .find(id)
                .unwrap()
                .states
                .contains(&"expanded".to_string())
        };
        click(&mut state, &ids::step("c1"));
        assert!(expanded(&state, &ids::step("c1")));
        assert!(!expanded(&state, &ids::step("c2")));
        click(&mut state, &ids::step("c1"));
        assert!(!expanded(&state, &ids::step("c1")));
        click(&mut state, ids::REPLY_REASONING);
        assert!(expanded(&state, ids::REPLY_REASONING));
        assert!(host(&state).click(&ids::step("c9")).is_err());

        // A reply that only talked has neither.
        if let Some(newest) = state.conversations[0].messages.last_mut() {
            newest.parts = vec![ChatPart::Text("Just words.".into())];
        }
        let tree = host(&state).snapshot();
        assert!(tree.find(ids::REPLY_STEPS).is_none());
        assert!(tree.find(ids::REPLY_REASONING).is_none());
    }

    /// The newest reply, one call this app timed and one it did not, in a thread of its own.
    fn one_timed_reply() -> AppState {
        use crate::opengrok::StepSpec;
        let step = |call_id: &str, took_ms: Option<u64>| {
            ChatPart::Step(StepSpec {
                call_id: call_id.into(),
                tool: "shell".into(),
                arguments: "{\"command\":\"uname -a\"}".into(),
                result: Some("Darwin".into()),
                ok: Some(true),
                took_ms,
            })
        };
        let mut state = AppState::new();
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Hex" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.active_conversation_id = Some("cw_1".into());
        state.conversations.push(crate::state::Conversation {
            id: "cw_1".into(),
            title: "Hex".into(),
            created_at: String::new(),
            updated_at: String::new(),
            messages: vec![crate::state::Message {
                id: "m_new".into(),
                sender: "AI".into(),
                content: "It is a Mac.".into(),
                sent_at: std::time::SystemTime::UNIX_EPOCH,
                finished_at: None,
                run_timing: None,
                reply_source: None,
                is_me: false,
                reply_preview: None,
                reply_to_id: None,
                reply_is_me: false,
                parts: vec![
                    step("c1", Some(1000)),
                    step("c2", None),
                    ChatPart::Text("It is a Mac.".into()),
                ],
                run_id: None,
                hidden: false,
            }],
            unread_count: 0,
            origin: None,
        });
        state
    }

    fn signed_in_host(state: &AppState) -> NativeChatHost {
        let mut host = NativeChatHost::from_app(state);
        host.signed_in = true;
        host
    }

    /// With Settings → Show turn timing on, a step whose row ends with how long the call took
    /// has that as a state, and its value still says how it came out; a call this app did not
    /// time has none. With the setting off, no row says it, and no step has it.
    #[test]
    fn a_step_s_time_is_a_state_while_its_row_says_it() {
        let mut state = one_timed_reply();
        let took = |state: &AppState, call_id: &str| {
            let tree = signed_in_host(state).snapshot();
            let node = tree.find(&ids::step(call_id)).unwrap();
            (
                node.value.clone().unwrap_or_default(),
                node.states
                    .iter()
                    .filter(|state| state.starts_with("took-"))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(took(&state, "c1"), ("ok".to_string(), Vec::new()));
        state.show_turn_timing = true;
        assert_eq!(
            took(&state, "c1"),
            ("ok".to_string(), vec!["took-1s".to_string()])
        );
        assert_eq!(took(&state, "c2"), ("ok".to_string(), Vec::new()));
    }

    /// With Settings → Show turn timing on, the Timing row under the newest reply is on the tree
    /// with what it says shut, and a click opens it on its lines, as a person's does. A row that
    /// is the total alone does not open, and the click says so. With the setting off, or with no
    /// `run-timing` frame, it is not there, as it is not drawn.
    #[test]
    fn the_timing_footer_is_on_the_tree_but_never_opens() {
        use crate::opengrok::TurnTiming;
        let mut state = one_timed_reply();
        state.show_turn_timing = true;
        let timing_node = |state: &AppState| {
            signed_in_host(state)
                .snapshot()
                .find(ids::REPLY_TIMING)
                .cloned()
        };
        assert!(timing_node(&state).is_none(), "no frame came");

        state.conversations[0].messages[0].run_timing =
            TurnTiming::from_value(&serde_json::json!({
                "model_ms": [2000, 2000],
                "tools": [{ "name": "shell", "ms": 1000 }],
                "tool_wait_ms": 1000,
                "total_ms": 10000,
                "tool_rounds": 1
            }));
        state.show_turn_timing = false;
        assert!(timing_node(&state).is_none(), "the setting is off");

        state.show_turn_timing = true;
        let shut = timing_node(&state).expect("the Timing row");
        assert_eq!(shut.value.as_deref(), Some("10s total"));
        assert!(!shut.states.contains(&"expanded".to_string()));
        assert!(shut.children.is_empty(), "shut, it is the total alone");
        assert!(signed_in_host(&state).snapshot().ids_are_unique());

        let mut driver = signed_in_host(&state);
        assert!(driver.click(ids::REPLY_TIMING).is_err());
        assert!(driver.take_command().is_none());
        state.mark_steps_open(&[crate::components::steps::timing_key("m1")], true);
        let open = timing_node(&state).expect("the Timing row");
        assert!(!open.states.contains(&"expanded".to_string()));
        let lines: Vec<(String, String)> = open
            .children
            .iter()
            .map(|line| (line.id.clone(), line.name.clone()))
            .collect();
        assert_eq!(lines, Vec::<(String, String)>::new());

        // A frame with a total and nothing else is a row that does not open.
        state.conversations[0].messages[0].run_timing =
            TurnTiming::from_value(&serde_json::json!({ "total_ms": 10000 }));
        assert_eq!(
            timing_node(&state).and_then(|node| node.value),
            Some("10s total".to_string())
        );
        assert!(signed_in_host(&state).click(ids::REPLY_TIMING).is_err());
    }

    /// Settings → Computer lists each connected computer's local-exec mode, marks this Mac, and
    /// a button per mode sets it: the Ask / Always / Never a check of this Mac's commands needs.
    #[test]
    fn a_computers_exec_mode_can_be_read_and_set_from_settings() {
        use crate::opengrok::LocalExecMode;
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.computers = vec![
            ComputerCard {
                label: "This Mac".into(),
                ..a_card("mac-1", true)
            },
            ComputerCard {
                label: "Studio".into(),
                mode: LocalExecMode::Never,
                ..a_card("mac-2", false)
            },
        ];
        let tree = host.snapshot();
        let mine = tree.find(&ids::computer_exec("mac-1")).unwrap();
        assert_eq!(mine.value.as_deref(), Some("ask"));
        assert!(mine.states.contains(&"this-mac".to_string()));
        assert_eq!(
            tree.find(&ids::computer_exec("mac-2"))
                .unwrap()
                .value
                .as_deref(),
            Some("never")
        );
        host.click(&ids::computer_exec_mode("mac-1", LocalExecMode::Always))
            .unwrap();
        assert!(matches!(
            host.take_command().unwrap(),
            Command::SetComputerExecMode { machine_id, mode: LocalExecMode::Always }
                if machine_id == "mac-1"
        ));
        host.account_open = false;
        assert!(host.snapshot().find(&ids::computer_exec("mac-1")).is_none());
        assert!(
            host.click(&ids::computer_exec_mode("mac-1", LocalExecMode::Never))
                .is_err(),
            "not a target while Settings is shut"
        );
    }

    /// The open recipe's Run, the run it started and its history are on the tree, so an async
    /// run can be followed from snapshots: Run plays on the first bot granted it, is dead while
    /// a run is going, and each history line says where its run got to.
    #[test]
    fn a_recipe_run_can_be_started_and_followed() {
        let mut host = host();
        host.recipes_open = true;
        host.recipe_open = Some("rcp_1".into());
        host.recipe_detail = Some(RecipeDetailSnap {
            run_bot: Some("cw_1".into()),
            runnable: true,
            running: false,
            result: None,
            error: None,
            runs: vec![
                ("rrun_2".into(), "running".into(), false),
                ("rrun_1".into(), "finished".into(), true),
            ],
        });
        let tree = host.snapshot();
        let run = tree.find(ids::RECIPE_RUN).unwrap();
        assert!(run.enabled);
        assert_eq!(run.value.as_deref(), Some("cw_1"));
        assert!(tree.find(ids::RECIPE_RUN_RESULT).is_none());
        assert_eq!(
            tree.find(&ids::recipe_history_run("rrun_2"))
                .unwrap()
                .value
                .as_deref(),
            Some("running")
        );
        assert!(
            tree.find(&ids::recipe_history_run("rrun_1"))
                .unwrap()
                .states
                .contains(&"ok".to_string())
        );
        host.click(ids::RECIPE_RUN).unwrap();
        assert!(
            matches!(host.take_command().unwrap(), Command::RunOpenRecipe(bot) if bot == "cw_1")
        );

        host.recipe_detail.as_mut().unwrap().running = true;
        host.recipe_detail.as_mut().unwrap().runnable = false;
        let tree = host.snapshot();
        assert!(!tree.find(ids::RECIPE_RUN).unwrap().enabled);
        assert_eq!(
            tree.find(ids::RECIPE_RUN_RESULT).unwrap().value.as_deref(),
            Some("running")
        );
        assert!(
            host.click(ids::RECIPE_RUN).is_err(),
            "not while a run is going"
        );

        host.recipe_detail.as_mut().unwrap().running = false;
        host.recipe_detail.as_mut().unwrap().runnable = true;
        host.recipe_detail.as_mut().unwrap().result = Some("interrupted");
        assert_eq!(
            host.snapshot()
                .find(ids::RECIPE_RUN_RESULT)
                .unwrap()
                .value
                .as_deref(),
            Some("interrupted")
        );
        host.recipe_detail.as_mut().unwrap().error = Some("this recipe is not granted".into());
        assert_eq!(
            host.snapshot().find(ids::RECIPE_ERROR).unwrap().name,
            "this recipe is not granted"
        );
        host.invoke("recipe.run", &serde_json::json!({ "bot": "cw_2" }))
            .unwrap();
        assert!(
            matches!(host.take_command().unwrap(), Command::RunOpenRecipe(bot) if bot == "cw_2")
        );
    }

    /// Run is on offer exactly when the page would enable it: a bot, a version to play, and
    /// nothing else under way. A save or an accept in flight is as good as a run in flight: a
    /// second start would drop the run the page follows, and one under a save loses its line
    /// when the save lands. The click and `recipe.run` both refuse then.
    #[test]
    fn a_recipe_is_run_from_the_tree_only_when_the_page_would_run_it() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../fixtures/wire/rest/GET__recipes__id_/200-a_run_whose_caller_hung_up_still_lands_in_history_with_its_pictures.json"
        ))
        .unwrap();
        let detail: crate::opengrok::RecipeDetail =
            serde_json::from_value(fixture["body"].clone()).unwrap();
        let mut state = AppState::new();
        // Signed in with a bot on the roster, or the tree is the sign-in or empty-roster page
        // and has no Recipes in it.
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_9", "name": "Bo" })).unwrap(),
        ];
        state.page = crate::state::MainPage::Recipes;
        state.recipe_open_id = Some(detail.recipe.id.clone());
        state.recipe_open = Some(detail);
        let signed_in = |state: &AppState| {
            let mut host = NativeChatHost::from_app(state);
            host.signed_in = true;
            host
        };

        let mut host = signed_in(&state);
        assert!(host.snapshot().find(ids::RECIPE_RUN).unwrap().enabled);
        host.click(ids::RECIPE_RUN).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::RunOpenRecipe(_))
        ));

        for busy in [crate::state::RECIPE_RUNNING, "Saving…", "Accepting…"] {
            state.recipe_busy = Some(busy.to_string());
            let mut host = signed_in(&state);
            assert!(
                !host.snapshot().find(ids::RECIPE_RUN).unwrap().enabled,
                "{busy}"
            );
            assert!(host.click(ids::RECIPE_RUN).is_err(), "{busy}");
            assert!(
                host.invoke("recipe.run", &serde_json::json!({})).is_err(),
                "{busy}"
            );
            assert!(host.take_command().is_none(), "{busy}");
        }
    }

    /// The sidebar row's value is the hover card's own line: whitespace folded, a long message
    /// cut with an ellipsis, and "No messages yet" for a thread with nothing in it.
    #[test]
    fn a_sidebar_row_reads_as_its_hover_card() {
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({
                "id": "acct_1", "email": "a@b.c"
            }))
            .unwrap(),
        );
        state.coworkers = ["cw_1", "cw_2"]
            .iter()
            .map(|id| serde_json::from_value(serde_json::json!({ "id": id, "name": id })).unwrap())
            .collect();
        let said = "two   meetings\n\ttoday ".repeat(20);
        state.conversations.push(crate::state::Conversation {
            id: "cw_1".into(),
            title: "cw_1".into(),
            created_at: String::new(),
            updated_at: "2026-09-27 10:00:00".into(),
            messages: vec![crate::state::Message {
                id: "m_1".into(),
                sender: "AI".into(),
                content: said.clone(),
                sent_at: std::time::SystemTime::UNIX_EPOCH,
                finished_at: None,
                run_timing: None,
                reply_source: None,
                is_me: false,
                reply_preview: None,
                reply_to_id: None,
                reply_is_me: false,
                parts: Vec::new(),
                run_id: None,
                hidden: false,
            }],
            unread_count: 0,
            origin: None,
        });
        let host = NativeChatHost::from_app(&state);
        let tree = host.snapshot();
        let listed = tree.find(&ids::coworker("cw_1")).unwrap();
        let expected = crate::components::sidebar::rail_preview(state.conversations.first()).0;
        assert_eq!(listed.value.as_deref(), Some(expected.as_str()));
        assert!(
            !expected.contains("  ") && expected.ends_with('…'),
            "{expected}"
        );
        assert!(listed.states.contains(&"listed".to_string()));
        let empty = tree.find(&ids::coworker("cw_2")).unwrap();
        assert_eq!(empty.value.as_deref(), Some("No messages yet"));
        assert!(!empty.states.contains(&"listed".to_string()));
    }

    /// A sidebar row carries what the bot's thread last said, and says when that thread came
    /// from the server's list rather than from this Mac: the merge of the two, visible.
    #[test]
    fn a_sidebar_row_says_what_its_thread_holds_and_where_it_came_from() {
        let mut host = host();
        host.sessions = vec![
            SessionSnap {
                id: "cw_1".into(),
                title: "Ada".into(),
                active: true,
                preview: Some("Two meetings.".into()),
                listed: true,
            },
            SessionSnap {
                id: "cw_2".into(),
                title: "Bo".into(),
                active: false,
                preview: None,
                listed: false,
            },
        ];
        let tree = host.snapshot();
        let ada = tree.find(&ids::coworker("cw_1")).unwrap();
        assert_eq!(ada.value.as_deref(), Some("Two meetings."));
        assert!(ada.states.contains(&"listed".to_string()));
        let bo = tree.find(&ids::coworker("cw_2")).unwrap();
        assert_eq!(bo.value, None);
        assert!(!bo.states.contains(&"listed".to_string()));
    }

    /// Why this Mac stopped running commands for the server by itself is a line on Settings →
    /// Computer, in the page's words, and in the tree only while the page shows it.
    #[test]
    fn why_this_mac_stopped_running_commands_is_a_line_on_settings_computer() {
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.is_app_settings_open = true;
        state.app_settings_tab = AppSettingsTab::Computer;
        assert!(
            NativeChatHost::from_app(&state)
                .snapshot()
                .find(ids::LOCAL_EXEC_STOPPED)
                .is_none()
        );

        let why =
            crate::opengrok::LocalExecStopped::NotEnrolled("could not enrol the machine".into());
        state.local_exec_stopped = Some(why.clone());
        let tree = NativeChatHost::from_app(&state).snapshot();
        let line = tree
            .find(ids::LOCAL_EXEC_STOPPED)
            .expect("the line is drawn");
        assert_eq!(line.name, why.sentence());
        assert!(
            line.name.contains("could not enrol the machine"),
            "{}",
            line.name
        );

        state.app_settings_tab = AppSettingsTab::Logins;
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert!(
            tree.find(ids::LOCAL_EXEC_STOPPED).is_none(),
            "only on Computer"
        );
    }

    /// A host on Settings → Computer with this Mac's rules as the server listed them: two
    /// allows, the second of which the gate never reads, and a deny.
    fn rules_host() -> NativeChatHost {
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.local_rules = Some(LocalRules {
            machine_id: "mac_1".into(),
            allow: vec![
                LocalRuleRow {
                    pattern: "ls -la".into(),
                    inert: None,
                },
                LocalRuleRow {
                    pattern: "sudo ls".into(),
                    inert: Some("sudo cannot be a standing allow".into()),
                },
            ],
            deny: vec![LocalRuleRow {
                pattern: "rm -rf /tmp/x".into(),
                inert: None,
            }],
            listed: true,
            ..LocalRules::default()
        });
        host
    }

    /// Each of this Mac's rules is a row a driver reads the command off exactly, under the list
    /// it is on, with a Remove of its own. An allow the gate never reads says so as a state, and
    /// why in the server's words.
    #[test]
    fn this_macs_rules_are_rows_of_their_commands_each_with_a_remove() {
        let tree = rules_host().snapshot();
        let allow = tree.find(&ids::local_rules(RuleKind::Allow)).unwrap();
        assert_eq!(allow.name, "Always allowed");
        assert_eq!(allow.value.as_deref(), Some("2"));
        let deny = tree.find(&ids::local_rules(RuleKind::Deny)).unwrap();
        assert_eq!(deny.name, "Never allowed");
        assert_eq!(deny.value.as_deref(), Some("1"));

        let read = tree.find("settings-local-rule-allow-0").unwrap();
        assert_eq!(read.value.as_deref(), Some("ls -la"));
        assert!(read.states.is_empty());
        assert!(tree.find("settings-local-rule-inert-allow-0").is_none());

        let never_read = tree.find("settings-local-rule-allow-1").unwrap();
        assert_eq!(never_read.value.as_deref(), Some("sudo ls"));
        assert_eq!(never_read.states, vec!["inert".to_string()]);
        let why = tree.find("settings-local-rule-inert-allow-1").unwrap();
        assert_eq!(why.name, "Not in effect: sudo cannot be a standing allow.");
        assert_eq!(
            why.value.as_deref(),
            Some("sudo cannot be a standing allow")
        );

        assert_eq!(
            tree.find("settings-local-rule-deny-0")
                .unwrap()
                .value
                .as_deref(),
            Some("rm -rf /tmp/x")
        );
        for id in [
            "settings-local-rule-remove-allow-0",
            "settings-local-rule-remove-allow-1",
            "settings-local-rule-remove-deny-0",
        ] {
            let button = tree.find(id).unwrap();
            assert_eq!(button.name, "Remove");
            assert!(button.enabled, "{id}");
        }
        assert!(tree.find(ids::LOCAL_RULES_EMPTY).is_none());
        assert!(tree.find(ids::LOCAL_RULES_ERROR).is_none());
    }

    /// Remove takes off the rule at that place, named by its command, and only while a person
    /// could press it: with the page on screen, on a row that is there, and not while that
    /// rule's Remove is still with the server.
    #[test]
    fn remove_names_its_rule_by_command_and_only_while_it_can_be_pressed() {
        let mut host = rules_host();
        host.dispatch(&Op::click("settings-local-rule-remove-deny-0"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::RemoveLocalRule { kind: RuleKind::Deny, pattern }) if pattern == "rm -rf /tmp/x"
        ));
        host.dispatch(&Op::click("settings-local-rule-remove-allow-1"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::RemoveLocalRule { kind: RuleKind::Allow, pattern }) if pattern == "sudo ls"
        ));
        assert!(
            host.dispatch(&Op::click("settings-local-rule-remove-deny-1"))
                .is_err(),
            "no rule is there"
        );

        host.local_rules
            .as_mut()
            .unwrap()
            .removing
            .insert((RuleKind::Allow, "ls -la".into()));
        let tree = host.snapshot();
        assert!(
            tree.find("settings-local-rule-allow-0")
                .unwrap()
                .states
                .contains(&"removing".to_string())
        );
        let button = tree.find("settings-local-rule-remove-allow-0").unwrap();
        assert_eq!(button.name, "Removing…");
        assert!(!button.enabled);
        assert!(
            host.dispatch(&Op::click("settings-local-rule-remove-allow-0"))
                .is_err()
        );
        assert!(host.take_command().is_none());

        host.computer_tab = false;
        host.updates_tab = true;
        assert!(
            host.snapshot()
                .find("settings-local-rule-allow-1")
                .is_none()
        );
        assert!(
            host.dispatch(&Op::click("settings-local-rule-remove-allow-1"))
                .is_err()
        );
        assert!(host.take_command().is_none());
    }

    /// No rules is a line saying so. Lists that could not be read say why instead, and never
    /// that there are none. A Remove that did not go through says why under its own row.
    #[test]
    fn no_rules_says_so_and_a_failure_says_why_where_it_happened() {
        let mut host = rules_host();
        host.local_rules = Some(LocalRules {
            machine_id: "mac_1".into(),
            listed: true,
            ..LocalRules::default()
        });
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::LOCAL_RULES_EMPTY).unwrap().name,
            NO_LOCAL_RULES
        );
        assert!(tree.find(&ids::local_rules(RuleKind::Allow)).is_none());
        assert!(tree.find(&ids::local_rules(RuleKind::Deny)).is_none());

        host.local_rules = Some(LocalRules {
            machine_id: "mac_1".into(),
            error: Some("This Mac's rules could not be read.".into()),
            ..LocalRules::default()
        });
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::LOCAL_RULES_ERROR).unwrap().name,
            "This Mac's rules could not be read."
        );
        assert!(tree.find(ids::LOCAL_RULES_EMPTY).is_none());

        let mut host = rules_host();
        host.local_rules.as_mut().unwrap().not_removed.insert(
            (RuleKind::Deny, "rm -rf /tmp/x".into()),
            "Not removed: could not remove the rule.".into(),
        );
        let tree = host.snapshot();
        assert_eq!(
            tree.find("settings-local-rule-error-deny-0").unwrap().name,
            "Not removed: could not remove the rule."
        );
        assert!(tree.find("settings-local-rule-error-allow-0").is_none());
    }

    fn open_choice(message_id: &str, keyed: bool) -> ChoiceSnap {
        ChoiceSnap {
            message_id: message_id.into(),
            title: "How do you want to continue?".into(),
            state: "open",
            one_question: true,
            keyed,
            fields: vec![ChoiceFieldSnap {
                label: "Next".into(),
                options: vec!["Try again".into(), "Hand me the screen".into()],
                picked: None,
            }],
            answer: None,
        }
    }

    /// A choice card can be read, picked and put away from a snapshot, and a letter pressed at
    /// it goes to the window with the caret out of the composer, the way a person's does.
    #[test]
    fn a_choice_card_is_on_the_tree_and_works_from_it() {
        let mut host = host();
        host.choices = vec![
            ChoiceSnap {
                state: "answered",
                answer: Some("Next: Try again".into()),
                ..open_choice("m_1", false)
            },
            open_choice("m_2", true),
        ];
        let tree = host.snapshot();
        let card = tree.find("choice-m_2").unwrap();
        assert_eq!(card.value.as_deref(), Some("open"));
        assert!(card.states.iter().any(|state| state == "keyboard"));
        assert_eq!(
            tree.find("choice-m_2-0-1").unwrap().value.as_deref(),
            Some("B")
        );
        assert!(tree.find("choice-m_2-dismiss").is_some());
        assert!(
            tree.find("choice-m_2-submit").is_none(),
            "one question answers on the pick"
        );
        assert_eq!(
            tree.find("choice-m_1-answer").unwrap().name,
            "Next: Try again"
        );
        assert!(
            tree.find("choice-m_1-0-0").is_none(),
            "an answered card offers nothing"
        );

        host.click("choice-m_2-0-1").unwrap();
        match host.take_command() {
            Some(Command::Choose {
                message_id,
                field_index: 0,
                option_index: 1,
            }) => assert_eq!(message_id, "m_2"),
            other => panic!("expected a pick, got {other:?}"),
        }
        host.click("choice-m_2-dismiss").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ChoiceDismiss { .. })
        ));
        assert!(host.click("choice-m_1-0-0").is_err());

        // One message id may be another's with a dash and more after it; each card's
        // choices still answer to their own card.
        host.choices = vec![open_choice("m", false), open_choice("m-1", true)];
        host.click("choice-m-1-0-0").unwrap();
        match host.take_command() {
            Some(Command::Choose {
                message_id,
                option_index: 0,
                ..
            }) => assert_eq!(message_id, "m-1"),
            other => panic!("expected m-1's pick, got {other:?}"),
        }

        assert!(
            host.dispatch(&Op::key("choice-m", "a")).is_err(),
            "a letter aimed at a card it would not answer is refused, not pressed"
        );
        let plan = keys(&mut host, Op::key("choice-m-1", "b"));
        assert!(plan.release_caret && !plan.focus_composer);
        assert_eq!(plan.keys, vec!["b"]);
    }

    /// A ceiling as the server gives it: `shell` on, `read_file` off, `user_machine_shell` off
    /// with no machine to run it, the Gmail plugin off, and `old_crm` a plugin the server no
    /// longer loads, still on.
    fn a_ceiling() -> crate::state::CeilingRead {
        crate::state::CeilingRead {
            rows: serde_json::from_value(serde_json::json!([
                {"name": "shell", "kind": "builtin", "enabled": true,
                    "description": "Run a shell command."},
                {"name": "read_file", "kind": "builtin", "enabled": false},
                {"name": "user_machine_shell", "kind": "builtin", "enabled": false,
                    "available": false},
                {"name": "gmail", "kind": "plugin", "enabled": false, "label": "Gmail",
                    "connector": "gmail"},
                {"name": "old_crm", "kind": "plugin", "enabled": true, "available": false}
            ]))
            .unwrap(),
            version: Some(3),
        }
    }

    /// The Tools card for `ceiling`, with nothing with the server and nothing said.
    fn ceiling_card(ceiling: ToolCeiling) -> CeilingCard {
        CeilingCard {
            ceiling,
            pending: None,
            blocked: None,
            note: None,
        }
    }

    fn switched(host: &mut NativeChatHost, target: &str) -> (String, bool) {
        host.dispatch(&Op::click(target)).unwrap();
        match host.take_command() {
            Some(Command::SetCeilingTool { name, enabled }) => (name, enabled),
            other => panic!("expected a switch from {target}, got {other:?}"),
        }
    }

    /// The bot's Tools card is on the tree as its settings draw it: what the next turn is offered
    /// always, the ceiling's line once the server has answered it, the switches only while the
    /// card is open, and the toggle only while there is something to show. Where the ceiling
    /// could not be read, what the next turn is offered is listed instead, read-only.
    #[test]
    fn a_bots_tools_are_on_the_tree_and_open_from_it() {
        use crate::opengrok::CoworkerTool;
        let mut host = host();
        host.agent_settings_open = true;
        host.agent_tools = Some(ToolList::Loading);
        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Loading));
        let tree = host.snapshot();
        assert_eq!(
            tree.find("agent-tools").unwrap().value.as_deref(),
            Some("Asking the server…")
        );
        assert!(
            tree.find(ids::AGENT_CEILING).is_none(),
            "nothing is said about the ceiling until the server has answered, as on screen"
        );
        assert!(host.dispatch(&Op::click("agent-tools-toggle")).is_err());

        host.agent_tools = Some(ToolList::Listed(vec![
            CoworkerTool {
                name: "shell".into(),
                description: "Run a shell command.".into(),
                kind: "builtin".into(),
            },
            CoworkerTool {
                name: "gmail_api_send".into(),
                description: String::new(),
                kind: "plugin".into(),
            },
        ]));
        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Read(a_ceiling())));
        let tree = host.snapshot();
        assert_eq!(
            tree.find("agent-tools").unwrap().value.as_deref(),
            Some("1 built in · 1 from plugins"),
            "what the next turn is offered is still the server's list"
        );
        assert_eq!(
            tree.find(ids::AGENT_CEILING).unwrap().value.as_deref(),
            Some("2 of 5 allowed")
        );
        assert!(
            tree.find(&ids::offered_tool("shell")).is_none(),
            "the switches say it"
        );
        let shell = tree.find("agent-ceiling-switch-shell").unwrap();
        assert_eq!(
            (shell.role.as_str(), shell.value.as_deref(), shell.checked),
            ("switch", Some("builtin"), Some(true))
        );
        assert!(
            !shell.visible,
            "closed card: the switches are not on screen"
        );
        let gmail = tree.find("agent-ceiling-switch-gmail").unwrap();
        assert_eq!((gmail.name.as_str(), gmail.checked), ("Gmail", Some(false)));
        assert_eq!(
            tree.find("agent-ceiling-connector-gmail").unwrap().name,
            "Uses your Gmail connection"
        );
        let machine = tree
            .find("agent-ceiling-switch-user_machine_shell")
            .unwrap();
        assert!(
            machine.states.contains(&"unavailable".to_string()) && machine.enabled,
            "a builtin the server cannot offer now is switched all the same"
        );
        assert_eq!(
            tree.find("agent-ceiling-why-user_machine_shell")
                .unwrap()
                .name,
            "Not available until this Mac runs commands for Bots"
        );
        assert_eq!(
            tree.find("agent-ceiling-why-old_crm").unwrap().name,
            "No longer on the server"
        );
        assert!(tree.ids_are_unique());

        host.dispatch(&Op::click("agent-tools-toggle")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleAgentTools)
        ));
        host.agent_tools_open = true;
        assert!(
            host.snapshot()
                .find("agent-ceiling-switch-shell")
                .unwrap()
                .visible
        );

        // With the settings closed, the toggle is not on screen to click.
        host.agent_settings_open = false;
        assert!(host.dispatch(&Op::click("agent-tools-toggle")).is_err());

        // A ceiling the server would not give has a line and no switches, and the card lists what
        // the next turn is offered instead, as it did before there were switches.
        host.agent_settings_open = true;
        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Unavailable(
            crate::state::CEILING_NOT_ON_SERVER.into(),
        )));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_CEILING).unwrap().value.as_deref(),
            Some(crate::state::CEILING_NOT_ON_SERVER)
        );
        assert!(tree.find("agent-ceiling-switch-shell").is_none());
        let offered = tree.find(&ids::offered_tool("gmail_api_send")).unwrap();
        assert_eq!(
            (
                offered.role.as_str(),
                offered.value.as_deref(),
                offered.visible
            ),
            ("listitem", Some("plugin"), true)
        );
        host.dispatch(&Op::click("agent-tools-toggle")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleAgentTools)
        ));

        // Nothing to list either, and there is nothing to open.
        host.agent_tools = Some(ToolList::Unavailable(
            "Only this bot's owner can see its tools.".into(),
        ));
        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Unavailable(
            crate::state::NOT_THE_OWNER.into(),
        )));
        assert!(host.dispatch(&Op::click("agent-tools-toggle")).is_err());
    }

    /// A switch is clicked as a person clicks it — the other way from where it stands — and only
    /// where a person could: the settings open, the card open to its switches, and the switch
    /// live. A builtin the server cannot offer now goes either way. A card with no switches says
    /// why it has none.
    #[test]
    fn a_ceiling_switch_is_clicked_only_where_a_person_could() {
        let mut host = host();
        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Read(a_ceiling())));
        assert!(
            host.dispatch(&Op::click("agent-ceiling-switch-shell"))
                .unwrap_err()
                .contains("settings, which are closed")
        );
        host.agent_settings_open = true;
        assert!(
            host.dispatch(&Op::click("agent-ceiling-switch-shell"))
                .unwrap_err()
                .contains("agent-tools-toggle"),
            "the card is shut, so its switches are not on screen"
        );
        host.agent_tools_open = true;

        assert_eq!(
            switched(&mut host, "agent-ceiling-switch-shell"),
            ("shell".into(), false)
        );
        assert_eq!(
            switched(&mut host, "agent-ceiling-switch-gmail"),
            ("gmail".into(), true)
        );
        assert_eq!(
            switched(&mut host, "agent-ceiling-switch-old_crm"),
            ("old_crm".into(), false),
            "a plugin the server no longer loads can still be taken off"
        );
        assert_eq!(
            switched(&mut host, "agent-ceiling-switch-user_machine_shell"),
            ("user_machine_shell".into(), true),
            "a builtin the server cannot offer now can be allowed for when it can"
        );

        assert!(
            host.dispatch(&Op::click("agent-ceiling-why-old_crm"))
                .unwrap_err()
                .contains("not a switch")
        );
        assert!(
            host.dispatch(&Op::click(ids::AGENT_CEILING))
                .unwrap_err()
                .contains("not a switch")
        );
        assert!(
            host.dispatch(&Op::click("agent-ceiling-switch-nope"))
                .unwrap_err()
                .contains("no switch")
        );
        assert!(
            host.dispatch(&Op::click("agent-ceiling-shell"))
                .unwrap_err()
                .contains("no switch"),
            "the old id names nothing"
        );

        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Unavailable(
            crate::state::NOT_THE_OWNER.into(),
        )));
        let none = host
            .dispatch(&Op::click("agent-ceiling-switch-shell"))
            .unwrap_err();
        assert!(
            none.contains(crate::state::NOT_THE_OWNER) && !none.contains("have not been"),
            "{none}"
        );
        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Loading));
        assert!(
            host.dispatch(&Op::click("agent-ceiling-switch-shell"))
                .unwrap_err()
                .contains("still being read")
        );
    }

    /// While a switch is with the server its row shows where it was asked to go, and nothing on
    /// the card can be clicked, whichever Bot the switch is for; the card says why when it is not
    /// this Bot's own switch holding it. The server's words are under their row, a card note is
    /// above the switches, and a card the server has made read-only is dead with its words on it.
    #[test]
    fn a_blocked_ceiling_card_is_dead_and_says_why() {
        use crate::state::{CeilingBlock, CeilingNote, CeilingSwitch, NotePlace};
        let mut host = host();
        host.agent_settings_open = true;
        host.agent_tools_open = true;
        host.agent_ceiling = Some(CeilingCard {
            pending: Some(CeilingSwitch {
                coworker_id: "bot-1".into(),
                name: "read_file".into(),
                enabled: true,
                token: 1,
            }),
            blocked: Some(CeilingBlock::Switching),
            ..ceiling_card(ToolCeiling::Read(a_ceiling()))
        });
        let tree = host.snapshot();
        let asked = tree.find("agent-ceiling-switch-read_file").unwrap();
        assert_eq!(asked.checked, Some(true));
        assert!(asked.states.contains(&"switching".to_string()) && !asked.enabled);
        assert!(!tree.find("agent-ceiling-switch-shell").unwrap().enabled);
        assert!(
            tree.find(ids::AGENT_CEILING_WAIT).is_none(),
            "its row says it"
        );
        assert_eq!(
            tree.find(ids::AGENT_CEILING).unwrap().value.as_deref(),
            Some("3 of 5 allowed")
        );
        let busy = host
            .dispatch(&Op::click("agent-ceiling-switch-shell"))
            .unwrap_err();
        assert!(
            busy.contains(crate::state::CEILING_SWITCH_IN_FLIGHT),
            "{busy}"
        );

        // Another Bot's switch, then a read of this one's: nothing of the switch is drawn here,
        // it still holds every switch, and the card says what it is waiting on.
        for (blocked, why) in [
            (CeilingBlock::AnotherBot, crate::state::ANOTHER_BOTS_SWITCH),
            (CeilingBlock::Reading, crate::state::CEILING_BEING_READ),
        ] {
            host.agent_ceiling = Some(CeilingCard {
                blocked: Some(blocked),
                ..ceiling_card(ToolCeiling::Read(a_ceiling()))
            });
            let tree = host.snapshot();
            assert_eq!(
                tree.find("agent-ceiling-switch-read_file").unwrap().checked,
                Some(false)
            );
            assert_eq!(tree.find(ids::AGENT_CEILING_WAIT).unwrap().name, why);
            let waiting = host
                .dispatch(&Op::click("agent-ceiling-switch-shell"))
                .unwrap_err();
            assert!(waiting.contains(why), "{waiting}");
        }

        let said = |words: &str, place: NotePlace| {
            Some(CeilingNote {
                coworker_id: "bot-1".into(),
                words: words.into(),
                place,
            })
        };
        host.agent_ceiling = Some(CeilingCard {
            note: said(
                "no tool or plugin named read_file",
                NotePlace::Row("read_file".into()),
            ),
            ..ceiling_card(ToolCeiling::Read(a_ceiling()))
        });
        let tree = host.snapshot();
        assert_eq!(
            tree.find("agent-ceiling-error-read_file").unwrap().name,
            "no tool or plugin named read_file"
        );
        assert!(tree.find("agent-ceiling-error-shell").is_none());
        assert!(tree.find(ids::AGENT_CEILING_NOTE).is_none());

        host.agent_ceiling = Some(CeilingCard {
            note: said("the tools changed since you looked", NotePlace::Card),
            ..ceiling_card(ToolCeiling::Read(a_ceiling()))
        });
        assert_eq!(
            host.snapshot().find(ids::AGENT_CEILING_NOTE).unwrap().name,
            "the tools changed since you looked"
        );

        host.agent_ceiling = Some(CeilingCard {
            blocked: Some(CeilingBlock::ReadOnly("no grant to change".into())),
            note: said("no grant to change", NotePlace::ReadOnly),
            ..ceiling_card(ToolCeiling::Read(a_ceiling()))
        });
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_CEILING_READ_ONLY).unwrap().name,
            "no grant to change"
        );
        assert!(tree.find(ids::AGENT_CEILING_NOTE).is_none(), "said once");
        assert!(!tree.find("agent-ceiling-switch-shell").unwrap().enabled);
        let theirs = host
            .dispatch(&Op::click("agent-ceiling-switch-shell"))
            .unwrap_err();
        assert!(theirs.contains("no grant to change"), "{theirs}");
    }

    /// No name a plugin can have makes one of the card's ids another's: a plugin called
    /// `read-only`, `note`, `why-x`, `error-x`, `connector-x` or `switch-x` is its own switch,
    /// every id on the card is one node's, and a line is never read as a switch.
    #[test]
    fn no_plugins_name_makes_one_ceiling_id_another() {
        let names = [
            "x",
            "read-only",
            "note",
            "wait",
            "why-x",
            "error-x",
            "connector-x",
            "switch-x",
        ];
        let rows: Vec<serde_json::Value> = names
            .iter()
            .map(|name| {
                serde_json::json!({"name": name, "kind": "plugin", "enabled": true,
                    "available": false, "connector": name})
            })
            .collect();
        let read = crate::state::CeilingRead {
            rows: serde_json::from_value(serde_json::Value::Array(rows)).unwrap(),
            version: None,
        };
        let mut host = host();
        host.agent_settings_open = true;
        host.agent_tools_open = true;
        host.agent_ceiling = Some(CeilingCard {
            note: Some(crate::state::CeilingNote {
                coworker_id: "bot-1".into(),
                words: "the tools changed since you looked".into(),
                place: crate::state::NotePlace::Card,
            }),
            ..ceiling_card(ToolCeiling::Read(read))
        });
        let tree = host.snapshot();
        assert!(tree.ids_are_unique());
        for name in names {
            assert_eq!(
                switched(&mut host, &ids::ceiling_switch(name)),
                (name.to_string(), false),
                "{name}"
            );
        }
        for line in [
            ids::AGENT_CEILING_NOTE,
            ids::AGENT_CEILING_READ_ONLY,
            "agent-ceiling-why-x",
            "agent-ceiling-why-why-x",
            "agent-ceiling-connector-switch-x",
        ] {
            assert!(
                host.dispatch(&Op::click(line))
                    .unwrap_err()
                    .contains("not a switch"),
                "{line}"
            );
        }
    }

    /// The transcript says whether it keeps its newest row in view, so a driver that scrolls it
    /// up can assert that it let go, and that it stayed let go through the redraws after.
    #[test]
    fn the_transcript_says_whether_it_follows_its_newest_row() {
        let mut host = host();
        host.transcript_following = true;
        let tree = host.snapshot();
        let transcript = tree.find(ids::TRANSCRIPT).unwrap();
        assert_eq!(transcript.role, "scroll");
        assert_eq!(transcript.value.as_deref(), Some("following"));

        host.transcript_following = false;
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::TRANSCRIPT).unwrap().value.as_deref(),
            Some("detached")
        );

        // What the host says is what the transcript published, and a thread opens following.
        let mut state = AppState::new();
        assert!(NativeChatHost::from_app(&state).transcript_following);
        state.set_transcript_following(false);
        assert!(!NativeChatHost::from_app(&state).transcript_following);
    }

    /// The draft's chips are on the tree as objects under the composer, in order, with their kind.
    #[test]
    fn a_drafts_chips_are_on_the_tree() {
        use crate::components::chat_input::TokenKind;
        let mut host = host();
        host.composer_chips = vec![
            (TokenKind::Recipe, "Weekly report".into()),
            (TokenKind::Skill, "Tone".into()),
        ];
        let tree = host.snapshot();
        let recipe = tree.find("composer-chip-0").unwrap();
        assert_eq!(recipe.name, "Weekly report");
        assert_eq!(recipe.value.as_deref(), Some("recipe"));
        assert_eq!(
            tree.find("composer-chip-1").unwrap().value.as_deref(),
            Some("skill")
        );
        assert!(tree.find("composer-chip-2").is_none());
    }

    /// The draft's files and a thread's sent files are on the tree, and a driver can attach one.
    #[test]
    fn files_are_on_the_tree_and_a_driver_can_attach_one() {
        let mut host = host();
        host.composer_files = vec![("q3.pdf".into(), "uploading"), ("a.png".into(), "ready")];
        host.sent_files = vec![("art_1".into(), "notes.txt".into(), "msg_1".into())];
        let tree = host.snapshot();
        assert_eq!(
            tree.find("composer-file-0").unwrap().value.as_deref(),
            Some("uploading")
        );
        assert_eq!(tree.find("composer-file-1").unwrap().name, "a.png");
        let sent = tree.find("message-file-art_1").unwrap();
        assert_eq!(
            (sent.name.as_str(), sent.value.as_deref()),
            ("notes.txt", Some("msg_1"))
        );
        host.invoke(
            "composer.attach",
            &serde_json::json!({"path": "/tmp/q3.pdf"}),
        )
        .unwrap();
        match host.take_command() {
            Some(Command::AttachFile(path)) => {
                assert_eq!(path, std::path::PathBuf::from("/tmp/q3.pdf"))
            }
            other => panic!("expected an attach, got {other:?}"),
        }
        assert!(
            host.invoke("composer.attach", &serde_json::json!({}))
                .is_err()
        );
        assert!(
            host.invoke("composer.attach", &serde_json::json!({"path": "q3.pdf"}))
                .is_err(),
            "a relative path would resolve against the app's directory"
        );
        assert!(
            host.invoke(
                "composer.attach",
                &serde_json::json!({"path": "/tmp/a.zip"})
            )
            .is_err(),
            "a kind the server does not take is refused at once"
        );
        host.invoke("composer.detach", &serde_json::json!({"index": "1"}))
            .unwrap();
        assert!(matches!(host.take_command(), Some(Command::DetachFile(1))));
        assert!(
            host.invoke("composer.detach", &serde_json::json!({"index": "5"}))
                .is_err()
        );
    }

    /// The Usage card is on the tree with what the server said the bot used (#138).
    #[test]
    fn a_bots_usage_is_on_the_tree() {
        use crate::opengrok::{CoworkerUsage, ModelUsage, UsageTotals};
        let mut host = host();
        host.agent_settings_open = true;
        host.agent_usage = Some(crate::state::UsageReport::Read(CoworkerUsage {
            metered: true,
            note: None,
            window: "month".into(),
            models: vec![ModelUsage {
                model_id: "oag/cheap".into(),
                requests: 2,
                input_tokens: 20,
                output_tokens: 10,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cost_usd: "2.000000".into(),
            }],
            totals: UsageTotals {
                requests: Some(2),
                input_tokens: Some(20),
                output_tokens: Some(10),
                cache_read_tokens: Some(0),
                cache_write_tokens: Some(0),
                cost_usd: Some("2.000000".into()),
            },
        }));
        let tree = host.snapshot();
        assert_eq!(
            tree.find("agent-usage").unwrap().value.as_deref(),
            Some("2 requests this month · 30 tokens · $2.00")
        );
        let row = tree.find("agent-usage-model-0").unwrap();
        assert_eq!(row.name, "oag/cheap · 2 requests · 30 tokens · $2.00");
        assert!(
            !row.visible,
            "closed card: the model lines are not on screen"
        );
        host.dispatch(&Op::click("agent-usage-toggle")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleAgentUsage)
        ));
        host.agent_usage_open = true;
        assert!(host.snapshot().find("agent-usage-model-0").unwrap().visible);

        // With the settings closed, the toggle is not on screen to click.
        host.agent_settings_open = false;
        assert!(host.dispatch(&Op::click("agent-usage-toggle")).is_err());
        host.agent_settings_open = true;

        // No models, nothing to open: the toggle is not there to click.
        host.agent_usage = Some(crate::state::UsageReport::Loading);
        assert!(host.snapshot().find("agent-usage-toggle").is_none());
        assert!(host.dispatch(&Op::click("agent-usage-toggle")).is_err());
    }

    /// One of the person's own connections, as `GET /connections` lists it.
    fn connection(id: &str, connector: &str, loans: &[&str]) -> crate::opengrok::ConnectionView {
        crate::opengrok::ConnectionView {
            id: id.into(),
            connector: connector.into(),
            owner: crate::opengrok::ConnectionOwner::User("acct_1".into()),
            label: format!("{connector} account"),
            loans: loans.iter().map(|lent| lent.to_string()).collect(),
            updated_at_ms: 1,
            expires_at_ms: None,
        }
    }

    /// A connection that is a bot's own sign-in, and not the person's to lend or disconnect.
    fn bots_own(id: &str, connector: &str) -> crate::opengrok::ConnectionView {
        crate::opengrok::ConnectionView {
            owner: crate::opengrok::ConnectionOwner::Bot("bot-1".into()),
            ..connection(id, connector, &[])
        }
    }

    fn service(name: &str, label: &str) -> crate::opengrok::Connector {
        crate::opengrok::Connector {
            name: name.into(),
            label: label.into(),
        }
    }

    /// Settings → Connections is on the tree as the page draws it (#2): each of the person's
    /// own connections with the line under it (the service, and who it is lent to by name) and
    /// its Disconnect, a Connect for each service on offer that is not connected, and what a
    /// refusal said. Each click is the page's own, and refused off the page and while its
    /// control is dead; the tab itself only while Settings is open.
    #[test]
    fn the_connections_page_is_on_the_tree_and_clicks_as_the_page_does() {
        use crate::state::{BROWSER_WAIT, ConnectionChange, ConnectorList};
        let mut host = host();
        // Settings is shut: nobody can press its tab, and arriving on it would read the list.
        let shut = host.click(ids::SETTINGS_CONNECTIONS).unwrap_err();
        assert!(shut.contains(ids::FOOTER_ACCOUNT), "{shut}");
        assert!(host.take_command().is_none());
        // The tab answers from anywhere in Settings once it is open: it is how the page is
        // reached. The other tabs are as they were.
        host.account_open = true;
        host.click(ids::SETTINGS_CONNECTIONS).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetAppSettingsTab(AppSettingsTab::Connections))
        ));
        host.account_open = false;
        host.click("settings-tab-logins").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetAppSettingsTab(AppSettingsTab::Logins))
        ));

        host.connections.list = Some(ConnectionList::Listed(vec![
            connection("conn_1", "gmail", &["bot-1", "cw_gone"]),
            bots_own("conn_2", "drive"),
        ]));
        host.connections.connectors = Some(ConnectorList::Listed(vec![
            service("gmail", "Gmail"),
            service("github", "GitHub"),
        ]));
        assert!(
            host.snapshot().find(ids::CONNECTIONS).is_none(),
            "Settings is not open on Connections"
        );
        assert!(host.click(&ids::connection_disconnect("conn_1")).is_err());
        assert!(host.click(&ids::connect("github")).is_err());
        assert!(host.click(ids::CONNECTIONS_REFRESH).is_err());

        host.account_open = true;
        host.connections_tab = true;
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::CONNECTIONS).unwrap().value.as_deref(),
            Some("1"),
            "only the person's own"
        );
        let row = tree.find(&ids::connection("conn_1")).unwrap();
        assert_eq!(row.name, "gmail account");
        assert_eq!(
            row.value.as_deref(),
            Some("Gmail · Lent to Ada and 1 Bot not on your list")
        );
        assert!(tree.find(&ids::connection("conn_2")).is_none());
        assert!(
            tree.find(&ids::connect("gmail")).is_none(),
            "a service already connected is not offered"
        );
        assert_eq!(
            tree.find(ids::CONNECTORS).unwrap().value.as_deref(),
            Some("1")
        );
        assert_eq!(
            tree.find(&ids::connect("github")).unwrap().name,
            "Connect GitHub"
        );
        assert!(tree.ids_are_unique());

        // Disconnect asks first; its Yes and No are not there until it does.
        let yes = crate::components::connections::confirm_yes_id("conn_1");
        let no = crate::components::connections::confirm_no_id("conn_1");
        assert!(host.click(&yes).is_err(), "nothing to say yes to yet");
        host.click(&ids::connection_disconnect("conn_1")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::AskDisconnect(id)) if id == "conn_1"
        ));
        host.connections.confirming_disconnect = Some("conn_1".into());
        let tree = host.snapshot();
        assert_eq!(
            tree.find(&crate::components::connections::confirm_question_id(
                "conn_1"
            ))
            .unwrap()
            .name,
            "Disconnect Gmail? Ada and 1 Bot not on your list will lose it."
        );
        assert!(tree.find(&ids::connection_disconnect("conn_1")).is_none());
        assert!(tree.ids_are_unique());
        host.click(&no).unwrap();
        assert!(matches!(host.take_command(), Some(Command::KeepConnection)));
        host.click(&yes).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::DisconnectConnection(id)) if id == "conn_1"
        ));
        host.connections.confirming_disconnect = None;
        host.click(&ids::connect("github")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ConnectService(name)) if name == "github"
        ));
        host.click(ids::CONNECTIONS_REFRESH).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::RefreshConnections)
        ));
        assert!(host.click(&ids::connect("gmail")).is_err());
        assert!(host.click(&ids::connection_disconnect("conn_9")).is_err());
        assert!(
            host.click(&ids::connection_disconnect("conn_2")).is_err(),
            "a bot's own sign-in is not the person's to disconnect"
        );

        // A change with the server leaves its row dead and saying so, and a sign-in page being
        // asked for leaves every Connect dead.
        host.connections
            .changing
            .insert("conn_1".into(), ConnectionChange::Disconnect);
        host.connections.opening = Some("github".into());
        let tree = host.snapshot();
        let row = tree.find(&ids::connection("conn_1")).unwrap();
        assert!(row.states.contains(&"changing".to_string()));
        let disconnect = tree.find(&ids::connection_disconnect("conn_1")).unwrap();
        assert_eq!(
            (disconnect.name.as_str(), disconnect.enabled),
            ("Disconnecting…", false)
        );
        let connect = tree.find(&ids::connect("github")).unwrap();
        assert!(connect.states.contains(&"opening".to_string()) && !connect.enabled);
        assert!(host.click(&ids::connection_disconnect("conn_1")).is_err());
        assert!(host.click(&ids::connect("github")).is_err());

        // A lend with the server is on the row as it is on the bot's switch.
        host.connections
            .changing
            .insert("conn_1".into(), ConnectionChange::Revoke("bot-1".into()));
        assert_eq!(
            host.snapshot()
                .find(&ids::connection("conn_1"))
                .unwrap()
                .value
                .as_deref(),
            Some("Gmail · Lent to 1 Bot not on your list")
        );

        // A refusal of a Disconnect is under its row in the server's words, and a lend's is
        // not; the browser's wait is on its Connect, and a Connect that could not start says
        // why beside it.
        host.connections.changing.clear();
        host.connections.opening = None;
        host.connections.not_disconnected.insert(
            "conn_1".into(),
            "Not disconnected: the store is unavailable.".into(),
        );
        host.connections.not_changed.insert(
            ("conn_1".into(), "bot-1".into()),
            "Could not change this: no such connection.".into(),
        );
        host.connections
            .waiting
            .insert("github".into(), std::time::Instant::now() + BROWSER_WAIT);
        host.connections.connect_refused = Some((
            "github".into(),
            "GitHub could not be connected: no provider is configured for github.".into(),
        ));
        let tree = host.snapshot();
        let row = tree.find(&ids::connection("conn_1")).unwrap();
        let lines: Vec<&str> = row.children.iter().map(|line| line.name.as_str()).collect();
        assert_eq!(
            lines,
            ["Disconnect", "Not disconnected: the store is unavailable."]
        );
        assert!(
            tree.find(&ids::connect("github"))
                .unwrap()
                .states
                .contains(&"waiting".to_string())
        );
        assert_eq!(
            tree.find(&ids::connect_error("github")).unwrap().name,
            "GitHub could not be connected: no provider is configured for github."
        );

        // Each empty list says which it is, and a list that could not be read says why. A list
        // with none of the person's own in it is nothing connected.
        host.connections.list = Some(ConnectionList::Listed(vec![bots_own("conn_2", "drive")]));
        host.connections.connectors = Some(ConnectorList::Listed(Vec::new()));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::CONNECTIONS_EMPTY).unwrap().name,
            connections::NOTHING_CONNECTED
        );
        assert_eq!(
            tree.find(ids::CONNECTORS_EMPTY).unwrap().name,
            connections::NO_CONNECTORS
        );
        assert!(tree.find(ids::CONNECTIONS).is_none() && tree.find(ids::CONNECTORS).is_none());
        host.connections.list = Some(ConnectionList::Unavailable(
            "Your connections could not be read.".into(),
        ));
        host.connections.connectors = Some(ConnectorList::Unavailable(
            crate::state::CONNECTORS_NOT_LISTED.into(),
        ));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::CONNECTIONS_ERROR).unwrap().name,
            "Your connections could not be read."
        );
        assert_eq!(
            tree.find(ids::CONNECTORS_ERROR).unwrap().name,
            crate::state::CONNECTORS_NOT_LISTED
        );
    }

    /// The open bot's Connections card is on the tree while its settings are open: a switch
    /// per connection of the person's own, checked while it is lent to this bot, whose click
    /// asks for the other way; dead while a change to it is with the server; a refusal about
    /// this bot beside it, and no other bot's; and the card's sentence about what lending does
    /// not do yet (opengrok-server#268).
    #[test]
    fn a_bots_connections_card_lends_from_the_tree() {
        use crate::state::ConnectionChange;
        let mut host = host();
        host.connections.list = Some(ConnectionList::Listed(vec![
            connection("conn_1", "gmail", &["bot-1"]),
            connection("conn_2", "github", &[]),
            bots_own("conn_3", "drive"),
        ]));
        assert!(
            host.snapshot().find(ids::AGENT_CONNECTIONS).is_none(),
            "the bot's settings are closed"
        );
        assert!(host.click(&ids::connection_lend("conn_1")).is_err());

        host.agent_settings_open = true;
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_CONNECTIONS).unwrap().value.as_deref(),
            Some("1 of 2 lent to this Bot"),
            "only the person's own are counted"
        );
        let lent = tree.find(&ids::connection_lend("conn_1")).unwrap();
        assert_eq!(
            (lent.role.as_str(), lent.checked, lent.enabled),
            ("switch", Some(true), true)
        );
        assert_eq!(lent.value.as_deref(), Some("gmail"));
        assert_eq!(
            tree.find(&ids::connection_lend("conn_2")).unwrap().checked,
            Some(false)
        );
        assert!(tree.find(&ids::connection_lend("conn_3")).is_none());
        assert_eq!(
            tree.find(ids::AGENT_CONNECTIONS_NOTE).unwrap().name,
            connections::LEND_NOTE
        );
        assert!(tree.ids_are_unique());

        host.click(&ids::connection_lend("conn_1")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetConnectionLent { connection_id, lent: false }) if connection_id == "conn_1"
        ));
        host.click(&ids::connection_lend("conn_2")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetConnectionLent { connection_id, lent: true }) if connection_id == "conn_2"
        ));
        assert!(host.click(&ids::connection_lend("conn_3")).is_err());

        // With the server, the switch shows what was asked, and is dead.
        host.connections
            .changing
            .insert("conn_2".into(), ConnectionChange::Lend("bot-1".into()));
        let tree = host.snapshot();
        let asked = tree.find(&ids::connection_lend("conn_2")).unwrap();
        assert_eq!((asked.checked, asked.enabled), (Some(true), false));
        assert!(asked.states.contains(&"changing".to_string()));
        assert!(host.click(&ids::connection_lend("conn_2")).is_err());

        // Refused, it shows the server's word again, with the server's words beside it; a
        // refusal about lending it to another bot is that bot's card's.
        host.connections.changing.clear();
        host.connections.not_changed.insert(
            ("conn_2".into(), "bot-1".into()),
            "Could not change this: no such connection.".into(),
        );
        host.connections.not_changed.insert(
            ("conn_1".into(), "bot-2".into()),
            "Could not change this: no such connection.".into(),
        );
        let tree = host.snapshot();
        assert_eq!(
            tree.find(&ids::connection_lend("conn_2")).unwrap().checked,
            Some(false)
        );
        assert_eq!(
            tree.find(&ids::connection_lend_error("conn_2"))
                .unwrap()
                .name,
            "Could not change this: no such connection."
        );
        assert!(tree.find(&ids::connection_lend_error("conn_1")).is_none());
        assert!(host.click("agent-connection-lend-conn_9").is_err());
    }

    /// What the app holds is what the tree draws: a state's connections and roster come through
    /// `from_app`, and the bot's names are the roster's.
    #[test]
    fn the_connections_on_the_tree_are_the_apps() {
        let mut state = AppState::new();
        // Signed in, or the roster is not what the sidebar lists and no bot has a name.
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
            serde_json::from_value(serde_json::json!({ "id": "cw_2", "name": "Bo" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_2".into());
        state.connections.list = Some(ConnectionList::Listed(vec![connection(
            "conn_1",
            "gmail",
            &["cw_1", "cw_2"],
        )]));
        state.is_app_settings_open = true;
        state.app_settings_tab = AppSettingsTab::Connections;
        state.right_pane = crate::state::RightPane::Settings;
        let host = NativeChatHost::from_app(&state);
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_CONNECTIONS).unwrap().value.as_deref(),
            Some("1 of 1 lent to this Bot"),
            "the open bot is Bo, who has it"
        );
        assert_eq!(
            tree.find(&ids::connection("conn_1"))
                .unwrap()
                .value
                .as_deref(),
            Some("gmail · Lent to Ada and Bo")
        );
    }

    /// Save is on the tree and pressed by way of the app, since the fields it sends are the
    /// pane's; a refused Save is on the tree in the server's words, where the pane shows it.
    #[test]
    fn a_driver_saves_the_bots_settings_and_reads_a_refusal() {
        let mut host = host();
        assert!(
            host.dispatch(&Op::click(ids::AGENT_SAVE)).is_err(),
            "the settings are closed"
        );
        host.agent_settings_open = true;
        assert!(host.snapshot().find(ids::AGENT_SAVE).is_some());
        host.dispatch(&Op::click(ids::AGENT_SAVE)).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SaveAgentSettings)
        ));

        assert!(host.snapshot().find("agent-settings-error").is_none());
        let said = "effort must be one of inherit, none, low, medium, high, xhigh, max, ultra";
        host.auth_error = Some(said.into());
        assert_eq!(
            host.snapshot().find("agent-settings-error").unwrap().name,
            said
        );
    }

    fn kept_source(
        kind: crate::opengrok::InferenceKind,
        healthy: bool,
    ) -> crate::opengrok::InferenceSource {
        crate::opengrok::InferenceSource {
            kind,
            base_url: None,
            local_model: None,
            healthy,
            has_api_key: false,
            via: None,
            relay: None,
            new_bot_default: None,
            relay_enabled: None,
            plan_fallback: None,
        }
    }

    /// A read from a server that knows the relay, with nobody's computer holding it.
    fn relay_read(via: &str) -> Option<crate::state::ReplySourceRead> {
        Some(crate::state::ReplySourceRead::Read(
            crate::opengrok::InferenceSource {
                via: Some(via.into()),
                relay: Some(crate::opengrok::RelayRead {
                    connected: false,
                    machine_id: None,
                    machine_label: None,
                    local_model: None,
                }),
                ..kept_source(crate::opengrok::InferenceKind::Gateway, true)
            },
        ))
    }

    /// The Relay tab is gone from the tree: Settings' pages are General, Computer, Updates, Logins,
    /// Connections and Skills, nothing answers at the id the Relay tab had, and none of the ids of
    /// the page it held is on the tree or takes a click. The relay is on Computer, in each
    /// computer's card.
    #[test]
    fn the_relay_tab_is_gone_from_the_tree_and_answers_no_click() {
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.computers = vec![a_card("mac_1", true)];
        let tree = host.snapshot();
        for id in [
            ids::SETTINGS_GENERAL,
            "settings-tab-computer",
            "settings-tab-updates",
            "settings-tab-logins",
            ids::SETTINGS_CONNECTIONS,
            ids::SETTINGS_SKILLS,
        ] {
            assert!(tree.find(id).is_some(), "{id} is a page of Settings");
        }
        for gone in [
            "settings-tab-reply-source",
            "settings-reply-source",
            "settings-relay",
            "settings-relay-switch",
            "settings-relay-switch-error",
            "settings-relay-status",
        ] {
            assert!(tree.find(gone).is_none(), "{gone} is gone from the tree");
            let refused = host.click(gone).unwrap_err();
            assert!(
                refused.contains(&format!("no `{gone}` on screen"))
                    && refused.contains("settings-computer-{id}-relay"),
                "{gone}: {refused}"
            );
            assert!(host.take_command().is_none(), "{gone} sent nothing");
        }
        assert!(tree.ids_are_unique());
    }

    /// Each computer is a card on the tree as the window draws it, while Settings is open on
    /// Computer: `settings-computer-{id}`, this computer's marked `this-computer`, holding its Relay
    /// your plan switch (checked as drawn, dead while a switch is with the server), where its relay
    /// stands, and what became of a switch that did not go as asked, under the switch. The switch
    /// asks for the other way from where it is drawn, from any computer's card; the card and its
    /// lines are no controls. This computer's card alone holds why its relay is not relaying. Off the
    /// page, none of it is on the tree or takes a click.
    #[test]
    fn each_computer_is_a_card_on_the_tree_and_its_switch_clicks_as_the_windows_does() {
        use crate::components::computers::RelayState;
        let mut host = host();
        host.computers = vec![
            ComputerCard {
                state: RelayState::Relaying,
                ..a_card("mac_1", true)
            },
            ComputerCard {
                online: false,
                state: RelayState::OnButAsleep,
                ..a_card("mac_2", false)
            },
            ComputerCard {
                relay_on: false,
                ..a_card("mac_3", false)
            },
        ];
        assert!(
            host.snapshot().find(&ids::computer_card("mac_1")).is_none(),
            "Settings is shut"
        );
        let shut = host.click(&ids::computer_relay("mac_1")).unwrap_err();
        assert!(shut.contains("settings-tab-computer"), "{shut}");
        host.account_open = true;
        let tree = host.snapshot();
        assert!(
            tree.find(&ids::computer_card("mac_1")).is_none(),
            "not on Computer"
        );
        host.computer_tab = true;
        let tree = host.snapshot();
        assert!(tree.ids_are_unique());

        // The cards, this computer first, and what each says.
        for (id, label, states) in [
            (
                "mac_1",
                "NativeChat on mac_1",
                vec!["online", "this-computer"],
            ),
            ("mac_2", "NativeChat on mac_2", vec!["offline"]),
            ("mac_3", "NativeChat on mac_3", vec!["online"]),
        ] {
            let card = tree.find(&ids::computer_card(id)).expect("a card");
            assert_eq!((card.role.as_str(), card.name.as_str()), ("group", label));
            assert_eq!(card.states, states, "{id}");
            assert_eq!(
                card.children
                    .iter()
                    .take(2)
                    .map(|node| node.id.clone())
                    .collect::<Vec<_>>(),
                [ids::computer_relay(id), ids::computer_status(id)],
                "{id}: the switch, then where its relay stands"
            );
        }
        for (id, checked, name, value) in [
            ("mac_1", true, "Relaying", "relaying"),
            (
                "mac_2",
                true,
                "On, but asleep: it can't answer right now",
                "asleep",
            ),
            ("mac_3", false, "Not relaying", "not-relaying"),
        ] {
            let switch = tree.find(&ids::computer_relay(id)).unwrap();
            assert_eq!(
                (
                    switch.role.as_str(),
                    switch.name.as_str(),
                    switch.checked,
                    switch.enabled
                ),
                ("switch", "Relay your plan", Some(checked), true),
                "{id}"
            );
            let status = tree.find(&ids::computer_status(id)).unwrap();
            assert_eq!(
                (status.name.as_str(), status.value.as_deref()),
                (name, Some(value)),
                "{id}"
            );
            assert!(
                tree.find(&ids::computer_error(id)).is_none(),
                "{id}: nothing said"
            );
        }
        // Only this computer's card holds its opencodex's fields.
        assert!(
            tree.find(ids::RELAY_ADDR).is_none(),
            "the setting is not read, and nothing is drawn but the line saying so"
        );
        assert_eq!(
            tree.find(ids::REPLY_SOURCE_UNAVAILABLE)
                .map(|node| node.name.as_str()),
            Some("Asking the server…")
        );
        let mine = tree.find(&ids::computer_card("mac_1")).unwrap();
        assert!(mine.find(ids::REPLY_SOURCE_UNAVAILABLE).is_some());
        for other in ["mac_2", "mac_3"] {
            let card = tree.find(&ids::computer_card(other)).unwrap();
            assert!(
                card.find(ids::REPLY_SOURCE_UNAVAILABLE).is_none(),
                "{other}"
            );
        }

        // Each switch asks for the other way, from any computer's card.
        for (id, on) in [("mac_1", false), ("mac_2", false), ("mac_3", true)] {
            host.click(&ids::computer_relay(id)).unwrap();
            assert!(
                matches!(host.take_command(), Some(Command::SetComputerRelay { machine_id, on: asked })
                    if machine_id == id && asked == on),
                "{id}"
            );
        }
        // The card and its lines are no controls, and the lines are named for what they are.
        for id in [ids::computer_card("mac_1"), ids::computer_status("mac_1")] {
            let line = host.click(&id).unwrap_err();
            assert!(line.contains("not a control"), "{id}: {line}");
        }

        // What a refused switch says, under the switch, in the server's words; and while a switch is
        // with the server the card draws it where it was asked and takes no click.
        host.computers[1].note = Some("no computer of yours has that id".into());
        host.computers[2].switching = true;
        let tree = host.snapshot();
        let error = tree.find(&ids::computer_error("mac_2")).unwrap();
        assert_eq!(error.name, "no computer of yours has that id");
        assert_eq!(
            tree.find(&ids::computer_card("mac_2"))
                .unwrap()
                .children
                .iter()
                .take(3)
                .map(|node| node.id.clone())
                .collect::<Vec<_>>(),
            [
                ids::computer_relay("mac_2"),
                ids::computer_status("mac_2"),
                ids::computer_error("mac_2")
            ],
            "under the switch"
        );
        assert!(host.click(&ids::computer_error("mac_2")).is_err(), "a line");
        let switch = tree.find(&ids::computer_relay("mac_3")).unwrap();
        assert!(!switch.enabled, "with the server");
        let dead = host.click(&ids::computer_relay("mac_3")).unwrap_err();
        assert!(dead.contains("with the server"), "{dead}");
        assert!(host.take_command().is_none());
        assert!(tree.ids_are_unique());

        // Off the page, nothing of it is a target.
        host.computer_tab = false;
        assert!(host.snapshot().find(&ids::computer_card("mac_1")).is_none());
        let off = host.click(&ids::computer_relay("mac_1")).unwrap_err();
        assert!(off.contains("settings-tab-computer"), "{off}");
    }

    /// This computer's relay fields are on its card, from a server that knows the relay: opencodex's
    /// address and the key field (never its text: states `set` while the Keychain holds a key,
    /// `typed` while one waits for Save) with Remove key, and Save; and why this computer's relay is
    /// not relaying, under its status. Each is the card's own, refused off the page and from a
    /// server without the relay; the typed key is never on the tree.
    #[test]
    fn this_computers_relay_fields_are_on_its_card_and_click_as_the_card_does() {
        use crate::components::computers::{
            RELAY_ADDR_LABEL, RELAY_KEY_LABEL, RELAY_NOT_ON_SERVER, RelayDetail, TAKE_BACK,
        };
        use crate::opengrok::InferenceKind;
        use crate::state::{REPLY_SOURCE_NOT_ON_SERVER, ReplySourceNote, ReplySourceRead};
        let mut host = host();
        host.computers = vec![a_card("mac_1", true), a_card("mac_2", false)];
        assert!(
            host.click(ids::RELAY_KEY_REMOVE).is_err(),
            "Settings is shut"
        );
        host.account_open = true;
        let off = host.click(ids::RELAY_KEY_REMOVE).unwrap_err();
        assert!(off.contains("settings-tab-computer"), "{off}");
        host.computer_tab = true;

        // Before the server has answered: the line the card draws meanwhile, and nothing to press.
        let tree = host.snapshot();
        let asking = tree.find(ids::REPLY_SOURCE_UNAVAILABLE).unwrap();
        assert_eq!(asking.name, "Asking the server…");
        assert!(asking.states.contains(&"asking".to_string()));
        assert!(host.click(ids::REPLY_SOURCE_SAVE).is_err());
        assert!(host.click(ids::RELAY_KEY_REMOVE).is_err());

        // A server without reply sources says so, and there is still nothing to press.
        host.reply_source.settings.kept = Some(ReplySourceRead::NotOnServer);
        let tree = host.snapshot();
        let said = tree.find(ids::REPLY_SOURCE_UNAVAILABLE).unwrap();
        assert_eq!(said.name, REPLY_SOURCE_NOT_ON_SERVER);
        assert!(!said.states.contains(&"asking".to_string()));
        let refused = host.click(ids::REPLY_SOURCE_SAVE).unwrap_err();
        assert!(refused.contains(REPLY_SOURCE_NOT_ON_SERVER), "{refused}");

        // A server from before the relay: the line saying so in the fields' place, no Save.
        host.reply_source.settings.kept = Some(ReplySourceRead::Read(kept_source(
            InferenceKind::Gateway,
            true,
        )));
        let tree = host.snapshot();
        let mine = tree.find(&ids::computer_card("mac_1")).unwrap();
        assert_eq!(
            mine.find(ids::RELAY_UNAVAILABLE).unwrap().name,
            RELAY_NOT_ON_SERVER
        );
        assert!(
            tree.find(ids::RELAY_ADDR).is_none() && tree.find(ids::REPLY_SOURCE_SAVE).is_none()
        );
        let refused = host.click(ids::REPLY_SOURCE_SAVE).unwrap_err();
        assert!(refused.contains(RELAY_NOT_ON_SERVER), "{refused}");
        assert!(
            host.set_value(ids::RELAY_ADDR, "http://127.0.0.1:9090")
                .is_err()
        );

        // A server with the relay, a key kept and one typed.
        host.reply_source.settings.kept = relay_read("loopback");
        host.reply_source.relay = RelaySnap {
            address: "http://127.0.0.1:8080".into(),
            has_key: true,
            key_typed: true,
        };
        host.reply_source.unsaved = true;
        let tree = host.snapshot();
        let mine = tree.find(&ids::computer_card("mac_1")).unwrap();
        assert!(mine.states.contains(&"unsaved".to_string()));
        assert_eq!(
            mine.children
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            [
                ids::computer_relay("mac_1").as_str(),
                ids::computer_status("mac_1").as_str(),
                ids::RELAY_ADDR,
                ids::RELAY_KEY,
                ids::RELAY_KEY_REMOVE,
                ids::REPLY_SOURCE_SAVE
            ],
            "the switch and its status, then what its relay needs: no model of the relay's own, \
             and nothing of the account's way"
        );
        let other = tree.find(&ids::computer_card("mac_2")).unwrap();
        assert_eq!(
            other.children.len(),
            2,
            "another computer's has only its switch and status"
        );
        let address = tree.find(ids::RELAY_ADDR).unwrap();
        assert_eq!(
            (address.name.as_str(), address.value.as_deref()),
            (RELAY_ADDR_LABEL, Some("http://127.0.0.1:8080"))
        );
        let key = tree.find(ids::RELAY_KEY).unwrap();
        assert_eq!(key.name, RELAY_KEY_LABEL);
        assert!(key.value.is_none(), "the key is never on the tree");
        assert!(
            key.states.contains(&"set".to_string()) && key.states.contains(&"typed".to_string())
        );
        let save = tree.find(ids::REPLY_SOURCE_SAVE).unwrap();
        assert_eq!((save.name.as_str(), save.enabled), ("Save", false));
        assert!(tree.ids_are_unique());

        // Each control is the card's own.
        host.click(ids::RELAY_KEY_REMOVE).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleRemoveRelayKey)
        ));
        host.set_value(ids::RELAY_ADDR, "http://127.0.0.1:9090")
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetRelayAddress(address)) if address == "http://127.0.0.1:9090"
        ));
        host.type_into(ids::RELAY_ADDR, "1").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetRelayAddress(address)) if address == "http://127.0.0.1:80801"
        ));
        host.set_value(ids::RELAY_KEY, "opencodex-test-key")
            .unwrap();
        let typed = host.take_command().expect("the key goes to the page");
        assert!(!format!("{typed:?}").contains("opencodex-test-key"));
        assert!(matches!(typed, Command::SetRelayKey(key) if key.0 == "opencodex-test-key"));
        assert!(host.type_into(ids::RELAY_KEY, "more").is_err());
        assert!(
            host.dispatch(&Op::key(ids::RELAY_ADDR, "backspace"))
                .is_err()
        );
        let nothing = host.click(ids::REPLY_SOURCE_SAVE).unwrap_err();
        assert!(
            nothing.contains("differs from what this computer keeps"),
            "{nothing}"
        );

        // A change waiting for Save, and what it waits for, beside it and when clicked.
        host.reply_source.hint = Some(crate::state::RELAY_ADDRESS_NOT_HERE);
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::REPLY_SOURCE_HINT).unwrap().name,
            crate::state::RELAY_ADDRESS_NOT_HERE
        );
        let waiting = host.click(ids::REPLY_SOURCE_SAVE).unwrap_err();
        assert!(
            waiting.contains(crate::state::RELAY_ADDRESS_NOT_HERE),
            "{waiting}"
        );
        host.reply_source.hint = None;
        host.reply_source.can_save = true;
        host.click(ids::REPLY_SOURCE_SAVE).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SaveReplySource)
        ));

        // A read that failed is trouble; what the Keychain said is drawn muted.
        host.reply_source.settings.note = Some(ReplySourceNote::ReadFailed("gone".into()));
        let tree = host.snapshot();
        let error = tree.find(ids::REPLY_SOURCE_ERROR).unwrap();
        assert_eq!(error.name, "gone");
        assert!(error.states.contains(&"trouble".to_string()));
        host.reply_source.settings.note = Some(ReplySourceNote::KeyNotKept("refused".into()));
        let error = host
            .snapshot()
            .find(ids::REPLY_SOURCE_ERROR)
            .cloned()
            .unwrap();
        assert!(error.states.is_empty());
        let line = host.click(ids::REPLY_SOURCE_ERROR).unwrap_err();
        assert!(line.contains("a line on the card"), "{line}");

        // Why this computer's relay is not relaying, in the relay's own words, under its status,
        // and on no other card.
        host.computers[0].detail = Some(RelayDetail {
            words: TAKE_BACK.into(),
            trouble: false,
        });
        host.computers[1].detail = None;
        let tree = host.snapshot();
        assert_eq!(tree.find(ids::RELAY_DETAIL).unwrap().name, TAKE_BACK);
        host.computers[0].detail = Some(RelayDetail {
            words: "Can't reach the server. Trying again…".into(),
            trouble: true,
        });
        let tree = host.snapshot();
        assert!(
            tree.find(ids::RELAY_DETAIL)
                .unwrap()
                .states
                .contains(&"trouble".to_string())
        );
        let line = host.click(ids::RELAY_DETAIL).unwrap_err();
        assert!(line.contains("a line on the card"), "{line}");

        // The snapshot never holds the key the page does.
        let mut state = AppState::new();
        state.reply_source.kept = relay_read("loopback");
        state.reply_source.relay_key_draft = crate::opengrok::RelayKey::new("opencodex-test-key");
        let host = NativeChatHost::from_app(&state);
        assert!(host.reply_source.relay.key_typed && host.reply_source.unsaved);
        assert_eq!(host.reply_source.settings.relay_key_draft, None);
    }

    /// With no computer of the person's own on the card list yet there is no card to hold the relay
    /// fields, and none of their ids answers: the fields are this computer's, and need its card.
    #[test]
    fn the_relay_fields_need_this_computers_card() {
        use crate::state::ReplySourceRead;
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.reply_source.settings.kept = relay_read("loopback");
        host.computers = vec![a_card("mac_2", false)];
        let tree = host.snapshot();
        assert!(tree.find(ids::RELAY_ADDR).is_none());
        for id in [
            ids::RELAY_KEY_REMOVE,
            ids::REPLY_SOURCE_SAVE,
            ids::RELAY_ADDR,
        ] {
            let refused = host.click(id).unwrap_err();
            assert!(
                refused.contains("this computer is not among"),
                "{id}: {refused}"
            );
        }
        let refused = host
            .set_value(ids::RELAY_ADDR, "http://127.0.0.1:9090")
            .unwrap_err();
        assert!(refused.contains("this computer is not among"), "{refused}");
        let _ = ReplySourceRead::Loading;
    }

    /// The card has nothing of the plan on the server's own machine (its proxy URL, its key and
    /// Remove key, its health line, the line on where it is set up from, the note on Claude and
    /// Gemini) and no model of the relay's own (its menu, No model, a model a computer lists, the
    /// line under it): none is on the tree, whatever the setting, and each is refused as not on the
    /// page, with nothing sent and nothing typed. Nor does it have a radio.
    #[test]
    fn the_relay_card_has_no_loopback_section_and_no_relay_model_picker() {
        use crate::state::ReplySourceRead;
        let gone = [
            "settings-reply-source-url",
            "settings-reply-source-key",
            "settings-reply-source-health",
            "settings-reply-source-remove-key",
            "settings-reply-source-elsewhere",
            "settings-reply-source-providers",
            "settings-relay-model",
            "settings-relay-no-model",
            "settings-relay-model-grok-4",
            "settings-relay-models-note",
            "settings-reply-source-kind",
            "settings-reply-source-kind-local_proxy",
            "settings-reply-source-via-mac",
            "settings-reply-source-model",
            "settings-reply-source-no-model",
        ];
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.computers = vec![a_card("mac_1", true)];
        let with_everything = crate::opengrok::InferenceSource {
            kind: crate::opengrok::InferenceKind::LocalProxy,
            base_url: Some("http://127.0.0.1:8080".into()),
            local_model: Some("gpt-5-codex".into()),
            has_api_key: true,
            via: Some("mac".into()),
            relay: Some(crate::opengrok::RelayRead {
                connected: true,
                machine_id: Some("mac_1".into()),
                machine_label: None,
                local_model: Some("grok-4".into()),
            }),
            ..kept_source(crate::opengrok::InferenceKind::LocalProxy, true)
        };
        for kept in [
            Some(ReplySourceRead::Read(with_everything.clone())),
            Some(ReplySourceRead::Read(crate::opengrok::InferenceSource {
                relay: None,
                ..with_everything
            })),
        ] {
            host.reply_source.settings.kept = kept;
            let tree = host.snapshot();
            assert!(tree.find(&ids::computer_card("mac_1")).is_some());
            for id in gone {
                assert!(tree.find(id).is_none(), "{id}");
                let refused = host.click(id).unwrap_err();
                assert!(
                    refused.contains(&format!("no `{id}` on")),
                    "{id}: {refused}"
                );
                for op in [
                    Op::key(id, "a"),
                    Op::type_text(id, "a"),
                    Op::SetValue {
                        target: id.into(),
                        value: "a".into(),
                    },
                ] {
                    let refused = host.dispatch(&op).unwrap_err();
                    assert!(refused.contains("is not editable"), "{id}: {refused}");
                }
            }
            assert!(host.take_command().is_none() && host.take_compose().is_none());
        }
        let none = host.dispatch(&Op::key(ids::SIDEBAR, "a")).unwrap_err();
        assert!(
            !none.contains("settings-reply-source-url") && none.contains(ids::RELAY_ADDR),
            "{none}"
        );
    }

    /// A key pressed at the relay's address or key field is answered as the card answers typing
    /// there, never with the list of what takes text as if the field were not one: first why the
    /// card draws it read-only (off the page, before the setting is read, a server without the
    /// relay, and for the key a Remove key waiting for Save), and otherwise what writes it.
    /// `set_value` and `type` are refused for the same reasons. None of them leaves a command or a
    /// keystroke behind, so no draft changes.
    #[test]
    fn a_key_at_a_relay_field_is_refused_as_the_card_refuses_typing() {
        use crate::components::computers::RELAY_NOT_ON_SERVER;
        use crate::opengrok::InferenceKind;
        use crate::state::ReplySourceRead;
        let fields = [ids::RELAY_ADDR, ids::RELAY_KEY];
        let typing = |target: &str| {
            [
                Op::key(target, "a"),
                Op::type_text(target, "a"),
                Op::SetValue {
                    target: target.into(),
                    value: "a".into(),
                },
            ]
        };
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.computers = vec![a_card("mac_1", true)];
        for field in fields {
            for op in typing(field) {
                let refused = host.dispatch(&op).unwrap_err();
                assert!(
                    refused.contains("on screen: Asking the server"),
                    "{refused}"
                );
            }
        }
        host.reply_source.settings.kept = Some(ReplySourceRead::Read(kept_source(
            InferenceKind::Gateway,
            true,
        )));
        for field in fields {
            for op in typing(field) {
                let refused = host.dispatch(&op).unwrap_err();
                assert!(refused.contains(RELAY_NOT_ON_SERVER), "{refused}");
            }
        }

        host.reply_source.settings.kept = relay_read("loopback");
        let address = host
            .dispatch(&Op::key(ids::RELAY_ADDR, "backspace"))
            .unwrap_err();
        assert!(
            address.contains("use set_value, or type") && !address.contains("is not editable"),
            "{address}"
        );
        let key = host.dispatch(&Op::key(ids::RELAY_KEY, "a")).unwrap_err();
        assert!(
            key.contains("takes the whole key: use set_value") && !key.contains("is not editable"),
            "{key}"
        );
        assert!(host.take_compose().is_none() && host.take_command().is_none());

        // Remove key waits for Save: the key field is drawn read-only and takes nothing, and the
        // address, which Remove key has nothing to do with, still does.
        host.reply_source.relay.has_key = true;
        host.reply_source.settings.relay_remove_key = true;
        assert!(!host.snapshot().find(ids::RELAY_KEY).unwrap().enabled);
        for op in typing(ids::RELAY_KEY) {
            let refused = host.dispatch(&op).unwrap_err();
            assert!(refused.contains(ids::RELAY_KEY_REMOVE), "{refused}");
        }
        assert!(host.take_command().is_none());
        host.set_value(ids::RELAY_ADDR, "http://127.0.0.1:9090")
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetRelayAddress(address)) if address == "http://127.0.0.1:9090"
        ));

        host.computer_tab = false;
        for field in fields {
            for op in typing(field) {
                let refused = host.dispatch(&op).unwrap_err();
                assert!(refused.contains("settings-tab-computer"), "{refused}");
            }
        }
        assert!(host.take_compose().is_none() && host.take_command().is_none());
    }

    /// Settings → General answers by its tab's id from anywhere in Settings, and not while
    /// Settings is shut; on it, Default models is a section of the page, and a line, not a
    /// control.
    #[test]
    fn the_general_tab_is_on_the_tree_and_holds_default_models() {
        use crate::components::default_models::TITLE;
        let mut host = host();
        let tree = host.snapshot();
        assert_eq!(tree.find(ids::SETTINGS_GENERAL).unwrap().name, "General");
        let shut = host.click(ids::SETTINGS_GENERAL).unwrap_err();
        assert!(shut.contains(ids::FOOTER_ACCOUNT), "{shut}");
        host.account_open = true;
        host.click(ids::SETTINGS_GENERAL).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetAppSettingsTab(AppSettingsTab::General))
        ));
        assert!(
            host.snapshot().find(ids::DEFAULT_MODELS).is_none(),
            "Settings is not open on General"
        );
        host.general_tab = true;
        let tree = host.snapshot();
        let models = tree.find(ids::DEFAULT_MODELS).unwrap();
        assert_eq!(models.name, TITLE);
        assert_eq!(TITLE, "Default models");
        assert_eq!(
            models
                .children
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            [ids::NEW_BOTS, ids::PLAN_FALLBACK],
            "its two pickers"
        );
        assert!(tree.ids_are_unique());
        let line = host.click(ids::DEFAULT_MODELS).unwrap_err();
        assert!(line.contains("not a control"), "{line}");
        assert!(host.take_command().is_none());
    }

    /// Settings → General opens with Default models, which holds Default for new Bots: while
    /// the server keeps no such default it says it is coming and its card is dead, the picker's
    /// card with no model, and refused with why; off the page it is refused for that first, and
    /// Settings → Relay does not hold it. From the app, as the state holds it.
    #[test]
    fn the_general_pages_default_for_new_bots_is_coming() {
        use crate::components::default_models::{NEW_BOTS_COMING_SOON, TITLE};
        use crate::opengrok::InferenceKind;
        use crate::state::ReplySourceRead;
        let mut state = AppState::new();
        // Signed in with a Bot, or the tree is the login page's or the empty roster's.
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
        ];
        state.is_app_settings_open = true;
        state.app_settings_tab = AppSettingsTab::General;
        state.reply_source.kept = Some(ReplySourceRead::Read(kept_source(
            InferenceKind::LocalProxy,
            true,
        )));
        let mut host = NativeChatHost::from_app(&state);
        let tree = host.snapshot();
        let models = tree.find(ids::DEFAULT_MODELS).expect("Default models");
        assert_eq!(
            (models.name.as_str(), models.children[0].id.as_str()),
            (TITLE, ids::NEW_BOTS)
        );
        let new_bots = tree.find(ids::NEW_BOTS).unwrap();
        assert_eq!(
            (new_bots.name.as_str(), new_bots.states.as_slice()),
            ("Default for new Bots", &["unavailable".to_string()][..])
        );
        assert_eq!(
            tree.find(ids::NEW_BOTS_UNAVAILABLE).unwrap().name,
            NEW_BOTS_COMING_SOON
        );
        let card = tree.find(ids::NEW_BOTS_CARD).unwrap();
        assert_eq!(
            (card.role.as_str(), card.name.as_str(), card.enabled),
            ("button", "No model", false)
        );
        let dead = host.click(ids::NEW_BOTS_CARD).unwrap_err();
        assert!(dead.contains(NEW_BOTS_COMING_SOON), "{dead}");
        let line = host.click(ids::NEW_BOTS_UNAVAILABLE).unwrap_err();
        assert!(line.contains("not a control"), "{line}");
        assert!(host.take_command().is_none());

        state.app_settings_tab = AppSettingsTab::Computer;
        let mut host = NativeChatHost::from_app(&state);
        let tree = host.snapshot();
        assert!(tree.find(ids::DEFAULT_MODELS).is_none() && tree.find(ids::NEW_BOTS).is_none());
        let off = host.click(ids::NEW_BOTS_CARD).unwrap_err();
        assert!(off.contains(ids::SETTINGS_GENERAL), "{off}");
    }

    /// What `GET /models` lists in the picker tests: two gateway routes, one that lists five
    /// levels of effort and one that lists none, and the plan's GPT-6 Luna with its fast twin
    /// (five levels) and Sol (six, up to ultra), each with a level of its own
    /// (`opengrok::model_fixtures::levelled_catalogue`).
    fn a_levelled_catalogue() -> crate::opengrok::ModelCatalogue {
        crate::opengrok::model_fixtures::levelled_catalogue()
    }

    /// Default for new Bots' picker, from the default as given, over a list of two plan models
    /// and two gateway routes ([`a_levelled_catalogue`]).
    fn a_new_bots_pick(
        default: Option<crate::opengrok::NewBotDefault>,
    ) -> crate::opengrok::ModelPick {
        let account = kept_source(crate::opengrok::InferenceKind::Gateway, true);
        crate::opengrok::new_bots_pick(
            default.as_ref(),
            Some(&account),
            &a_levelled_catalogue(),
            |_| vec!["gpt-6-luna".to_string(), "gpt-6-luna--fast".to_string()],
        )
    }

    /// Where the server keeps a default for new Bots, Default for new Bots is the Bot's picker on
    /// Settings → General, every part of it under `settings-new-bots-` for `agent-model-`, and a
    /// driver works it by the same roads: the card, ⚡, the slider, the list with None over its
    /// models, the search box, a model, and the popover's dismiss. Each change is sent as the
    /// default's, and every control that sends one is dead while one is with the server. What a
    /// refused change came to is in the popover while it is open, and under the card while it is
    /// shut. Off the page it is all refused.
    #[test]
    fn the_default_for_new_bots_is_the_bots_picker_and_a_driver_works_it() {
        use crate::opengrok::{InferenceKind, NEW_BOTS_PICK_FIRST, NewBotDefault};
        use crate::state::DefaultForNewBots;
        let mut host = host();
        host.account_open = true;
        host.general_tab = true;
        host.reply_source.settings.kept = relay_read("loopback");
        host.reply_source.new_bots = DefaultForNewBots::Kept(None);
        host.new_bots_pick = Some(a_new_bots_pick(None));
        let tree = host.snapshot();
        let section = tree.find(ids::NEW_BOTS).unwrap();
        assert!(section.states.is_empty(), "{:?}", section.states);
        assert!(tree.find(ids::NEW_BOTS_UNAVAILABLE).is_none());
        let card = tree.find(ids::NEW_BOTS_CARD).unwrap();
        assert_eq!(
            (card.name.as_str(), card.value.as_deref(), card.enabled),
            ("None", Some(""), true)
        );
        assert!(!tree.find("settings-new-bots-pop").unwrap().visible);
        host.click(ids::NEW_BOTS_CARD).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::Toggle))
        ));

        // None set: ⚡ and the slider are dead and say why.
        host.new_bots_picker.open = true;
        let tree = host.snapshot();
        assert!(tree.find("settings-new-bots-pop").unwrap().visible);
        let fast = tree.find("settings-new-bots-fast").unwrap();
        assert_eq!(
            (fast.enabled, fast.value.as_deref()),
            (false, Some(NEW_BOTS_PICK_FIRST))
        );
        assert!(
            tree.find("settings-new-bots-effort").is_none(),
            "no model, no levels, no slider"
        );
        let dead = host.click("settings-new-bots-fast").unwrap_err();
        assert!(dead.contains(NEW_BOTS_PICK_FIRST), "{dead}");
        let no_effort = host
            .set_value("settings-new-bots-effort", "high")
            .unwrap_err();
        assert!(no_effort.contains("no model is picked"), "{no_effort}");
        host.click("settings-new-bots-open-list").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::ToggleList))
        ));

        // The list: None over the models, ticked while none is set, and the Bot's groups.
        host.new_bots_picker.list_open = true;
        let tree = host.snapshot();
        let list = tree.find("settings-new-bots-list").unwrap();
        let rows: Vec<(&str, &[String])> = list
            .children
            .iter()
            .map(|row| (row.id.as_str(), row.states.as_slice()))
            .collect();
        assert_eq!(
            rows,
            [
                ("settings-new-bots-none", &["selected".to_string()][..]),
                ("settings-new-bots-group-local_proxy", &[][..]),
                (
                    "settings-new-bots-row-local_proxy-gpt-6-luna",
                    &["fast".to_string()][..]
                ),
                ("settings-new-bots-group-gateway", &[][..]),
                ("settings-new-bots-row-gateway-oag/cheap", &[][..]),
                ("settings-new-bots-row-gateway-xai/grok-4.7", &[][..]),
            ]
        );
        assert_eq!(
            ids::new_bots_row(InferenceKind::LocalProxy, "gpt-6-luna"),
            "settings-new-bots-row-local_proxy-gpt-6-luna"
        );
        host.click(&ids::new_bots_row(InferenceKind::LocalProxy, "gpt-6-luna"))
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::Pick { source: InferenceKind::LocalProxy, base_id }))
                if base_id == "gpt-6-luna"
        ));
        host.click("settings-new-bots-none").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ClearNewBotsDefault)
        ));
        host.set_value("settings-new-bots-search", "grok").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::SetSearch(query))) if query == "grok"
        ));
        let rows: Vec<String> = host
            .snapshot()
            .find("settings-new-bots-list")
            .unwrap()
            .children
            .iter()
            .map(|row| row.id.clone())
            .collect();
        assert_eq!(
            rows,
            [
                "settings-new-bots-group-gateway",
                "settings-new-bots-row-gateway-xai/grok-4.7"
            ],
            "the search leaves None out too"
        );
        let left_out = host.click("settings-new-bots-none").unwrap_err();
        assert!(left_out.contains("leaves it out"), "{left_out}");
        host.dispatch(&Op::key("settings-new-bots-search", "backspace"))
            .unwrap();
        assert_eq!(host.new_bots_picker.search, "gro");

        // A default set: the card names it, its row is ticked, and ⚡ and the slider work.
        host.new_bots_pick = Some(a_new_bots_pick(Some(NewBotDefault {
            source: InferenceKind::LocalProxy,
            model: "gpt-6-luna".into(),
            effort: "high".into(),
        })));
        host.new_bots_picker.list_open = false;
        host.new_bots_picker.search.clear();
        let tree = host.snapshot();
        let card = tree.find(ids::NEW_BOTS_CARD).unwrap();
        assert_eq!(
            (card.name.as_str(), card.states.as_slice()),
            (
                "GPT-6 Luna · High",
                &["local_proxy".to_string(), "expanded".to_string()][..]
            )
        );
        host.click("settings-new-bots-fast").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::SetFast(true)))
        ));
        host.set_value("settings-new-bots-effort", "max").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::SetEffort(word))) if word == "max"
        ));
        host.click("settings-new-bots-reset").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::Reset))
        ));

        // A change with the server: every control that sends one is dead, and the card says so.
        host.new_bots_busy = true;
        let tree = host.snapshot();
        assert!(
            tree.find(ids::NEW_BOTS_CARD)
                .unwrap()
                .states
                .contains(&"saving".to_string())
        );
        assert!(!tree.find("settings-new-bots-fast").unwrap().enabled);
        for control in ["settings-new-bots-fast", "settings-new-bots-reset"] {
            let busy = host.click(control).unwrap_err();
            assert!(busy.contains("a change is with the server"), "{busy}");
        }
        assert!(host.set_value("settings-new-bots-effort", "low").is_err());
        host.new_bots_busy = false;

        // What a refusal said: in the popover while open, under the card while shut.
        host.new_bots_note = Some("newBotDefault.effort must be one of inherit".into());
        let tree = host.snapshot();
        let pop = tree.find("settings-new-bots-pop").unwrap();
        assert!(
            pop.children
                .iter()
                .any(|node| node.id == "settings-new-bots-error")
        );
        host.click(ids::NEW_BOTS_DISMISS).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::NewBots(PickerCommand::SetOpen(false)))
        ));
        host.new_bots_picker.open = false;
        let tree = host.snapshot();
        let section = tree.find(ids::NEW_BOTS).unwrap();
        assert!(
            section
                .children
                .iter()
                .any(|node| node.id == "settings-new-bots-error"),
            "under the card"
        );
        assert!(tree.ids_are_unique());
        let line = host.click("settings-new-bots-error").unwrap_err();
        assert!(line.contains("a line"), "{line}");
        assert!(host.take_command().is_none());

        // Off the page, nothing of it answers, on Computer as anywhere else; the Bot's picker is a
        // picker of its own.
        host.general_tab = false;
        host.computer_tab = true;
        let off = host.click(ids::NEW_BOTS_CARD).unwrap_err();
        assert!(off.contains(ids::SETTINGS_GENERAL), "{off}");
        let off = host.set_value("settings-new-bots-search", "x").unwrap_err();
        assert!(off.contains(ids::SETTINGS_GENERAL), "{off}");
        assert!(!is_picker_part(ids::NEW_BOTS_CARD));
    }

    /// The Relay-off fallback's picker, over a gateway route and a plan model the server lists,
    /// with the fallback as given.
    fn a_plan_fallback_pick(
        fallback: Option<crate::opengrok::PlanFallback>,
    ) -> crate::opengrok::ModelPick {
        crate::opengrok::plan_fallback_pick(fallback.as_ref(), &a_levelled_catalogue())
    }

    /// Where the server keeps a Relay-off fallback, it is the Bot's picker on Settings → General
    /// over the Gateway group alone, every part of it under `settings-plan-fallback-`, and a
    /// driver works it as it works Default for new Bots: the card, the list with None over its
    /// rows, a model, None, the search box and the popover's dismiss. Each change is sent as the
    /// fallback's. Off the page it is all refused.
    #[test]
    fn the_relay_off_fallback_is_a_gateway_picker_and_a_driver_works_it() {
        use crate::opengrok::{InferenceKind, PLAN_FALLBACK_PICK_FIRST};
        use crate::state::RelayOffFallback;
        let mut host = host();
        host.account_open = true;
        host.general_tab = true;
        host.reply_source.plan_fallback = RelayOffFallback::Kept(None);
        host.plan_fallback_pick = Some(a_plan_fallback_pick(None));
        let tree = host.snapshot();
        let section = tree.find(ids::PLAN_FALLBACK).unwrap();
        assert_eq!(
            (section.name.as_str(), section.states.as_slice()),
            ("When Relay is off, Subscription Bots use", &[][..])
        );
        assert!(tree.find(ids::PLAN_FALLBACK_UNAVAILABLE).is_none());
        let card = tree.find(ids::PLAN_FALLBACK_CARD).unwrap();
        assert_eq!((card.name.as_str(), card.enabled), ("None", true));
        host.click(ids::PLAN_FALLBACK_CARD).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PlanFallback(PickerCommand::Toggle))
        ));

        host.plan_fallback_picker.open = true;
        let tree = host.snapshot();
        assert!(tree.find("settings-plan-fallback-pop").unwrap().visible);
        let fast = tree.find("settings-plan-fallback-fast").unwrap();
        assert_eq!(
            (fast.enabled, fast.value.as_deref()),
            (false, Some(PLAN_FALLBACK_PICK_FIRST))
        );
        host.plan_fallback_picker.list_open = true;
        let tree = host.snapshot();
        let rows: Vec<(&str, Option<&str>)> = tree
            .find("settings-plan-fallback-list")
            .unwrap()
            .children
            .iter()
            .map(|row| (row.id.as_str(), row.value.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                ("settings-plan-fallback-none", Some("no fallback")),
                ("settings-plan-fallback-group-gateway", None),
                (
                    "settings-plan-fallback-row-gateway-oag/cheap",
                    Some("gateway")
                ),
                (
                    "settings-plan-fallback-row-gateway-xai/grok-4.7",
                    Some("gateway")
                ),
            ],
            "the Gateway's rows alone"
        );
        let plan = host
            .click("settings-plan-fallback-row-local_proxy-gpt-6-luna")
            .unwrap_err();
        assert!(
            plan.contains("no `settings-plan-fallback-row-local_proxy"),
            "{plan}"
        );
        host.click(&ids::plan_fallback_row("xai/grok-4.7")).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PlanFallback(PickerCommand::Pick { source: InferenceKind::Gateway, base_id }))
                if base_id == "xai/grok-4.7"
        ));
        host.click("settings-plan-fallback-none").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ClearPlanFallback)
        ));
        host.set_value("settings-plan-fallback-search", "grok")
            .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PlanFallback(PickerCommand::SetSearch(query))) if query == "grok"
        ));
        host.click(ids::PLAN_FALLBACK_DISMISS).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PlanFallback(PickerCommand::SetOpen(false)))
        ));

        // What a refusal said is under the card while the popover is shut.
        host.plan_fallback_picker = PickerView::default();
        host.plan_fallback_note = Some(
            "planFallback.effort must be one of inherit, none, low, medium, high, xhigh, max, ultra"
                .into(),
        );
        let tree = host.snapshot();
        assert!(
            tree.find(ids::PLAN_FALLBACK)
                .unwrap()
                .children
                .iter()
                .any(|node| node.id == "settings-plan-fallback-error"),
            "under the card"
        );
        assert!(tree.ids_are_unique());

        // Off the page, nothing of it answers.
        host.general_tab = false;
        let off = host.click(ids::PLAN_FALLBACK_CARD).unwrap_err();
        assert!(off.contains(ids::SETTINGS_GENERAL), "{off}");
        let off = host
            .set_value("settings-plan-fallback-search", "x")
            .unwrap_err();
        assert!(off.contains(ids::SETTINGS_GENERAL), "{off}");
        assert!(host.take_command().is_none());
    }

    /// From the app: the Relay-off fallback is live only where the read of the setting carries
    /// `planFallback`, `null` or not; without the key the section says it is coming and its card
    /// is dead and refused with why.
    #[test]
    fn the_relay_off_fallback_is_live_from_the_app_only_where_the_server_keeps_one() {
        use crate::components::default_models::PLAN_FALLBACK_COMING_SOON;
        use crate::state::ReplySourceRead;
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
        ];
        state.is_app_settings_open = true;
        state.app_settings_tab = AppSettingsTab::General;
        let mut kept = kept_source(crate::opengrok::InferenceKind::LocalProxy, true);
        state.reply_source.kept = Some(ReplySourceRead::Read(kept.clone()));
        let mut host = NativeChatHost::from_app(&state);
        let tree = host.snapshot();
        let section = tree.find(ids::PLAN_FALLBACK).unwrap();
        assert!(section.states.contains(&"unavailable".to_string()));
        assert_eq!(
            tree.find(ids::PLAN_FALLBACK_UNAVAILABLE).unwrap().name,
            PLAN_FALLBACK_COMING_SOON
        );
        let card = tree.find(ids::PLAN_FALLBACK_CARD).unwrap();
        assert_eq!((card.name.as_str(), card.enabled), ("No model", false));
        let dead = host.click(ids::PLAN_FALLBACK_CARD).unwrap_err();
        assert!(dead.contains(PLAN_FALLBACK_COMING_SOON), "{dead}");
        assert!(host.take_command().is_none());

        kept.plan_fallback = Some(Some(crate::opengrok::PlanFallback {
            model: "xai/grok-4.7".into(),
            effort: "low".into(),
        }));
        state.reply_source.kept = Some(ReplySourceRead::Read(kept));
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert!(tree.find(ids::PLAN_FALLBACK_UNAVAILABLE).is_none());
        let card = tree.find(ids::PLAN_FALLBACK_CARD).unwrap();
        assert_eq!(
            (card.name.as_str(), card.enabled, card.states.as_slice()),
            ("Grok 4.7", true, &["gateway".to_string()][..])
        );
    }

    /// From the app: a read whose setting carries `newBotDefault` puts the live picker on the
    /// tree, and one without the key the dead card that says it is coming.
    #[test]
    fn the_default_for_new_bots_is_live_from_the_app_only_where_the_server_keeps_one() {
        use crate::components::default_models::NEW_BOTS_COMING_SOON;
        use crate::state::ReplySourceRead;
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
        ];
        state.is_app_settings_open = true;
        state.app_settings_tab = AppSettingsTab::General;
        let mut kept = kept_source(crate::opengrok::InferenceKind::Gateway, true);
        state.reply_source.kept = Some(ReplySourceRead::Read(kept.clone()));
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert_eq!(
            tree.find(ids::NEW_BOTS_UNAVAILABLE).unwrap().name,
            NEW_BOTS_COMING_SOON
        );
        assert!(!tree.find(ids::NEW_BOTS_CARD).unwrap().enabled);

        kept.new_bot_default = Some(Some(crate::opengrok::NewBotDefault {
            source: crate::opengrok::InferenceKind::Gateway,
            model: "xai/grok-4.7".into(),
            effort: "low".into(),
        }));
        state.reply_source.kept = Some(ReplySourceRead::Read(kept));
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert!(tree.find(ids::NEW_BOTS_UNAVAILABLE).is_none());
        let card = tree.find(ids::NEW_BOTS_CARD).unwrap();
        assert_eq!((card.name.as_str(), card.enabled), ("Grok 4.7", true));
    }

    /// The cards have no line or button of their own for the account's way to the plan: a switch
    /// going on points the account at the relay in the same press. With the relay on, whatever way
    /// the account keeps, and a refusal of what the switch sent said, neither the line, its button
    /// nor a line under them is on the tree, and each is refused as not on screen, with nothing
    /// sent.
    #[test]
    fn the_cards_have_no_line_or_button_for_the_accounts_way() {
        let gone = [
            "settings-relay-via",
            "settings-relay-via-use",
            "settings-relay-via-error",
        ];
        let mut host = host();
        host.account_open = true;
        host.computer_tab = true;
        host.computers = vec![ComputerCard {
            note: Some("via must be \"loopback\" or \"mac\"".into()),
            ..a_card("mac_1", true)
        }];
        host.reply_source.relay = RelaySnap {
            address: "http://127.0.0.1:8080".into(),
            ..RelaySnap::default()
        };
        for way in ["loopback", "mac", "helper"] {
            host.reply_source.settings.kept = relay_read(way);
            let tree = host.snapshot();
            assert!(tree.find(&ids::computer_relay("mac_1")).is_some(), "{way}");
            for id in gone {
                assert!(tree.find(id).is_none(), "{way}: {id}");
                let refused = host.click(id).unwrap_err();
                assert!(
                    refused.contains(&format!("no `{id}` on")),
                    "{way}: {id}: {refused}"
                );
            }
            assert!(host.take_command().is_none(), "{way}");
        }
    }

    /// A reply the server's paid keys answered because the relay is off wears its badge on the
    /// tree as the feed draws it: `paid key · relay off`, valued by its door, with state
    /// `relay-off`.
    #[test]
    fn a_reply_answered_because_the_relay_is_off_says_so_on_the_tree() {
        use crate::opengrok::ReplySource;
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.active_conversation_id = Some("cw_1".into());
        let source = ReplySource::from_event(&serde_json::json!({
            "type": "CUSTOM", "name": "opengrok.inferenceSource",
            "value": {"kind": "gateway", "model": "oag/cheap", "fallbackFor": "relay_disabled"}
        }));
        state.conversations.push(crate::state::Conversation {
            id: "cw_1".into(),
            title: "Ada".into(),
            created_at: String::new(),
            updated_at: String::new(),
            messages: vec![crate::state::Message {
                id: "m_1".into(),
                sender: "AI".into(),
                content: "Here it is.".into(),
                sent_at: std::time::SystemTime::UNIX_EPOCH,
                finished_at: None,
                run_timing: None,
                reply_source: source,
                is_me: false,
                reply_preview: None,
                reply_to_id: None,
                reply_is_me: false,
                parts: Vec::new(),
                run_id: None,
                hidden: false,
            }],
            unread_count: 0,
            origin: None,
        });
        let tree = NativeChatHost::from_app(&state).snapshot();
        let badge = tree.find(&ids::reply_badge("m_1")).unwrap();
        assert_eq!(
            (
                badge.name.as_str(),
                badge.value.as_deref(),
                badge.states.as_slice()
            ),
            (
                "paid key · relay off",
                Some("gateway"),
                &["relay-off".to_string()][..]
            )
        );
    }

    /// Each reply's badge is on the tree as the feed draws it: under a reply with words, and not
    /// under one that said nothing but its status line.
    #[test]
    fn each_replys_badge_is_on_the_tree() {
        use crate::opengrok::{InferenceKind, InferenceSource, ReplySource};
        use crate::state::ReplySourceRead;
        // From the app: a reply with words wears its badge, and one that said only its status
        // line does not.
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" })).unwrap(),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.active_conversation_id = Some("cw_1".into());
        state.reply_source.kept = Some(ReplySourceRead::Read(InferenceSource {
            local_model: Some("gpt-5-codex".into()),
            ..kept_source(InferenceKind::LocalProxy, true)
        }));
        let reply = |id: &str, content: &str, kind: InferenceKind| crate::state::Message {
            id: id.into(),
            sender: "AI".into(),
            content: content.into(),
            sent_at: std::time::SystemTime::UNIX_EPOCH,
            finished_at: None,
            run_timing: None,
            reply_source: Some(ReplySource {
                kind,
                model: Some("gpt-5-codex".into()),
                via: None,
                fallback_for: None,
            }),
            is_me: false,
            reply_preview: None,
            reply_to_id: None,
            reply_is_me: false,
            parts: Vec::new(),
            run_id: None,
            hidden: false,
        };
        state.conversations.push(crate::state::Conversation {
            id: "cw_1".into(),
            title: "Ada".into(),
            created_at: String::new(),
            updated_at: String::new(),
            messages: vec![
                reply("m_1", "Here it is.", InferenceKind::LocalProxy),
                reply(
                    "m_2",
                    &format!("{}the gateway timed out", crate::state::RUN_ERROR_PREFIX),
                    InferenceKind::Gateway,
                ),
            ],
            unread_count: 0,
            origin: None,
        });
        let tree = NativeChatHost::from_app(&state).snapshot();
        let badge = tree.find(&ids::reply_badge("m_1")).unwrap();
        assert_eq!(
            (badge.name.as_str(), badge.value.as_deref()),
            ("your plan", Some("local_proxy"))
        );
        assert!(badge.states.is_empty(), "the model is not a state");
        assert_eq!(
            tree.find(&ids::reply_badge_model("m_1"))
                .map(|node| node.name.as_str()),
            Some("gpt-5-codex"),
            "the model the badge shows on hover, under it"
        );
        assert!(
            tree.find(&ids::reply_badge("m_2")).is_none(),
            "a status line has no bubble to wear a badge"
        );
    }

    /// The open Bot's picker as `AppState::model_pick` builds it: the Bot's row with `source`
    /// as given (missing where `None`), the account on the server's keys, the gateway's two
    /// routes and a plan that lists GPT-6 Luna with its fast twin ([`a_levelled_catalogue`]).
    fn a_pick(
        source: Option<serde_json::Value>,
        model: &str,
        effort: &str,
    ) -> crate::opengrok::ModelPick {
        a_pick_among(&["gpt-6-luna", "gpt-6-luna--fast"], source, model, effort)
    }

    /// [`a_pick`] over the plan models given.
    fn a_pick_among(
        plan: &[&str],
        source: Option<serde_json::Value>,
        model: &str,
        effort: &str,
    ) -> crate::opengrok::ModelPick {
        let mut row = serde_json::json!({
            "id": "cw_1", "name": "Ada", "model": model, "effort": effort
        });
        if let Some(source) = source {
            row["source"] = source;
        }
        let bot = serde_json::from_value(row).unwrap();
        let account = crate::opengrok::InferenceSource {
            local_model: Some("gpt-5-codex".into()),
            ..kept_source(crate::opengrok::InferenceKind::Gateway, true)
        };
        crate::opengrok::bot_pick(&bot, Some(&account), &a_levelled_catalogue(), |_| {
            plan.iter().map(|id| id.to_string()).collect()
        })
    }

    /// The Model card is on the tree while a Bot is open, named as the picker reads in a line
    /// and valued by the model the next turn runs on, and a driver works its popover by the roads
    /// a person's clicks take: ⚡, the slider by its words, the list and a model in it. Each
    /// control is refused while it is not on screen, and the card while the Bot's settings are
    /// shut.
    #[test]
    fn the_model_card_opens_its_popover_and_a_driver_works_it() {
        let mut host = host();
        assert!(
            host.snapshot().find(ids::AGENT_MODEL_CARD).is_none(),
            "no Bot open"
        );
        assert!(host.click(ids::AGENT_MODEL_CARD).is_err());
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("local_proxy")),
            "gpt-6-luna",
            "inherit",
        ));
        let tree = host.snapshot();
        let card = tree.find(ids::AGENT_MODEL_CARD).unwrap();
        assert_eq!(
            (card.name.as_str(), card.value.as_deref()),
            ("GPT-6 Luna · Medium", Some("gpt-6-luna")),
            "a Bot that chose no effort is on its model's own level, by name"
        );
        assert_eq!(card.states, ["local_proxy"]);
        assert!(!tree.find("agent-model-pop").unwrap().visible);
        assert!(
            tree.find("agent-model-fast").is_none(),
            "shut, and no controls"
        );
        assert!(
            host.click("agent-model-fast").is_err(),
            "the popover is shut"
        );
        let closed = host.click(ids::AGENT_MODEL_CARD).unwrap_err();
        assert!(closed.contains("settings, which are closed"), "{closed}");
        host.agent_settings_open = true;
        host.click(ids::AGENT_MODEL_CARD).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleModelPicker)
        ));

        host.model_picker.open = true;
        let tree = host.snapshot();
        assert!(tree.find("agent-model-pop").unwrap().visible);
        assert!(
            tree.find(ids::AGENT_MODEL_CARD)
                .unwrap()
                .states
                .contains(&"expanded".to_string())
        );
        let fast = tree.find("agent-model-fast").unwrap();
        assert_eq!((fast.enabled, fast.checked), (true, Some(false)));
        let effort = tree.find("agent-model-effort").unwrap();
        assert_eq!(
            (
                effort.role.as_str(),
                effort.name.as_str(),
                effort.value.as_deref()
            ),
            ("slider", "Medium", Some("medium")),
            "lit on the model's own level with nothing chosen, and valued by that level's word"
        );
        assert_eq!(
            tree.find("agent-model-open-list").unwrap().name,
            "GPT-6 Luna"
        );
        assert!(
            !tree.find("agent-model-reset").unwrap().enabled,
            "nothing chosen, and ⚡ off: nothing to put back"
        );
        assert!(!tree.find("agent-model-list").unwrap().visible);
        host.click("agent-model-fast").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelFast(true))
        ));
        host.dispatch(&Op::SetValue {
            target: "agent-model-effort".into(),
            value: "max".into(),
        })
        .unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelEffort(word)) if word == "max"
        ));
        for refused in ["inherit", "none", "Max", "ultra", "extreme"] {
            assert!(
                host.dispatch(&Op::SetValue {
                    target: "agent-model-effort".into(),
                    value: refused.into(),
                })
                .is_err(),
                "{refused}"
            );
        }
        let said = host
            .dispatch(&Op::SetValue {
                target: "agent-model-effort".into(),
                value: "ultra".into(),
            })
            .unwrap_err();
        assert!(
            said.contains(
                "`ultra` is not one of GPT-6 Luna's levels: low, medium, high, xhigh, max"
            ),
            "{said}"
        );
        assert!(
            host.click("agent-model-effort").is_err(),
            "a slider is set, not clicked"
        );
        assert!(
            host.click("agent-model-reset").is_err(),
            "nothing to put back"
        );
        assert!(
            host.click("agent-model-row-local_proxy-gpt-6-luna")
                .is_err(),
            "the list is not showing"
        );
        host.click("agent-model-open-list").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleModelList)
        ));

        host.model_picker.list_open = true;
        let tree = host.snapshot();
        let search = tree.find("agent-model-search").unwrap();
        assert_eq!(
            (
                search.role.as_str(),
                search.name.as_str(),
                search.value.as_deref()
            ),
            ("textbox", "Search models", Some("")),
            "at the top of the list, nothing typed"
        );
        let list = tree.find("agent-model-list").unwrap();
        assert!(list.visible);
        assert_eq!(list.value.as_deref(), Some("3"));
        let rows: Vec<(&str, &str, &[String])> = list
            .children
            .iter()
            .map(|row| (row.id.as_str(), row.name.as_str(), row.states.as_slice()))
            .collect();
        // Under the rows, the line saying where this Bot's routines run: its own door is the
        // person's plan, and its routines run there.
        let routines = host
            .model_pick
            .as_ref()
            .and_then(|pick| pick.routines.clone())
            .expect("a Bot on its own plan");
        assert_eq!(
            rows,
            [
                ("agent-model-group-local_proxy", "Subscription", &[][..]),
                (
                    "agent-model-row-local_proxy-gpt-6-luna",
                    "GPT-6 Luna",
                    &["selected".to_string(), "fast".to_string()][..]
                ),
                ("agent-model-group-gateway", "Gateway", &[][..]),
                ("agent-model-row-gateway-oag/cheap", "Cheap (auto)", &[][..]),
                ("agent-model-row-gateway-xai/grok-4.7", "Grok 4.7", &[][..]),
                ("agent-model-routines", routines.as_str(), &[][..]),
            ]
        );
        assert!(
            tree.find("agent-model-fast").is_none(),
            "the list has the controls' place"
        );
        assert!(host.click("agent-model-fast").is_err());
        host.click("agent-model-row-gateway-xai/grok-4.7").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PickModel { source: crate::opengrok::InferenceKind::Gateway, base_id })
                if base_id == "xai/grok-4.7"
        ));
        assert!(
            host.click("agent-model-row-gateway-xai/grok-4.6@sub")
                .is_err(),
            "not a model the list offers"
        );
        assert!(host.click("agent-model-row-local_proxy-oag/cheap").is_err());

        // On a fast twin: ⚡ is a state of the card, and checked in the popover.
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("local_proxy")),
            "gpt-6-luna--fast",
            "xhigh",
        ));
        host.model_picker.list_open = false;
        let tree = host.snapshot();
        let card = tree.find(ids::AGENT_MODEL_CARD).unwrap();
        assert_eq!(card.name, "GPT-6 Luna · Xhigh ⚡");
        assert_eq!(card.states, ["local_proxy", "fast", "expanded"]);
        assert_eq!(tree.find("agent-model-fast").unwrap().checked, Some(true));
        host.click("agent-model-fast").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelFast(false))
        ));
    }

    /// The open Bot's picker on Cheap (auto), a gateway route that lists five levels of effort and
    /// names none as its own, with the effort given.
    fn an_ownless_pick(effort: &str) -> crate::opengrok::ModelPick {
        let bot = serde_json::from_value(serde_json::json!({
            "id": "cw_1", "name": "Ada", "model": "oag/cheap", "effort": effort,
            "source": "gateway"
        }))
        .unwrap();
        crate::opengrok::bot_pick(
            &bot,
            Some(&kept_source(crate::opengrok::InferenceKind::Gateway, true)),
            &crate::opengrok::model_fixtures::levelled_catalogue_without_own("oag/cheap"),
            |_| Vec::new(),
        )
    }

    /// The slider has exactly the levels the model lists for its stops, low to high, and a driver
    /// sets one by its word: GPT-6 Luna five, up to max, and Sol six, up to ultra. A level the
    /// model does not list is refused in words that name the ones it does, and a model that lists
    /// none has no slider on the tree at all, and nothing in its place: its node is absent, the
    /// popover's controls are ⚡, the name and ↺ alone, and the card says no effort.
    #[test]
    fn the_slider_has_the_levels_the_model_lists_and_is_absent_where_it_lists_none() {
        let set = |host: &mut NativeChatHost, value: &str| {
            host.dispatch(&Op::SetValue {
                target: "agent-model-effort".into(),
                value: value.into(),
            })
        };
        let mut host = host();
        host.agent_settings_open = true;
        host.model_picker.open = true;

        // GPT-6 Luna: five levels, the one the Bot chose lit.
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("local_proxy")),
            "gpt-6-luna",
            "xhigh",
        ));
        let tree = host.snapshot();
        let effort = tree.find("agent-model-effort").unwrap();
        assert_eq!(
            (
                effort.name.as_str(),
                effort.value.as_deref(),
                effort.enabled
            ),
            ("Xhigh", Some("xhigh"), true)
        );
        for word in ["low", "medium", "high", "xhigh", "max"] {
            set(&mut host, word).unwrap();
            assert!(
                matches!(host.take_command(), Some(Command::SetModelEffort(sent)) if sent == word),
                "{word}"
            );
        }
        let said = set(&mut host, "ultra").unwrap_err();
        assert!(
            said.contains(
                "`ultra` is not one of GPT-6 Luna's levels: low, medium, high, xhigh, max"
            ),
            "{said}"
        );
        let click = host.click("agent-model-effort").unwrap_err();
        assert!(
            click.contains("set_value it to one of low, medium, high, xhigh, max"),
            "{click}"
        );

        // Sol: six levels, ultra among them, and none chosen sits on its own level, High.
        host.model_pick = Some(a_pick_among(
            &["gpt-6-luna", "gpt-5.6-sol"],
            Some(serde_json::json!("local_proxy")),
            "gpt-5.6-sol",
            "inherit",
        ));
        let tree = host.snapshot();
        let effort = tree.find("agent-model-effort").unwrap();
        assert_eq!(
            (effort.name.as_str(), effort.value.as_deref()),
            ("High", Some("high")),
            "the model's own level, with nothing chosen: its word, and not `inherit`"
        );
        assert_eq!(
            tree.find(ids::AGENT_MODEL_CARD).unwrap().name,
            "GPT-5.6 Sol · High"
        );
        set(&mut host, "ultra").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelEffort(sent)) if sent == "ultra"
        ));
        let click = host.click("agent-model-effort").unwrap_err();
        assert!(
            click.contains("set_value it to one of low, medium, high, xhigh, max, ultra"),
            "{click}"
        );

        // Cheap (auto) lists levels and names none as its own: with none chosen nothing is lit,
        // so the slider is named for what it is and has no word to say, and it takes a level.
        host.model_pick = Some(an_ownless_pick("inherit"));
        let tree = host.snapshot();
        let effort = tree.find("agent-model-effort").unwrap();
        assert_eq!(
            (
                effort.name.as_str(),
                effort.value.as_deref(),
                effort.enabled
            ),
            ("Effort", None, true)
        );
        assert_eq!(
            tree.find(ids::AGENT_MODEL_CARD).unwrap().name,
            "Cheap (auto)"
        );
        set(&mut host, "low").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelEffort(sent)) if sent == "low"
        ));

        // Grok 4.7's source publishes no levels: no slider, nothing in its place.
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("gateway")),
            "xai/grok-4.7",
            "inherit",
        ));
        let tree = host.snapshot();
        assert!(tree.find("agent-model-effort").is_none());
        let popover: Vec<&str> = tree
            .find("agent-model-pop")
            .unwrap()
            .children
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(
            popover,
            [
                "agent-model-fast",
                "agent-model-open-list",
                "agent-model-reset",
                "agent-model-list"
            ]
        );
        assert_eq!(tree.find(ids::AGENT_MODEL_CARD).unwrap().name, "Grok 4.7");
        for refused in [
            set(&mut host, "high").unwrap_err(),
            host.click("agent-model-effort").unwrap_err(),
        ] {
            assert!(
                refused.contains("Grok 4.7 lists no levels of effort, so there is no slider"),
                "{refused}"
            );
        }
        assert!(host.take_command().is_none());
    }

    /// Nothing the picker says, in the card or in the popover, says "Default" for an effort, in
    /// whatever state the Bot's picker, Default for new Bots' and the Relay-off fallback's are:
    /// where no effort was chosen the model's own level is named, where the model lists no
    /// levels nothing is, and the card that is dead names no model and no effort.
    #[test]
    fn no_picker_says_default_for_an_effort_in_any_state() {
        use crate::opengrok::{InferenceKind, NewBotDefault, PlanFallback};
        use crate::state::{DefaultForNewBots, RelayOffFallback};
        fn said_under(node: &UiNode, into: &mut Vec<String>) {
            into.push(node.name.clone());
            into.extend(node.value.clone());
            into.extend(node.states.clone());
            for child in &node.children {
                said_under(child, into);
            }
        }
        let mut said = Vec::new();
        let mut look = |host: &NativeChatHost, card: &str| {
            let tree = host.snapshot();
            let node = tree
                .find(card)
                .unwrap_or_else(|| panic!("{card} is on the tree"));
            said_under(node, &mut said);
        };
        let states = [
            ("gpt-6-luna", "inherit"),
            ("gpt-6-luna", "medium"),
            ("gpt-6-luna", "ultra"),
            ("gpt-6-luna", "none"),
            ("oag/cheap", "inherit"),
            ("xai/grok-4.7", "inherit"),
            ("xai/grok-4.7", "high"),
            ("oag/unlisted", "inherit"),
        ];
        // Each of the three, shut, open on its controls, and open on its list.
        for (open, list_open) in [(false, false), (true, false), (true, true)] {
            for (model, effort) in states {
                let source = if model.starts_with("gpt") {
                    "local_proxy"
                } else {
                    "gateway"
                };
                let mut host = host();
                host.agent_settings_open = true;
                host.model_pick = Some(a_pick(Some(serde_json::json!(source)), model, effort));
                host.model_picker.open = open;
                host.model_picker.list_open = list_open;
                look(&host, ids::AGENT_MODEL_CARD);

                let mut host = self::host();
                host.account_open = true;
                host.general_tab = true;
                host.reply_source.settings.kept = relay_read("loopback");
                host.reply_source.new_bots = DefaultForNewBots::Kept(None);
                host.new_bots_pick = Some(a_new_bots_pick(Some(NewBotDefault {
                    source: if source == "gateway" {
                        InferenceKind::Gateway
                    } else {
                        InferenceKind::LocalProxy
                    },
                    model: model.into(),
                    effort: effort.into(),
                })));
                host.new_bots_picker.open = open;
                host.new_bots_picker.list_open = list_open;
                look(&host, ids::NEW_BOTS_CARD);

                if source == "gateway" {
                    host.reply_source.plan_fallback = RelayOffFallback::Kept(None);
                    host.plan_fallback_pick = Some(a_plan_fallback_pick(Some(PlanFallback {
                        model: model.into(),
                        effort: effort.into(),
                    })));
                    host.plan_fallback_picker.open = open;
                    host.plan_fallback_picker.list_open = list_open;
                    look(&host, ids::PLAN_FALLBACK_CARD);
                }
            }
        }
        // None set in the two that can have none, and the dead cards of a server that keeps none.
        for open in [false, true] {
            let mut host = host();
            host.account_open = true;
            host.general_tab = true;
            host.reply_source.settings.kept = relay_read("loopback");
            host.reply_source.new_bots = DefaultForNewBots::Kept(None);
            host.new_bots_pick = Some(a_new_bots_pick(None));
            host.new_bots_picker.open = open;
            look(&host, ids::NEW_BOTS_CARD);
            host.reply_source.plan_fallback = RelayOffFallback::Kept(None);
            host.plan_fallback_pick = Some(a_plan_fallback_pick(None));
            host.plan_fallback_picker.open = open;
            look(&host, ids::PLAN_FALLBACK_CARD);
        }
        let mut host = host();
        host.account_open = true;
        host.general_tab = true;
        look(&host, ids::NEW_BOTS_CARD);
        look(&host, ids::PLAN_FALLBACK_CARD);

        assert!(said.len() > 100, "a good many words were looked at");
        let defaults: Vec<&String> = said
            .iter()
            .filter(|word| word.contains("Default"))
            .collect();
        assert!(defaults.is_empty(), "{defaults:?}");
    }

    /// The same slider is in all three pickers, under their own ids: the model's levels for its
    /// stops, the model's own level lit where none was chosen, one of its levels set by its word
    /// and any other refused in the same words, and no slider at all where there is no model, in
    /// Default for new Bots' and the Relay-off fallback's.
    #[test]
    fn the_three_pickers_have_the_same_slider() {
        use crate::opengrok::{InferenceKind, NewBotDefault, PlanFallback};
        use crate::state::{DefaultForNewBots, RelayOffFallback};
        // Each picker on Cheap (auto), which lists five levels, with the effort given, open on
        // its controls.
        let on = |which: PickerFor, effort: &str| {
            let mut host = host();
            host.account_open = true;
            host.general_tab = true;
            host.agent_settings_open = true;
            host.reply_source.settings.kept = relay_read("loopback");
            match which {
                PickerFor::Bot => {
                    host.model_pick = Some(a_pick(
                        Some(serde_json::json!("gateway")),
                        "oag/cheap",
                        effort,
                    ));
                    host.model_picker.open = true;
                }
                PickerFor::NewBots => {
                    host.reply_source.new_bots = DefaultForNewBots::Kept(None);
                    host.new_bots_pick = Some(a_new_bots_pick(Some(NewBotDefault {
                        source: InferenceKind::Gateway,
                        model: "oag/cheap".into(),
                        effort: effort.into(),
                    })));
                    host.new_bots_picker.open = true;
                }
                PickerFor::PlanFallback => {
                    host.reply_source.plan_fallback = RelayOffFallback::Kept(None);
                    host.plan_fallback_pick = Some(a_plan_fallback_pick(Some(PlanFallback {
                        model: "oag/cheap".into(),
                        effort: effort.into(),
                    })));
                    host.plan_fallback_picker.open = true;
                }
            }
            host
        };
        let ids_of = |which: PickerFor| model_picker::ids(which).effort;
        // The word of the level each picker's command carries, if it is a slider's.
        let sent_word = |command: Option<Command>, which: PickerFor| match (which, command) {
            (PickerFor::Bot, Some(Command::SetModelEffort(word))) => Some(word),
            (PickerFor::NewBots, Some(Command::NewBots(PickerCommand::SetEffort(word)))) => {
                Some(word)
            }
            (
                PickerFor::PlanFallback,
                Some(Command::PlanFallback(PickerCommand::SetEffort(word))),
            ) => Some(word),
            _ => None,
        };
        for which in [PickerFor::Bot, PickerFor::NewBots, PickerFor::PlanFallback] {
            let effort = ids_of(which);
            // None chosen: the model's own level, Medium, is lit, and its word is the
            // node's, which a driver can write back; the word kept is still `inherit`.
            let mut host = on(which, "inherit");
            let tree = host.snapshot();
            let node = tree.find(effort).unwrap();
            assert_eq!(
                (
                    node.role.as_str(),
                    node.name.as_str(),
                    node.value.as_deref(),
                    node.enabled
                ),
                ("slider", "Medium", Some("medium"), true),
                "{effort}"
            );
            let read = node.value.clone().expect("a level is lit");
            host.set_value(effort, &read).unwrap();
            assert_eq!(
                sent_word(host.take_command(), which),
                Some(read),
                "{effort}: what was read is taken back"
            );
            // A level the Bot chose: lit by name, and valued by its word.
            let host_high = on(which, "xhigh");
            let tree = host_high.snapshot();
            let node = tree.find(effort).unwrap();
            assert_eq!(
                (node.name.as_str(), node.value.as_deref()),
                ("Xhigh", Some("xhigh")),
                "{effort}"
            );
            // A level of the model's is sent, and one that is not is refused in the same words.
            host.set_value(effort, "max").unwrap();
            assert_eq!(
                sent_word(host.take_command(), which).as_deref(),
                Some("max"),
                "{effort}"
            );
            let refused = host.set_value(effort, "ultra").unwrap_err();
            assert!(
                refused.ends_with(
                    "`ultra` is not one of Cheap (auto)'s levels: low, medium, high, xhigh, max"
                ),
                "{refused}"
            );
        }

        // Cheap (auto) lists five levels and names none as the one it runs at: with none
        // chosen nothing is lit in any of the three, so the slider is named for what it is and
        // has no word to say, and it still takes a level.
        let ownless = crate::opengrok::model_fixtures::levelled_catalogue_without_own("oag/cheap");
        for which in [PickerFor::Bot, PickerFor::NewBots, PickerFor::PlanFallback] {
            let effort = ids_of(which);
            let mut host = on(which, "inherit");
            let account = kept_source(InferenceKind::Gateway, true);
            match which {
                PickerFor::Bot => host.model_pick = Some(an_ownless_pick("inherit")),
                PickerFor::NewBots => {
                    host.new_bots_pick = Some(crate::opengrok::new_bots_pick(
                        Some(&NewBotDefault {
                            source: InferenceKind::Gateway,
                            model: "oag/cheap".into(),
                            effort: "inherit".into(),
                        }),
                        Some(&account),
                        &ownless,
                        |_| Vec::new(),
                    ))
                }
                PickerFor::PlanFallback => {
                    host.plan_fallback_pick = Some(crate::opengrok::plan_fallback_pick(
                        Some(&PlanFallback {
                            model: "oag/cheap".into(),
                            effort: "inherit".into(),
                        }),
                        &ownless,
                    ))
                }
            }
            let tree = host.snapshot();
            let node = tree.find(effort).unwrap();
            assert_eq!(
                (node.name.as_str(), node.value.as_deref(), node.enabled),
                ("Effort", None, true),
                "{effort}"
            );
            host.set_value(effort, "low").unwrap();
            assert_eq!(
                sent_word(host.take_command(), which).as_deref(),
                Some("low"),
                "{effort}"
            );
        }

        // No model, no levels, no slider: Default for new Bots' and the fallback's, before one
        // is picked.
        for which in [PickerFor::NewBots, PickerFor::PlanFallback] {
            let effort = ids_of(which);
            let mut host = on(which, "inherit");
            match which {
                PickerFor::NewBots => host.new_bots_pick = Some(a_new_bots_pick(None)),
                _ => host.plan_fallback_pick = Some(a_plan_fallback_pick(None)),
            }
            let tree = host.snapshot();
            assert!(tree.find(effort).is_none(), "{effort}");
            let refused = host.set_value(effort, "high").unwrap_err();
            assert!(refused.contains("no model is picked"), "{refused}");
        }
    }

    /// After a pick that put the effort back on the model's own level the popover says so in a
    /// line under the slider, `agent-model-effort-note`, which is a line and not a control; it is
    /// there while the controls show, and only for the model it is about.
    #[test]
    fn the_line_a_pick_leaves_is_under_the_slider() {
        use crate::state::EffortNote;
        let said = "GPT-6 Luna has no Ultra, so it's on Medium, its own level.";
        let mut host = host();
        host.agent_settings_open = true;
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("local_proxy")),
            "gpt-6-luna",
            "inherit",
        ));
        host.model_picker.open = true;
        assert!(host.snapshot().find("agent-model-effort-note").is_none());
        host.model_picker.effort_note = Some(EffortNote {
            base_id: "gpt-6-luna".into(),
            line: said.into(),
        });
        let tree = host.snapshot();
        let pop = tree.find("agent-model-pop").unwrap();
        let ids: Vec<&str> = pop.children.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "agent-model-fast",
                "agent-model-open-list",
                "agent-model-reset",
                "agent-model-effort",
                "agent-model-effort-note",
                "agent-model-list"
            ],
            "in the window's order"
        );
        let line = tree.find("agent-model-effort-note").unwrap();
        assert_eq!((line.role.as_str(), line.name.as_str()), ("status", said));
        let click = host.click("agent-model-effort-note").unwrap_err();
        assert!(click.contains("a line, not a control"), "{click}");
        assert!(is_picker_part("agent-model-effort-note"));

        // The list takes the controls' place, and the line with them.
        host.model_picker.list_open = true;
        assert!(host.snapshot().find("agent-model-effort-note").is_none());
        host.model_picker.list_open = false;
        // A line about a model that is not the one on the card says nothing.
        host.model_picker.effort_note = Some(EffortNote {
            base_id: "gpt-5.6-sol".into(),
            line: said.into(),
        });
        assert!(host.snapshot().find("agent-model-effort-note").is_none());
    }

    /// The composer has no chip: a Bot's model is picked on its card in its settings, and
    /// nowhere else. The chat page holds no part of the picker, and the composer's old ids are
    /// no part of it, whatever is open.
    #[test]
    fn the_composer_has_no_model_chip() {
        fn ids_under<'a>(node: &'a UiNode, into: &mut Vec<&'a str>) {
            into.push(node.id.as_str());
            for child in &node.children {
                ids_under(child, into);
            }
        }
        let mut host = host();
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("gateway")),
            "oag/cheap",
            "low",
        ));
        host.agent_settings_open = true;
        host.model_picker.open = true;
        host.model_picker.list_open = true;
        let tree = host.snapshot();
        let mut on_the_page = Vec::new();
        ids_under(tree.find(ids::PAGE).unwrap(), &mut on_the_page);
        assert!(
            on_the_page
                .iter()
                .all(|id| !id.starts_with("model-") && !id.starts_with("agent-model-")),
            "{on_the_page:?}"
        );
        assert!(tree.find(ids::AGENT_MODEL_CARD).is_some(), "the card is");
        for old in [
            "model-chip",
            "model-pop",
            "model-fast",
            "model-effort",
            "model-open-list",
            "model-reset",
            "model-list",
            "model-row-gateway-oag/cheap",
            "agent-model-chip",
        ] {
            assert!(tree.find(old).is_none(), "{old}");
            assert!(!is_picker_part(old), "{old}");
            let refused = host.click(old).unwrap_err();
            assert!(refused.contains("unknown click target"), "{old}: {refused}");
        }
        assert!(host.take_command().is_none());
    }

    /// The list's search box filters both groups at once, whatever the case, by a model's name
    /// and by its id, and the tree draws what it leaves, headings and all; a search that leaves
    /// nothing says "No model matches". A driver writes the box with `set_value`, adds to it with
    /// `type` and takes a letter off with Backspace, and is refused while the box is not on
    /// screen. A model the search leaves out is not on screen to pick.
    #[test]
    fn the_model_search_filters_both_groups_and_a_driver_writes_it() {
        let mut host = host();
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("local_proxy")),
            "gpt-6-luna",
            "medium",
        ));
        host.agent_settings_open = true;
        let shut = host.set_value("agent-model-search", "grok").unwrap_err();
        assert!(shut.contains("which is shut"), "{shut}");
        host.model_picker.open = true;
        let controls = host.set_value("agent-model-search", "grok").unwrap_err();
        assert!(controls.contains("agent-model-open-list"), "{controls}");
        assert!(host.take_command().is_none());
        host.model_picker.list_open = true;
        let field = host.click("agent-model-search").unwrap_err();
        assert!(field.contains("use set_value"), "{field}");

        let in_list = |host: &NativeChatHost| -> (Option<String>, Vec<(String, String)>) {
            let tree = host.snapshot();
            let list = tree.find("agent-model-list").unwrap();
            (
                list.value.clone(),
                list.children
                    .iter()
                    .filter(|node| node.id != "agent-model-routines")
                    .map(|node| (node.id.clone(), node.name.clone()))
                    .collect(),
            )
        };
        let line = |id: &str, name: &str| (id.to_string(), name.to_string());
        host.set_value("agent-model-search", "GROK").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelSearch(query)) if query == "GROK"
        ));
        assert_eq!(
            in_list(&host),
            (
                Some("1".to_string()),
                vec![
                    line("agent-model-group-gateway", "Gateway"),
                    line("agent-model-row-gateway-xai/grok-4.7", "Grok 4.7"),
                ]
            ),
            "by name, whatever the case: the Subscription group has nothing left and goes"
        );
        let left_out = host
            .click("agent-model-row-local_proxy-gpt-6-luna")
            .unwrap_err();
        assert!(left_out.contains("leaves it out"), "{left_out}");
        host.click("agent-model-row-gateway-xai/grok-4.7").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PickModel { base_id, .. }) if base_id == "xai/grok-4.7"
        ));

        host.set_value("agent-model-search", "oag/").unwrap();
        assert_eq!(
            in_list(&host).1,
            [
                line("agent-model-group-gateway", "Gateway"),
                line("agent-model-row-gateway-oag/cheap", "Cheap (auto)"),
            ],
            "by the raw id, which the name does not hold"
        );

        host.set_value("agent-model-search", "claude").unwrap();
        assert_eq!(
            in_list(&host),
            (
                Some("0".to_string()),
                vec![line("agent-model-no-match", "No model matches")]
            )
        );
        assert!(host.click("agent-model-no-match").is_err(), "a line");

        host.set_value("agent-model-search", "lun").unwrap();
        host.type_into("agent-model-search", "a").unwrap();
        assert_eq!(host.model_picker.search, "luna");
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelSearch(query)) if query == "luna"
        ));
        host.dispatch(&Op::key("agent-model-search", "backspace"))
            .unwrap();
        assert_eq!(host.model_picker.search, "lun");
        host.dispatch(&Op::key("agent-model-search", "enter"))
            .unwrap();
        assert_eq!(
            in_list(&host).1,
            [
                line("agent-model-group-local_proxy", "Subscription"),
                line("agent-model-row-local_proxy-gpt-6-luna", "GPT-6 Luna"),
            ]
        );
    }

    /// A long list is on the tree as the window draws it: five models at a time, each group's
    /// heading over its first model in view and not one of the five, and the list valued by all
    /// it holds. A model out of view is refused, as a person cannot click it without scrolling,
    /// and the search box brings it into view; a heading is no model to pick.
    #[test]
    fn the_list_shows_five_models_at_a_time_and_a_driver_finds_the_rest_by_search() {
        let mut host = host();
        let bot = serde_json::from_value(serde_json::json!({
            "id": "cw_1", "name": "Ada", "model": "oag/route-1", "effort": "medium",
            "source": "gateway"
        }))
        .unwrap();
        let catalogue = crate::opengrok::ModelCatalogue {
            models: (0..6)
                .map(|at| crate::opengrok::ModelEntry {
                    id: format!("oag/route-{at}"),
                    source: Some("gateway".into()),
                    via: None,
                    ..Default::default()
                })
                .collect(),
            note: None,
            local_proxy: None,
        };
        let account = kept_source(crate::opengrok::InferenceKind::Gateway, true);
        host.model_pick = Some(crate::opengrok::bot_pick(
            &bot,
            Some(&account),
            &catalogue,
            |_| {
                ["gpt-6-luna", "gpt-5.6-sol", "grok-4.7"]
                    .map(str::to_string)
                    .to_vec()
            },
        ));
        host.agent_settings_open = true;
        host.model_picker.open = true;
        host.model_picker.list_open = true;
        let ids_in_list = |host: &NativeChatHost| -> Vec<String> {
            host.snapshot()
                .find("agent-model-list")
                .unwrap()
                .children
                .iter()
                .map(|node| node.id.clone())
                .collect()
        };
        assert_eq!(
            host.snapshot()
                .find("agent-model-list")
                .unwrap()
                .value
                .as_deref(),
            Some("9"),
            "valued by all the list holds"
        );
        assert_eq!(
            ids_in_list(&host),
            [
                "agent-model-group-local_proxy",
                "agent-model-row-local_proxy-gpt-6-luna",
                "agent-model-row-local_proxy-gpt-5.6-sol",
                "agent-model-row-local_proxy-grok-4.7",
                "agent-model-group-gateway",
                "agent-model-row-gateway-oag/route-0",
                "agent-model-row-gateway-oag/route-1",
            ]
        );
        let out = host
            .click("agent-model-row-gateway-oag/route-5")
            .unwrap_err();
        assert!(out.contains("out of view"), "{out}");
        assert!(host.take_command().is_none());
        let heading = host.click("agent-model-group-gateway").unwrap_err();
        assert!(heading.contains("heading"), "{heading}");

        // Scrolled to the end: the Gateway group's heading over its first model in view.
        host.model_picker.list_start = 4;
        assert_eq!(
            ids_in_list(&host),
            [
                "agent-model-group-gateway",
                "agent-model-row-gateway-oag/route-1",
                "agent-model-row-gateway-oag/route-2",
                "agent-model-row-gateway-oag/route-3",
                "agent-model-row-gateway-oag/route-4",
                "agent-model-row-gateway-oag/route-5",
            ]
        );
        host.click("agent-model-row-gateway-oag/route-5").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PickModel { base_id, .. }) if base_id == "oag/route-5"
        ));

        // From the top again, the search box finds it.
        host.model_picker.list_start = 0;
        host.set_value("agent-model-search", "route-5").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::SetModelSearch(query)) if query == "route-5"
        ));
        assert_eq!(
            ids_in_list(&host),
            [
                "agent-model-group-gateway",
                "agent-model-row-gateway-oag/route-5",
            ]
        );
        host.click("agent-model-row-gateway-oag/route-5").unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::PickModel { base_id, .. }) if base_id == "oag/route-5"
        ));
    }

    /// ⚡ is dead where the list holds no fast version of the model, and says why: on the tree as
    /// its value, and as the refusal of a driver's click. ↺ puts back what there is to put back.
    #[test]
    fn model_fast_refuses_with_its_reason_where_there_is_no_twin() {
        let mut host = host();
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("gateway")),
            "xai/grok-4.7",
            "high",
        ));
        host.agent_settings_open = true;
        host.model_picker.open = true;
        let tree = host.snapshot();
        let fast = tree.find("agent-model-fast").unwrap();
        assert!(!fast.enabled);
        assert_eq!(fast.value.as_deref(), Some(crate::opengrok::FAST_NO_TWIN));
        let refused = host.click("agent-model-fast").unwrap_err();
        assert!(refused.contains(crate::opengrok::FAST_NO_TWIN), "{refused}");
        assert!(host.take_command().is_none());
        assert!(tree.find("agent-model-reset").unwrap().enabled);
        host.click("agent-model-reset").unwrap();
        assert!(matches!(host.take_command(), Some(Command::ResetModelPick)));
    }

    /// A server whose rows carry no `source` keeps no door per Bot: the list offers no plan
    /// models for the Bot, and while the account is on the plan names its plan model, which
    /// answers for every Bot there, as a line and not a row; ⚡ is dead and says why.
    #[test]
    fn a_server_without_per_bot_doors_lists_no_plan_rows_for_a_bot() {
        let mut host = host();
        let bot = serde_json::from_value(serde_json::json!({
            "id": "cw_1", "name": "Ada", "model": "oag/cheap", "effort": "medium"
        }))
        .unwrap();
        let catalogue = crate::opengrok::ModelCatalogue {
            models: vec![crate::opengrok::ModelEntry {
                id: "oag/cheap".into(),
                source: Some("gateway".into()),
                via: None,
                ..Default::default()
            }],
            note: Some("the gateway could not be reached: …".into()),
            local_proxy: None,
        };
        let on_plan = crate::opengrok::InferenceSource {
            local_model: Some("gpt-5-codex".into()),
            ..kept_source(crate::opengrok::InferenceKind::LocalProxy, true)
        };
        host.model_pick = Some(crate::opengrok::bot_pick(
            &bot,
            Some(&on_plan),
            &catalogue,
            |_| vec!["gpt-6-luna".to_string()],
        ));
        host.model_note = catalogue.note.clone();
        host.agent_settings_open = true;
        host.model_picker.open = true;
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_MODEL_CARD).unwrap().name,
            "GPT-5 Codex",
            "no row of the list answers, so no levels to name"
        );
        assert_eq!(
            tree.find("agent-model-fast").unwrap().value.as_deref(),
            Some(crate::opengrok::FAST_ACCOUNT_PLAN)
        );
        host.model_picker.list_open = true;
        let tree = host.snapshot();
        let list = tree.find("agent-model-list").unwrap();
        let ids_in_list: Vec<&str> = list.children.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(
            ids_in_list,
            [
                "agent-model-plan",
                "agent-model-group-gateway",
                "agent-model-row-gateway-oag/cheap",
                "agent-model-note"
            ]
        );
        assert_eq!(
            tree.find("agent-model-plan").map(|node| node.name.as_str()),
            Some("GPT-5 Codex")
        );
        assert!(host.click("agent-model-plan").is_err(), "a line, not a row");
    }

    /// A Bot on its own plan (opengrok-server #304, and #334 for its routines): while the list
    /// shows, `agent-model-routines` under its rows says the Bot's routines run on that plan and
    /// are skipped while it cannot answer, as the window does. It is a line, not a control. It is
    /// there whatever the Bot is pinned to, a model the gateway lists included, and there is none
    /// for a Bot that follows the account or is on the gateway.
    #[test]
    fn the_list_says_where_a_bot_on_its_own_plan_runs_its_routines() {
        let said = crate::opengrok::ROUTINES_ON_PLAN;
        let mut host = host();
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("local_proxy")),
            "gpt-6-luna--fast",
            "medium",
        ));
        host.agent_settings_open = true;
        host.model_picker.open = true;
        assert!(
            host.snapshot().find("agent-model-routines").is_none(),
            "the popover shows its controls"
        );
        host.model_picker.list_open = true;
        let tree = host.snapshot();
        let ids_in_list: Vec<&str> = tree
            .find("agent-model-list")
            .unwrap()
            .children
            .iter()
            .map(|node| node.id.as_str())
            .collect();
        assert_eq!(
            ids_in_list,
            [
                "agent-model-group-local_proxy",
                "agent-model-row-local_proxy-gpt-6-luna",
                "agent-model-group-gateway",
                "agent-model-row-gateway-oag/cheap",
                "agent-model-row-gateway-xai/grok-4.7",
                "agent-model-routines",
            ]
        );
        assert_eq!(
            tree.find("agent-model-routines")
                .map(|node| node.name.as_str()),
            Some(said)
        );
        let refused = host.click("agent-model-routines").unwrap_err();
        assert!(refused.contains("a line, not a control"), "{refused}");
        assert!(host.take_command().is_none());

        // A pin the gateway lists: on the plan all the same.
        host.model_pick = Some(a_pick(
            Some(serde_json::json!("local_proxy")),
            "xai/grok-4.7",
            "medium",
        ));
        assert_eq!(
            host.snapshot()
                .find("agent-model-routines")
                .map(|node| node.name.as_str()),
            Some(said)
        );
        for (source, pin) in [
            (serde_json::Value::Null, "gpt-6-luna"),
            (serde_json::json!("gateway"), "xai/grok-4.7"),
            (serde_json::json!("gateway"), "gpt-6-luna"),
        ] {
            host.model_pick = Some(a_pick(Some(source.clone()), pin, "medium"));
            assert!(
                host.snapshot().find("agent-model-routines").is_none(),
                "{source} on {pin}"
            );
        }
    }

    /// From the app: the card is the open Bot's picker, in the Bot's settings, the chat page holds
    /// none of it, and the list on the tree is the one the state's search and window leave.
    #[test]
    fn the_picker_on_the_tree_is_the_open_bots() {
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({ "id": "acct_1", "email": "a@b.c" }))
                .unwrap(),
        );
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({
                "id": "cw_1", "name": "Ada", "model": "oag/cheap", "effort": "low",
                "source": "gateway"
            }))
            .unwrap(),
        ];
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert!(tree.find(ids::AGENT_MODEL_CARD).is_none(), "no Bot open");
        state.active_coworker_id = Some("cw_1".into());
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert_eq!(
            tree.find(ids::AGENT_MODEL_CARD)
                .map(|node| node.name.as_str()),
            Some("Cheap (auto)"),
            "no list read yet: no levels to name"
        );
        state.model_catalogue = a_levelled_catalogue();
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert_eq!(
            tree.find(ids::AGENT_MODEL_CARD)
                .map(|node| node.name.as_str()),
            Some("Cheap (auto) · Low")
        );
        assert!(
            tree.find(ids::AGENT_SETTINGS)
                .unwrap()
                .children
                .iter()
                .any(|node| node.id == ids::AGENT_MODEL_CARD),
            "under the Bot's settings"
        );
        assert!(tree.find("model-chip").is_none());
        state.model_catalogue = crate::opengrok::ModelCatalogue {
            models: vec![crate::opengrok::ModelEntry {
                id: "oag/cheap".into(),
                source: Some("gateway".into()),
                via: None,
                ..Default::default()
            }],
            note: None,
            local_proxy: None,
        };
        state.model_picker.open = true;
        state.model_picker.list_open = true;
        state.model_picker.search = "cheap".into();
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert_eq!(
            tree.find("agent-model-search")
                .and_then(|node| node.value.as_deref()),
            Some("cheap")
        );
        assert!(
            tree.find("agent-model-row-gateway-oag/cheap").is_some(),
            "the search leaves it"
        );
        state.model_picker.search = "luna".into();
        let tree = NativeChatHost::from_app(&state).snapshot();
        assert!(tree.find("agent-model-row-gateway-oag/cheap").is_none());
        assert!(tree.find("agent-model-no-match").is_some());
    }

    /// A bot's skills as the server gives them (opengrok-server#270): the owner's `triage`
    /// attached and `draft` not, a colleague's `review` attached, and two of the owner's switched
    /// off in Settings → Skills, `old-notes` still attached and `archive` not.
    fn some_skills() -> crate::state::SkillsRead {
        crate::state::SkillsRead {
            rows: serde_json::from_value(serde_json::json!([
                {"id": "sk_triage", "name": "triage", "description": "Sort the inbox.",
                    "scope": "mine", "attached": true, "enabled": true},
                {"id": "sk_draft", "name": "draft", "description": "",
                    "scope": "mine", "attached": false, "enabled": true},
                {"id": "sk_review", "name": "review", "description": "",
                    "scope": "org", "attached": true, "enabled": true},
                {"id": "sk_old", "name": "old-notes", "description": "",
                    "scope": "mine", "attached": true, "enabled": false},
                {"id": "sk_archive", "name": "archive", "description": "",
                    "scope": "mine", "attached": false, "enabled": false}
            ]))
            .unwrap(),
            version: Some(4),
        }
    }

    /// The Skills card for `skills`, with nothing with the server, nothing said, and a Bot the
    /// roster does not say is shared.
    fn skills_card(skills: BotSkills) -> SkillsCard {
        SkillsCard {
            skills,
            pending: None,
            blocked: None,
            note: None,
            shared: false,
        }
    }

    fn skill_switched(host: &mut NativeChatHost, target: &str) -> (String, bool) {
        host.dispatch(&Op::click(target)).unwrap();
        match host.take_command() {
            Some(Command::SetBotSkill { skill_id, attached }) => (skill_id, attached),
            other => panic!("expected a skill switch from {target}, got {other:?}"),
        }
    }

    /// The Skills card is on the tree as the settings draw it: its line always, the switches only
    /// while the card is open, the toggle only while there are skills to show, and the line about
    /// who can read what is attached only on a Bot the roster says is shared.
    #[test]
    fn a_bots_skills_are_on_the_tree_and_open_from_it() {
        let mut host = host();
        host.agent_settings_open = true;
        host.agent_skills = Some(skills_card(BotSkills::Loading));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_SKILLS).unwrap().value.as_deref(),
            Some("Asking the server…")
        );
        assert!(tree.find(ids::AGENT_SKILLS_TOGGLE).is_none());
        assert!(host.dispatch(&Op::click(ids::AGENT_SKILLS_TOGGLE)).is_err());

        host.agent_skills = Some(skills_card(BotSkills::Read(some_skills())));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_SKILLS).unwrap().value.as_deref(),
            Some("3 attached · 1 switched off")
        );
        let triage = tree.find("agent-skills-switch-sk_triage").unwrap();
        assert_eq!(
            (
                triage.role.as_str(),
                triage.name.as_str(),
                triage.value.as_deref(),
                triage.checked
            ),
            ("switch", "triage", Some("mine"), Some(true))
        );
        assert!(
            !triage.visible,
            "closed card: the switches are not on screen"
        );
        let review = tree.find("agent-skills-switch-sk_review").unwrap();
        assert_eq!(review.value.as_deref(), Some("org"));
        let old = tree.find("agent-skills-switch-sk_old").unwrap();
        assert!(
            old.states.contains(&"switched-off".to_string()) && old.enabled,
            "switched off and attached: it can be detached"
        );
        assert_eq!(
            tree.find("agent-skills-off-sk_old").unwrap().name,
            "Switched off in Settings → Skills"
        );
        let archive = tree.find("agent-skills-switch-sk_archive").unwrap();
        assert!(
            !archive.enabled,
            "switched off and not attached: never attached"
        );
        assert!(tree.find(ids::AGENT_SKILLS_SHARED).is_none(), "not shared");
        assert!(tree.ids_are_unique());

        host.dispatch(&Op::click(ids::AGENT_SKILLS_TOGGLE)).unwrap();
        assert!(matches!(
            host.take_command(),
            Some(Command::ToggleAgentSkills)
        ));
        host.agent_skills_open = true;
        assert!(
            host.snapshot()
                .find("agent-skills-switch-sk_triage")
                .unwrap()
                .visible
        );

        host.agent_skills = Some(SkillsCard {
            shared: true,
            ..skills_card(BotSkills::Read(some_skills()))
        });
        let shared = host.snapshot();
        let line = shared.find(ids::AGENT_SKILLS_SHARED).unwrap();
        assert_eq!(
            (line.name.as_str(), line.visible),
            (
                "People who use this Bot can read its attached skills.",
                true
            )
        );

        // With the settings closed, the toggle is not on screen to click.
        host.agent_settings_open = false;
        assert!(host.dispatch(&Op::click(ids::AGENT_SKILLS_TOGGLE)).is_err());

        // Skills the server would not give: the card's line says why, and there is nothing to
        // open.
        host.agent_settings_open = true;
        host.agent_skills = Some(skills_card(BotSkills::Unavailable(
            crate::state::NOT_THE_SKILLS_OWNER.into(),
        )));
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_SKILLS).unwrap().value.as_deref(),
            Some(crate::state::NOT_THE_SKILLS_OWNER)
        );
        assert!(tree.find("agent-skills-switch-sk_triage").is_none());
        assert!(host.dispatch(&Op::click(ids::AGENT_SKILLS_TOGGLE)).is_err());
    }

    /// A skill switch is clicked as a person clicks it — the other way from where it stands — and
    /// only where a person could: the settings open, the card open to its skills, and the switch
    /// live. A skill switched off in Settings → Skills can be detached and not attached. A card
    /// with no switches says why it has none.
    #[test]
    fn a_skill_switch_is_clicked_only_where_a_person_could() {
        let mut host = host();
        host.agent_skills = Some(skills_card(BotSkills::Read(some_skills())));
        assert!(
            host.dispatch(&Op::click("agent-skills-switch-sk_triage"))
                .unwrap_err()
                .contains("settings, which are closed")
        );
        host.agent_settings_open = true;
        assert!(
            host.dispatch(&Op::click("agent-skills-switch-sk_triage"))
                .unwrap_err()
                .contains(ids::AGENT_SKILLS_TOGGLE),
            "the card is shut, so its switches are not on screen"
        );
        host.agent_skills_open = true;

        assert_eq!(
            skill_switched(&mut host, "agent-skills-switch-sk_triage"),
            ("sk_triage".into(), false)
        );
        assert_eq!(
            skill_switched(&mut host, "agent-skills-switch-sk_draft"),
            ("sk_draft".into(), true)
        );
        assert_eq!(
            skill_switched(&mut host, "agent-skills-switch-sk_old"),
            ("sk_old".into(), false),
            "a skill switched off in Settings can still be detached"
        );
        let refused = host
            .dispatch(&Op::click("agent-skills-switch-sk_archive"))
            .unwrap_err();
        assert!(
            refused.contains("cannot be attached")
                && refused.contains(crate::state::SWITCHED_OFF_STAYS_DETACHED),
            "{refused}"
        );

        for line in [
            ids::AGENT_SKILLS,
            ids::AGENT_SKILLS_NOTE,
            ids::AGENT_SKILLS_SHARED,
            "agent-skills-off-sk_old",
            "agent-skills-error-sk_draft",
        ] {
            assert!(
                host.dispatch(&Op::click(line))
                    .unwrap_err()
                    .contains("not a switch"),
                "{line}"
            );
        }
        assert!(
            host.dispatch(&Op::click("agent-skills-switch-sk_nope"))
                .unwrap_err()
                .contains("no switch")
        );
        assert!(
            host.dispatch(&Op::click("agent-skills-switch-triage"))
                .unwrap_err()
                .contains("no switch"),
            "a skill is switched by its id, not its name"
        );

        host.agent_skills = Some(skills_card(BotSkills::Unavailable(
            crate::state::NOT_THE_SKILLS_OWNER.into(),
        )));
        let none = host
            .dispatch(&Op::click("agent-skills-switch-sk_triage"))
            .unwrap_err();
        assert!(none.contains(crate::state::NOT_THE_SKILLS_OWNER), "{none}");
        host.agent_skills = Some(skills_card(BotSkills::Loading));
        assert!(
            host.dispatch(&Op::click("agent-skills-switch-sk_triage"))
                .unwrap_err()
                .contains("still being read")
        );
        host.agent_skills = None;
        assert!(
            host.dispatch(&Op::click("agent-skills-switch-sk_triage"))
                .unwrap_err()
                .contains("have not been asked for")
        );
    }

    /// While a skill switch is with the server its row shows where it was asked to go, and nothing
    /// on the card can be clicked, whichever Bot the switch is for. The server's words are under
    /// their skill, a card note is above the switches, and a card the server has made read-only is
    /// dead with its words on it.
    #[test]
    fn a_blocked_skills_card_is_dead_and_says_why() {
        use crate::state::{SkillNote, SkillNotePlace, SkillSwitch, SkillsBlock};
        let mut host = host();
        host.agent_settings_open = true;
        host.agent_skills_open = true;
        host.agent_skills = Some(SkillsCard {
            pending: Some(SkillSwitch {
                coworker_id: "bot-1".into(),
                skill_id: "sk_draft".into(),
                attached: true,
                token: 1,
            }),
            blocked: Some(SkillsBlock::Switching),
            ..skills_card(BotSkills::Read(some_skills()))
        });
        let tree = host.snapshot();
        let asked = tree.find("agent-skills-switch-sk_draft").unwrap();
        assert_eq!(asked.checked, Some(true));
        assert!(asked.states.contains(&"switching".to_string()) && !asked.enabled);
        assert!(!tree.find("agent-skills-switch-sk_triage").unwrap().enabled);
        assert!(
            tree.find(ids::AGENT_SKILLS_WAIT).is_none(),
            "its row says it"
        );
        assert_eq!(
            tree.find(ids::AGENT_SKILLS).unwrap().value.as_deref(),
            Some("4 attached · 1 switched off")
        );
        let busy = host
            .dispatch(&Op::click("agent-skills-switch-sk_triage"))
            .unwrap_err();
        assert!(
            busy.contains(crate::state::CEILING_SWITCH_IN_FLIGHT),
            "{busy}"
        );

        for (blocked, why) in [
            (
                SkillsBlock::AnotherBot,
                crate::state::ANOTHER_BOTS_SKILL_SWITCH,
            ),
            (SkillsBlock::Reading, crate::state::SKILLS_BEING_READ),
        ] {
            host.agent_skills = Some(SkillsCard {
                blocked: Some(blocked),
                ..skills_card(BotSkills::Read(some_skills()))
            });
            let tree = host.snapshot();
            assert_eq!(tree.find(ids::AGENT_SKILLS_WAIT).unwrap().name, why);
            let waiting = host
                .dispatch(&Op::click("agent-skills-switch-sk_triage"))
                .unwrap_err();
            assert!(waiting.contains(why), "{waiting}");
        }

        let said = |words: &str, place: SkillNotePlace| {
            Some(SkillNote {
                coworker_id: "bot-1".into(),
                words: words.into(),
                place,
            })
        };
        host.agent_skills = Some(SkillsCard {
            note: said("no skill sk_draft", SkillNotePlace::Row("sk_draft".into())),
            ..skills_card(BotSkills::Read(some_skills()))
        });
        let tree = host.snapshot();
        assert_eq!(
            tree.find("agent-skills-error-sk_draft").unwrap().name,
            "no skill sk_draft"
        );
        assert!(tree.find(ids::AGENT_SKILLS_NOTE).is_none());

        host.agent_skills = Some(SkillsCard {
            note: said(
                "a coworker takes at most 20 skills, and this is 21",
                SkillNotePlace::Card,
            ),
            ..skills_card(BotSkills::Read(some_skills()))
        });
        assert_eq!(
            host.snapshot().find(ids::AGENT_SKILLS_NOTE).unwrap().name,
            "a coworker takes at most 20 skills, and this is 21"
        );

        host.agent_skills = Some(SkillsCard {
            blocked: Some(SkillsBlock::ReadOnly("your grant was withdrawn".into())),
            note: said("your grant was withdrawn", SkillNotePlace::ReadOnly),
            ..skills_card(BotSkills::Read(some_skills()))
        });
        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::AGENT_SKILLS_READ_ONLY).unwrap().name,
            "your grant was withdrawn"
        );
        assert!(tree.find(ids::AGENT_SKILLS_NOTE).is_none(), "said once");
        assert!(!tree.find("agent-skills-switch-sk_old").unwrap().enabled);
        let theirs = host
            .dispatch(&Op::click("agent-skills-switch-sk_old"))
            .unwrap_err();
        assert!(theirs.contains("your grant was withdrawn"), "{theirs}");
    }

    /// No id a skill can have makes one of the Skills card's ids another's, nor one of the Tools
    /// card's: a skill whose id is `toggle`, `note`, `off-x`, `error-x` or `switch-x` is its own
    /// switch, every id on the tree is one node's, and a line is never read as a switch.
    #[test]
    fn no_skill_id_makes_one_skills_id_another() {
        let skill_ids = [
            "x",
            "toggle",
            "read-only",
            "note",
            "wait",
            "shared",
            "row-x",
            "off-x",
            "error-x",
            "switch-x",
        ];
        let rows: Vec<serde_json::Value> = skill_ids
            .iter()
            .map(|id| {
                serde_json::json!({"id": id, "name": id, "description": "", "scope": "mine",
                    "attached": true, "enabled": false})
            })
            .collect();
        let read = crate::state::SkillsRead {
            rows: serde_json::from_value(serde_json::Value::Array(rows)).unwrap(),
            version: None,
        };
        let mut host = host();
        host.agent_settings_open = true;
        host.agent_skills_open = true;
        host.agent_tools_open = true;
        host.agent_ceiling = Some(ceiling_card(ToolCeiling::Read(a_ceiling())));
        host.agent_skills = Some(SkillsCard {
            note: Some(crate::state::SkillNote {
                coworker_id: "bot-1".into(),
                words: "the skills changed since you looked".into(),
                place: crate::state::SkillNotePlace::Card,
            }),
            shared: true,
            ..skills_card(BotSkills::Read(read))
        });
        let tree = host.snapshot();
        assert!(tree.ids_are_unique());
        for id in skill_ids {
            assert_eq!(
                skill_switched(&mut host, &ids::skill_switch(id)),
                (id.to_string(), false),
                "{id}"
            );
        }
        for line in [
            ids::AGENT_SKILLS_NOTE,
            ids::AGENT_SKILLS_SHARED,
            "agent-skills-off-x",
            "agent-skills-off-off-x",
            "agent-skills-error-switch-x",
        ] {
            assert!(
                host.dispatch(&Op::click(line))
                    .unwrap_err()
                    .contains("not a switch"),
                "{line}"
            );
        }
        // The Tools card's switches are still the Tools card's.
        assert_eq!(
            switched(&mut host, "agent-ceiling-switch-shell"),
            ("shell".into(), false)
        );
    }
}
