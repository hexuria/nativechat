use gpui::{prelude::FluentBuilder, *};
use gpui_component::{ActiveTheme, h_flex, v_flex};

#[derive(Clone, IntoElement)]
pub struct MessageBubble {
    text: String,
    is_me: bool,
    bg_color: Hsla,
    text_color: Hsla,
    timestamp: Option<String>,
}

impl MessageBubble {
    pub fn new(text: String) -> Self {
        Self {
            text,
            is_me: false,
            bg_color: gpui::white(),
            text_color: gpui::black(),
            timestamp: None,
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
        let _align_class = if self.is_me {
            "justify-end"
        } else {
            "justify-start"
        };

        let _bg_color = if self.is_me {
            cx.theme().primary
        } else {
            cx.theme().secondary
        };

        let _text_color = if self.is_me {
            cx.theme().primary_foreground
        } else {
            cx.theme().secondary_foreground
        };

        let _rounded_class = if self.is_me {
            "rounded-br-none"
        } else {
            "rounded-bl-none"
        };

        h_flex()
            .w_full()
            .map(|this| {
                if self.is_me {
                    this.justify_end()
                } else {
                    this.justify_start()
                }
            })
            .child(
                div()
                    .max_w_3_4()
                    .p_3()
                    .rounded_xl()
                    .map(|this| {
                        if self.is_me {
                            this.rounded_tr_none()
                        } else {
                            this.rounded_tl_none()
                        }
                    })
                    .bg(self.bg_color)
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_sm().text_color(self.text_color).child(self.text))
                            .when_some(self.timestamp, |this, timestamp| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(self.text_color.opacity(0.7))
                                        .child(timestamp),
                                )
                            }),
                    ),
            )
    }
}
