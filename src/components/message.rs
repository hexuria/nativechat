use std::rc::Rc;

use crate::{
    actions::{CopyMessage, ToggleReadAloud},
    components::message_actions::MessageActions,
};
use gpui_kit::component::bubble::{Bubble, BubbleVariant};
use gpui_kit::component::message::{
    Message as KitMessage, MessageAlignment, MessageContent, MessageFooter,
};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::{prelude::FluentBuilder, *};

#[derive(Clone, IntoElement)]
pub struct MessageBubble {
    text: String,
    is_me: bool,
    #[allow(dead_code)]
    bg_color: Hsla,
    #[allow(dead_code)]
    text_color: Hsla,
    timestamp: Option<String>,
    message_id: String,
    debug_mode: bool,
    can_read_aloud: bool,
    is_native_speaking: bool,
    is_native_paused: bool,
    is_native_loading: bool,
    is_ai_speaking: bool,
    is_ai_paused: bool,
    is_ai_loading: bool,
    is_cached: bool,
    highlight_range: Option<std::ops::Range<usize>>,
    highlight_color: Option<Hsla>,
    on_read_aloud: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    use_markdown: bool,
    show_footer: bool,
    copy_text: String,
}

impl MessageBubble {
    pub fn new(text: String) -> Self {
        let message_id = text.len().to_string();
        Self {
            text: text.clone(),
            copy_text: text,
            is_me: false,
            bg_color: gpui_kit::white(),
            text_color: gpui_kit::black(),
            timestamp: None,
            message_id,
            debug_mode: false,
            can_read_aloud: false,
            is_native_speaking: false,
            is_native_paused: false,
            is_native_loading: false,
            is_ai_speaking: false,
            is_ai_paused: false,
            is_ai_loading: false,
            is_cached: false,
            highlight_range: None,
            highlight_color: None,
            on_read_aloud: None,
            use_markdown: true,
            show_footer: true,
        }
    }

    pub fn use_markdown(mut self, use_markdown: bool) -> Self {
        self.use_markdown = use_markdown;
        self
    }

    pub fn show_footer(mut self, show_footer: bool) -> Self {
        self.show_footer = show_footer;
        self
    }

    pub fn copy_text(mut self, copy_text: impl Into<String>) -> Self {
        self.copy_text = copy_text.into();
        self
    }

    pub fn highlight_range(mut self, range: Option<std::ops::Range<usize>>) -> Self {
        self.highlight_range = range;
        self
    }

    pub fn highlight_color(mut self, color: Option<Hsla>) -> Self {
        self.highlight_color = color;
        self
    }

    pub fn on_read_aloud(
        mut self,
        on_read_aloud: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_read_aloud = Some(Rc::new(on_read_aloud));
        self
    }

    pub fn message_id(mut self, id: impl Into<String>) -> Self {
        self.message_id = id.into();
        self
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

    pub fn can_read_aloud(mut self, can: bool) -> Self {
        self.can_read_aloud = can;
        self
    }

    pub fn is_native_speaking(mut self, is: bool) -> Self {
        self.is_native_speaking = is;
        self
    }
    pub fn is_native_paused(mut self, is: bool) -> Self {
        self.is_native_paused = is;
        self
    }
    pub fn is_native_loading(mut self, is: bool) -> Self {
        self.is_native_loading = is;
        self
    }

    pub fn is_ai_speaking(mut self, is: bool) -> Self {
        self.is_ai_speaking = is;
        self
    }
    pub fn is_ai_paused(mut self, is: bool) -> Self {
        self.is_ai_paused = is;
        self
    }
    pub fn is_ai_loading(mut self, is: bool) -> Self {
        self.is_ai_loading = is;
        self
    }

    pub fn is_cached(mut self, is_cached: bool) -> Self {
        self.is_cached = is_cached;
        self
    }
}

impl RenderOnce for MessageBubble {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let alignment = if self.is_me {
            MessageAlignment::End
        } else {
            MessageAlignment::Start
        };
        let variant = if self.is_me {
            BubbleVariant::Secondary
        } else if self.highlight_color.is_some() {
            BubbleVariant::Tinted
        } else {
            BubbleVariant::Ghost
        };

        let body = if self.debug_mode {
            div()
                .text_sm()
                .font_family("monospace")
                .p_2()
                .bg(cx.theme().muted.opacity(0.3))
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .child(self.text.clone())
                .into_any_element()
        } else if self.highlight_range.is_some() {
            highlighted_text(
                &self.text,
                self.highlight_range.as_ref(),
                self.highlight_color,
            )
        } else if self.is_me || !self.use_markdown {
            div()
                .id(ElementId::Name(
                    format!("msg-body-{}", self.message_id).into(),
                ))
                .w_full()
                .min_w_0()
                .text_sm()
                .child(self.text.clone())
                .into_any_element()
        } else {
            TextView::markdown(
                ElementId::Name(self.message_id.clone().into()),
                self.text.clone(),
            )
            .into_any_element()
        };

        let actions = MessageActions::new(self.message_id.clone())
            .message_text(self.copy_text.clone())
            .can_read_aloud(self.can_read_aloud)
            .is_native_speaking(self.is_native_speaking)
            .is_native_paused(self.is_native_paused)
            .is_native_loading(self.is_native_loading)
            .is_ai_speaking(self.is_ai_speaking)
            .is_ai_paused(self.is_ai_paused)
            .is_ai_loading(self.is_ai_loading)
            .is_cached(self.is_cached)
            .when_some(self.on_read_aloud, |this, cb| {
                this.on_read_aloud(move |w, cx| cb(w, cx))
            });

        let copy_text = self.copy_text.clone();
        let message_id = self.message_id.clone();
        let text = self.copy_text.clone();

        h_flex()
            .w_full()
            .id(ElementId::Name(self.message_id.clone().into()))
            .focusable()
            .on_action(move |_: &CopyMessage, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
            })
            .on_key_down(move |event: &KeyDownEvent, _window, cx| {
                if event.keystroke.key == "f8" {
                    cx.dispatch_action(&ToggleReadAloud {
                        text: text.clone(),
                        message_id: message_id.clone(),
                        mode: crate::actions::TtsSource::Native,
                    });
                }
            })
            .child(
                KitMessage::new()
                    .alignment(alignment)
                    .content(
                        MessageContent::new()
                            .bubble(Bubble::new().with_variant(variant).child(body)),
                    )
                    .when(self.show_footer, |this| {
                        this.footer(
                            MessageFooter::new()
                                .when_some(self.timestamp, |this, timestamp| {
                                    this.child(div().text_xs().child(timestamp))
                                })
                                .when(!self.is_me, |this| this.child(actions)),
                        )
                    }),
            )
    }
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn highlighted_text(
    text: &str,
    range: Option<&std::ops::Range<usize>>,
    color: Option<Hsla>,
) -> AnyElement {
    let Some(range) = range else {
        return div().text_sm().child(text.to_string()).into_any_element();
    };
    let start = floor_char_boundary(text, range.start.min(text.len()));
    let end = floor_char_boundary(text, range.end.min(text.len())).max(start);
    let hl = color.unwrap_or_else(|| gpui_kit::yellow().opacity(0.4));
    let body = if start < end {
        StyledText::new(SharedString::from(text.to_string())).with_highlights([(
            start..end,
            HighlightStyle {
                background_color: Some(hl),
                ..Default::default()
            },
        )])
        .into_any_element()
    } else {
        text.to_string().into_any_element()
    };
    div()
        .text_sm()
        .w_full()
        .min_w_0()
        .child(body)
        .into_any_element()
}
