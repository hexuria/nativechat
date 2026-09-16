//! The composer's picker: one wide panel, the width of the composer itself, that stands above
//! it and offers a list of things to pick from.
//!
//! The panel is one component because the composer has three ways into the same list — the "+"
//! button, `@`, and `/` — and three narrow menus that drift apart are three things to fix every
//! time a row changes. It holds no catalogue of its own: the caller hands it the rows it should
//! show and hears back which one was picked, so what is on offer stays with whoever knows.

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
    /// The kind of thing this is, right-aligned: "Tool", "Skill", "Action".
    pub label: Option<SharedString>,
    /// A key glyph drawn before the label, for a row that stands for a command.
    pub glyph: Option<SharedString>,
    /// A row that says something rather than offering it. It is dimmed, the arrow keys step
    /// over it, and Enter never lands on it.
    pub selectable: bool,
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
            glyph: None,
            selectable: true,
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

    pub fn glyph(mut self, glyph: impl Into<SharedString>) -> Self {
        self.glyph = Some(glyph.into());
        self
    }

    /// Mark this row as a notice: shown, dimmed, and never picked.
    pub fn note(mut self) -> Self {
        self.selectable = false;
        self
    }

    fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.title.to_lowercase().contains(needle)
            || self.description.to_lowercase().contains(needle)
    }
}

/// What the person did with the panel. The panel never acts on a row itself: it says which one
/// was picked and the composer, which knows what the rows stand for, does the rest.
#[derive(Debug, Clone)]
pub enum ComposerPanelEvent {
    Selected(SharedString),
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
                // search field without reaching for the mouse.
                InputEvent::PressEnter { .. } => this.confirm(cx),
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
    pub fn set_rows(&mut self, rows: Vec<ComposerPanelRow>, cx: &mut Context<Self>) {
        if self.rows == rows {
            return;
        }
        self.rows = rows;
        let query = self.search.read(cx).value().to_string();
        self.apply_filter(&query);
        cx.notify();
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

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self
            .highlighted
            .and_then(|position| self.row_at(position))
            .filter(|row| row.selectable)
        else {
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
            .when_some(row.glyph.clone(), |this, glyph| {
                this.child(keycap(glyph, theme.muted_foreground))
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
        let rows: Vec<AnyElement> = self
            .filtered
            .clone()
            .into_iter()
            .enumerate()
            .filter_map(|(position, index)| {
                self.rows
                    .get(index)
                    .cloned()
                    .map(|row| self.row_element(position, &row, &theme, cx))
            })
            .collect();
        let empty = rows.is_empty();
        v_flex()
            .id("composer-panel")
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
    use super::ComposerPanelRow;

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

    #[test]
    fn a_notice_is_shown_but_never_offered() {
        let rows = roster();
        assert!(rows[0].selectable);
        assert!(!rows[2].selectable, "a notice is not a thing to pick");
    }
}
