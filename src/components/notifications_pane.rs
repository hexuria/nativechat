//! The bell's page in the right sidebar: what went wrong for the open Bot, newest first, each
//! with where in the app and in the source it was caught, and Copy for handing it over whole
//! (8 Oct 2026). It takes the place of the Bot's settings while it is open.

use crate::chrome::{HEADER_PX, INFO_PANE_WIDTH, TITLE_BAR_H};
use crate::components::title_bar::{PANE_ROW_UNDER_BUTTONS, window_drag};
use crate::state::AppState;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub const PANE: &str = "notifications-pane";
pub const EMPTY: &str = "notifications-empty";
pub const EMPTY_UNREAD: &str = "notifications-empty-unread";
pub const SHOW_ALL: &str = "notifications-show-all";
pub const UNREAD_COUNT: &str = "notifications-unread-count";
pub const TOOLBAR: &str = "notifications-toolbar";
pub const SELECT_ALL: &str = "notifications-select-all";
pub const FILTER_ALL: &str = "notifications-filter-all";
pub const FILTER_UNREAD: &str = "notifications-filter-unread";
pub const SELECTED_COUNT: &str = "notifications-selected-count";
pub const MARK_ALL_READ: &str = "notifications-mark-all-read";
pub const DELETE_ALL: &str = "notifications-delete-all";
pub const MARK_READ: &str = "notifications-mark-read";
pub const MARK_UNREAD: &str = "notifications-mark-unread";
pub const DELETE_SELECTED: &str = "notifications-delete-selected";
pub const UNDO_BAR: &str = "notifications-undo-bar";
pub const UNDO: &str = "notifications-undo";

/// One notice's row and its controls.
pub fn row_id(id: &str) -> String {
    format!("notification-{id}")
}
/// A fault notice's "Go to …" link, `notification-go-<id>`.
pub fn go_to_id(id: &str) -> String {
    format!("notification-go-{id}")
}

pub fn copy_id(id: &str) -> String {
    format!("notification-copy-{id}")
}
pub fn select_id(id: &str) -> String {
    format!("notification-select-{id}")
}
pub fn read_toggle_id(id: &str) -> String {
    format!("notification-read-toggle-{id}")
}
pub fn delete_id(id: &str) -> String {
    format!("notification-delete-{id}")
}
pub fn details_id(id: &str) -> String {
    format!("notification-details-{id}")
}
pub fn day_id(day: &str) -> String {
    format!("notifications-day-{day}")
}

pub struct NotificationsPane {
    state: Entity<AppState>,
}

impl NotificationsPane {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

/// A 16px tick box: empty, ticked, or a dash for some (the select-all box). Drawn here because
/// the kit's checkbox drew nothing against this pane's fill.
fn tick_box(
    id: impl Into<SharedString>,
    ticked: bool,
    some: bool,
    theme: &gpui_kit::component::Theme,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let on = ticked || some;
    div()
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .size(px(16.))
        .flex_shrink_0()
        .rounded(px(4.))
        .border_1()
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .map(|this| {
            if on {
                this.bg(theme.primary).border_color(theme.primary)
            } else {
                this.bg(theme.background)
                    .border_color(theme.muted_foreground.opacity(0.6))
            }
        })
        .when(ticked, |this| {
            this.child(
                Icon::default()
                    .path("icons/check.svg")
                    .size(px(12.))
                    .text_color(theme.primary_foreground),
            )
        })
        .when(some && !ticked, |this| {
            this.child(
                div()
                    .w(px(8.))
                    .h(px(2.))
                    .rounded_full()
                    .bg(theme.primary_foreground),
            )
        })
}

/// A small text button, for the toast.
fn text_button(id: SharedString, label: &'static str, color: Hsla) -> Stateful<Div> {
    div()
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .text_xs()
        .px(px(8.))
        .py(px(3.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_color(color)
        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
        .child(label)
}

/// A 28px icon button with its tooltip; danger on hover for a delete.
fn icon_button(
    id: impl Into<SharedString>,
    icon: &'static str,
    tip: &'static str,
    enabled: bool,
    danger: bool,
    theme: &gpui_kit::component::Theme,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let (muted, red) = (theme.muted_foreground, theme.danger);
    div()
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .size(px(28.))
        .flex_shrink_0()
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(muted)
        .when(!enabled, |this| this.opacity(0.4))
        .when(enabled, |this| {
            this.cursor_pointer().hover(move |s| {
                let s = s.bg(rgb(0x777777).opacity(0.2));
                if danger { s.text_color(red) } else { s }
            })
        })
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tip).build(window, cx)
        })
        .child(Icon::default().path(icon).size(px(16.)))
}

/// "now", "4m", "2h", or the clock for older; the day header carries the date.
fn relative(at_ms: i64) -> String {
    let now = chrono::Local::now().timestamp_millis();
    let mins = (now - at_ms).max(0) / 60_000;
    match mins {
        0 => "now".into(),
        1..=59 => format!("{mins}m"),
        60..=359 => format!("{}h", mins / 60),
        _ => chrono::DateTime::from_timestamp_millis(at_ms)
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_default(),
    }
}

/// "Today", "Yesterday", or "Mon, Oct 6", and the day's key for its id.
fn day_of(at_ms: i64) -> (String, String) {
    let Some(t) = chrono::DateTime::from_timestamp_millis(at_ms) else {
        return (String::new(), String::new());
    };
    let day = t.with_timezone(&chrono::Local).date_naive();
    let today = chrono::Local::now().date_naive();
    let label = if day == today {
        "Today".into()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".into()
    } else {
        day.format("%a, %b %-d").to_string()
    };
    (label, day.format("%Y-%m-%d").to_string())
}

impl Render for NotificationsPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let app = self.state.clone();
        let state = self.state.read(cx);
        let all_count = state.bot_notices().len();
        let unread = state.bot_notices().iter().filter(|n| !n.read).count();
        let shown: Vec<crate::notifications::Notice> =
            state.visible_notices().into_iter().cloned().collect();
        let selection = state.notice_selection.clone();
        let unread_only = state.notice_unread_only;
        let expanded = state.notice_expanded.clone();
        let undo = state
            .notice_undo
            .as_ref()
            .map(|(_, _, label)| label.clone());
        let bot_name = state
            .active_coworker_id
            .as_ref()
            .and_then(|id| state.coworkers.iter().find(|c| &c.id == id))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "this Bot".into());
        let muted = theme.muted_foreground;
        let selecting = !selection.is_empty();

        // Title row: the title and how many are unread. Its right end is the title bar's.
        let header = h_flex()
            .id("notifications-header")
            .w_full()
            .h(px(TITLE_BAR_H))
            .flex_shrink_0()
            .pl(px(HEADER_PX))
            .pr(px(PANE_ROW_UNDER_BUTTONS))
            .items_center()
            .gap(px(8.))
            .child(window_drag(
                h_flex()
                    .h_full()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Notifications"),
                    )
                    .when(unread > 0, |this| {
                        this.child(
                            div()
                                .id(UNREAD_COUNT)
                                .debug_selector(|| UNREAD_COUNT.into())
                                .h(px(18.))
                                .px(px(6.))
                                .rounded_full()
                                .bg(theme.danger.opacity(0.15))
                                .text_color(theme.danger)
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .flex()
                                .items_center()
                                .child(unread.to_string()),
                        )
                    }),
            ));

        // Toolbar: select all, then the filter or "N selected", then what can be done.
        let shown_ids: Vec<String> = shown.iter().map(|n| n.id.clone()).collect();
        let all_ticked = !shown_ids.is_empty() && shown_ids.iter().all(|id| selection.contains(id));
        let segment = |id: &'static str, label: String, on: bool, unread_only: bool| {
            let app = app.clone();
            div()
                .id(id)
                .debug_selector(move || id.into())
                .h(px(20.))
                .px(px(8.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .text_xs()
                .cursor_pointer()
                .when(on, |this| this.bg(theme.background).shadow_sm())
                .text_color(if on { theme.foreground } else { muted })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    app.update(cx, |state, cx| state.set_notice_filter(unread_only, cx));
                })
                .child(label)
        };
        let toolbar = {
            let select_all = app.clone();
            let mut bar = h_flex()
                .id(TOOLBAR)
                .debug_selector(|| TOOLBAR.into())
                .w_full()
                .h(px(36.))
                .flex_shrink_0()
                .px(px(12.))
                .gap(px(4.))
                .items_center()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .size(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            tick_box(SELECT_ALL, all_ticked, selecting && !all_ticked, &theme)
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                    select_all.update(cx, |state, cx| {
                                        state.toggle_select_all_notices(cx)
                                    });
                                }),
                        ),
                );
            bar = if selecting {
                bar.child(
                    div()
                        .id(SELECTED_COUNT)
                        .debug_selector(|| SELECTED_COUNT.into())
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{} selected", selection.len())),
                )
            } else {
                bar.child(
                    h_flex()
                        .p(px(2.))
                        .gap(px(2.))
                        .rounded(px(8.))
                        .bg(rgb(0x777777).opacity(0.12))
                        .child(segment(FILTER_ALL, "All".into(), !unread_only, false))
                        .child(segment(
                            FILTER_UNREAD,
                            format!("Unread ({unread})"),
                            unread_only,
                            true,
                        )),
                )
            };
            bar = bar.child(div().flex_1());
            if selecting {
                let (r, u, d) = (app.clone(), app.clone(), app.clone());
                let ids: Vec<String> = selection.iter().cloned().collect();
                bar.child(
                    icon_button(
                        MARK_READ,
                        "icons/mail-open.svg",
                        "Mark read",
                        true,
                        false,
                        &theme,
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        r.update(cx, |state, cx| state.mark_selected_notices(true, cx));
                    }),
                )
                .child(
                    icon_button(
                        MARK_UNREAD,
                        "icons/mail.svg",
                        "Mark unread",
                        true,
                        false,
                        &theme,
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        u.update(cx, |state, cx| state.mark_selected_notices(false, cx));
                    }),
                )
                .child(
                    icon_button(
                        DELETE_SELECTED,
                        "icons/trash.svg",
                        "Delete",
                        true,
                        true,
                        &theme,
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let ids = ids.clone();
                        d.update(cx, |state, cx| state.delete_notices(ids, cx));
                    }),
                )
            } else {
                let (r, d) = (app.clone(), app.clone());
                let ids: Vec<String> = shown_ids.clone();
                let unread_ids: Vec<String> = shown
                    .iter()
                    .filter(|n| !n.read)
                    .map(|n| n.id.clone())
                    .collect();
                bar.child(
                    icon_button(
                        MARK_ALL_READ,
                        "icons/check-check.svg",
                        "Mark all read",
                        unread > 0,
                        false,
                        &theme,
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let ids = unread_ids.clone();
                        r.update(cx, |state, cx| state.set_notices_read(ids, true, cx));
                    }),
                )
                .child(
                    icon_button(
                        DELETE_ALL,
                        "icons/trash.svg",
                        "Delete all",
                        !ids.is_empty(),
                        true,
                        &theme,
                    )
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let ids = ids.clone();
                        d.update(cx, |state, cx| state.delete_notices(ids, cx));
                    }),
                )
            }
        };

        let body = if all_count == 0 {
            v_flex()
                .id(EMPTY)
                .debug_selector(|| EMPTY.into())
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .px(px(24.))
                .child(Icon::default().path("icons/bell.svg").size(px(32.)).text_color(muted))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("You're all caught up"))
                .child(div().text_xs().text_color(muted).text_center().child(format!(
                    "When something goes wrong for {bot_name}, it shows up here with where it happened, so you can trace it back and copy it."
                )))
                .into_any_element()
        } else if shown.is_empty() {
            let show_all = app.clone();
            v_flex()
                .id(EMPTY_UNREAD)
                .debug_selector(|| EMPTY_UNREAD.into())
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(
                    Icon::default()
                        .path("icons/check-circle.svg")
                        .size(px(28.))
                        .text_color(muted),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child("No unread notifications"),
                )
                .child(
                    div()
                        .id(SHOW_ALL)
                        .debug_selector(|| SHOW_ALL.into())
                        .text_xs()
                        .text_color(theme.primary)
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            show_all.update(cx, |state, cx| state.set_notice_filter(false, cx));
                        })
                        .child("Show all"),
                )
                .into_any_element()
        } else {
            let mut list = v_flex()
                .id("notifications-list")
                .debug_selector(|| "notifications-list".into())
                .flex_1()
                .overflow_y_scroll()
                .px(px(12.))
                .pt(px(8.))
                .pb(px(56.))
                .gap(px(6.));
            let mut last_day = String::new();
            for notice in shown {
                let (label, key) = day_of(notice.at_ms);
                if key != last_day {
                    let did = day_id(&key);
                    list = list.child(
                        div()
                            .id(SharedString::from(did.clone()))
                            .debug_selector(move || did.clone())
                            .pt(px(12.))
                            .pb(px(4.))
                            .px(px(4.))
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(muted)
                            .child(label),
                    );
                    last_day = key;
                }
                list = list.child(notice_row(
                    &app,
                    &notice,
                    selection.contains(&notice.id),
                    selecting,
                    expanded.as_deref() == Some(notice.id.as_str()),
                    &theme,
                ));
            }
            list.into_any_element()
        };

        v_flex()
            .id(PANE)
            .debug_selector(|| PANE.into())
            .relative()
            .h_full()
            .w(px(INFO_PANE_WIDTH))
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .text_color(theme.foreground)
            .child(header)
            .when(all_count > 0, |this| this.child(toolbar))
            .child(body)
            .when_some(undo, |this, label| {
                let undo = app.clone();
                this.child(
                    h_flex()
                        .id(UNDO_BAR)
                        .debug_selector(|| UNDO_BAR.into())
                        .absolute()
                        .left(px(12.))
                        .right(px(12.))
                        .bottom(px(12.))
                        .h(px(36.))
                        .px(px(12.))
                        .rounded(px(8.))
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.border)
                        .shadow_lg()
                        .items_center()
                        .child(div().flex_1().text_xs().child(label))
                        .child(
                            div()
                                .id(UNDO)
                                .debug_selector(|| UNDO.into())
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.primary)
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    undo.update(cx, |state, cx| state.undo_notice_delete(cx));
                                })
                                .child("Undo"),
                        ),
                )
            })
    }
}

/// One notice: tick box (on hover, or while selecting), place · time, unread dot or the hover
/// actions, the sentence (three lines until opened), and opened, its details.
fn notice_row(
    app: &Entity<AppState>,
    notice: &crate::notifications::Notice,
    ticked: bool,
    selecting: bool,
    open: bool,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    let muted = theme.muted_foreground;
    let group = SharedString::from(format!("notice-{}", notice.id));
    let rid = row_id(&notice.id);
    let id = notice.id.clone();
    let (tick, toggle, expand, copy, del) = (
        app.clone(),
        app.clone(),
        app.clone(),
        app.clone(),
        app.clone(),
    );
    let when_full = chrono::DateTime::from_timestamp_millis(notice.at_ms)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%b %-d, %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();
    let read = notice.read;
    let show_box = ticked || selecting;
    // A fault's way back to where it happened: 🎯 opens the Bot and the card or page, ringed.
    let go_to = notice.fault.as_ref().map(|_| {
        let id = id.clone();
        let app = copy.clone();
        icon_button(
            go_to_id(&id),
            "icons/target.svg",
            "Show where it happened",
            true,
            false,
            theme,
        )
        .size(px(24.))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            app.update(cx, |state, cx| state.reveal_fault(&id, cx));
        })
    });
    let actions = h_flex()
        .gap(px(2.))
        .opacity(0.)
        .group_hover(group.clone(), |s| s.opacity(1.))
        .children(go_to)
        .child(
            icon_button(copy_id(&id), "icons/copy.svg", "Copy", true, false, theme)
                .size(px(24.))
                .on_mouse_down(MouseButton::Left, {
                    let id = id.clone();
                    move |_, _, cx| {
                        cx.stop_propagation();
                        copy.update(cx, |state, cx| state.copy_notice(&id, cx));
                    }
                }),
        )
        .child(
            icon_button(
                read_toggle_id(&id),
                if read {
                    "icons/mail.svg"
                } else {
                    "icons/mail-open.svg"
                },
                if read { "Mark unread" } else { "Mark read" },
                true,
                false,
                theme,
            )
            .size(px(24.))
            .on_mouse_down(MouseButton::Left, {
                let id = id.clone();
                move |_, _, cx| {
                    cx.stop_propagation();
                    toggle.update(cx, |state, cx| {
                        state.set_notices_read(vec![id.clone()], !read, cx)
                    });
                }
            }),
        )
        .child(
            icon_button(
                delete_id(&id),
                "icons/trash.svg",
                "Delete",
                true,
                true,
                theme,
            )
            .size(px(24.))
            .on_mouse_down(MouseButton::Left, {
                let id = id.clone();
                move |_, _, cx| {
                    cx.stop_propagation();
                    del.update(cx, |state, cx| state.delete_notices(vec![id.clone()], cx));
                }
            }),
        );
    let details = {
        let mut lines = Vec::new();
        if let Some(raw) = &notice.raw {
            lines.push(raw.clone());
        }
        lines.push(notice.code.clone());
        if let Some(run) = &notice.run_id {
            lines.push(format!("run {run}"));
        }
        lines.join("\n")
    };
    v_flex()
        .id(SharedString::from(rid.clone()))
        .debug_selector(move || rid.clone())
        .group(group.clone())
        .w_full()
        .px(px(12.))
        .py(px(10.))
        .gap(px(4.))
        .rounded(px(10.))
        .border_1()
        .cursor_pointer()
        .map(|this| {
            if ticked {
                this.bg(theme.primary.opacity(0.08))
                    .border_color(theme.primary.opacity(0.4))
            } else {
                this.bg(theme.background)
                    .border_color(theme.border)
                    .hover(move |s| s.border_color(muted.opacity(0.3)))
            }
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            let id = id.clone();
            expand.update(cx, |state, cx| state.toggle_notice_expanded(id, cx));
        })
        .child(
            h_flex()
                .gap(px(6.))
                .items_center()
                .child(
                    div()
                        .size(px(16.))
                        .when(!show_box, |this| {
                            this.opacity(0.)
                                .group_hover(group.clone(), |s| s.opacity(1.))
                        })
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            tick_box(select_id(&notice.id), ticked, false, theme).on_mouse_down(
                                MouseButton::Left,
                                {
                                    let id = notice.id.clone();
                                    move |_, _, cx| {
                                        cx.stop_propagation();
                                        tick.update(cx, |state, cx| {
                                            state.toggle_notice_selected(id.clone(), cx)
                                        });
                                    }
                                },
                            ),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(if read {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::SEMIBOLD
                        })
                        .child(notice.place.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "notification-time-{}",
                            notice.id
                        )))
                        .text_xs()
                        .text_color(muted)
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(when_full.clone())
                                .build(window, cx)
                        })
                        .child(format!("· {}", relative(notice.at_ms))),
                )
                .child(div().flex_1())
                .child(div().relative().child(actions).when(!read, |this| {
                    this.child(
                        div()
                            .absolute()
                            .right(px(4.))
                            .top(px(9.))
                            .size(px(7.))
                            .rounded_full()
                            .bg(theme.danger)
                            .group_hover(group.clone(), |s| s.opacity(0.)),
                    )
                })),
        )
        .child(
            div()
                .text_sm()
                .text_color(if read { muted } else { theme.foreground })
                .when(!open, |this| this.line_clamp(3))
                .child(notice.said.clone()),
        )
        .when(open, |this| {
            let did = details_id(&notice.id);
            this.child(
                div()
                    .id(SharedString::from(did.clone()))
                    .debug_selector(move || did.clone())
                    .mt(px(8.))
                    .p(px(8.))
                    .rounded(px(6.))
                    .bg(rgb(0x777777).opacity(0.06))
                    .font_family("Menlo")
                    .text_xs()
                    .text_color(muted)
                    .whitespace_normal()
                    .child(details),
            )
        })
        .into_any_element()
}

pub const TOAST: &str = "notification-toast";
pub const TOAST_CLOSE: &str = "notification-toast-close";
pub const TOAST_COPY: &str = "notification-toast-copy";
pub const TOAST_OPEN: &str = "notification-toast-open";

/// The toast for the newest notice, bottom-right over everything: which Bot and which part of
/// the app, the sentence (selectable by Copy, since GPUI text is not), ✕, Copy, and the way to
/// the Bot's notifications. It stays while the pointer is on it.
pub fn toast(app: Entity<AppState>, cx: &App) -> Option<AnyElement> {
    let theme = cx.theme().clone();
    let state = app.read(cx);
    let id = state.toast.clone()?;
    let notice = state.notices.iter().find(|n| n.id == id)?.clone();
    let name = state.notice_bot_name(&notice);
    let title = match &name {
        Some(name) => format!("{name} · {}", notice.place),
        None => notice.place.clone(),
    };
    let (hover, close, copy, open) = (app.clone(), app.clone(), app.clone(), app.clone());
    let bot = notice.bot.clone();
    let copy_id = notice.id.clone();
    Some(
        div()
            .absolute()
            .right(px(16.))
            .bottom(px(16.))
            .child(
                v_flex()
                    .id(TOAST)
                    .debug_selector(|| TOAST.into())
                    .occlude()
                    .w(px(360.))
                    .p(px(12.))
                    .gap(px(6.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(theme.danger.opacity(0.5))
                    .bg(theme.background)
                    .text_color(theme.foreground)
                    .shadow_lg()
                    .on_hover(move |on, _, cx| {
                        hover.update(cx, |state, cx| state.set_toast_hovered(*on, cx));
                    })
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(
                                Icon::default()
                                    .path("icons/bell.svg")
                                    .size(px(14.))
                                    .text_color(theme.danger),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .id(TOAST_CLOSE)
                                    .debug_selector(|| TOAST_CLOSE.into())
                                    .cursor_pointer()
                                    .px(px(4.))
                                    .text_color(theme.muted_foreground)
                                    .hover(|s| s.text_color(theme.foreground))
                                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                        cx.stop_propagation();
                                        close.update(cx, |state, cx| state.dismiss_toast(cx));
                                    })
                                    .child("✕"),
                            ),
                    )
                    .child(div().text_sm().child(notice.said.clone()))
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .when(bot.is_some(), |this| {
                                this.child(
                                    text_button(
                                        TOAST_OPEN.into(),
                                        "Open notifications",
                                        theme.primary,
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            let bot = bot.clone();
                                            open.update(cx, |state, cx| {
                                                state.open_notifications_for(bot, cx)
                                            });
                                        },
                                    ),
                                )
                            })
                            .child(
                                text_button(TOAST_COPY.into(), "Copy", theme.muted_foreground)
                                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                        cx.stop_propagation();
                                        copy.update(cx, |state, cx| {
                                            state.copy_notice(&copy_id, cx)
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element(),
    )
}
