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
    debug_mode: bool,
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
            debug_mode: false,
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

    pub fn debug_mode(mut self, enabled: bool) -> Self {
        self.debug_mode = enabled;
        self
    }
}

impl RenderOnce for MessageBubble {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.is_me {
            // User message: gray bubble on the right (ChatGPT style)
            // For now, keep user messages as plain text or also use Markdown if desired.
            // Let's use Markdown for consistency but keep the bubble styling.
            h_flex().w_full().justify_end().child(
                div()
                    .px_4()
                    .py_2p5()
                    .rounded(px(20.0))
                    .bg(cx.theme().secondary) // Use theme secondary (grayish)
                    .max_w_full()
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().secondary_foreground)
                                    .overflow_x_hidden()
                                    .child(self.text), // User text usually doesn't need complex markdown, but we could swap this too.
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
            h_flex().w_full().justify_start().child(
                v_flex()
                    .flex_1()
                    .gap_2()
                    .max_w_full()
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .overflow_hidden()
                            .when(self.debug_mode, |this| {
                                // Debug mode: plain text with styling for visibility
                                this.child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .p_2()
                                        .bg(cx.theme().muted.opacity(0.3))
                                        .rounded_md()
                                        .border_1()
                                        .border_color(cx.theme().border)
                                        .font_family("monospace")
                                        .child(self.text.clone()),
                                )
                            })
                            .when(!self.debug_mode, |this| {
                                // Normal mode: markdown rendering
                                this.child(
                                    ui::text::TextView::markdown(
                                        ElementId::Name(self.message_id.clone().into()),
                                        self.text.clone(),
                                        window,
                                        cx,
                                    )
                                    .selectable(true),
                                )
                            })
                            .when_some(self.timestamp, |this, timestamp| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(timestamp),
                                )
                            }),
                    )
                    .child(MessageActions::new(self.message_id)),
            )
        }
    }
}
