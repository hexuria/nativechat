use crate::actions::CloseFind;
use crate::components::fields::field_input;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// Grok's find chrome: search field, `n/m`, previous, next, close.
pub fn find_bar_element(
    query: &Entity<InputState>,
    current: Option<usize>,
    total: usize,
    has_query: bool,
    on_prev: impl Fn(&mut Window, &mut App) + 'static + Clone,
    on_next: impl Fn(&mut Window, &mut App) + 'static + Clone,
    on_close: impl Fn(&mut Window, &mut App) + 'static + Clone,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let dark = theme.is_dark();
    let muted = theme.muted_foreground;
    let fg = theme.foreground;
    let border = theme.border;
    let panel: Hsla = if dark {
        rgb(0x2A2A2A).into()
    } else {
        rgb(0xF4F4F4).into()
    };
    let hover: Hsla = rgb(0x777777).opacity(0.16).into();
    let ordinal = current.map(|i| i + 1).unwrap_or(0);
    let counter = if !has_query {
        String::new()
    } else if total == 0 {
        "0/0".into()
    } else {
        format!("{ordinal}/{total}")
    };
    let has_matches = total > 0;
    let on_prev_btn = on_prev.clone();
    let on_next_btn = on_next.clone();
    let on_close_btn = on_close.clone();

    h_flex()
        .id("chat-find-bar")
        .key_context("FindInChat")
        .h(px(34.))
        .w(px(320.))
        .max_w(px(360.))
        .px(px(8.))
        .gap(px(4.))
        .items_center()
        .rounded(px(12.))
        .border_1()
        .border_color(border.opacity(0.7))
        .bg(panel)
        .shadow_sm()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_action({
            let on_close = on_close.clone();
            move |_: &CloseFind, window: &mut Window, cx: &mut App| {
                on_close(window, cx);
            }
        })
        .child(
            Icon::new(IconName::Search)
                .size(px(14.))
                .text_color(muted),
        )
        .child(
            div().flex_1().min_w(px(0.)).child(
                field_input(query)
                    .id("chat-find-input")
                    .appearance(false)
                    .w_full(),
            ),
        )
        .when(has_query, |this| {
            this.child(
                div()
                    .min_w(px(36.))
                    .px(px(2.))
                    .text_xs()
                    .font_family("monospace")
                    .text_color(muted)
                    .child(counter),
            )
        })
        .child(find_icon_btn(
            "find-prev",
            IconName::ChevronUp,
            "Previous match",
            has_matches,
            muted,
            fg,
            hover,
            on_prev_btn,
        ))
        .child(find_icon_btn(
            "find-next",
            IconName::ChevronDown,
            "Next match",
            has_matches,
            muted,
            fg,
            hover,
            on_next_btn,
        ))
        .child(find_icon_btn(
            "find-close",
            IconName::Close,
            "Close find",
            true,
            muted,
            fg,
            hover,
            on_close_btn,
        ))
}

fn find_icon_btn(
    id: &'static str,
    icon: IconName,
    _label: &'static str,
    enabled: bool,
    muted: Hsla,
    fg: Hsla,
    hover: Hsla,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let color = if enabled { fg } else { muted.opacity(0.4) };
    div()
        .id(id)
        .size(px(22.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(|s| s.bg(hover))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    on_click(window, cx);
                })
        })
        .child(Icon::new(icon).size(px(14.)).text_color(color))
}
