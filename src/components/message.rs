use crate::components::message_actions::MessageActions;
use gpui::{prelude::FluentBuilder, *};
use ui::{ActiveTheme, h_flex, v_flex};

#[derive(Clone, IntoElement)]
pub struct MessageBubble {
    text: String,
    is_me: bool,
    bg_color: Hsla,
    text_color: Hsla,
    timestamp: Option<String>,
    message_id: String,
}

impl MessageBubble {
    pub fn new(text: String) -> Self {
        Self {
            text: text.clone(),
            is_me: false,
            bg_color: gpui::white(),
            text_color: gpui::black(),
            timestamp: None,
            message_id: text.len().to_string(), // Simple ID for now
        }
    }

    pub fn is_me(mut self, is_me: bool) -> Self {
        self.is_me = is_me;
        self
    }

    pub fn bg_color(mut self, bg_color: Hsla) -> Self {
        self.bg_color = bg_color;
        self
    }

    pub fn text_color(mut self, text_color: Hsla) -> Self {
        self.text_color = text_color;
        self
    }

    pub fn timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = Some(timestamp.into());
        self
    }
}

impl RenderOnce for MessageBubble {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.is_me {
            // User message: gray bubble on the right (ChatGPT style)
            h_flex().w_full().justify_end().child(
                div()
                    .max_w(px(360.0))
                    .px_4()
                    .py_2p5()
                    .rounded(px(20.0))
                    .bg(cx.theme().secondary) // Use theme secondary (grayish)
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().secondary_foreground)
                                    .child(self.text),
                            )
                            .when_some(self.timestamp, |this, timestamp| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().secondary_foreground.opacity(0.7))
                                        .child(timestamp),
                                )
                            }),
                    ),
            )
        } else {
            // AI message: plain text with action buttons (No background, no padding)
            h_flex().w_full().justify_start().child(
                v_flex()
                    .max_w_3_4()
                    .gap_2() // Space between message and actions
                    // Message content - Plain text
                    .child(
                        div()
                            .pr_4() // Add some right padding for readability
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().foreground)
                                            .child(self.text),
                                    )
                                    .when_some(self.timestamp, |this, timestamp| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().foreground.opacity(0.6))
                                                .child(timestamp),
                                        )
                                    }),
                            ),
                    )
                    // Action buttons
                    .child(MessageActions::new(self.message_id)),
            )
        }
    }
}
