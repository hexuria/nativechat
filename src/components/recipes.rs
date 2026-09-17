//! The Recipes page: the tasks taught on a bot's screen. The list says whose each one is and
//! what became of it last time; one recipe's own page puts its versions in a tab strip, the
//! steps of the chosen version in a table that scrolls inside a frame of fixed height, and
//! what a person may do with it in a toolbar that stays above that table.
//!
//! The page takes the chat's slot when the dock opens it, and the window's title bar carries
//! its header (see [`recipes_header`]), so the body starts straight under the bar.

use std::rc::Rc;

use crate::chrome::{HEADER_PX, chat_column_width};
use crate::components::fields::field_input;
use crate::components::multi_select::{
    MultiSelectEvent, MultiSelectIds, MultiSelectOption, MultiSelectState,
};
use crate::components::title_bar::window_drag;
use crate::icons::NativeIcon;
use crate::opengrok::{
    RecipeDetail, RecipeParameter, RecipeRelation, RecipeRun, RecipeScreen, RecipeShare,
    RecipeShareTarget, RecipeStep, RecipeSummary, RecipeTapeEvent, RecipeVersion,
};
use crate::state::{AppState, RecipeFilter, RecipeRunNote, RecipeRunOutcome, RightPane};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
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
/// The share modal: wide enough for an email address on one line and for the bots picker's
/// panel under its trigger.
const SHARE_MODAL_W: f32 = 460.;
/// The share modal's body, so the modal is the same height whichever tab is on and the bots
/// picker's panel has somewhere to hang.
const SHARE_BODY_HEIGHT: f32 = 300.;
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
/// The air above the filter chips, under the window's title bar, and the same again below
/// them: the chips were sitting on the bar and the first card was sitting on the chips.
const LIST_GAP: f32 = 50.;
/// Under the last card, so the list does not end flush with the bottom of the window.
const LIST_BOTTOM_PAD: f32 = 32.;
/// A card's side padding, and what it spends instead in a column too narrow to afford it.
const ROW_PAD: f32 = 18.;
const ROW_PAD_NARROW: f32 = 14.;
/// The gap between two cards. Wider than the gaps inside one, so a card reads as a thing of
/// its own rather than as another line of the same block.
const ROW_GAP: f32 = 12.;
/// Below this column width a name has no room beside a badge, so the badge drops down to the
/// facts and the name keeps the line to itself.
const ROW_STACK_W: f32 = 520.;
/// The empty state, however short the page is: enough for the icon, the headline and the
/// sentence with air around them, and it scrolls rather than being squeezed below that.
const EMPTY_MIN_H: f32 = 260.;
/// The disc the page's own icon sits in, above the empty state's headline.
const EMPTY_ICON_BOX: f32 = 56.;
/// A line of the empty state's copy: short enough to be taken in at a glance, whatever the
/// window is doing.
const EMPTY_COPY_MAX: f32 = 380.;
/// The spinner that stands where the list or the recipe will: big enough to read as the page
/// working, small enough not to read as the page's content.
const SPINNER_PX: f32 = 28.;
/// How long the About card says "Saved" after an auto-save lands. Long enough to be seen,
/// short enough that it is gone before the person wonders what it is still doing there.
const SAVED_FOR: std::time::Duration = std::time::Duration::from_millis(1800);

/// What a step does, apart from what it does it to: what Add step offers, and what the modal
/// asks for when a step of that kind is added or opened.
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

    /// The parameters the modal asks for, one labelled field each. Every kind has at least
    /// one: a step is only ever edited in the modal, so a kind with no field would be a step
    /// that cannot be changed.
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
        const TYPE: [StepParam; 1] = [StepParam::new("text", "Text", ParamKind::Text)];
        const KEY: [StepParam; 1] = [StepParam::new("key", "Key", ParamKind::Key)];
        const WAIT: [StepParam; 1] = [StepParam::new("ms", "Milliseconds", ParamKind::Wait)];
        match self {
            Self::Click => &CLICK,
            Self::DoubleClick => &PLACE,
            Self::Drag => &DRAG,
            Self::Scroll => &SCROLL,
            Self::Type => &TYPE,
            Self::Key => &KEY,
            Self::Wait => &WAIT,
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
    /// The text to type, which may be anything at all — spaces at either end included, since
    /// those are as much a part of what is typed as the letters between them.
    Text,
    /// A key by the name the box knows it as, such as Return.
    Key,
    /// How long to wait, in milliseconds. Past the server's cap the wait is shortened rather
    /// than refused, and the page says so.
    Wait,
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

/// What the modal is editing, and of what kind, so Save knows what to build from the fields
/// and where to put it.
#[derive(Debug, Clone, Copy)]
struct StepModal {
    /// The row under edit, or None while a step is being added: a step added from the picker
    /// joins the draft when it is saved, so Cancel leaves no half-filled row behind.
    index: Option<usize>,
    kind: StepKind,
}

/// The tabs of the share modal: the three ways a recipe goes somewhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShareTab {
    Bots,
    Org,
    Email,
}

impl ShareTab {
    /// Bots first: it is the one tab everyone who can see the recipe gets, since only an owner
    /// may pass a recipe on.
    const ALL: [Self; 3] = [Self::Bots, Self::Org, Self::Email];

    fn label(self) -> &'static str {
        match self {
            Self::Bots => "Bots",
            Self::Org => "Org",
            Self::Email => "Email",
        }
    }

    /// The word in the element id, `recipe-share-tab-bots`.
    fn slug(self) -> &'static str {
        match self {
            Self::Bots => "bots",
            Self::Org => "org",
            Self::Email => "email",
        }
    }
}

pub struct RecipesView {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    description_input: Entity<InputState>,
    share_email_input: Entity<InputState>,
    note_input: Entity<InputState>,
    /// The bots picker in the share modal. It holds no truth of its own: it says what was
    /// ticked and the server's answer comes back as the detail's grants.
    bots: Entity<MultiSelectState>,
    /// The recipe whose name and description the fields hold; refilled when another opens.
    synced_id: Option<String>,
    /// The version the tabs have chosen. None until someone picks one, so a page that has
    /// just opened stands on the version a run would play.
    selected_version: Option<u32>,
    /// The steps under edit (the owner's Edit steps), until Save as new version or Cancel.
    draft: Option<Vec<RecipeStep>>,
    /// The step the modal is on: a row of the draft, or one being added.
    step_modal: Option<StepModal>,
    /// The modal's fields, one per parameter of the longest step there is. They are reused
    /// rather than made per step, so opening the modal costs no entities.
    step_fields: Vec<Entity<InputState>>,
    /// Why the draft cannot be saved as it is.
    draft_error: Option<String>,
    /// The share modal is open over the page, and on which tab.
    share_open: bool,
    share_tab: ShareTab,
    /// The modal was opened from a row of the list, so closing it puts the person back on the
    /// list rather than on the recipe the row stood for.
    share_from_list: bool,
    /// The version whose Delete version is waiting on a yes.
    delete_version_confirm: Option<u32>,
    /// The bot Run plays on; the first granted one until the person picks.
    run_bot: Option<String>,
    /// The history modal is open over the page.
    history_open: bool,
    /// The version whose runs the history is showing; None until someone picks a tab, so an
    /// opened history stands on the version a run plays.
    history_version: Option<u32>,
    /// The run whose receipt is open under its row.
    history_run: Option<String>,
    /// An auto-save of the name and description is in flight, so the About card can say so
    /// and the answer to it can be turned into a word or left to the page's error line.
    saving: bool,
    /// The About card is saying "Saved", until its tick takes the word away.
    saved: bool,
    /// The tick that takes it away. Held rather than detached, so a second save drops the
    /// first one's tick instead of letting it clear the newer word.
    saved_tick: Option<Task<()>>,
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
        let step_fields: Vec<Entity<InputState>> = (0..STEP_FIELDS)
            .map(|_| cx.new(|cx| InputState::new(window, cx)))
            .collect();
        let bots = cx.new(|cx| {
            MultiSelectState::new(
                MultiSelectIds {
                    trigger: "recipe-bots".into(),
                    search: "recipe-bots-search".into(),
                    select_all: "recipe-bots-all".into(),
                    clear_all: "recipe-bots-none".into(),
                    option_prefix: "recipe-bot-".into(),
                },
                window,
                cx,
            )
            .placeholder("No bots")
        });
        cx.observe(&state, |this, _, cx| {
            this.settle_save(cx);
            cx.notify();
        })
        .detach();
        // A tick in the picker is a grant, and an untick takes it back; the server's answer is
        // what the picker is filled from next.
        cx.subscribe(&bots, |this, _, event: &MultiSelectEvent, cx| {
            let app = this.state.clone();
            match event {
                MultiSelectEvent::Toggled { id, selected } => {
                    let (id, selected) = (id.clone(), *selected);
                    app.update(cx, |state, cx| state.set_recipe_grant(id, selected, cx));
                }
                MultiSelectEvent::Bulk { ids, selected } => {
                    let (ids, selected) = (ids.clone(), *selected);
                    app.update(cx, |state, cx| state.set_recipe_grants(ids, selected, cx));
                }
            }
        })
        .detach();
        // There is no Save button on the About card: leaving a field is the save, so the
        // fields say when they were left and what they hold is compared with the server's.
        for field in [&name_input, &description_input] {
            cx.subscribe(field, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.save_about(cx);
                }
            })
            .detach();
        }
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
            bots,
            synced_id: None,
            selected_version: None,
            draft: None,
            step_modal: None,
            step_fields,
            draft_error: None,
            share_open: false,
            share_tab: ShareTab::Bots,
            share_from_list: false,
            delete_version_confirm: None,
            run_bot: None,
            history_open: false,
            history_version: None,
            history_run: None,
            saving: false,
            saved: false,
            saved_tick: None,
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
        self.step_modal = None;
        self.draft_error = None;
        self.delete_version_confirm = None;
        self.run_bot = None;
        self.history_open = false;
        self.history_version = None;
        self.history_run = None;
        self.saving = false;
        self.saved = false;
        self.saved_tick = None;
        // The share modal is left alone: the share icon on a row of the list opens a recipe
        // and the modal over it, and the detail landing here is the answer to that click.
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

    /// Save the name and the description when a field is left holding something other than
    /// what the server last sent. There is no Save button on the card any more: tabbing away
    /// is the save, and saving on every keystroke would be one request per letter.
    fn save_about(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).value().trim().to_string();
        let description = self.description_input.read(cx).value().trim().to_string();
        {
            let state = self.state.read(cx);
            let worth_saving = state.recipe_open.as_ref().is_some_and(|detail| {
                // Only an owner may rename, and only an owner's card draws these fields.
                detail.recipe.is_mine() && about_edited(&detail.recipe, &name, &description)
            });
            // One request at a time: a second over the first would race it for which answer
            // the page ends up standing on.
            if !worth_saving || state.recipe_busy.is_some() {
                return;
            }
        }
        self.state.update(cx, |state, cx| {
            state.rename_open_recipe(name, description, cx)
        });
        // The page is only waiting on a save if the request actually went.
        self.saving = self.state.read(cx).recipe_busy.is_some();
        self.saved = false;
        self.saved_tick = None;
        cx.notify();
    }

    /// What became of that save, once it is no longer in flight: "Saved" for a moment, or
    /// nothing at all when it was refused — the reason is on the page's error line by then,
    /// and the field still holds what the person typed.
    fn settle_save(&mut self, cx: &mut Context<Self>) {
        if !self.saving || self.state.read(cx).recipe_busy.is_some() {
            return;
        }
        self.saving = false;
        if self.state.read(cx).recipe_error.is_some() {
            return;
        }
        self.saved = true;
        self.saved_tick = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVED_FOR).await;
            let _ = this.update(cx, |this, cx| {
                this.saved = false;
                this.saved_tick = None;
                cx.notify();
            });
        }));
    }

    /// What the About card's meta line says about the save, while there is anything to say.
    fn save_note(&self) -> Option<&'static str> {
        if self.saving {
            Some("Saving…")
        } else if self.saved {
            Some("Saved")
        } else {
            None
        }
    }

    /// The picker follows the open recipe: the person's bots as its options, the ones this
    /// recipe is granted to as its selection, and nothing tickable while a grant is in flight.
    fn sync_bots(&self, cx: &mut Context<Self>) {
        let (options, selected, busy) = {
            let state = self.state.read(cx);
            match state.recipe_open.as_ref() {
                Some(detail) => (
                    detail
                        .my_bots
                        .iter()
                        .map(|bot| MultiSelectOption::new(&bot.id, bot_label(&bot.name, &bot.id)))
                        .collect(),
                    detail
                        .my_bots
                        .iter()
                        .filter(|bot| detail.is_granted(&bot.id))
                        .map(|bot| bot.id.clone())
                        .collect(),
                    state.recipe_busy.is_some(),
                ),
                None => (Vec::new(), Vec::new(), state.recipe_loading),
            }
        };
        self.bots
            .update(cx, |picker, cx| picker.sync(options, selected, busy, cx));
    }

    /// Open the share modal over the page, on Bots. Only one thing stands over the page at a
    /// time, so whatever else was open goes.
    fn open_share(&mut self, from_list: bool, cx: &mut Context<Self>) {
        self.share_open = true;
        self.share_from_list = from_list;
        self.share_tab = ShareTab::Bots;
        self.history_open = false;
        self.step_modal = None;
        self.delete_version_confirm = None;
        cx.notify();
    }

    /// Close it, and put someone who opened it from a row back on the list they opened it from.
    fn close_share(&mut self, cx: &mut Context<Self>) {
        self.share_open = false;
        self.bots.update(cx, |picker, cx| picker.close(cx));
        if self.share_from_list {
            self.share_from_list = false;
            self.state.update(cx, |state, cx| state.close_recipe(cx));
        }
        cx.notify();
    }

    fn select_share_tab(&mut self, tab: ShareTab, cx: &mut Context<Self>) {
        self.share_tab = tab;
        self.bots.update(cx, |picker, cx| picker.close(cx));
        cx.notify();
    }

    fn select_version(&mut self, version: u32, cx: &mut Context<Self>) {
        self.selected_version = Some(version);
        cx.notify();
    }

    fn ask_delete_version(&mut self, version: u32, cx: &mut Context<Self>) {
        self.delete_version_confirm = Some(version);
        cx.notify();
    }

    fn cancel_delete_version(&mut self, cx: &mut Context<Self>) {
        self.delete_version_confirm = None;
        cx.notify();
    }

    /// Delete the version the dialog asked about. The tabs let go of it at the same moment, so
    /// what is left is shown on the newest version rather than on a tab that is no longer there.
    fn confirm_delete_version(&mut self, cx: &mut Context<Self>) {
        let Some(version) = self.delete_version_confirm.take() else {
            return;
        };
        self.selected_version = None;
        self.state
            .update(cx, |state, cx| state.delete_recipe_version(version, cx));
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

    /// Start editing: a copy of the filtered version, which is what an edit is meant to start
    /// from — an edit of an edit carries whatever the last one got wrong. The one exception is
    /// a tab the person went to themselves: if they are standing on an edited version, that is
    /// the one they asked to change. The tab the page opens on does not count, since that is
    /// the newest edit and starting from it is the thing this avoids.
    fn begin_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = {
            let state = self.state.read(cx);
            let Some(detail) = state.recipe_open.as_ref() else {
                return;
            };
            self.selected_version
                .and_then(|number| version_of(detail, number))
                .filter(|version| version.is_edited())
                .or_else(|| filtered_version(detail))
                .or_else(|| detail.runnable_version())
                .map(|version| (version.version, version.body.steps.clone()))
        };
        let Some((version, steps)) = picked else {
            return;
        };
        // The tab moves to the version the draft came from, so the line under the toolbar says
        // which one that was.
        self.selected_version = Some(version);
        self.draft = Some(steps);
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

    /// Add step: the modal on a step of the chosen kind, filled with sensible defaults. It
    /// joins the draft when it is saved, so a step that was thought better of leaves no row at
    /// the bottom of a table the person would have to scroll to to be rid of.
    fn add_step(&mut self, kind: StepKind, window: &mut Window, cx: &mut Context<Self>) {
        let screen = self.screen(cx);
        let Some(draft) = self.draft.as_ref() else {
            return;
        };
        if draft.len() >= MAX_STEPS {
            self.draft_error = Some(format!("A version holds at most {MAX_STEPS} steps."));
            cx.notify();
            return;
        }
        self.draft_error = None;
        self.fill_step_modal(
            StepModal { index: None, kind },
            &kind.new_step(screen),
            window,
            cx,
        );
    }

    /// A row's editor, which is the modal whatever the step is: a field in the table was a
    /// second way of editing that only some kinds of step had, and it sat in the part of the
    /// page that scrolls.
    fn open_step_editor(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self
            .draft
            .as_ref()
            .and_then(|draft| draft.get(index))
            .cloned()
        else {
            return;
        };
        let kind = StepKind::of(&step);
        self.fill_step_modal(
            StepModal {
                index: Some(index),
                kind,
            },
            &step,
            window,
            cx,
        );
    }

    /// Open the modal on a step: one field per parameter, filled with what the step holds.
    fn fill_step_modal(
        &mut self,
        modal: StepModal,
        step: &RecipeStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (field, value) in self.step_fields.iter().zip(step_param_values(step)) {
            field.update(cx, |input, cx| {
                input.set_value(value, window, cx);
            });
        }
        // The first field takes the caret, so the modal can be filled in and saved without
        // reaching for the mouse again.
        if let Some(first) = self.step_fields.first() {
            first.update(cx, |input, cx| input.focus(window, cx));
        }
        self.step_modal = Some(modal);
        self.draft_error = None;
        cx.notify();
    }

    /// The modal's Save: the fields as a step, in the row it was opened on or at the end of
    /// the draft when it was opened by Add step. A refusal shows on the page's error line with
    /// the modal left open on what was typed; a wait that was only shortened is saved, and the
    /// same line says so.
    fn save_step_modal(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.step_modal else {
            return;
        };
        let screen = self.screen(cx);
        let typed: Vec<String> = self
            .step_fields
            .iter()
            .map(|field| field.read(cx).value().to_string())
            .collect();
        let (step, note) = match build_step(modal.kind, &typed, screen) {
            Ok(built) => built,
            Err(reason) => {
                self.draft_error = Some(reason);
                cx.notify();
                return;
            }
        };
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        match modal.index {
            Some(index) => {
                if let Some(slot) = draft.get_mut(index) {
                    *slot = step;
                }
            }
            None => {
                if draft.len() >= MAX_STEPS {
                    self.draft_error = Some(format!("A version holds at most {MAX_STEPS} steps."));
                    cx.notify();
                    return;
                }
                draft.push(step);
            }
        }
        self.step_modal = None;
        self.draft_error = note;
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
        self.step_modal = None;
        self.draft_error = None;
        cx.notify();
    }
}

impl Render for RecipesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_fields(window, cx);
        self.sync_bots(cx);
        let theme = cx.theme().clone();
        let open = self.state.read(cx).recipe_open_id.is_some();
        let view = cx.entity();
        v_flex()
            .id("page-recipes")
            .size_full()
            // The modals hang off the page, so they are centred on the page's own slot.
            .relative()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(if open {
                self.detail(&theme, cx)
            } else {
                self.list(window, &theme, cx)
            })
            .when(self.share_open, |this| {
                this.child(self.share_overlay(&view, &theme, cx))
            })
            .when_some(self.step_modal, |this, modal| {
                this.child(self.step_modal_overlay(modal, &view, &theme, cx))
            })
            .when_some(self.delete_version_confirm, |this, version| {
                this.child(self.delete_version_overlay(version, &view, &theme))
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
    /// The list: the filter chips, then a card per recipe in the page's centred column. The
    /// chips stand clear of the title bar above them and of the first card below them, and an
    /// empty list takes the rest of the page and stands in the middle of it.
    fn list(&self, window: &Window, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = self.state.clone();
        let view = cx.entity();
        let (filter, recipes, loading, error, me, last_runs, page_width) = {
            let state = self.state.read(cx);
            (
                state.recipes_filter,
                // The listing carries recipes and workflows both, on one fetch, and this page is
                // the recipes. Everything on it below this line is tape machinery — versions,
                // the table of steps, adding a step, playing it back — and a decision tree has
                // no use for any of it. The workflows are in the composer's `/` list, where
                // picking one is all there is to do with it today; the page that manages a tree
                // is not built.
                state
                    .recipes
                    .iter()
                    .filter(|recipe| !recipe.is_workflow())
                    .cloned()
                    .collect::<Vec<_>>(),
                state.recipes_loading,
                state.recipes_error.clone(),
                state.account.as_ref().map(|account| account.id.clone()),
                state.recipe_last_runs.clone(),
                // The page takes the chat's slot, so the chat's width is this page's width.
                chat_column_width(
                    f32::from(window.viewport_size().width),
                    state.sidebar_hidden,
                    state.sidebar_collapsed,
                    state.sidebar_expanded_width,
                    state.right_pane != RightPane::Closed,
                ),
            )
        };
        let stacked = row_stacks(list_column_width(page_width));
        let pending = recipes
            .iter()
            .filter(|recipe| recipe.is_pending_invite())
            .count();
        let body = list_body(loading, recipes.len());
        v_flex()
            .size_full()
            .child(
                v_flex()
                    .id("recipes-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(HEADER_PX))
                    .pt(px(LIST_GAP))
                    .pb(px(LIST_BOTTOM_PAD))
                    .child(
                        centered(
                            h_flex()
                                .id("recipes-filters")
                                .w_full()
                                .max_w(px(COLUMN_MAX))
                                .gap(px(8.))
                                // Three chips in a narrow window take a second line rather
                                // than run off the side of it.
                                .flex_wrap()
                                .children(
                                    RecipeFilter::ALL
                                        .into_iter()
                                        .map(|chip| filter_chip(chip, filter, app.clone())),
                                ),
                        )
                        .id("recipes-filter-bar")
                        .flex_shrink_0()
                        .pb(px(LIST_GAP)),
                    )
                    .when_some(error, |this, error| {
                        this.child(centered(
                            div()
                                .id("recipes-error")
                                .w_full()
                                .max_w(px(COLUMN_MAX))
                                .pb(px(12.))
                                .text_xs()
                                .text_color(theme.danger)
                                .child(error),
                        ))
                    })
                    .map(move |this| match body {
                        ListBody::Loading => this.child(loading_state("recipes-loading", theme)),
                        ListBody::Empty => this.child(empty_state(filter, theme)),
                        ListBody::List => this.child(centered(
                            column().id("recipes-list").gap(px(ROW_GAP)).children(
                                recipes.into_iter().map(|recipe| {
                                    let last_run = last_runs.get(&recipe.id).copied();
                                    recipe_row(
                                        recipe,
                                        last_run,
                                        me.as_deref(),
                                        pending == 1,
                                        stacked,
                                        app.clone(),
                                        &view,
                                        theme,
                                    )
                                }),
                            ),
                        )),
                    }),
            )
            .into_any_element()
    }

    /// One recipe: what it is, its versions and their steps, the bots that may run it, who it
    /// is shared with, and the runs so far.
    fn detail(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = self.state.clone();
        let view = cx.entity();
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
        let waiting = loading && detail.is_none();
        v_flex()
            .size_full()
            .child(
                v_flex()
                    .id("recipe-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(HEADER_PX))
                    .pb(px(24.))
                    // Card frames with nothing in them read as a recipe that holds nothing,
                    // so until it is here the page shows only that it is coming.
                    .map(|this| {
                        if waiting {
                            return this.child(loading_state("recipe-loading", theme));
                        }
                        this.child(centered(
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
                        ))
                    }),
            )
            .into_any_element()
    }

    /// The cards of the detail, for what the person may do with this recipe: everything for
    /// its owner; the steps, a run and the history for someone it is shared with; only Accept
    /// and Decline for someone it is offered to. Who else may have it is not a card at all —
    /// it is the share icon in the title bar, and the modal that opens under it.
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
        let waiting = busy.is_some();
        let mut sections = vec![self.about_section(detail, waiting, me, app, theme)];
        if detail.recipe.is_pending_invite() {
            return sections;
        }
        sections.push(self.steps_section(detail, busy, app, view, theme));
        if let Some(run) = run {
            sections.push(last_run_section(&run, detail, theme));
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
            return card(theme)
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .child(section_title("About"))
                        .child(delete_icon(app, busy, theme)),
                )
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
                        // What the card is doing with what was typed, said quietly: the
                        // fields save themselves, so the line is the only word of it.
                        .when_some(self.save_note(), |this, note| {
                            this.child(
                                div()
                                    .id("recipe-save-note")
                                    .text_xs()
                                    .text_color(muted)
                                    .child(note),
                            )
                        }),
                )
                .into_any_element();
        }
        card(theme)
            .child(
                h_flex().w_full().items_center().gap(px(8.)).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(recipe.name.clone()),
                ),
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
            .child(parameters_line(shown, muted, theme))
            .map(|this| match steps {
                Some(steps) => this.child(self.steps_table(steps, editing, view, muted, theme)),
                // A tape the server sent whole reads as a table of its own; one it stripped to
                // a count can only say how much there was.
                None => match shown.and_then(RecipeVersion::tape_events) {
                    Some(events) if !events.is_empty() => this.child(tape_table(
                        events,
                        shown.map(RecipeVersion::event_count).unwrap_or_default(),
                        shown.is_some_and(RecipeVersion::tape_truncated),
                        muted,
                        theme,
                    )),
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
    /// steps, Delete version and Delete recipe, or the editor's own controls while a draft is
    /// open.
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
        // Only a version someone wrote by editing may go on its own, and only its owner may
        // send it: the tape and the steps filtered from it are what the recipe is.
        let deletable = mine
            .then(|| shown_version(detail, self.selected_version))
            .flatten()
            .filter(|version| version.is_edited())
            .map(|version| version.version);
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
                        // Only this version goes: the recipe itself is the trash icon on the
                        // About card, well away from a toolbar meant for the one on screen.
                        .when_some(deletable, |this, number| {
                            this.child(
                                Button::new("recipe-delete-version")
                                    .small()
                                    .label("Delete version")
                                    .tooltip(format!("Remove v{number}, not the recipe"))
                                    .disabled(waiting)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.ask_delete_version(number, cx)
                                            });
                                        }
                                    }),
                            )
                        })
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

    /// One row of the table: its number, what it does, and to what. In edit mode a double
    /// click on the details opens the step's modal, and the row carries move and delete.
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

    /// Sharing, all of it in one place: the bots that may run this recipe, the whole org, and
    /// one person by email. Three tabs rather than three cards, because they are one question
    /// — who else gets this task — asked of three kinds of somebody.
    fn share_overlay(&self, view: &Entity<Self>, theme: &Theme, cx: &App) -> AnyElement {
        let muted = theme.muted_foreground;
        let state = self.state.read(cx);
        let detail = state.recipe_open.clone();
        let busy = state.recipe_busy.is_some();
        let loading = state.recipe_loading && detail.is_none();
        // Only an owner may pass a recipe on; everyone else may still say which of their own
        // bots is allowed to run it, so they get that tab and no other.
        let mine = detail
            .as_ref()
            .is_some_and(|detail| detail.recipe.is_mine());
        let tabs: Vec<ShareTab> = ShareTab::ALL
            .into_iter()
            .filter(|tab| mine || *tab == ShareTab::Bots)
            .collect();
        let shown = if tabs.contains(&self.share_tab) {
            self.share_tab
        } else {
            ShareTab::Bots
        };
        let app = self.state.clone();
        div()
            .id("recipe-share-modal")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.32))
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| this.close_share(cx));
                }
            })
            .child(
                v_flex()
                    .id("recipe-share-card")
                    .w(px(SHARE_MODAL_W))
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
                            .child("Share"),
                    )
                    .child(
                        h_flex()
                            .id("recipe-share-tabs")
                            .w_full()
                            .items_end()
                            .gap(px(2.))
                            .border_b_1()
                            .border_color(theme.border)
                            .children(
                                tabs.into_iter()
                                    .map(|tab| share_tab(tab, tab == shown, view, muted, theme)),
                            ),
                    )
                    .child(
                        // The body is the same height whichever tab is on, so the modal does
                        // not jump about as a person moves between them, and the bots picker's
                        // panel has room to hang open.
                        v_flex()
                            .id("recipe-share-body")
                            .w_full()
                            .h(px(SHARE_BODY_HEIGHT))
                            .gap(px(8.))
                            .when(loading, |this| {
                                this.child(div().text_xs().text_color(muted).child("Loading…"))
                            })
                            .when_some(detail, |this, detail| {
                                this.child(match shown {
                                    ShareTab::Bots => self.share_bots_tab(&detail, muted),
                                    ShareTab::Org => {
                                        share_org_tab(&detail, busy, &app, muted, theme)
                                    }
                                    ShareTab::Email => {
                                        self.share_email_tab(&detail, busy, &app, muted, theme)
                                    }
                                })
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap(px(8.))
                            .pt(px(6.))
                            .child(Button::new("recipe-share-close").label("Close").on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| this.close_share(cx));
                                }
                            })),
                    ),
            )
            .into_any_element()
    }

    /// The Bots tab: which of the person's own bots may run this recipe. Ticking one grants it
    /// and unticking takes the grant back, each on its own request.
    fn share_bots_tab(&self, detail: &RecipeDetail, muted: Hsla) -> AnyElement {
        if detail.my_bots.is_empty() {
            return div()
                .text_xs()
                .text_color(muted)
                .child("You have no bots yet.")
                .into_any_element();
        }
        let granted = granted_bots(detail).len();
        v_flex()
            .w_full()
            .gap(px(8.))
            .child(div().text_xs().text_color(muted).child(
                "A ticked bot may run this recipe on its own computer, and Run plays it on one of them.",
            ))
            .child(self.bots.clone())
            .child(div().text_xs().text_color(muted).child(if granted == 0 {
                "No bot may run this recipe yet.".to_string()
            } else {
                format!("{} may run this recipe.", count_of(granted, "bot"))
            }))
            .into_any_element()
    }

    /// The Email tab: the address, Share, and the people it has gone to with a way to take it
    /// back from each.
    fn share_email_tab(
        &self,
        detail: &RecipeDetail,
        busy: bool,
        app: &Entity<AppState>,
        muted: Hsla,
        theme: &Theme,
    ) -> AnyElement {
        let email_input = self.share_email_input.clone();
        let people: Vec<&RecipeShare> = detail
            .shares
            .iter()
            .filter(|share| share.scope != "org")
            .collect();
        v_flex()
            .w_full()
            .gap(px(8.))
            .child(field_label("Share with a person in your org", muted))
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
            .child(field_label("Shared with", muted))
            .when(people.is_empty(), |this| {
                this.child(div().text_xs().text_color(muted).child("Nobody yet."))
            })
            .child(
                div()
                    .id("recipe-share-people")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(v_flex().w_full().children(people.into_iter().map(|share| {
                        share_row(
                            share.scope_id.clone(),
                            share.scope.clone(),
                            share.scope_id.clone(),
                            share.state_label(),
                            busy,
                            app,
                            muted,
                            theme,
                        )
                    }))),
            )
            .into_any_element()
    }

    /// The modal every step is added and edited in: one labelled field per parameter, Cancel
    /// and Save. It mirrors the delete dialog — the same scrim, the same centred card — and a
    /// click on the scrim is its Cancel, because a dialog that traps a stray click is worse
    /// than one that is easy to leave.
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
        let title = match modal.index {
            Some(index) => format!("Step {}: {}", index + 1, modal.kind.label()),
            None => format!("Add step: {}", modal.kind.label()),
        };
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
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(step_modal_note(modal.kind, screen)),
                    )
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

    /// "Delete v3?" — the version on screen and nothing else. It says what goes with it and
    /// what stays, because the other Delete on the same toolbar takes the whole recipe.
    fn delete_version_overlay(
        &self,
        version: u32,
        view: &Entity<Self>,
        theme: &Theme,
    ) -> AnyElement {
        div()
            .id("recipe-delete-version-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.32))
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| this.cancel_delete_version(cx));
                }
            })
            .child(
                v_flex()
                    .id("recipe-delete-version-confirm")
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
                            .child(format!("Delete version {version}?")),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "Only this version goes, with the runs that played it. The recipe, the tape it was taught from and its other versions stay.",
                    ))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap(px(8.))
                            .pt(px(6.))
                            .child(
                                Button::new("recipe-delete-version-cancel")
                                    .label("Cancel")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.cancel_delete_version(cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("recipe-delete-version-yes")
                                    .danger()
                                    .label(format!("Delete v{version}"))
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.confirm_delete_version(cx)
                                            });
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

/// The share icon: what opens the modal, from the title bar and from a row of the list. An
/// icon and not a button, because it sits beside a heading and a name rather than in a row of
/// things to do. It carries no click of its own — the one on a row has a recipe to open first.
fn share_icon(id: impl Into<ElementId>, muted: Hsla, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.14)))
        .tooltip(|window, cx| Tooltip::new("Share").build(window, cx))
        .child(
            Icon::default()
                .path("icons/share.svg")
                .size(px(14.))
                .text_color(muted),
        )
}

/// The title bar's share icon, at the far right of the row the open recipe's name is on.
fn header_share_icon(view: &Entity<RecipesView>, muted: Hsla, theme: &Theme) -> Stateful<Div> {
    share_icon("recipe-share-open", muted, theme).on_click({
        let view = view.clone();
        move |_, _, cx| {
            view.update(cx, |this, cx| this.open_share(false, cx));
        }
    })
}

/// The About card's trash icon: the one way a whole recipe goes, in the corner the share icon
/// used to hold. It only asks — the dialog it opens is what deletes — and it says nothing
/// while another request is in flight, so a recipe cannot go out from under one.
fn delete_icon(app: &Entity<AppState>, busy: bool, theme: &Theme) -> Stateful<Div> {
    let tone = if busy {
        theme.muted_foreground
    } else {
        status_tone(theme.danger, theme)
    };
    let wash = theme.danger.opacity(0.14);
    div()
        .id("recipe-delete")
        .size(px(28.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .tooltip(|window, cx| Tooltip::new("Delete recipe").build(window, cx))
        .when(!busy, |this| {
            this.cursor_pointer().hover(move |s| s.bg(wash)).on_click({
                let app = app.clone();
                move |_, _, cx| {
                    app.update(cx, |state, cx| state.open_recipe_delete_confirm(cx));
                }
            })
        })
        .child(Icon::new(NativeIcon::Trash).size(px(14.)).text_color(tone))
}

/// Whether what the About card's fields hold is worth saving: there is a name, and one of the
/// two is not what the server last sent. Trimmed on both sides, because the save trims and a
/// trailing space is not an edit worth a request.
fn about_edited(recipe: &RecipeSummary, name: &str, description: &str) -> bool {
    let (name, description) = (name.trim(), description.trim());
    !name.is_empty() && (recipe.name.trim() != name || recipe.description.trim() != description)
}

/// One tab of the share modal, the way a version tab reads: the word, underlined when it is
/// the one on screen.
fn share_tab(
    tab: ShareTab,
    selected: bool,
    view: &Entity<RecipesView>,
    muted: Hsla,
    theme: &Theme,
) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "recipe-share-tab-{}",
            tab.slug()
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
                view.update(cx, |this, cx| this.select_share_tab(tab, cx));
            }
        })
        .child(tab.label())
        .into_any_element()
}

/// The Org tab: everyone in the org, on or off. It is one share and not a list, so it says
/// where it stands and offers the one thing left to do with it.
fn share_org_tab(
    detail: &RecipeDetail,
    busy: bool,
    app: &Entity<AppState>,
    muted: Hsla,
    theme: &Theme,
) -> AnyElement {
    let org = detail.shares.iter().find(|share| share.scope == "org");
    v_flex()
        .w_full()
        .gap(px(8.))
        .child(div().text_xs().text_color(muted).child(
            "Everyone in your org can find a recipe shared this way and run it on their own bots.",
        ))
        .when(org.is_none(), |this| {
            this.child(
                h_flex().w_full().child(
                    Button::new("recipe-share-org")
                        .small()
                        .primary()
                        .label("Share with org")
                        .disabled(busy)
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
        })
        .when_some(org, |this, share| {
            this.child(field_label("Shared with", muted))
                .child(share_row(
                    "Everyone in the org".to_string(),
                    share.scope.clone(),
                    share.scope_id.clone(),
                    share.state_label(),
                    busy,
                    app,
                    muted,
                    theme,
                ))
        })
        .into_any_element()
}

/// One line of who has the recipe: who they are, where the share stands with them, and the ✕
/// that takes it back.
#[allow(clippy::too_many_arguments)]
fn share_row(
    who: String,
    scope: String,
    scope_id: String,
    state: &'static str,
    busy: bool,
    app: &Entity<AppState>,
    muted: Hsla,
    theme: &Theme,
) -> Stateful<Div> {
    let unshare_id = format!("recipe-unshare-{scope}-{scope_id}");
    h_flex()
        .id(SharedString::from(format!(
            "recipe-share-{scope}-{scope_id}"
        )))
        .w_full()
        .items_center()
        .gap(px(8.))
        .py(px(4.))
        .border_b_1()
        .border_color(theme.border)
        .child(div().flex_1().min_w_0().text_sm().truncate().child(who))
        .child(div().text_xs().text_color(muted).child(state))
        .child(
            Button::new(SharedString::from(unshare_id))
                .xsmall()
                .ghost()
                .label("✕")
                .tooltip("Stop sharing")
                .disabled(busy)
                .on_click({
                    let app = app.clone();
                    move |_, _, cx| {
                        let (scope, scope_id) = (scope.clone(), scope_id.clone());
                        app.update(cx, |state, cx| {
                            state.unshare_open_recipe(scope, scope_id, cx);
                        });
                    }
                }),
        )
}

/// The version of that number, when the recipe still has one.
fn version_of(detail: &RecipeDetail, number: u32) -> Option<&RecipeVersion> {
    detail
        .versions
        .iter()
        .find(|version| version.version == number)
}

/// The steps the server filtered off the tape, which is v2 unless a server numbers them
/// otherwise. A version named `filtered` is the plain case; the number is the fallback.
fn filtered_version(detail: &RecipeDetail) -> Option<&RecipeVersion> {
    detail
        .versions
        .iter()
        .find(|version| version.kind == "filtered")
        .or_else(|| {
            detail
                .versions
                .iter()
                .find(|version| version.version == 2 && version.is_runnable())
        })
}

/// The line under the step modal's title: what bounds what may be typed into it, in the terms
/// of the kind being edited.
fn step_modal_note(kind: StepKind, screen: RecipeScreen) -> String {
    match kind {
        StepKind::Wait => format!("A wait is a number of milliseconds, up to {MAX_WAIT_MS}."),
        StepKind::Key => "A key by the name the box knows it as, such as Return or a.".to_string(),
        StepKind::Type => "The text to type, exactly as it should be typed.".to_string(),
        _ => format!(
            "This recipe's screen is {} × {}.",
            screen.width, screen.height
        ),
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
    if let Some(found) = picked.and_then(|picked| version_of(detail, picked)) {
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

/// What the version on screen declares it needs told before it runs, read only: one row for
/// each, with what it is called, what it takes, whether it is required and what it is for.
///
/// A version that declares nothing shows nothing at all, which is every version taught before
/// parameters existed and every recipe that simply does not need telling anything.
fn parameters_line(version: Option<&RecipeVersion>, muted: Hsla, theme: &Theme) -> AnyElement {
    let declared = version.map(RecipeVersion::declared).unwrap_or_default();
    if declared.is_empty() {
        return div().into_any_element();
    }
    v_flex()
        .id("recipe-version-parameters")
        .w_full()
        .gap(px(4.))
        .pt(px(8.))
        .child(div().text_xs().text_color(muted).child(format!(
            "{} it needs told",
            count_of(declared.len(), "thing")
        )))
        .children(declared.iter().map(|parameter| {
            h_flex()
                .id(SharedString::from(format!(
                    "recipe-parameter-{}",
                    parameter.name
                )))
                .w_full()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .child(parameter.name.clone()),
                )
                .child(badge(parameter.kind.label(), muted, theme))
                .child(badge(
                    if parameter.required {
                        "required"
                    } else {
                        "optional"
                    },
                    muted,
                    theme,
                ))
                .child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .truncate()
                        .child(parameter_detail(parameter)),
                )
        }))
        .into_any_element()
}

/// What a declared parameter is for, and anything else the declaration pins down about it.
fn parameter_detail(parameter: &RecipeParameter) -> String {
    let mut said: Vec<String> = Vec::new();
    let description = parameter.description.trim();
    if !description.is_empty() {
        said.push(description.to_string());
    }
    if let Some(allowed) = parameter.allowed() {
        said.push(format!("one of: {}", allowed.join(", ")));
    }
    if let Some(default) = parameter.default.as_deref().filter(|it| !it.is_empty()) {
        said.push(format!("{default} unless told otherwise"));
    }
    said.join(" · ")
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
fn tape_table(
    events: &[RecipeTapeEvent],
    taped: u64,
    truncated: bool,
    muted: Hsla,
    theme: &Theme,
) -> AnyElement {
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
        // A long teach is mostly pointer moves, so the server sends the front of the tape and
        // says it did; without this line the table would quietly look like the whole thing.
        .when(truncated, |this| {
            this.child(
                div()
                    .id("recipe-events-truncated")
                    .w_full()
                    .px(px(10.))
                    .py(px(6.))
                    .text_xs()
                    .text_color(muted)
                    .child(format!(
                        "showing the first {} of {taped} events",
                        events.len()
                    )),
            )
        })
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
/// them.
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
        RecipeStep::Type { text } => vec![text.clone()],
        RecipeStep::Key { key } => vec![key.clone()],
        RecipeStep::Wait { ms } => vec![ms.to_string()],
    }
}

/// The modal's fields as a step and, where the step was taken but not quite as it was typed,
/// what to say about that. A bot plays a recipe on a screen of its own, so a coordinate off
/// that screen is a step that would miss whatever it was aimed at; a wait past the server's cap
/// is shortened rather than refused, since the person meant "a long time" and the server has a
/// longest one.
fn build_step(
    kind: StepKind,
    typed: &[String],
    screen: RecipeScreen,
) -> Result<(RecipeStep, Option<String>), String> {
    match kind {
        // What is typed is what is typed: a space at either end of it is as much a part of the
        // text as the letters between them, so this one value is not trimmed.
        StepKind::Type => {
            return Ok((
                RecipeStep::Type {
                    text: typed.first().cloned().unwrap_or_default(),
                },
                None,
            ));
        }
        StepKind::Key => {
            let key = typed.first().map(|value| value.trim()).unwrap_or_default();
            if key.is_empty() {
                return Err("A key step needs a key, such as Return or a.".to_string());
            }
            return Ok((
                RecipeStep::Key {
                    key: key.to_string(),
                },
                None,
            ));
        }
        StepKind::Wait => {
            let typed = typed.first().map(|value| value.trim()).unwrap_or_default();
            let ms = typed
                .parse::<u64>()
                .map_err(|_| format!("A wait is a number of milliseconds, up to {MAX_WAIT_MS}."))?;
            let note = (ms > MAX_WAIT_MS)
                .then(|| format!("A wait is at most {MAX_WAIT_MS} ms, so that one was shortened."));
            return Ok((
                RecipeStep::Wait {
                    ms: ms.min(MAX_WAIT_MS),
                },
                note,
            ));
        }
        _ => {}
    }
    let params = kind.params();
    let mut numbers: Vec<i64> = Vec::with_capacity(params.len());
    let mut button: Option<Value> = None;
    for (param, value) in params.iter().zip(typed) {
        match param.kind {
            ParamKind::X | ParamKind::Y | ParamKind::Amount => {
                let number = value
                    .trim()
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
            // The kinds of one value answered above, before any field was read as a number.
            ParamKind::Text | ParamKind::Key | ParamKind::Wait => {}
        }
    }
    let step = match kind {
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
        // Answered above, each from its own one field.
        StepKind::Type | StepKind::Key | StepKind::Wait => kind.new_step(screen),
    };
    Ok((step, None))
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

/// The column the list draws in, from the width of the page's own slot: what is left of the
/// slot once the body has taken its padding, and never wider than the page's column.
fn list_column_width(page_width: f32) -> f32 {
    if !page_width.is_finite() {
        return COLUMN_MAX;
    }
    (page_width - 2. * HEADER_PX).clamp(0., COLUMN_MAX)
}

/// Whether a card stacks what it holds rather than laying it out across the column. A width
/// of zero is a window that has not been measured yet, and is taken as the roomy case.
fn row_stacks(column_width: f32) -> bool {
    column_width > 0. && column_width < ROW_STACK_W
}

/// One card of the list: the name as its headline, the description quieter under it, and the
/// facts — whose it is, how many versions it has, what its last run came to and when — as
/// small print grouped beneath that. Accept and Decline join them while a share is waiting on
/// the person. The whole card opens the recipe.
#[allow(clippy::too_many_arguments)]
fn recipe_row(
    recipe: RecipeSummary,
    last_run: Option<RecipeRunNote>,
    me: Option<&str>,
    single_pending: bool,
    stacked: bool,
    app: Entity<AppState>,
    view: &Entity<RecipesView>,
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
    let name = if recipe.name.trim().is_empty() {
        "Untitled task".to_string()
    } else {
        recipe.name.clone()
    };
    let description = recipe.description.trim().to_string();
    let facts = format!(
        "by {owner} · {}",
        count_of(recipe.latest_version as usize, "version")
    );
    // A share waiting on the person is the one thing on the card they have to answer, so its
    // badge takes the ink; the rest only name a relation and stay quiet.
    let relation_badge =
        relation.map(|label| pill(label, if pending { theme.primary } else { muted }));
    // Wide, the badge sits beside the name; narrow, it drops in with the facts so the name
    // keeps the line to itself instead of being squeezed into a word and an ellipsis.
    let (head_badge, meta_badge) = if stacked {
        (None, relation_badge)
    } else {
        (relation_badge, None)
    };
    let run_badge = last_run.map(|note| {
        pill(
            note.label(),
            status_tone(if note.ok { theme.success } else { theme.danger }, theme),
        )
    });
    let run_time = last_run.map(|note| format_time(note.at_ms));
    v_flex()
        .id(SharedString::from(format!("recipe-{id}")))
        .w_full()
        .gap(px(6.))
        .px(px(if stacked { ROW_PAD_NARROW } else { ROW_PAD }))
        .py(px(14.))
        .rounded(px(14.))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        // The card lifts under the pointer: the whole of it is the way into the recipe, not
        // the chevron at the end of its first line.
        .hover(|s| {
            s.bg(rgb(0x777777).opacity(0.1))
                .border_color(theme.primary.opacity(0.4))
                .shadow_sm()
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
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_base()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(name),
                )
                .children(head_badge)
                .when(!pending, |this| {
                    this.child(
                        share_icon(
                            SharedString::from(format!("recipe-share-open-{id}")),
                            muted,
                            theme,
                        )
                        .on_click({
                            let app = app.clone();
                            let view = view.clone();
                            let id = id.clone();
                            move |_, _, cx| {
                                // The modal needs the recipe itself — its bots, its shares —
                                // so the row opens it and the modal stands over it.
                                cx.stop_propagation();
                                app.update(cx, |state, cx| state.open_recipe(id.clone(), cx));
                                view.update(cx, |this, cx| this.open_share(true, cx));
                            }
                        }),
                    )
                })
                .child(
                    div().flex_shrink_0().flex().items_center().child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(16.))
                            .text_color(muted),
                    ),
                ),
        )
        .when(!description.is_empty(), |this| {
            this.child(
                div()
                    .w_full()
                    .text_sm()
                    .text_color(muted)
                    // Two lines of it: a long description wraps in a narrow column rather than
                    // being cut off after a few words, and a card still cannot run away.
                    .line_clamp(2)
                    .child(description),
            )
        })
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .pt(px(4.))
                // The facts sit together at the left rather than stretching to both edges of
                // the card, and take a second line when the column is too narrow for them.
                .flex_wrap()
                .child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .truncate()
                        .child(facts),
                )
                .children(meta_badge)
                .children(run_badge)
                .when_some(run_time, |this, time| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(time),
                    )
                }),
        )
        .when(pending, |this| {
            this.child(
                h_flex()
                    .gap(px(8.))
                    .pt(px(6.))
                    .flex_wrap()
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

/// What stands in the page's content area: one of three things, never two of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListBody {
    Loading,
    Empty,
    List,
}

/// Which of the three it is. While the answer is on its way it is the spinner and nothing
/// else: the list a chip is leaving belongs to the chip before it, and "No tasks taught yet"
/// is a thing the page does not know yet — it showed for an instant and was a lie.
fn list_body(loading: bool, count: usize) -> ListBody {
    if loading {
        ListBody::Loading
    } else if count == 0 {
        ListBody::Empty
    } else {
        ListBody::List
    }
}

/// The page working, in the place the list or the recipe is about to take: the kit's spinner,
/// alone and in the middle. A word at the top of the page flashed and was gone before it could
/// be read; a turning ring in the middle reads as an answer on its way.
fn loading_state(id: impl Into<ElementId>, theme: &Theme) -> AnyElement {
    v_flex()
        .id(id)
        .w_full()
        .flex_1()
        .min_h(px(EMPTY_MIN_H))
        .items_center()
        .justify_center()
        .child(
            Spinner::new()
                .with_size(px(SPINNER_PX))
                .color(theme.muted_foreground),
        )
        .into_any_element()
}

/// An empty list is not an error: the page's own icon, what is missing, why, and — where
/// there is one — the thing a person would do to fill it. It stands in the middle of what is
/// left of the page rather than hanging under the chips.
fn empty_state(filter: RecipeFilter, theme: &Theme) -> AnyElement {
    let (headline, sentence, hint) = empty_words(filter);
    let muted = theme.muted_foreground;
    v_flex()
        .id("recipes-empty")
        .w_full()
        .flex_1()
        .min_h(px(EMPTY_MIN_H))
        .items_center()
        .justify_center()
        .gap(px(10.))
        .child(
            div()
                .id("recipes-empty-icon")
                .size(px(EMPTY_ICON_BOX))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(theme.border)
                .bg(muted.opacity(0.08))
                .child(
                    Icon::default()
                        .path("icons/record.svg")
                        .size(px(22.))
                        .text_color(muted),
                ),
        )
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_center()
                .child(headline),
        )
        .child(
            div()
                .max_w(px(EMPTY_COPY_MAX))
                .text_sm()
                .text_color(muted)
                .text_center()
                .child(sentence),
        )
        .when_some(hint, |this, hint| {
            this.child(
                div()
                    .id("recipes-empty-hint")
                    .max_w(px(EMPTY_COPY_MAX))
                    .mt(px(2.))
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(muted)
                    .text_center()
                    .child(hint),
            )
        })
        .into_any_element()
}

/// What an empty list says under each chip: the headline, the sentence under it, and the step
/// that would fill it where the person is the one who would take it — nobody can make another
/// person share a task with them, so that one ends at the sentence.
fn empty_words(filter: RecipeFilter) -> (&'static str, &'static str, Option<&'static str>) {
    match filter {
        RecipeFilter::Mine => (
            "No tasks taught yet",
            "A recipe is a task you teach a bot once on its computer; it plays that back whenever you ask.",
            Some("Open a bot's computer and use Teach a task."),
        ),
        RecipeFilter::Shared => (
            "Nothing shared with you",
            "A task someone shares with you waits here until you accept it.",
            None,
        ),
        RecipeFilter::Org => (
            "Nothing in your org yet",
            "Tasks shared with the whole org land here, for anyone in it to run.",
            Some("Open one of your own and use Share with org."),
        ),
    }
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
/// open, the title, what the page is doing, and — on an open recipe — the share icon at the
/// far right. The app has ONE header row and it is the title bar, so the page itself draws
/// none, and sharing belongs on the line the recipe's name is on rather than inside a card.
pub fn recipes_header(
    app: Entity<AppState>,
    recipes: &Entity<RecipesView>,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
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
    // The list says it is loading with the spinner in the middle of the page; a word up here
    // came and went before it could be read.
    let note = open.then(|| state.recipe_busy.clone()).flatten();
    // A recipe that is here and is the person's to pass on: there is nothing to share of one
    // still being fetched, and an invite is answered before it is anyone's to share.
    let shareable = state
        .recipe_open
        .as_ref()
        .is_some_and(|detail| !detail.recipe.is_pending_invite());
    let share = shareable.then(|| header_share_icon(recipes, theme.muted_foreground, theme));
    let back: Option<Rc<dyn Fn(&mut App)>> = open.then(|| {
        let app = app.clone();
        Rc::new(move |cx: &mut App| {
            app.update(cx, |state, cx| state.close_recipe(cx));
        }) as Rc<dyn Fn(&mut App)>
    });
    page_header(back, title, note, share, theme.muted_foreground).into_any_element()
}

/// The header's content: a back chevron when there is somewhere to go back to, the title, and
/// what the page is doing and the share icon on the right.
fn page_header(
    back: Option<Rc<dyn Fn(&mut App)>>,
    title: String,
    note: Option<String>,
    share: Option<Stateful<Div>>,
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
        // The bar between the title and what is on the right is nothing but a handle to drag
        // the window by, as the rest of the title bar is.
        .child(window_drag(div().flex_1().h_full()))
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(8.))
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
                .children(share),
        )
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
fn centered(column: impl IntoElement) -> Div {
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

/// A status colour as words rather than as a fill. The theme pitches success and danger for
/// a filled button, where what is written on them is white; on a light page the same green and
/// red are too pale to read, so a light theme takes them a shade deeper.
fn status_tone(tone: Hsla, theme: &Theme) -> Hsla {
    if theme.is_dark() {
        return tone;
    }
    let mut deeper = tone;
    deeper.l *= 0.72;
    deeper
}

/// A small pill: a word or two tinted by what they say — a relation, or what a run came to.
/// The tint is faint and the text is not, so the fact reads at a glance without shouting over
/// the name above it.
fn pill(label: impl Into<SharedString>, tone: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .px(px(8.))
        .py(px(2.))
        .rounded_full()
        .bg(tone.opacity(0.12))
        .border_1()
        .border_color(tone.opacity(0.3))
        .text_xs()
        .text_color(tone)
        .child(label.into())
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
        COLUMN_MAX, ListBody, MAX_WAIT_MS, RecipeDetail, RecipeFilter, RecipeParameter,
        RecipeScreen, RecipeStep, RecipeSummary, RecipeTapeEvent, RecipeVersion, StepKind,
        about_edited, build_step, empty_words, event_offset, filtered_version, list_body,
        list_column_width, parameter_detail, row_stacks, runs_of, shown_version, step_param_values,
        step_words, tape_words, version_kind, version_of,
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
    fn a_declared_parameter_says_what_it_is_for_and_what_it_will_take() {
        let declared = |value: Value| -> RecipeParameter { serde_json::from_value(value).unwrap() };
        assert_eq!(
            parameter_detail(&declared(json!({
                "name": "search_term", "description": "What to search YouTube for",
                "required": true, "kind": "text", "default": null, "values": null
            }))),
            "What to search YouTube for"
        );
        assert_eq!(
            parameter_detail(&declared(json!({
                "name": "lang", "description": "Which language", "kind": "text",
                "values": ["en", "es"]
            }))),
            "Which language · one of: en, es"
        );
        assert_eq!(
            parameter_detail(&declared(json!({
                "name": "count", "description": "", "kind": "number", "default": 5
            }))),
            "5 unless told otherwise",
            "a parameter the declaration says nothing about still says what it stands at"
        );
        assert_eq!(
            parameter_detail(&declared(json!({"name": "bare", "kind": "text"}))),
            "",
            "and one the declaration pins down nothing about says nothing"
        );
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
            built(StepKind::Drag, &["10", "10", "200", "40"], screen),
            drag
        );
        assert_eq!(
            built(StepKind::Click, &["412", "88", ""], screen),
            RecipeStep::Click {
                x: 412,
                y: 88,
                button: None
            },
            "no button named is the left one"
        );
        assert_eq!(
            built(StepKind::Click, &["412", "88", "Right"], screen),
            RecipeStep::Click {
                x: 412,
                y: 88,
                button: Some(Value::from("right"))
            }
        );
        assert_eq!(
            built(StepKind::Scroll, &["640", "400", "0", "-120"], screen),
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
    fn a_step_of_one_value_fills_and_builds_the_same_way() {
        let screen = RecipeScreen {
            width: 1280,
            height: 800,
        };
        let text = RecipeStep::Type {
            text: "example.com".to_string(),
        };
        assert_eq!(step_param_values(&text), vec!["example.com"]);
        assert_eq!(built(StepKind::Type, &["example.com"], screen), text);
        assert_eq!(
            built(StepKind::Type, &["  two words  "], screen),
            RecipeStep::Type {
                text: "  two words  ".to_string()
            },
            "a space at either end is part of what is typed"
        );
        assert_eq!(
            step_param_values(&RecipeStep::Key {
                key: "Return".to_string()
            }),
            vec!["Return"]
        );
        assert_eq!(
            built(StepKind::Key, &[" Return "], screen),
            RecipeStep::Key {
                key: "Return".to_string()
            }
        );
        assert!(
            build_step(StepKind::Key, &values(&[" "]), screen)
                .unwrap_err()
                .contains("Return"),
            "a key step with no key says what one looks like"
        );
        assert_eq!(
            step_param_values(&RecipeStep::Wait { ms: 500 }),
            vec!["500"]
        );
        assert_eq!(
            built(StepKind::Wait, &["500"], screen),
            RecipeStep::Wait { ms: 500 }
        );
        let (step, note) = build_step(StepKind::Wait, &values(&["99999"]), screen).unwrap();
        assert_eq!(
            step,
            RecipeStep::Wait { ms: MAX_WAIT_MS },
            "a longer wait is shortened rather than refused"
        );
        assert!(
            note.is_some_and(|note| note.contains(&MAX_WAIT_MS.to_string())),
            "and the page is told it was"
        );
        assert!(
            build_step(StepKind::Wait, &values(&["soon"]), screen)
                .unwrap_err()
                .contains("milliseconds")
        );
    }

    #[test]
    fn every_kind_of_step_is_edited_in_the_modal() {
        for kind in StepKind::ALL {
            assert!(
                !kind.params().is_empty(),
                "{} has no field, so it could not be edited anywhere",
                kind.label()
            );
        }
    }

    /// What [`super::RecipesView::begin_draft`] picks, apart from the entity that holds the
    /// tab: the tab the person went to when that is an edit of their own, else the filtered
    /// version, else whatever can be run at all.
    fn draft_seed(detail: &RecipeDetail, tab: Option<u32>) -> Option<u32> {
        tab.and_then(|number| version_of(detail, number))
            .filter(|version| version.is_edited())
            .or_else(|| filtered_version(detail))
            .or_else(|| detail.runnable_version())
            .map(|version| version.version)
    }

    #[test]
    fn a_new_version_starts_from_the_filtered_one() {
        let detail = detail_of(&[(1, "raw"), (2, "filtered"), (3, "edited")]);
        assert_eq!(
            draft_seed(&detail, None),
            Some(2),
            "the page stands on v3, and an edit still starts from the filtered steps"
        );
        assert_eq!(
            draft_seed(&detail, Some(3)),
            Some(3),
            "a tab the person went to themselves is the one they asked to change"
        );
        assert_eq!(
            draft_seed(&detail, Some(1)),
            Some(2),
            "the tape cannot be edited, so that one starts from the filtered steps too"
        );
        assert_eq!(
            draft_seed(&detail_of(&[(1, "raw"), (2, "filtered")]), None),
            Some(2)
        );
        assert_eq!(
            filtered_version(&detail).map(|version| version.version),
            Some(2),
            "an edit starts from the steps the server filtered, not from the last edit"
        );
        let taped = detail_of(&[(1, "raw")]);
        assert_eq!(
            filtered_version(&taped).map(|version| version.version),
            None,
            "a tape nothing has been filtered from yet has nothing to start from"
        );
        let unnamed: RecipeDetail = serde_json::from_value(json!({
            "recipe": {"id": "rcp_1"},
            "versions": [{"version": 1, "kind": "raw"}, {"version": 2, "kind": ""}],
        }))
        .unwrap();
        assert_eq!(
            filtered_version(&unnamed).map(|version| version.version),
            Some(2),
            "a server that names no kind still numbers them"
        );
    }

    #[test]
    fn only_a_version_someone_edited_may_be_deleted_on_its_own() {
        let detail = detail_of(&[(1, "raw"), (2, "filtered"), (3, "edited"), (4, "")]);
        let edited: Vec<u32> = detail
            .versions
            .iter()
            .filter(|version| version.is_edited())
            .map(|version| version.version)
            .collect();
        assert_eq!(
            edited,
            vec![3, 4],
            "the tape and the steps filtered from it are what the recipe is"
        );
    }

    /// The step a set of typed values builds, for the cases that are never shortened.
    fn built(kind: StepKind, typed: &[&str], screen: RecipeScreen) -> RecipeStep {
        let (step, note) = build_step(kind, &values(typed), screen).unwrap();
        assert!(note.is_none(), "nothing to say about this one");
        step
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
    fn a_card_stacks_its_badge_once_the_column_is_narrow() {
        assert_eq!(
            list_column_width(2000.),
            COLUMN_MAX,
            "the column is capped however wide the window is"
        );
        assert!(!row_stacks(list_column_width(1200.)));
        assert!(
            row_stacks(list_column_width(480.)),
            "a phone-width window has no room beside the name"
        );
        assert!(
            !row_stacks(list_column_width(0.)),
            "a window that has not been measured yet is not a narrow one"
        );
    }

    #[test]
    fn every_filter_says_what_is_missing_and_what_would_fill_it() {
        let mut headlines = Vec::new();
        for filter in RecipeFilter::ALL {
            let (headline, sentence, _) = empty_words(filter);
            assert!(!headline.is_empty() && !sentence.is_empty());
            headlines.push(headline);
        }
        headlines.sort_unstable();
        headlines.dedup();
        assert_eq!(headlines.len(), 3, "each chip has its own empty state");
        assert!(
            empty_words(RecipeFilter::Mine)
                .2
                .is_some_and(|hint| hint.contains("Teach a task")),
            "the hint names the control that fills the list"
        );
        assert_eq!(
            empty_words(RecipeFilter::Shared).2,
            None,
            "there is nothing a person can do to be shared with"
        );
    }

    #[test]
    fn only_a_field_that_changed_is_worth_a_save() {
        let recipe: RecipeSummary = serde_json::from_value(json!({
            "id": "rcp_1",
            "name": "Open the mail",
            "description": "Opens Mail and reads the newest one",
        }))
        .unwrap();
        let edited = |name, description| about_edited(&recipe, name, description);
        assert!(
            !edited("Open the mail", "Opens Mail and reads the newest one"),
            "tabbing through a field nobody touched is not a save"
        );
        assert!(
            !edited("  Open the mail  ", "Opens Mail and reads the newest one"),
            "the save trims, so a space either side is not an edit"
        );
        assert!(edited(
            "Open the post",
            "Opens Mail and reads the newest one"
        ));
        assert!(edited("Open the mail", "Opens Mail"));
        assert!(
            !edited("", "Opens Mail"),
            "a recipe with no name is refused, and the field would be left holding what was never stored"
        );
    }

    #[test]
    fn the_page_says_nothing_of_the_list_until_it_has_one() {
        assert_eq!(list_body(true, 0), ListBody::Loading);
        assert_eq!(
            list_body(true, 3),
            ListBody::Loading,
            "the list a chip is leaving is not this chip's answer"
        );
        assert_eq!(
            list_body(false, 0),
            ListBody::Empty,
            "only an answer that came back empty is an empty state"
        );
        assert_eq!(list_body(false, 3), ListBody::List);
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
