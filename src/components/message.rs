use std::rc::Rc;

use crate::{
    actions::{CopyMessage, ToggleReadAloud},
    components::message_actions::MessageActions,
};
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
    can_read_aloud: bool,
    is_speaking: bool,
    is_paused: bool,
    is_loading: bool,
    is_cached: bool,
    active_tts_source: Option<crate::actions::TtsSource>,
    on_read_aloud: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
}

impl MessageBubble {
    pub fn new(text: String) -> Self {
        Self {
            text: text.clone(),
            is_me: false,
            bg_color: gpui::white(),
            text_color: gpui::black(),
            timestamp: None,
            message_id: text.len().to_string(), // Default ID, should be overridden
            debug_mode: false,
            can_read_aloud: false,
            is_speaking: false,
            is_paused: false,
            is_loading: false,
            is_cached: false,
            active_tts_source: None,
            on_read_aloud: None,
        }
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

    pub fn is_speaking(mut self, is_speaking: bool) -> Self {
        self.is_speaking = is_speaking;
        self
    }

    pub fn is_paused(mut self, is_paused: bool) -> Self {
        self.is_paused = is_paused;
        self
    }

    pub fn is_loading(mut self, is_loading: bool) -> Self {
        self.is_loading = is_loading;
        self
    }

    pub fn is_cached(mut self, is_cached: bool) -> Self {
        self.is_cached = is_cached;
        self
    }

    pub fn active_tts_source(mut self, source: Option<crate::actions::TtsSource>) -> Self {
        self.active_tts_source = source;
        self
    }
}

impl RenderOnce for MessageBubble {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.is_me {
            // User message: gray bubble on the right (ChatGPT style)
            // For now, keep user messages as plain text or also use Markdown if desired.
            // Let's use Markdown for consistency but keep the bubble styling.
            h_flex()
                .w_full()
                .justify_end()
                .on_action({
                    let text = self.text.clone();
                    move |_: &CopyMessage, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    }
                })
                .child(
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
                                            .text_color(
                                                cx.theme().secondary_foreground.opacity(0.7),
                                            )
                                            .child(timestamp),
                                    )
                                }),
                        ),
                )
        } else {
            h_flex()
                .w_full()
                .justify_start()
                .on_action({
                    let text = self.text.clone();
                    move |_: &CopyMessage, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    }
                })
                .child(
                    v_flex()
                        .id(ElementId::Name(self.message_id.clone().into()))
                        .focusable()
                        .on_key_down({
                            let message_id = self.message_id.clone();
                            let text = self.text.clone();
                            move |event, _window, cx| {
                                if event.keystroke.key == "f8" {
                                    cx.dispatch_action(&ToggleReadAloud {
                                        message_id: message_id.clone(),
                                        text: text.clone(),
                                        mode: crate::actions::TtsSource::Native,
                                    });
                                }
                            }
                        })
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
                                    this.child(ui::text::MarkdownView::new(
                                        ElementId::Name(self.message_id.clone().into()),
                                        self.text.clone(),
                                        window,
                                        cx,
                                    ))
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
                        .child(
                            MessageActions::new(self.message_id.clone())
                                .message_text(self.text.clone())
                                .can_read_aloud(self.can_read_aloud)
                                .is_speaking(self.is_speaking)
                                .is_paused(self.is_paused)
                                .is_loading(self.is_loading)
                                .is_cached(self.is_cached)
                                .active_tts_source(self.active_tts_source)
                                .when_some(self.on_read_aloud, |this, cb| {
                                    this.on_read_aloud(move |w, cx| cb(w, cx))
                                }),
                        )
                        .on_key_down({
                            let text = self.text.clone();
                            let message_id = self.message_id.clone();
                            move |event: &KeyDownEvent, _window: &mut Window, cx: &mut App| {
                                if event.keystroke.key == "f8" {
                                    cx.dispatch_action(&ToggleReadAloud {
                                        text: text.clone(),
                                        message_id: message_id.clone(),
                                        mode: crate::actions::TtsSource::Native,
                                    });
                                }
                            }
                        }),
                )
        }
    }
}
