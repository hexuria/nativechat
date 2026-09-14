use crate::actions::{CloseBotFinder, PickFinderItem};
use crate::components::fields::field_input;
use crate::components::persona::PersonaMark;
use crate::opengrok::Coworker;
use crate::state::AppState;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[derive(Clone)]
enum FinderEntry {
    CreateNew,
    CreateNamed(String),
    Bot {
        id: String,
        name: String,
        shape: Option<String>,
        color: Option<String>,
    },
}

pub struct BotFinder {
    state: Entity<AppState>,
    query: Entity<InputState>,
    was_open: bool,
    pending_focus: bool,
}

impl BotFinder {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search or create Bots")
        });
        cx.observe(&state, |this, state, cx| {
            let open = state.read(cx).bot_finder_open;
            if open && !this.was_open {
                this.pending_focus = true;
            }
            if !open && this.was_open {
                this.pending_focus = false;
            }
            this.was_open = open;
            cx.notify();
        })
        .detach();
        cx.subscribe(&query, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { shift, secondary } if !shift && !secondary => {
                let query = input.read(cx).value().to_string();
                this.pick_enter(&query, cx);
            }
            _ => {}
        })
        .detach();
        Self {
            state,
            query,
            was_open: false,
            pending_focus: false,
        }
    }

    fn pick(&mut self, index: usize, query: &str, cx: &mut Context<Self>) {
        let coworkers = self.state.read(cx).ranked_coworkers();
        let Some(entry) = entries(&coworkers, query).into_iter().nth(index) else {
            return;
        };
        self.activate(entry, cx);
    }

    fn pick_nth_bot(&mut self, bot_index: usize, query: &str, cx: &mut Context<Self>) {
        let coworkers = self.state.read(cx).ranked_coworkers();
        let Some(entry) = entries(&coworkers, query)
            .into_iter()
            .filter(|entry| matches!(entry, FinderEntry::Bot { .. }))
            .nth(bot_index)
        else {
            return;
        };
        self.activate(entry, cx);
    }

    fn pick_enter(&mut self, query: &str, cx: &mut Context<Self>) {
        let coworkers = self.state.read(cx).ranked_coworkers();
        let items = entries(&coworkers, query);
        let entry = items
            .iter()
            .find(|entry| matches!(entry, FinderEntry::Bot { .. }))
            .cloned()
            .or_else(|| items.into_iter().next());
        if let Some(entry) = entry {
            self.activate(entry, cx);
        }
    }

    fn activate(&mut self, entry: FinderEntry, cx: &mut Context<Self>) {
        match entry {
            FinderEntry::CreateNew => {
                self.state.update(cx, |state, cx| state.hire_agent("New Bot", cx));
            }
            FinderEntry::CreateNamed(name) => {
                self.state.update(cx, |state, cx| state.hire_agent(&name, cx));
            }
            FinderEntry::Bot { id, .. } => {
                self.state.update(cx, |state, cx| {
                    state.select_coworker(id, cx);
                    state.close_bot_finder(cx);
                });
            }
        }
    }
}

fn entries(coworkers: &[Coworker], query: &str) -> Vec<FinderEntry> {
    let query = query.trim();
    let bots: Vec<&Coworker> = if query.is_empty() {
        coworkers.iter().collect()
    } else {
        let needle = query.to_lowercase();
        coworkers
            .iter()
            .filter(|c| {
                c.name.to_lowercase().contains(&needle)
                    || c.title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&needle)
            })
            .collect()
    };
    let bots: Vec<FinderEntry> = bots.into_iter().map(bot_entry).collect();
    if query.is_empty() {
        let mut items = vec![FinderEntry::CreateNew];
        items.extend(bots);
        items
    } else if bots.is_empty() {
        vec![FinderEntry::CreateNamed(query.to_string())]
    } else {
        let exact = bots.iter().any(|entry| {
            matches!(entry, FinderEntry::Bot { name, .. } if name.eq_ignore_ascii_case(query))
        });
        let mut items = bots;
        if !exact {
            items.push(FinderEntry::CreateNamed(query.to_string()));
        }
        items
    }
}

fn bot_entry(c: &Coworker) -> FinderEntry {
    FinderEntry::Bot {
        id: c.id.clone(),
        name: c.name.clone(),
        shape: c.avatar_shape.clone(),
        color: c.avatar_color.clone(),
    }
}

impl Render for BotFinder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_focus {
            self.pending_focus = false;
            self.query.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
        }
        let theme = cx.theme();
        let dark = theme.is_dark();
        let muted = theme.muted_foreground;
        let fg = theme.foreground;
        let border = theme.border;
        let panel = if dark {
            rgb(0x2a2a2a)
        } else {
            rgb(0xffffff)
        };
        let hover: Hsla = rgb(0x777777).opacity(0.16).into();
        let query = self.query.read(cx).value().to_string();
        let coworkers = self.state.read(cx).ranked_coworkers();
        let items = entries(&coworkers, &query);
        let view = cx.entity();

        div()
            .id("bot-finder")
            .absolute()
            .inset_0()
            .occlude()
            .bg(theme.background)
            .key_context("BotFinder")
            .on_action({
                let view = view.clone();
                let query = query.clone();
                move |action: &PickFinderItem, _, cx| {
                    view.update(cx, |this, cx| this.pick_nth_bot(action.index, &query, cx));
                }
            })
            .child(
                v_flex()
                    .size_full()
                    .child(
                        h_flex()
                            .id("bot-finder-bar")
                            .w_full()
                            .h(px(52.))
                            .px(px(16.))
                            .gap(px(10.))
                            .items_center()
                            .flex_shrink_0()
                            .border_b_1()
                            .border_color(border)
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("To:"),
                            )
                            .child(
                                div().flex_1().min_w(px(0.)).child(
                                    field_input(&self.query)
                                        .id("bot-finder-input")
                                        .appearance(false)
                                        .w_full(),
                                ),
                            )
                            .child(
                                div()
                                    .id("bot-finder-close")
                                    .size(px(28.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(hover))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.dispatch_action(&CloseBotFinder);
                                    })
                                    .child(
                                        Icon::default()
                                            .path("icons/close.svg")
                                            .size(px(14.)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .p(px(12.))
                            .child(
                                v_flex()
                                    .id("bot-finder-menu")
                                    .w(px(420.))
                                    .max_h(px(420.))
                                    .overflow_y_scroll()
                                    .rounded(px(12.))
                                    .border_1()
                                    .border_color(border)
                                    .bg(panel)
                                    .shadow_lg()
                                    .py(px(6.))
                                    .children({
                                        let mut bot_shortcut = 0usize;
                                        items.into_iter().enumerate().map(move |(i, entry)| {
                                            let shortcut = if matches!(entry, FinderEntry::Bot { .. })
                                                && bot_shortcut < 9
                                            {
                                                bot_shortcut += 1;
                                                Some(bot_shortcut)
                                            } else {
                                                None
                                            };
                                            finder_row(
                                                i,
                                                entry,
                                                shortcut,
                                                dark,
                                                fg,
                                                muted,
                                                hover,
                                                view.clone(),
                                                query.clone(),
                                            )
                                        })
                                    }),
                            ),
                    ),
            )
    }
}

fn finder_row(
    index: usize,
    entry: FinderEntry,
    shortcut: Option<usize>,
    dark: bool,
    fg: Hsla,
    muted: Hsla,
    hover: Hsla,
    view: Entity<BotFinder>,
    query: String,
) -> impl IntoElement {
    let (mark, label) = match &entry {
        FinderEntry::CreateNew => (
            div()
                .size(px(28.))
                .rounded_full()
                .bg(rgb(0x777777).opacity(0.18))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(IconName::Plus).size(px(14.)))
                .into_any_element(),
            "Create new Bot".to_string(),
        ),
        FinderEntry::CreateNamed(name) => (
            div()
                .size(px(28.))
                .rounded_full()
                .bg(rgb(0x777777).opacity(0.18))
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(IconName::Plus).size(px(14.)))
                .into_any_element(),
            format!("Create \"{name}\" Bot"),
        ),
        FinderEntry::Bot {
            id,
            name,
            shape,
            color,
        } => (
            PersonaMark::new(id.clone())
                .shape(shape.clone())
                .color(color.clone())
                .size(px(24.))
                .dark(dark)
                .into_any_element(),
            name.clone(),
        ),
    };
    h_flex()
        .id(SharedString::from(format!("bot-finder-item-{index}")))
        .w_full()
        .h(px(40.))
        .px(px(12.))
        .gap(px(10.))
        .items_center()
        .cursor_pointer()
        .hover(|s| s.bg(hover))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            view.update(cx, |this, cx| this.pick(index, &query, cx));
        })
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_sm()
                .text_color(fg)
                .truncate()
                .child(label),
        )
        .when_some(shortcut, |this, n| {
            this.child(
                h_flex()
                    .gap(px(3.))
                    .flex_shrink_0()
                    .child(keycap("⌘", muted))
                    .child(keycap(&format!("{n}"), muted)),
            )
        })
}

fn keycap(label: &str, muted: Hsla) -> impl IntoElement {
    div()
        .min_w(px(20.))
        .h(px(20.))
        .px(px(5.))
        .rounded(px(5.))
        .bg(rgb(0x777777).opacity(0.22))
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(muted)
        .child(label.to_string())
}
