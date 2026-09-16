use crate::state::AppState;
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// Renders a popover menu item with hover and click functionality
pub fn render_popover_item<V: 'static>(
    id: &str,
    icon: &str,
    label: &str,
    state_model: Entity<AppState>,
    on_action: impl Fn(&mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let inner_theme = cx.theme();
    let inner_secondary = inner_theme.secondary;
    let inner_secondary_foreground = inner_theme.secondary_foreground;

    div()
        .id(SharedString::from(id.to_string()))
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .hover(move |s| s.bg(inner_secondary))
        .cursor_pointer()
        .on_hover({
            let state_model = state_model.clone();
            move |hovered, _, cx| {
                if *hovered {
                    let state_model = state_model.clone();
                    cx.defer(move |cx| {
                        state_model.update(cx, |state, cx| {
                            if state.more_menu_open {
                                state.more_menu_open = false;
                                cx.notify();
                            }
                        });
                    });
                }
            }
        })
        .on_click({
            let state_model = state_model.clone();
            let label = label.to_string();
            cx.listener(move |_, _, _, cx| {
                state_model.update(cx, |state, cx| state.select_app(label.clone(), cx));
                on_action(cx);
            })
        })
        .child(
            svg()
                .path(SharedString::from(icon.to_string()))
                .size(px(16.0))
                .text_color(inner_secondary_foreground),
        )
        .child(SharedString::from(label.to_string()))
}

/// Renders a flyout menu item with mouse down functionality
pub fn render_flyout_item<V: 'static>(
    id: &str,
    icon: &str,
    label: &str,
    state_model: Entity<AppState>,
    on_action: impl Fn(&mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let inner_theme = cx.theme();
    let inner_secondary = inner_theme.secondary;
    let inner_secondary_foreground = inner_theme.secondary_foreground;

    div()
        .id(SharedString::from(id.to_string()))
        .px_3()
        .py_2()
        .hover(move |s| s.bg(inner_secondary))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, {
            let state_model = state_model.clone();
            let label = label.to_string();
            cx.listener(move |_, _, _, cx| {
                println!("{} mouse down in flyout", label);
                state_model.update(cx, |state, cx| state.select_app(label.clone(), cx));
                on_action(cx);
            })
        })
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    svg()
                        .path(SharedString::from(icon.to_string()))
                        .size(px(16.0))
                        .text_color(inner_secondary_foreground),
                )
                .child(SharedString::from(label.to_string())),
        )
}
