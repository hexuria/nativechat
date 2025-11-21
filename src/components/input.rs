use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName, h_flex,
    input::{Input, InputEvent, InputState},
    tooltip::Tooltip,
};

actions!(chat, [SubmitMessage]);

pub struct MessageInput {
    input_state: Entity<InputState>,
    on_submit: Option<Box<dyn Fn(String, &mut Context<Self>)>>,
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

        Self {
            input_state,
            on_submit: None,
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
                // Plus icon on the left
                .child(
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
                // Input field (grows to fill space)
                .child(
                    div()
                        .flex_grow()
                        .child(Input::new(&self.input_state).appearance(false)),
                )
                // Right side icons
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        // Microphone icon (Voice Mode)
                        .child(
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
                                    Icon::new(IconName::Settings).text_color(secondary_foreground),
                                ), // Fallback to Settings
                        )
                        // Send / Headphone button
                        .child(if self.input_state.read(cx).text().len() == 0 {
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
                                .child(Icon::new(IconName::Menu).text_color(secondary_foreground)) // Fallback to Menu
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
                                .child(Icon::new(IconName::ArrowUp).text_color(theme.background))
                        }),
                ),
        )
    }
}
