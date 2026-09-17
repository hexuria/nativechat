mod sources;
#[macro_use]
mod sync_macros;

pub use sources::{AppCommand, ComposerPick, TokenKind};

use crate::actions::{Library, NewChat, OpenSettings, Projects, ToggleTheme};
use crate::audio::AudioInput;
use crate::components::composer_panel::{ComposerPanel, ComposerPanelEvent, ComposerPanelRow};
use crate::components::voice_wave::VoiceWave;
use crate::icons::NativeIcon;
use crate::opengrok::{RecipeParameter, RecipeParameterKind};
use crate::state::{ActiveRecipe, AppState, ReplyTo, SubmitChord};
use sources::{ParameterSource, SkillSource, ToolSource, ValueSource};
use std::ops::Range;
use std::path::{Path, PathBuf};

use gpui_kit::InteractiveElement;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Backspace, InputEvent, Textarea, TextareaState},
    popover::Popover,
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

actions!(chat, [SubmitMessage]);

type SubmitCallback = Box<dyn Fn(String, &mut Context<MessageInput>)>;

/// The images that may be attached. The picker itself cannot be told to show only these — GPUI's
/// path prompt has no type filter — so the list is applied to what comes back.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

/// Which list the open panel is showing, and therefore what a picked row means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PanelMode {
    /// The "+" button: attach files, teach a task.
    Plus,
    /// `@` with no recipe on the draft: the bot's tools and apps.
    Tools,
    /// `/`: recipes and the app's own commands.
    Skills,
    /// `@` with a recipe on the draft: what that recipe needs told.
    Parameters,
    /// One parameter of the active recipe, by its place in the declaration, being given a value.
    Value { parameter: usize },
}

/// A chip in the message: what it stands for, and where its text sits.
///
/// The chip is text in the field, because the text field cannot host an element mid-line (see
/// `MessageInput::chip_fills`). This is what makes it more than text: the kind and the id are
/// kept beside it so the message can be sent as structured data rather than re-parsed out of a
/// string that anyone could have typed by hand.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposerToken {
    pub kind: TokenKind,
    /// What the thing is called where it lives: a tool's name, a recipe's id.
    pub id: String,
    /// What the chip reads as in the message: the skill's name, without the `/` that opened the
    /// panel, because that was how it was asked for and not part of what is being said.
    pub text: String,
    /// Where that text sits, in bytes. Kept true across edits by `resync_tokens`.
    pub range: Range<usize>,
}

pub struct MessageInput {
    input_state: Entity<TextareaState>,
    on_submit: Option<SubmitCallback>,
    voice_mode: bool,
    voice_wave: Option<Entity<VoiceWave>>,
    audio_input: Option<AudioInput>,
    state: Entity<AppState>,
    // Cached state to avoid re-rendering on every AppState change
    picked_tools: Vec<crate::state::PickedTool>,
    is_voice_mode_open: bool,
    is_app_settings_open: bool,
    submit_chord: SubmitChord,
    reply_to: Option<ReplyTo>,
    coworker_name: String,
    /// The one wide panel, shared by the "+" button and by the `@` and `/` triggers.
    panel: Entity<ComposerPanel>,
    /// Which list the panel is showing, when it is open.
    panel_mode: Option<PanelMode>,
    /// What each row of the open panel stands for, by row id. The panel only says which row was
    /// picked; this is how the composer knows what to do about it.
    picks: Vec<(SharedString, ComposerPick)>,
    /// Where the caret was when the panel opened, which is where a chip goes.
    caret: usize,
    /// The chips in the message, in the order they appear.
    tokens: Vec<ComposerToken>,
    /// The recipe the message runs, cached off [`AppState`] for the sake of drawing. Every
    /// decision reads the state itself, so a value filled in a moment ago is never missed.
    active_recipe: Option<ActiveRecipe>,
    /// Images to send with the message, shown as thumbnails above the text.
    attachments: Vec<PathBuf>,
    /// A line above the field for something the person needs told: a file that was not an image,
    /// a picker that would not open.
    notice: Option<String>,
    /// Where the mouse went down when a click outside shut the panel, so the click on the "+"
    /// that shut it is not also taken as a click to open it again.
    dismissed_at: Option<Point<Pixels>>,
}

impl MessageInput {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(format!("Message {}", composer_bot_name(&state.read(cx))))
                .auto_grow(1, 20)
                .submit_on_enter(true)
        });
        let panel = cx.new(|cx| ComposerPanel::new(window, cx));

        // Cache initial values from AppState
        let app_state = state.read(cx);
        let picked_tools = app_state.picked_tools.clone();
        let is_voice_mode_open = app_state.is_voice_mode_open;
        let is_app_settings_open = app_state.is_app_settings_open;
        let active_recipe = app_state.active_recipe.clone();
        let submit_chord = app_state.submit_chord;
        let reply_to = app_state.reply_to.clone();
        let coworker_name = composer_bot_name(&app_state);

        let this = Self {
            state: state.clone(),
            input_state: input_state.clone(),
            picked_tools,
            is_voice_mode_open,
            is_app_settings_open,
            submit_chord,
            reply_to,
            coworker_name: coworker_name.clone(),
            on_submit: None,
            voice_mode: false,
            voice_wave: None,
            audio_input: None,
            panel: panel.clone(),
            panel_mode: None,
            picks: Vec::new(),
            caret: 0,
            tokens: Vec::new(),
            active_recipe,
            attachments: Vec::new(),
            notice: None,
            dismissed_at: None,
        };

        // Subscribe to state changes to update cached values and notify only when relevant
        // fields change. It watches with the window, because rows built here have to be able to
        // ask the window what keys are bound, the same as rows built when the panel opened.
        cx.observe_in(&state, window, |this: &mut Self, state, window, cx| {
            let mut changed = false;
            {
                let state = state.read(cx);
                sync_field_clone!(this, state, picked_tools, changed);
                sync_field_copy!(this, state, is_voice_mode_open, changed);
                sync_field_copy!(this, state, is_app_settings_open, changed);
                sync_field_clone!(this, state, reply_to, changed);
                sync_field_clone!(this, state, active_recipe, changed);
                if this.submit_chord != state.submit_chord {
                    this.submit_chord = state.submit_chord;
                    changed = true;
                }
                let name = composer_bot_name(&state);
                if this.coworker_name != name {
                    this.coworker_name = name;
                    changed = true;
                }
            }

            // Recipes that were still being fetched when `/` opened the panel land here.
            if this.panel_mode == Some(PanelMode::Skills) {
                let mut rows = SkillSource.rows(&state.read(cx).recipes);
                apply_shortcuts(&mut rows, window);
                this.remember_picks(&rows);
                let rows: Vec<ComposerPanelRow> = rows.into_iter().map(|(row, _)| row).collect();
                this.panel.update(cx, |panel, cx| panel.set_rows(rows, cx));
            }

            // The open list of parameters shows each value as it is filled in, and shuts if the
            // recipe it is about is dropped out from under it.
            if this.panel_mode == Some(PanelMode::Parameters) {
                match state.read(cx).active_recipe.clone() {
                    Some(recipe) => {
                        let rows = ParameterSource.rows(&recipe);
                        this.remember_picks(&rows);
                        let rows: Vec<ComposerPanelRow> =
                            rows.into_iter().map(|(row, _)| row).collect();
                        this.panel.update(cx, |panel, cx| panel.set_rows(rows, cx));
                    }
                    None => this.close_panel(false, window, cx),
                }
            }

            if changed {
                let send_on_enter = this.submit_chord == SubmitChord::Enter;
                this.input_state.update(cx, |input, cx| {
                    input.set_submit_on_enter(send_on_enter, cx);
                });
                cx.notify();
            }
        })
        .detach();

        // The chips are drawn from where the field put the text, which is only known once it has
        // been laid out; a layout that moved anything is a reason to draw them again.
        cx.observe(&input_state, |this: &mut Self, _input, cx| {
            if !this.tokens.is_empty() {
                cx.notify();
            }
        })
        .detach();

        cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event, window, cx| match event {
                InputEvent::PressEnter { secondary, shift } => {
                    let send = match this.submit_chord {
                        SubmitChord::Enter => !shift && !secondary,
                        SubmitChord::CommandEnter => *secondary,
                    };
                    if send {
                        this.trigger_submit(window, cx);
                    }
                }
                InputEvent::Change => {
                    this.resync_tokens(cx);
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &panel,
            window,
            |this, _panel, event, window, cx| match event {
                ComposerPanelEvent::Selected(id) => this.pick_row(id.clone(), window, cx),
                ComposerPanelEvent::Submitted(typed) => {
                    this.submit_typed_value(typed.to_string(), window, cx)
                }
                ComposerPanelEvent::Dismissed { at } => {
                    this.dismissed_at = *at;
                    // A click outside meant to land somewhere else; only Escape hands the caret back.
                    this.close_panel(at.is_none(), window, cx);
                }
            },
        )
        .detach();

        this
    }

    pub fn on_submit(mut self, handler: impl Fn(String, &mut Context<Self>) + 'static) -> Self {
        self.on_submit = Some(Box::new(handler));
        self
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input_state.update(cx, |state, cx| {
            state.focus_handle(cx).focus(window, cx);
        });
    }

    /// The chips in the draft, for whoever sends it. Nothing reads this yet: the message still
    /// goes to the server as text, and carrying the chips as data is the next step.
    pub fn tokens(&self) -> &[ComposerToken] {
        &self.tokens
    }

    /// The images waiting on the draft. Nothing sends them yet — see `trigger_submit`.
    pub fn attachments(&self) -> &[PathBuf] {
        &self.attachments
    }

    fn trigger_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        println!("Triggering submit...");
        let text = self.input_state.read(cx).value();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            // A recipe that has not been told what it needs cannot run, and the server would
            // refuse the turn. Say which parameter here, before anything is sent and while the
            // draft is still on screen to fix.
            let missing = self
                .state
                .read(cx)
                .active_recipe
                .as_ref()
                .and_then(missing_note);
            if let Some(note) = missing {
                self.notice = Some(note);
                cx.notify();
                return;
            }
            println!("Submitting message: {}", trimmed);
            if let Some(handler) = &self.on_submit {
                (handler)(trimmed.to_string(), cx);
            }
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
            self.tokens.clear();
            // The recipe belonged to the message that has just gone, not to the next one.
            self.state
                .update(cx, |state, cx| state.clear_active_recipe(cx));
            // The images are not on their way anywhere: nothing carries them yet, so saying so
            // is better than leaving them over an empty composer as if they had gone with it.
            if !self.attachments.is_empty() {
                self.attachments.clear();
                self.notice =
                    Some("Images are not sent yet, so that message went without them.".into());
            }
            cx.notify();
            // Focus is handled by the input state usually, or we might need to re-focus
        } else {
            println!("Message is empty, ignoring.");
        }
    }

    fn toggle_voice_mode(&mut self, cx: &mut Context<Self>) {
        if self.voice_mode {
            self.voice_mode = false;
            self.audio_input = None;
            self.voice_wave = None;
        } else {
            let amplitude = self.state.read(cx).amplitude.clone();
            match AudioInput::new(amplitude.clone()) {
                Ok(input) => {
                    self.voice_mode = true;
                    self.audio_input = Some(input);
                    self.voice_wave = Some(VoiceWave::new(amplitude, self.state.clone(), cx));
                }
                Err(e) => {
                    eprintln!("Failed to start audio input: {}", e);
                }
            }
        }
        cx.notify();
    }

    fn confirm_voice_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.voice_mode = false;
        self.audio_input = None;
        self.voice_wave = None;
        cx.notify();

        // Mock transcription
        let mock_text = "This is a simulated transcription of your voice.";
        self.input_state.update(cx, |state, cx| {
            state.set_value(mock_text.to_string(), window, cx);
        });
    }

    // --- The panel -------------------------------------------------------------------------

    fn field_focused(&self, window: &Window, cx: &App) -> bool {
        self.input_state
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }

    fn remember_picks(&mut self, rows: &[(ComposerPanelRow, ComposerPick)]) {
        self.picks = rows
            .iter()
            .map(|(row, pick)| (row.id.clone(), pick.clone()))
            .collect();
    }

    fn show_panel(
        &mut self,
        mode: PanelMode,
        rows: Vec<(ComposerPanelRow, ComposerPick)>,
        placeholder: &str,
        hint: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Read the caret before the search field takes the focus, so a chip lands where the
        // person was typing rather than at the start of the message.
        self.caret = self.input_state.read(cx).cursor();
        self.panel_mode = Some(mode);
        self.dismissed_at = None;
        self.remember_picks(&rows);
        let rows: Vec<ComposerPanelRow> = rows.into_iter().map(|(row, _)| row).collect();
        self.panel.update(cx, |panel, cx| {
            panel.open_with(rows, placeholder, hint, window, cx);
        });
        cx.notify();
    }

    fn close_panel(&mut self, refocus: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_mode.is_none() {
            return;
        }
        self.panel_mode = None;
        self.picks.clear();
        self.panel.update(cx, |panel, cx| panel.close(cx));
        if refocus {
            self.focus(window, cx);
        }
        cx.notify();
    }

    fn toggle_plus_panel(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dismissed = self.dismissed_at.take();
        if self.panel_mode == Some(PanelMode::Plus) {
            self.close_panel(true, window, cx);
            return;
        }
        // This click is the one that shut the panel a moment ago, not a new one.
        if mouse_down_at(event).is_some_and(|at| dismissed == Some(at)) {
            return;
        }
        let rows = vec![
            (
                ComposerPanelRow::new(
                    "attach",
                    "icons/clip.svg",
                    "Attach files",
                    "Images from this Mac — PNG, JPEG, WebP or GIF",
                )
                .element_id("composer-attach"),
                ComposerPick::AttachFiles,
            ),
            (
                ComposerPanelRow::new(
                    "teach",
                    "icons/monitor.svg",
                    "Teach a task",
                    "Show the bot on its screen, and keep what it saw as a recipe",
                )
                .element_id("composer-teach"),
                ComposerPick::TeachTask,
            ),
        ];
        self.show_panel(
            PanelMode::Plus,
            rows,
            "Search",
            "⌘1–9 picks a row. Type @ for the bot's tools, / for its skills.",
            window,
            cx,
        );
    }

    fn open_tools_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = ToolSource.rows();
        self.show_panel(
            PanelMode::Tools,
            rows,
            "Search tools",
            "↑↓ to move, ⌘1–9 to take one straight away, ↵ to put it in the message, esc to close.",
            window,
            cx,
        );
    }

    fn open_skills_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // An empty list may only mean the recipes have never been fetched in this session; ask
        // for them, and the observer above fills the open panel when they land.
        if self.state.read(cx).recipes.is_empty() {
            self.state.update(cx, |state, cx| state.refresh_recipes(cx));
        }
        let mut rows = SkillSource.rows(&self.state.read(cx).recipes);
        apply_shortcuts(&mut rows, window);
        self.show_panel(
            PanelMode::Skills,
            rows,
            "Search skills and actions",
            "↑↓ to move, ⌘1–9 to take one straight away, ↵ to run it or put it in the message, esc to close.",
            window,
            cx,
        );
    }

    /// What the recipe on the draft still needs told: what `@` offers in place of the tools.
    fn open_parameters_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(recipe) = self.state.read(cx).active_recipe.clone() else {
            return;
        };
        let rows = ParameterSource.rows(&recipe);
        let placeholder = format!("Search what {} needs", recipe.name);
        self.show_panel(
            PanelMode::Parameters,
            rows,
            &placeholder,
            &parameters_hint(&recipe),
            window,
            cx,
        );
    }

    /// One parameter's value: the choices its declaration allows, or the panel's own field for
    /// a parameter the declaration leaves open.
    fn open_value_panel(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(recipe) = self.state.read(cx).active_recipe.clone() else {
            return;
        };
        let Some(parameter) = recipe.parameters.get(index) else {
            return;
        };
        let filled = recipe.value(&parameter.name);
        let rows = ValueSource.rows(parameter, filled);
        let placeholder = match filled {
            Some(value) => format!("{} is {value}", parameter.name),
            None => format!("Value for {}", parameter.name),
        };
        self.show_panel(
            PanelMode::Value { parameter: index },
            rows,
            &placeholder,
            &value_hint(parameter),
            window,
            cx,
        );
    }

    fn pick_row(&mut self, id: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pick) = self
            .picks
            .iter()
            .find(|(row, _)| *row == id)
            .map(|(_, pick)| pick.clone())
        else {
            return;
        };
        // Which list this row came out of, before closing the panel forgets it: a value only
        // means anything beside the parameter whose panel was open.
        let mode = self.panel_mode;
        self.close_panel(true, window, cx);
        match pick {
            ComposerPick::AttachFiles => self.attach_files(cx),
            ComposerPick::TeachTask => self.teach_task(cx),
            // A tool is a chip beside the "+", not text in the message: naming a tool says
            // something ABOUT the message, and the message should not have to carry it. A skill
            // is the opposite — it reads as part of the sentence, so it goes in at the caret.
            ComposerPick::Token { kind, id, text } => match kind {
                TokenKind::Tool => {
                    let label = text.trim_start_matches('@').to_string();
                    self.state
                        .update(cx, |state, cx| state.pick_tool(id, label, cx));
                }
                TokenKind::Skill => {
                    // A recipe picked from `/` is not only a word in the sentence: it is what
                    // the turn runs, so it goes on the draft as well as into the message. One
                    // recipe to a message, so picking another takes the first one's chip out
                    // rather than leaving a word standing for a recipe that is not running.
                    let replacing = self
                        .state
                        .read(cx)
                        .active_recipe
                        .as_ref()
                        .is_some_and(|active| active.id != id);
                    if replacing {
                        self.drop_recipe(window, cx);
                    }
                    self.state
                        .update(cx, |state, cx| state.start_recipe(&id, cx));
                    self.insert_token(kind, id, text, window, cx);
                }
            },
            ComposerPick::Command(command) => self.run_command(command, window, cx),
            ComposerPick::Parameter { index } => self.open_value_panel(index, window, cx),
            ComposerPick::Value(value) => {
                let Some(PanelMode::Value { parameter }) = mode else {
                    return;
                };
                self.fill_parameter(parameter, value, window, cx);
            }
            ComposerPick::Nothing => {}
        }
    }

    /// What was typed into the value panel's field, offered as the open parameter's value.
    ///
    /// A value the declaration would not take is refused under the field it was typed into,
    /// with the panel still open and the words still there, so it can be corrected rather than
    /// asked for again. The server checks it too and is the authority; this is only early.
    fn submit_typed_value(&mut self, typed: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(PanelMode::Value { parameter }) = self.panel_mode else {
            return;
        };
        let typed = typed.trim().to_string();
        let refusal = self
            .state
            .read(cx)
            .active_recipe
            .as_ref()
            .and_then(|recipe| recipe.parameters.get(parameter))
            .map(|declared| declared.reject(&typed));
        match refusal {
            Some(Some(refusal)) => {
                self.panel
                    .update(cx, |panel, cx| panel.set_hint(refusal, cx));
            }
            Some(None) => self.fill_parameter(parameter, Some(typed), window, cx),
            None => {}
        }
    }

    /// Give a parameter a value, or take its value away, and go back to the list so the next
    /// one can be filled in without asking for it again.
    fn fill_parameter(
        &mut self,
        index: usize,
        value: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .state
            .read(cx)
            .active_recipe
            .as_ref()
            .and_then(|recipe| recipe.parameters.get(index))
            .map(|parameter| parameter.name.clone())
        else {
            return;
        };
        self.set_parameter_value(&name, value, cx);
        self.close_panel(false, window, cx);
        self.open_parameters_panel(window, cx);
    }

    fn set_parameter_value(&mut self, name: &str, value: Option<String>, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_recipe_value(name, value, cx));
    }

    /// Take the recipe off the draft, and its chip out of the message with it: one pick put
    /// both there, so undoing it undoes both.
    ///
    /// Editing the chip away does not do this. A mode the person set should not come off the
    /// message because of a keystroke in the text, and the bar is where it can be dropped on
    /// purpose.
    fn drop_recipe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(recipe) = self.state.read(cx).active_recipe.clone() else {
            return;
        };
        self.state
            .update(cx, |state, cx| state.clear_active_recipe(cx));
        if let Some(index) = self
            .tokens
            .iter()
            .position(|token| token.kind == TokenKind::Skill && token.id == recipe.id)
        {
            let range = self.tokens.remove(index).range;
            self.remove_text(range, window, cx);
        }
        cx.notify();
    }

    fn run_command(&mut self, command: AppCommand, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(action) = command_action(command) {
            window.dispatch_action(action, cx);
            return;
        }
        match command {
            AppCommand::SettingsTab(tab) => {
                self.state
                    .update(cx, |state, cx| state.open_app_settings(tab, cx));
            }
            AppCommand::Recipes => self.state.update(cx, |state, cx| state.open_recipes(cx)),
            AppCommand::Settings
            | AppCommand::NewChat
            | AppCommand::ToggleTheme
            | AppCommand::Collections
            | AppCommand::Groups => {}
        }
    }

    fn teach_task(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| state.teach_task(cx));
    }

    // --- Chips -----------------------------------------------------------------------------

    /// Put a chip at the caret the panel was opened from, and leave the caret after it.
    fn insert_token(
        &mut self,
        kind: TokenKind,
        id: String,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let caret = self.caret.min(self.input_state.read(cx).value().len());
        self.input_state.update(cx, |input, cx| {
            input.set_selected_range(caret..caret, cx);
            // The trailing space is what lets typing carry on after the chip instead of running
            // into it, and it keeps the chip a token of its own.
            input.insert(format!("{text} "), window, cx);
        });
        let range = caret..caret + text.len();
        let at = self
            .tokens
            .iter()
            .position(|token| token.range.start >= caret)
            .unwrap_or(self.tokens.len());
        self.tokens.insert(
            at,
            ComposerToken {
                kind,
                id,
                text,
                range,
            },
        );
        // The chips after this one have moved along by what was inserted.
        self.resync_tokens(cx);
        self.focus(window, cx);
        cx.notify();
    }

    /// Put every chip's range back where its text actually is.
    ///
    /// The field knows nothing about chips, so an edit anywhere moves them without telling
    /// anyone. Scanning forward in order finds each chip's text after the one before it; a chip
    /// whose text is no longer there was edited away, and it stops being a chip.
    fn resync_tokens(&mut self, cx: &App) {
        if self.tokens.is_empty() {
            return;
        }
        let text = self.input_state.read(cx).value().to_string();
        let mut from = 0usize;
        self.tokens.retain_mut(|token| {
            match text.get(from..).and_then(|rest| rest.find(&token.text)) {
                Some(offset) => {
                    let start = from + offset;
                    token.range = start..start + token.text.len();
                    from = token.range.end;
                    true
                }
                None => false,
            }
        });
    }

    /// Backspace right after a chip takes the whole chip, not one character of it.
    ///
    /// Returns whether it did, so the caller knows whether to let the field have the key.
    fn backspace_over_token(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.field_focused(window, cx) {
            return false;
        }
        let (caret, selection) = {
            let input = self.input_state.read(cx);
            (input.cursor(), input.selected_range())
        };
        if selection.start != selection.end {
            return false;
        }
        let Some(index) = self
            .tokens
            .iter()
            .position(|token| token.range.end == caret)
        else {
            return false;
        };
        let range = self.tokens.remove(index).range;
        self.input_state.update(cx, |input, cx| {
            input.set_selected_range(range, cx);
            input.replace("", window, cx);
        });
        cx.notify();
        true
    }

    /// Take a stretch of the message out, with the space that was inserted after it: a chip
    /// goes in with one, and leaving it behind would leave a gap where the chip was.
    fn remove_text(&mut self, range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).value().to_string();
        let end = match text.get(range.end..) {
            Some(rest) if rest.starts_with(' ') => range.end + 1,
            _ => range.end,
        };
        self.input_state.update(cx, |input, cx| {
            input.set_selected_range(range.start..end, cx);
            input.replace("", window, cx);
        });
        self.resync_tokens(cx);
    }

    /// `@` and `/` open the panel instead of being typed.
    ///
    /// Returns whether the key was taken. A trigger only counts at the start of a token — at the
    /// very start of the message or right after a space — so an email address types its `@`.
    fn trigger_panel(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.voice_mode || self.panel_mode.is_some() || !self.field_focused(window, cx) {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return false;
        }
        let mode = match event.keystroke.key_char.as_deref() {
            Some("@") => PanelMode::Tools,
            Some("/") => PanelMode::Skills,
            _ => return false,
        };
        let (text, caret, selection) = {
            let input = self.input_state.read(cx);
            (
                input.value().to_string(),
                input.cursor(),
                input.selected_range(),
            )
        };
        // Typing over a selection replaces it, which is the field's business and not a trigger.
        if selection.start != selection.end || !starts_token(&text, caret) {
            return false;
        }
        match mode {
            // Once a recipe is on the draft, `@` is for what that recipe needs told. The turn
            // is already a recipe run, and a roster of the bot's tools is not what is missing
            // from it — which is exactly what someone who has just picked a recipe finds when
            // the list they are shown is tools.
            PanelMode::Tools if self.state.read(cx).active_recipe.is_some() => {
                self.open_parameters_panel(window, cx)
            }
            PanelMode::Tools => self.open_tools_panel(window, cx),
            PanelMode::Skills => self.open_skills_panel(window, cx),
            PanelMode::Plus | PanelMode::Parameters | PanelMode::Value { .. } => {}
        }
        true
    }

    /// Where each chip's fill goes, in the window's own coordinates.
    ///
    /// The field paints its text in one style and hosts no elements of its own, so a chip cannot
    /// be an element in the text flow. What it can be is this: the field says where a byte range
    /// ended up in the window, and a fill goes there, under the glyphs the field paints over it.
    /// The chip therefore reads inline and wraps with the text, because it is the text.
    fn chip_fills(&self, cx: &App) -> Vec<Bounds<Pixels>> {
        if self.tokens.is_empty() {
            return Vec::new();
        }
        let input = self.input_state.read(cx);
        let line_height = input.line_height();
        self.tokens
            .iter()
            .filter_map(|token| {
                let bounds = input.range_to_bounds(&token.range)?;
                // A chip that wrapped onto a second line has no one rectangle to sit in; leave
                // it plain rather than fill the whole box the two lines make between them.
                let wrapped = line_height.is_some_and(|line| bounds.size.height > line * 1.5);
                if wrapped || bounds.size.width <= px(0.) {
                    return None;
                }
                // A little more than the glyphs, so the fill reads as a chip around the word.
                Some(Bounds::new(
                    bounds.origin - point(px(3.), px(1.)),
                    bounds.size + size(px(6.), px(2.)),
                ))
            })
            .collect()
    }

    /// The text field, with the chips' fills under it.
    ///
    /// The fills are painted rather than placed as elements because the field's text does not
    /// start where this box does: the input keeps its own padding between the two, and an
    /// absolutely placed fill, whose offsets can only be measured from the text, therefore lands
    /// above and to the left of the word it belongs to by exactly that padding. What the field
    /// reports is a rectangle in the window, so the window is where it is painted. The canvas
    /// covers this box and clips to it, which keeps a chip that has scrolled out of a tall draft
    /// from being painted over whatever is above the composer.
    fn field(&self, theme: &gpui_kit::component::Theme, cx: &App) -> AnyElement {
        let fills = self.chip_fills(cx);
        let color = theme.primary.opacity(0.16);
        div()
            .relative()
            .w_full()
            .when(!fills.is_empty(), |this| {
                this.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                for chip in fills {
                                    window.paint_quad(
                                        fill(chip, color).corner_radii(Corners::all(px(5.))),
                                    );
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .child(Textarea::new(&self.input_state).appearance(false).w_full())
            .into_any_element()
    }

    // --- The recipe on the draft -------------------------------------------------------------

    /// The bar that says which recipe the message runs, what it still needs, and how to drop it.
    ///
    /// The chip in the message is not this, and cannot be. A chip is text: it can be typed over
    /// or deleted like any other word, and the message has to stay exactly what the person
    /// wrote, so nothing that must be true about the turn can be read off it. The bar is also
    /// the only place a parameter's value can be shown — a value is not prose, and putting it in
    /// the sentence would change what was said.
    fn recipe_bar(
        &self,
        theme: &gpui_kit::component::Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let recipe = self.active_recipe.clone()?;
        let secondary = theme.secondary;
        let muted_foreground = theme.muted_foreground;
        let secondary_foreground = theme.secondary_foreground;
        let border = theme.border;
        let danger = theme.danger;
        let has_parameters = !recipe.parameters.is_empty();
        let chips: Vec<AnyElement> = recipe
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let name = parameter.name.clone();
                let filled = recipe.value(&name).map(str::to_string);
                let needed = parameter.required && filled.is_none();
                let label = match &filled {
                    Some(value) => format!("{name}: {}", chip_value(value)),
                    None if parameter.required => format!("{name} (needed)"),
                    None => name.clone(),
                };
                let tip = SharedString::from(chip_tooltip(parameter));
                let clear_name = name.clone();
                h_flex()
                    .id(SharedString::from(format!("composer-recipe-param-{name}")))
                    .items_center()
                    .gap_1()
                    .px(px(6.))
                    .py(px(1.))
                    .rounded_md()
                    .border_1()
                    .border_color(if needed { danger } else { border })
                    .when(filled.is_some(), |this| this.bg(secondary))
                    .text_size(px(11.))
                    .text_color(if needed { danger } else { secondary_foreground })
                    .cursor_pointer()
                    .hover(move |style| style.bg(secondary))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_value_panel(index, window, cx);
                    }))
                    .child(div().child(label))
                    .when(filled.is_some(), |this| {
                        this.child(
                            div()
                                .id(SharedString::from(format!(
                                    "composer-recipe-param-clear-{clear_name}"
                                )))
                                .size(px(12.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .child(
                                    Icon::new(NativeIcon::Close)
                                        .size(px(8.))
                                        .text_color(secondary_foreground),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.set_parameter_value(&clear_name, None, cx);
                                })),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        Some(
            h_flex()
                .id("composer-recipe-bar")
                .w_full()
                .items_start()
                .justify_between()
                .gap_2()
                .px_1()
                .pb_1()
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .gap(px(3.))
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    Icon::default()
                                        .path("icons/record.svg")
                                        .size(px(11.))
                                        .text_color(muted_foreground),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                                        .text_color(muted_foreground)
                                        .truncate()
                                        .child(format!("Recipe · {}", recipe.name)),
                                )
                                .child(div().text_xs().text_color(muted_foreground).child(
                                    if has_parameters {
                                        "— @ fills these in"
                                    } else {
                                        "— it needs nothing told"
                                    },
                                )),
                        )
                        .when(has_parameters, |this| {
                            this.child(
                                h_flex()
                                    .id("composer-recipe-parameters")
                                    .w_full()
                                    .flex_wrap()
                                    .gap_1()
                                    .children(chips),
                            )
                        }),
                )
                .child(
                    div()
                        .id("composer-recipe-drop")
                        .size(px(22.))
                        .flex_shrink_0()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |style| style.bg(secondary))
                        .tooltip(|window, cx| Tooltip::new("Drop the recipe").build(window, cx))
                        .child(
                            Icon::new(IconName::Close)
                                .size(px(12.))
                                .text_color(secondary_foreground),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.drop_recipe(window, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    // --- Attachments -----------------------------------------------------------------------

    fn attach_files(&mut self, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |this, cx| {
            let chosen = answer.await;
            let _ = this.update(cx, |this, cx| {
                match chosen {
                    Ok(Ok(Some(paths))) => this.add_attachments(paths),
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => {
                        this.notice = Some(format!("The file picker would not open: {error}"));
                    }
                    Err(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Keep the images and say so when something else was picked: the prompt cannot be told to
    /// offer images only, so this is the only place the answer is narrowed.
    fn add_attachments(&mut self, paths: Vec<PathBuf>) {
        let before = self.attachments.len();
        let mut refused = 0;
        for path in paths {
            if is_image(&path) {
                if !self.attachments.contains(&path) {
                    self.attachments.push(path);
                }
            } else {
                refused += 1;
            }
        }
        self.notice = (refused > 0).then(|| {
            if self.attachments.len() == before {
                "Only images can be attached: PNG, JPEG, WebP or GIF.".to_string()
            } else {
                format!("{refused} of those were not images, so they were left out.")
            }
        });
    }

    fn thumbnails(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .id("composer-attachments")
            .w_full()
            .flex_wrap()
            .gap(px(8.))
            .px(px(2.))
            .pb(px(2.))
            .children(self.attachments.iter().enumerate().map(|(index, path)| {
                let group = SharedString::from(format!("composer-attachment-{index}"));
                div()
                    .id(SharedString::from(format!("composer-attachment-{index}")))
                    .group(group.clone())
                    .relative()
                    .size(px(56.))
                    .flex_shrink_0()
                    .rounded(px(10.))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.secondary)
                    .child(
                        img(path.clone())
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .rounded(px(10.)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "composer-attachment-remove-{index}"
                            )))
                            .absolute()
                            .top(px(2.))
                            .right(px(2.))
                            .size(px(18.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme.background.opacity(0.85))
                            .cursor_pointer()
                            .opacity(0.)
                            .group_hover(group, |style| style.opacity(1.))
                            .child(
                                Icon::new(NativeIcon::Close)
                                    .size(px(10.))
                                    .text_color(theme.secondary_foreground),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if index < this.attachments.len() {
                                    this.attachments.remove(index);
                                    this.notice = None;
                                    cx.notify();
                                }
                            })),
                    )
            }))
            .into_any_element()
    }
}

/// The action one of the app's own commands dispatches, for the commands that have one. The
/// rest reach the app through [`AppState`] instead, and so have no chord to show or to send.
/// One table serves both, so a row can never advertise a key that runs something else.
fn command_action(command: AppCommand) -> Option<Box<dyn Action>> {
    match command {
        AppCommand::Settings => Some(Box::new(OpenSettings)),
        AppCommand::NewChat => Some(Box::new(NewChat)),
        AppCommand::ToggleTheme => Some(Box::new(ToggleTheme)),
        AppCommand::Collections => Some(Box::new(Library)),
        AppCommand::Groups => Some(Box::new(Projects)),
        AppCommand::SettingsTab(_) | AppCommand::Recipes => None,
    }
}

/// Put the real chord on every row that stands for one of the app's commands, read from the
/// keymap the app registered rather than written out beside the row. A row whose command has no
/// binding — and there are several — keeps its empty chord and shows no keys at all.
fn apply_shortcuts(rows: &mut [(ComposerPanelRow, ComposerPick)], window: &Window) {
    for (row, pick) in rows {
        let ComposerPick::Command(command) = pick else {
            continue;
        };
        let Some(action) = command_action(*command) else {
            continue;
        };
        let Some(binding) = window.highest_precedence_binding_for_action(action.as_ref()) else {
            continue;
        };
        let chord = binding
            .keystrokes()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        if !chord.is_empty() {
            row.shortcut = Some(chord.into());
        }
    }
}

/// What the recipe still needs before the message can be sent, named, or nothing when it can
/// go. Naming them is the point: "fill in the required fields" leaves someone hunting.
fn missing_note(recipe: &ActiveRecipe) -> Option<String> {
    let missing = recipe.missing();
    if missing.is_empty() {
        return None;
    }
    Some(format!(
        "{} needs {} before this can be sent.",
        recipe.name,
        join_names(&missing)
    ))
}

/// The line under the list of parameters: what is still missing, or how the list is worked.
fn parameters_hint(recipe: &ActiveRecipe) -> String {
    match missing_note(recipe) {
        Some(note) => format!("{note} ↵ fills one in, esc closes."),
        None => "↑↓ to move, ↵ to fill one in, esc to close.".to_string(),
    }
}

/// The line under one parameter's value: whether it is picked from a list or typed.
fn value_hint(parameter: &RecipeParameter) -> String {
    match (parameter.allowed(), parameter.kind) {
        (Some(allowed), _) => format!("One of: {}. ↵ takes it, esc closes.", allowed.join(", ")),
        (None, RecipeParameterKind::Boolean) => {
            "Pick the yes or the no. ↵ takes it, esc closes.".to_string()
        }
        (None, RecipeParameterKind::Number) => "Type a number and press ↵. esc closes.".to_string(),
        (None, RecipeParameterKind::Text) => "Type it and press ↵. esc closes.".to_string(),
    }
}

/// What a parameter's chip says when the pointer rests on it, which is the room the chip itself
/// does not have: what the parameter is for, and what it takes.
fn chip_tooltip(parameter: &RecipeParameter) -> String {
    let said = parameter.description.trim();
    let takes = match parameter.allowed() {
        Some(allowed) => format!("one of: {}", allowed.join(", ")),
        None => parameter.kind.label().to_string(),
    };
    let standing = if parameter.required {
        "required"
    } else {
        "optional"
    };
    if said.is_empty() {
        format!("{} — {standing} {takes}", parameter.name)
    } else {
        format!("{said} — {standing} {takes}")
    }
}

/// A value that would push the drop button off the end of the bar is cut: the chip is there to
/// say the parameter is filled, and the whole of a long value can be read where it was typed.
fn chip_value(value: &str) -> String {
    const MOST: usize = 18;
    if value.chars().count() <= MOST {
        return value.to_string();
    }
    let kept: String = value.chars().take(MOST - 1).collect();
    format!("{kept}…")
}

/// Several names as a person would say them: "a", "a and b", "a, b and c".
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Whether a trigger character sits at the start of a token: the start of the message, or right
/// after a space. `me@example.com` therefore types its `@` rather than opening the panel.
fn starts_token(text: &str, caret: usize) -> bool {
    let caret = caret.min(text.len());
    text[..caret]
        .chars()
        .next_back()
        .is_none_or(char::is_whitespace)
}

/// Where a click's own mouse down was, for telling one click from another. A click from the
/// keyboard has no such place, and is never the one that shut a panel.
fn mouse_down_at(event: &ClickEvent) -> Option<Point<Pixels>> {
    match event {
        ClickEvent::Mouse(mouse) => Some(mouse.down.position),
        _ => None,
    }
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension.as_str()))
}

impl Render for MessageInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state_model = self.state.clone();
        let placeholder = format!("Message {}", self.coworker_name);
        self.input_state.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
        });

        let theme = cx.theme().clone();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;
        let muted_foreground = theme.muted_foreground;
        let foreground = theme.foreground;
        let background = theme.background;
        let border = theme.border;
        let picked_tools = self.picked_tools.clone();

        // Check if any modal is open using cached state
        let any_modal_open = self.is_voice_mode_open || self.is_app_settings_open;
        let draft = self.input_state.read(cx).value();
        let compact = !self.voice_mode
            && self.picked_tools.is_empty()
            && self.attachments.is_empty()
            && self.notice.is_none()
            && self.active_recipe.is_none()
            && !draft.contains('\n');
        let panel_open = self.panel_mode.is_some();
        let thumbnails = (!self.attachments.is_empty()).then(|| self.thumbnails(&theme, cx));

        // ChatGPT-style: centered container with max-width
        h_flex().w_full().justify_center().child(
            v_flex()
                .relative()
                .max_w(px(crate::chrome::CHAT_CONTENT_MAX))
                .w_full()
                // `@` and `/` never reach the field: the panel opens instead, and nothing is
                // typed. Capture, because the field would otherwise have the character first.
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if this.trigger_panel(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                // Backspace against a chip takes the chip. Capture, because the field binds the
                // key to its own action, and actions are dispatched before key listeners.
                .capture_action(cx.listener(|this, _: &Backspace, window, cx| {
                    if this.backspace_over_token(window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .when(panel_open, |this| {
                    this.child(deferred(
                        // Above the composer and the width of it: `bottom: 100%` puts the
                        // panel's bottom edge on the composer's top edge.
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(relative(1.))
                            .mb(px(8.))
                            .child(self.panel.clone()),
                    )
                    .with_priority(3))
                })
                .child(
            // Input container - rounded pill shape with shadow
            v_flex()
                .key_context("MessageInput")
                .w_full()
                .gap_2()
                .when(compact, |this| this.px_3().py(px(6.)))
                .when(!compact, |this| this.px_4().py_3())
                .bg(background) // Match chat background (white in light mode)
                .border_1()
                .border_color(border)
                .rounded(px(26.0)) // Rounded pill shape
                .shadow_sm()
                .when_some(self.reply_to.clone(), |this, reply| {
                    let preview = reply.preview.clone();
                    this.child(
                        h_flex()
                            .id("reply-bar")
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_1()
                            .pb_1()
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                                            .text_color(muted_foreground)
                                            .child("Replying"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted_foreground)
                                            .truncate()
                                            .child(preview),
                                    ),
                            )
                            .child(
                                div()
                                    .id("reply-dismiss")
                                    .size(px(22.))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(secondary))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(12.))
                                            .text_color(secondary_foreground),
                                    )
                                    .on_click({
                                        let state_model = state_model.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            state_model.update(cx, |state, cx| {
                                                state.clear_reply_to(cx);
                                            });
                                        }
                                    }),
                            ),
                    )
                })
                .children(self.recipe_bar(&theme, cx))
                .when_some(self.notice.clone(), |this, notice| {
                    this.child(
                        h_flex()
                            .id("composer-notice")
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_1()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(muted_foreground)
                                    .child(notice),
                            )
                            .child(
                                div()
                                    .id("composer-notice-dismiss")
                                    .size(px(18.))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(secondary))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(10.))
                                            .text_color(secondary_foreground),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.notice = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                })
                .children(thumbnails)
                .when(!compact, |this| {
                    this.child(
                        // Top: Input field (grows to fill space)
                        div().flex_grow(1.).child(if self.voice_mode {
                            if let Some(voice_wave) = &self.voice_wave {
                                voice_wave.clone().into_any_element()
                            } else {
                                div().into_any_element()
                            }
                        } else {
                            self.field(&theme, cx)
                        }),
                    )
                })
                .child(
                    // Bottom: Toolbar (and the field, when the composer is one line)
                    h_flex()
                        .when(compact, |this| this.items_center().gap_1())
                        .when(!compact, |this| this.justify_between().items_start().gap_2())
                        .child(
                            // The one wide panel, for everything the composer offers.
                            Button::new("add-app")
                                .icon(IconName::Plus)
                                .ghost()
                                .rounded_full()
                                .when(!any_modal_open, |this| this.cursor_pointer())
                                .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                                    this.toggle_plus_panel(event, window, cx);
                                })),
                        )
                        .when(!compact, |this| {
                        this.child(
                            // Bottom Row
                            div()
                                .flex()
                                .flex_1() // Allow this section to shrink/grow
                                .min_w_0() // Allow shrinking below content size to force wrapping
                                .flex_wrap() // Allow wrapping
                                .items_center()
                                .gap_2()
                                .children(
                                    std::iter::once(
                                        div().into_any_element() // Placeholder or remove entirely if not needed
                                    )
                                    .chain({
                                        // Grouped by what each chip IS, which the pick recorded.
                                        // Matching names against a hardcoded list only worked
                                        // while the names came from one hardcoded menu.
                                        use crate::state::PickedKind;
                                        let (tool_calls, mini_apps): (Vec<_>, Vec<_>) = picked_tools
                                            .iter()
                                            .cloned()
                                            .partition(|picked| picked.kind == PickedKind::Tool);

                                        let groups = vec![
                                            ("Tools", tool_calls, "icons/wrench.svg"),
                                            ("Apps", mini_apps, "icons/plugins.svg"),
                                        ];

                                        let state_model = state_model.clone();
                                        groups.into_iter().flat_map(move |(group_name, apps, icon_path)| -> Box<dyn Iterator<Item = AnyElement>> {
                                            if apps.len() >= 2 {
                                                let apps_clone = apps.clone();
                                                let state_model = state_model.clone();
                                                let group_name = group_name.to_string();
                                                let icon_path = icon_path.to_string();

                                                Box::new(std::iter::once(
                                                    Popover::new(SharedString::from(format!("aggregated-{}-popover", group_name.to_lowercase())))
                                                        .anchor(Anchor::BottomLeft)
                                                        .trigger(
                                                            Button::new(SharedString::from(format!("aggregated-{}-btn", group_name.to_lowercase())))
                                                                .ghost()
                                                                .bg(secondary)
                                                                .rounded_md()
                                                                .px_2()
                                                                .py_1()
                                                                .child(
                                                                    h_flex()
                                                                        .gap_1()
                                                                        .items_center()
                                                                        .child(
                                                                            svg()
                                                                                .path(icon_path.clone())
                                                                                .size(px(12.0))
                                                                                .text_color(secondary_foreground)
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .child(format!("{} {}", apps.len(), group_name.to_lowercase()))
                                                                                .text_size(px(12.0)),
                                                                        )
                                                                        .child(
                                                                            Icon::new(IconName::ChevronDown)
                                                                                .size(px(12.0))
                                                                                .text_color(secondary_foreground)
                                                                        )
                                                                )
                                                        )
                                                        .content(move |_, _, cx| {
                                                            let theme = cx.theme();
                                                            v_flex()
                                                                .w(px(200.0))
                                                                .p_1()
                                                                .gap_1()
                                                                .children(
                                                                    apps_clone.iter().enumerate().map(|(i, app)| {
                                                                        let app_id = app.id.clone();
                                                                        let app_label = app.label.clone();
                                                                        let icon = tool_icon(&app_label);

                                                                        h_flex()
                                                                            .gap_2()
                                                                            .items_center()
                                                                            .px_2()
                                                                            .py_1()
                                                                            .rounded_sm()
                                                                            .hover(move |s| s.bg(theme.secondary))
                                                                            .cursor_pointer()
                                                                            .id(SharedString::from(format!("remove-{}-aggregated-{}", group_name.to_lowercase(), i)))
                                                                            .on_click({
                                                                                let state_model = state_model.clone();
                                                                                move |_event, _window, cx| {
                                                                                    state_model.update(cx, |state, cx| {
                                                                                        state.unpick_tool(&app_id, cx);
                                                                                    });
                                                                                }
                                                                            })
                                                                            .child(
                                                                                Icon::new(icon)
                                                                                    .size(px(12.0))
                                                                                    .text_color(theme.secondary_foreground)
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .child(app_label.clone())
                                                                                    .text_size(px(12.0))
                                                                            )
                                                                            .child(
                                                                                div().flex_grow(1.) // Spacer
                                                                            )
                                                                            .child(
                                                                                Icon::new(NativeIcon::Close)
                                                                                    .size(px(12.0))
                                                                                    .text_color(theme.secondary_foreground)
                                                                            )
                                                                    })
                                                                )
                                                        })
                                                        .into_any_element()
                                                ))
                                            } else {
                                                // Render individual tags
                                                let state_model = state_model.clone();
                                                Box::new(apps.into_iter().enumerate().map(move |(i, app)| {
                                                        let app_id = app.id.clone();
                                                        let app_name = app.label.clone();
                                                        let icon = tool_icon(&app_name);

                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .bg(secondary)
                                                            .rounded_md()
                                                            .px_2()
                                                            .py_1()
                                                            .child(
                                                                Icon::new(icon)
                                                                    .size(px(12.0))
                                                                    .text_color(secondary_foreground)
                                                            )
                                                            .child(
                                                                div()
                                                                    .child(app_name.clone())
                                                                    .text_size(px(12.0)),
                                                            )
                                                            .child(
                                                                div()
                                                                    .id(SharedString::from(format!("remove-{}-{}", app_name.to_lowercase(), i)))
                                                                    .cursor_pointer()
                                                                    .on_click({
                                                                        let state_model = state_model.clone();
                                                                        move |_event, _window, cx| {
                                                                            state_model.update(cx, |state, cx| {
                                                                                state.unpick_tool(&app_id, cx);
                                                                            });
                                                                        }
                                                                    })
                                                                    .child(
                                                                        Icon::new(NativeIcon::Close)
                                                                            .size(px(14.0)),
                                                                    ),
                                                            )
                                                            .into_any_element()
                                                    }))
                                            }
                                        })
                                    })
                                )
                                )
                        })
                        .when(compact, |this| {
                            this.child(
                                div()
                                    .id("composer-field")
                                    .flex_1()
                                    .min_w_0()
                                    .w_full()
                                    .child(self.field(&theme, cx)),
                            )
                        })
                        .child(
                            // Right: Action Icons
                            h_flex()
                                .flex_none() // Prevent this section from shrinking
                                .gap_1()
                                .items_center()
                                .when(self.voice_mode, |this| {
                                    // Voice Mode: Cancel (X) and Confirm (Check)
                                    this.child({
                                        let mut cancel_btn = div()
                                            .id("cancel-voice-btn")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_voice_mode(cx);
                                            }))
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(gpui_kit::transparent_black())
                                            .text_color(secondary_foreground)
                                            .hover(move |style| style.bg(secondary))
                                            .tooltip(|w, cx| Tooltip::new("Cancel").build(w, cx))
                                            .child(
                                                Icon::new(NativeIcon::Close)
                                                    .text_color(secondary_foreground),
                                            );

                                        if !any_modal_open {
                                            cancel_btn = cancel_btn.cursor_pointer();
                                        }

                                        cancel_btn
                                    })
                                    .child({
                                        let mut confirm_btn = div()
                                            .id("confirm-voice-btn")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.confirm_voice_input(window, cx);
                                            }))
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(foreground) // Black/White
                                            .text_color(background) // White/Black
                                            .hover(move |style| {
                                                style.bg(foreground.opacity(0.8))
                                            })
                                            .tooltip(|w, cx| Tooltip::new("Done").build(w, cx))
                                            .child(
                                                Icon::new(IconName::Check)
                                                    .text_color(background),
                                            );

                                        if !any_modal_open {
                                            confirm_btn = confirm_btn.cursor_pointer();
                                        }

                                        confirm_btn
                                    })
                                })
                                .when(!self.voice_mode, |this| {
                                    // Text Mode: Mic and Send/Headphone
                                    this.child({
                                        let mut mic_btn = div()
                                            .id("dictate")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_voice_mode(cx);
                                            }))
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(gpui_kit::transparent_black()) // Transparent/White by default
                                            .text_color(secondary_foreground)
                                            .hover(move |style| style.bg(secondary)) // Gray on hover
                                            .tooltip(|w, cx| Tooltip::new("Dictate").build(w, cx))
                                            .child(
                                                svg()
                                                    .path("icons/mic.svg")
                                                    .size(px(18.0))
                                                    .text_color(secondary_foreground),
                                            );

                                        if !any_modal_open {
                                            mic_btn = mic_btn.cursor_pointer();
                                        }

                                        mic_btn
                                    })
                                    .child(
                                        if self.input_state.read(cx).text().len() == 0 {
                                            // Empty state: Sparkles icon - opens voice mode modal
                                            let mut sparkles_btn = div()
                                                .id("voice-mode")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.state.update(cx, |state, cx| {
                                                        state.start_voice_mode(cx);
                                                    });
                                                }))
                                                .w(px(36.0))
                                                .h(px(36.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .bg(gpui_kit::transparent_black())
                                                .text_color(secondary_foreground)
                                                .hover(move |style| style.bg(secondary))
                                                .tooltip(|w, cx| {
                                                    Tooltip::new("Voice Mode").build(w, cx)
                                                })
                                                .child(
                                                    svg()
                                                        .path("icons/sparkles.svg")
                                                        .size(px(18.0))
                                                        .text_color(secondary_foreground),
                                                );

                                            if !any_modal_open {
                                                sparkles_btn = sparkles_btn.cursor_pointer();
                                            }

                                            sparkles_btn
                                        } else {
                                            // Typing state: Send button (Black bg, White arrow)
                                            let mut send_btn = div()
                                                .id("send-btn")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.trigger_submit(window, cx);
                                                }))
                                                .w(px(36.0))
                                                .h(px(36.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .bg(foreground) // Theme-aware foreground (Black in light, White in dark)
                                                .text_color(background) // Theme-aware background (White in light, Black in dark)
                                                .hover(move |style| {
                                                    style.bg(foreground.opacity(0.8))
                                                })
                                                .tooltip(|w, cx| {
                                                    Tooltip::new("Send message").build(w, cx)
                                                })
                                                .child(
                                                    Icon::new(IconName::ArrowUp)
                                                        .text_color(background),
                                                );

                                            if !any_modal_open {
                                                send_btn = send_btn.cursor_pointer();
                                            }

                                            send_btn
                                        }
                                    )
                                })
                        )
                )),
        )
    }
}

fn tool_icon(name: &str) -> Icon {
    match name {
        "Image Generation" => Icon::new(NativeIcon::CreateImage),
        "Thinking" => Icon::new(NativeIcon::Thinking),
        "Deep Research" => Icon::new(NativeIcon::DeepSearch),
        "Study" => Icon::new(NativeIcon::Study),
        "Web search" => Icon::new(NativeIcon::WebSearch),
        "Canvas" => Icon::new(NativeIcon::Canvas),
        "Canva" => Icon::new(NativeIcon::Canva),
        "Coursera" => Icon::new(NativeIcon::Coursera),
        "Figma" => Icon::new(NativeIcon::Figma),
        "Spotify" => Icon::new(NativeIcon::Spotify),
        _ => Icon::new(NativeIcon::Clip),
    }
}

fn composer_bot_name(state: &AppState) -> String {
    state
        .active_coworker_id
        .as_ref()
        .and_then(|id| state.coworkers.iter().find(|c| &c.id == id))
        .map(|c| c.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "bot".into())
}

#[cfg(test)]
mod tests {
    use super::{is_image, join_names, missing_note, starts_token};
    use crate::opengrok::RecipeSummary;
    use crate::state::ActiveRecipe;
    use std::path::PathBuf;

    #[test]
    fn a_message_is_refused_by_the_name_of_what_is_missing() {
        let recipe: RecipeSummary = serde_json::from_value(serde_json::json!({
            "id": "rcp_1",
            "name": "youtube",
            "parameters": [
                { "name": "search_term", "required": true, "kind": "text" },
                { "name": "channel", "required": true, "kind": "text" },
                { "name": "count", "required": false, "kind": "number" }
            ]
        }))
        .unwrap();
        let mut recipe = ActiveRecipe::from_summary(&recipe);
        assert_eq!(
            missing_note(&recipe).as_deref(),
            Some("youtube needs search_term and channel before this can be sent."),
            "a refusal that does not name the parameter leaves someone hunting for it"
        );
        recipe.set_value("search_term", Some("mundo".to_string()));
        assert_eq!(
            missing_note(&recipe).as_deref(),
            Some("youtube needs channel before this can be sent.")
        );
        recipe.set_value("channel", Some("anything".to_string()));
        assert_eq!(
            missing_note(&recipe),
            None,
            "nothing required is missing, so the message goes; count was never required"
        );
    }

    #[test]
    fn names_are_joined_the_way_they_are_said() {
        assert_eq!(join_names(&["a"]), "a");
        assert_eq!(join_names(&["a", "b"]), "a and b");
        assert_eq!(join_names(&["a", "b", "c"]), "a, b and c");
    }

    #[test]
    fn a_trigger_only_counts_at_the_start_of_a_token() {
        assert!(starts_token("", 0), "the start of an empty message");
        assert!(starts_token("ask ", 4), "right after a space");
        assert!(starts_token("one\n", 4), "right after a newline");
        assert!(
            !starts_token("me", 2),
            "mid-word, which is where an email address would open the panel"
        );
        assert!(!starts_token("path/to", 7), "mid-word for a path too");
    }

    #[test]
    fn only_image_files_are_attached() {
        for name in ["shot.PNG", "a.jpg", "b.jpeg", "c.webp", "d.gif"] {
            assert!(is_image(&PathBuf::from(name)), "{name} is an image");
        }
        for name in ["notes.pdf", "clip.mov", "noextension"] {
            assert!(!is_image(&PathBuf::from(name)), "{name} is not an image");
        }
    }
}
