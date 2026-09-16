mod items;
mod sources;
#[macro_use]
mod sync_macros;

pub use items::{render_flyout_item, render_popover_item};
pub use sources::{AppCommand, ComposerPick, TokenKind};

use crate::actions::{Library, NewChat, OpenSettings, Projects, ToggleTheme};
use crate::audio::AudioInput;
use crate::components::composer_panel::{ComposerPanel, ComposerPanelEvent, ComposerPanelRow};
use crate::components::voice_wave::VoiceWave;
use crate::icons::NativeIcon;
use crate::state::{AppState, ReplyTo, SubmitChord};
use sources::{SkillSource, ToolSource};
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
    /// `@`: the bot's tools and apps.
    Tools,
    /// `/`: recipes and the app's own commands.
    Skills,
}

/// A chip in the message: what it stands for, and where its text sits.
///
/// The chip is text in the field, because the text field cannot host an element mid-line (see
/// `MessageInput::chip_pills`). This is what makes it more than text: the kind and the id are
/// kept beside it so the message can be sent as structured data rather than re-parsed out of a
/// string that anyone could have typed by hand.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposerToken {
    pub kind: TokenKind,
    /// What the thing is called where it lives: a tool's name, a recipe's id.
    pub id: String,
    /// What the chip reads as in the message, `@shell`.
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
    selected_apps: Vec<String>,
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
        let selected_apps = app_state.selected_apps.clone();
        let is_voice_mode_open = app_state.is_voice_mode_open;
        let is_app_settings_open = app_state.is_app_settings_open;
        let submit_chord = app_state.submit_chord;
        let reply_to = app_state.reply_to.clone();
        let coworker_name = composer_bot_name(&app_state);

        let this = Self {
            state: state.clone(),
            input_state: input_state.clone(),
            selected_apps,
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
            attachments: Vec::new(),
            notice: None,
            dismissed_at: None,
        };

        // Subscribe to state changes to update cached values and notify only when relevant fields change
        cx.observe(&state, |this: &mut Self, state, cx| {
            let mut changed = false;
            {
                let state = state.read(cx);
                sync_field_clone!(this, state, selected_apps, changed);
                sync_field_copy!(this, state, is_voice_mode_open, changed);
                sync_field_copy!(this, state, is_app_settings_open, changed);
                sync_field_clone!(this, state, reply_to, changed);
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
                let rows = SkillSource.rows(&state.read(cx).recipes);
                this.remember_picks(&rows);
                let rows: Vec<ComposerPanelRow> = rows.into_iter().map(|(row, _)| row).collect();
                this.panel.update(cx, |panel, cx| panel.set_rows(rows, cx));
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
            println!("Submitting message: {}", trimmed);
            if let Some(handler) = &self.on_submit {
                (handler)(trimmed.to_string(), cx);
            }
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
            self.tokens.clear();
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
            "Type @ for the bot's tools, / for its skills.",
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
            "↑↓ to move, ↵ to put it in the message, esc to close.",
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
        let rows = SkillSource.rows(&self.state.read(cx).recipes);
        self.show_panel(
            PanelMode::Skills,
            rows,
            "Search skills and actions",
            "↑↓ to move, ↵ to run it or put it in the message, esc to close.",
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
        self.close_panel(true, window, cx);
        match pick {
            ComposerPick::AttachFiles => self.attach_files(cx),
            ComposerPick::TeachTask => self.teach_task(cx),
            ComposerPick::Token { kind, id, text } => self.insert_token(kind, id, text, window, cx),
            ComposerPick::Command(command) => self.run_command(command, window, cx),
            ComposerPick::Nothing => {}
        }
    }

    fn run_command(&mut self, command: AppCommand, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            AppCommand::Settings => window.dispatch_action(Box::new(OpenSettings), cx),
            AppCommand::NewChat => window.dispatch_action(Box::new(NewChat), cx),
            AppCommand::ToggleTheme => window.dispatch_action(Box::new(ToggleTheme), cx),
            AppCommand::Collections => window.dispatch_action(Box::new(Library), cx),
            AppCommand::Groups => window.dispatch_action(Box::new(Projects), cx),
            AppCommand::SettingsTab(tab) => {
                self.state
                    .update(cx, |state, cx| state.open_app_settings(tab, cx));
            }
            AppCommand::Recipes => self.state.update(cx, |state, cx| state.open_recipes(cx)),
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
            PanelMode::Tools => self.open_tools_panel(window, cx),
            PanelMode::Skills => self.open_skills_panel(window, cx),
            PanelMode::Plus => {}
        }
        true
    }

    /// The chips, as rounded fills behind the text they belong to.
    ///
    /// The field paints its text in one style and hosts no elements of its own, so a chip cannot
    /// be an element in the text flow. What it can be is this: the field says where a byte range
    /// ended up on screen, and the fill goes there, under the glyphs, which the field then paints
    /// over it. The chip therefore reads inline and wraps and scrolls with the text, because it
    /// is the text.
    fn chip_pills(&self, theme: &gpui_kit::component::Theme, cx: &App) -> Vec<AnyElement> {
        if self.tokens.is_empty() {
            return Vec::new();
        }
        let input = self.input_state.read(cx);
        let origin = input.input_bounds().origin;
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
                Some(
                    div()
                        .absolute()
                        .left(bounds.origin.x - origin.x - px(3.))
                        .top(bounds.origin.y - origin.y - px(1.))
                        .w(bounds.size.width + px(6.))
                        .h(bounds.size.height + px(2.))
                        .rounded(px(5.))
                        .bg(theme.primary.opacity(0.16))
                        .into_any_element(),
                )
            })
            .collect()
    }

    /// The text field, with the chips' fills under it.
    fn field(&self, theme: &gpui_kit::component::Theme, cx: &App) -> AnyElement {
        div()
            .relative()
            .w_full()
            .children(self.chip_pills(theme, cx))
            .child(Textarea::new(&self.input_state).appearance(false).w_full())
            .into_any_element()
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
        let selected_apps = self.selected_apps.clone();

        // Check if any modal is open using cached state
        let any_modal_open = self.is_voice_mode_open || self.is_app_settings_open;
        let draft = self.input_state.read(cx).value();
        let compact = !self.voice_mode
            && self.selected_apps.is_empty()
            && self.attachments.is_empty()
            && self.notice.is_none()
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
                                        let (tool_calls, rest): (Vec<_>, Vec<_>) = selected_apps.iter()
                                            .cloned()
                                            .partition(|app| matches!(app.as_str(), "Web search" | "Deep Research" | "Image Generation" | "Photos" | "Thinking"));

                                        let (skills, mini_apps): (Vec<_>, Vec<_>) = rest.into_iter()
                                            .partition(|app| matches!(app.as_str(), "Study" | "Canvas"));

                                        let groups = vec![
                                            ("Tools", tool_calls, "icons/wrench.svg"),
                                            ("Skills", skills, "icons/wizard_hat.svg"),
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
                                                                        let app_name = app.clone();
                                                                        let icon = tool_icon(&app_name);

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
                                                                                        state.remove_app(app_name.clone(), cx);
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
                                                                                    .child(app.clone())
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
                                                        let app_name = app.clone();
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
                                                                                state.remove_app(app_name.clone(), cx);
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
    use super::{is_image, starts_token};
    use std::path::PathBuf;

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
