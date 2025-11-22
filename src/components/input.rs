use crate::audio::AudioInput;
use crate::components::voice_wave::VoiceWave;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, h_flex,
    input::{Input, InputEvent, InputState},
    tooltip::Tooltip,
};
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

actions!(chat, [SubmitMessage]);

pub struct MessageInput {
    input_state: Entity<InputState>,
    on_submit: Option<Box<dyn Fn(String, &mut Context<Self>)>>,
    voice_mode: bool,
    voice_wave: Entity<VoiceWave>,
    audio_input: Option<AudioInput>,
    amplitude: Arc<AtomicU32>,
}

impl MessageInput {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type a message...")
                .multi_line()
                .auto_grow(1, 10)
                .clean_on_escape()
        });

        // Subscribe to input events to handle Enter key
        cx.subscribe_in(&input_state, window, |this, _state, event, window, cx| {
            match event {
                InputEvent::PressEnter { secondary } => {
                    if !secondary {
                        // Enter without Shift - submit the message
                        this.trigger_submit(window, cx);
                    }
                    // Shift+Enter is handled by the editor (newline)
                }
                _ => {}
            }
        })
        .detach();

        let amplitude = Arc::new(AtomicU32::new(0));
        let voice_wave = VoiceWave::new(cx, amplitude.clone());

        Self {
            input_state,
            on_submit: None,
            voice_mode: false,
            voice_wave,
            audio_input: None,
            amplitude,
        }
    }

    pub fn on_submit(mut self, handler: impl Fn(String, &mut Context<Self>) + 'static) -> Self {
        self.on_submit = Some(Box::new(handler));
        self
    }

    fn trigger_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        println!("Triggering submit...");
        let text = self.input_state.read(cx).value();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            println!("Submitting message: {}", trimmed);
            if let Some(handler) = &self.on_submit {
                (handler)(trimmed.to_string(), cx);
            }
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
            // Focus is handled by the input state usually, or we might need to re-focus
        } else {
            println!("Message is empty, ignoring.");
        }
    }

    fn toggle_voice_mode(&mut self, cx: &mut Context<Self>) {
        if self.voice_mode {
            self.voice_mode = false;
            self.audio_input = None;
        } else {
            match AudioInput::new(self.amplitude.clone()) {
                Ok(input) => {
                    self.voice_mode = true;
                    self.audio_input = Some(input);
                }
                Err(e) => {
                    eprintln!("Failed to start audio input: {}", e);
                }
            }
        }
        cx.notify();
    }

    fn confirm_voice_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.voice_mode = false;
        self.audio_input = None;
        cx.notify();

        // Mock transcription
        let mock_text = "This is a simulated transcription of your voice.";
        self.input_state.update(cx, |state, cx| {
            state.set_value(mock_text.to_string(), window, cx);
        });
    }
}

impl Render for MessageInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;
        let border = theme.border;

        // ChatGPT-style: centered container with max-width
        h_flex().w_full().justify_center().p_4().child(
            // Input container - rounded pill shape with shadow
            h_flex()
                .max_w(px(800.0)) // Max width like ChatGPT
                .w_full()
                .items_center()
                .gap_2()
                .px_4()
                .py_3()
                .bg(theme.background) // Match chat background (white in light mode)
                .border_1()
                .border_color(border)
                .rounded(px(26.0)) // Rounded pill shape
                .shadow_sm()
                // Plus icon on the left (Hidden in Voice Mode)
                .when(!self.voice_mode, |this| {
                    this.child(
                        div()
                            .id("attach-btn")
                            .w(px(36.0))
                            .h(px(36.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(gpui::transparent_black())
                            .text_color(secondary_foreground) // Set color on parent
                            .hover(move |style| style.bg(secondary))
                            .cursor_pointer()
                            .tooltip(|w, cx| Tooltip::new("Attach files").build(w, cx))
                            .child(Icon::new(IconName::Plus).text_color(secondary_foreground)),
                    )
                })
                // Input field (grows to fill space)
                .child(div().flex_grow().child(if self.voice_mode {
                    self.voice_wave.clone().into_any_element()
                } else {
                    Input::new(&self.input_state)
                        .appearance(false)
                        .into_any_element()
                }))
                // Right side icons
                .child(h_flex().gap_1().items_center().map(|this| {
                    if self.voice_mode {
                        // Voice Mode: Cancel (X) and Confirm (Check)
                        this.child(
                            div()
                                .id("cancel-voice-btn")
                                .w(px(36.0))
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(gpui::transparent_black())
                                .text_color(secondary_foreground)
                                .hover(move |style| style.bg(secondary))
                                .cursor_pointer()
                                .tooltip(|w, cx| Tooltip::new("Cancel").build(w, cx))
                                .child(Icon::new(IconName::Close).text_color(secondary_foreground))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_voice_mode(cx);
                                })),
                        )
                        .child(
                            div()
                                .id("confirm-voice-btn")
                                .w(px(36.0))
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(theme.foreground) // Black/White
                                .text_color(theme.background) // White/Black
                                .hover(move |style| style.bg(theme.foreground.opacity(0.8)))
                                .cursor_pointer()
                                .tooltip(|w, cx| Tooltip::new("Done").build(w, cx))
                                .child(Icon::new(IconName::Check).text_color(theme.background))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.confirm_voice_input(window, cx);
                                })),
                        )
                    } else {
                        // Text Mode: Mic and Send/Headphone
                        this.child(
                            div()
                                .id("voice-btn")
                                .w(px(36.0))
                                .h(px(36.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(gpui::transparent_black()) // Transparent/White by default
                                .text_color(secondary_foreground)
                                .hover(move |style| style.bg(secondary)) // Gray on hover
                                .cursor_pointer()
                                .tooltip(|w, cx| Tooltip::new("Voice Mode").build(w, cx))
                                .child(
                                    svg()
                                        .path("icons/mic.svg")
                                        .size(px(18.0))
                                        .text_color(secondary_foreground),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_voice_mode(cx);
                                })),
                        )
                        .child(
                            if self.input_state.read(cx).text().len() == 0 {
                                // Empty state: Headphone icon
                                div()
                                    .id("headphone-btn")
                                    .w(px(36.0))
                                    .h(px(36.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(gpui::transparent_black()) // Transparent/White by default
                                    .text_color(secondary_foreground)
                                    .hover(move |style| style.bg(secondary)) // Gray on hover
                                    .cursor_pointer()
                                    .tooltip(|w, cx| Tooltip::new("Read Aloud").build(w, cx))
                                    .child(
                                        svg()
                                            .path("icons/sparkles.svg")
                                            .size(px(18.0))
                                            .text_color(secondary_foreground),
                                    ) // Fallback to Sparkles as requested
                            } else {
                                // Typing state: Send button (Black bg, White arrow)
                                div()
                                    .id("send-btn")
                                    .w(px(36.0))
                                    .h(px(36.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .bg(theme.foreground) // Theme-aware foreground (Black in light, White in dark)
                                    .text_color(theme.background) // Theme-aware background (White in light, Black in dark)
                                    .hover(move |style| style.bg(theme.foreground.opacity(0.8)))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.trigger_submit(window, cx);
                                    }))
                                    .tooltip(|w, cx| Tooltip::new("Send message").build(w, cx))
                                    .child(
                                        Icon::new(IconName::ArrowUp).text_color(theme.background),
                                    )
                            },
                        )
                    }
                })),
        )
    }
}
