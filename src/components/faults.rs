//! The ⚠ badge on a card's title row, and the fault window it opens (`crate::faults`).
//!
//! The badge says only that the place has an unread fault; nothing about it is printed in the
//! card. A click opens the window, which is the same size whatever the fault: its text sits in a
//! box of exactly [`RAW_ROWS`] lines that scrolls, so one line and a thousand do not move anything.

use crate::faults::Place;
use crate::state::AppState;
use gpui_kit::component::text::TextView;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The badge's colour: amber, "look here", where the theme's red would read as the whole card
/// having failed.
const WARNING: u32 = 0xd97706;

/// How many lines the fault's text box shows, always: neither fewer for a short fault nor more
/// for a long one.
pub const RAW_ROWS: f32 = 10.;

/// The window's width, fixed for the same reason.
const WINDOW_W: f32 = 620.;

pub const WINDOW_ID: &str = "fault-window";
pub const COPY_ID: &str = "fault-copy";
pub const CLOSE_ID: &str = "fault-close";
pub const READ_ID: &str = "fault-mark-read";
pub const NEWER_ID: &str = "fault-newer";
pub const OLDER_ID: &str = "fault-older";
pub const RAW_ID: &str = "fault-raw";

/// Ring a card or page while a notice's 🎯 has just brought the person to it: an amber border,
/// over whatever border it already has, until the focus fades (`AppState::focus_place`).
pub fn ring<E: Styled>(element: E, focused: bool) -> E {
    if focused {
        element.border_2().border_color(rgb(WARNING))
    } else {
        element
    }
}

/// A card's title row: its title, a spacer, and the ⚠ badge while `place` has an unread fault.
pub fn title_row(
    title: impl Into<SharedString>,
    place: Place,
    app: &Entity<AppState>,
    cx: &App,
) -> Div {
    h_flex()
        .w_full()
        .items_center()
        .child(div().child(title.into()))
        .child(div().flex_1())
        .when_some(badge(place, app, cx), |row, badge| row.child(badge))
}

/// The ⚠ badge for `place`, while it has an unread fault. Hovering names the place; a click
/// opens the fault window.
pub fn badge(place: Place, app: &Entity<AppState>, cx: &App) -> Option<AnyElement> {
    app.read(cx).open_fault(place)?;
    Some(badge_element(place, app.clone()))
}

/// The badge itself, for a view that has already read whether `place` has a fault.
pub fn badge_element(place: Place, app: Entity<AppState>) -> AnyElement {
    let id = place.badge_id();
    let tip: SharedString = format!("{} failed · click for details", place.label()).into();
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id)
        .flex_none()
        .p(px(3.))
        .rounded(px(5.))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(WARNING).opacity(0.15)))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            app.update(cx, |state, cx| state.open_fault_window(place, cx));
        })
        .child(
            Icon::default()
                .path("icons/triangle-alert.svg")
                .size(px(14.))
                .text_color(rgb(WARNING)),
        )
        .into_any_element()
}

/// What "Copy all" puts on the clipboard: every fact, then the whole text.
pub fn copy_block(state: &AppState, notice: &crate::notifications::Notice) -> String {
    notice.copy_text(state.notice_bot_name(notice).as_deref())
}

fn when(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

fn fact(label: &'static str, value: String, muted: Hsla) -> Div {
    h_flex()
        .gap(px(12.))
        .text_xs()
        .child(div().w(px(76.)).flex_none().text_color(muted).child(label))
        .child(div().min_w(px(0.)).truncate().child(value))
}

fn button(
    id: &'static str,
    label: &'static str,
    primary: bool,
    theme: &gpui_kit::component::Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .px(px(10.))
        .py(px(4.))
        .rounded(px(7.))
        .text_xs()
        .cursor_pointer()
        .border_1()
        .border_color(theme.border)
        .when(primary, |this| {
            this.bg(theme.foreground).text_color(theme.background)
        })
        .when(!primary, |this| this.hover(|s| s.bg(theme.muted)))
        .child(label)
}

/// The fault window over the app, while one is open: a dimmed backdrop that closes it on a click,
/// and the window, which a click inside keeps open.
pub fn window(app: &Entity<AppState>, cx: &App) -> Option<AnyElement> {
    let state = app.read(cx);
    let (place, at, count, notice) = state.fault_in_window()?;
    let theme = cx.theme().clone();
    let muted = theme.muted_foreground;
    let fault = notice.fault.clone()?;
    let bot = state
        .notice_bot_name(notice)
        .or_else(|| notice.bot.clone())
        .unwrap_or_else(|| "—".to_string());
    let raw = notice.raw.clone().unwrap_or_default();
    let copied = copy_block(state, notice);
    let id = notice.id.clone();
    let status = match fault.status {
        Some(status) => status.to_string(),
        None => "— (no answer)".to_string(),
    };
    let seen = if fault.count > 1 {
        format!(
            "{} → {} · ×{}",
            when(notice.at_ms),
            when(fault.last_ms),
            fault.count
        )
    } else {
        when(notice.at_ms)
    };
    let (close, back, copy, read, newer, older) = (
        app.clone(),
        app.clone(),
        copied,
        app.clone(),
        app.clone(),
        app.clone(),
    );
    // The text as a code block, so it is monospace and can be selected and copied in part.
    let shown = format!("```text\n{raw}\n```");
    let line = px(18.);
    Some(
        div()
            .id("fault-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x000000).opacity(0.25))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                back.update(cx, |state, cx| state.close_fault_window(cx));
            })
            .child(
                v_flex()
                    .id(WINDOW_ID)
                    .debug_selector(|| WINDOW_ID.into())
                    .w(px(WINDOW_W))
                    .rounded(px(14.))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        v_flex()
                            .gap(px(4.))
                            .p(px(14.))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!("{} failed", place.label())),
                                    )
                                    .child(div().flex_1())
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(format!("{} of {count}", at + 1)),
                                    )
                                    .child(button(NEWER_ID, "‹", false, &theme).on_mouse_down(
                                        MouseButton::Left,
                                        move |_, _, cx| {
                                            newer.update(cx, |s, cx| s.page_fault_window(-1, cx))
                                        },
                                    ))
                                    .child(button(OLDER_ID, "›", false, &theme).on_mouse_down(
                                        MouseButton::Left,
                                        move |_, _, cx| {
                                            older.update(cx, |s, cx| s.page_fault_window(1, cx))
                                        },
                                    )),
                            )
                            .child(fact("Bot", bot, muted))
                            .child(fact(
                                "Request",
                                fault.endpoint.clone().unwrap_or_else(|| "—".to_string()),
                                muted,
                            ))
                            .child(fact("Status", status, muted))
                            .child(fact("Raised at", notice.code.clone(), muted))
                            .child(fact("Seen", seen, muted)),
                    )
                    .child(
                        div()
                            .id(RAW_ID)
                            .debug_selector(|| RAW_ID.into())
                            .h(line * RAW_ROWS + px(20.))
                            .overflow_y_scroll()
                            .px(px(14.))
                            .py(px(10.))
                            .text_xs()
                            .child(TextView::markdown("fault-raw-text", shown).selectable(true)),
                    )
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .p(px(12.))
                            .border_t_1()
                            .border_color(theme.border)
                            .child(button(COPY_ID, "Copy all", false, &theme).on_mouse_down(
                                MouseButton::Left,
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                                },
                            ))
                            .child(div().flex_1())
                            .child(button(READ_ID, "Mark read", false, &theme).on_mouse_down(
                                MouseButton::Left,
                                move |_, _, cx| {
                                    let id = id.clone();
                                    read.update(cx, |s, cx| s.set_notices_read(vec![id], true, cx))
                                },
                            ))
                            .child(
                                button(CLOSE_ID, "Close", true, &theme).on_mouse_down(
                                    MouseButton::Left,
                                    move |_, _, cx| {
                                        close.update(cx, |s, cx| s.close_fault_window(cx))
                                    },
                                ),
                            ),
                    ),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::copy_block;
    use crate::state::AppState;

    /// "Copy all" carries the fault's text once, whole, with its cause chain's lines intact.
    #[test]
    fn copy_all_carries_the_text_once() {
        let mut notice = crate::notifications::Notice::new(
            None,
            "Usage",
            "Could not load this bot's usage.",
            std::panic::Location::caller(),
        );
        let raw = "error sending request\n\nCaused by:\n    2: Connection refused (os error 61)";
        notice.raw = Some(raw.into());
        let copied = copy_block(&AppState::new(), &notice);
        assert_eq!(copied.matches("Connection refused").count(), 1, "{copied}");
        assert!(copied.contains(raw), "{copied}");
    }
}
