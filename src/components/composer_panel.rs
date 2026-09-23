//! The composer's picker: one wide panel, the width of the composer itself, that stands above
//! it and offers a list of things to pick from.
//!
//! The panel is one component because the composer has three ways into the same list — the "+"
//! button, `@`, and `/` — and three narrow menus that drift apart are three things to fix every
//! time a row changes. It holds no catalogue of its own: the caller hands it the rows it should
//! show and hears back which one was picked, so what is on offer stays with whoever knows.

use crate::actions::PickFinderItem;
use crate::components::fields::field_input;
use gpui_kit::component::input::{Escape, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// One row's height. The list's height is counted in these, so the row and not the text decides
/// where the panel stops and starts scrolling.
const ROW_HEIGHT: f32 = 52.;
/// How many rows the panel shows before it scrolls.
const MAX_VISIBLE_ROWS: usize = 6;
/// How far down the list the ⌘1…⌘9 keys reach. Ten keys would need ⌘0, which reads as zero and
/// not as tenth, so the numbers stop at nine and the rest of the list is arrowed to.
const QUICK_KEYS: usize = 9;
/// The glyphs a chord starts with, so a chord can be drawn as one keycap per key. Anything that
/// is not one of these is the key itself, which may be a word — "esc", "enter" — and so is never
/// split into letters.
const MODIFIER_GLYPHS: &[char] = &['⌘', '⇧', '⌥', '⌃', '^', '❖', '⊞'];

/// Where the highlight goes when the rows under an open panel change: back onto the row it was
/// already on, wherever the new list put it, and onto `first` only when that row has gone.
///
/// A listing landing is not something the person did, and `/` now guarantees one shortly after
/// it opens, because the skills arrive on a second route from the recipes. A highlight that went
/// back to the top would move under the hand between the arrow key and the Enter — and the row
/// at the top is a recipe, which is a mode set on the draft and a panel over the message.
fn held_highlight(
    shown: &[(SharedString, bool)],
    held: Option<&SharedString>,
    first: Option<usize>,
) -> Option<usize> {
    let Some(held) = held else {
        return first;
    };
    shown
        .iter()
        .position(|(id, selectable)| id == held && *selectable)
        .or(first)
}

/// One thing the panel offers: an icon, what it is, what it does, and what kind of thing it is.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposerPanelRow {
    /// What the caller calls this row. It comes back in [`ComposerPanelEvent::Selected`] and it
    /// is what the row's element id is built from, so it has to be unique within one panel.
    pub id: SharedString,
    /// An icon path in the asset bundle, `icons/wrench.svg`.
    pub icon: SharedString,
    pub title: SharedString,
    pub description: SharedString,
    /// The kind of thing this is, right-aligned: "Tool", "Recipe", "Action".
    pub label: Option<SharedString>,
    /// The chord that does this row's work from anywhere in the app, as the keyboard shows it —
    /// "⌘,". It is drawn as keycaps before the label, and it is `None` for a row with nothing
    /// bound: a command glyph with no key beside it promises a shortcut that does not exist.
    pub shortcut: Option<SharedString>,
    /// A row that says something rather than offering it. It is dimmed, the arrow keys step
    /// over it, and Enter never lands on it.
    pub selectable: bool,
    /// A row the search never hides. It is for a panel whose field is the answer rather than a
    /// search over answers — the line telling someone to type a value would vanish the moment
    /// they started typing one, which is exactly when it is being read.
    pub always: bool,
    /// The element id this row answers to, when the caller wants one of its own. Rows are
    /// `composer-panel-row-<id>` by default, which is what a list built from data wants; a row
    /// that is a fixed control of the composer, like Attach files, is named by the composer.
    pub element_id: Option<SharedString>,
}

impl ComposerPanelRow {
    pub fn new(
        id: impl Into<SharedString>,
        icon: impl Into<SharedString>,
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            icon: icon.into(),
            title: title.into(),
            description: description.into(),
            label: None,
            shortcut: None,
            selectable: true,
            always: false,
            element_id: None,
        }
    }

    pub fn element_id(mut self, id: impl Into<SharedString>) -> Self {
        self.element_id = Some(id.into());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Mark this row as a notice: shown, dimmed, and never picked.
    pub fn note(mut self) -> Self {
        self.selectable = false;
        self
    }

    /// Keep this row through any search.
    pub fn always(mut self) -> Self {
        self.always = true;
        self
    }

    /// Whether this row is left by the search. The rule is the panel's own; it is reachable
    /// from the sources that build the rows so each list can assert that what it wrote in a
    /// title and a description is what a person typing that word will find.
    pub(crate) fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.always
            || self.title.to_lowercase().contains(needle)
            || self.description.to_lowercase().contains(needle)
    }
}

/// What the person did with the panel. The panel never acts on a row itself: it says which one
/// was picked and the composer, which knows what the rows stand for, does the rest.
#[derive(Debug, Clone)]
pub enum ComposerPanelEvent {
    Selected(SharedString),
    /// Enter with no row to take, carrying what was typed. A panel that lists things ignores
    /// this — there was simply nothing matching — and a panel whose field is the answer, such
    /// as the one a recipe parameter's value is typed into, takes it as the answer.
    Submitted(SharedString),
    /// Escape, or a click outside. `at` is where that click went down, which is `None` for
    /// Escape: a click outside shuts the panel in the capture phase, before the click a person
    /// meant by it is dispatched at all, so without knowing where it was the click on the "+"
    /// that shut the panel would open it straight back up.
    Dismissed {
        at: Option<Point<Pixels>>,
    },
}

pub struct ComposerPanel {
    rows: Vec<ComposerPanelRow>,
    /// The rows the search leaves, as indices into `rows`.
    filtered: Vec<usize>,
    search: Entity<InputState>,
    open: bool,
    /// The row the arrow keys are on, as an index into `filtered`.
    highlighted: Option<usize>,
    scroll: ScrollHandle,
    /// The muted line under the list, saying how the panel is worked.
    hint: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ComposerPanelEvent> for ComposerPanel {}

impl ComposerPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let subscription =
            cx.subscribe(&search, |this, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    this.apply_filter(&query);
                    cx.notify();
                }
                // Enter takes the row the arrows are on, so the panel can be worked from the
                // search field without reaching for the mouse. ⌘↵ is the composer's send
                // chord and is left alone here: it has to reach the composer while the panel
                // holds the focus, which it cannot do if the panel takes it as a pick first.
                InputEvent::PressEnter { secondary, .. } if !*secondary => this.confirm(cx),
                _ => {}
            });
        Self {
            rows: Vec::new(),
            filtered: Vec::new(),
            search,
            open: false,
            highlighted: None,
            scroll: ScrollHandle::new(),
            hint: SharedString::default(),
            _subscriptions: vec![subscription],
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Show these rows, with the search empty so a query left over from last time is never
    /// quietly hiding half the list.
    pub fn open_with(
        &mut self,
        rows: Vec<ComposerPanelRow>,
        placeholder: impl Into<SharedString>,
        hint: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rows = rows;
        self.hint = hint.into();
        self.open = true;
        let placeholder = placeholder.into();
        self.search.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
            input.set_value(String::new(), window, cx);
            input.focus(window, cx);
        });
        self.apply_filter("");
        cx.notify();
    }

    /// Replace the rows of an open panel, for a list that was still being fetched when the
    /// panel opened. The query stands, so what was typed while waiting is not thrown away.
    ///
    /// So does the highlight. A listing landing under an open panel is not something the person
    /// did, and `/` now guarantees one shortly after it opens, because the skills come on a
    /// second route: arrow down to the third row, press ↵ as they land, and a highlight that had
    /// been put back to the top takes the first row instead — which for a recipe is a mode set
    /// on the draft and a parameter panel over the message.
    pub fn set_rows(&mut self, rows: Vec<ComposerPanelRow>, cx: &mut Context<Self>) {
        if self.rows == rows {
            return;
        }
        let held = self.highlighted_id();
        self.rows = rows;
        let query = self.search.read(cx).value().to_string();
        self.apply_filter(&query);
        self.highlighted = held_highlight(&self.shown(), held.as_ref(), self.highlighted);
        // Into view as well as onto the row. Twenty skills landing above it can push the row
        // somebody is pointing at below the fold, which is the same complaint the line above
        // answers, one step quieter.
        if let Some(position) = self.highlighted {
            self.scroll.scroll_to_item(position);
        }
        cx.notify();
    }

    /// What the highlight is on, by the row's own id rather than by where it sits: where it sits
    /// is the thing a new listing moves.
    fn highlighted_id(&self) -> Option<SharedString> {
        self.row_at(self.highlighted?).map(|row| row.id.clone())
    }

    /// The rows the search is leaving, each with whether it can be picked.
    fn shown(&self) -> Vec<(SharedString, bool)> {
        (0..self.filtered.len())
            .filter_map(|position| self.row_at(position))
            .map(|row| (row.id.clone(), row.selectable))
            .collect()
    }

    /// Change the line under the list without disturbing what is typed above it, so a value the
    /// panel would not take can be answered where it was typed rather than after a reopen.
    pub fn set_hint(&mut self, hint: impl Into<SharedString>, cx: &mut Context<Self>) {
        let hint = hint.into();
        if self.hint != hint {
            self.hint = hint;
            cx.notify();
        }
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            cx.notify();
        }
    }

    fn apply_filter(&mut self, query: &str) {
        let needle = query.trim().to_lowercase();
        self.filtered = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.matches(&needle))
            .map(|(index, _)| index)
            .collect();
        self.highlighted = self.first_selectable();
    }

    fn row_at(&self, position: usize) -> Option<&ComposerPanelRow> {
        self.filtered
            .get(position)
            .and_then(|index| self.rows.get(*index))
    }

    fn first_selectable(&self) -> Option<usize> {
        (0..self.filtered.len())
            .find(|position| self.row_at(*position).is_some_and(|row| row.selectable))
    }

    /// Step the highlight over the rows that can be picked. A notice in the middle of the list
    /// is stepped over rather than landed on, so Enter always has something to take.
    fn move_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.filtered.is_empty() {
            return;
        }
        let last = self.filtered.len() - 1;
        let mut position = match (self.highlighted, down) {
            (Some(row), true) => row.min(last),
            (Some(row), false) => row.min(last),
            (None, _) => return self.reset_highlight(cx),
        };
        loop {
            let next = if down {
                if position == last {
                    break;
                }
                position + 1
            } else {
                if position == 0 {
                    break;
                }
                position - 1
            };
            position = next;
            if self.row_at(position).is_some_and(|row| row.selectable) {
                self.highlighted = Some(position);
                self.scroll.scroll_to_item(position);
                cx.notify();
                return;
            }
        }
    }

    fn reset_highlight(&mut self, cx: &mut Context<Self>) {
        self.highlighted = self.first_selectable();
        cx.notify();
    }

    /// Which of the shown rows can be picked, in the order they are drawn. The ⌘ numbers are
    /// counted over this, so they follow the search rather than the roster behind it.
    fn shown_selectable(&self) -> Vec<bool> {
        (0..self.filtered.len())
            .map(|position| self.row_at(position).is_some_and(|row| row.selectable))
            .collect()
    }

    /// Take the row ⌘1…⌘9 asked for, counting from zero the way the keymap does.
    fn quick_pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(id) = quick_position(&self.shown_selectable(), index)
            .and_then(|position| self.row_at(position))
            .map(|row| row.id.clone())
        else {
            return;
        };
        cx.emit(ComposerPanelEvent::Selected(id));
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self
            .highlighted
            .and_then(|position| self.row_at(position))
            .filter(|row| row.selectable)
        else {
            let typed = self.search.read(cx).value().trim().to_string();
            if !typed.is_empty() {
                cx.emit(ComposerPanelEvent::Submitted(typed.into()));
            }
            return;
        };
        let id = row.id.clone();
        cx.emit(ComposerPanelEvent::Selected(id));
    }

    fn pick(&mut self, id: SharedString, cx: &mut Context<Self>) {
        cx.emit(ComposerPanelEvent::Selected(id));
    }

    fn row_element(
        &self,
        position: usize,
        row: &ComposerPanelRow,
        quick: Option<usize>,
        theme: &gpui_kit::component::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = self.highlighted == Some(position) && row.selectable;
        let id = row.id.clone();
        let selectable = row.selectable;
        let title_color = if selectable {
            theme.foreground
        } else {
            theme.muted_foreground
        };
        h_flex()
            .id(row
                .element_id
                .clone()
                .unwrap_or_else(|| SharedString::from(format!("composer-panel-row-{}", row.id))))
            .w_full()
            .h(px(ROW_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .gap(px(12.))
            .px(px(12.))
            .when(highlighted, |this| this.bg(rgb(0x777777).opacity(0.18)))
            .when(selectable, |this| {
                this.cursor_pointer()
                    .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
            })
            .child(
                div()
                    .size(px(30.))
                    .flex_shrink_0()
                    .rounded(px(8.))
                    .bg(rgb(0x777777).opacity(0.14))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::default()
                            .path(row.icon.clone())
                            .size(px(16.))
                            .text_color(if selectable {
                                theme.secondary_foreground
                            } else {
                                theme.muted_foreground
                            }),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(px(2.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(title_color)
                            .truncate()
                            .child(row.title.clone()),
                    )
                    .when(!row.description.is_empty(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .truncate()
                                .child(row.description.clone()),
                        )
                    }),
            )
            .when_some(row.shortcut.clone(), |this, shortcut| {
                this.child(
                    h_flex()
                        .id(SharedString::from(format!(
                            "composer-panel-shortcut-{}",
                            row.id
                        )))
                        .gap(px(3.))
                        .flex_shrink_0()
                        .children(chord_keys(&shortcut, theme.muted_foreground)),
                )
            })
            .when_some(row.label.clone(), |this, label| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(label),
                )
            })
            // The number a person can hold ⌘ with to take this row without arrowing to it. It
            // sits where the ⌘K palette puts the same thing, so one habit works in both.
            .when_some(quick, |this, number| {
                this.child(
                    h_flex()
                        .id(SharedString::from(format!(
                            "composer-panel-quick-{}",
                            row.id
                        )))
                        .gap(px(3.))
                        .flex_shrink_0()
                        .child(keycap("⌘".into(), theme.muted_foreground))
                        .child(keycap(
                            SharedString::from(number.to_string()),
                            theme.muted_foreground,
                        )),
                )
            })
            .when(selectable, |this| {
                this.on_click(cx.listener(move |this, _, _, cx| this.pick(id.clone(), cx)))
            })
            .into_any_element()
    }
}

impl Render for ComposerPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let selectable = self.shown_selectable();
        let rows: Vec<AnyElement> = self
            .filtered
            .clone()
            .into_iter()
            .enumerate()
            .filter_map(|(position, index)| {
                self.rows.get(index).cloned().map(|row| {
                    let quick = quick_number(&selectable, position);
                    self.row_element(position, &row, quick, &theme, cx)
                })
            })
            .collect();
        let empty = rows.is_empty();
        v_flex()
            .id("composer-panel")
            // The panel holds the focus while it is open, so a chord that has to work from
            // inside it needs somewhere to be bound: the composer's own context is on the
            // other branch of the tree and never reaches this far.
            .key_context("ComposerPanel")
            .w_full()
            .occlude()
            .rounded(px(16.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            .overflow_hidden()
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                cx.emit(ComposerPanelEvent::Dismissed {
                    at: Some(event.position),
                });
                this.close(cx);
            }))
            // The arrows and Escape belong to the panel while it is open, and the search field
            // binds all three to actions of its own. Actions are dispatched before key
            // listeners, so taking them in the capture phase — which runs from the outside in —
            // is the only way the panel gets them first.
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                cx.stop_propagation();
                this.move_highlight(false, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                cx.stop_propagation();
                this.move_highlight(true, cx);
            }))
            .capture_action(cx.listener(|this, _: &Escape, _, cx| {
                cx.stop_propagation();
                cx.emit(ComposerPanelEvent::Dismissed { at: None });
                this.close(cx);
            }))
            // ⌘1…⌘9 are already bound app-wide for the bot finder and the ⌘K palette, and the
            // panel answers the same action rather than asking for a keymap of its own: one
            // number key picks the nth row in every picker, and taking it in the capture phase
            // keeps the row under the panel from being picked instead.
            .capture_action(cx.listener(|this, pick: &PickFinderItem, _, cx| {
                cx.stop_propagation();
                this.quick_pick(pick.index, cx);
            }))
            .child(
                h_flex()
                    .w_full()
                    .h(px(44.))
                    .px(px(12.))
                    .gap(px(10.))
                    .items_center()
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(15.))
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            field_input(&self.search)
                                .id("composer-panel-search")
                                .appearance(false)
                                .w_full(),
                        ),
                    ),
            )
            .child(
                v_flex()
                    .id("composer-panel-rows")
                    .w_full()
                    .max_h(px(MAX_VISIBLE_ROWS as f32 * ROW_HEIGHT))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .children(rows)
                    .when(empty, |this| {
                        this.child(
                            div()
                                .px(px(12.))
                                .py(px(14.))
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("Nothing matches that."),
                        )
                    }),
            )
            .when(!self.hint.is_empty(), |this| {
                this.child(
                    div()
                        .id("composer-panel-hint")
                        .w_full()
                        .px(px(12.))
                        .py(px(8.))
                        .border_t_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(self.hint.clone()),
                )
            })
            .into_any_element()
    }
}

/// The ⌘ number the row at this position in the shown list answers to, if it has one. Only the
/// rows that can be picked are counted, so a notice in the middle of the list neither takes a
/// number nor pushes the rows under it along.
fn quick_number(selectable: &[bool], position: usize) -> Option<usize> {
    if !selectable.get(position).copied().unwrap_or(false) {
        return None;
    }
    let number = selectable[..position].iter().filter(|it| **it).count() + 1;
    (number <= QUICK_KEYS).then_some(number)
}

/// Where in the shown list ⌘<index + 1> lands, counting from zero the way the keymap does.
fn quick_position(selectable: &[bool], index: usize) -> Option<usize> {
    selectable
        .iter()
        .enumerate()
        .filter(|(_, it)| **it)
        .map(|(position, _)| position)
        .nth(index)
}

/// A chord as keycaps, one per key: "⌘⇧B" is three caps, "⌘," is two.
fn chord_keys(chord: &str, muted: Hsla) -> Vec<AnyElement> {
    let mut keys: Vec<AnyElement> = chord
        .chars()
        .take_while(|key| MODIFIER_GLYPHS.contains(key))
        .map(|key| keycap(SharedString::from(key.to_string()), muted).into_any_element())
        .collect();
    let key: String = chord
        .chars()
        .skip_while(|key| MODIFIER_GLYPHS.contains(key))
        .collect();
    if !key.is_empty() {
        keys.push(keycap(SharedString::from(key), muted).into_any_element());
    }
    keys
}

fn keycap(label: SharedString, muted: Hsla) -> impl IntoElement {
    div()
        .min_w(px(20.))
        .h(px(20.))
        .px(px(5.))
        .rounded(px(5.))
        .bg(rgb(0x777777).opacity(0.22))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(muted)
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::{
        ComposerPanelRow, SharedString, chord_keys, held_highlight, quick_number, quick_position,
    };

    fn roster() -> Vec<ComposerPanelRow> {
        vec![
            ComposerPanelRow::new("tool:shell", "icons/wrench.svg", "shell", "Run a command"),
            ComposerPanelRow::new(
                "tool:open_url",
                "icons/globe.svg",
                "open_url",
                "Open a page",
            ),
            ComposerPanelRow::new("note", "icons/plugins.svg", "Plugins", "Not listed yet").note(),
        ]
    }

    #[test]
    fn the_search_reads_both_the_name_and_what_it_does() {
        let rows = roster();
        let matching = |needle: &str| -> Vec<&str> {
            rows.iter()
                .filter(|row| row.matches(needle))
                .map(|row| row.title.as_ref())
                .collect()
        };
        assert_eq!(matching(""), vec!["shell", "open_url", "Plugins"]);
        assert_eq!(matching("url"), vec!["open_url"]);
        assert_eq!(
            matching("command"),
            vec!["shell"],
            "what a tool does is worth searching, not just what it is called"
        );
    }

    /// A list that fills in under an open panel must not move what the person is pointing at.
    #[test]
    fn a_listing_landing_leaves_the_highlight_on_the_row_it_was_on() {
        let before: Vec<(SharedString, bool)> = vec![
            ("recipe:a".into(), true),
            ("recipe:b".into(), true),
            ("action:settings".into(), true),
        ];
        let held: SharedString = "recipe:b".into();
        // The skills land between the recipes and the actions, so every row under it moves.
        let after: Vec<(SharedString, bool)> = vec![
            ("recipe:a".into(), true),
            ("recipe:b".into(), true),
            ("skill:skl_1".into(), true),
            ("action:settings".into(), true),
        ];
        assert_eq!(
            held_highlight(&after, Some(&held), Some(0)),
            Some(1),
            "the row is where it was, and the highlight is still on it"
        );

        // A search that no longer shows it, or a row that has gone: the top is what is left.
        let narrowed: Vec<(SharedString, bool)> = vec![("skill:skl_1".into(), true)];
        assert_eq!(held_highlight(&narrowed, Some(&held), Some(0)), Some(0));
        assert_eq!(
            held_highlight(&after, None, Some(0)),
            Some(0),
            "nothing was highlighted before, so nothing is being kept"
        );
        // And a row that is still listed but has become a notice cannot hold it.
        let dimmed: Vec<(SharedString, bool)> = vec![("recipe:b".into(), false)];
        assert_eq!(held_highlight(&dimmed, Some(&held), None), None);
        assert_eq!(before.len(), 3);
    }

    #[test]
    fn a_notice_is_shown_but_never_offered() {
        let rows = roster();
        assert!(rows[0].selectable);
        assert!(!rows[2].selectable, "a notice is not a thing to pick");
    }

    #[test]
    fn the_numbers_skip_a_notice_and_stop_at_nine() {
        // A notice sits third, and eleven rows are shown in all.
        let mut selectable = vec![true; 11];
        selectable[2] = false;
        assert_eq!(quick_number(&selectable, 0), Some(1));
        assert_eq!(quick_number(&selectable, 1), Some(2));
        assert_eq!(
            quick_number(&selectable, 2),
            None,
            "a notice cannot be picked, so no key is offered for it"
        );
        assert_eq!(
            quick_number(&selectable, 3),
            Some(3),
            "the notice is stepped over rather than counted"
        );
        assert_eq!(quick_number(&selectable, 9), Some(9));
        assert_eq!(
            quick_number(&selectable, 10),
            None,
            "past nine there is no number key left to show"
        );
    }

    #[test]
    fn a_number_takes_the_row_it_is_drawn_on() {
        let mut selectable = vec![true; 5];
        selectable[2] = false;
        // ⌘1 is index 0, and the third pickable row is the fourth one shown.
        assert_eq!(quick_position(&selectable, 0), Some(0));
        assert_eq!(quick_position(&selectable, 2), Some(3));
        assert_eq!(
            quick_position(&selectable, 4),
            None,
            "there is no fifth row to pick once the notice is left out"
        );
    }

    #[test]
    fn a_chord_is_one_cap_per_key() {
        let muted = gpui_kit::hsla(0., 0., 0., 1.);
        assert_eq!(chord_keys("⌘,", muted).len(), 2);
        assert_eq!(chord_keys("⌘⇧B", muted).len(), 3);
        assert_eq!(
            chord_keys("⌘esc", muted).len(),
            2,
            "a key with a name is one cap, not one per letter"
        );
    }
}
