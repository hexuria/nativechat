use crate::actions::{
    CloseCommandPalette, PaletteNextTab, PalettePrevTab, PaletteSelectNext, PaletteSelectPrev,
};
use crate::components::fields::field_input;
use crate::components::persona::PersonaMark;
use crate::opengrok::Coworker;
use crate::state::{AppSettingsTab, AppState};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaletteTab {
    All,
    Messages,
    Bots,
    Groups,
    Files,
    Links,
    Routines,
    Actions,
}

impl PaletteTab {
    fn all() -> &'static [PaletteTab] {
        &[
            Self::All,
            Self::Messages,
            Self::Bots,
            Self::Groups,
            Self::Files,
            Self::Links,
            Self::Routines,
            Self::Actions,
        ]
    }

    fn next(self) -> Self {
        let all = Self::all();
        let i = all.iter().position(|&tab| tab == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }

    fn prev(self) -> Self {
        let all = Self::all();
        let i = all.iter().position(|&tab| tab == self).unwrap_or(0);
        all[(i + all.len() - 1) % all.len()]
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Messages => "Messages",
            Self::Bots => "Bots",
            Self::Groups => "Groups",
            Self::Files => "Files",
            Self::Links => "Links",
            Self::Routines => "Routines",
            Self::Actions => "Actions",
        }
    }

    fn empty_title(self) -> &'static str {
        match self {
            Self::Messages => "Search messages",
            Self::Bots => "Search bots",
            Self::Groups => "Search groups",
            Self::Files => "Search files",
            Self::Links => "Search links",
            Self::Routines => "Search routines",
            Self::Actions => "No matching actions",
            Self::All => "No results",
        }
    }

    fn empty_hint(self) -> &'static str {
        match self {
            Self::Messages => "Type to find messages across your chats.",
            Self::Bots => "Type to find a bot.",
            Self::Groups => "Groups are not set up yet.",
            Self::Files => "Files are not indexed yet.",
            Self::Links => "Links are not indexed yet.",
            Self::Routines => "Routines are not indexed yet.",
            Self::Actions => "Try a different search.",
            Self::All => "Try a different search.",
        }
    }
}

#[derive(Clone)]
enum PaletteItem {
    Bot {
        id: String,
        name: String,
        detail: String,
        shape: Option<String>,
        color: Option<String>,
    },
    Message {
        coworker_id: String,
        name: String,
        snippet: String,
        shape: Option<String>,
        color: Option<String>,
    },
    Action {
        title: String,
        hint: String,
        icon: &'static str,
        action: PaletteAction,
        checked: bool,
    },
}

#[derive(Clone)]
enum PaletteAction {
    OpenSettings(AppSettingsTab),
    SetTheme(&'static str),
    ToggleSidebar,
    ToggleMiniSidebar,
    ShowAgentSettings,
    ShowComputer,
}

pub struct CommandPalette {
    state: Entity<AppState>,
    query: Entity<InputState>,
    tab: PaletteTab,
    selected: usize,
    was_open: bool,
    pending_focus: bool,
}

impl CommandPalette {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        cx.observe(&state, |this, state, cx| {
            let open = state.read(cx).command_palette_open;
            if open && !this.was_open {
                this.pending_focus = true;
                this.tab = PaletteTab::All;
                this.selected = 0;
            }
            this.was_open = open;
            cx.notify();
        })
        .detach();
        cx.subscribe(&query, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                this.selected = 0;
                cx.notify();
            }
            InputEvent::PressEnter { shift, secondary } if !shift && !secondary => {
                let query = input.read(cx).value().to_string();
                let index = this.selected;
                this.activate_nth(index, &query, cx);
            }
            _ => {}
        })
        .detach();
        Self {
            state,
            query,
            tab: PaletteTab::All,
            selected: 0,
            was_open: false,
            pending_focus: false,
        }
    }

    fn cycle_tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.tab = if forward {
            self.tab.next()
        } else {
            self.tab.prev()
        };
        self.selected = 0;
        cx.notify();
    }

    fn move_selection(&mut self, delta: i32, cx: &mut Context<Self>) {
        let query = self.query.read(cx).value().to_string();
        let n = self.items(&query, cx).len();
        if n == 0 {
            self.selected = 0;
            cx.notify();
            return;
        }
        let next = (self.selected as i32 + delta).rem_euclid(n as i32) as usize;
        self.selected = next;
        cx.notify();
    }

    pub(crate) fn activate_shortcut(&mut self, index: usize, cx: &mut Context<Self>) {
        let query = self.query.read(cx).value().to_string();
        self.activate_nth(index, &query, cx);
    }

    fn activate_nth(&mut self, index: usize, query: &str, cx: &mut Context<Self>) {
        let items = self.items(query, cx);
        let Some(item) = items.into_iter().nth(index) else {
            return;
        };
        match item {
            PaletteItem::Bot { id, .. } | PaletteItem::Message { coworker_id: id, .. } => {
                self.state.update(cx, |state, cx| {
                    state.select_coworker(id, cx);
                });
            }
            PaletteItem::Action { action, .. } => {
                self.state.update(cx, |state, cx| match action {
                    PaletteAction::OpenSettings(tab) => state.open_app_settings(tab, cx),
                    PaletteAction::SetTheme(mode) => state.set_theme_mode(mode, cx),
                    PaletteAction::ToggleSidebar => state.toggle_sidebar(cx),
                    PaletteAction::ToggleMiniSidebar => state.toggle_mini_sidebar(cx),
                    PaletteAction::ShowAgentSettings => state.show_agent_settings(cx),
                    PaletteAction::ShowComputer => state.show_computer_pane(cx),
                });
            }
        }
        self.state.update(cx, |state, cx| {
            state.close_command_palette(cx);
        });
        cx.dispatch_action(&CloseCommandPalette);
    }

    fn items(&self, query: &str, cx: &App) -> Vec<PaletteItem> {
        let state = self.state.read(cx);
        let needle = query.trim().to_lowercase();
        let coworkers = state.ranked_coworkers();
        let mut items = Vec::new();
        let want_bots = matches!(self.tab, PaletteTab::All | PaletteTab::Bots);
        let want_messages = matches!(self.tab, PaletteTab::All | PaletteTab::Messages);
        let want_actions = matches!(self.tab, PaletteTab::All | PaletteTab::Actions);
        if want_bots {
            for c in &coworkers {
                if needle.is_empty() || bot_matches(c, &needle) {
                    items.push(PaletteItem::Bot {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        detail: c.role.clone().or(c.title.clone()).unwrap_or_default(),
                        shape: c.avatar_shape.clone(),
                        color: c.avatar_color.clone(),
                    });
                }
            }
        }
        if want_messages && !needle.is_empty() {
            for c in &coworkers {
                let Some(conv) = state.conversations.iter().find(|conv| conv.id == c.id) else {
                    continue;
                };
                if let Some(message) = conv
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.content.to_lowercase().contains(&needle))
                {
                    items.push(PaletteItem::Message {
                        coworker_id: c.id.clone(),
                        name: c.name.clone(),
                        snippet: clip(&message.content, 80),
                        shape: c.avatar_shape.clone(),
                        color: c.avatar_color.clone(),
                    });
                }
            }
        }
        if want_actions {
            items.extend(
                action_catalog(&state)
                    .into_iter()
                    .filter(|item| {
                        needle.is_empty() || {
                            let PaletteItem::Action { title, hint, .. } = item else {
                                return false;
                            };
                            title.to_lowercase().contains(&needle)
                                || hint.to_lowercase().contains(&needle)
                        }
                    }),
            );
        }
        items
    }
}

fn bot_matches(c: &Coworker, needle: &str) -> bool {
    c.name.to_lowercase().contains(needle)
        || c.title
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(needle)
        || c.role
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(needle)
}

fn action_catalog(state: &AppState) -> Vec<PaletteItem> {
    let theme = state.theme_mode.as_str();
    vec![
        action_item(
            "Agent settings",
            "Current chat",
            "icons/wrench.svg",
            PaletteAction::ShowAgentSettings,
            false,
        ),
        action_item(
            "Agent screen",
            "Current chat",
            "icons/monitor.svg",
            PaletteAction::ShowComputer,
            false,
        ),
        action_item(
            if state.sidebar_hidden {
                "Show sidebar"
            } else {
                "Hide sidebar"
            },
            "View",
            "icons/panel-left.svg",
            PaletteAction::ToggleSidebar,
            false,
        ),
        action_item(
            if state.sidebar_collapsed {
                "Expand sidebar"
            } else {
                "Mini sidebar"
            },
            "View",
            "icons/panel.svg",
            PaletteAction::ToggleMiniSidebar,
            false,
        ),
        action_item(
            "Settings: General",
            "Settings",
            "icons/wrench.svg",
            PaletteAction::OpenSettings(AppSettingsTab::General),
            false,
        ),
        action_item(
            "Settings: Profile",
            "Settings",
            "icons/account_settings.svg",
            PaletteAction::OpenSettings(AppSettingsTab::Profile),
            false,
        ),
        action_item(
            "Settings: Appearance",
            "Settings",
            "icons/sun.svg",
            PaletteAction::OpenSettings(AppSettingsTab::Appearance),
            false,
        ),
        action_item(
            "Settings: Keyboard shortcuts",
            "Settings",
            "icons/session.svg",
            PaletteAction::OpenSettings(AppSettingsTab::Shortcuts),
            false,
        ),
        action_item(
            "Theme: Light",
            "Settings · Appearance",
            "icons/sun.svg",
            PaletteAction::SetTheme("light"),
            theme == "light",
        ),
        action_item(
            "Theme: Dark",
            "Settings · Appearance",
            "icons/moon.svg",
            PaletteAction::SetTheme("dark"),
            theme == "dark",
        ),
        action_item(
            "Theme: System",
            "Settings · Appearance",
            "icons/system_theme.svg",
            PaletteAction::SetTheme("system"),
            theme == "system",
        ),
    ]
}

fn action_item(
    title: &str,
    hint: &str,
    icon: &'static str,
    action: PaletteAction,
    checked: bool,
) -> PaletteItem {
    PaletteItem::Action {
        title: title.into(),
        hint: hint.into(),
        icon,
        action,
        checked,
    }
}

fn clip(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > max {
        format!("{}…", collapsed.chars().take(max).collect::<String>())
    } else {
        collapsed
    }
}

impl Render for CommandPalette {
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
            rgb(0x2c2c2c)
        } else {
            rgb(0xffffff)
        };
        let hover: Hsla = rgb(0x777777).opacity(0.16).into();
        let selected_fill: Hsla = rgb(0x777777).opacity(0.22).into();
        let query = self.query.read(cx).value().to_string();
        let items = self.items(&query, cx);
        if items.is_empty() {
            self.selected = 0;
        } else if self.selected >= items.len() {
            self.selected = items.len() - 1;
        }
        let selected = self.selected;
        let tab = self.tab;
        let view = cx.entity();

        div()
            .id("command-palette")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x000000).opacity(if dark { 0.45 } else { 0.18 }))
            .key_context("CommandPalette")
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.dispatch_action(&CloseCommandPalette);
            })
            .on_action({
                let view = view.clone();
                move |_: &PaletteNextTab, _, cx| {
                    view.update(cx, |this, cx| this.cycle_tab(true, cx));
                }
            })
            .on_action({
                let view = view.clone();
                move |_: &PalettePrevTab, _, cx| {
                    view.update(cx, |this, cx| this.cycle_tab(false, cx));
                }
            })
            .on_action({
                let view = view.clone();
                move |_: &PaletteSelectNext, _, cx| {
                    view.update(cx, |this, cx| this.move_selection(1, cx));
                }
            })
            .on_action({
                let view = view.clone();
                move |_: &PaletteSelectPrev, _, cx| {
                    view.update(cx, |this, cx| this.move_selection(-1, cx));
                }
            })
            .child(
                v_flex()
                    .id("command-palette-card")
                    .w(px(520.))
                    .h(px(480.))
                    .rounded(px(16.))
                    .border_1()
                    .border_color(border)
                    .bg(panel)
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .w_full()
                            .h(px(48.))
                            .px(px(14.))
                            .gap(px(10.))
                            .items_center()
                            .border_b_1()
                            .border_color(border)
                            .child(
                                Icon::new(IconName::Search)
                                    .size(px(16.))
                                    .text_color(muted),
                            )
                            .child(
                                field_input(&self.query)
                                    .id("command-palette-input")
                                    .appearance(false)
                                    .w_full(),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .px(px(12.))
                            .py(px(8.))
                            .gap(px(4.))
                            .overflow_x_hidden()
                            .children(PaletteTab::all().iter().copied().map(|t| {
                                let active = t == tab;
                                div()
                                    .id(SharedString::from(format!("palette-tab-{}", t.label())))
                                    .px(px(8.))
                                    .py(px(4.))
                                    .rounded(px(6.))
                                    .text_sm()
                                    .cursor_pointer()
                                    .when(active, |this| {
                                        this.bg(selected_fill).text_color(fg).font_weight(
                                            FontWeight::MEDIUM,
                                        )
                                    })
                                    .when(!active, |this| this.text_color(muted))
                                    .hover(|s| s.bg(hover))
                                    .on_mouse_down(MouseButton::Left, {
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.tab = t;
                                                cx.notify();
                                            });
                                        }
                                    })
                                    .child(t.label())
                            })),
                    )
                    .child(
                        if items.is_empty() {
                            empty_state(tab, muted)
                        } else {
                            v_flex()
                                .id("command-palette-results")
                                .flex_1()
                                .w_full()
                                .min_h(px(0.))
                                .overflow_y_scroll()
                                .pb(px(8.))
                                .children(items.into_iter().enumerate().map(|(i, item)| {
                                    palette_row(
                                        i,
                                        item,
                                        i == selected,
                                        dark,
                                        fg,
                                        muted,
                                        hover,
                                        selected_fill,
                                        view.clone(),
                                        query.clone(),
                                    )
                                }))
                                .into_any_element()
                        },
                    ),
            )
    }
}

fn empty_state(tab: PaletteTab, muted: Hsla) -> AnyElement {
    v_flex()
        .id("command-palette-empty")
        .flex_1()
        .w_full()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .child(
            Icon::new(IconName::Search)
                .size(px(28.))
                .text_color(muted),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(tab.empty_title()),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(tab.empty_hint()),
        )
        .into_any_element()
}

fn palette_row(
    index: usize,
    item: PaletteItem,
    highlighted: bool,
    dark: bool,
    fg: Hsla,
    muted: Hsla,
    hover: Hsla,
    selected_fill: Hsla,
    view: Entity<CommandPalette>,
    query: String,
) -> AnyElement {
    let shortcut = (index < 9).then(|| index + 1);
    let (leading, title, subtitle, kind) = match item {
        PaletteItem::Action {
            title,
            hint,
            icon,
            checked,
            ..
        } => (
            action_icon(icon, muted),
            title,
            hint,
            RowKind::Action { checked },
        ),
        PaletteItem::Bot {
            id,
            name,
            detail,
            shape,
            color,
        } => (
            PersonaMark::new(id)
                .shape(shape)
                .color(color)
                .size(px(32.))
                .dark(dark)
                .into_any_element(),
            name,
            detail,
            RowKind::Bot,
        ),
        PaletteItem::Message {
            coworker_id,
            name,
            snippet,
            shape,
            color,
        } => (
            PersonaMark::new(coworker_id)
                .shape(shape)
                .color(color)
                .size(px(32.))
                .dark(dark)
                .into_any_element(),
            name,
            snippet,
            RowKind::Bot,
        ),
    };
    h_flex()
        .id(SharedString::from(format!("palette-item-{index}")))
        .w_full()
        .h(px(56.))
        .px(px(12.))
        .gap(px(12.))
        .items_center()
        .flex_shrink_0()
        .cursor_pointer()
        .when(highlighted, |this| this.bg(selected_fill))
        .hover(|s| s.bg(hover))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            view.update(cx, |this, cx| this.activate_nth(index, &query, cx));
        })
        .child(leading)
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(
                    div()
                        .text_sm()
                        .text_color(fg)
                        .truncate()
                        .child(title),
                )
                .when(!subtitle.is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(subtitle),
                    )
                }),
        )
        .when(matches!(kind, RowKind::Action { .. }), |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child("Action"),
            )
        })
        .when_some(shortcut, |this, n| {
            this.child(
                h_flex()
                    .gap(px(3.))
                    .flex_shrink_0()
                    .child(keycap("⌘", muted))
                    .child(keycap(&format!("{n}"), muted)),
            )
        })
        .when(matches!(kind, RowKind::Action { checked: true }), |this| {
            this.child(
                Icon::default()
                    .path("icons/check.svg")
                    .size(px(14.))
                    .text_color(muted),
            )
        })
        .into_any_element()
}

enum RowKind {
    Bot,
    Action { checked: bool },
}

fn action_icon(path: &'static str, muted: Hsla) -> AnyElement {
    div()
        .size(px(32.))
        .rounded(px(8.))
        .bg(rgb(0x777777).opacity(0.14))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .child(
            Icon::default()
                .path(path)
                .size(px(16.))
                .text_color(muted),
        )
        .into_any_element()
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


