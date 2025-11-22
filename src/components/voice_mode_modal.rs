use crate::components::voice_wave::VoiceWave;
use crate::state::AppState;
use gpui::InteractiveElement;
use gpui::prelude::*;
use gpui::*;
use gpui_component::{ActiveTheme, h_flex, v_flex};

pub fn render_voice_mode_modal<V: 'static>(
    state: Entity<AppState>,
    voice_wave: Entity<VoiceWave>,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let theme = cx.theme();
    let app_state = state.read(cx);
    let is_muted = app_state.is_voice_muted;
    let state_mute = state.clone();
    let state_close = state.clone();

    // Full screen overlay
    div()
        .absolute()
        .inset_0()
        // .z_index(100) // Ensure it's on top
        .bg(theme.background)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        // Top Right Button (History)
        .child(
            div().absolute().top_4().right_4().child(
                div()
                    .id("history-btn")
                    .w(px(40.0))
                    .h(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(gpui::transparent_black())
                    .hover(move |style| style.bg(theme.secondary))
                    .cursor_pointer()
                    .on_click(|_, _, _| {
                        // TODO: Implement history functionality
                        println!("History button clicked!");
                    })
                    .child(
                        svg()
                            .path("icons/panel.svg")
                            .size(px(20.0))
                            .text_color(theme.secondary_foreground),
                    ),
            ),
        )
        // Center Content: Voice Wave
        .child(
            v_flex()
                .flex_grow()
                .items_center()
                .justify_center()
                .w_full()
                .child(
                    div()
                        .w(px(600.0)) // Wider container for the wave
                        .h(px(200.0))
                        .child(voice_wave),
                ),
        )
        // Bottom Controls
        .child(
            h_flex()
                .gap_6()
                .pb_12()
                .items_center()
                .justify_center()
                // Mute Button (Sparkles/Mic Mute)
                .child(
                    div()
                        .id("mute-btn")
                        .w(px(64.0))
                        .h(px(64.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(if is_muted {
                            gpui::red().opacity(0.1)
                        } else {
                            theme.secondary.opacity(0.5)
                        })
                        .hover(move |style| {
                            style.bg(if is_muted {
                                gpui::red().opacity(0.2)
                            } else {
                                theme.secondary.opacity(0.7)
                            })
                        })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            state_mute.update(cx, |state, cx| {
                                state.toggle_voice_mute(cx);
                            });
                        })
                        .child(
                            svg()
                                .path(if is_muted {
                                    "icons/mic_mute.svg"
                                } else {
                                    "icons/mic.svg"
                                })
                                .size(px(24.0))
                                .text_color(if is_muted {
                                    gpui::red()
                                } else {
                                    theme.foreground
                                }),
                        ),
                )
                // Close Button (X)
                .child(
                    div()
                        .id("close-btn")
                        .w(px(64.0))
                        .h(px(64.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(theme.secondary.opacity(0.5))
                        .hover(move |style| style.bg(theme.secondary.opacity(0.7)))
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            state_close.update(cx, |state, cx| {
                                state.set_voice_mode(false, cx);
                            });
                        })
                        .child(
                            svg()
                                .path("icons/close.svg")
                                .size(px(24.0))
                                .text_color(theme.foreground),
                        ),
                ),
        )
}
