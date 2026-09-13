use crate::components::circular_voice_viz::CircularVoiceViz;
use crate::state::{AppState, VoiceStatus};
use gpui_kit::InteractiveElement;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};

pub fn render_voice_mode_modal<V: 'static>(
    state: Entity<AppState>,
    circular_viz: Entity<CircularVoiceViz>,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let theme = cx.theme();
    let state_clone = state.clone();

    let app_state = state.read(cx);
    let voice_status = app_state.voice_status.clone();

    let (status_color, status_text) = match voice_status {
        VoiceStatus::Ready => (gpui_kit::hsla(0.0, 0.0, 0.5, 1.0), "READY"),
        VoiceStatus::Connecting => (gpui_kit::hsla(0.0, 0.0, 0.5, 1.0), "CONNECTING..."),
        VoiceStatus::Connected => (gpui_kit::hsla(0.3, 0.8, 0.5, 1.0), "LIVE"), // Green
        VoiceStatus::Disconnected => (gpui_kit::hsla(0.0, 0.8, 0.5, 1.0), "DISCONNECTED"), // Red
        VoiceStatus::Error(_) => (gpui_kit::hsla(0.0, 0.8, 0.5, 1.0), "ERROR"), // Red
    };

    // Full screen overlay
    div()
        .absolute()
        .inset_0()
        // Prevent clicks from passing through to elements behind the modal
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Middle, |_, _, cx| {
            cx.stop_propagation();
        })
        // 1. Visualizer (Background + Grid + Gauge) - Full Screen
        .child(div().absolute().inset_0().child(circular_viz))
        // 2. UI Overlay
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .justify_between()
                .p_8()
                // HUD Header (Top Left)
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_start()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    h_flex().gap_2().items_center().child(
                                        div()
                                            .text_3xl()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.foreground)
                                            .child("NATIVECHAT"),
                                    ),
                                )
                                .child(
                                    div()
                                        .text_base()
                                        .text_color(theme.muted_foreground)
                                        .child("BETA"),
                                ),
                        )
                        // Status Indicator (Top Right)
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(div().w_2().h_2().rounded_full().bg(status_color))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(status_color)
                                        .child(status_text),
                                ),
                        ),
                )
                // Bottom Controls
                .child(
                    div().flex().justify_center().child(
                        div()
                            .id("terminate-btn")
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .bg(gpui_kit::hsla(0.0, 0.0, 0.0, 1.0)) // Always black
                            .text_color(gpui_kit::hsla(0.0, 0.0, 1.0, 1.0)) // Always white
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .on_click(cx.listener(move |_, _, _, cx| {
                                state_clone.update(cx, |state, cx| {
                                    state.stop_voice_mode(cx);
                                });
                            }))
                            .child(svg().path("icons/power.svg").size(px(16.0)).text_color(
                                if theme.mode.is_dark() {
                                    gpui_kit::hsla(0.0, 0.8, 0.6, 1.0) // Red on dark
                                } else {
                                    gpui_kit::hsla(0.0, 0.0, 1.0, 1.0) // White on light
                                },
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .child("TERMINATE"),
                            ),
                    ),
                ),
        )
}
