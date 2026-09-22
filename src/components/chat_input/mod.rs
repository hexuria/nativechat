// Public because the agent host rebuilds the open panel's rows from the very same sources, so
// the id it hands a driver is the id the row on screen answers to.
pub mod sources;
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
use sources::{ParameterSource, SkillLibrary, SlashSource, ToolSource, ValueSource};
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

actions!(
    chat,
    [
        SubmitMessage,
        /// Send the draft, from the field or from the panel standing over it. See
        /// `MessageInput::send_draft` for why the send needs an action of its own.
        SendDraft,
        /// ⌘⇧↵: send the draft now, even over a running turn. The turn is stopped at its
        /// next step and this message goes ahead of anything queued behind it.
        SendDraftSteer
    ]
);

/// The text, and whether the person asked for it to go now (⌘⇧↵) rather than queue.
type SubmitCallback = Box<dyn Fn(String, bool, &mut Context<MessageInput>)>;

/// The images that may be attached. The picker itself cannot be told to show only these — GPUI's
/// path prompt has no type filter — so the list is applied to what comes back.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

/// Which list the open panel is showing, and therefore what a picked row means.
///
/// Public because [`AppState`] keeps a copy of it: the panel itself belongs to this view, and
/// anything outside the view — the agent host, which is built from the state alone — can only
/// tell that `/` opened something if the state says so.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelMode {
    /// The "+" button: attach files, teach a task.
    Plus,
    /// `@` with no recipe on the draft: the bot's tools and apps.
    Tools,
    /// `/`: the recipes, workflows and skills the bot can be pointed at, and the app's own
    /// actions.
    Slash,
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
    /// What the chip reads as in the message: the recipe's name, without the `/` that opened
    /// the panel, because that was how it was asked for and not part of what is being said.
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
    /// The message as it stood when the chips were last put where they are.
    ///
    /// Every edit is read as the difference between this and what the field holds now. It is
    /// the only way to tell a chip that MOVED from one that was DELETED while the same words
    /// sit somewhere else in the draft — see [`remap_tokens`].
    last_text: String,
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
    /// The open thread has a turn in flight, which is what turns the send button into a stop
    /// button. Cached off [`AppState`] like the rest, so the composer draws without reading the
    /// state on every frame.
    turn_in_flight: bool,
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
        let turn_in_flight = app_state.is_turn_in_flight();

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
            last_text: String::new(),
            active_recipe,
            attachments: Vec::new(),
            notice: None,
            dismissed_at: None,
            turn_in_flight,
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
                // Not a field, so not one of the macros: whether a turn is in flight is a
                // question about the open thread's live turn, which only the state can answer.
                let running = state.is_turn_in_flight();
                if this.turn_in_flight != running {
                    this.turn_in_flight = running;
                    changed = true;
                }
            }

            // Recipes and skills that were still being fetched when `/` opened the panel land
            // here, each as it arrives: they are two listings from two routes, and the panel
            // fills in twice rather than waiting for the slower of them.
            if this.panel_mode == Some(PanelMode::Slash) {
                let mut rows = {
                    let state = state.read(cx);
                    SlashSource.rows(&state.recipes, &your_skills(state))
                };
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
                        this.trigger_submit(false, window, cx);
                    }
                }
                InputEvent::Change => {
                    this.resync_tokens(cx);
                    this.drop_recipe_without_its_chip(window, cx);
                    this.drop_skill_without_its_chip(cx);
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

    pub fn on_submit(
        mut self,
        handler: impl Fn(String, bool, &mut Context<Self>) + 'static,
    ) -> Self {
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

    /// The send chord, from wherever the caret is.
    ///
    /// ↵ and ⌘↵ already reach the field as [`InputEvent::PressEnter`], and which of them sends
    /// is the person's own setting. Neither reaches the composer while the panel is open: the
    /// panel holds the focus, and inside it ↵ fills the highlighted parameter in — which is
    /// what it should do, and is why the send needs a chord of its own rather than a share of
    /// that one. A key listener would be too late, because GPUI dispatches a keybinding's
    /// action before any listener sees the key, so [`SendDraft`] is bound to ⌘↵ in `main.rs`
    /// — in the composer's context and in the panel's — and answered here.
    ///
    /// The panel goes first. Sending is a decision about the message, not about the list, and
    /// leaving a picker standing over a composer that has just emptied reads as if nothing
    /// happened.
    fn send_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_panel(true, window, cx);
        self.trigger_submit(false, window, cx);
    }

    fn send_draft_steer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_panel(true, window, cx);
        self.trigger_submit(true, window, cx);
    }

    fn trigger_submit(&mut self, steer: bool, window: &mut Window, cx: &mut Context<Self>) {
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
                (handler)(trimmed.to_string(), steer, cx);
            }
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
            self.tokens.clear();
            // `set_value` above emits no Change, so this is the only thing that puts the
            // difference on record: without it the next keystroke would be read against the
            // message that has just gone.
            self.remember_text(cx);
            // The recipe belonged to the message that has just gone, not to the next one.
            self.state
                .update(cx, |state, cx| state.clear_active_recipe(cx));
            // The skill did too. The draft's own door takes it as the turn is built (see
            // `AppState::send_draft`), so this is already done in the ordinary case; it is here
            // for the one where nothing was sent at all — an empty draft, or a handler that is
            // not wired — which would otherwise leave a skill on a draft whose chip has just
            // been cleared away.
            self.state
                .update(cx, |state, cx| state.clear_active_skill(cx));
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
        // What was in the message went with it. `set_value` writes the field behind its own
        // back — no Change event — so nothing else here would have noticed: the chips kept the
        // ranges they had, which are now bytes of the transcription, and the fills painted over
        // somebody's dictated words while the skill behind them still rode out on the turn.
        self.tokens.clear();
        self.remember_text(cx);
        self.drop_recipe_without_its_chip(window, cx);
        self.drop_skill_without_its_chip(cx);
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
        // Say on the state which list is open. The panel is drawn from this view and nothing
        // outside it can read the view, so this is the only place an agent driver — which sees
        // the state and nothing else — can learn that a `/` did anything.
        self.state
            .update(cx, |state, cx| state.set_composer_panel(Some(mode), cx));
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
        self.state
            .update(cx, |state, cx| state.set_composer_panel(None, cx));
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
                    // Not "as a recipe" any more: stopping the tape asks which of three things
                    // to make of it, and only one of the three is a recipe.
                    "Show the bot on its screen, and keep what it saw",
                )
                .element_id("composer-teach"),
                ComposerPick::TeachTask,
            ),
        ];
        self.show_panel(
            PanelMode::Plus,
            rows,
            "Search",
            "⌘1–9 picks a row. Type @ for the bot's tools, / for its recipes, workflows and \
             skills.",
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

    fn open_slash_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // An empty list may only mean the recipes have never been fetched in this session; ask
        // for them, and the observer above fills the open panel when they land. The skills are
        // a second route and so a second ask, on the same rule.
        if self.state.read(cx).recipes.is_empty() {
            self.state.update(cx, |state, cx| state.refresh_recipes(cx));
        }
        // The skills are asked for every time, not only when the list is empty. One `/` is one
        // small request, a library changes while the app is open — somebody writes one on
        // another machine, or a colleague shares one — and a list that was only ever fetched
        // once is a list that is wrong for the rest of the session. What has already arrived
        // stays on screen while the answer is on its way, so nothing flickers.
        self.state
            .update(cx, |state, cx| state.refresh_your_skills(cx));
        let mut rows = {
            let state = self.state.read(cx);
            SlashSource.rows(&state.recipes, &your_skills(state))
        };
        apply_shortcuts(&mut rows, window);
        self.show_panel(
            PanelMode::Slash,
            rows,
            "Search recipes, workflows, skills and actions",
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
            // something ABOUT the message, and the message should not have to carry it. A
            // recipe is the opposite — it reads as part of the sentence, so it goes in at the
            // caret.
            ComposerPick::Token { kind, id, text } => match kind {
                TokenKind::Tool => {
                    let label = text.trim_start_matches('@').to_string();
                    self.state
                        .update(cx, |state, cx| state.pick_tool(id, label, cx));
                }
                // A workflow goes the same way a recipe does, and on purpose: both are what the
                // turn runs, both declare their parameters on the same field of the same row,
                // and a second path through here would be the first one copied with one word
                // changed. What the two differ in is what they are called, which is on the chip
                // and on the bar.
                TokenKind::Recipe | TokenKind::Workflow => {
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
                    // A recipe that cannot run until it is told something asks now, rather
                    // than leaving it behind an `@` nobody has been told about: picking it is
                    // the one moment the person is certainly thinking about this recipe, and
                    // a bar reading "needed" beside a name is a puzzle, not an instruction.
                    if self
                        .state
                        .read(cx)
                        .active_recipe
                        .as_ref()
                        .is_some_and(opens_on_pick)
                    {
                        self.open_parameters_panel(window, cx);
                    }
                }
                // A skill is prose the bot reads before it works, so it is not the message's
                // mode: no bar, no parameters, nothing to be told. The chip is the whole of
                // what it looks like, and the id behind the chip is what the turn carries.
                TokenKind::Skill => {
                    // Picking the one that is already there again is nothing happening. It is
                    // already attached and its chip is already in the message, and a second
                    // word standing for the same skill is one the person would have to delete
                    // twice to be rid of it.
                    if skill_chip(&self.tokens, &id).is_some() {
                        return;
                    }
                    let held = self
                        .state
                        .read(cx)
                        .active_skill
                        .as_ref()
                        .map(|skill| skill.id.clone());
                    // The pick is resolved before anything is taken away. `start_skill` refuses
                    // an id the library no longer holds — which the refresh on every `/` makes
                    // reachable, with a listing landing between the rows being built and the
                    // Enter that takes one — and a draft that had already been emptied for it
                    // would leave the person with no skill, no chip, and nothing said about it.
                    if !self
                        .state
                        .update(cx, |state, cx| state.start_skill(&id, cx))
                    {
                        return;
                    }
                    // One skill to a message, because the turn names one id: the one that was
                    // there goes, chip and all, rather than leaving a word in the message
                    // standing for a skill that is not going anywhere.
                    if let Some(held) = held {
                        self.remove_skill_chip(&held, window, cx);
                    }
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
    /// both there, so undoing it undoes both. This is what the `×` on the bar does.
    ///
    /// [`Self::drop_recipe_without_its_chip`] is the same rule read the other way round.
    fn drop_recipe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(recipe) = self.state.read(cx).active_recipe.clone() else {
            return;
        };
        self.state
            .update(cx, |state, cx| state.clear_active_recipe(cx));
        if let Some(index) = self
            .tokens
            .iter()
            .position(|token| token.kind.is_mode() && token.id == recipe.id)
        {
            let range = self.tokens.remove(index).range;
            self.remove_text(range, window, cx);
        }
        cx.notify();
    }

    /// Take the recipe off the draft once its chip is no longer in the message, whether it was
    /// backspaced over or typed away a letter at a time.
    ///
    /// This reverses what the recipe mode was first built with. The rule then was that editing
    /// the chip away left the recipe running, on the reasoning that a mode set on purpose
    /// should not fall off because of a keystroke in the text. In use that turned out to be
    /// the worse half of the bargain: deleting the chip left a bar still demanding
    /// `search_term`, with nothing anywhere in the message to say where that demand came from
    /// or how to be rid of it. The rule is symmetric now — one pick puts the recipe and its
    /// chip there, and losing either takes both away — which is the reading a person arrives
    /// at on their own, and the `×` on the bar still goes the other way round.
    ///
    /// Everything the recipe was told goes with it, because the values live inside the recipe
    /// and nothing outside it remembers them; picking the same recipe again starts empty
    /// rather than resuming a run that was abandoned.
    fn drop_recipe_without_its_chip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .state
            .read(cx)
            .active_recipe
            .as_ref()
            .map(|recipe| recipe.id.clone())
        else {
            return;
        };
        if self
            .tokens
            .iter()
            .any(|token| token.kind.is_mode() && token.id == id)
        {
            return;
        }
        self.state
            .update(cx, |state, cx| state.clear_active_recipe(cx));
        // A list of what to tell a recipe that is no longer on the draft is a list about
        // nothing. The parameter list closes itself when the recipe goes (see the observer in
        // `new`); the value panel under it has to be told.
        if matches!(self.panel_mode, Some(PanelMode::Value { .. })) {
            self.close_panel(false, window, cx);
        }
        cx.notify();
    }

    /// Take one skill's chip out of the message, with the space that went in beside it.
    ///
    /// Only the draft's own state says which skill is attached, and that is not touched here:
    /// the one caller replaces the attachment first and then clears away the word standing for
    /// what it replaced. There is no `×` to press either, because a skill has no bar, having
    /// nothing to be told and nothing to show.
    fn remove_skill_chip(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = skill_chip(&self.tokens, id) else {
            return;
        };
        let range = self.tokens.remove(index).range;
        self.remove_text(range, window, cx);
        cx.notify();
    }

    /// Take the skill off the draft once its chip is no longer in the message, whether it was
    /// backspaced over or typed away a letter at a time.
    ///
    /// The same rule a recipe follows ([`Self::drop_recipe_without_its_chip`]) and for a
    /// stronger reason: the chip is the only place the app says this message carries a skill. A
    /// draft that kept the skill after the word was deleted would be sending something with the
    /// message that nothing on screen mentions.
    ///
    /// One hole in that, older than skills and not closed here: a chip whose words wrap across a
    /// line is painted with no fill at all (see [`Self::chip_fills`], which has no one rectangle
    /// to put there), so a skill whose name straddles a wrap rides out with nothing drawn around
    /// it. The word is still in the message and the chip is still a chip — it is the paint that
    /// is missing, not the attachment — but the argument above is weaker there than everywhere
    /// else, and it is the fill that would have to learn to be two rectangles.
    fn drop_skill_without_its_chip(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .state
            .read(cx)
            .active_skill
            .as_ref()
            .map(|skill| skill.id.clone())
        else {
            return;
        };
        if skill_chip(&self.tokens, &id).is_some() {
            return;
        }
        self.state
            .update(cx, |state, cx| state.clear_active_skill(cx));
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
        // The chips already in the message slide along by what was put in front of them. This
        // is an edit the view made itself, so its shape is known exactly and nothing has to be
        // guessed from the words — which is what kept a chip from landing on somebody's prose
        // that happened to read the same.
        shift_tokens(&mut self.tokens, caret..caret, text.len() + 1);
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
        self.remember_text(cx);
        self.focus(window, cx);
        cx.notify();
    }

    /// Move every chip through whatever was just done to the message.
    ///
    /// The field knows nothing about chips and reports no edit, so the edit is worked out as
    /// the difference between the message as it was and the message as it is.
    ///
    /// This used to look for each chip's words in the text instead, taking the first occurrence
    /// after the chip before it. That reads a chip as a WORD rather than as a PLACE, and the two
    /// come apart exactly where it matters: delete the chip while the same words sit further
    /// down the draft and the chip simply moves onto them, so the message still carries the
    /// skill after the only thing on screen that said so is gone.
    fn resync_tokens(&mut self, cx: &App) {
        let (text, caret) = {
            let input = self.input_state.read(cx);
            (input.value().to_string(), input.cursor())
        };
        if text == self.last_text {
            return;
        }
        remap_tokens(&mut self.tokens, &self.last_text, &text, caret);
        self.last_text = text;
    }

    /// Remember the message as it now stands, so the next edit is read against it.
    fn remember_text(&mut self, cx: &App) {
        self.last_text = self.input_state.read(cx).value().to_string();
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
            input.set_selected_range(range.clone(), cx);
            input.replace("", window, cx);
        });
        // Its real shape, like every other edit this view makes: the guess from the two texts
        // is for the edits only the person knows about.
        shift_tokens(&mut self.tokens, range.clone(), 0);
        self.caret = caret_after_cut(self.caret, range);
        self.remember_text(cx);
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
        shift_tokens(&mut self.tokens, range.start..end, 0);
        // The caret the open panel will put its chip at moves with everything else after the
        // cut. Picking a second skill takes the first one's chip out and then puts the new one
        // in, and a caret left where it was would land the new chip that many characters into
        // whatever follows.
        self.caret = caret_after_cut(self.caret, range.start..end);
        self.remember_text(cx);
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
            Some("/") => PanelMode::Slash,
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
            PanelMode::Slash => self.open_slash_panel(window, cx),
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
                                        .path(recipe.kind.icon())
                                        .size(px(11.))
                                        .text_color(muted_foreground),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                                        .text_color(muted_foreground)
                                        .truncate()
                                        // The noun is the thing's own, never "Recipe" for both:
                                        // the bar is the one place that says what the next
                                        // message runs, and a tree and a tape are not the same
                                        // promise.
                                        .child(format!(
                                            "{} · {}",
                                            recipe.kind.label(),
                                            recipe.name
                                        )),
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
                        .tooltip({
                            let drop =
                                SharedString::from(format!("Drop the {}", recipe.kind.word()));
                            move |window, cx| Tooltip::new(drop.clone()).build(window, cx)
                        })
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

/// Whether picking this recipe should open the list of what it needs there and then, instead of
/// waiting for an `@` nobody has said is coming: it declares something required that nobody has
/// filled in, so it cannot run as it stands. A recipe that declares nothing, and one whose
/// required parameters all arrived with defaults, has nothing to ask about and opens nothing —
/// a panel over a composer that was ready to send is a step backwards.
fn opens_on_pick(recipe: &ActiveRecipe) -> bool {
    !recipe.missing().is_empty()
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

/// The chord that sends the draft, spelled as the keyboard shows it. It is written out here
/// rather than read back from the keymap because it stands in a line of prose beside ↵ and esc,
/// which are written the same way; the binding it names is [`SendDraft`], in `main.rs`.
const SEND_CHORD: &str = "⌘↵";

/// The line under the list of parameters: what is still stopping the message, or — once nothing
/// is — that it can go, and on which keys.
///
/// The send is worth naming here and nowhere else in the panel. Someone who has just filled in
/// the last thing a recipe needed is done, and the panel they are looking at has no row left
/// that says so; being told the keys beats closing the panel to find out whether it worked.
fn parameters_hint(recipe: &ActiveRecipe) -> String {
    match missing_note(recipe) {
        Some(note) => format!("{note} ↵ fills one in, esc closes."),
        None if recipe.unfilled().is_empty() => {
            format!("{SEND_CHORD} sends the message, esc closes.")
        }
        None => format!(
            "The rest is optional — {SEND_CHORD} sends the message. ↵ fills one in, esc closes."
        ),
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

/// The library `/` offers, as the state has it: this person's skills, and how that listing is
/// getting on. Never [`AppState::skills`], which is whichever side of the Settings toggle was
/// last looked at.
fn your_skills(state: &AppState) -> SkillLibrary<'_> {
    SkillLibrary {
        skills: &state.your_skills,
        loading: state.your_skills_loading,
        error: state.your_skills_error.as_deref(),
    }
}

/// Where the chip for one skill sits in the message, if it is still there.
///
/// The chip is the whole of what the app says about a skill on the draft, so this is the
/// question behind both halves of the rule: whether a pick has anything left to do, and whether
/// an attachment still has something on screen standing for it.
fn skill_chip(tokens: &[ComposerToken], id: &str) -> Option<usize> {
    tokens
        .iter()
        .position(|token| token.kind == TokenKind::Skill && token.id == id)
}

/// Where the caret sits once the bytes in `cut` have been taken out: back by as much as was
/// removed before it, and at the cut itself when it was inside what went.
fn caret_after_cut(caret: usize, cut: Range<usize>) -> usize {
    if caret <= cut.start {
        return caret;
    }
    if caret >= cut.end {
        return caret - (cut.end - cut.start);
    }
    cut.start
}

/// Move the chips through one edit whose shape is known: the bytes in `edited` — where they
/// were before the edit — became `now` bytes.
///
/// A chip wholly before the edit stays where it is, a chip wholly after it slides, and a chip
/// the edit ran into stops being a chip: the words the person picked are no longer the words
/// that are there, whatever else the message may say elsewhere.
fn shift_tokens(tokens: &mut Vec<ComposerToken>, edited: Range<usize>, now: usize) {
    let delta = now as isize - (edited.end - edited.start) as isize;
    let slide = |at: usize| (at as isize + delta).max(0) as usize;
    tokens.retain_mut(|token| {
        if token.range.end <= edited.start {
            return true;
        }
        if token.range.start >= edited.end {
            token.range = slide(token.range.start)..slide(token.range.end);
            return true;
        }
        false
    });
}

/// Move the chips through an edit nobody described, by reading it off the two texts and the
/// caret.
///
/// The caret is what settles it. Comparing the texts alone cannot tell deleting the first of two
/// identical words from deleting the second, and picking either answer is wrong half the time:
/// the words a person deleted are the words in front of the caret, so the edit is taken to END
/// there and the texts are only asked where it began. Select `expense-report ` — a chip, and the
/// space that came with it — while the same word sits further down the draft, and without the
/// caret the deletion is read as the later copy's and the chip lives on over somebody's prose,
/// with the skill still attached to a message that no longer names it.
///
/// A caret that is not where the edit was cannot make this unsafe: every range here is clamped
/// into the text, nothing is sliced, and each surviving chip moves by the difference in length
/// between the two texts — so a chip's range stays on the character boundaries it was already
/// on, whatever this decides about which chips survive.
fn remap_tokens(tokens: &mut Vec<ComposerToken>, before: &str, after: &str, caret: usize) {
    // Where the edit ended, in each text. The second is the first read backwards through the
    // change in length, which is what makes the two ends describe one contiguous edit.
    let grew = after.len() as isize - before.len() as isize;
    let ends_after = caret.min(after.len());
    let ends_before = (ends_after as isize - grew).clamp(0, before.len() as isize) as usize;
    // And where it began: as far in as the two texts agree, but never past either end.
    let head = common_head(before, after).min(ends_after).min(ends_before);
    shift_tokens(tokens, head..ends_before, ends_after - head);
}

/// How many bytes two texts begin with in common, never splitting a character in half.
fn common_head(before: &str, after: &str) -> usize {
    let mut at = 0;
    let (a, b) = (before.as_bytes(), after.as_bytes());
    while at < a.len().min(b.len()) && a[at] == b[at] {
        at += 1;
    }
    while at > 0 && !(before.is_char_boundary(at) && after.is_char_boundary(at)) {
        at -= 1;
    }
    at
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
                // ⌘↵ sends, from the field or from the panel. This listener sits outside both
                // on purpose: the panel is a deferred child of this element, so an action
                // dispatched while it holds the focus bubbles out through here.
                .on_action(cx.listener(|this, _: &SendDraft, window, cx| {
                    this.send_draft(window, cx);
                }))
                .on_action(cx.listener(|this, _: &SendDraftSteer, window, cx| {
                    this.send_draft_steer(window, cx);
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
                                        if self.turn_in_flight {
                                            // The same round button in the same place, so the
                                            // composer does not move under the hand that is
                                            // about to press it. The square is drawn rather than
                                            // brought in as an icon: it is a square.
                                            let mut stop_btn = div()
                                                .id("stop-btn")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.state.update(cx, |state, cx| {
                                                        state.stop_turn(cx);
                                                    });
                                                }))
                                                .w(px(36.0))
                                                .h(px(36.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .bg(foreground)
                                                .text_color(background)
                                                .hover(move |style| {
                                                    style.bg(foreground.opacity(0.8))
                                                })
                                                .tooltip(|w, cx| {
                                                    Tooltip::new("Stop").build(w, cx)
                                                })
                                                .child(
                                                    div()
                                                        .w(px(11.0))
                                                        .h(px(11.0))
                                                        .rounded(px(2.0))
                                                        .bg(background),
                                                );

                                            if !any_modal_open {
                                                stop_btn = stop_btn.cursor_pointer();
                                            }

                                            stop_btn
                                        } else if self.input_state.read(cx).text().len() == 0 {
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
                                                    this.trigger_submit(false, window, cx);
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
    use super::{
        ComposerToken, TokenKind, is_image, join_names, missing_note, opens_on_pick,
        parameters_hint, remap_tokens, shift_tokens, skill_chip, starts_token,
    };
    use crate::opengrok::RecipeSummary;
    use crate::state::ActiveRecipe;
    use std::path::PathBuf;

    /// A chip for a skill, where the words sit in the message.
    fn chip(id: &str, text: &str, at: usize) -> ComposerToken {
        ComposerToken {
            kind: TokenKind::Skill,
            id: id.to_string(),
            text: text.to_string(),
            range: at..at + text.len(),
        }
    }

    fn picked(declaration: serde_json::Value) -> ActiveRecipe {
        let recipe: RecipeSummary = serde_json::from_value(declaration).unwrap();
        ActiveRecipe::from_summary(&recipe)
    }

    #[test]
    fn a_recipe_that_cannot_run_as_it_stands_opens_its_list_on_the_pick() {
        assert!(
            opens_on_pick(&picked(serde_json::json!({
                "id": "rcp_1", "name": "youtube",
                "parameters": [{ "name": "search_term", "required": true, "kind": "text" }]
            }))),
            "there is a required parameter with nothing in it, so the pick has left the \
             message unsendable and the list is what to do about that"
        );
        assert!(
            !opens_on_pick(&picked(serde_json::json!({
                "id": "rcp_2", "name": "digest",
                "parameters": [
                    { "name": "since", "required": true, "kind": "text", "default": "monday" },
                    { "name": "tone", "required": false, "kind": "text" }
                ]
            }))),
            "every required parameter came with a default standing in its field, so the \
             recipe would run as picked and a panel over it is a step backwards"
        );
        assert!(
            !opens_on_pick(&picked(
                serde_json::json!({ "id": "rcp_3", "name": "Mail" })
            )),
            "a recipe that declares nothing has nothing to ask about"
        );
    }

    #[test]
    fn the_hint_names_the_send_chord_once_nothing_is_stopping_the_message() {
        let mut recipe = picked(serde_json::json!({
            "id": "rcp_1", "name": "youtube",
            "parameters": [
                { "name": "search_term", "required": true, "kind": "text" },
                { "name": "count", "required": false, "kind": "number" }
            ]
        }));
        assert!(
            !parameters_hint(&recipe).contains(super::SEND_CHORD),
            "a chord that will only be refused is not worth offering while something \
             required is still missing"
        );

        recipe.set_value("search_term", Some("mundo".to_string()));
        assert!(
            parameters_hint(&recipe).contains(super::SEND_CHORD),
            "nothing stops the message now, and the optional one left does not"
        );

        recipe.set_value("count", Some("5".to_string()));
        let hint = parameters_hint(&recipe);
        assert!(
            hint.contains(super::SEND_CHORD),
            "the empty state's only row says there is nothing to do, so the line under it \
             has to say what to do instead, and it read {hint:?}"
        );
    }

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

    /// The bug this rule was rewritten for. A chip is a PLACE in the message, not a word that
    /// happens to be in it: delete the chip while the same words sit further down the draft and
    /// the chip must go, because the only thing on screen saying the message carries that skill
    /// has gone. Reading it as a word moved the chip onto the other copy and sent the skill
    /// anyway.
    #[test]
    fn a_deleted_chip_does_not_move_onto_words_that_read_the_same() {
        let mut tokens = vec![chip("skl_1", "expense-report", 0)];
        remap_tokens(
            &mut tokens,
            "expense-report and expense-report",
            " and expense-report",
            // The caret is where the words were taken from, which is the start of the message.
            0,
        );
        assert!(tokens.is_empty(), "the chip was deleted, so it is gone");
        assert_eq!(
            skill_chip(&tokens, "skl_1"),
            None,
            "which is what takes the skill off the draft with it"
        );
    }

    /// The same deletion, in the shape a person actually makes it: the chip and the space that
    /// came with it, taken out in one go by a double-click-drag or by option-backspace, with the
    /// very same word typed out further down the draft.
    ///
    /// The two texts alone cannot say which copy went — and the answer they give without the
    /// caret is the wrong one, because the longest common start runs through the copy that is
    /// left. What settles it is that the words a person deleted are the words in front of the
    /// caret.
    #[test]
    fn deleting_a_chip_and_its_space_is_read_as_the_copy_the_caret_is_at() {
        let mut tokens = vec![chip("skl_1", "expense-report", 0)];
        remap_tokens(
            &mut tokens,
            "expense-report expense-report",
            "expense-report",
            0,
        );
        assert!(
            tokens.is_empty(),
            "the chip and its space went, so the chip is gone and the skill goes with it"
        );

        // And the other way round: the typed copy deleted, the chip untouched.
        let mut tokens = vec![chip("skl_1", "expense-report", 0)];
        remap_tokens(
            &mut tokens,
            "expense-report expense-report",
            "expense-report",
            14,
        );
        assert_eq!(
            tokens[0].range,
            0..14,
            "the caret was at the end of the chip, so what went was everything after it"
        );
    }

    /// Typing around a chip moves it; typing into it ends it.
    #[test]
    fn a_chip_slides_past_an_edit_before_it_and_dies_inside_one() {
        let said = "hi expense-report ok";
        let mut tokens = vec![chip("skl_1", "expense-report", 3)];
        remap_tokens(&mut tokens, said, "oh hi expense-report ok", 3);
        assert_eq!(
            tokens[0].range,
            6..20,
            "three more bytes went in front of it"
        );

        let mut tokens = vec![chip("skl_1", "expense-report", 3)];
        remap_tokens(&mut tokens, said, "hi expense-report ok!", 21);
        assert_eq!(
            tokens[0].range,
            3..17,
            "what was typed after it is nothing to do with it"
        );

        let mut tokens = vec![chip("skl_1", "expense-report", 3)];
        remap_tokens(&mut tokens, said, "hi expense ok", 10);
        assert!(
            tokens.is_empty(),
            "half the name is not the name: the words the person picked are not there any more"
        );

        // Two chips, and an edit between them: the first stays, the second slides.
        let mut tokens = vec![chip("skl_1", "alpha", 0), chip("skl_2", "beta", 6)];
        remap_tokens(&mut tokens, "alpha beta", "alpha and beta", 10);
        assert_eq!(tokens[0].range, 0..5);
        assert_eq!(tokens[1].range, 10..14);
    }

    /// A letter typed hard up against the front of a chip belongs to the letter, not to the
    /// chip: the chip is pushed along rather than ended. The texts alone cannot see that — the
    /// new letter reads as part of the word — and the caret can.
    #[test]
    fn typing_right_in_front_of_a_chip_pushes_it_along() {
        let mut tokens = vec![chip("skl_1", "expense-report", 0)];
        remap_tokens(&mut tokens, "expense-report", "eexpense-report", 1);
        assert_eq!(tokens[0].range, 1..15);

        // And a caret nowhere near the edit — which is not a thing the field does, but is what
        // an odd one would look like — still leaves every surviving chip on its own words.
        let mut tokens = vec![chip("skl_1", "expense-report", 0)];
        remap_tokens(&mut tokens, "expense-report", "expense-report!", 900);
        assert!(tokens.iter().all(|token| token.range.end <= 15));
    }

    /// The caret the panel puts its chip at moves with the text: picking a second skill takes
    /// the first one's chip out from under it.
    #[test]
    fn the_caret_comes_back_by_what_was_taken_out_in_front_of_it() {
        // "expense-report " out of "expense-report hello", with the caret at the end.
        assert_eq!(super::caret_after_cut(20, 0..15), 5);
        assert_eq!(super::caret_after_cut(0, 5..9), 0, "it sat before the cut");
        assert_eq!(
            super::caret_after_cut(7, 5..9),
            5,
            "it sat inside what went, so it is where that was"
        );
    }

    /// The property under all of the above, over a few thousand edits: a chip that survives one
    /// covers its own words and nobody else's.
    ///
    /// Written down here because the caret went into the rule after it had been fuzzed once
    /// already, and a rule this quiet is one nobody re-reads. The edits are the shapes a field
    /// makes — one stretch put in or taken out, with the caret left at the end of it — over an
    /// alphabet with characters of one, two and four bytes in it, so a boundary walked past
    /// would show up as a panic rather than as a silent nothing.
    #[test]
    fn a_surviving_chip_always_covers_its_own_words() {
        const WORDS: [&str; 6] = ["a", " ", "é", "😀", "expense-report", "ok"];
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut roll = move |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % bound.max(1) as u64) as usize
        };

        for _ in 0..3_000 {
            // A draft with two chips in it, and prose between and around them.
            let mut before = String::new();
            let mut tokens: Vec<ComposerToken> = Vec::new();
            for id in ["skl_1", "skl_2"] {
                for _ in 0..roll(3) {
                    before.push_str(WORDS[roll(WORDS.len())]);
                }
                let at = before.len();
                before.push_str(WORDS[4]);
                before.push(' ');
                tokens.push(chip(id, WORDS[4], at));
            }
            for _ in 0..roll(3) {
                before.push_str(WORDS[roll(WORDS.len())]);
            }

            // One edit, of the shape a field makes, with the caret where it left off.
            let boundary = |text: &str, at: usize| {
                let mut at = at.min(text.len());
                while !text.is_char_boundary(at) {
                    at -= 1;
                }
                at
            };
            let start = boundary(&before, roll(before.len() + 1));
            let (after, caret) = if roll(2) == 0 {
                let end = boundary(&before, start + roll(before.len() - start + 1));
                (format!("{}{}", &before[..start], &before[end..]), start)
            } else {
                let put = WORDS[roll(WORDS.len())];
                (
                    format!("{}{put}{}", &before[..start], &before[start..]),
                    start + put.len(),
                )
            };

            remap_tokens(&mut tokens, &before, &after, caret);
            for token in &tokens {
                assert_eq!(
                    after.get(token.range.clone()),
                    Some(token.text.as_str()),
                    "a chip that survived {before:?} becoming {after:?} at {caret} is sitting on \
                     {:?}",
                    after.get(token.range.clone())
                );
            }
        }
    }

    /// An edit this view makes itself is handed over with its real shape rather than guessed at
    /// from the words, which is why putting a chip in cannot land it on prose that reads the
    /// same: the new chip goes at the caret and everything after the caret slides.
    #[test]
    fn putting_a_chip_in_slides_the_ones_after_it_and_leaves_the_ones_before() {
        let mut tokens = vec![chip("skl_1", "alpha", 0), chip("skl_2", "beta", 6)];
        // "gamma " goes in at 6, where `beta` starts.
        shift_tokens(&mut tokens, 6..6, 6);
        assert_eq!(tokens[0].range, 0..5, "it sits before the caret");
        assert_eq!(tokens[1].range, 12..16, "and this one was pushed along");

        // Taking one out again brings the rest back.
        shift_tokens(&mut tokens, 6..12, 0);
        assert_eq!(tokens[1].range, 6..10);
    }

    /// Picking the skill that is already on the draft is nothing happening: the chip is already
    /// in the message, and a second word for the same skill is one to delete twice.
    #[test]
    fn a_skill_already_in_the_message_is_found_before_a_second_chip_goes_in() {
        let tokens = vec![
            ComposerToken {
                kind: TokenKind::Recipe,
                id: "rcp_1".into(),
                text: "Weekly report".into(),
                range: 0..13,
            },
            chip("skl_1", "expense-report", 14),
        ];
        assert_eq!(skill_chip(&tokens, "skl_1"), Some(1));
        assert_eq!(
            skill_chip(&tokens, "rcp_1"),
            None,
            "a recipe's chip is not a skill's, whatever the id says"
        );
        assert_eq!(skill_chip(&tokens, "skl_2"), None);
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
