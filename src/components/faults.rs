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
pub const REPORT_ID: &str = "fault-report";
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
    let (report, report_id) = (app.clone(), notice.id.clone());
    // The text as a code block, so it is monospace and can be selected and copied in part.
    let shown = format!("```text\n{raw}\n```");
    let line = px(18.);
    Some(
        div()
            .id("fault-backdrop")
            .absolute()
            .inset_0()
            // Nothing under the dimmed backdrop takes the pointer or the scroll wheel: the chat
            // must not scroll behind a window that is reading it.
            .occlude()
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
                            // 🐞: the report preview, over this window, on this fault.
                            .child(button(REPORT_ID, "🐞 Report", false, &theme).on_mouse_down(
                                MouseButton::Left,
                                move |_, _, cx| {
                                    let id = report_id.clone();
                                    report.update(cx, |s, cx| s.open_report(&id, cx))
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

/// The report preview's ids, for gpui-agent.
pub const SHEET_ID: &str = "report-sheet";
pub const SHEET_BODY_ID: &str = "report-body";
pub const SHEET_TEXT_ID: &str = "report-include-text";
pub const SHEET_SEND_ID: &str = "report-open-github";
pub const SHEET_CANCEL_ID: &str = "report-cancel";
pub const SHEET_VERDICT_ID: &str = "report-verdict";
pub const SHEET_ANYWAY_ID: &str = "report-anyway";

/// The first gate's words for a fault it decides.
pub fn words_of(gate: crate::report::triage::Gate1) -> &'static str {
    match gate {
        crate::report::triage::Gate1::Noise(words)
        | crate::report::triage::Gate1::YourSide(words) => words,
        crate::report::triage::Gate1::Ask => "",
    }
}

/// How many lines the preview's body box shows, always.
const SHEET_ROWS: f32 = 12.;

/// The report preview over everything, while one is open: the issue's title and body exactly
/// as they will reach GitHub, the "include the server's own text" choice, and the button that
/// opens GitHub's page with them filled in. The person submits there; the app sends nothing.
pub fn report_sheet(app: &Entity<AppState>, cx: &App) -> Option<AnyElement> {
    let state = app.read(cx);
    let draft = state.report_draft.clone()?;
    let report = state.report_preview()?;
    let theme = cx.theme().clone();
    let muted = theme.muted_foreground;
    let title = crate::report::github::title(&report);
    let link = crate::report::github::issue_link(&report);
    // The body as GitHub will show it: rendered Markdown, the failure's text in its code block.
    // The hidden fingerprint marker is GitHub's to hide; here it would show as text.
    let shown = crate::report::github::body(&report)
        .lines()
        .filter(|line| !line.starts_with("<!-- fp:"))
        .collect::<Vec<_>>()
        .join("\n");
    let (back, toggle, send, cancel) = (app.clone(), app.clone(), app.clone(), app.clone());
    let tick = if draft.with_text { "☑" } else { "☐" };
    let line = px(18.);
    // The first gate's word, unless the person insisted: a fault it decides is said in plain
    // words where the body would be, with no checkbox and no way to GitHub but "Report anyway".
    let decided = state
        .report_gate()
        .filter(|gate| !gate.worth_reporting() && !draft.insisted);
    let body_area = match decided {
        Some(gate) => {
            let mark = match gate {
                crate::report::triage::Gate1::YourSide(_) => "💡",
                _ => "🚫",
            };
            v_flex()
                .id(SHEET_VERDICT_ID)
                .debug_selector(|| SHEET_VERDICT_ID.into())
                .h(line * SHEET_ROWS + px(20.))
                .justify_center()
                .items_center()
                .gap(px(10.))
                .px(px(28.))
                .child(div().text_2xl().child(mark))
                .child(div().text_sm().text_center().child(words_of(gate)))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .text_center()
                        .child("Nothing here is the project's to fix, so it is not reported."),
                )
                .into_any_element()
        }
        None => div()
            .id(SHEET_BODY_ID)
            .debug_selector(|| SHEET_BODY_ID.into())
            .h(line * SHEET_ROWS + px(20.))
            .overflow_y_scroll()
            .px(px(14.))
            .py(px(10.))
            .text_xs()
            .child(TextView::markdown("report-body-text", shown).selectable(true))
            .into_any_element(),
    };
    let insist = app.clone();
    let footer = if decided.is_some() {
        h_flex()
            .gap(px(8.))
            .p(px(12.))
            .border_t_1()
            .border_color(theme.border)
            .child(div().flex_1())
            .child(
                button(SHEET_ANYWAY_ID, "Report anyway", false, &theme)
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        insist.update(cx, |s, cx| s.insist_report(cx))
                    }),
            )
            .child(
                button(SHEET_CANCEL_ID, "Close", true, &theme)
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cancel.update(cx, |s, cx| s.close_report(cx))
                    }),
            )
    } else if draft.opened {
        let note = if link.clipboard.is_some() {
            "Opened on GitHub. The full report is on your clipboard: paste it into the body, then submit."
        } else {
            "Opened on GitHub. Submit it there."
        };
        h_flex()
            .gap(px(8.))
            .p(px(12.))
            .border_t_1()
            .border_color(theme.border)
            .child(div().flex_1().text_xs().child(note))
            .child(
                button(SHEET_CANCEL_ID, "Close", true, &theme)
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cancel.update(cx, |s, cx| s.close_report(cx))
                    }),
            )
    } else {
        h_flex()
            .gap(px(8.))
            .p(px(12.))
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(muted)
                    .child("You submit it on GitHub, signed in as you. It will be public."),
            )
            .child(
                button(SHEET_CANCEL_ID, "Cancel", false, &theme)
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cancel.update(cx, |s, cx| s.close_report(cx))
                    }),
            )
            .child(
                button(SHEET_SEND_ID, "Open on GitHub", true, &theme)
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        send.update(cx, |s, cx| s.send_report(cx))
                    }),
            )
    };
    Some(
        div()
            .id("report-backdrop")
            .absolute()
            .inset_0()
            // Nothing under the dimmed backdrop takes the pointer or the scroll wheel: the chat
            // must not scroll behind a window that is reading it.
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x000000).opacity(0.25))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                back.update(cx, |state, cx| state.close_report(cx));
            })
            .child(
                v_flex()
                    .id(SHEET_ID)
                    .debug_selector(|| SHEET_ID.into())
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
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("Report to {}", crate::report::github::REPO)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(if decided.is_some() {
                                        "Checked on this Mac first. Nothing has left it."
                                    } else {
                                        "This is everything that leaves this Mac. Names, hosts, ids and secrets are already taken out."
                                    }),
                            )
                            .child(div().pt(px(6.)).text_sm().truncate().child(title)),
                    )
                    .child(body_area)
                    .when(decided.is_none(), |this| this.child(
                        h_flex()
                            .id(SHEET_TEXT_ID)
                            .debug_selector(|| SHEET_TEXT_ID.into())
                            .gap(px(8.))
                            .px(px(14.))
                            .py(px(8.))
                            .border_t_1()
                            .border_color(theme.border)
                            .cursor_pointer()
                            .text_sm()
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                toggle.update(cx, |s, cx| s.toggle_report_text(cx))
                            })
                            .child(tick)
                            .child("Include the server's own text")
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child("(redacted, off by default)"),
                            ),
                    ))
                    .child(footer),
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
