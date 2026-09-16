//! A multi-select picker: a trigger that says what is chosen, and a panel under it with a
//! search field, Select All / Clear All, and a checkbox beside every option.
//!
//! The picker does not own the selection. It says what the person did — one box, or all the
//! boxes at once — and whoever owns the truth applies it and hands the selection back with
//! [`MultiSelectState::sync`]. A request that is refused therefore leaves the boxes as the
//! owner has them rather than as the click left them.

use crate::components::fields::field_input;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

type Theme = gpui_kit::component::Theme;

/// One row of the panel. The list's height is counted in these, so it is the row and not the
/// text that decides where the panel stops and starts scrolling.
const ITEM_HEIGHT: f32 = 30.;
/// How many rows the panel shows before it scrolls.
const MAX_VISIBLE_ITEMS: usize = 6;
/// The box drawn beside an option's label.
const CHECKBOX: f32 = 16.;
/// Chips on the trigger up to this many; past it the trigger says how many instead, because a
/// row of chips that wraps three times is no longer a summary.
const MAX_CHIPS: usize = 3;

/// One option the picker offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiSelectOption {
    pub id: String,
    pub label: String,
}

impl MultiSelectOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

/// The element ids this picker's controls answer to. The page names them rather than the
/// component, so a driver working the page sees `recipe-bots` and not a generic id shared by
/// every picker in the app.
#[derive(Clone, Debug)]
pub struct MultiSelectIds {
    pub trigger: SharedString,
    pub search: SharedString,
    pub select_all: SharedString,
    pub clear_all: SharedString,
    /// An option's row is this with the option's own id after it, `recipe-bot-cw_1`.
    pub option_prefix: SharedString,
}

/// What the person did to the selection, as the change they made rather than as the whole of
/// it: the owner of the truth usually has one request for a box and another for all of them.
#[derive(Debug, Clone)]
pub enum MultiSelectEvent {
    /// One box, and which way it went.
    Toggled { id: String, selected: bool },
    /// Select All, Deselect All or Clear All: the options that changed, and which way.
    Bulk { ids: Vec<String>, selected: bool },
}

pub struct MultiSelectState {
    ids: MultiSelectIds,
    options: Vec<MultiSelectOption>,
    /// The chosen options, in the order the owner of the truth gives them.
    selected: Vec<String>,
    /// The options the search leaves, as indices into `options`.
    filtered: Vec<usize>,
    search: Entity<InputState>,
    open: bool,
    /// The row the arrow keys are on, as an index into `filtered`.
    highlighted: Option<usize>,
    scroll: ScrollHandle,
    /// What the trigger says when nothing is chosen.
    placeholder: SharedString,
    /// No box may be ticked while the answer to the last one is still out.
    disabled: bool,
    /// Where the mouse went down when a click outside shut the panel. That happens in the
    /// capture phase, before the click a person meant by it is dispatched at all, so without
    /// this the click on the trigger that shut the panel would open it straight back up.
    dismissed_at: Option<Point<Pixels>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<MultiSelectEvent> for MultiSelectState {}

impl MultiSelectState {
    pub fn new(ids: MultiSelectIds, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search")
                .clean_on_escape()
        });
        let subscription =
            cx.subscribe(&search, |this, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    this.apply_filter(&query);
                    cx.notify();
                }
                // Enter takes the row the arrows are on, so the panel can be worked without
                // leaving the search field.
                InputEvent::PressEnter { .. } => this.toggle_highlighted(cx),
                _ => {}
            });
        Self {
            ids,
            options: Vec::new(),
            selected: Vec::new(),
            filtered: Vec::new(),
            search,
            open: false,
            highlighted: None,
            scroll: ScrollHandle::new(),
            placeholder: "Select…".into(),
            disabled: false,
            dismissed_at: None,
            _subscriptions: vec![subscription],
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// The options and the selection as their owner has them. It is called from a page's
    /// render, so it does nothing at all when nothing has changed: a notify per frame would
    /// redraw the app forever.
    pub fn sync(
        &mut self,
        options: Vec<MultiSelectOption>,
        selected: Vec<String>,
        disabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.options == options && self.selected == selected && self.disabled == disabled {
            return;
        }
        let changed = self.options != options;
        self.options = options;
        self.selected = selected;
        self.disabled = disabled;
        if changed {
            let query = self.search.read(cx).value().to_string();
            self.apply_filter(&query);
        }
        cx.notify();
    }

    /// Shut the panel, for a page that is putting away whatever the picker stands in.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.notify();
        }
    }

    fn is_selected(&self, id: &str) -> bool {
        self.selected.iter().any(|chosen| chosen == id)
    }

    /// The options whose label holds what was typed, and the highlight back on the first of
    /// them, since the row it was on may not be in the list any more.
    fn apply_filter(&mut self, query: &str) {
        let query = query.trim().to_lowercase();
        self.filtered = self
            .options
            .iter()
            .enumerate()
            .filter(|(_, option)| query.is_empty() || option.label.to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect();
        self.highlighted = (!self.filtered.is_empty()).then_some(0);
    }

    fn toggle(&mut self, id: String, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let selected = !self.is_selected(&id);
        cx.emit(MultiSelectEvent::Toggled { id, selected });
    }

    fn toggle_highlighted(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self
            .highlighted
            .and_then(|row| self.filtered.get(row))
            .and_then(|index| self.options.get(*index))
            .map(|option| option.id.clone())
        else {
            return;
        };
        self.toggle(id, cx);
    }

    /// Select All and Deselect All work on what the search leaves, which is what the person
    /// can see; Clear All works on everything chosen, wherever it is.
    fn set_visible(&mut self, selected: bool, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        let ids: Vec<String> = self
            .filtered
            .iter()
            .filter_map(|index| self.options.get(*index))
            .filter(|option| self.is_selected(&option.id) != selected)
            .map(|option| option.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        cx.emit(MultiSelectEvent::Bulk { ids, selected });
    }

    fn clear_all(&mut self, cx: &mut Context<Self>) {
        if self.disabled || self.selected.is_empty() {
            return;
        }
        cx.emit(MultiSelectEvent::Bulk {
            ids: self.selected.clone(),
            selected: false,
        });
    }

    /// Open the panel on the whole list with the search empty, so a query left over from last
    /// time is never quietly hiding options.
    fn open_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.dismissed_at = None;
        self.search.update(cx, |input, cx| {
            input.set_value(String::new(), window, cx);
            input.focus(window, cx);
        });
        self.apply_filter("");
        cx.notify();
    }

    fn move_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.filtered.is_empty() {
            return;
        }
        let last = self.filtered.len() - 1;
        self.highlighted = Some(match (self.highlighted, down) {
            (Some(row), true) => (row + 1).min(last),
            (Some(row), false) => row.saturating_sub(1),
            (None, _) => 0,
        });
        if let Some(row) = self.highlighted {
            self.scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    /// What the trigger says: the chosen options one by one while there are few enough to
    /// read, how many there are once there are not, and the placeholder when there are none.
    fn trigger_label(&self) -> TriggerLabel {
        let chosen: Vec<&MultiSelectOption> = self
            .options
            .iter()
            .filter(|option| self.is_selected(&option.id))
            .collect();
        match chosen.len() {
            0 => TriggerLabel::Empty,
            count if count > MAX_CHIPS => TriggerLabel::Count(count),
            _ => TriggerLabel::Chips(
                chosen
                    .into_iter()
                    .map(|option| (option.id.clone(), option.label.clone()))
                    .collect(),
            ),
        }
    }

    fn trigger(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let content: AnyElement = match self.trigger_label() {
            TriggerLabel::Empty => div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(self.placeholder.clone())
                .into_any_element(),
            TriggerLabel::Count(count) => count_pill(count, theme).into_any_element(),
            TriggerLabel::Chips(chips) => h_flex()
                .gap(px(4.))
                .flex_wrap()
                .items_center()
                .children(chips.into_iter().map(|(id, label)| {
                    let remove = format!("{}-chip-remove-{id}", self.ids.trigger);
                    h_flex()
                        .id(SharedString::from(format!(
                            "{}-chip-{id}",
                            self.ids.trigger
                        )))
                        .items_center()
                        .gap(px(2.))
                        .pl(px(8.))
                        .pr(px(4.))
                        .py(px(1.))
                        .rounded(px(6.))
                        .bg(theme.secondary)
                        .text_xs()
                        .text_color(theme.secondary_foreground)
                        .child(label)
                        .child(
                            div()
                                .id(SharedString::from(remove))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .p(px(2.))
                                .cursor_pointer()
                                .hover(|s| s.bg(theme.danger.opacity(0.15)))
                                .child(
                                    Icon::new(IconName::Close)
                                        .size(px(10.))
                                        .text_color(theme.muted_foreground),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    // The chip's ✕ takes that one off, and must not also
                                    // count as a click on the trigger behind it.
                                    cx.stop_propagation();
                                    this.toggle(id.clone(), cx);
                                })),
                        )
                }))
                .into_any_element(),
        };
        h_flex()
            .id(self.ids.trigger.clone())
            .w_full()
            .min_h(px(34.))
            .items_center()
            .justify_between()
            .gap(px(6.))
            .px(px(10.))
            .py(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .cursor_pointer()
            .hover(|s| s.border_color(theme.ring))
            .child(div().flex_1().min_w_0().overflow_hidden().child(content))
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
            )
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                let dismissed = this.dismissed_at.take();
                if this.open {
                    this.close(cx);
                } else if !mouse_down_at(event).is_some_and(|at| dismissed == Some(at)) {
                    this.open_panel(window, cx);
                }
            }))
            .into_any_element()
    }

    /// The row over the options: Select All (or Deselect All, once everything the search left
    /// is chosen) on the left, Clear All on the right.
    fn actions(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let visible: Vec<&MultiSelectOption> = self
            .filtered
            .iter()
            .filter_map(|index| self.options.get(*index))
            .collect();
        let all_visible_chosen =
            !visible.is_empty() && visible.iter().all(|option| self.is_selected(&option.id));
        let dim = self.disabled || visible.is_empty();
        let clearable = !self.disabled && !self.selected.is_empty();
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .px(px(10.))
            .py(px(6.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id(self.ids.select_all.clone())
                    .text_xs()
                    .text_color(if dim {
                        theme.muted_foreground
                    } else {
                        theme.primary
                    })
                    .when(!dim, |this| {
                        this.cursor_pointer()
                            .hover(|s| s.text_color(theme.primary.opacity(0.8)))
                    })
                    .child(if all_visible_chosen {
                        "Deselect All"
                    } else {
                        "Select All"
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_visible(!all_visible_chosen, cx);
                    })),
            )
            .child(
                div()
                    .id(self.ids.clear_all.clone())
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .when(clearable, |this| {
                        this.cursor_pointer().hover(|s| s.text_color(theme.danger))
                    })
                    .child("Clear All")
                    .on_click(cx.listener(|this, _, _, cx| this.clear_all(cx))),
            )
            .into_any_element()
    }

    fn option_row(
        &self,
        row: usize,
        option: &MultiSelectOption,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chosen = self.is_selected(&option.id);
        let highlighted = self.highlighted == Some(row);
        let id = option.id.clone();
        h_flex()
            .id(SharedString::from(format!(
                "{}{}",
                self.ids.option_prefix, option.id
            )))
            .w_full()
            .h(px(ITEM_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .when(highlighted, |this| this.bg(rgb(0x777777).opacity(0.14)))
            .when(!self.disabled, |this| {
                this.cursor_pointer()
                    .hover(|s| s.bg(rgb(0x777777).opacity(0.1)))
            })
            .child(
                div()
                    .size(px(CHECKBOX))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if chosen {
                        theme.primary
                    } else {
                        theme.muted_foreground
                    })
                    .when(chosen, |this| {
                        this.bg(theme.primary).child(
                            Icon::new(IconName::Check)
                                .size(px(11.))
                                .text_color(theme.primary_foreground),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .when(self.disabled, |this| {
                        this.text_color(theme.muted_foreground)
                    })
                    .child(option.label.clone()),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.toggle(id.clone(), cx)))
            .into_any_element()
    }
}

/// What the trigger has to show, worked out once so the trigger only draws it.
enum TriggerLabel {
    Empty,
    Count(usize),
    Chips(Vec<(String, String)>),
}

/// Where a click's own mouse down was, for telling one click from another. A click from the
/// keyboard has no such place, and is never the one that shut a panel.
fn mouse_down_at(event: &ClickEvent) -> Option<Point<Pixels>> {
    match event {
        ClickEvent::Mouse(mouse) => Some(mouse.down.position),
        _ => None,
    }
}

/// "4 selected": the summary a trigger falls back to once there are too many chips to read.
fn count_pill(count: usize, theme: &Theme) -> Div {
    div()
        .flex_shrink_0()
        .px(px(8.))
        .py(px(2.))
        .rounded(px(6.))
        .bg(theme.secondary)
        .text_xs()
        .text_color(theme.secondary_foreground)
        .child(format!("{count} selected"))
}

impl Render for MultiSelectState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let trigger = self.trigger(&theme, cx);
        let panel = self.open.then(|| {
            let actions = self.actions(&theme, cx);
            let rows: Vec<AnyElement> = self
                .filtered
                .clone()
                .into_iter()
                .enumerate()
                .filter_map(|(row, index)| {
                    self.options
                        .get(index)
                        .cloned()
                        .map(|option| self.option_row(row, &option, &theme, cx))
                })
                .collect();
            let empty = rows.is_empty();
            v_flex()
                // The panel hangs below the trigger rather than pushing what is under it
                // down, so opening the picker does not move the rest of the form.
                .absolute()
                .left_0()
                .w_full()
                .mt(px(4.))
                .occlude()
                .rounded(px(10.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .shadow_lg()
                .overflow_hidden()
                .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.dismissed_at = Some(event.position);
                    this.close(cx);
                }))
                .child(
                    div()
                        .w_full()
                        .px(px(8.))
                        .py(px(6.))
                        .border_b_1()
                        .border_color(theme.border)
                        .child(
                            field_input(&self.search)
                                .id(self.ids.search.clone())
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .size(px(13.))
                                        .text_color(theme.muted_foreground),
                                ),
                        ),
                )
                .child(actions)
                .child(
                    v_flex()
                        .id(SharedString::from(format!("{}-options", self.ids.trigger)))
                        .w_full()
                        .max_h(px(MAX_VISIBLE_ITEMS as f32 * ITEM_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .children(rows)
                        .when(empty, |this| {
                            this.child(
                                div()
                                    .px(px(10.))
                                    .py(px(10.))
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Nothing matches that."),
                            )
                        }),
                )
        });
        div()
            .relative()
            .w_full()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if !this.open {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" => this.move_highlight(false, cx),
                    "down" => this.move_highlight(true, cx),
                    "escape" => this.close(cx),
                    _ => {}
                }
            }))
            .child(trigger)
            // Deferred, so the panel paints over whatever stands under the trigger instead of
            // being clipped by it.
            .when_some(panel, |this, panel| {
                this.child(deferred(panel).with_priority(3))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_CHIPS, MultiSelectOption};

    /// The trigger's own rule, apart from the entity that draws it: chips while they can be
    /// read, a count once they cannot.
    fn summary(options: &[MultiSelectOption], selected: &[&str]) -> String {
        let chosen: Vec<&MultiSelectOption> = options
            .iter()
            .filter(|option| selected.contains(&option.id.as_str()))
            .collect();
        match chosen.len() {
            0 => "placeholder".to_string(),
            count if count > MAX_CHIPS => format!("{count} selected"),
            _ => chosen
                .iter()
                .map(|option| option.label.clone())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    #[test]
    fn the_trigger_names_a_few_and_counts_the_rest() {
        let options: Vec<MultiSelectOption> = ["hex", "ada", "bo", "cy"]
            .iter()
            .map(|name| MultiSelectOption::new(format!("cw_{name}"), *name))
            .collect();
        assert_eq!(summary(&options, &[]), "placeholder");
        assert_eq!(summary(&options, &["cw_hex"]), "hex");
        assert_eq!(summary(&options, &["cw_hex", "cw_ada"]), "hex, ada");
        assert_eq!(
            summary(&options, &["cw_hex", "cw_ada", "cw_bo", "cw_cy"]),
            "4 selected",
            "past three, how many is easier to read than which"
        );
    }
}
