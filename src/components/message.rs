use std::rc::Rc;

use crate::actions::{CopyMessage, ToggleReadAloud};
use crate::chrome::is_narrow_viewport;
use crate::components::message_actions::{MessageToolbar, TOOLBAR_W};
use crate::state::AppState;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::{prelude::FluentBuilder, *};

pub const TIMESTAMP_W: f32 = 82.0;
pub const TS_PEEK_MAX: f32 = 82.0;

#[derive(Clone, IntoElement)]
pub struct MessageBubble {
    text: String,
    is_me: bool,
    timestamp: Option<String>,
    message_id: String,
    source_id: String,
    debug_mode: bool,
    highlight_range: Option<std::ops::Range<usize>>,
    highlight_color: Option<Hsla>,
    on_read_aloud: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_reply: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    use_markdown: bool,
    show_footer: bool,
    copy_text: String,
    reply_preview: Option<String>,
    reaction: Option<String>,
    picker_open: bool,
    ts_peek: f32,
    timestamps_ok: bool,
    app: Option<Entity<AppState>>,
}

impl MessageBubble {
    pub fn new(text: String) -> Self {
        let message_id = text.len().to_string();
        Self {
            text: text.clone(),
            copy_text: text,
            is_me: false,
            timestamp: None,
            message_id,
            source_id: String::new(),
            debug_mode: false,
            highlight_range: None,
            highlight_color: None,
            on_read_aloud: None,
            on_reply: None,
            use_markdown: true,
            show_footer: true,
            reply_preview: None,
            reaction: None,
            picker_open: false,
            ts_peek: 0.0,
            timestamps_ok: true,
            app: None,
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

    pub fn source_id(mut self, id: impl Into<String>) -> Self {
        self.source_id = id.into();
        self
    }

    pub fn reply_preview(mut self, preview: Option<String>) -> Self {
        self.reply_preview = preview;
        self
    }

    pub fn reaction(mut self, reaction: Option<String>) -> Self {
        self.reaction = reaction;
        self
    }

    pub fn picker_open(mut self, open: bool) -> Self {
        self.picker_open = open;
        self
    }

    pub fn ts_peek(mut self, peek: f32) -> Self {
        self.ts_peek = peek;
        self
    }

    pub fn timestamps_ok(mut self, ok: bool) -> Self {
        self.timestamps_ok = ok;
        self
    }

    pub fn app_state(mut self, app: Entity<AppState>) -> Self {
        self.app = Some(app);
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

    pub fn on_reply(mut self, on_reply: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_reply = Some(Rc::new(on_reply));
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

    pub fn bg_color(self, _bg_color: Hsla) -> Self {
        self
    }

    pub fn text_color(self, _text_color: Hsla) -> Self {
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

    pub fn can_read_aloud(self, _can: bool) -> Self {
        self
    }

    pub fn is_native_speaking(self, _is: bool) -> Self {
        self
    }
    pub fn is_native_paused(self, _is: bool) -> Self {
        self
    }
    pub fn is_native_loading(self, _is: bool) -> Self {
        self
    }
    pub fn is_ai_speaking(self, _is: bool) -> Self {
        self
    }
    pub fn is_ai_paused(self, _is: bool) -> Self {
        self
    }
    pub fn is_ai_loading(self, _is: bool) -> Self {
        self
    }
    pub fn is_cached(self, _is_cached: bool) -> Self {
        self
    }
}

fn bubble_colors(is_me: bool, cx: &App) -> (Hsla, Hsla) {
    let theme = cx.theme();
    let dark = theme.is_dark();
    if is_me {
        if dark {
            (rgb(0x3E3E3E).into(), rgb(0xF2F2F2).into())
        } else {
            (rgb(0x111111).into(), rgb(0xFFFFFF).into())
        }
    } else if dark {
        (rgb(0x2A2A2A).into(), rgb(0xEDEDED).into())
    } else {
        (rgb(0xEBEBEB).into(), rgb(0x1B1B1B).into())
    }
}

impl RenderOnce for MessageBubble {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let compact = is_narrow_viewport(f32::from(window.viewport_size().width));
        let row_key = format!("{}:{}", self.source_id, self.message_id);
        let hover_state = window.use_keyed_state(
            ElementId::Name(format!("msg-hover-{row_key}").into()),
            cx,
            |_, _| false,
        );
        let menu_state = window.use_keyed_state(
            ElementId::Name(format!("msg-menu-{row_key}").into()),
            cx,
            |_, _| false,
        );
        let peek = if self.timestamps_ok {
            self.ts_peek.clamp(0.0, TS_PEEK_MAX)
        } else {
            0.0
        };
        let peeking = peek > 0.5;
        let progress = if TS_PEEK_MAX > 0.0 {
            peek / TS_PEEK_MAX
        } else {
            0.0
        };
        let hovered = *hover_state.read(cx) || self.picker_open || *menu_state.read(cx);
        let show_toolbar = self.show_footer && hovered && !peeking;
        let show_ai_time = self.timestamps_ok
            && !self.is_me
            && self.show_footer
            && hovered
            && !compact
            && !peeking;
        let (bg, fg) = bubble_colors(self.is_me, cx);
        let muted = cx.theme().muted_foreground;
        let max_bubble = relative(0.92);

        let body = if self.debug_mode {
            div()
                .text_sm()
                .font_family("monospace")
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
                .id(ElementId::Name(format!("msg-body-{row_key}").into()))
                .text_sm()
                .child(self.text.clone())
                .into_any_element()
        } else {
            TextView::markdown(
                ElementId::Name(format!("md-{row_key}").into()),
                self.text.clone(),
            )
            .into_any_element()
        };

        let quote = self.reply_preview.clone().map(|preview| {
            div()
                .mb(px(6.))
                .pl(px(8.))
                .border_l_2()
                .border_color(fg.opacity(0.35))
                .text_xs()
                .text_color(fg.opacity(0.7))
                .child(truncate_preview(&preview, 88))
                .into_any_element()
        });

        let bubble = div()
            .id(ElementId::Name(format!("bubble-{row_key}").into()))
            .max_w(max_bubble)
            .px(px(14.))
            .py(px(8.))
            .rounded(px(18.))
            .bg(bg)
            .text_color(fg)
            .child(
                v_flex()
                    .when_some(quote, |this, quote| this.child(quote))
                    .child(body),
            );

        let reaction_bg = cx.theme().background;
        let reaction_border = cx.theme().border;
        let is_me = self.is_me;
        // Sit on the bubble's bottom edge: half the 22px chip is on the fill.
        let bubble_stack = div()
            .relative()
            .flex_shrink_0()
            .min_w(px(48.))
            .when(self.reaction.is_some(), |this| this.mb(px(12.)))
            .child(bubble)
            .when_some(self.reaction.clone(), |this, emoji| {
                this.child(
                    div()
                        .absolute()
                        .bottom(px(-11.))
                        .when(is_me, |this| this.right(px(8.)))
                        .when(!is_me, |this| this.left(px(8.)))
                        .h(px(22.))
                        .px(px(7.))
                        .rounded_full()
                        .bg(reaction_bg)
                        .border_1()
                        .border_color(reaction_border)
                        .shadow_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().text_sm().child(emoji)),
                )
            });

        let source_id = if self.source_id.is_empty() {
            self.message_id.clone()
        } else {
            self.source_id.clone()
        };
        let toolbar = self.app.clone().and_then(|app| {
            if !self.show_footer {
                return None;
            }
            let preview = truncate_preview(&self.copy_text, 72);
            let menu_state = menu_state.clone();
            Some(
                MessageToolbar::new(app, row_key.clone(), source_id)
                    .message_text(self.copy_text.clone())
                    .preview(preview)
                    .is_me(self.is_me)
                    .on_menu_open(move |open, cx| {
                        menu_state.update(cx, |state, cx| {
                            if *state != open {
                                *state = open;
                                cx.notify();
                            }
                        });
                    })
                    .when_some(self.on_read_aloud.clone(), |this, cb| {
                        this.on_read_aloud(move |w, cx| cb(w, cx))
                    })
                    .when_some(self.on_reply.clone(), |this, cb| {
                        this.on_reply(move |w, cx| cb(w, cx))
                    }),
            )
        });

        // Fixed slot: always occupies TOOLBAR_W so the bubble never jumps or
        // reaches the trailing edge. Icons fade in on row hover.
        let toolbar_slot = div()
            .w(px(TOOLBAR_W))
            .h(px(28.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .when(self.is_me, |this| this.justify_end())
            .opacity(if show_toolbar { 1. } else { 0. })
            .when_some(toolbar, |this, toolbar| this.child(toolbar));

        let time_label = self.timestamp.clone().unwrap_or_default();
        let peek_time = peeking || show_ai_time;

        let main = if self.is_me {
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().min_w(px(0.)))
                .child(toolbar_slot)
                .child(bubble_stack)
        } else {
            h_flex()
                .w_full()
                .items_center()
                .gap(px(8.))
                .child(bubble_stack)
                .child(toolbar_slot)
                .child(div().flex_1().min_w(px(0.)))
        };

        let copy_text = self.copy_text.clone();
        let message_id = self.message_id.clone();
        let text = self.copy_text.clone();
        let hover_state_row = hover_state.clone();

        // Grok: --sand-ts-peek shifts every row together; timestamps slide in from
        // the right with --sand-ts-progress. Hover time is AI-only when not peeking.
        let body_row = div()
            .relative()
            .w_full()
            .child(
                div()
                    .w_full()
                    .ml(px(-peek))
                    .child(main),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(px(TIMESTAMP_W))
                    .flex()
                    .items_center()
                    .justify_end()
                    .pl(px(10.))
                    .opacity(if peeking {
                        progress
                    } else if peek_time {
                        1.
                    } else {
                        0.
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .whitespace_nowrap()
                            .child(time_label),
                    ),
            );

        div()
            .id(ElementId::Name(format!("msg-row-{row_key}").into()))
            .w_full()
            .on_hover(move |hovered, _, cx| {
                hover_state_row.update(cx, |state, cx| {
                    if *state != *hovered {
                        *state = *hovered;
                        cx.notify();
                    }
                });
            })
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
            .child(body_row)
    }
}

fn truncate_preview(text: &str, max: usize) -> String {
    let trimmed = text.trim().replace('\n', " ");
    if trimmed.chars().count() <= max {
        trimmed
    } else {
        let cut: String = trimmed.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
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
        StyledText::new(SharedString::from(text.to_string()))
            .with_highlights([(
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
    div().text_sm().child(body).into_any_element()
}
