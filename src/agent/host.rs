use gpui_agent::prelude::*;
use gpui_agent::{DispatchResult, virtual_unavailable};

use crate::components::chat_input::PanelMode;
use crate::components::chat_input::sources::{
    ParameterSource, SlashSource, ToolSource, ValueSource,
};
use crate::components::composer_panel::ComposerPanelRow;
use crate::opengrok::{
    BoxHandoffResolution, ChatPart, ComputerHandoffStatus, CoworkerPatch, LocalExecResolution,
    RecipeKind, RecipeSummary, ScreenshotSpec, UserFormDismissMode, UserFormFieldKind,
    computer_attention_done_id, computer_attention_id, computer_attention_skip_id,
    computer_handoff_card_id, computer_handoff_done_id, computer_handoff_skip_id,
    computer_handoff_takeover_id, computer_window_attention_done_id, computer_window_attention_id,
    computer_window_attention_skip_id, save_login_card_id, save_login_save_id, save_login_skip_id,
    user_form_card_id, user_form_continue_id, user_form_dismiss_id, user_form_field_id,
    user_form_pill_id, user_form_saved_clear_id, user_form_saved_note_id, user_form_screen_id,
    user_form_use_saved_id,
};
use crate::site_login::{SiteLoginRecord, grouped_logins, login_title};
use crate::state::{ActiveRecipe, AppSettingsTab, AppState};

pub mod ids {
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
    pub const LIGHTBOX: &str = "lightbox";
    pub const PAGE_LOGIN: &str = "page-login";
    pub const LOGIN_EMAIL: &str = "login-email";
    pub const LOGIN_PASSWORD: &str = "login-password";
    pub const LOGIN_SUBMIT: &str = "login-submit";
    pub const LOGIN_ERROR: &str = "login-error";
    pub const DIALOG_ACCOUNT: &str = "dialog-account";
    pub const DIALOG_VOICE: &str = "dialog-voice";
    pub const HEADER_SETTINGS: &str = "header-settings";
    pub const AGENT_SETTINGS: &str = "agent-settings";
    pub const AGENT_SAVE: &str = "agent-save";
    /// The one control that opens a blank routine, whichever of its two shapes the Computer
    /// pane is drawing: the "Create routine" card when the bot has none, the `+` when it has.
    pub const ROUTINE_NEW: &str = "routine-new";

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

    pub fn routine_delete(id: &str) -> String {
        format!("routine-{id}-delete")
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
    ToggleModelPicker,
    SetModelPicker(bool),
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
    /// Update / Reset the active bot's computer: open the confirm dialog, then answer it.
    OpenComputerConfirm(crate::state::ComputerAction),
    ConfirmComputerAction,
    CancelComputerConfirm,
    SetEgressTunnelEnabled(bool),
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
    DeleteRoutine {
        routine_id: String,
    },
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
            Self::ToggleModelPicker => {
                let open = !state.model_picker_open;
                state.set_model_picker_open(open, cx);
            }
            Self::SetModelPicker(open) => state.set_model_picker_open(open, cx),
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
            Self::SendMessageSteer(text) => state.send_message_with(text, true, cx),
            Self::StopTurn => state.stop_turn(cx),
            Self::RetryTurn => state.retry_turn(cx),
            Self::ToggleComputerPane => state.toggle_computer_pane(cx),
            Self::OpenCoworkerScreen => state.open_coworker_screen(cx),
            Self::OpenComputerConfirm(action) => state.open_computer_confirm(action, cx),
            Self::ConfirmComputerAction => state.confirm_computer_action(cx),
            Self::CancelComputerConfirm => state.close_computer_confirm(cx),
            Self::SetEgressTunnelEnabled(enabled) => state.set_egress_tunnel_enabled(enabled, cx),
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
            Self::SetSiteLoginQuery(query) => state.set_site_login_query(query, cx),
            Self::SelectSiteLogin(id) => state.select_site_login(id, cx),
            Self::OpenSiteLoginAdd => state.open_site_login_add(cx),
            Self::CloseSiteLoginAdd => state.close_site_login_add(cx),
            Self::SetSiteLoginNotes { id, notes } => state.update_site_login_notes(id, notes, cx),
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
            Self::DeleteRoutine { routine_id } => {
                if let Some(coworker_id) = state.active_coworker_id.clone() {
                    state.delete_routine(&coworker_id, &routine_id, cx);
                }
            }
            Self::Shutdown => {}
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
            keys,
        }),
        // The protocol's own "the focused editable widget". It is how everything the composer
        // opens is worked: a panel's search field takes the caret as it opens, and the keys
        // that filter and pick a row are meant for that field rather than for the message.
        "" | "focused" => Ok(ComposePlan {
            focus_composer: false,
            keys,
        }),
        other => Err(not_editable(other)),
    }
}

fn not_editable(target: &str) -> String {
    format!(
        "`{target}` is not editable (composer, login-email, login-password, \
         user-form-field-*, settings-logins-search, settings-login-notes-*, or \"\" for \
         whatever holds the caret)"
    )
}

/// The login page's two fields. The page keeps its own text; what the host keeps is the draft
/// the rest of the app reads, which is what `click login-submit` signs in with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginField {
    Email,
    Password,
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
    for message in conversation.messages.iter().rev() {
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
        PanelMode::Slash => SlashSource.rows(recipes),
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
        PanelMode::Slash => "Recipes, workflows and actions".to_string(),
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
}

/// An approval card still waiting on the person.
#[derive(Clone)]
struct ApprovalSnap {
    call_id: String,
    tool: String,
    place: &'static str,
    local: bool,
    review: bool,
    /// The server's word for what suspended the run, and the thread it filed
    /// the card under. Both on the card as states, so a driver can tell an
    /// MCP card from a shell's without reading the title.
    reason: String,
    thread_id: String,
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
fn routine_snap(routine: &crate::state::AgentRoutine) -> RoutineSnap {
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

/// A recipe row's id, and only a row's. Every control on the recipe page is named
/// `recipe-<something>` too, so a bare prefix match turned a click on a version tab into a
/// fetch of a recipe called "version-1" — the page then said "no such recipe" and the driver
/// could not work the page at all. A recipe's id is what the server mints, `rcp_…`.
fn recipe_row_target(target: &str) -> Option<String> {
    let rest = target.strip_prefix("recipe-")?;
    rest.starts_with("rcp_").then(|| rest.to_string())
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

/// `Default` is the host with nothing in it — signed out, no sessions, no panel. Typing ops
/// are planned without reading the app at all, so that empty host is what the tests plan
/// against; everything else comes through [`NativeChatHost::from_app`].
#[derive(Default)]
pub struct NativeChatHost {
    ready: bool,
    sidebar_collapsed: bool,
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
    model_picker_open: bool,
    avatar_editor_open: bool,
    approvals: Vec<ApprovalSnap>,
    computer_open: bool,
    /// The coworker's computer as the pane sees it: "<state>; screen: yes|no",
    /// "endpoint missing", or "unknown".
    computer_status: String,
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
    /// How many routes the Model field can offer, and the server's note about why that is not
    /// more — which is the sentence the person read under the field while the gateway was down.
    model_count: usize,
    model_note: Option<String>,
    /// The Recipes page, when it fills the main slot: its rows, and the recipe open in it.
    recipes_open: bool,
    recipes_filter: &'static str,
    recipes: Vec<RecipeSnap>,
    recipe_open: Option<String>,
    /// The open bot's routines, as the Computer pane lists them.
    routines: Vec<RoutineSnap>,
    /// The composer's panel, when one is open: which list it is, and the rows in it.
    composer_panel: Option<PanelMode>,
    panel_rows: Vec<PanelRow>,
    /// The recipe the next message runs, as the composer's bar shows it.
    recipe_bar: Option<RecipeBarSnap>,
    /// The captions of the newest set of pictures in the transcript, in tile order.
    thumbs: Vec<String>,
    /// The picture overlay, while it is open.
    lightbox: Option<LightboxSnap>,
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
    computer_tab: bool,
    updates_tab: bool,
    /// Dedicated provisioned box: Route traffic icon on the Computer pane.
    route_traffic_on_bot_pane: bool,
    /// User-scope / shared box: Route traffic on Settings → Computer.
    route_traffic_in_user_settings: bool,
    egress_tunnel_enabled: bool,
    /// Box `egress_tunnel.ready` when the computer JSON exposed it.
    egress_tunnel_ready: Option<bool>,
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
                .map(|c| SessionSnap {
                    active: state.active_coworker_id.as_ref() == Some(&c.id),
                    id: c.id.clone(),
                    title: c.name.clone(),
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
                })
                .collect()
        };
        Self {
            ready: true,
            sidebar_collapsed: state.sidebar_collapsed,
            theme_mode: state.theme_mode.clone(),
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
                .and_then(|c| c.messages.iter().rev().find(|m| !m.is_me))
                .map(|m| m.content.clone())
                .unwrap_or_default(),
            bot_status: state.visible_bot_status(),
            turn_in_flight: state.is_turn_in_flight(),
            queued_sends: state.queued_send_count(),
            agent_settings_open: state.is_agent_settings_open(),
            model_picker_open: state.model_picker_open,
            avatar_editor_open: state.avatar_editor_open,
            approvals: state
                .open_approvals()
                .into_iter()
                .map(|spec| ApprovalSnap {
                    local: spec.runs_on_this_mac(),
                    review: spec.is_review_an_action() && state.egress_tunnel_available(),
                    place: spec.place(),
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
            model_count: state.model_catalogue.models.len(),
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
            routines: state
                .active_coworker_id
                .as_deref()
                .map(|id| state.coworker_routines(id))
                .unwrap_or_default()
                .iter()
                .map(routine_snap)
                .collect(),
            composer_panel: state.composer_panel,
            panel_rows: state
                .composer_panel
                .map(|mode| panel_rows(mode, &state.recipes, state.active_recipe.as_ref()))
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
                item
            })
            .collect();

        let sidebar =
            UiNode::navigation(ids::SIDEBAR, "Sidebar")
                .with_child(UiNode::button(ids::NAV_TOGGLE, "Toggle sidebar"))
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
            .with_child(UiNode::textbox(ids::COMPOSER, "Type a message..."))
            .with_child(self.composer_send_node())
            .with_child(UiNode::new(
                "transcript-tail",
                "status",
                if self.last_assistant.is_empty() {
                    "(empty)".to_string()
                } else {
                    self.last_assistant.chars().take(400).collect()
                },
            ));
        if let Some(status) = &self.bot_status {
            page = page.with_child(UiNode::new("bot-status", "status", status.clone()));
        }
        if self.queued_sends > 0 {
            page = page.with_child(UiNode::new(
                ids::COMPOSER_QUEUED,
                "status",
                format!("{} queued", self.queued_sends),
            ));
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
        for (index, caption) in self.thumbs.iter().enumerate() {
            page = page.with_child(UiNode::button(ids::image_thumb(index), caption.clone()));
        }
        if let Some(lightbox) = self.lightbox_node() {
            page = page.with_child(lightbox);
        }
        for approval in &self.approvals {
            let id = format!("approval-{}", approval.call_id);
            let title = if approval.review {
                "Review an action".to_string()
            } else {
                format!("Allow {} on {}?", approval.tool, approval.place)
            };
            let mut card = UiNode::new(id.clone(), "dialog", title)
                .with_child(UiNode::button(format!("{id}-allow-once"), "Allow once"))
                .with_child(UiNode::button(
                    format!("{id}-deny-once"),
                    if approval.review { "Deny" } else { "Deny once" },
                ));
            // Bare facts as states, so an assert does not have to match a sentence.
            for state in [&approval.reason, &approval.thread_id] {
                if !state.is_empty() {
                    card.states.push(state.clone());
                }
            }
            if approval.local || approval.review {
                card = card.with_child(UiNode::button(format!("{id}-always"), "Always allow"));
            }
            if approval.local && !approval.review {
                card = card.with_child(UiNode::button(format!("{id}-never"), "Never"));
            }
            page = page.with_child(card);
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
        let mut computer = UiNode::new("computer-pane", "dialog", "Computer")
            .with_visible(self.computer_open)
            .with_child(UiNode::new(
                "computer-status",
                "status",
                self.computer_status.clone(),
            ))
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
        }
        computer = computer.with_child(UiNode::button(ids::ROUTINE_NEW, "Create routine"));
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
        if let Some(ready) = self.egress_tunnel_ready {
            page = page.with_child(UiNode::status(
                "egress_tunnel.ready",
                if ready { "ready" } else { "not-ready" },
            ));
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
            )
            .with_child(UiNode::button("agent-model-field", "Model"))
            .with_child(
                UiNode::new("agent-model-list", "list", "Models")
                    // How many routes the field can offer. The bug was this going to nothing and
                    // staying there, so it is the number a driver watches — always in the tree,
                    // because "none" is as much an answer as any other count.
                    .with_value(self.model_count.to_string())
                    .with_visible(self.model_picker_open),
            );
        if let Some(note) = &self.model_note {
            // The server's word about why the list is not fuller, under the field, exactly where
            // the person read it. In the tree only while there is one, so its absence is the
            // assertion that the gateway answered.
            settings = settings.with_child(UiNode::status("agent-model-note", note.clone()));
        }

        UiTree {
            app: "nativechat".into(),
            platform: PlatformKind::Desktop,
            ready: self.ready,
            nodes: vec![
                UiNode::window(ids::WINDOW, "NativeChat")
                    .with_child(sidebar)
                    .with_child(page)
                    .with_child(self.recipes_node())
                    .with_child({
                        let mut settings = UiNode::dialog(ids::DIALOG_ACCOUNT, "Settings")
                            .with_visible(self.account_open)
                            .with_child(UiNode::button("app-settings-back", "← Back to app"))
                            .with_child(UiNode::button("settings-tab-computer", "Computer"))
                            .with_child(UiNode::button("settings-tab-updates", "Updates"))
                            .with_child(UiNode::button("settings-tab-logins", "Logins"));
                        if self.logins_tab {
                            settings = self.logins_nodes(settings);
                        }
                        if self.updates_tab {
                            settings = settings.with_child(UiNode::button(
                                "settings-computer-update",
                                self.computer_update_label.clone(),
                            ));
                        }
                        if self.computer_tab && self.route_traffic_in_user_settings {
                            settings = settings.with_child(UiNode::new(
                                "route-traffic-this-computer",
                                "switch",
                                "Route traffic through this computer",
                            ));
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
                        "avatar-editor-dismiss",
                        "Dismiss avatar editor",
                    )),
            ],
        }
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
            page = page.with_child(
                UiNode::new("recipe-detail", "dialog", format!("Recipe {id}"))
                    .with_child(UiNode::button("recipe-back", "Back")),
            );
        }
        page
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
        {
            if let Some(field) = form.fields.iter_mut().find(|field| field.id == field_id) {
                field.value = value.clone();
            }
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
        } else if let Some(rest) = target.strip_prefix("recipe-decline") {
            (rest, false)
        } else {
            return None;
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

    fn click(&mut self, target: &str) -> Result<DispatchResult, String> {
        let cmd = if target == ids::NAV_NEW_CHAT || target == "create-first-bot" {
            Command::NewChat
        } else if target == ids::NAV_TOGGLE {
            Command::ToggleSidebar
        } else if target == ids::FOOTER_THEME {
            Command::ToggleTheme
        } else if target == ids::FOOTER_ACCOUNT {
            Command::ToggleAccount
        } else if target == ids::HEADER_SETTINGS || target == ids::AGENT_SETTINGS {
            Command::ToggleAgentSettings
        } else if target == "agent-model-field" || target == "agent-model-dismiss" {
            Command::ToggleModelPicker
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
        } else if let Some(cmd) = self.user_form_command(target) {
            cmd
        } else if let Some(cmd) = self.computer_handoff_command(target) {
            cmd
        } else if let Some(cmd) = self.save_login_command(target) {
            cmd
        } else if target == "settings-tab-logins" {
            Command::SetAppSettingsTab(AppSettingsTab::Logins)
        } else if target == "settings-tab-computer" {
            Command::SetAppSettingsTab(AppSettingsTab::Computer)
        } else if target == "settings-tab-updates" {
            Command::SetAppSettingsTab(AppSettingsTab::Updates)
        } else if target == "app-settings-back" {
            Command::CloseAppSettings
        } else if target == "computer-update" || target == "settings-computer-update" {
            Command::OpenComputerConfirm(crate::state::ComputerAction::Update)
        } else if target == "computer-reset" {
            Command::OpenComputerConfirm(crate::state::ComputerAction::Reset)
        } else if target == "route-traffic-this-computer" || target == "egress-tunnel-enabled" {
            Command::SetEgressTunnelEnabled(!self.egress_tunnel_enabled)
        } else if target == ids::ROUTINE_NEW {
            Command::OpenRoutineEditor(None)
        } else if let Some(cmd) = self.routine_command(target) {
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

    /// One of a routine's controls, or `None` for a target that is not a routine's at all.
    ///
    /// The id is the server's and can hold anything, dashes included, so this reads the tail
    /// first and takes what is left as the id — and then only if that id is a routine the open
    /// bot has. An id nobody is showing is a wrong address, not a click.
    fn routine_command(&self, target: &str) -> Option<Command> {
        let rest = target.strip_prefix("routine-")?;
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
                "-delete",
                (|id| Command::DeleteRoutine { routine_id: id }) as fn(String) -> Command,
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
            "computer.toggle" => Command::ToggleComputerPane,
            "computer.open" => Command::OpenCoworkerScreen,
            "computer.update" => Command::OpenComputerConfirm(crate::state::ComputerAction::Update),
            "computer.reset" => Command::OpenComputerConfirm(crate::state::ComputerAction::Reset),
            "computer.confirm" => Command::ConfirmComputerAction,
            "computer.cancel" => Command::CancelComputerConfirm,
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
            "routine.rotate" => Command::RotateRoutineWebhook {
                routine_id: self.invoke_routine_id(args, "routine.rotate")?,
            },
            "routine.delete" => Command::DeleteRoutine {
                routine_id: self.invoke_routine_id(args, "routine.delete")?,
            },
            other => return Err(format!("unknown invoke `{other}`")),
        };
        self.pending = Some(cmd);
        Ok(DispatchResult::empty())
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
        }
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
            Command::DeleteRoutine { routine_id } if routine_id == "sch-1-2"
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

    /// A host with a bot and a session, which is what the chat tree is drawn for.
    fn host() -> NativeChatHost {
        NativeChatHost {
            ready: true,
            signed_in: true,
            sessions: vec![SessionSnap {
                id: "bot-1".into(),
                title: "Ada".into(),
                active: true,
            }],
            ..Default::default()
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
        host.panel_rows = panel_rows(PanelMode::Parameters, &[], Some(&untold));
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
        host.panel_rows = panel_rows(PanelMode::Parameters, &[], Some(&told));
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

    /// The Model field's own account of the outage: the list it can still offer, and the note
    /// saying why it is not longer. The note goes when the gateway answers; the count does not
    /// drop to nothing while it is away.
    #[test]
    fn the_model_field_says_how_many_routes_it_has_and_why_it_has_no_more() {
        let mut host = host();
        host.model_count = 12;
        host.model_note = Some("the gateway could not be reached: …".to_string());
        let tree = host.snapshot();
        assert_eq!(
            tree.find("agent-model-list")
                .and_then(|n| n.value.as_deref()),
            Some("12")
        );
        assert!(tree.find("agent-model-note").is_some());

        host.model_note = None;
        assert!(host.snapshot().find("agent-model-note").is_none());
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

    /// What a driver needs to work `/`: the rows are there while the panel is open, and each
    /// one says which of the four kinds it is. Without the word, a workflow row and a recipe
    /// row are the same node under the same id prefix, and "pick the workflow" is a guess.
    #[test]
    fn the_slash_list_tells_a_driver_which_rows_are_workflows() {
        let mut host = host();
        assert!(host.snapshot().find(ids::COMPOSER_PANEL).is_none());

        let listing: Vec<RecipeSummary> = serde_json::from_value(serde_json::json!([
            { "id": "rcp_tape", "name": "Weekly report", "kind": "recipe" },
            { "id": "rcp_tree", "name": "Search and retry", "kind": "workflow" }
        ]))
        .unwrap();
        host.composer_panel = Some(PanelMode::Slash);
        host.panel_rows = panel_rows(PanelMode::Slash, &listing, None);

        let tree = host.snapshot();
        assert_eq!(
            tree.find(ids::COMPOSER_PANEL).unwrap().name,
            "Recipes, workflows and actions",
            "the panel says what is in it, and it is no longer called skills"
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
        // Nothing lists a skill yet, and the row that says so must not look pickable.
        let skill = tree.find("composer-skills-none").unwrap();
        assert!(skill.states.contains(&"note".to_string()));
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
            place: "its computer",
            local: false,
            review: false,
            reason: "policy-approval".into(),
            thread_id: "mcp-cw_1".into(),
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
            vec!["policy-approval".to_string(), "mcp-cw_1".to_string()]
        );
    }
}
