use std::rc::Rc;

use crate::actions::{CopyMessage, ToggleReadAloud};
use crate::chrome::{BUBBLE_RADIUS, CHAT_CONTENT_MAX, chat_column_width};
use crate::components::message_actions::{CONTROL_PX, MessageToolbar, TOOLBAR_W};
use crate::state::{AppState, RightPane};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::{prelude::FluentBuilder, *};

pub const TIMESTAMP_W: f32 = 82.0;
pub const TS_PEEK_MAX: f32 = 82.0;
/// The transcript's padding either side of a row: the 12px the message scroller gives every
/// row (px_3) and the 16px (px_4) around it in chat.rs.
const ROW_PAD: f32 = 28.0;
/// Between the bubble and the toolbar's slot, when the slot is in the row.
const TOOLBAR_GAP: f32 = 8.0;
/// The narrowest bubble worth keeping the toolbar's slot beside. Below it the toolbar floats
/// above the bubble's corner instead, and the bubble takes the whole row.
const MIN_COMFORTABLE_BUBBLE: f32 = 260.0;
/// The floating toolbar's padding around its controls; with its 1px border the pill is
/// CONTROL_PX + 2 * PILL_PAD + 2 tall.
const PILL_PAD: f32 = 2.0;
/// How far the pill reaches down into the bubble's top padding (no text in the bubble's
/// first 8px), so the pointer never crosses a gap between the bubble and the pill.
const PILL_OVERLAP: f32 = 4.0;

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
    /// Being read aloud right now / paused mid-read: the toolbar's menu says so.
    native_speaking: bool,
    native_paused: bool,
    find_marks: Vec<(std::ops::Range<usize>, bool)>,
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
            native_speaking: false,
            native_paused: false,
            find_marks: Vec::new(),
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

    pub fn find_marks(mut self, marks: Vec<(std::ops::Range<usize>, bool)>) -> Self {
        self.find_marks = marks;
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

    pub fn is_native_speaking(mut self, is: bool) -> Self {
        self.native_speaking = is;
        self
    }
    pub fn is_native_paused(mut self, is: bool) -> Self {
        self.native_paused = is;
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
        // The floating toolbar hangs above the row's box, so its own hover keeps it shown
        // while the pointer is on it.
        let pill_hover_state = window.use_keyed_state(
            ElementId::Name(format!("msg-pill-hover-{row_key}").into()),
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
        let hovered = *hover_state.read(cx)
            || self.picker_open
            || *menu_state.read(cx)
            || *pill_hover_state.read(cx);
        let show_toolbar = self.show_footer && hovered && !peeking;
        let (bg, fg) = bubble_colors(self.is_me, cx);
        let muted = cx.theme().muted_foreground;
        // Grok: max-width: min(88%, 640px, calc(100% - 82px)) with width: fit-content.
        // Percent of a shrink-wrapped parent cycles in Taffy and wraps too early.
        let win = f32::from(window.viewport_size().width);
        let chat_w = self
            .app
            .as_ref()
            .map(|app| {
                let state = app.read(cx);
                chat_column_width(
                    win,
                    state.sidebar_hidden,
                    state.sidebar_collapsed,
                    state.sidebar_expanded_width,
                    state.right_pane != RightPane::Closed,
                )
            })
            .unwrap_or(win)
            .min(CHAT_CONTENT_MAX);
        let max_bubble = (chat_w * 0.88).min(640.0).min((chat_w - 82.0).max(160.0));
        // The bubble comes first. The toolbar's slot sits beside it only while the bubble
        // that leaves is still comfortable to read; narrower than that, the toolbar floats
        // above the bubble's corner and takes no width from the row, so the bubble may have
        // the whole of it: the cap's margin was only ever room for the slot.
        let row_w = (chat_w - 2.0 * ROW_PAD).max(0.0);
        let room = row_w - (TOOLBAR_W + TOOLBAR_GAP);
        let toolbar_floats = room < MIN_COMFORTABLE_BUBBLE;
        let max_bubble = px(if toolbar_floats {
            row_w.min(640.0)
        } else {
            max_bubble.min(room)
        });

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
        } else if !self.find_marks.is_empty() {
            find_highlighted_text(&self.text, &self.find_marks)
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
            .flex_shrink_0()
            .max_w(max_bubble)
            .when(self.is_me, |this| {
                this.px(px(14.)).py(px(8.)).rounded(px(BUBBLE_RADIUS))
            })
            .when(!self.is_me, |this| {
                this.px(px(12.)).py(px(8.)).rounded(px(BUBBLE_RADIUS))
            })
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
        // The bubble shrink-wraps its text up to its cap; the shrink is a safety net only, for
        // a row narrower than chat_w says. The chip sits on the bubble's bottom edge: half of
        // its 22px is on the fill.
        let bubble_stack = div()
            .relative()
            .flex_shrink(1.)
            .min_w_0()
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
                    .reading(self.native_speaking, self.native_paused)
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

        let (slot_toolbar, float_toolbar) = if toolbar_floats {
            (None, toolbar)
        } else {
            (toolbar, None)
        };
        // In the row: a slot of the toolbar's own width, always there so the bubble never
        // jumps, with the icons fading in on hover.
        let toolbar_slot = (!toolbar_floats).then(|| {
            div()
                .w(px(TOOLBAR_W))
                .h(px(CONTROL_PX))
                .flex_shrink_0()
                .flex()
                .items_center()
                .when(is_me, |this| this.justify_end())
                .opacity(if show_toolbar { 1. } else { 0. })
                .when_some(slot_toolbar, |this, toolbar| this.child(toolbar))
        });
        // Floating: on hover only, a pill above the bubble's outer top corner (the row's edge
        // on that side, so it never leaves the column whatever the bubble's width), in the
        // 32px the scroller keeps between rows: it covers no text in this bubble or the one
        // before, and the reaction chip on the bottom edge is out of its way. It hangs outside
        // the row's box, so it keeps its own hover; and it fades rather than goes while the
        // timestamps peek, so that hover stays consistent.
        let pill_hover = pill_hover_state.clone();
        let bubble_stack = bubble_stack.when_some(
            float_toolbar.filter(|_| self.show_footer && hovered),
            |this, toolbar| {
                this.child(
                    div()
                        .id(ElementId::Name(format!("msg-pill-{row_key}").into()))
                        .absolute()
                        .top(px(-(CONTROL_PX + 2. * PILL_PAD + 2. - PILL_OVERLAP)))
                        .when(is_me, |this| this.right(px(0.)))
                        .when(!is_me, |this| this.left(px(0.)))
                        .p(px(PILL_PAD))
                        .rounded(px(8.))
                        .bg(reaction_bg)
                        .border_1()
                        .border_color(reaction_border)
                        .shadow_sm()
                        .opacity(if peeking { 0. } else { 1. })
                        .on_hover(move |on, _, cx| {
                            pill_hover.update(cx, |state, cx| {
                                if *state != *on {
                                    *state = *on;
                                    cx.notify();
                                }
                            });
                        })
                        .child(toolbar),
                )
            },
        );

        let time_label = self.timestamp.clone().unwrap_or_default();

        let main = if is_me {
            h_flex()
                .w_full()
                .items_center()
                .justify_end()
                .gap(px(TOOLBAR_GAP))
                .children(toolbar_slot)
                .child(bubble_stack)
        } else {
            h_flex()
                .w_full()
                .items_center()
                .gap(px(TOOLBAR_GAP))
                .child(bubble_stack)
                .children(toolbar_slot)
        };

        let copy_text = self.copy_text.clone();
        let message_id = self.message_id.clone();
        let text = self.copy_text.clone();
        let hover_state_row = hover_state.clone();

        // Grok: --sand-ts-peek shifts every row together; timestamps slide in from
        // the right with --sand-ts-progress. The time shows only while peeking, never on
        // hover: the space beside the bubble is the toolbar's alone.
        let body_row = div()
            .relative()
            .w_full()
            .child(div().w_full().ml(px(-peek)).child(main))
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
                    .opacity(if peeking { progress } else { 0. })
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

/// Grok find: dim amber on every hit, solid `--cursor-warn` on the current one.
fn find_highlighted_text(text: &str, marks: &[(std::ops::Range<usize>, bool)]) -> AnyElement {
    let match_bg: Hsla = rgb(0xFFC000).opacity(0.3).into();
    let current_bg: Hsla = rgb(0xFFC000).into();
    let current_fg: Hsla = rgb(0x1F1F1F).into();
    let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = marks
        .iter()
        .filter_map(|(range, is_current)| {
            let start = floor_char_boundary(text, range.start.min(text.len()));
            let end = floor_char_boundary(text, range.end.min(text.len())).max(start);
            if start >= end {
                return None;
            }
            let style = if *is_current {
                HighlightStyle {
                    background_color: Some(current_bg),
                    color: Some(current_fg),
                    ..Default::default()
                }
            } else {
                HighlightStyle {
                    background_color: Some(match_bg),
                    ..Default::default()
                }
            };
            Some((start..end, style))
        })
        .collect();
    let body = if highlights.is_empty() {
        text.to_string().into_any_element()
    } else {
        StyledText::new(SharedString::from(text.to_string()))
            .with_highlights(highlights)
            .into_any_element()
    };
    div().text_sm().child(body).into_any_element()
}
