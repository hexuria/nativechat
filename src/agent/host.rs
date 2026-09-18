use gpui_agent::prelude::*;
use gpui_agent::{DispatchResult, virtual_unavailable};

use crate::components::chat_input::PanelMode;
use crate::components::chat_input::sources::{
    ParameterSource, SlashSource, ToolSource, ValueSource,
};
use crate::components::composer_panel::ComposerPanelRow;
use crate::opengrok::{
    ChatPart, CoworkerPatch, LocalExecResolution, RecipeKind, RecipeSummary, ScreenshotSpec,
    UserFormDismissMode, UserFormFieldKind, user_form_card_id, user_form_continue_id,
    user_form_dismiss_id, user_form_field_id, user_form_screen_id,
};
use crate::state::{ActiveRecipe, AppState};

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
            Self::StopTurn => state.stop_turn(cx),
            Self::RetryTurn => state.retry_turn(cx),
            Self::ToggleComputerPane => state.toggle_computer_pane(cx),
            Self::OpenCoworkerScreen => state.open_coworker_screen(cx),
            Self::OpenComputerConfirm(action) => state.open_computer_confirm(action, cx),
            Self::ConfirmComputerAction => state.confirm_computer_action(cx),
            Self::CancelComputerConfirm => state.close_computer_confirm(cx),
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
         user-form-field-*, or \"\" for whatever holds the caret)"
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

/// An approval card still waiting on the person.
#[derive(Clone)]
struct ApprovalSnap {
    call_id: String,
    tool: String,
    place: &'static str,
    local: bool,
    review: bool,
}

/// Idle user-form card: fields still on screen. Settled / Sending cards
/// are absent so E2E sees zero email/password fields after Continue.
#[derive(Clone)]
struct UserFormSnap {
    card_key: String,
    title: String,
    fields: Vec<UserFormFieldSnap>,
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

fn user_form_node(form: &UserFormSnap) -> UiNode {
    let key = &form.card_key;
    let mut card = UiNode::dialog(user_form_card_id(key), form.title.clone());
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
    card.with_child(UiNode::button(user_form_continue_id(key), "Continue"))
        .with_child(UiNode::button(user_form_screen_id(key), "Open the screen"))
        .with_child(UiNode::button(user_form_dismiss_id(key), "Dismiss"))
}

/// `approval-<call_id>-<verb>` → the answer it stands for.
/// A recipe row's id, and only a row's. Every control on the recipe page is named
/// `recipe-<something>` too, so a bare prefix match turned a click on a version tab into a
/// fetch of a recipe called "version-1" — the page then said "no such recipe" and the driver
/// could not work the page at all. A recipe's id is what the server mints, `rcp_…`.
fn recipe_row_target(target: &str) -> Option<String> {
    let rest = target.strip_prefix("recipe-")?;
    rest.starts_with("rcp_").then(|| rest.to_string())
}

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
    /// Settings → Computers: Route traffic row, when host/env/box says the tunnel exists.
    route_traffic_visible: bool,
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
                .open_user_forms()
                .into_iter()
                .map(|spec| {
                    let key = spec.card_key().to_string();
                    let typed = state.user_form_typed.get(&key);
                    let picks = state.user_form_picks.get(&key);
                    UserFormSnap {
                        title: if spec.title.is_empty() {
                            "Form".into()
                        } else {
                            spec.title.clone()
                        },
                        fields: spec
                            .fields
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
                            .collect(),
                        card_key: key,
                    }
                })
                .collect(),
            route_traffic_visible: state.show_egress_tunnel_settings(),
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
        page = page.with_child(
            UiNode::new("computer-pane", "dialog", "Computer")
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
                )),
        );
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
                            .with_visible(self.account_open);
                        if self.route_traffic_visible {
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
            other => return Err(format!("unknown invoke `{other}`")),
        };
        self.pending = Some(cmd);
        Ok(DispatchResult::empty())
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

    fn google_login_form() -> UserFormSnap {
        UserFormSnap {
            card_key: "e_form".into(),
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
    fn settled_user_form_has_no_field_nodes() {
        let host = host();
        let tree = host.snapshot();
        assert!(
            tree.find("user-form-field-e_form-email").is_none(),
            "after Continue the snapshot must not list idle fields"
        );
        assert!(tree.find("user-form-continue-e_form").is_none());
    }
}
