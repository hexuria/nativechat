use crate::chrome::{
    AVATAR_PX, MASCOT_BOX_PX, RAIL_HOVER, RAIL_HOVER_ALPHA, SIDEBAR_GAP, SIDEBAR_ROW,
};
use crate::components::persona::PersonaMark;
use crate::icons::NativeIcon;
use crate::state::{AppSettingsTab, AppState, Conversation, THREAD_LIST_UNAVAILABLE};
use chrono::NaiveDateTime;
use gpui_kit::assets::IconNamed;
use gpui_kit::base::{Align, ElementExt as _, POPUP_PRIORITY, Placement, Positioner};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::{Duration, SystemTime};

#[derive(Clone, PartialEq, Eq)]
struct RailCoworker {
    id: String,
    name: String,
    shape: Option<String>,
    color: Option<String>,
    preview: String,
    time: String,
}

#[derive(Clone, PartialEq, Eq)]
struct SidebarRev {
    collapsed: bool,
    hidden: bool,
    expanded_width: i32,
    theme_mode: String,
    active_id: Option<String>,
    any_modal: bool,
    coworkers: Vec<RailCoworker>,
    pinned: Vec<String>,
    hidden_ids: Vec<String>,
    renaming: Option<String>,
    account_label: String,
    account_email: String,
    thread_list_unavailable: bool,
}

impl SidebarRev {
    fn from_state(state: &AppState) -> Self {
        Self {
            collapsed: state.sidebar_collapsed,
            hidden: state.sidebar_hidden,
            expanded_width: state.sidebar_expanded_width.round() as i32,
            theme_mode: state.theme_mode.clone(),
            active_id: state.active_coworker_id.clone(),
            any_modal: state.is_voice_mode_open || state.is_app_settings_open,
            coworkers: state
                .ranked_coworkers()
                .into_iter()
                .map(|c| {
                    let (preview, time) =
                        rail_preview(state.conversations.iter().find(|conv| conv.id == c.id));
                    RailCoworker {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        shape: c.avatar_shape.clone(),
                        color: c.avatar_color.clone(),
                        preview,
                        time,
                    }
                })
                .collect(),
            pinned: {
                let mut ids: Vec<_> = state.pinned_coworker_ids.iter().cloned().collect();
                ids.sort();
                ids
            },
            hidden_ids: {
                let mut ids: Vec<_> = state.hidden_coworker_ids.iter().cloned().collect();
                ids.sort();
                ids
            },
            renaming: state.renaming_coworker_id.clone(),
            account_label: state
                .account
                .as_ref()
                .map(|a| a.display_name())
                .unwrap_or_else(|| "Account".into()),
            account_email: state
                .account
                .as_ref()
                .map(|a| a.email.clone())
                .unwrap_or_default(),
            thread_list_unavailable: state.thread_list_unavailable,
        }
    }
}

#[derive(Clone)]
struct RailHover {
    id: String,
    bounds: Bounds<Pixels>,
    name: String,
    shape: Option<String>,
    color: Option<String>,
    preview: String,
    time: String,
}

pub struct SidebarView {
    state: Entity<AppState>,
    list_scroll: ScrollHandle,
    rename_input: Entity<InputState>,
    search_input: Entity<InputState>,
    rev: SidebarRev,
    rail_hover: Option<RailHover>,
    hover_close: Option<Task<()>>,
    hover_epoch: usize,
    hidden_row_hover: bool,
}

impl SidebarView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let rename_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let rev = SidebarRev::from_state(&state.read(cx));
        cx.observe(&state, |this, state, cx| {
            let rev = SidebarRev::from_state(&state.read(cx));
            if this.rev != rev {
                this.rev = rev;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&rename_input, window, |this, input, event, _window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let name = input.read(cx).value().to_string();
                this.state.update(cx, |state, cx| {
                    state.commit_rename_coworker(name, cx);
                });
            }
        })
        .detach();
        cx.subscribe(&search_input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();

        Self {
            state,
            list_scroll: ScrollHandle::new(),
            rename_input,
            search_input,
            rev,
            rail_hover: None,
            hover_close: None,
            hover_epoch: 0,
            hidden_row_hover: false,
        }
    }

    fn show_rail_hover(&mut self, hover: RailHover, cx: &mut Context<Self>) {
        self.hover_close = None;
        self.hover_epoch += 1;
        self.rail_hover = Some(hover);
        cx.notify();
    }

    fn keep_rail_hover(&mut self) {
        self.hover_close = None;
        self.hover_epoch += 1;
    }

    pub fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub fn search_is_focused(&self, window: &Window, cx: &App) -> bool {
        self.search_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }

    pub fn clear_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    fn schedule_hide_rail_hover(&mut self, cx: &mut Context<Self>) {
        self.hover_epoch += 1;
        let epoch = self.hover_epoch;
        self.hover_close = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(140))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.hover_epoch == epoch {
                    this.rail_hover = None;
                    this.hover_close = None;
                    cx.notify();
                }
            });
        }));
    }
}

fn hover_fill() -> Hsla {
    rgb(RAIL_HOVER).opacity(RAIL_HOVER_ALPHA).into()
}

fn row() -> Div {
    div()
        .h(px(SIDEBAR_ROW))
        .min_h(px(SIDEBAR_ROW))
        .flex()
        .items_center()
        .flex_shrink_0()
}

fn kit_icon(path: &'static str, size: f32, color: Hsla) -> Icon {
    Icon::default().path(path).size(px(size)).text_color(color)
}

impl Render for SidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let collapsed = state.sidebar_collapsed;
        let hidden = state.sidebar_hidden;
        if hidden {
            return div().id("sidebar").w(px(0.)).h_full().overflow_hidden();
        }
        let theme = cx.theme().clone();
        let coworkers = state.ranked_coworkers();
        let conversations = state.conversations.clone();
        let active_coworker = state.active_coworker_id.clone();
        let hidden_ids = state.hidden_coworker_ids.clone();
        let thread_list_unavailable = state.thread_list_unavailable;
        let any_modal_open = state.is_voice_mode_open || state.is_app_settings_open;
        let account_label = state
            .account
            .as_ref()
            .map(|a| a.display_name())
            .unwrap_or_else(|| "Account".into());
        let account_email = state
            .account
            .as_ref()
            .map(|a| a.email.clone())
            .unwrap_or_default();
        let account_id = state
            .account
            .as_ref()
            .map(|a| a.id.clone())
            .unwrap_or_else(|| "account".into());
        let view = cx.entity().clone();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let icon_color = fg;
        let rail_hover = hover_fill();

        let app = self.state.clone();
        v_flex()
            .id("sidebar")
            .size_full()
            .flex_shrink_0()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                app.update(cx, |state, cx| {
                    if state.model_picker_open || state.avatar_editor_open {
                        state.dismiss_popovers(cx);
                    }
                });
            })
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .id("sidebar-main")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .gap(px(SIDEBAR_GAP))
                    .child(self.brand_row(
                        collapsed,
                        any_modal_open,
                        state.bot_finder_open,
                        fg,
                        muted,
                        rail_hover,
                        view.clone(),
                    ))
                    .child(self.search_row(
                        collapsed,
                        state.bot_finder_open,
                        icon_color,
                        rail_hover,
                        view.clone(),
                    ))
                    .child(
                        div()
                            .id("sidebar-chat-list")
                            .flex_1()
                            .min_h(px(0.))
                            .w_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.list_scroll)
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap(px(SIDEBAR_GAP))
                                    .when(collapsed, |this| this.items_center())
                                    .when(!collapsed, |this| this.px(px(12.)))
                                    .pb(px(12.))
                                    .children({
                                        let mut rows: Vec<_> = coworkers.into_iter().collect();
                                        let hidden = state.hidden_coworker_ids.clone();
                                        let pinned = state.pinned_coworker_ids.clone();
                                        let renaming = state.renaming_coworker_id.clone();
                                        let rename_input = self.rename_input.clone();
                                        let app = self.state.clone();
                                        let list_view = view.clone();
                                        rows.retain(|c| !hidden.contains(&c.id));
                                        rows.sort_by_key(|c| !pinned.contains(&c.id));
                                        rows.into_iter().map(move |c| {
                                            let id = c.id.clone();
                                            let name = c.name.clone();
                                            let is_active = Some(id.clone()) == active_coworker;
                                            let is_renaming = renaming.as_ref() == Some(&id);
                                            let is_pinned = pinned.contains(&id);
                                            let (preview, when) = rail_preview(
                                                conversations.iter().find(|conv| conv.id == id),
                                            );
                                            let view = list_view.clone();
                                            let app = app.clone();
                                            let row =
                                                row()
                                                    .id(SharedString::from(format!(
                                                        "coworker-{id}"
                                                    )))
                                                    .when(collapsed, |this| {
                                                        this.w(px(SIDEBAR_ROW))
                                                            .justify_center()
                                                            .px(px(9.))
                                                            .rounded(px(10.))
                                                    })
                                                    .when(!collapsed, |this| {
                                                        this.w_full()
                                                            .px(px(9.))
                                                            .gap(px(9.))
                                                            .rounded(px(10.))
                                                    })
                                                    .cursor_pointer()
                                                    .when(!collapsed, |this| {
                                                        this.hover(|s| s.bg(rail_hover))
                                                            .when(is_active, |this| {
                                                                this.bg(rail_hover)
                                                            })
                                                    })
                                                    .on_mouse_down(MouseButton::Left, {
                                                        let id = id.clone();
                                                        let view = view.clone();
                                                        move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.state.update(cx, |state, cx| {
                                                            if state.renaming_coworker_id.as_ref()
                                                                != Some(&id)
                                                            {
                                                                state.cancel_rename_coworker(cx);
                                                            }
                                                            state.select_coworker(id.clone(), cx);
                                                        });
                                                            });
                                                        }
                                                    })
                                                    .context_menu({
                                                        let app = app.clone();
                                                        let view = view.clone();
                                                        let id = id.clone();
                                                        let name = name.clone();
                                                        move |menu, _, _cx| {
                                                            agent_menu(
                                                                menu,
                                                                app.clone(),
                                                                view.clone(),
                                                                id.clone(),
                                                                name.clone(),
                                                                is_pinned,
                                                            )
                                                        }
                                                    })
                                                    .child(
                                                        PersonaMark::new(id.clone())
                                                            .shape(c.avatar_shape.clone())
                                                            .color(c.avatar_color.clone())
                                                            .size(px(AVATAR_PX))
                                                            .lit(is_active),
                                                    );
                                            let row = if collapsed {
                                                row.into_any_element()
                                            } else if is_renaming {
                                                row.child(
                                                    div().min_w(px(0.)).flex_1().child(
                                                        Input::new(&rename_input)
                                                            .appearance(false)
                                                            .focus_bordered(false)
                                                            .h(px(28.)),
                                                    ),
                                                )
                                                .into_any_element()
                                            } else {
                                                row.child(
                                                    div()
                                                        .min_w(px(0.))
                                                        .flex_1()
                                                        .text_sm()
                                                        .truncate()
                                                        .child(name.clone()),
                                                )
                                                .into_any_element()
                                            };
                                            div()
                                                .id(SharedString::from(format!(
                                                    "coworker-hover-{id}"
                                                )))
                                                .flex_shrink_0()
                                                .when(!collapsed, |this| this.w_full())
                                                .on_hover({
                                                    let view = view.clone();
                                                    let id = id.clone();
                                                    let name = name.clone();
                                                    let shape = c.avatar_shape.clone();
                                                    let color = c.avatar_color.clone();
                                                    move |hovered, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            if *hovered {
                                                                let bounds = this
                                                                    .rail_hover
                                                                    .as_ref()
                                                                    .filter(|h| h.id == id)
                                                                    .map(|h| h.bounds)
                                                                    .unwrap_or_else(
                                                                        Bounds::default,
                                                                    );
                                                                this.show_rail_hover(
                                                                    RailHover {
                                                                        id: id.clone(),
                                                                        bounds,
                                                                        name: name.clone(),
                                                                        shape: shape.clone(),
                                                                        color: color.clone(),
                                                                        preview: preview.clone(),
                                                                        time: when.clone(),
                                                                    },
                                                                    cx,
                                                                );
                                                            } else {
                                                                this.schedule_hide_rail_hover(cx);
                                                            }
                                                        });
                                                    }
                                                })
                                                .on_prepaint({
                                                    let view = view.clone();
                                                    let id = id.clone();
                                                    move |bounds, _, cx| {
                                                        view.update(cx, |this, cx| {
                                                            if let Some(hover) =
                                                                this.rail_hover.as_mut()
                                                                && hover.id == id
                                                                && hover.bounds != bounds
                                                            {
                                                                hover.bounds = bounds;
                                                                cx.notify();
                                                            }
                                                        });
                                                    }
                                                })
                                                .child(row)
                                                .into_any_element()
                                        })
                                    })
                                    .when(!collapsed && !hidden_ids.is_empty(), |this| {
                                        this.child(self.hidden_bots_row(
                                            hidden_ids.len(),
                                            muted,
                                            fg,
                                            rail_hover,
                                            view.clone(),
                                        ))
                                    })
                                    // Under the bots it is about: their order and times are
                                    // only what this Mac knew.
                                    .when(!collapsed && thread_list_unavailable, |this| {
                                        this.child(
                                            div()
                                                .id("thread-list-unavailable")
                                                .w_full()
                                                .px(px(9.))
                                                .text_xs()
                                                .text_color(muted)
                                                .child(THREAD_LIST_UNAVAILABLE),
                                        )
                                    }),
                            ),
                    ),
            )
            .child(self.dock(
                collapsed,
                cx.theme().is_dark(),
                account_id,
                account_label,
                account_email,
                any_modal_open,
                fg,
                muted,
                rail_hover,
                view.clone(),
            ))
            .when_some(self.rail_hover.clone(), |this, hover| {
                if hover.bounds.size.width <= px(0.) {
                    return this;
                }
                let view = view.clone();
                this.child(
                    deferred(
                        Positioner::side(hover.bounds)
                            .placement(Placement::Right)
                            .align(Align::Start)
                            .offset(px(8.))
                            .occlude()
                            .child(
                                div()
                                    .id("coworker-preview-hit")
                                    .on_hover(move |hovered, _, cx| {
                                        view.update(cx, |this, cx| {
                                            if *hovered {
                                                this.keep_rail_hover();
                                            } else {
                                                this.schedule_hide_rail_hover(cx);
                                            }
                                        });
                                    })
                                    .child(agent_hover_card(
                                        hover.id,
                                        hover.name,
                                        hover.shape,
                                        hover.color,
                                        hover.preview,
                                        hover.time,
                                        theme.is_dark(),
                                        muted,
                                        fg,
                                        theme.border,
                                    )),
                            ),
                    )
                    .with_priority(POPUP_PRIORITY),
                )
            })
    }
}

impl SidebarView {
    fn brand_row(
        &self,
        collapsed: bool,
        any_modal_open: bool,
        finder_open: bool,
        fg: Hsla,
        muted: Hsla,
        hover: Hsla,
        view: Entity<Self>,
    ) -> impl IntoElement {
        row()
            .id("sidebar-header")
            .w_full()
            .when(collapsed, |this| this.justify_center())
            .when(!collapsed, |this| this.px(px(12.)).pl(px(16.)).gap(px(8.)))
            .when(collapsed, |this| {
                this.child(
                    div()
                        .id("nav-toggle-sidebar")
                        .w(px(SIDEBAR_ROW))
                        .h(px(SIDEBAR_ROW))
                        .rounded(px(10.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(hover))
                        .on_mouse_down(MouseButton::Left, {
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.state.update(cx, |state, cx| {
                                        state.set_sidebar_collapsed(false, false, cx);
                                    });
                                });
                            }
                        })
                        .child(kit_icon("icons/panel-left.svg", 16.0, fg)),
                )
            })
            .when(!collapsed, |this| {
                this.child(
                    div()
                        .size(px(MASCOT_BOX_PX))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .child(
                            PersonaMark::new("nativechat-brand")
                                .shape(Some("blob"))
                                .color(Some("cyan"))
                                .size(px(AVATAR_PX)),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("NativeChat"),
                )
                .child(
                    div()
                        .id("nav-new-chat")
                        .ml_auto()
                        .size(px(28.))
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(if finder_open { fg } else { muted })
                        .cursor_pointer()
                        .when(finder_open, |this| this.bg(hover))
                        .hover(|s| s.bg(hover).text_color(fg))
                        .when(!any_modal_open, |this| {
                            this.on_mouse_down(MouseButton::Left, {
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.state.update(cx, |state, cx| {
                                            if state.bot_finder_open {
                                                state.close_bot_finder(cx);
                                            } else {
                                                state.open_bot_finder(cx);
                                            }
                                        });
                                    });
                                }
                            })
                        })
                        .child(Icon::new(IconName::Plus).size(px(16.))),
                )
            })
    }

    fn hidden_bots_row(
        &self,
        count: usize,
        muted: Hsla,
        fg: Hsla,
        hover: Hsla,
        view: Entity<Self>,
    ) -> impl IntoElement {
        let show_chevron = self.hidden_row_hover;
        row()
            .id("hidden-bots-row")
            .w_full()
            .px(px(9.))
            .rounded(px(10.))
            .cursor_pointer()
            .hover(|s| s.bg(hover))
            .on_hover({
                let view = view.clone();
                move |hovered, _, cx| {
                    view.update(cx, |this, cx| {
                        if this.hidden_row_hover != *hovered {
                            this.hidden_row_hover = *hovered;
                            cx.notify();
                        }
                    });
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.state
                            .update(cx, |state, cx| state.open_hidden_bots(cx));
                    });
                }
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().text_color(muted).child("Hidden Bots"))
                    .child(if show_chevron {
                        Icon::new(IconName::ChevronRight)
                            .size(px(14.))
                            .text_color(fg)
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(count.to_string())
                            .into_any_element()
                    }),
            )
    }

    fn search_row(
        &self,
        collapsed: bool,
        finder_open: bool,
        icon_color: Hsla,
        hover: Hsla,
        view: Entity<Self>,
    ) -> impl IntoElement {
        // Same 54px track as brand/avatars so collapse only moves this slot sideways.
        let track = row().id("nav-search").w_full();
        if collapsed {
            track.justify_center().child(
                div()
                    .id("nav-new-chat")
                    .w(px(SIDEBAR_ROW))
                    .h(px(SIDEBAR_ROW))
                    .rounded(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(finder_open, |this| this.bg(hover))
                    .hover(|s| s.bg(hover))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.update(cx, |state, cx| {
                                if state.bot_finder_open {
                                    state.close_bot_finder(cx);
                                } else {
                                    state.open_bot_finder(cx);
                                }
                            });
                        });
                    })
                    .child(
                        Icon::new(IconName::Plus)
                            .size(px(16.))
                            .text_color(icon_color),
                    ),
            )
        } else {
            track.px(px(12.)).child(
                h_flex()
                    .id("sidebar-search")
                    .w_full()
                    .h(px(36.))
                    .px(px(10.))
                    .gap(px(8.))
                    .items_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(0x777777).opacity(0.28))
                    .cursor_pointer()
                    .hover(|s| s.bg(hover))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.update(cx, |state, cx| {
                                state.open_command_palette(cx);
                            });
                        });
                    })
                    .child(
                        Icon::new(IconName::Search)
                            .size(px(16.))
                            .text_color(icon_color),
                    )
                    .child(div().text_sm().text_color(icon_color).child("Search")),
            )
        }
    }

    fn dock(
        &self,
        collapsed: bool,
        is_dark: bool,
        account_id: String,
        account_label: String,
        account_email: String,
        _any_modal_open: bool,
        fg: Hsla,
        muted: Hsla,
        hover: Hsla,
        view: Entity<Self>,
    ) -> impl IntoElement {
        let (theme_label, theme_icon) = if is_dark {
            ("Theme: Dark", "icons/moon.svg")
        } else {
            ("Theme: Light", "icons/sun.svg")
        };

        v_flex()
            .id("sidebar-footer")
            .flex_shrink_0()
            .w_full()
            .gap(px(SIDEBAR_GAP))
            .when(collapsed, |this| this.pb(px(8.)).items_center())
            .when(!collapsed, |this| this.px(px(12.)).pb(px(8.)))
            .child(self.dock_item(
                "nav-library",
                collapsed,
                NativeIcon::Collections.path(),
                "Collections",
                fg,
                hover,
                None,
            ))
            .child(self.dock_item(
                "nav-projects",
                collapsed,
                "icons/groups.svg",
                "Groups",
                fg,
                hover,
                None,
            ))
            .child({
                let view = view.clone();
                self.dock_item(
                    "nav-recipes",
                    collapsed,
                    "icons/record.svg",
                    "Recipes",
                    fg,
                    hover,
                    Some(Box::new(move |cx: &mut App| {
                        view.update(cx, |this, cx| {
                            this.state.update(cx, |state, cx| {
                                state.open_recipes(cx);
                            });
                        });
                    })),
                )
            })
            .child(self.dock_item(
                "footer-plugins",
                collapsed,
                NativeIcon::Plugins.path(),
                "Plugins",
                fg,
                hover,
                None,
            ))
            .child({
                let view = view.clone();
                self.dock_item(
                    "footer-theme",
                    collapsed,
                    theme_icon,
                    theme_label,
                    fg,
                    hover,
                    Some(Box::new(move |cx: &mut App| {
                        view.update(cx, |this, cx| {
                            this.state.update(cx, |state, cx| {
                                state.toggle_theme(cx);
                            });
                        });
                    })),
                )
            })
            .child({
                let view = view.clone();
                let label = account_label.clone();
                let email = account_email.clone();
                row()
                    .id("footer-account")
                    .when(collapsed, |this| {
                        this.w(px(SIDEBAR_ROW))
                            .justify_center()
                            .px(px(9.))
                            .rounded(px(10.))
                    })
                    .when(!collapsed, |this| {
                        this.w_full().px(px(8.)).gap(px(8.)).rounded(px(10.))
                    })
                    .cursor_pointer()
                    .when(!collapsed, |this| this.hover(|s| s.bg(hover)))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.update(cx, |state, cx| {
                                state.open_app_settings(AppSettingsTab::Profile, cx);
                            });
                        });
                    })
                    .child(
                        PersonaMark::new(account_id)
                            .shape(Some("squircle"))
                            .color(Some("gray"))
                            .size(px(AVATAR_PX)),
                    )
                    .when(!collapsed, |this| {
                        this.child(
                            v_flex()
                                .min_w(px(0.))
                                .flex_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .truncate()
                                        .child(label),
                                )
                                .when(!email.is_empty(), |this| {
                                    this.child(
                                        div().text_xs().text_color(muted).truncate().child(email),
                                    )
                                }),
                        )
                    })
            })
    }

    fn dock_item(
        &self,
        id: &'static str,
        collapsed: bool,
        icon: impl Into<SharedString>,
        label: &'static str,
        fg: Hsla,
        hover: Hsla,
        on_click: Option<Box<dyn Fn(&mut App) + 'static>>,
    ) -> impl IntoElement {
        let icon = icon.into();
        row()
            .id(id)
            .when(collapsed, |this| {
                this.w(px(SIDEBAR_ROW))
                    .justify_center()
                    .px(px(9.))
                    .rounded(px(10.))
            })
            .when(!collapsed, |this| {
                this.w_full().px(px(8.)).gap(px(10.)).rounded(px(10.))
            })
            .cursor_pointer()
            .hover(|s| s.bg(hover))
            .when_some(on_click, |this, on_click| {
                this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    on_click(cx);
                })
            })
            .child(Icon::default().path(icon).size(px(16.)).text_color(fg))
            .when(!collapsed, |this| this.child(div().text_sm().child(label)))
    }
}

fn rail_preview(conversation: Option<&Conversation>) -> (String, String) {
    let Some(conversation) = conversation else {
        return ("No messages yet".into(), String::new());
    };
    let last = conversation
        .messages
        .iter()
        .rev()
        // What the person hid is not what the sidebar says about the thread.
        .find(|message| !message.hidden && !message.content.trim().is_empty());
    let preview = last
        .map(|message| rail_preview_text(&message.content))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "No messages yet".into());
    let since = if conversation.known_only_from_list() {
        &conversation.updated_at
    } else {
        &conversation.created_at
    };
    let time = match last {
        Some(message) => rail_card_time(message.sent_at),
        None => NaiveDateTime::parse_from_str(since, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|dt| rail_card_time(SystemTime::from(dt.and_utc())))
            .unwrap_or_default(),
    };
    (preview, time)
}

fn rail_preview_text(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX: usize = 140;
    let count = collapsed.chars().count();
    if count > MAX {
        format!("{}…", collapsed.chars().take(MAX).collect::<String>())
    } else {
        collapsed
    }
}

fn rail_card_time(at: SystemTime) -> String {
    let at = chrono::DateTime::<chrono::Local>::from(at);
    let today = chrono::Local::now().date_naive();
    let then = at.date_naive();
    match (today - then).num_days() {
        0 => at.format("%l:%M %p").to_string().trim().to_string(),
        1 => "Yesterday".into(),
        2..=6 => at.format("%A").to_string(),
        _ => at.format("%b %d").to_string(),
    }
}

fn agent_hover_card(
    id: String,
    name: String,
    shape: Option<String>,
    color: Option<String>,
    preview: String,
    time: String,
    dark: bool,
    muted: Hsla,
    foreground: Hsla,
    border: Hsla,
) -> impl IntoElement {
    let panel_bg = if dark { rgb(0x1c1c1c) } else { rgb(0xffffff) };
    v_flex()
        .id(SharedString::from(format!("coworker-preview-{id}")))
        .w(px(260.))
        .p(px(10.))
        .gap(px(4.))
        .rounded(px(10.))
        .border_1()
        .border_color(border)
        .bg(panel_bg)
        .text_color(foreground)
        .shadow_lg()
        .occlude()
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .child(
                    PersonaMark::new(id)
                        .shape(shape)
                        .color(color)
                        .size(px(16.))
                        .dark(dark),
                )
                .child(
                    div()
                        .min_w(px(0.))
                        .flex_1()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(name),
                )
                .when(!time.is_empty(), |this| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(time),
                    )
                }),
        )
        .child(div().w_full().text_xs().text_color(muted).child(preview))
}

fn agent_menu(
    menu: PopupMenu,
    app: Entity<AppState>,
    view: Entity<SidebarView>,
    id: String,
    name: String,
    pinned: bool,
) -> PopupMenu {
    menu.item(PopupMenuItem::new("Pin").checked(pinned).on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.toggle_pin_coworker(id.clone(), cx));
        }
    }))
    .item(PopupMenuItem::new("Move to new section").disabled(true))
    .item(PopupMenuItem::new("Mark as Read").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.mark_coworker_read(&id, cx));
        }
    }))
    .separator()
    .item(PopupMenuItem::new("Rename Bot").on_click({
        let app = app.clone();
        let id = id.clone();
        let name = name.clone();
        move |_, window, cx| {
            view.update(cx, |this, cx| {
                this.rename_input.update(cx, |input, cx| {
                    input.set_value(name.clone(), window, cx);
                    input.focus_handle(cx).focus(window, cx);
                });
            });
            app.update(cx, |state, cx| {
                state.begin_rename_coworker(id.clone(), cx);
            });
        }
    }))
    .item(PopupMenuItem::new("Edit Profile").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.open_agent_profile(id.clone(), cx));
        }
    }))
    .item(PopupMenuItem::new("Duplicate").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.duplicate_coworker(id.clone(), cx));
        }
    }))
    .item(PopupMenuItem::new("Copy conversation ID").on_click({
        let id = id.clone();
        move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(id.clone()));
        }
    }))
    .separator()
    .item(PopupMenuItem::new("Hide from sidebar").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.hide_coworker(id.clone(), cx));
        }
    }))
    .item(PopupMenuItem::new("Delete").on_click({
        let app = app.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.delete_coworker(id.clone(), cx));
        }
    }))
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob: this module globs `gpui_kit::*`, whose root re-exports GPUI's
    // own `test` attribute, and a glob here would make every `#[test]` below resolve to it.
    use super::{rail_card_time, rail_preview};
    use crate::state::Conversation;
    use std::time::{Duration, SystemTime};

    fn unopened(created_at: &str, updated_at: &str) -> Conversation {
        Conversation {
            id: "cw_1".to_string(),
            title: "cw_1".to_string(),
            created_at: created_at.to_string(),
            updated_at: updated_at.to_string(),
            messages: Vec::new(),
            unread_count: 0,
        }
    }

    /// A thread the server listed has no start this Mac knows of, and its card carries when it
    /// last moved rather than no time at all. One this Mac began keeps the time it began.
    #[test]
    fn a_listed_thread_is_dated_by_when_it_last_moved() {
        let (_, listed) = rail_preview(Some(&unopened("", "2026-09-21 14:13:20")));
        assert_eq!(
            listed,
            rail_card_time(SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_000_000))
        );
        assert!(!listed.is_empty());

        let (_, local) = rail_preview(Some(&unopened(
            "2026-09-01 10:00:00",
            "2026-09-21 14:13:20",
        )));
        assert_eq!(
            local,
            rail_card_time(SystemTime::UNIX_EPOCH + Duration::from_secs(1_788_256_800))
        );
    }
}
