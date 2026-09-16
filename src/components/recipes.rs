//! The Recipes page: the tasks taught on a bot's screen. The list says whose each one is and
//! what became of it last time; one recipe's own page puts its versions in a tab strip, the
//! steps of the chosen version in a table that scrolls inside a frame of fixed height, and
//! what a person may do with it in a toolbar that stays above that table.
//!
//! The page takes the chat's slot when the dock opens it, and the window's title bar carries
//! its header (see [`recipes_header`]), so the body starts straight under the bar.

use std::rc::Rc;

use crate::chrome::HEADER_PX;
use crate::components::fields::field_input;
use crate::opengrok::{
    RecipeDetail, RecipeRelation, RecipeRun, RecipeScreen, RecipeShareTarget, RecipeStep,
    RecipeSummary, RecipeTapeEvent, RecipeVersion,
};
use crate::state::{AppState, RecipeFilter, RecipeRunNote, RecipeRunOutcome};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::{Value, json};

type Theme = gpui_kit::component::Theme;

/// The longest a wait step may be: the server's rule.
const MAX_WAIT_MS: u64 = 10_000;
/// The most steps a version may hold: the server's rule.
const MAX_STEPS: usize = 256;
/// The wait a step added by hand starts with.
const NEW_WAIT_MS: u64 = 500;
/// The page's column. Wider than the chat's, because a table of steps needs the room, and
/// capped, because rows that run the width of a large window are hard to read across.
const COLUMN_MAX: f32 = 900.;
/// The steps table's frame. The table scrolls inside it rather than growing, so the toolbar
/// above it and the cards below it stay where they are however long the version is.
const STEPS_HEIGHT: f32 = 320.;
/// The table's first column: the step's number.
const STEP_NUMBER_W: f32 = 40.;
/// The table's second column: what the step does. The details take the rest of the row.
const STEP_VERB_W: f32 = 116.;
/// The tape table's last column: how long after the first event this one came.
const EVENT_TIME_W: f32 = 72.;
/// The run's screenshot, at the width the chat draws the bot's screen.
const SCREENSHOT_WIDTH: f32 = 520.;
/// The bots picker's panel: wide enough for a bot's name beside its box.
const BOTS_MENU_W: f32 = 240.;
/// The history modal: wide enough for a run's line and the screen it kept.
const HISTORY_MODAL_W: f32 = 560.;
/// The history's list of runs, so the modal is the same height whichever tab is on.
const HISTORY_RUNS_HEIGHT: f32 = 300.;
/// How many runs of one version the history shows. The server prunes a version to this many
/// as each run lands, so it is also how many there are.
const RUNS_PER_VERSION: usize = 5;
/// The column kept for what a run or a step left behind — screenshots, a recording. It is
/// empty until the server asks the box for artifacts, keeps them and serves them.
const ARTIFACTS_W: f32 = 28.;

/// What a step does, apart from what it does it to: what Add step offers, and what decides
/// whether a row's editor is the field in its Details cell or the modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepKind {
    Click,
    DoubleClick,
    Drag,
    Type,
    Key,
    Scroll,
    Wait,
}

impl StepKind {
    /// In the order the menu offers them: the pointer first, then the keyboard, then the pause.
    const ALL: [Self; 7] = [
        Self::Click,
        Self::DoubleClick,
        Self::Drag,
        Self::Type,
        Self::Key,
        Self::Scroll,
        Self::Wait,
    ];

    fn of(step: &RecipeStep) -> Self {
        match step {
            RecipeStep::Click { .. } => Self::Click,
            RecipeStep::DoubleClick { .. } => Self::DoubleClick,
            RecipeStep::Drag { .. } => Self::Drag,
            RecipeStep::Type { .. } => Self::Type,
            RecipeStep::Key { .. } => Self::Key,
            RecipeStep::Scroll { .. } => Self::Scroll,
            RecipeStep::Wait { .. } => Self::Wait,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Click => "Click",
            Self::DoubleClick => "Double click",
            Self::Drag => "Drag",
            Self::Type => "Type",
            Self::Key => "Key",
            Self::Scroll => "Scroll",
            Self::Wait => "Wait",
        }
    }

    /// The word in the element id, `recipe-add-step-double-click`.
    fn slug(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::DoubleClick => "double-click",
            Self::Drag => "drag",
            Self::Type => "type",
            Self::Key => "key",
            Self::Scroll => "scroll",
            Self::Wait => "wait",
        }
    }

    /// The parameters the modal edits, none for a step of one value (those are edited in the
    /// row itself).
    fn params(self) -> &'static [StepParam] {
        const PLACE: [StepParam; 2] = [
            StepParam::new("x", "X", ParamKind::X),
            StepParam::new("y", "Y", ParamKind::Y),
        ];
        const CLICK: [StepParam; 3] = [
            StepParam::new("x", "X", ParamKind::X),
            StepParam::new("y", "Y", ParamKind::Y),
            StepParam::new("button", "Button", ParamKind::Button),
        ];
        const DRAG: [StepParam; 4] = [
            StepParam::new("x1", "From X", ParamKind::X),
            StepParam::new("y1", "From Y", ParamKind::Y),
            StepParam::new("x2", "To X", ParamKind::X),
            StepParam::new("y2", "To Y", ParamKind::Y),
        ];
        const SCROLL: [StepParam; 4] = [
            StepParam::new("x", "X", ParamKind::X),
            StepParam::new("y", "Y", ParamKind::Y),
            StepParam::new("dx", "By X", ParamKind::Amount),
            StepParam::new("dy", "By Y", ParamKind::Amount),
        ];
        match self {
            Self::Click => &CLICK,
            Self::DoubleClick => &PLACE,
            Self::Drag => &DRAG,
            Self::Scroll => &SCROLL,
            Self::Type | Self::Key | Self::Wait => &[],
        }
    }

    /// A step of this kind to start from: in the middle of the screen, where a person can see
    /// what they are aiming at before they change it.
    fn new_step(self, screen: RecipeScreen) -> RecipeStep {
        let x = (screen.width / 2) as i64;
        let y = (screen.height / 2) as i64;
        match self {
            Self::Click => RecipeStep::Click { x, y, button: None },
            Self::DoubleClick => RecipeStep::DoubleClick { x, y },
            Self::Drag => RecipeStep::Drag {
                x1: x,
                y1: y,
                x2: (x + 100).min(screen.width as i64),
                y2: y,
            },
            Self::Type => RecipeStep::Type {
                text: String::new(),
            },
            Self::Key => RecipeStep::Key {
                key: "Return".to_string(),
            },
            Self::Scroll => RecipeStep::Scroll {
                x,
                y,
                dx: 0,
                dy: -120,
            },
            Self::Wait => RecipeStep::Wait { ms: NEW_WAIT_MS },
        }
    }
}

/// What a parameter of a step may hold, and so what a typed value has to be to be taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParamKind {
    /// Across the screen; the recipe's width bounds it.
    X,
    /// Down the screen; the recipe's height bounds it.
    Y,
    /// How far to scroll. Nothing bounds it: a wheel turns either way and past any edge.
    Amount,
    /// Which mouse button, as a word or as the number the tape used.
    Button,
}

/// One parameter of a step as the modal edits it: the word in its element id, the label over
/// its field, and what may be typed into it.
#[derive(Debug, Clone, Copy)]
struct StepParam {
    name: &'static str,
    label: &'static str,
    kind: ParamKind,
}

impl StepParam {
    const fn new(name: &'static str, label: &'static str, kind: ParamKind) -> Self {
        Self { name, label, kind }
    }
}

/// The row whose parameters the modal is showing, and of what kind, so Save knows what to
/// build from the fields.
#[derive(Debug, Clone, Copy)]
struct StepModal {
    index: usize,
    kind: StepKind,
}

pub struct RecipesView {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    description_input: Entity<InputState>,
    share_email_input: Entity<InputState>,
    note_input: Entity<InputState>,
    step_input: Entity<InputState>,
    /// The recipe whose name and description the fields hold; refilled when another opens.
    synced_id: Option<String>,
    /// The version the tabs have chosen. None until someone picks one, so a page that has
    /// just opened stands on the version a run would play.
    selected_version: Option<u32>,
    /// The steps under edit (the owner's Edit steps), until Save as new version or Cancel.
    draft: Option<Vec<RecipeStep>>,
    /// The draft row whose text, key or wait is in `step_input`.
    editing_step: Option<usize>,
    /// The row the modal is editing, for a step of more than one parameter.
    step_modal: Option<StepModal>,
    /// The modal's fields, one per parameter of the longest step there is. They are reused
    /// rather than made per step, so opening the modal costs no entities.
    step_fields: Vec<Entity<InputState>>,
    /// Why the draft cannot be saved as it is.
    draft_error: Option<String>,
    /// The bot Run plays on; the first granted one until the person picks.
    run_bot: Option<String>,
    /// The history modal is open over the page.
    history_open: bool,
    /// The version whose runs the history is showing; None until someone picks a tab, so an
    /// opened history stands on the version a run plays.
    history_version: Option<u32>,
    /// The run whose receipt is open under its row.
    history_run: Option<String>,
}

/// The most parameters any step has: a drag's two corners.
const STEP_FIELDS: usize = 4;

impl RecipesView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let description_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What this task does"));
        let share_email_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("name@company.com"));
        let note_input = cx.new(|cx| InputState::new(window, cx).placeholder("What changed"));
        let step_input = cx.new(|cx| InputState::new(window, cx));
        let step_fields: Vec<Entity<InputState>> = (0..STEP_FIELDS)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        // Enter closes the cell being edited, as it would in any table.
        cx.subscribe(&step_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.commit_step_edit(cx);
            }
        })
        .detach();
        // Enter in any of the modal's fields is its Save, as it would be in any dialog.
        for field in &step_fields {
            cx.subscribe(field, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.save_step_modal(cx);
                }
            })
            .detach();
        }
        Self {
            state,
            name_input,
            description_input,
            share_email_input,
            note_input,
            step_input,
            synced_id: None,
            selected_version: None,
            draft: None,
            editing_step: None,
            step_modal: None,
            step_fields,
            draft_error: None,
            run_bot: None,
            history_open: false,
            history_version: None,
            history_run: None,
        }
    }

    /// The fields follow the open recipe: filled when one opens, left alone while it is the
    /// same one (a rename comes back from the server with what was typed).
    fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (id, name, description) = {
            let state = self.state.read(cx);
            match &state.recipe_open {
                Some(detail) => (
                    Some(detail.recipe.id.clone()),
                    detail.recipe.name.clone(),
                    detail.recipe.description.clone(),
                ),
                None => (None, String::new(), String::new()),
            }
        };
        if self.synced_id == id {
            return;
        }
        self.synced_id = id;
        self.selected_version = None;
        self.draft = None;
        self.editing_step = None;
        self.step_modal = None;
        self.draft_error = None;
        self.run_bot = None;
        self.history_open = false;
        self.history_version = None;
        self.history_run = None;
        self.name_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
        });
        self.description_input.update(cx, |input, cx| {
            input.set_value(description, window, cx);
        });
        self.share_email_input.update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
        });
        self.note_input.update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
        });
    }

    fn select_version(&mut self, version: u32, cx: &mut Context<Self>) {
        self.selected_version = Some(version);
        cx.notify();
    }

    /// Open the history over the page, on the version a run plays and with no run expanded.
    fn open_history(&mut self, cx: &mut Context<Self>) {
        self.history_open = true;
        self.history_version = None;
        self.history_run = None;
        cx.notify();
    }

    fn close_history(&mut self, cx: &mut Context<Self>) {
        self.history_open = false;
        self.history_run = None;
        cx.notify();
    }

    fn select_history_version(&mut self, version: u32, cx: &mut Context<Self>) {
        self.history_version = Some(version);
        // A run of another version is not on screen any more, so nothing is expanded.
        self.history_run = None;
        cx.notify();
    }

    /// A row opens its receipt under itself, and closes it when it is the open one.
    fn toggle_history_run(&mut self, id: String, cx: &mut Context<Self>) {
        self.history_run = if self.history_run.as_deref() == Some(id.as_str()) {
            None
        } else {
            Some(id)
        };
        cx.notify();
    }

    /// The bot Run plays on: the one last picked, else the first of the person's bots this
    /// recipe was granted to, else the first they have (a person with one bot and no grant
    /// still means that bot).
    fn picked_bot(&self, detail: &RecipeDetail) -> Option<String> {
        self.run_bot
            .clone()
            .filter(|id| detail.my_bots.iter().any(|bot| &bot.id == id))
            .or_else(|| granted_bots(detail).first().map(|bot| bot.0.clone()))
            .or_else(|| detail.my_bots.first().map(|bot| bot.id.clone()))
    }

    /// Start editing: a copy of the steps of the version on screen, or of the one a run plays
    /// when the tape is what is on screen.
    fn begin_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = {
            let state = self.state.read(cx);
            let Some(detail) = state.recipe_open.as_ref() else {
                return;
            };
            shown_version(detail, self.selected_version)
                .filter(|version| version.is_runnable())
                .or_else(|| detail.runnable_version())
                .map(|version| (version.version, version.body.steps.clone()))
        };
        let Some((version, steps)) = picked else {
            return;
        };
        self.selected_version = Some(version);
        self.draft = Some(steps);
        self.editing_step = None;
        self.step_modal = None;
        // Only one thing stands over the page at a time.
        self.history_open = false;
        self.draft_error = None;
        self.note_input.update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
        });
        cx.notify();
    }

    fn cancel_draft(&mut self, cx: &mut Context<Self>) {
        self.draft = None;
        self.editing_step = None;
        self.step_modal = None;
        self.draft_error = None;
        cx.notify();
    }

    fn move_step(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        let other = if up {
            index.checked_sub(1)
        } else {
            Some(index + 1).filter(|next| *next < draft.len())
        };
        let Some(other) = other else {
            return;
        };
        if index >= draft.len() {
            return;
        }
        draft.swap(index, other);
        // The row being edited moves with its step.
        if self.editing_step == Some(index) {
            self.editing_step = Some(other);
        } else if self.editing_step == Some(other) {
            self.editing_step = Some(index);
        }
        cx.notify();
    }

    fn delete_step(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        if index >= draft.len() {
            return;
        }
        draft.remove(index);
        match self.editing_step {
            Some(editing) if editing == index => self.editing_step = None,
            Some(editing) if editing > index => self.editing_step = Some(editing - 1),
            _ => {}
        }
        cx.notify();
    }

    /// The screen this recipe was taught on, which is what bounds a step's coordinates.
    fn screen(&self, cx: &App) -> RecipeScreen {
        self.state
            .read(cx)
            .recipe_open
            .as_ref()
            .map(|detail| detail.recipe.screen)
            .unwrap_or_default()
    }

    /// Add step: a step of the chosen kind at the end of the draft, with its editor open, so
    /// the defaults it starts on are never what gets saved by accident.
    fn add_step(&mut self, kind: StepKind, window: &mut Window, cx: &mut Context<Self>) {
        let screen = self.screen(cx);
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        if draft.len() >= MAX_STEPS {
            self.draft_error = Some(format!("A version holds at most {MAX_STEPS} steps."));
            cx.notify();
            return;
        }
        draft.push(kind.new_step(screen));
        let index = draft.len() - 1;
        self.draft_error = None;
        self.open_step_editor(index, window, cx);
    }

    /// A row's editor: the field in its Details cell for a step of one value, the modal for a
    /// step of several.
    fn open_step_editor(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.draft.as_ref().and_then(|draft| draft.get(index)) else {
            return;
        };
        if StepKind::of(step).params().is_empty() {
            self.begin_step_edit(index, window, cx);
        } else {
            self.open_step_modal(index, window, cx);
        }
    }

    /// Put the step's text, key or wait in the field and open that cell for editing.
    fn begin_step_edit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let value = match self.draft.as_ref().and_then(|draft| draft.get(index)) {
            Some(RecipeStep::Type { text }) => text.clone(),
            Some(RecipeStep::Key { key }) => key.clone(),
            Some(RecipeStep::Wait { ms }) => ms.to_string(),
            _ => return,
        };
        self.editing_step = Some(index);
        self.step_modal = None;
        self.draft_error = None;
        self.step_input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
        });
        cx.notify();
    }

    fn commit_step_edit(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.editing_step else {
            return;
        };
        let value = self.step_input.read(cx).value().to_string();
        let Some(step) = self.draft.as_mut().and_then(|draft| draft.get_mut(index)) else {
            self.editing_step = None;
            return;
        };
        let mut note = None;
        match step {
            RecipeStep::Type { text } => *text = value,
            RecipeStep::Key { key } => {
                let typed = value.trim();
                if typed.is_empty() {
                    self.draft_error =
                        Some("A key step needs a key, such as Return or a.".to_string());
                    cx.notify();
                    return;
                }
                *key = typed.to_string();
            }
            RecipeStep::Wait { ms } => match value.trim().parse::<u64>() {
                Ok(parsed) => {
                    *ms = parsed.min(MAX_WAIT_MS);
                    if parsed > MAX_WAIT_MS {
                        note = Some(format!(
                            "A wait is at most {MAX_WAIT_MS} ms, so that one was shortened."
                        ));
                    }
                }
                Err(_) => {
                    self.draft_error = Some(format!(
                        "A wait is a number of milliseconds, up to {MAX_WAIT_MS}."
                    ));
                    cx.notify();
                    return;
                }
            },
            _ => {}
        }
        self.editing_step = None;
        self.draft_error = note;
        cx.notify();
    }

    fn cancel_step_edit(&mut self, cx: &mut Context<Self>) {
        self.editing_step = None;
        cx.notify();
    }

    /// Open the modal on a row: one field per parameter of the step, filled with what it holds.
    fn open_step_modal(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.draft.as_ref().and_then(|draft| draft.get(index)) else {
            return;
        };
        let kind = StepKind::of(step);
        let values = step_param_values(step);
        if values.is_empty() {
            return;
        }
        for (field, value) in self.step_fields.iter().zip(values) {
            field.update(cx, |input, cx| {
                input.set_value(value, window, cx);
            });
        }
        // The first field takes the caret, so the modal can be filled in and saved without
        // reaching for the mouse again.
        if let Some(first) = self.step_fields.first() {
            first.update(cx, |input, cx| input.focus(window, cx));
        }
        self.editing_step = None;
        self.step_modal = Some(StepModal { index, kind });
        self.draft_error = None;
        cx.notify();
    }

    /// The modal's Save: the fields as a step, or the reason they are not one on the page's
    /// error line, with the modal left open on what was typed.
    fn save_step_modal(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.step_modal else {
            return;
        };
        let screen = self.screen(cx);
        let typed: Vec<String> = self
            .step_fields
            .iter()
            .map(|field| field.read(cx).value().trim().to_string())
            .collect();
        let step = match build_step(modal.kind, &typed, screen) {
            Ok(step) => step,
            Err(reason) => {
                self.draft_error = Some(reason);
                cx.notify();
                return;
            }
        };
        if let Some(slot) = self
            .draft
            .as_mut()
            .and_then(|draft| draft.get_mut(modal.index))
        {
            *slot = step;
        }
        self.step_modal = None;
        self.draft_error = None;
        cx.notify();
    }

    fn cancel_step_modal(&mut self, cx: &mut Context<Self>) {
        self.step_modal = None;
        cx.notify();
    }

    /// The draft as the recipe's next version. The draft closes at once; a refusal shows in
    /// the page's error line.
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.clone() else {
            return;
        };
        if draft.is_empty() {
            self.draft_error = Some("A version needs at least one step.".to_string());
            cx.notify();
            return;
        }
        if draft.len() > MAX_STEPS {
            self.draft_error = Some(format!("A version holds at most {MAX_STEPS} steps."));
            cx.notify();
            return;
        }
        let note = self.note_input.read(cx).value().trim().to_string();
        self.state.update(cx, |state, cx| {
            state.add_recipe_version(draft, note, cx);
        });
        // The saved version is the newest, and the newest is what a fresh page stands on.
        self.selected_version = None;
        self.draft = None;
        self.editing_step = None;
        self.step_modal = None;
        self.draft_error = None;
        cx.notify();
    }
}

impl Render for RecipesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_fields(window, cx);
        let theme = cx.theme().clone();
        let open = self.state.read(cx).recipe_open_id.is_some();
        let view = cx.entity();
        v_flex()
            .id("page-recipes")
            .size_full()
            // The step modal hangs off the page, so it is centred on the page's own slot.
            .relative()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(if open {
                self.detail(&theme, cx)
            } else {
                self.list(&theme, cx)
            })
            .when_some(self.step_modal, |this, modal| {
                this.child(self.step_modal_overlay(modal, &view, &theme, cx))
            })
            .when_some(
                self.history_open
                    .then(|| self.state.read(cx).recipe_open.clone())
                    .flatten(),
                |this, detail| this.child(self.history_overlay(&detail, &view, &theme)),
            )
    }
}

impl RecipesView {
    /// The list: the filter chips, then a row per recipe.
    fn list(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = self.state.clone();
        let muted = theme.muted_foreground;
        let (filter, recipes, loading, error, me, last_runs) = {
            let state = self.state.read(cx);
            (
                state.recipes_filter,
                state.recipes.clone(),
                state.recipes_loading,
                state.recipes_error.clone(),
                state.account.as_ref().map(|account| account.id.clone()),
                state.recipe_last_runs.clone(),
            )
        };
        let pending = recipes
            .iter()
            .filter(|recipe| recipe.is_pending_invite())
            .count();
        let empty_copy = match filter {
            RecipeFilter::Mine => "Nothing taught yet. Open a bot's computer and use Teach a task.",
            RecipeFilter::Shared => "Nothing has been shared with you.",
            RecipeFilter::Org => "Nothing in your org yet.",
        };
        v_flex()
            .size_full()
            .child(
                div()
                    .id("recipes-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(HEADER_PX))
                    .pb(px(24.))
                    .child(centered(
                        column()
                            .child(
                                h_flex().id("recipes-filters").gap(px(6.)).children(
                                    RecipeFilter::ALL
                                        .into_iter()
                                        .map(|chip| filter_chip(chip, filter, app.clone())),
                                ),
                            )
                            .when_some(error, |this, error| {
                                this.child(
                                    div()
                                        .id("recipes-error")
                                        .text_xs()
                                        .text_color(theme.danger)
                                        .child(error),
                                )
                            })
                            .when(loading && recipes.is_empty(), |this| {
                                this.child(div().text_sm().text_color(muted).child("Loading…"))
                            })
                            .when(!loading && recipes.is_empty(), |this| {
                                this.child(
                                    div()
                                        .id("recipes-empty")
                                        .text_sm()
                                        .text_color(muted)
                                        .child(empty_copy),
                                )
                            })
                            .children(recipes.into_iter().map(|recipe| {
                                let last_run = last_runs.get(&recipe.id).copied();
                                recipe_row(
                                    recipe,
                                    last_run,
                                    me.as_deref(),
                                    pending == 1,
                                    app.clone(),
                                    theme,
                                )
                            })),
                    )),
            )
            .into_any_element()
    }

    /// One recipe: what it is, its versions and their steps, the bots that may run it, who it
    /// is shared with, and the runs so far.
    fn detail(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = self.state.clone();
        let view = cx.entity();
        let muted = theme.muted_foreground;
        let (detail, loading, busy, error, run, me) = {
            let state = self.state.read(cx);
            (
                state.recipe_open.clone(),
                state.recipe_loading,
                state.recipe_busy.clone(),
                state.recipe_error.clone(),
                state.recipe_run_result.clone(),
                state.account.as_ref().map(|account| account.id.clone()),
            )
        };
        v_flex()
            .size_full()
            .child(
                div()
                    .id("recipe-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(HEADER_PX))
                    .pb(px(24.))
                    .child(centered(
                        column()
                            .when_some(error, |this, error| {
                                this.child(
                                    div()
                                        .id("recipe-error")
                                        .text_xs()
                                        .text_color(theme.danger)
                                        .child(error),
                                )
                            })
                            .when(loading && detail.is_none(), |this| {
                                this.child(div().text_sm().text_color(muted).child("Loading…"))
                            })
                            .when_some(detail, |this, detail| {
                                this.children(self.sections(
                                    &detail,
                                    busy.as_deref(),
                                    run,
                                    me.as_deref(),
                                    &app,
                                    &view,
                                    theme,
                                ))
                            }),
                    )),
            )
            .into_any_element()
    }

    /// The cards of the detail, for what the person may do with this recipe: everything for
    /// its owner; the steps, the bots, a run and the history for someone it is shared with;
    /// only Accept and Decline for someone it is offered to.
    fn sections(
        &self,
        detail: &RecipeDetail,
        busy: Option<&str>,
        run: Option<RecipeRunOutcome>,
        me: Option<&str>,
        app: &Entity<AppState>,
        view: &Entity<Self>,
        theme: &Theme,
    ) -> Vec<AnyElement> {
        let mine = detail.recipe.is_mine();
        let waiting = busy.is_some();
        let mut sections = vec![self.about_section(detail, waiting, me, app, theme)];
        if detail.recipe.is_pending_invite() {
            return sections;
        }
        sections.push(self.steps_section(detail, busy, app, view, theme));
        if let Some(run) = run {
            sections.push(last_run_section(&run, detail, theme));
        }
        sections.push(bots_section(detail, waiting, app, theme));
        if mine {
            sections.push(self.share_section(detail, waiting, app, theme));
        }
        sections
    }

    fn about_section(
        &self,
        detail: &RecipeDetail,
        busy: bool,
        me: Option<&str>,
        app: &Entity<AppState>,
        theme: &Theme,
    ) -> AnyElement {
        let muted = theme.muted_foreground;
        let recipe = &detail.recipe;
        let owner = owner_label(recipe, me);
        if recipe.is_pending_invite() {
            return card(theme)
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(recipe.name.clone()),
                )
                .when(!recipe.description.trim().is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(recipe.description.clone()),
                    )
                })
                .child(div().text_xs().text_color(muted).child(format!(
                    "Shared with you by {owner}. Accept it to see its steps and run it."
                )))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .pt(px(4.))
                        .child(answer_button(
                            "recipe-accept",
                            "Accept",
                            true,
                            recipe.id.clone(),
                            busy,
                            app.clone(),
                        ))
                        .child(answer_button(
                            "recipe-decline",
                            "Decline",
                            false,
                            recipe.id.clone(),
                            busy,
                            app.clone(),
                        )),
                )
                .into_any_element();
        }
        if recipe.is_mine() {
            let name_input = self.name_input.clone();
            let description_input = self.description_input.clone();
            return card(theme)
                .child(section_title("About"))
                .child(field_label("Name", muted))
                .child(
                    div()
                        .w_full()
                        .child(field_input(&self.name_input).id("recipe-name")),
                )
                .child(field_label("Description", muted))
                .child(
                    div()
                        .w_full()
                        .child(field_input(&self.description_input).id("recipe-description")),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .child(div().text_xs().text_color(muted).child(format!(
                            "v{} · updated {}",
                            recipe.latest_version,
                            format_time(recipe.updated_at_ms)
                        )))
                        .child(
                            Button::new("recipe-save-meta")
                                .small()
                                .primary()
                                .label("Save")
                                .disabled(busy)
                                .on_click({
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        let name = name_input.read(cx).value().trim().to_string();
                                        let description =
                                            description_input.read(cx).value().trim().to_string();
                                        if name.is_empty() {
                                            return;
                                        }
                                        app.update(cx, |state, cx| {
                                            state.rename_open_recipe(name, description, cx);
                                        });
                                    }
                                }),
                        ),
                )
                .into_any_element();
        }
        card(theme)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(recipe.name.clone()),
            )
            .when(!recipe.description.trim().is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(recipe.description.clone()),
                )
            })
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("by {owner} · v{}", recipe.latest_version)),
            )
            .into_any_element()
    }

    /// The versions as a tab strip, the toolbar, and the chosen version's steps in a table
    /// that scrolls inside its own frame.
    fn steps_section(
        &self,
        detail: &RecipeDetail,
        busy: Option<&str>,
        app: &Entity<AppState>,
        view: &Entity<Self>,
        theme: &Theme,
    ) -> AnyElement {
        let muted = theme.muted_foreground;
        let editing = self.draft.is_some();
        let current = detail.runnable_version().map(|version| version.version);
        let shown = shown_version(detail, self.selected_version);
        let shown_number = shown.map(|version| version.version);
        // Newest last, the way a tape grows.
        let mut versions: Vec<&RecipeVersion> = detail.versions.iter().collect();
        versions.sort_by_key(|version| version.version);
        let steps: Option<&Vec<RecipeStep>> = match self.draft.as_ref() {
            Some(draft) => Some(draft),
            None => shown
                .filter(|version| version.is_runnable())
                .map(|version| &version.body.steps),
        };
        card(theme)
            .child(
                h_flex()
                    .id("recipe-versions")
                    .w_full()
                    .flex_wrap()
                    .items_end()
                    .gap(px(2.))
                    .border_b_1()
                    .border_color(theme.border)
                    .when(versions.is_empty(), |this| {
                        this.child(
                            div()
                                .py(px(8.))
                                .text_xs()
                                .text_color(muted)
                                .child("No versions yet."),
                        )
                    })
                    .children(versions.into_iter().map(|version| {
                        version_tab(
                            version,
                            shown_number == Some(version.version),
                            current == Some(version.version),
                            editing,
                            view,
                            muted,
                            theme,
                        )
                    })),
            )
            .child(self.steps_toolbar(detail, busy, app, view, theme))
            .child(version_line(shown, editing, muted))
            .map(|this| match steps {
                Some(steps) => this.child(self.steps_table(steps, editing, view, muted, theme)),
                // A tape the server sent whole reads as a table of its own; one it stripped to
                // a count can only say how much there was.
                None => match shown.and_then(RecipeVersion::tape_events) {
                    Some(events) if !events.is_empty() => {
                        this.child(tape_table(events, muted, theme))
                    }
                    _ => this.child(tape_frame(shown, muted, theme)),
                },
            })
            .when_some(self.draft_error.clone(), |this, error| {
                this.child(
                    div()
                        .id("recipe-steps-error")
                        .text_xs()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .into_any_element()
    }

    /// What may be done with the recipe, above the table and out of its scroll: Run, Edit
    /// steps and Delete, or the editor's own controls while a draft is open.
    fn steps_toolbar(
        &self,
        detail: &RecipeDetail,
        busy: Option<&str>,
        app: &Entity<AppState>,
        view: &Entity<Self>,
        theme: &Theme,
    ) -> AnyElement {
        let muted = theme.muted_foreground;
        let waiting = busy.is_some();
        if let Some(draft) = self.draft.as_ref() {
            let total = draft.len();
            return h_flex()
                .id("recipe-steps-editor")
                .w_full()
                .flex_wrap()
                .items_center()
                .gap(px(8.))
                .child(add_step_menu(total >= MAX_STEPS, view))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .child(field_input(&self.note_input).id("recipe-version-note")),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{total} of {MAX_STEPS} steps")),
                )
                .child(
                    Button::new("recipe-save-version")
                        .small()
                        .primary()
                        .label("Save as new version")
                        .disabled(waiting || total == 0)
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| this.save_draft(cx));
                            }
                        }),
                )
                .child(
                    Button::new("recipe-cancel-edit")
                        .small()
                        .label("Cancel")
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| this.cancel_draft(cx));
                            }
                        }),
                )
                .into_any_element();
        }
        let mine = detail.recipe.is_mine();
        let running = busy == Some("Running…");
        let picked = self.picked_bot(detail);
        let granted = granted_bots(detail);
        let version = detail.runnable_version().map(|version| version.version);
        let can_run = picked.is_some() && version.is_some() && !waiting;
        let plays = version.map(|version| match picked.as_ref() {
            Some(bot) => format!("plays v{version} on {}", detail.bot_name(bot)),
            None => format!("plays v{version}"),
        });
        h_flex()
            .id("recipe-toolbar")
            .w_full()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(2.))
                            .child(
                                Button::new("recipe-run")
                                    .small()
                                    .primary()
                                    .label(if running { "Running…" } else { "Run" })
                                    .disabled(!can_run)
                                    .on_click({
                                        let app = app.clone();
                                        let picked = picked.clone();
                                        move |_, _, cx| {
                                            let Some(id) = picked.clone() else {
                                                return;
                                            };
                                            app.update(cx, |state, cx| {
                                                state.run_open_recipe(id, cx)
                                            });
                                        }
                                    }),
                            )
                            // One granted bot is no choice at all, and the line beside Run
                            // already names it.
                            .when(granted.len() > 1, |this| {
                                this.child(run_pick_menu(granted, picked.clone(), view))
                            }),
                    )
                    .when_some(plays, |this, plays| {
                        this.child(div().text_xs().text_color(muted).child(plays))
                    }),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        Button::new("recipe-history")
                            .small()
                            .label("History")
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.open_history(cx));
                                }
                            }),
                    )
                    .when(mine, |this| {
                        this.child(
                            Button::new("recipe-edit-steps")
                                .small()
                                .label("Edit steps")
                                .disabled(waiting || version.is_none())
                                .on_click({
                                    let view = view.clone();
                                    move |_, window, cx| {
                                        view.update(cx, |this, cx| this.begin_draft(window, cx));
                                    }
                                }),
                        )
                        .child(
                            Button::new("recipe-delete")
                                .small()
                                .danger()
                                .label("Delete")
                                .disabled(waiting)
                                .on_click({
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.open_recipe_delete_confirm(cx)
                                        });
                                    }
                                }),
                        )
                    }),
            )
            .into_any_element()
    }

    /// The steps: a header row that stays, and the rows themselves in a frame of fixed
    /// height, so a longer version scrolls rather than pushing the page down.
    fn steps_table(
        &self,
        steps: &[RecipeStep],
        editing: bool,
        view: &Entity<Self>,
        muted: Hsla,
        theme: &Theme,
    ) -> AnyElement {
        let total = steps.len();
        v_flex()
            .w_full()
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(steps_head(muted, theme))
            .child(
                div()
                    .id("recipe-steps")
                    .w_full()
                    .h(px(STEPS_HEIGHT))
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .w_full()
                            .when(steps.is_empty(), |this| {
                                this.child(
                                    div()
                                        .px(px(10.))
                                        .py(px(10.))
                                        .text_xs()
                                        .text_color(muted)
                                        .child("This version has no steps."),
                                )
                            })
                            .children(steps.iter().enumerate().map(|(index, step)| {
                                self.step_row(index, step, total, editing, view, muted, theme)
                            })),
                    ),
            )
            .into_any_element()
    }

    /// One row of the table: its number, what it does, and to what. In edit mode the details
    /// of a type or a wait open in place, and the row carries move and delete.
    fn step_row(
        &self,
        index: usize,
        step: &RecipeStep,
        total: usize,
        editing: bool,
        view: &Entity<Self>,
        muted: Hsla,
        theme: &Theme,
    ) -> AnyElement {
        let (verb, details) = step_words(step);
        let editable = editing;
        let open = editing && self.editing_step == Some(index);
        let row = h_flex()
            .id(SharedString::from(format!("recipe-step-{index}")))
            .w_full()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .py(px(6.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .w(px(STEP_NUMBER_W))
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{}", index + 1)),
            )
            .child(
                div()
                    .w(px(STEP_VERB_W))
                    .flex_shrink_0()
                    .text_sm()
                    .truncate()
                    .child(verb),
            );
        if open {
            let hint = match step {
                RecipeStep::Wait { .. } => "milliseconds, up to 10000",
                RecipeStep::Key { .. } => "a key, such as Return",
                _ => "the text to type",
            };
            return row
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(field_input(&self.step_input).id("recipe-step-input")),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(hint),
                )
                .child(
                    Button::new("recipe-step-done")
                        .xsmall()
                        .primary()
                        .label("Done")
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| this.commit_step_edit(cx));
                            }
                        }),
                )
                .child(
                    Button::new("recipe-step-cancel")
                        .xsmall()
                        .label("Cancel")
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| this.cancel_step_edit(cx));
                            }
                        }),
                )
                .into_any_element();
        }
        row.child(
            div()
                .id(SharedString::from(format!("recipe-step-edit-{index}")))
                .flex_1()
                .min_w_0()
                .text_sm()
                .truncate()
                .when(editable, |this| {
                    this.px(px(6.))
                        .py(px(1.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0x777777).opacity(0.14)))
                        // Double click, not single: a single click on a row in a table
                        // selects, and an editor that opens under the pointer while someone
                        // is only reading is in the way.
                        .on_click({
                            let view = view.clone();
                            move |event: &ClickEvent, window, cx| {
                                if event.click_count() < 2 {
                                    return;
                                }
                                view.update(cx, |this, cx| {
                                    this.open_step_editor(index, window, cx)
                                });
                            }
                        })
                })
                .child(details),
        )
        // What this step left behind — its screenshots — will hang here; empty until the
        // server keeps them.
        .child(div().w(px(ARTIFACTS_W)).flex_shrink_0())
        .when(editing, |this| {
            this.child(
                Button::new(SharedString::from(format!("recipe-step-up-{index}")))
                    .xsmall()
                    .ghost()
                    .icon(Icon::new(IconName::ChevronUp).size(px(14.)))
                    .disabled(index == 0)
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.move_step(index, true, cx));
                        }
                    }),
            )
            .child(
                Button::new(SharedString::from(format!("recipe-step-down-{index}")))
                    .xsmall()
                    .ghost()
                    .icon(Icon::new(IconName::ChevronDown).size(px(14.)))
                    .disabled(index + 1 >= total)
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.move_step(index, false, cx));
                        }
                    }),
            )
            .child(
                Button::new(SharedString::from(format!("recipe-step-delete-{index}")))
                    .xsmall()
                    .ghost()
                    .icon(Icon::default().path("icons/trash.svg").size(px(14.)))
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.delete_step(index, cx));
                        }
                    }),
            )
        })
        .into_any_element()
    }

    /// The owner's sharing: the org, one person by email, and who has it now.
    fn share_section(
        &self,
        detail: &RecipeDetail,
        busy: bool,
        app: &Entity<AppState>,
        theme: &Theme,
    ) -> AnyElement {
        let muted = theme.muted_foreground;
        let org_shared = detail.shares.iter().any(|share| share.scope == "org");
        let email_input = self.share_email_input.clone();
        card(theme)
            .child(section_title("Share"))
            .child(
                h_flex().w_full().gap(px(8.)).items_center().child(
                    Button::new("recipe-share-org")
                        .small()
                        .label(if org_shared {
                            "Shared with the org"
                        } else {
                            "Share with org"
                        })
                        .disabled(busy || org_shared)
                        .on_click({
                            let app = app.clone();
                            move |_, _, cx| {
                                app.update(cx, |state, cx| {
                                    state.share_open_recipe(RecipeShareTarget::Org, cx);
                                });
                            }
                        }),
                ),
            )
            .child(field_label("Share with a person", muted))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(field_input(&self.share_email_input).id("recipe-share-email")),
                    )
                    .child(
                        Button::new("recipe-share")
                            .small()
                            .primary()
                            .label("Share")
                            .disabled(busy)
                            .on_click({
                                let app = app.clone();
                                move |_, window, cx| {
                                    let email = email_input.read(cx).value().trim().to_string();
                                    if email.is_empty() {
                                        return;
                                    }
                                    email_input.update(cx, |input, cx| {
                                        input.set_value(String::new(), window, cx);
                                    });
                                    app.update(cx, |state, cx| {
                                        state.share_open_recipe(
                                            RecipeShareTarget::Account { email },
                                            cx,
                                        );
                                    });
                                }
                            }),
                    ),
            )
            .when(!detail.shares.is_empty(), |this| {
                this.child(field_label("Shared with", muted))
            })
            .children(detail.shares.iter().map(|share| {
                let who = if share.scope == "org" {
                    "Everyone in the org".to_string()
                } else {
                    share.scope_id.clone()
                };
                let scope = share.scope.clone();
                let scope_id = share.scope_id.clone();
                let row_id = format!("recipe-share-{scope}-{scope_id}");
                let unshare_id = format!("recipe-unshare-{scope}-{scope_id}");
                h_flex()
                    .id(SharedString::from(row_id))
                    .w_full()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().text_sm().truncate().child(who))
                    .child(div().text_xs().text_color(muted).child(share.state_label()))
                    .child(
                        Button::new(SharedString::from(unshare_id))
                            .xsmall()
                            .ghost()
                            .label("✕")
                            .disabled(busy)
                            .on_click({
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.unshare_open_recipe(
                                            scope.clone(),
                                            scope_id.clone(),
                                            cx,
                                        );
                                    });
                                }
                            }),
                    )
            }))
            .into_any_element()
    }

    /// The modal a step of more than one parameter opens on: one labelled field per parameter,
    /// Cancel and Save. It mirrors the delete dialog — the same scrim, the same centred card —
    /// and a click on the scrim is its Cancel, because a dialog that traps a stray click is
    /// worse than one that is easy to leave.
    fn step_modal_overlay(
        &self,
        modal: StepModal,
        view: &Entity<Self>,
        theme: &Theme,
        cx: &App,
    ) -> AnyElement {
        let muted = theme.muted_foreground;
        let screen = self.screen(cx);
        let params = modal.kind.params();
        div()
            .id("recipe-step-modal")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.32))
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| this.cancel_step_modal(cx));
                }
            })
            .child(
                v_flex()
                    .id("recipe-step-modal-card")
                    .w(px(420.))
                    .bg(theme.popover)
                    .text_color(theme.foreground)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(14.))
                    .shadow_lg()
                    .px(px(20.))
                    .py(px(18.))
                    .gap(px(10.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Step {}: {}", modal.index + 1, modal.kind.label())),
                    )
                    .child(div().text_xs().text_color(muted).child(format!(
                        "This recipe's screen is {} × {}.",
                        screen.width, screen.height
                    )))
                    .children(params.iter().zip(&self.step_fields).map(|(param, field)| {
                        v_flex()
                            .w_full()
                            .gap(px(4.))
                            .child(field_label(param.label, muted))
                            .child(
                                div()
                                    .w_full()
                                    .child(field_input(field).id(SharedString::from(format!(
                                        "recipe-step-field-{}",
                                        param.name
                                    )))),
                            )
                    }))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap(px(8.))
                            .pt(px(6.))
                            .child(
                                Button::new("recipe-step-modal-cancel")
                                    .label("Cancel")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| this.cancel_step_modal(cx));
                                        }
                                    }),
                            )
                            .child(
                                Button::new("recipe-step-modal-save")
                                    .primary()
                                    .label("Save")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| this.save_step_modal(cx));
                                        }
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// The history: every version a tab, and under the chosen one the runs of that version,
    /// newest first. The server keeps five runs per version and prunes the rest as each run
    /// lands, so five is all there ever is to show.
    fn history_overlay(
        &self,
        detail: &RecipeDetail,
        view: &Entity<Self>,
        theme: &Theme,
    ) -> AnyElement {
        let muted = theme.muted_foreground;
        let mut versions: Vec<u32> = detail
            .versions
            .iter()
            .map(|version| version.version)
            .collect();
        versions.sort_unstable();
        let shown = shown_version(detail, self.history_version).map(|version| version.version);
        let runs = shown
            .map(|version| runs_of(detail, version))
            .unwrap_or_default();
        div()
            .id("recipe-history-modal")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.32))
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| this.close_history(cx));
                }
            })
            .child(
                v_flex()
                    .id("recipe-history-card")
                    .w(px(HISTORY_MODAL_W))
                    .bg(theme.popover)
                    .text_color(theme.foreground)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(14.))
                    .shadow_lg()
                    .px(px(20.))
                    .py(px(18.))
                    .gap(px(10.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("History"),
                    )
                    .child(
                        h_flex()
                            .id("recipe-history-versions")
                            .w_full()
                            .flex_wrap()
                            .items_end()
                            .gap(px(2.))
                            .border_b_1()
                            .border_color(theme.border)
                            .when(versions.is_empty(), |this| {
                                this.child(
                                    div()
                                        .py(px(8.))
                                        .text_xs()
                                        .text_color(muted)
                                        .child("No versions yet."),
                                )
                            })
                            .children(versions.into_iter().map(|number| {
                                history_tab(number, shown == Some(number), view, muted, theme)
                            })),
                    )
                    .child(
                        div()
                            .id("recipe-history-runs")
                            .w_full()
                            .h(px(HISTORY_RUNS_HEIGHT))
                            .overflow_y_scroll()
                            .child(
                                v_flex()
                                    .w_full()
                                    .when(runs.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .py(px(10.))
                                                .text_xs()
                                                .text_color(muted)
                                                .child("No runs of this version yet."),
                                        )
                                    })
                                    .children(runs.into_iter().map(|run| {
                                        history_run_row(
                                            run,
                                            detail,
                                            self.history_run.as_deref() == Some(run.id.as_str()),
                                            view,
                                            muted,
                                            theme,
                                        )
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap(px(8.))
                            .pt(px(6.))
                            .child(
                                Button::new("recipe-history-close")
                                    .label("Close")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| this.close_history(cx));
                                        }
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
}

/// The runs of one version, newest first and no more than the five the server keeps. It prunes
/// a version to that many as each run lands, so this is a floor under a server that has not.
fn runs_of(detail: &RecipeDetail, version: u32) -> Vec<&RecipeRun> {
    let mut runs: Vec<&RecipeRun> = detail
        .runs
        .iter()
        .filter(|run| run.version == version)
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.at_ms));
    runs.truncate(RUNS_PER_VERSION);
    runs
}

/// One tab of the history: the version number, with the chosen one underlined.
fn history_tab(
    number: u32,
    selected: bool,
    view: &Entity<RecipesView>,
    muted: Hsla,
    theme: &Theme,
) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "recipe-history-version-{number}"
        )))
        .px(px(10.))
        .py(px(7.))
        .rounded_t(px(8.))
        .border_b_2()
        .border_color(if selected {
            theme.primary
        } else {
            theme.transparent
        })
        .text_sm()
        .when(!selected, |this| this.text_color(muted))
        .when(selected, |this| this.font_weight(FontWeight::SEMIBOLD))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.1)))
        .on_click({
            let view = view.clone();
            move |_, _, cx| {
                view.update(cx, |this, cx| this.select_history_version(number, cx));
            }
        })
        .child(format!("v{number}"))
        .into_any_element()
}

/// One run: when it ran, which bot, and what it came to — and under it, when it is the open
/// one, what the box said of every step.
fn history_run_row(
    run: &RecipeRun,
    detail: &RecipeDetail,
    open: bool,
    view: &Entity<RecipesView>,
    muted: Hsla,
    theme: &Theme,
) -> AnyElement {
    let outcome = if run.ok {
        "ok".to_string()
    } else {
        match run.stopped_at {
            Some(step) => format!("stopped at step {step}"),
            None => "stopped".to_string(),
        }
    };
    let error = run.error();
    let id = run.id.clone();
    v_flex()
        .id(SharedString::from(format!("recipe-history-run-{id}")))
        .w_full()
        .gap(px(4.))
        .px(px(10.))
        .py(px(8.))
        .border_b_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.1)))
        .on_click({
            let view = view.clone();
            move |_, _, cx| {
                view.update(cx, |this, cx| this.toggle_history_run(id.clone(), cx));
            }
        })
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .child(
                    Icon::new(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(14.))
                    .text_color(muted),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .truncate()
                        .text_color(if run.ok {
                            theme.foreground
                        } else {
                            theme.danger
                        })
                        .child(format!(
                            "{} · {} · {outcome}",
                            format_time(run.at_ms),
                            detail.bot_name(&run.coworker_id)
                        )),
                )
                // The artifacts a run left behind — its screen recording — go here, once the
                // server keeps them and serves them.
                .child(div().w(px(ARTIFACTS_W)).flex_shrink_0()),
        )
        .when_some(error.clone().filter(|_| !open), |this, error| {
            this.child(
                div()
                    .pl(px(22.))
                    .text_xs()
                    .text_color(theme.danger)
                    .truncate()
                    .child(error),
            )
        })
        .when(open, |this| {
            this.child(history_receipt(run, error, muted, theme))
        })
        .into_any_element()
}

/// A run's receipt, under its row: every step as the box reported it, why it stopped, and the
/// screen it left behind if the run kept one.
fn history_receipt(
    run: &RecipeRun,
    error: Option<String>,
    muted: Hsla,
    theme: &Theme,
) -> AnyElement {
    let steps = run.receipt_steps();
    v_flex()
        .w_full()
        .gap(px(4.))
        .pl(px(22.))
        .pt(px(4.))
        .when(steps.is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("This run kept no step-by-step receipt."),
            )
        })
        .children(steps.iter().enumerate().map(|(index, (ok, step_error))| {
            let said = match (ok, step_error) {
                (true, _) => "ok".to_string(),
                (false, Some(error)) => format!("stopped: {error}"),
                (false, None) => "stopped".to_string(),
            };
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .w(px(STEP_NUMBER_W))
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{}", index + 1)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .truncate()
                        .text_color(if *ok { muted } else { theme.danger })
                        .child(said),
                )
        }))
        .when_some(error, |this, error| {
            this.child(div().text_xs().text_color(theme.danger).child(error))
        })
        .when_some(
            receipt_image(&run.receipt),
            |this, (image, width, height)| {
                let shown_width = SCREENSHOT_WIDTH.min(HISTORY_MODAL_W - 80.);
                let shown_height = shown_width * height.max(1) as f32 / width.max(1) as f32;
                this.child(
                    img(image)
                        .w(px(shown_width))
                        .h(px(shown_height))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(theme.border),
                )
            },
        )
        .into_any_element()
}

/// The screen a receipt kept, when it kept one. The server strips the picture off a run on its
/// way to the store, so this is only ever a receipt that came another way; the box spells the
/// bytes `png_base64` where the run route renames them `base64`, and both are read here.
fn receipt_image(receipt: &Value) -> Option<(std::sync::Arc<gpui_kit::Image>, u32, u32)> {
    let shot = receipt.get("screenshot")?;
    let frame = json!({
        "mime": shot.get("mime").cloned().unwrap_or_else(|| Value::from("image/png")),
        "base64": shot.get("base64").or_else(|| shot.get("png_base64")),
        "width": shot.get("width"),
        "height": shot.get("height"),
    });
    crate::opengrok::ScreenshotSpec::from_frame("recipe-history", "", &frame)
        .map(|spec| (spec.image, spec.width, spec.height))
}

/// The version the page shows: the one the tabs picked, else the newest that can be run, else
/// the newest of all (a recipe whose tape has not been filtered yet).
fn shown_version(detail: &RecipeDetail, picked: Option<u32>) -> Option<&RecipeVersion> {
    if let Some(picked) = picked
        && let Some(found) = detail
            .versions
            .iter()
            .find(|version| version.version == picked)
    {
        return Some(found);
    }
    detail
        .runnable_version()
        .or_else(|| detail.versions.iter().max_by_key(|version| version.version))
}

/// One tab: "v2 filtered", with a mark on the one a run plays. The strip is inert while a
/// draft is open, so an edit is never lost to a click on another version.
fn version_tab(
    version: &RecipeVersion,
    selected: bool,
    current: bool,
    locked: bool,
    view: &Entity<RecipesView>,
    muted: Hsla,
    theme: &Theme,
) -> AnyElement {
    let number = version.version;
    let tab = h_flex()
        .id(SharedString::from(format!("recipe-version-{number}")))
        .items_center()
        .gap(px(6.))
        .px(px(10.))
        .py(px(7.))
        .rounded_t(px(8.))
        .border_b_2()
        .border_color(if selected {
            theme.primary
        } else {
            theme.transparent
        })
        .text_sm()
        .when(!selected, |this| this.text_color(muted))
        .when(selected, |this| this.font_weight(FontWeight::SEMIBOLD))
        .child(format!("v{number} {}", version_kind(version)))
        .when(current, |this| this.child(badge("current", muted, theme)));
    if locked {
        return tab.into_any_element();
    }
    tab.cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.1)))
        .on_click({
            let view = view.clone();
            move |_, _, cx| {
                view.update(cx, |this, cx| this.select_version(number, cx));
            }
        })
        .into_any_element()
}

/// The word after the number in a tab: what the server calls the version, or what it holds
/// when it calls it nothing.
fn version_kind(version: &RecipeVersion) -> &str {
    let kind = version.kind.trim();
    if !kind.is_empty() {
        kind
    } else if version.is_runnable() {
        "steps"
    } else {
        "raw"
    }
}

/// The line under the toolbar: what the version on screen is, and its note.
fn version_line(version: Option<&RecipeVersion>, editing: bool, muted: Hsla) -> AnyElement {
    let Some(version) = version else {
        return div().into_any_element();
    };
    let number = version.version;
    let head = if editing {
        format!("editing a copy of v{number} · unsaved · double click a step to change it")
    } else if version.is_runnable() {
        format!(
            "{} · {} · {}",
            version_kind(version),
            count_of(version.body.steps.len(), "step"),
            format_time(version.created_at_ms)
        )
    } else {
        format!(
            "raw · {} · {}",
            count_of(version.event_count() as usize, "event"),
            format_time(version.created_at_ms)
        )
    };
    let note = version
        .note
        .clone()
        .filter(|note| !note.trim().is_empty() && !editing);
    v_flex()
        .id("recipe-version-line")
        .w_full()
        .gap(px(2.))
        .child(div().text_xs().text_color(muted).child(head))
        .when_some(note, |this, note| {
            this.child(div().text_xs().text_color(muted).child(note))
        })
        .into_any_element()
}

/// The table's header row. It sits above the scroll, so it stays while the steps move.
fn steps_head(muted: Hsla, theme: &Theme) -> AnyElement {
    h_flex()
        .id("recipe-steps-head")
        .w_full()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .py(px(6.))
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .w(px(STEP_NUMBER_W))
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child("#"),
        )
        .child(
            div()
                .w(px(STEP_VERB_W))
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child("Step"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(muted)
                .child("Details"),
        )
        // The column the step's screenshots will hang in, kept empty and unlabelled until the
        // server has them to give.
        .child(div().w(px(ARTIFACTS_W)).flex_shrink_0())
        .into_any_element()
}

/// The tape's tab has no steps to show: how much was taped, and when.
fn tape_frame(version: Option<&RecipeVersion>, muted: Hsla, theme: &Theme) -> AnyElement {
    let (head, when) = match version {
        Some(version) => (
            count_of(version.event_count() as usize, "event"),
            Some(format!("taped {}", format_time(version.created_at_ms))),
        ),
        None => ("Nothing taught yet.".to_string(), None),
    };
    v_flex()
        .id("recipe-steps")
        .w_full()
        .h(px(STEPS_HEIGHT))
        .items_center()
        .justify_center()
        .gap(px(4.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .child(div().text_sm().child(head))
        .when_some(when, |this, when| {
            this.child(div().text_xs().text_color(muted).child(when))
        })
        .when(version.is_some(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("A tape is kept as it was taken; the steps a bot plays are in the versions after it."),
            )
        })
        .into_any_element()
}

/// A step in two columns: what it does, and to what. Read across, a row is a sentence —
/// "click (412, 88)", "type \"example.com\"", "wait 500 ms".
fn step_words(step: &RecipeStep) -> (String, String) {
    match step {
        RecipeStep::Click { x, y, button } => {
            // The left button is the one a click means; only another is worth a word.
            let other = button.as_ref().and_then(|button| match button {
                Value::String(name) if name != "left" => Some(name.clone()),
                Value::Number(number) if number.as_i64() != Some(0) => Some(number.to_string()),
                _ => None,
            });
            match other {
                Some(button) => (
                    "click".to_string(),
                    format!("({x}, {y}) with the {button} button"),
                ),
                None => ("click".to_string(), format!("({x}, {y})")),
            }
        }
        RecipeStep::DoubleClick { x, y } => ("double click".to_string(), format!("({x}, {y})")),
        RecipeStep::Drag { x1, y1, x2, y2 } => {
            ("drag".to_string(), format!("({x1}, {y1}) → ({x2}, {y2})"))
        }
        RecipeStep::Type { text } => ("type".to_string(), format!("{text:?}")),
        RecipeStep::Key { key } => ("key".to_string(), key.clone()),
        RecipeStep::Scroll { x, y, dx, dy } => {
            ("scroll".to_string(), format!("({x}, {y}) by ({dx}, {dy})"))
        }
        RecipeStep::Wait { ms } => ("wait".to_string(), format!("{ms} ms")),
    }
}

/// The tape itself, in the frame the steps use: every event as it was taken, in order and
/// unfiltered, with how long after the first one it came. This is what a raw version holds,
/// and the only place a person can see what was actually taped.
fn tape_table(events: &[RecipeTapeEvent], muted: Hsla, theme: &Theme) -> AnyElement {
    // The tape's own clock is the wall clock; only the distance from its start is readable.
    let start = events.first().map(|event| event.at).unwrap_or_default();
    v_flex()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(events_head(muted, theme))
        .child(
            div()
                .id("recipe-steps")
                .w_full()
                .h(px(STEPS_HEIGHT))
                .overflow_y_scroll()
                .child(v_flex().w_full().children(events.iter().enumerate().map(
                    |(index, event)| {
                        let (verb, details) = tape_words(event);
                        h_flex()
                            .id(SharedString::from(format!("recipe-event-{index}")))
                            .w_full()
                            .items_center()
                            .gap(px(8.))
                            .px(px(10.))
                            .py(px(6.))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .w(px(STEP_NUMBER_W))
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(format!("{}", index + 1)),
                            )
                            .child(
                                div()
                                    .w(px(STEP_VERB_W))
                                    .flex_shrink_0()
                                    .text_sm()
                                    .truncate()
                                    .child(verb),
                            )
                            .child(div().flex_1().min_w_0().text_sm().truncate().child(details))
                            .child(
                                div()
                                    .w(px(EVENT_TIME_W))
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(event_offset(event.at - start)),
                            )
                    },
                ))),
        )
        .into_any_element()
}

/// The tape table's header row, above its scroll so it stays while the events move.
fn events_head(muted: Hsla, theme: &Theme) -> AnyElement {
    h_flex()
        .id("recipe-events-head")
        .w_full()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .py(px(6.))
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .w(px(STEP_NUMBER_W))
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child("#"),
        )
        .child(
            div()
                .w(px(STEP_VERB_W))
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child("Event"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_xs()
                .text_color(muted)
                .child("Details"),
        )
        .child(
            div()
                .w(px(EVENT_TIME_W))
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child("At"),
        )
        .into_any_element()
}

/// One event in two columns, the way a step is: what happened, and where or to what. Read
/// across, a row is a sentence — "down (640, 60) button 1", "wheel (640, 400) by (0, -120)",
/// "keyup Return".
fn tape_words(event: &RecipeTapeEvent) -> (String, String) {
    let kind = event.kind.clone();
    let (x, y) = (event.x, event.y);
    let details = match event.kind.as_str() {
        "down" | "up" => format!("({x}, {y}) button {}", event.button),
        "move" => format!("({x}, {y})"),
        "wheel" => format!("({x}, {y}) by ({}, {})", event.dx, event.dy),
        "keydown" | "keyup" => key_words(event),
        // A kind this app has no word for is still worth its row; what it carries is not
        // worth guessing at.
        _ => String::new(),
    };
    (kind, details)
}

/// The key an event pressed: a letter in quotes, the way `type` shows its text, and a named
/// key as it is. The browser's `code` stands in when there is no `key`.
fn key_words(event: &RecipeTapeEvent) -> String {
    let key = if event.key.is_empty() {
        event.code.as_str()
    } else {
        event.key.as_str()
    };
    if key.is_empty() {
        String::new()
    } else if key.chars().count() == 1 {
        format!("{key:?}")
    } else {
        key.to_string()
    }
}

/// How long after the tape started an event came, as "+1.20 s". Seconds rather than the
/// milliseconds the tape counts in, because a tape runs for minutes and reads better that way.
fn event_offset(ms: i64) -> String {
    format!("+{:.2} s", ms.max(0) as f64 / 1000.)
}

/// A step's parameters as the modal's fields start out, in the order [`StepKind::params`] puts
/// them. A step of one value has none: that one is edited in its own row.
fn step_param_values(step: &RecipeStep) -> Vec<String> {
    match step {
        RecipeStep::Click { x, y, button } => vec![
            x.to_string(),
            y.to_string(),
            match button {
                Some(Value::String(name)) => name.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            },
        ],
        RecipeStep::DoubleClick { x, y } => vec![x.to_string(), y.to_string()],
        RecipeStep::Drag { x1, y1, x2, y2 } => vec![
            x1.to_string(),
            y1.to_string(),
            x2.to_string(),
            y2.to_string(),
        ],
        RecipeStep::Scroll { x, y, dx, dy } => {
            vec![x.to_string(), y.to_string(), dx.to_string(), dy.to_string()]
        }
        RecipeStep::Type { .. } | RecipeStep::Key { .. } | RecipeStep::Wait { .. } => Vec::new(),
    }
}

/// The modal's fields as a step, or why they are not one. A bot plays a recipe on a screen of
/// its own, so a coordinate off that screen is a step that would miss whatever it was aimed at.
fn build_step(
    kind: StepKind,
    typed: &[String],
    screen: RecipeScreen,
) -> Result<RecipeStep, String> {
    let params = kind.params();
    let mut numbers: Vec<i64> = Vec::with_capacity(params.len());
    let mut button: Option<Value> = None;
    for (param, value) in params.iter().zip(typed) {
        match param.kind {
            ParamKind::X | ParamKind::Y | ParamKind::Amount => {
                let number = value
                    .parse::<i64>()
                    .map_err(|_| format!("{} is a whole number.", param.label))?;
                let bound = match param.kind {
                    ParamKind::X => Some(screen.width as i64),
                    ParamKind::Y => Some(screen.height as i64),
                    _ => None,
                };
                if let Some(bound) = bound
                    && !(0..=bound).contains(&number)
                {
                    return Err(format!(
                        "{} must be between 0 and {bound}: this recipe's screen is {} × {}.",
                        param.label, screen.width, screen.height
                    ));
                }
                numbers.push(number);
            }
            ParamKind::Button => button = parse_button(value)?,
        }
    }
    Ok(match kind {
        StepKind::Click => RecipeStep::Click {
            x: numbers[0],
            y: numbers[1],
            button,
        },
        StepKind::DoubleClick => RecipeStep::DoubleClick {
            x: numbers[0],
            y: numbers[1],
        },
        StepKind::Drag => RecipeStep::Drag {
            x1: numbers[0],
            y1: numbers[1],
            x2: numbers[2],
            y2: numbers[3],
        },
        StepKind::Scroll => RecipeStep::Scroll {
            x: numbers[0],
            y: numbers[1],
            dx: numbers[2],
            dy: numbers[3],
        },
        // A step of one value never opens the modal, so its fields are never built from.
        StepKind::Type | StepKind::Key | StepKind::Wait => kind.new_step(screen),
    })
}

/// Which button a click uses: the word the box knows, the number the tape used, or nothing at
/// all, which is the left one.
fn parse_button(value: &str) -> Result<Option<Value>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if let Ok(number) = value.parse::<i64>() {
        return Ok(Some(Value::from(number)));
    }
    match value.to_ascii_lowercase().as_str() {
        "left" | "right" | "middle" => Ok(Some(Value::from(value.to_ascii_lowercase()))),
        _ => Err("A button is left, right, middle, or the number the tape used.".to_string()),
    }
}

/// The last run's one line, and the screen it left behind.
fn run_outcome(run: &RecipeRunOutcome, detail: &RecipeDetail, theme: &Theme) -> AnyElement {
    let color = if run.ok {
        theme.foreground
    } else {
        theme.danger
    };
    v_flex()
        .id("recipe-run-result")
        .w_full()
        .gap(px(6.))
        .pt(px(4.))
        .child(div().text_sm().text_color(color).child(format!(
            "{} · on {}",
            run.headline(),
            detail.bot_name(&run.coworker_id)
        )))
        .when_some(run.image.clone(), |this, (image, width, height)| {
            let shown_width = SCREENSHOT_WIDTH.min(COLUMN_MAX - 28.);
            let shown_height = shown_width * height.max(1) as f32 / width.max(1) as f32;
            this.child(
                img(image)
                    .w(px(shown_width))
                    .h(px(shown_height))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border),
            )
        })
        .into_any_element()
}

/// The one place bots are dealt with: which of the person's bots may run this recipe, and so
/// which one Run may play it on. One control rather than a list of checkboxes beside a row of
/// chips, because checking a bot and choosing a bot were the same choice said twice.
fn bots_section(
    detail: &RecipeDetail,
    busy: bool,
    app: &Entity<AppState>,
    theme: &Theme,
) -> AnyElement {
    let muted = theme.muted_foreground;
    let bots: Vec<(String, String, bool)> = detail
        .my_bots
        .iter()
        .map(|bot| {
            (
                bot.id.clone(),
                bot_label(&bot.name, &bot.id),
                detail.is_granted(&bot.id),
            )
        })
        .collect();
    let summary = granted_summary(&bots);
    let ungranted: Vec<String> = bots
        .iter()
        .filter(|(_, _, granted)| !granted)
        .map(|(id, _, _)| id.clone())
        .collect();
    let granted: Vec<String> = bots
        .iter()
        .filter(|(_, _, granted)| *granted)
        .map(|(id, _, _)| id.clone())
        .collect();
    card(theme)
        .child(section_title("Bots"))
        .child(div().text_xs().text_color(muted).child(
            "A checked bot may run this recipe on its own computer, and Run plays it on one of them.",
        ))
        .when(bots.is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("You have no bots yet."),
            )
        })
        .when(!bots.is_empty(), |this| {
            this.child(
                Popover::new("recipe-bots-popover")
                    .w(px(BOTS_MENU_W))
                    .trigger(
                        // The trigger only opens the panel, so it stays live while a grant is
                        // in flight; it is the checkboxes inside that wait for the answer.
                        Button::new("recipe-bots")
                            .small()
                            .label(summary)
                            .icon(Icon::new(IconName::ChevronDown).size(px(14.))),
                    )
                    .content({
                        let app = app.clone();
                        let theme = theme.clone();
                        move |_, _, _| bots_menu(&bots, &granted, &ungranted, busy, &app, &theme)
                    }),
            )
        })
        .into_any_element()
}

/// The picker's panel: Select all and Deselect all over one checkbox per bot. Checking grants
/// the recipe to that bot and unchecking takes it back, each on its own request.
fn bots_menu(
    bots: &[(String, String, bool)],
    granted: &[String],
    ungranted: &[String],
    busy: bool,
    app: &Entity<AppState>,
    theme: &Theme,
) -> AnyElement {
    v_flex()
        .id("recipe-bots-menu")
        .w_full()
        .gap(px(6.))
        .child(
            h_flex()
                .w_full()
                .gap(px(6.))
                .child(
                    Button::new("recipe-bots-all")
                        .xsmall()
                        .label("Select all")
                        .disabled(busy || ungranted.is_empty())
                        .on_click({
                            let app = app.clone();
                            let ungranted = ungranted.to_vec();
                            move |_, _, cx| {
                                let ungranted = ungranted.clone();
                                app.update(cx, |state, cx| {
                                    state.set_recipe_grants(ungranted, true, cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new("recipe-bots-none")
                        .xsmall()
                        .label("Deselect all")
                        .disabled(busy || granted.is_empty())
                        .on_click({
                            let app = app.clone();
                            let granted = granted.to_vec();
                            move |_, _, cx| {
                                let granted = granted.clone();
                                app.update(cx, |state, cx| {
                                    state.set_recipe_grants(granted, false, cx);
                                });
                            }
                        }),
                ),
        )
        .child(div().w_full().h(px(1.)).bg(theme.border))
        .children(bots.iter().map(|(id, label, granted)| {
            let id = id.clone();
            let app = app.clone();
            Checkbox::new(SharedString::from(format!("recipe-bot-{id}")))
                .checked(*granted)
                .label(label.clone())
                .disabled(busy)
                .on_click(move |checked, _, cx| {
                    let granted = *checked;
                    let id = id.clone();
                    app.update(cx, |state, cx| {
                        state.set_recipe_grant(id, granted, cx);
                    });
                })
        }))
        .into_any_element()
}

/// What the picker's trigger says: "hex", "hex and 2 others", or "No bots" when none is
/// checked.
fn granted_summary(bots: &[(String, String, bool)]) -> String {
    let names: Vec<&str> = bots
        .iter()
        .filter(|(_, _, granted)| *granted)
        .map(|(_, label, _)| label.as_str())
        .collect();
    match names.split_first() {
        None => "No bots".to_string(),
        Some((first, [])) => (*first).to_string(),
        Some((first, rest)) => format!("{first} and {}", count_of(rest.len(), "other")),
    }
}

/// The person's bots this recipe is granted to, in the order the picker lists them.
fn granted_bots(detail: &RecipeDetail) -> Vec<(String, String)> {
    detail
        .my_bots
        .iter()
        .filter(|bot| detail.is_granted(&bot.id))
        .map(|bot| (bot.id.clone(), bot_label(&bot.name, &bot.id)))
        .collect()
}

/// Add step: every kind of step a version may hold, each adding one with sensible defaults and
/// opening its editor. A drop-down rather than a button per kind, because seven buttons over a
/// table is a second toolbar.
fn add_step_menu(full: bool, view: &Entity<RecipesView>) -> AnyElement {
    Button::new("recipe-add-step")
        .small()
        .label("Add step")
        .disabled(full)
        .icon(Icon::new(IconName::ChevronDown).size(px(14.)))
        .dropdown_menu_with_anchor(Anchor::TopLeft, {
            let view = view.clone();
            move |menu, _, _| {
                StepKind::ALL.into_iter().fold(menu, |menu, kind| {
                    let view = view.clone();
                    menu.item(
                        PopupMenuItem::element(move |_, _| {
                            div()
                                .id(SharedString::from(format!(
                                    "recipe-add-step-{}",
                                    kind.slug()
                                )))
                                .w_full()
                                .child(kind.label())
                        })
                        .on_click(move |_, window, cx| {
                            view.update(cx, |this, cx| this.add_step(kind, window, cx));
                        }),
                    )
                })
            }
        })
        .into_any_element()
}

/// Which granted bot Run plays on, when there is more than one to choose between. It hangs off
/// Run rather than standing as a row of its own: the choice only matters at the moment of a run.
fn run_pick_menu(
    granted: Vec<(String, String)>,
    picked: Option<String>,
    view: &Entity<RecipesView>,
) -> AnyElement {
    Button::new("recipe-run-pick")
        .small()
        .primary()
        .icon(Icon::new(IconName::ChevronDown).size(px(14.)))
        .tooltip("Which bot Run plays on")
        .dropdown_menu_with_anchor(Anchor::TopLeft, {
            let view = view.clone();
            move |menu, _, _| {
                granted.iter().fold(menu, |menu, (id, label)| {
                    let view = view.clone();
                    let id = id.clone();
                    menu.item(
                        PopupMenuItem::new(label.clone())
                            .checked(picked.as_deref() == Some(id.as_str()))
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                view.update(cx, |this, cx| {
                                    this.run_bot = Some(id);
                                    cx.notify();
                                });
                            }),
                    )
                })
            }
        })
        .into_any_element()
}

/// What the last run came to, once there has been one. It is its own card because a run's
/// screen is the largest thing on the page and does not belong inside the steps.
fn last_run_section(run: &RecipeRunOutcome, detail: &RecipeDetail, theme: &Theme) -> AnyElement {
    card(theme)
        .child(section_title("Last run"))
        .child(run_outcome(run, detail, theme))
        .into_any_element()
}

/// One row of the list: the name, the description, whose it is, how many versions it has and
/// what its last run came to — and Accept and Decline when it is a share waiting on the
/// person. The whole row opens the recipe.
fn recipe_row(
    recipe: RecipeSummary,
    last_run: Option<RecipeRunNote>,
    me: Option<&str>,
    single_pending: bool,
    app: Entity<AppState>,
    theme: &Theme,
) -> AnyElement {
    let muted = theme.muted_foreground;
    let id = recipe.id.clone();
    let owner = owner_label(&recipe, me);
    let pending = recipe.is_pending_invite();
    let relation = if pending {
        Some("waiting on you")
    } else {
        match recipe.relation {
            RecipeRelation::Mine => None,
            RecipeRelation::Shared => Some("shared with you"),
            RecipeRelation::Invited => Some("invitation"),
            RecipeRelation::None => Some("org"),
        }
    };
    // The one pending share answers to the plain ids; several need the suffix.
    let (accept_id, decline_id) = if single_pending {
        ("recipe-accept".to_string(), "recipe-decline".to_string())
    } else {
        (
            format!("recipe-accept-{id}"),
            format!("recipe-decline-{id}"),
        )
    };
    let run_line = last_run.map(|note| format!("{} · {}", note.label(), format_time(note.at_ms)));
    v_flex()
        .id(SharedString::from(format!("recipe-{id}")))
        .w_full()
        .gap(px(4.))
        .px(px(14.))
        .py(px(12.))
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|s| {
            s.bg(rgb(0x777777).opacity(0.12))
                .border_color(theme.primary)
        })
        .on_click({
            let app = app.clone();
            let id = id.clone();
            move |_, _, cx| {
                app.update(cx, |state, cx| state.open_recipe(id.clone(), cx));
            }
        })
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(if recipe.name.trim().is_empty() {
                            "Untitled task".to_string()
                        } else {
                            recipe.name.clone()
                        }),
                )
                .when_some(relation, |this, label| {
                    this.child(badge(label, muted, theme))
                })
                .child(
                    Icon::new(IconName::ChevronRight)
                        .size(px(14.))
                        .text_color(muted),
                ),
        )
        .when(!recipe.description.trim().is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .truncate()
                    .child(recipe.description.clone()),
            )
        })
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .truncate()
                        .child(format!(
                            "by {owner} · {}",
                            count_of(recipe.latest_version as usize, "version")
                        )),
                )
                .when_some(run_line, |this, line| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(line),
                    )
                }),
        )
        .when(pending, |this| {
            this.child(
                h_flex()
                    .gap(px(8.))
                    .pt(px(4.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(answer_button(
                        SharedString::from(accept_id),
                        "Accept",
                        true,
                        id.clone(),
                        false,
                        app.clone(),
                    ))
                    .child(answer_button(
                        SharedString::from(decline_id),
                        "Decline",
                        false,
                        id.clone(),
                        false,
                        app.clone(),
                    )),
            )
        })
        .into_any_element()
}

/// Accept or Decline a shared recipe; the click stays off the row under it.
fn answer_button(
    id: impl Into<ElementId>,
    label: &'static str,
    accept: bool,
    recipe_id: String,
    busy: bool,
    app: Entity<AppState>,
) -> Button {
    let button = Button::new(id)
        .small()
        .label(label)
        .disabled(busy)
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            app.update(cx, |state, cx| {
                state.answer_recipe_share(recipe_id.clone(), accept, cx);
            });
        });
    if accept { button.primary() } else { button }
}

fn filter_chip(chip: RecipeFilter, current: RecipeFilter, app: Entity<AppState>) -> Button {
    let button = Button::new(chip.element_id())
        .small()
        .label(chip.label())
        .on_click(move |_, _, cx| {
            app.update(cx, |state, cx| state.set_recipes_filter(chip, cx));
        });
    if chip == current {
        button.primary()
    } else {
        button
    }
}

/// What the title bar shows while the Recipes page is open: a back chevron when a recipe is
/// open, the title, and what the page is doing. The app has ONE header row and it is the title
/// bar, so the page itself draws none.
pub fn recipes_header(app: Entity<AppState>, theme: &Theme, cx: &App) -> AnyElement {
    let state = app.read(cx);
    let open = state.recipe_open.is_some() || state.recipe_open_id.is_some();
    let title = state
        .recipe_open
        .as_ref()
        .map(|detail| detail.recipe.name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            if open {
                "Recipe".to_string()
            } else {
                "Recipes".to_string()
            }
        });
    let note = if open {
        state.recipe_busy.clone()
    } else {
        state.recipes_loading.then(|| "Loading…".to_string())
    };
    let back: Option<Rc<dyn Fn(&mut App)>> = open.then(|| {
        let app = app.clone();
        Rc::new(move |cx: &mut App| {
            app.update(cx, |state, cx| state.close_recipe(cx));
        }) as Rc<dyn Fn(&mut App)>
    });
    page_header(back, title, note, theme.muted_foreground).into_any_element()
}

/// The header's content: a back chevron when there is somewhere to go back to, the title, and
/// what the page is doing on the right.
fn page_header(
    back: Option<Rc<dyn Fn(&mut App)>>,
    title: String,
    note: Option<String>,
    muted: Hsla,
) -> impl IntoElement {
    h_flex()
        .id("recipes-header")
        // Sized by what is left, not by the whole span: `w_full` here took the title bar's
        // chat span entire and pushed whatever followed it over the right pane's header.
        .flex_1()
        .min_w_0()
        .h_full()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .child(
            h_flex()
                .min_w_0()
                .gap(px(6.))
                .items_center()
                .when_some(back, |this, back| {
                    this.child(
                        div()
                            .id("recipe-back")
                            .size(px(28.))
                            .flex_shrink_0()
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| back(cx))
                            .child(Icon::default().path("icons/chevron-left.svg").size(px(16.))),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(title),
                ),
        )
        .when_some(note, |this, note| {
            this.child(
                div()
                    .id("recipe-busy")
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(note),
            )
        })
}

/// "Delete Open the mail?" — the question, what it means, Cancel and Delete. Click outside or
/// Cancel closes it; Delete does the thing.
pub fn recipe_delete_overlay(
    app: Entity<AppState>,
    name: String,
    theme: &Theme,
) -> impl IntoElement {
    let title = if name.trim().is_empty() {
        "Delete this recipe?".to_string()
    } else {
        format!("Delete {name}?")
    };
    div()
        .id("recipe-delete-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::black().opacity(0.32))
        .on_mouse_down(MouseButton::Left, {
            let app = app.clone();
            move |_, _, cx| {
                app.update(cx, |state, cx| state.close_recipe_delete_confirm(cx));
            }
        })
        .child(
            v_flex()
                .id("recipe-delete-confirm")
                .w(px(420.))
                .bg(theme.popover)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.border)
                .rounded(px(14.))
                .shadow_lg()
                .px(px(20.))
                .py(px(18.))
                .gap(px(10.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    "Every version goes with it, and the bots and people it was shared with lose it. This cannot be undone.",
                ))
                .child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap(px(8.))
                        .pt(px(6.))
                        .child(
                            Button::new("recipe-delete-cancel")
                                .label("Cancel")
                                .on_click({
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.close_recipe_delete_confirm(cx)
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("recipe-delete-yes")
                                .danger()
                                .label("Delete")
                                .on_click({
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.confirm_recipe_delete(cx)
                                        });
                                    }
                                }),
                        ),
                ),
        )
}

fn card(theme: &Theme) -> Div {
    v_flex()
        .w_full()
        .gap(px(8.))
        .px(px(14.))
        .py(px(12.))
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
}

fn column() -> Div {
    v_flex()
        .w_full()
        .max_w(px(COLUMN_MAX))
        .gap(px(12.))
        .pt(px(4.))
}

/// The column in the middle of the slot, however wide the slot is.
fn centered(column: Div) -> Div {
    div().w_full().flex().justify_center().child(column)
}

fn section_title(label: &'static str) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(label)
}

fn field_label(label: &'static str, muted: Hsla) -> Div {
    div().text_xs().text_color(muted).child(label)
}

fn badge(label: &'static str, muted: Hsla, theme: &Theme) -> Div {
    div()
        .flex_shrink_0()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .text_xs()
        .text_color(muted)
        .child(label)
}

/// "you" for the person's own, else what the server names the owner by.
fn owner_label(recipe: &RecipeSummary, me: Option<&str>) -> String {
    if recipe.is_mine() || (me.is_some() && me == Some(recipe.owner_id.as_str())) {
        "you".to_string()
    } else if recipe.owner_id.trim().is_empty() {
        "someone else".to_string()
    } else {
        recipe.owner_id.clone()
    }
}

fn bot_label(name: &str, id: &str) -> String {
    if name.trim().is_empty() {
        id.to_string()
    } else {
        name.to_string()
    }
}

fn count_of(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// "Sep 16, 4:05 PM" from a server millisecond time, in the person's zone.
fn format_time(at_ms: i64) -> String {
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_millis(at_ms.max(0) as u64);
    chrono::DateTime::<chrono::Local>::from(at)
        .format("%b %-d, %-I:%M %p")
        .to_string()
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob: `use super::*` would pull GPUI's `test` attribute in over
    // the one the test harness wants.
    use super::{
        RecipeDetail, RecipeScreen, RecipeStep, RecipeTapeEvent, RecipeVersion, StepKind,
        build_step, event_offset, granted_summary, runs_of, shown_version, step_param_values,
        step_words, tape_words, version_kind,
    };
    use serde_json::{Value, json};

    fn version(number: u32, kind: &str) -> RecipeVersion {
        serde_json::from_value(json!({ "version": number, "kind": kind })).unwrap()
    }

    fn detail_of(versions: &[(u32, &str)]) -> RecipeDetail {
        let versions: Vec<Value> = versions
            .iter()
            .map(|(number, kind)| json!({ "version": number, "kind": kind }))
            .collect();
        serde_json::from_value(json!({
            "recipe": { "id": "rcp_1" },
            "versions": versions,
        }))
        .unwrap()
    }

    #[test]
    fn a_row_of_the_table_reads_as_a_sentence() {
        let row = |step: &RecipeStep| {
            let (verb, details) = step_words(step);
            format!("{verb} {details}")
        };
        assert_eq!(
            row(&RecipeStep::Click {
                x: 412,
                y: 88,
                button: None
            }),
            "click (412, 88)"
        );
        assert_eq!(
            row(&RecipeStep::DoubleClick { x: 500, y: 300 }),
            "double click (500, 300)"
        );
        assert_eq!(
            row(&RecipeStep::Drag {
                x1: 10,
                y1: 10,
                x2: 200,
                y2: 40
            }),
            "drag (10, 10) → (200, 40)"
        );
        assert_eq!(
            row(&RecipeStep::Type {
                text: "example.com".to_string()
            }),
            "type \"example.com\""
        );
        assert_eq!(
            row(&RecipeStep::Key {
                key: "Return".to_string()
            }),
            "key Return"
        );
        assert_eq!(
            row(&RecipeStep::Scroll {
                x: 640,
                y: 400,
                dx: 0,
                dy: -120
            }),
            "scroll (640, 400) by (0, -120)"
        );
        assert_eq!(row(&RecipeStep::Wait { ms: 500 }), "wait 500 ms");
    }

    #[test]
    fn a_row_of_the_tape_reads_as_a_sentence() {
        let event = |kind: &str, json: Value| -> RecipeTapeEvent {
            let mut value = json;
            value["kind"] = Value::from(kind);
            serde_json::from_value(value).unwrap()
        };
        let row = |event: &RecipeTapeEvent| {
            let (verb, details) = tape_words(event);
            format!("{verb} {details}").trim_end().to_string()
        };
        assert_eq!(
            row(&event("down", json!({"x": 640, "y": 60, "button": 1}))),
            "down (640, 60) button 1"
        );
        assert_eq!(
            row(&event("up", json!({"x": 640, "y": 60, "button": 1}))),
            "up (640, 60) button 1"
        );
        assert_eq!(
            row(&event("move", json!({"x": 700, "y": 120}))),
            "move (700, 120)"
        );
        assert_eq!(
            row(&event(
                "wheel",
                json!({"x": 640, "y": 400, "dx": 0, "dy": -120})
            )),
            "wheel (640, 400) by (0, -120)"
        );
        assert_eq!(
            row(&event("keydown", json!({"key": "e", "code": "KeyE"}))),
            "keydown \"e\""
        );
        assert_eq!(
            row(&event("keyup", json!({"key": "Return"}))),
            "keyup Return"
        );
        assert_eq!(
            row(&event("keydown", json!({"code": "Enter"}))),
            "keydown Enter",
            "the code stands in when the tape has no key"
        );
        assert_eq!(
            row(&event("blur", json!({}))),
            "blur",
            "an event this app has no word for still has its row"
        );
    }

    #[test]
    fn an_event_says_how_long_after_the_tape_started_it_came() {
        assert_eq!(event_offset(0), "+0.00 s");
        assert_eq!(event_offset(1200), "+1.20 s");
        assert_eq!(event_offset(80), "+0.08 s");
    }

    #[test]
    fn the_bots_trigger_says_which_bots_are_checked() {
        let bots = |checked: &[bool]| -> Vec<(String, String, bool)> {
            ["hex", "ada", "bo"]
                .iter()
                .zip(checked)
                .map(|(name, on)| (format!("cw_{name}"), name.to_string(), *on))
                .collect()
        };
        assert_eq!(granted_summary(&bots(&[false, false, false])), "No bots");
        assert_eq!(granted_summary(&bots(&[true, false, false])), "hex");
        assert_eq!(
            granted_summary(&bots(&[true, true, false])),
            "hex and 1 other"
        );
        assert_eq!(
            granted_summary(&bots(&[true, true, true])),
            "hex and 2 others"
        );
    }

    #[test]
    fn the_modal_fills_from_the_step_and_builds_one_back() {
        let screen = RecipeScreen {
            width: 1280,
            height: 800,
        };
        let drag = RecipeStep::Drag {
            x1: 10,
            y1: 10,
            x2: 200,
            y2: 40,
        };
        assert_eq!(step_param_values(&drag), vec!["10", "10", "200", "40"]);
        assert_eq!(
            build_step(StepKind::Drag, &values(&["10", "10", "200", "40"]), screen).unwrap(),
            drag
        );
        assert_eq!(
            build_step(StepKind::Click, &values(&["412", "88", ""]), screen).unwrap(),
            RecipeStep::Click {
                x: 412,
                y: 88,
                button: None
            },
            "no button named is the left one"
        );
        assert_eq!(
            build_step(StepKind::Click, &values(&["412", "88", "Right"]), screen).unwrap(),
            RecipeStep::Click {
                x: 412,
                y: 88,
                button: Some(Value::from("right"))
            }
        );
        assert_eq!(
            build_step(
                StepKind::Scroll,
                &values(&["640", "400", "0", "-120"]),
                screen
            )
            .unwrap(),
            RecipeStep::Scroll {
                x: 640,
                y: 400,
                dx: 0,
                dy: -120
            },
            "a scroll may go either way, however far"
        );
    }

    #[test]
    fn a_value_off_the_screen_is_refused_with_its_reason() {
        let screen = RecipeScreen {
            width: 1280,
            height: 800,
        };
        let refused = |kind, typed: &[&str]| build_step(kind, &values(typed), screen).unwrap_err();
        assert!(
            refused(StepKind::Click, &["1400", "88", ""]).contains("1280"),
            "the reason names the screen"
        );
        assert!(refused(StepKind::DoubleClick, &["10", "-1"]).contains("Y"));
        assert!(refused(StepKind::DoubleClick, &["ten", "10"]).contains("whole number"));
        assert!(refused(StepKind::DoubleClick, &["", "10"]).contains("X"));
        assert!(refused(StepKind::Click, &["10", "10", "sideways"]).contains("button"));
    }

    fn values(typed: &[&str]) -> Vec<String> {
        typed.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn a_history_tab_holds_the_newest_five_runs_of_its_version() {
        let run = |version: u32, at_ms: i64| json!({"id": format!("rrun_{version}_{at_ms}"), "version": version, "atMs": at_ms});
        let detail: RecipeDetail = serde_json::from_value(json!({
            "recipe": {"id": "rcp_1"},
            "versions": [{"version": 1, "kind": "raw"}, {"version": 2, "kind": "filtered"}],
            "runs": [
                run(2, 10), run(2, 60), run(2, 20), run(2, 50), run(2, 30), run(2, 40),
                run(1, 5),
            ],
        }))
        .unwrap();
        let at = |version| {
            runs_of(&detail, version)
                .into_iter()
                .map(|run| run.at_ms)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            at(2),
            vec![60, 50, 40, 30, 20],
            "newest first, five at most"
        );
        assert_eq!(at(1), vec![5], "a tab holds only its own version's runs");
        assert!(runs_of(&detail, 3).is_empty());
    }

    #[test]
    fn a_tab_says_the_number_and_what_the_version_holds() {
        assert_eq!(version_kind(&version(1, "raw")), "raw");
        assert_eq!(version_kind(&version(2, "filtered")), "filtered");
        assert_eq!(version_kind(&version(3, "")), "steps");
    }

    #[test]
    fn the_page_opens_on_the_version_a_run_plays() {
        let detail = detail_of(&[(1, "raw"), (2, "filtered"), (3, "edited")]);
        let number = |picked| shown_version(&detail, picked).map(|version| version.version);
        assert_eq!(number(None), Some(3));
        assert_eq!(number(Some(1)), Some(1), "a picked tab wins");
        assert_eq!(number(Some(9)), Some(3), "a version that is gone does not");
        let taped = detail_of(&[(1, "raw")]);
        assert_eq!(
            shown_version(&taped, None).map(|version| version.version),
            Some(1),
            "a tape with nothing filtered from it yet is still shown"
        );
    }
}
