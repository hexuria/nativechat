//! A Bot's model, fast tier and effort, picked the way Codex picks them, on the Model card in the
//! Bot's settings: the model's name, over its door, the effort and ⚡. The card opens a popover
//! whose top row is ⚡, the effort, the model's name, which opens the list of models, and ↺, over
//! a slider of five stops of effort. The list is a combobox: a search box over the models grouped
//! by door, Subscription (the person's own plan) and Gateway (the server's paid keys), five at a
//! time, which the wheel scrolls.
//!
//! The card is the only place a Bot's model is picked. The composer has no chip for it: the model
//! is the Bot's setting, changed where the Bot's other settings are, and every turn goes through
//! the door the card shows.
//!
//! Every change is saved on the Bot at once (`AppState::pick_model` and its neighbours): nothing
//! waits for a Save, and nothing is kept anywhere but the server. What the picker offers and what
//! each change sends is `opengrok::model_choice`'s; the words and element ids live here so the
//! gpui-agent tree (`agent/host.rs`) says what the window says.

use crate::chrome::INFO_PANE_WIDTH;
use crate::components::fields::field_input;
use crate::opengrok::{
    AccountPlan, EFFORT_NOT_KEPT, EFFORT_STOPS, InferenceKind, LIST_ROWS, ListLine, ModelChoice,
    ModelPick, NO_MODEL, SUBSCRIPTION_GROUP, base_label, effort_label, effort_stop, group_title,
    last_window_start, list_window, row_count, slider_stop, stop_word,
};
use crate::state::AppState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, IconName, Selectable, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

/// The card in the Bot's settings, which opens the popover.
pub(crate) const CARD: &str = "agent-model-card";
/// The popover, and each of its parts.
pub(crate) const POP: &str = "agent-model-pop";
pub(crate) const FAST: &str = "agent-model-fast";
pub(crate) const RESET: &str = "agent-model-reset";
pub(crate) const EFFORT: &str = "agent-model-effort";
/// The model's name, which opens the list, and in the list the heading that goes back.
pub(crate) const OPEN_LIST: &str = "agent-model-open-list";
/// At the top of the list: the search box, which filters both groups as it is typed in.
pub(crate) const SEARCH: &str = "agent-model-search";
pub(crate) const LIST: &str = "agent-model-list";
/// Before each group's heading's door ([`group_id`]).
pub(crate) const GROUP: &str = "agent-model-group-";
/// Before each row's door and id ([`row_id`]).
pub(crate) const ROW: &str = "agent-model-row-";
/// In the list, while what is typed in the search box leaves no model of a list that has some.
pub(crate) const NO_MATCH: &str = "agent-model-no-match";
/// In the list, on a server without per-Bot doors while the account is on the person's plan:
/// the plan's model, which answers for every Bot there.
pub(crate) const PLAN: &str = "agent-model-plan";
/// In the list: the server's word about why it lists no more, as `GET /models` gives it.
pub(crate) const NOTE: &str = "agent-model-note";
/// In the list, on a Bot whose own door is the person's plan, whatever it is pinned to: its
/// routines, which run on the server's paid keys, won't run (`ModelPick::routines`).
pub(crate) const ROUTINES: &str = "agent-model-routines";
/// Under the controls: the server's words for the last change that did not go through.
pub(crate) const ERROR: &str = "agent-model-error";

/// One row of the list, by its door's wire word and the id a pick of it pins with ⚡ off.
pub(crate) fn row_id(source: InferenceKind, base_id: &str) -> String {
    format!("{ROW}{}-{base_id}", source.word())
}

/// A group's heading in the list, by its door's wire word.
pub(crate) fn group_id(source: InferenceKind) -> String {
    format!("{GROUP}{}", source.word())
}

/// What a driver's tree names ⚡ and ↺ by, which the window draws as their marks alone.
#[cfg(feature = "agent")]
pub(crate) const FAST_LABEL: &str = "Fast";
#[cfg(feature = "agent")]
pub(crate) const RESET_LABEL: &str = "Reset";

pub(crate) const MODELS_TITLE: &str = "Models";
/// What the search box says while nothing is typed in it, and what a driver's tree names it by.
pub(crate) const SEARCH_PLACEHOLDER: &str = "Search models";
/// In the list while it has nothing to offer.
pub(crate) const NO_MODELS: &str = "The server lists no models to pick from yet.";
/// In the list while the search leaves nothing of a list that has some.
pub(crate) const NO_MODEL_MATCHES: &str = "No model matches";
/// Under the plan's line on a server without per-Bot doors: whose model it is, and when a Gateway
/// model picked here answers instead.
pub(crate) const ACCOUNT_PLAN_LINE: &str = "Every Bot's on this server, set in Settings → Reply \
     source. A Gateway model answers once replies there are on the server's paid keys.";
const RESET_TIP: &str = "Default effort, and ⚡ off";
const FAST_ON_TIP: &str = "Fast is on. Click to turn it off.";
const FAST_OFF_TIP: &str = "Use this model's fast version";

/// The popover is the settings pane's width, as the card is.
const CARD_WIDTH: f32 = INFO_PANE_WIDTH - 32.;

/// A model's row in the list, which is also how far the wheel scrolls to move the window by one.
const ROW_HEIGHT: f32 = 28.;

/// The card's second line: the door, by the name of its group in the list, the effort, and ⚡
/// while it is on.
pub(crate) fn card_detail(pick: &ModelPick) -> String {
    let door = pick.door.map(group_title);
    let fast = pick.is_fast().then_some("⚡ Fast");
    let effort = effort_label(&pick.effort);
    door.into_iter()
        .chain([effort.as_str()])
        .chain(fast)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What the picker draws, read off the state whenever it changes, so a notify about anything
/// else (a streaming reply notifies many times a second) repaints nothing here.
#[derive(Clone, PartialEq)]
struct Snap {
    pick: Option<ModelPick>,
    open: bool,
    list_open: bool,
    note: Option<String>,
    catalogue_note: Option<String>,
    search: String,
    list_start: usize,
}

impl Snap {
    fn read(state: &AppState) -> Self {
        let open = state.model_picker_open;
        Self {
            pick: state.model_pick(),
            open,
            list_open: open && state.model_list_open,
            note: state.picker_note().map(str::to_string),
            catalogue_note: state.model_catalogue.note.clone(),
            search: state.model_search.clone(),
            list_start: state.model_list_start,
        }
    }
}

/// The picker on the Bot's card. Its own view, because the slider's state and the search box's
/// are entities of their own that have to outlive every frame, and what is done to them is heard
/// here.
pub struct ModelPicker {
    state: Entity<AppState>,
    slider: Entity<SliderState>,
    /// The Bot and the effort the slider was last put at: it is moved when the Bot's effort
    /// changes (a pick saved, a refusal put back, another Bot opened), and never under a drag.
    slider_at: Option<(String, String)>,
    /// The list's search box. What is typed in it is the state's (`AppState::model_search`), so
    /// a person's typing and a driver's `set_value` filter the same list.
    search: Entity<InputState>,
    /// The search box took the caret when the list last opened, which it does once per opening.
    search_focused: bool,
    /// What the wheel has scrolled the list by and not yet moved it a whole model ([`wheel_rows`]).
    scroll_rest: Rc<Cell<f32>>,
    snap: Snap,
}

impl ModelPicker {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let last = (EFFORT_STOPS.len() - 1) as f32;
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(last)
                .step(1.)
                .default_value(slider_stop("medium") as f32)
        });
        // A drag is a pick once it is let go. Every stop the thumb crosses on the way would be a
        // save of its own, and saves that cross on the wire can land in either order.
        cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
            let SliderEvent::Release(value) = event else {
                return;
            };
            let stop = value.end().round().clamp(0., last) as usize;
            if let Some(word) = stop_word(stop) {
                this.state
                    .update(cx, |state, cx| state.pick_model_effort(word, cx));
            }
        })
        .detach();
        cx.observe(&state, |this, state, cx| {
            let snap = Snap::read(state.read(cx));
            if snap != this.snap {
                this.snap = snap;
                cx.notify();
            }
        })
        .detach();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(SEARCH_PLACEHOLDER));
        // What is typed filters the list at once: the state keeps the copy, so the list, the
        // wheel and a driver all read the same search.
        cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                this.state
                    .update(cx, |state, cx| state.set_model_search(query, cx));
            }
        })
        .detach();
        let snap = Snap::read(state.read(cx));
        Self {
            state,
            slider,
            slider_at: None,
            search,
            search_focused: false,
            scroll_rest: Rc::new(Cell::new(0.)),
            snap,
        }
    }

    /// Put the slider at the Bot's effort, when that is not where it was last put.
    fn sync_slider(&mut self, pick: &ModelPick, window: &mut Window, cx: &mut Context<Self>) {
        let at = (pick.bot_id.clone(), pick.effort.clone());
        if self.slider_at.as_ref() == Some(&at) {
            return;
        }
        let stop = slider_stop(&pick.effort) as f32;
        self.slider
            .update(cx, |slider, cx| slider.set_value(stop, window, cx));
        self.slider_at = Some(at);
    }

    /// The search box follows the state, so a search a driver wrote shows in it and a list
    /// opened afresh shows it empty; and it takes the caret as the list opens, so typing filters
    /// at once. Only as it opens: a click elsewhere in the popover keeps the caret where it went.
    fn sync_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.snap.search.clone();
        if self.search.read(cx).value().as_ref() != query.as_str() {
            self.search
                .update(cx, |input, cx| input.set_value(query, window, cx));
        }
        let list_open = self.snap.list_open;
        if list_open && !self.search_focused {
            self.search.update(cx, |input, cx| input.focus(window, cx));
        }
        self.search_focused = list_open;
    }
}

impl Render for ModelPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(pick) = self.snap.pick.clone() else {
            return div().into_any_element();
        };
        self.sync_slider(&pick, window, cx);
        self.sync_search(window, cx);
        let open = self.snap.open;
        let app = self.state.clone();
        let panel = Panel {
            pick: pick.clone(),
            list_open: self.snap.list_open,
            note: self.snap.note.clone(),
            catalogue_note: self.snap.catalogue_note.clone(),
            slider: self.slider.clone(),
            search: self.search.clone(),
            query: self.snap.search.clone(),
            list_start: self.snap.list_start,
            scroll_rest: self.scroll_rest.clone(),
        };
        let popover = Popover::new("agent-model-picker")
            .appearance(false)
            .overlay_closable(true)
            .open(open)
            .on_open_change({
                let app = app.clone();
                move |open, _, cx| {
                    app.update(cx, |state, cx| state.set_model_picker_open(*open, cx));
                }
            })
            .content({
                let app = app.clone();
                move |_, _, cx| panel.render(app.clone(), cx.theme())
            });
        // Under the card, from an anchor of its own the width of the card, as the settings pane's
        // model list always hung: a trigger's popover opens over the trigger itself.
        v_flex()
            .w(px(CARD_WIDTH))
            .child(card(&pick, open, app, cx.theme()))
            .child(popover.trigger(CardAnchor { selected: open }))
            .into_any_element()
    }
}

/// Where the card's popover hangs from: an element of its own under the card, the card's width.
#[derive(IntoElement)]
struct CardAnchor {
    selected: bool,
}

impl Selectable for CardAnchor {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for CardAnchor {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().id("agent-model-anchor").w(px(CARD_WIDTH)).h(px(0.))
    }
}

/// The Model card in the Bot's settings: the model's name, and under it the door, the effort and
/// ⚡. A click opens the popover or shuts it.
fn card(pick: &ModelPick, open: bool, app: Entity<AppState>, theme: &Theme) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let secondary = theme.secondary;
    div()
        .id(CARD)
        .w_full()
        .px(px(14.))
        .py(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .when(open, |this| this.bg(secondary))
        .hover(move |style| style.bg(secondary))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            // The pane shuts its popovers on any mouse down that reaches it. And the popover's
            // own click-outside may have shut it already by the time this runs, so this sets
            // what the card showed when it was clicked, rather than turning whatever is now.
            cx.stop_propagation();
            app.update(cx, |state, cx| state.set_model_picker_open(!open, cx));
        })
        .child(
            h_flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .gap(px(2.))
                        .child(div().text_sm().truncate().child(pick.model_label()))
                        .child(div().text_xs().text_color(muted).child(card_detail(pick))),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(14.))
                        .text_color(muted),
                ),
        )
}

/// What the popover draws.
#[derive(Clone)]
struct Panel {
    pick: ModelPick,
    list_open: bool,
    note: Option<String>,
    catalogue_note: Option<String>,
    slider: Entity<SliderState>,
    search: Entity<InputState>,
    /// What is typed in the search box.
    query: String,
    /// The first model in the list's window, among those the search leaves.
    list_start: usize,
    scroll_rest: Rc<Cell<f32>>,
}

impl Panel {
    fn render(&self, app: Entity<AppState>, theme: &Theme) -> AnyElement {
        let fill = if theme.is_dark() {
            rgb(0x1c1c1c)
        } else {
            rgb(0xffffff)
        };
        v_flex()
            .id(POP)
            .w(px(CARD_WIDTH))
            .p(px(12.))
            .gap(px(10.))
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border)
            .bg(fill)
            .text_color(theme.foreground)
            .shadow_lg()
            .occlude()
            // The chat and the settings pane shut their popovers on any mouse down that reaches
            // them, and a click in here is not one outside.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .map(|this| {
                if self.list_open {
                    this.child(self.list(&app, theme))
                } else {
                    this.child(self.controls(&app, theme))
                }
            })
            .when_some(self.note.clone(), |this, note| {
                this.child(
                    div()
                        .id(ERROR)
                        .text_xs()
                        .text_color(theme.danger)
                        .child(note),
                )
            })
            .into_any_element()
    }

    /// ⚡, the effort, the model's name and ↺, over the slider.
    fn controls(&self, app: &Entity<AppState>, theme: &Theme) -> impl IntoElement {
        let pick = &self.pick;
        let muted = theme.muted_foreground;
        let hover = rgb(0x777777).opacity(0.16);
        let fast = pick.is_fast();
        let fast_toggle = {
            let app = app.clone();
            let blocked = pick.fast_blocked;
            div()
                .id(FAST)
                .size(px(28.))
                .flex_none()
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(theme.border)
                .when(fast, |this| this.bg(theme.secondary))
                .child("⚡")
                .map(|this| match blocked {
                    // Dead, and saying why, rather than gone: the person can see there is such a
                    // thing as a fast version, and why this model has none to switch to.
                    Some(why) => this
                        .opacity(0.4)
                        .tooltip(move |window, cx| Tooltip::new(why).build(window, cx)),
                    None => this
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .tooltip(move |window, cx| {
                            Tooltip::new(if fast { FAST_ON_TIP } else { FAST_OFF_TIP })
                                .build(window, cx)
                        })
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            app.update(cx, |state, cx| state.set_model_fast(!fast, cx));
                        }),
                })
        };
        let open_list = {
            let app = app.clone();
            h_flex()
                .id(OPEN_LIST)
                .min_w(px(0.))
                .gap(px(4.))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(8.))
                .items_center()
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    app.update(cx, |state, cx| state.toggle_model_list(cx));
                })
                .child(
                    div()
                        .min_w(px(0.))
                        .text_sm()
                        .truncate()
                        .child(pick.model_label()),
                )
                .child(
                    Icon::new(IconName::ChevronRight)
                        .size(px(14.))
                        .text_color(muted),
                )
        };
        let reset = {
            let app = app.clone();
            let live = pick.can_reset();
            div()
                .id(RESET)
                .size(px(28.))
                .flex_none()
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .child("↺")
                .tooltip(|window, cx| Tooltip::new(RESET_TIP).build(window, cx))
                .map(|this| {
                    if live {
                        this.cursor_pointer()
                            .hover(move |style| style.bg(hover))
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                cx.stop_propagation();
                                app.update(cx, |state, cx| state.reset_model_pick(cx));
                            })
                    } else {
                        this.opacity(0.4)
                    }
                })
        };
        // A Bot whose effort is none of the stops (Default, or a word such as `none` that
        // something else set) sits on Medium, drawn muted: the label beside it says which it is.
        let on_a_stop = effort_stop(&pick.effort).is_some();
        let shown_stop = slider_stop(&pick.effort);
        v_flex()
            .gap(px(10.))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(8.))
                    .child(fast_toggle)
                    .child(
                        div()
                            .flex_none()
                            .text_sm()
                            .text_color(muted)
                            .child(effort_label(&pick.effort)),
                    )
                    .child(div().flex_1())
                    .child(open_list)
                    .child(reset),
            )
            .child(
                v_flex()
                    .id(EFFORT)
                    .gap(px(4.))
                    .child(
                        Slider::new(&self.slider)
                            .disabled(!pick.effort_kept)
                            .when(!on_a_stop, |this| this.bg(muted)),
                    )
                    .child(h_flex().justify_between().children(
                        EFFORT_STOPS.iter().enumerate().map(|(stop, (name, _))| {
                            let here = on_a_stop && stop == shown_stop;
                            div()
                                .text_xs()
                                .text_color(if here { theme.foreground } else { muted })
                                .child(*name)
                        }),
                    )),
            )
            .when(!pick.effort_kept, |this| {
                this.child(div().text_xs().text_color(muted).child(EFFORT_NOT_KEPT))
            })
    }

    /// The models, grouped by door, under a heading that goes back to the controls and a search
    /// box that filters both groups at once. At most [`LIST_ROWS`] models are in view, each group's
    /// heading over its first model in view and not counted among them; the wheel scrolls the
    /// rest into view anywhere over the list.
    fn list(&self, app: &Entity<AppState>, theme: &Theme) -> impl IntoElement {
        let pick = &self.pick;
        let muted = theme.muted_foreground;
        let groups = pick.search(&self.query);
        let plan = pick.plan_line(&self.query).cloned();
        let total = row_count(&groups);
        // A list with nothing in it is the server's to explain, whatever is typed; a search that
        // leaves nothing of a list that has some is the search's.
        let offers_nothing = pick.groups.is_empty() && pick.account_plan.is_none();
        let no_match = !offers_nothing && total == 0 && plan.is_none();
        let back = {
            let app = app.clone();
            h_flex()
                .id(OPEN_LIST)
                .gap(px(6.))
                .items_center()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    app.update(cx, |state, cx| state.toggle_model_list(cx));
                })
                .child(
                    Icon::new(IconName::ChevronLeft)
                        .size(px(14.))
                        .text_color(muted),
                )
                .child(div().text_sm().child(MODELS_TITLE))
        };
        let wheel = {
            let app = app.clone();
            let rest = self.scroll_rest.clone();
            move |event: &ScrollWheelEvent, window: &mut Window, cx: &mut App| {
                // The list scrolls, and nothing under the popover does.
                cx.stop_propagation();
                let dy = f32::from(event.delta.pixel_delta(window.line_height()).y);
                let rows = wheel_rows(&rest, dy);
                if rows != 0 {
                    app.update(cx, |state, cx| state.scroll_model_list(rows, cx));
                }
            }
        };
        v_flex()
            .gap(px(8.))
            .child(back)
            .child(
                div()
                    .id(SEARCH)
                    .child(field_input(&self.search).cleanable(true)),
            )
            .child(
                v_flex()
                    .id(LIST)
                    .gap(px(10.))
                    .on_scroll_wheel(wheel)
                    .when_some(plan, |this, plan| this.child(account_plan(&plan, theme)))
                    .when(total > 0, |this| {
                        this.child(self.window(&groups, total, app, theme))
                    })
                    .when(offers_nothing, |this| {
                        this.child(
                            div()
                                .px(px(8.))
                                .text_sm()
                                .text_color(muted)
                                .child(NO_MODELS),
                        )
                    })
                    .when(no_match, |this| {
                        this.child(
                            div()
                                .id(NO_MATCH)
                                .px(px(8.))
                                .text_sm()
                                .text_color(muted)
                                .child(NO_MODEL_MATCHES),
                        )
                    })
                    // Under the rows it is about, where a Gateway model to pick instead is in
                    // view.
                    .when_some(pick.routines.clone(), |this, line| {
                        this.child(
                            div()
                                .id(ROUTINES)
                                .px(px(8.))
                                .text_xs()
                                .text_color(muted)
                                .child(line),
                        )
                    })
                    .when_some(self.catalogue_note.clone(), |this, note| {
                        this.child(
                            div()
                                .id(NOTE)
                                .px(px(8.))
                                .text_xs()
                                .text_color(muted)
                                .child(note),
                        )
                    }),
            )
    }

    /// The models in view ([`list_window`]), each group's heading over its first, and while the
    /// list holds more than it shows, a thumb at the right edge saying where in it the window is.
    fn window(
        &self,
        groups: &[crate::opengrok::ChoiceGroup],
        total: usize,
        app: &Entity<AppState>,
        theme: &Theme,
    ) -> impl IntoElement {
        let start = self.list_start.min(last_window_start(total));
        let lines = list_window(groups, start);
        v_flex()
            .relative()
            .gap(px(2.))
            .children(lines.into_iter().enumerate().map(|(at, line)| match line {
                // A heading under another group's rows stands off from them.
                ListLine::Heading(source) => {
                    group_heading(source, at > 0, theme).into_any_element()
                }
                ListLine::Row(row) => self.row(row, app.clone(), theme).into_any_element(),
            }))
            .when(total > LIST_ROWS, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(0.))
                        .top(relative(start as f32 / total as f32))
                        .h(relative(LIST_ROWS as f32 / total as f32))
                        .w(px(3.))
                        .rounded_full()
                        .bg(theme.muted_foreground.opacity(0.4)),
                )
            })
    }

    /// One model: a click puts the Bot on it, fast where ⚡ is on and it has a fast version.
    fn row(&self, row: &ModelChoice, app: Entity<AppState>, theme: &Theme) -> impl IntoElement {
        let current = self.pick.is_current(row);
        let source = row.source;
        let base_id = row.base_id.clone();
        h_flex()
            .id(SharedString::from(row_id(row.source, &row.base_id)))
            .h(px(ROW_HEIGHT))
            .flex_none()
            .gap(px(6.))
            .px(px(8.))
            .rounded(px(6.))
            .items_center()
            .cursor_pointer()
            .hover(|style| style.bg(rgb(0x777777).opacity(0.12)))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                app.update(cx, |state, cx| state.pick_model(source, &base_id, cx));
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_sm()
                    .truncate()
                    .child(row.label.clone()),
            )
            .when(row.has_fast, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("⚡"),
                )
            })
            .when(current, |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .size(px(13.))
                        .flex_shrink_0()
                        .text_color(theme.primary),
                )
            })
    }
}

/// A group's heading, over the first of its models in the window.
fn group_heading(source: InferenceKind, stand_off: bool, theme: &Theme) -> impl IntoElement {
    div()
        .id(SharedString::from(group_id(source)))
        .when(stand_off, |this| this.mt(px(8.)))
        .px(px(8.))
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(group_title(source))
}

/// How many models the wheel moves the list's window: a model for every row's height scrolled,
/// what is scrolled short of one kept in `rest` for the next event, so a trackpad's many small
/// deltas move it as far as a wheel's few large ones. `dy` is toward the top above zero, as the
/// event gives it, which is fewer models; scrolling the other way starts afresh.
fn wheel_rows(rest: &Cell<f32>, dy: f32) -> isize {
    let kept = rest.get();
    let scrolled = if kept * dy < 0. { dy } else { kept + dy };
    let rows = (scrolled / ROW_HEIGHT).trunc();
    rest.set(scrolled - rows * ROW_HEIGHT);
    -(rows as isize)
}

/// The plan's model on a server without per-Bot doors, while the account is on the person's
/// plan: ticked, because it is what answers, and not a row to pick, because a Bot's pick of it
/// would change nothing there.
fn account_plan(plan: &AccountPlan, theme: &Theme) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let model = plan
        .model
        .as_deref()
        .map_or_else(|| NO_MODEL.to_string(), base_label);
    v_flex()
        .gap(px(2.))
        .child(
            div()
                .px(px(8.))
                .text_xs()
                .text_color(muted)
                .child(SUBSCRIPTION_GROUP),
        )
        .child(
            h_flex()
                .id(PLAN)
                .gap(px(6.))
                .px(px(8.))
                .py(px(5.))
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_sm()
                        .truncate()
                        .child(model),
                )
                .child(
                    Icon::new(IconName::Check)
                        .size(px(13.))
                        .flex_shrink_0()
                        .text_color(theme.primary),
                ),
        )
        .child(
            div()
                .px(px(8.))
                .text_xs()
                .text_color(muted)
                .child(ACCOUNT_PLAN_LINE),
        )
}

#[cfg(test)]
mod tests {
    // Item by item rather than a glob: `use super::*` would drag in gpui_kit's own `test`.
    use super::{card_detail, group_id, row_id, wheel_rows};
    use crate::opengrok::{InferenceKind, ModelCatalogue, bot_pick};

    fn pick(source: serde_json::Value, model: &str, effort: &str) -> crate::opengrok::ModelPick {
        let bot = serde_json::from_value(serde_json::json!({
            "id": "cw_1", "name": "Ada", "model": model, "effort": effort, "source": source
        }))
        .expect("a row");
        bot_pick(&bot, None, &ModelCatalogue::default(), |_| Vec::new())
    }

    /// Every part of the picker is under `agent-`, in the Bot's settings, and a row is named by
    /// its door's wire word and the id a pick of it pins.
    #[test]
    fn a_rows_id_is_its_door_and_the_id_it_pins() {
        assert_eq!(super::FAST, "agent-model-fast");
        assert_eq!(
            row_id(InferenceKind::LocalProxy, "gpt-6-luna"),
            "agent-model-row-local_proxy-gpt-6-luna"
        );
        assert_eq!(
            row_id(InferenceKind::Gateway, "oag/cheap"),
            "agent-model-row-gateway-oag/cheap"
        );
        assert_eq!(
            InferenceKind::ALL.map(group_id),
            ["agent-model-group-gateway", "agent-model-group-local_proxy"]
        );
    }

    /// The wheel moves the window a model for every row's height scrolled: a trackpad's small
    /// deltas add up, and a turn the other way starts afresh. Down the list is more.
    #[test]
    fn the_wheel_moves_the_window_a_model_for_every_rows_height() {
        let rest = std::cell::Cell::new(0.);
        assert_eq!(wheel_rows(&rest, -10.), 0);
        assert_eq!(wheel_rows(&rest, -10.), 0);
        assert_eq!(wheel_rows(&rest, -10.), 1, "a row's height gone by");
        assert_eq!(wheel_rows(&rest, 30.), -1, "back up, starting afresh");
        assert_eq!(wheel_rows(&rest, 2. * super::ROW_HEIGHT), -2);
        assert_eq!(wheel_rows(&rest, 0.), 0);
    }

    /// The card's second line says the door, by its group's name in the list, the effort and ⚡.
    #[test]
    fn the_card_says_the_door_the_effort_and_fast() {
        let plan = pick(serde_json::json!("local_proxy"), "gpt-6-luna--fast", "max");
        assert_eq!(card_detail(&plan), "Subscription · Ultra · ⚡ Fast");
        let keys = pick(serde_json::json!("gateway"), "oag/cheap", "inherit");
        assert_eq!(card_detail(&keys), "Gateway · Default");
        let follows = pick(serde_json::Value::Null, "oag/cheap", "low");
        assert_eq!(card_detail(&follows), "Light");
    }
}
