use std::sync::Arc;
use std::time::Duration;

use crate::actions::{PauseReadAloud, ResumeReadAloud, StopReadAloud, ToggleReadAloud};
use crate::chrome::{CHAT_CONTENT_MAX, chat_column_width, timestamps_fit};
use crate::components::chat_find::find_bar_element;
use crate::components::chat_input::MessageInput;
use crate::components::emoji_picker::{full_picker, reaction_strip};
use crate::components::gen_ui::{render_approval, render_screenshot, render_ui_spec};
use crate::components::message::{MessageBubble, TS_PEEK_MAX};
use crate::components::persona::PersonaMark;
use crate::find_text::{FindHit, marks_for_row, project_hits};
use crate::opengrok::{ApprovalSpec, ChatPart, ScreenshotSpec, UiSpec, collapse_open_approvals};
use crate::state::{AppState, EmojiPickerOpen};
use crate::tts_text::{looks_like_markdown, map_utf16_range_to_utf8};
use gpui_kit::base::{Align, Placement, Positioner};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::rc::Rc;

/// Cheap fingerprint so ChatView does not rebuild markdown on unrelated AppState
/// changes (sidebar toggle, theme, amplitude, etc.).
#[derive(Clone, PartialEq, Eq)]
struct ChatFeedRev {
    conversation_id: Option<String>,
    message_count: usize,
    last_id: Option<String>,
    last_len: usize,
    last_ui: usize,
    form_picks: Vec<(String, String, String)>,
    is_ai_responding: bool,
    debug_mode: bool,
    can_read_aloud: bool,
    theme_mode: String,
    native_speaking_id: Option<String>,
    native_paused: bool,
    native_loading: bool,
    highlight: Option<(usize, usize)>,
    reactions: Vec<(String, String)>,
    reply_to: Option<String>,
    emoji_for: Option<String>,
    approvals: Vec<(String, String)>,
    last_output: usize,
    expanded_output: Vec<String>,
}

impl ChatFeedRev {
    fn from_state(state: &AppState) -> Self {
        let conv = state
            .active_conversation_id
            .as_ref()
            .and_then(|id| state.conversations.iter().find(|c| &c.id == id));
        let last = conv.and_then(|c| c.messages.last());
        let highlight = state
            .active_highlight_range()
            .map(|range| (range.start, range.end));
        Self {
            conversation_id: state.active_conversation_id.clone(),
            message_count: conv.map(|c| c.messages.len()).unwrap_or(0),
            last_id: last.map(|m| m.id.clone()),
            last_len: last.map(|m| m.content.len()).unwrap_or(0),
            last_ui: last.map(|m| m.parts.len()).unwrap_or(0),
            form_picks: {
                let mut picks: Vec<(String, String, String)> = state
                    .form_picks
                    .iter()
                    .flat_map(|(msg, fields)| {
                        fields
                            .iter()
                            .map(|(field, value)| (msg.clone(), field.clone(), value.clone()))
                    })
                    .collect();
                picks.sort();
                picks
            },
            is_ai_responding: state.is_active_bot_responding(),
            debug_mode: state.debug_markdown_disabled,
            can_read_aloud: true,
            theme_mode: state.theme_mode.clone(),
            native_speaking_id: state.native_tts.message_id.clone(),
            native_paused: state.native_tts.is_paused,
            native_loading: state.native_tts.is_loading,
            highlight,
            reactions: {
                let mut pairs: Vec<(String, String)> = state
                    .message_reactions
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                pairs.sort();
                pairs
            },
            reply_to: state.reply_to.as_ref().map(|r| r.message_id.clone()),
            emoji_for: state.emoji_picker.as_ref().map(|p| p.message_id.clone()),
            approvals: {
                let mut pairs: Vec<(String, String)> = state
                    .approval_decisions
                    .iter()
                    .map(|(id, decision)| (id.clone(), format!("{decision:?}")))
                    .collect();
                pairs.sort();
                pairs
            },
            last_output: last
                .map(|msg| {
                    msg.parts
                        .iter()
                        .map(|part| match part {
                            ChatPart::Approval(spec) => {
                                spec.output.as_ref().map(String::len).unwrap_or(0)
                            }
                            _ => 0,
                        })
                        .sum()
                })
                .unwrap_or(0),
            expanded_output: {
                let mut ids: Vec<String> = state.expanded_shell_output.iter().cloned().collect();
                ids.sort();
                ids
            },
        }
    }
}

#[derive(Clone)]
struct ChatRow {
    id: String,
    content: SharedString,
    is_me: bool,
    timestamp: SharedString,
    is_native_speaking: bool,
    is_native_paused: bool,
    is_native_loading: bool,
    is_ai_speaking: bool,
    is_ai_paused: bool,
    is_ai_loading: bool,
    is_cached: bool,
    highlight_range: Option<std::ops::Range<usize>>,
    highlight_native: bool,
    use_markdown: bool,
    show_footer: bool,
    source_id: String,
    tts_text: SharedString,
    reply_preview: Option<String>,
    reaction: Option<String>,
    widget: Option<UiSpec>,
    approval: Option<ApprovalSpec>,
    status_line: Option<String>,
    screenshot: Option<ScreenshotSpec>,
}

impl ChatRow {
    /// A row with nothing said yet: no speech, no footer, no card. Each
    /// kind of row sets the few fields it owns on top of this.
    fn slot(id: String, source_id: String) -> Self {
        Self {
            id,
            content: SharedString::from(""),
            is_me: false,
            timestamp: SharedString::from(""),
            is_native_speaking: false,
            is_native_paused: false,
            is_native_loading: false,
            is_ai_speaking: false,
            is_ai_paused: false,
            is_ai_loading: false,
            is_cached: false,
            highlight_range: None,
            highlight_native: false,
            use_markdown: false,
            show_footer: false,
            source_id,
            tts_text: SharedString::from(""),
            reply_preview: None,
            reaction: None,
            widget: None,
            approval: None,
            status_line: None,
            screenshot: None,
        }
    }
}

fn snapshot_rows(state: &AppState) -> Arc<Vec<ChatRow>> {
    let Some(conv) = state
        .active_conversation_id
        .as_ref()
        .and_then(|id| state.conversations.iter().find(|c| &c.id == id))
    else {
        return Arc::new(Vec::new());
    };
    let mut rows = Vec::new();
    for msg in &conv.messages {
        let is_native_speaking = state.native_tts.message_id.as_ref() == Some(&msg.id);
        let full_highlight = if is_native_speaking {
            state
                .active_highlight_range()
                .and_then(|range| map_utf16_range_to_utf8(&msg.content, range))
        } else {
            None
        };
        if !msg.is_me && !msg.has_visible_body() {
            continue;
        }
        let highlight_native = is_native_speaking && full_highlight.is_some();
        let bot_name = state.active_bot_name();
        let display: Vec<ChatPart> = if msg.parts.iter().any(ChatPart::is_widget) {
            let open: std::collections::HashSet<String> = msg
                .parts
                .iter()
                .filter_map(|part| match part {
                    ChatPart::Approval(spec) => match state.approval_decisions.get(&spec.call_id) {
                        Some(decision) if decision.is_settled() => None,
                        _ => Some(spec.call_id.clone()),
                    },
                    _ => None,
                })
                .collect();
            collapse_open_approvals(&msg.parts, &open)
        } else {
            vec![ChatPart::Text(msg.content.clone())]
        };
        let mut text_buf = String::new();
        let mut ui_n = 0usize;
        let mut text_n = 0usize;
        let flush_text = |rows: &mut Vec<ChatRow>, text_buf: &mut String, text_n: &mut usize| {
            let text = std::mem::take(text_buf);
            if text.trim().is_empty() {
                return;
            }
            let id = text_row_id(&msg.id, *text_n);
            *text_n += 1;
            rows.push(ChatRow {
                content: SharedString::from(text.clone()),
                is_me: msg.is_me,
                timestamp: SharedString::from(msg.formatted_time()),
                is_native_speaking,
                is_native_paused: state.native_tts.is_paused && is_native_speaking,
                is_native_loading: state.native_tts.is_loading && is_native_speaking,
                highlight_range: full_highlight.clone(),
                highlight_native,
                use_markdown: !msg.is_me && looks_like_markdown(&text),
                show_footer: true,
                tts_text: SharedString::from(text),
                reply_preview: msg.reply_preview.clone(),
                reaction: state.message_reactions.get(&msg.id).cloned(),
                ..ChatRow::slot(id, msg.id.clone())
            });
        };
        for part in display {
            match part {
                ChatPart::Text(text) => text_buf.push_str(&text),
                ChatPart::Ui(spec) => {
                    flush_text(&mut rows, &mut text_buf, &mut text_n);
                    rows.push(ChatRow {
                        widget: Some(spec),
                        ..ChatRow::slot(format!("{}-ui-{ui_n}", msg.id), msg.id.clone())
                    });
                    ui_n += 1;
                }
                ChatPart::Screenshot(spec) => {
                    flush_text(&mut rows, &mut text_buf, &mut text_n);
                    rows.push(ChatRow {
                        screenshot: Some(spec),
                        ..ChatRow::slot(format!("{}-shot-{ui_n}", msg.id), msg.id.clone())
                    });
                    ui_n += 1;
                }
                ChatPart::Approval(spec) => {
                    flush_text(&mut rows, &mut text_buf, &mut text_n);
                    let outcome = state.approval_status_line(&spec, &bot_name);
                    rows.push(ChatRow {
                        content: SharedString::from(outcome.clone().unwrap_or_default()),
                        approval: outcome.is_none().then_some(spec),
                        status_line: outcome,
                        ..ChatRow::slot(format!("{}-ask-{ui_n}", msg.id), msg.id.clone())
                    });
                    ui_n += 1;
                }
            }
        }
        flush_text(&mut rows, &mut text_buf, &mut text_n);
    }
    Arc::new(rows)
}

#[derive(Clone)]
struct ChatPalette {
    primary: Hsla,
    primary_foreground: Hsla,
    secondary: Hsla,
    secondary_foreground: Hsla,
    yellow: Hsla,
    green: Hsla,
}

impl ChatPalette {
    fn from_cx(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            primary: theme.primary,
            primary_foreground: theme.primary_foreground,
            secondary: theme.secondary,
            secondary_foreground: theme.secondary_foreground,
            yellow: theme.yellow,
            green: theme.green,
        }
    }
}

struct ChatTranscript {
    app_state: Entity<AppState>,
    input: Entity<MessageInput>,
    scroller: Entity<MessageScrollerState>,
    rows: Arc<Vec<ChatRow>>,
    feed_rev: ChatFeedRev,
    last_conversation_id: Option<String>,
    debug_mode: bool,
    can_read_aloud: bool,
    palette: ChatPalette,
    highlight_pump: bool,
    ts_peek: f32,
    peek_epoch: u64,
    find_query: String,
    find_hits: Vec<FindHit>,
    find_current: Option<usize>,
}

impl ChatTranscript {
    fn new(state: Entity<AppState>, input: Entity<MessageInput>, cx: &mut Context<Self>) -> Self {
        let (rows, feed_rev, debug_mode, can_read_aloud, last_conversation_id) = {
            let app = state.read(cx);
            let feed_rev = ChatFeedRev::from_state(&app);
            (
                snapshot_rows(&app),
                feed_rev.clone(),
                feed_rev.debug_mode,
                feed_rev.can_read_aloud,
                app.active_conversation_id.clone(),
            )
        };
        let scroller = cx.new(|cx| MessageScrollerState::new(rows.len(), cx));
        cx.observe(&state, |this, state, cx| {
            let (feed, rows) = {
                let app = state.read(cx);
                let feed = ChatFeedRev::from_state(&app);
                if this.feed_rev == feed {
                    return;
                }
                (feed, snapshot_rows(&app))
            };
            let conv_changed = this.feed_rev.conversation_id != feed.conversation_id;
            let is_ai_responding = feed.is_ai_responding;
            this.rows = rows;
            this.debug_mode = feed.debug_mode;
            this.can_read_aloud = feed.can_read_aloud;
            this.last_conversation_id = feed.conversation_id.clone();
            this.palette = ChatPalette::from_cx(cx);
            this.feed_rev = feed;
            let count = this.rows.len();
            this.scroller.update(cx, |scroller, cx| {
                let old = scroller.item_count();
                if conv_changed || count != old {
                    scroller.reset(count, cx);
                } else if is_ai_responding && count > 0 {
                    scroller.remeasure_items(count - 1..count, cx);
                }
            });
            if this.feed_rev.native_speaking_id.is_some() {
                this.start_highlight_pump(cx);
            }
            this.recompute_find(false, cx);
            cx.notify();
        })
        .detach();

        let mut this = Self {
            app_state: state,
            input,
            scroller,
            rows,
            feed_rev,
            last_conversation_id,
            debug_mode,
            can_read_aloud,
            palette: ChatPalette::from_cx(cx),
            highlight_pump: false,
            ts_peek: 0.0,
            peek_epoch: 0,
            find_query: String::new(),
            find_hits: Vec::new(),
            find_current: None,
        };
        if this.feed_rev.native_speaking_id.is_some() {
            this.start_highlight_pump(cx);
        }
        this
    }

    fn set_find_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.find_query == query {
            return;
        }
        self.find_query = query;
        self.recompute_find(true, cx);
    }

    fn step_find(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.find_hits.is_empty() {
            return;
        }
        let len = self.find_hits.len() as i32;
        let index = match self.find_current {
            Some(current) => (current as i32 + delta).rem_euclid(len) as usize,
            None if delta < 0 => self.find_hits.len() - 1,
            None => 0,
        };
        self.find_current = Some(index);
        self.scroll_to_hit(index, cx);
        cx.notify();
    }

    fn clear_find(&mut self, cx: &mut Context<Self>) {
        self.find_query.clear();
        self.find_hits.clear();
        self.find_current = None;
        cx.notify();
    }

    fn find_status(&self) -> (Option<usize>, usize, bool) {
        (
            self.find_current,
            self.find_hits.len(),
            !self.find_query.trim().is_empty(),
        )
    }

    fn recompute_find(&mut self, land: bool, cx: &mut Context<Self>) {
        let hits = project_hits(
            self.rows.iter().map(|row| row.content.as_ref()),
            &self.find_query,
        );
        if hits.is_empty() {
            self.find_hits = hits;
            self.find_current = None;
            cx.notify();
            return;
        }
        if land {
            self.find_current = Some(0);
        } else if let Some(current) = self.find_current {
            if current >= hits.len() {
                self.find_current = Some(hits.len() - 1);
            }
        } else {
            self.find_current = Some(0);
        }
        self.find_hits = hits;
        if land {
            if let Some(index) = self.find_current {
                self.scroll_to_hit(index, cx);
            }
        }
        cx.notify();
    }

    fn scroll_to_hit(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(hit) = self.find_hits.get(index) else {
            return;
        };
        let row = hit.row;
        self.scroller.update(cx, |scroller, cx| {
            scroller.scroll_to_item(row, cx);
            let _ = scroller.remeasure_items(row..row + 1, cx);
        });
    }

    fn arm_peek_release(&mut self, cx: &mut Context<Self>) {
        self.peek_epoch = self.peek_epoch.wrapping_add(1);
        let epoch = self.peek_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(90))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.peek_epoch != epoch {
                    return;
                }
                this.ts_peek = 0.0;
                cx.notify();
            });
        })
        .detach();
    }

    /// Whether a wheel event is the timestamp gesture: a sideways swipe, or any sideways
    /// movement while the timestamps are already peeking. Decided in the CAPTURE phase, before
    /// the list underneath sees the event — the list scrolls by the vertical part of whatever
    /// reaches it, which is what made a sideways swipe creep up and down.
    fn is_peek_gesture(&self, dx: f32, dy: f32) -> bool {
        if self.ts_peek > 0.0 {
            dx.abs() > 0.5
        } else {
            dx.abs() > dy.abs() && dx.abs() >= 1.5
        }
    }

    /// Move the timestamp peek by a swipe. Natural scrolling: fingers moving left give a
    /// negative dx, and that is the swipe that reveals — the bubbles slide left to make room, as
    /// on the phone. So the peek grows with `-dx`.
    fn peek_by(&mut self, dx: f32, window: &Window, cx: &mut Context<Self>) {
        let win = f32::from(window.viewport_size().width);
        let app = self.app_state.read(cx);
        let chat_w = chat_column_width(
            win,
            app.sidebar_hidden,
            app.sidebar_collapsed,
            app.sidebar_expanded_width,
            app.right_pane != crate::state::RightPane::Closed,
        );
        if !timestamps_fit(chat_w) {
            return;
        }
        self.ts_peek = (self.ts_peek - dx).clamp(0.0, TS_PEEK_MAX);
        self.arm_peek_release(cx);
        cx.notify();
    }

    /// Tick highlight on this view only. Do not notify AppState (that dirties the window).
    fn start_highlight_pump(&mut self, cx: &mut Context<Self>) {
        if self.highlight_pump {
            return;
        }
        self.highlight_pump = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
                let keep = this
                    .update(cx, |this, cx| {
                        let app = this.app_state.read(cx);
                        let speaking = app.native_tts.message_id.is_some();
                        let feed = ChatFeedRev::from_state(&app);
                        if this.feed_rev != feed {
                            this.rows = snapshot_rows(&app);
                            this.feed_rev = feed;
                            cx.notify();
                        }
                        if !speaking {
                            this.highlight_pump = false;
                        }
                        speaking
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Render for ChatTranscript {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        let app_state = self.app_state.clone();
        let input = self.input.clone();
        let debug_mode = self.debug_mode;
        let can_read_aloud = self.can_read_aloud;
        let palette = self.palette.clone();
        let picker_id = self
            .app_state
            .read(cx)
            .emoji_picker
            .as_ref()
            .map(|p| p.message_id.clone());
        let ts_peek = self.ts_peek;
        let find_hits = self.find_hits.clone();
        let find_current = self.find_current;
        let timestamps_ok = {
            let win = f32::from(_window.viewport_size().width);
            let app = self.app_state.read(cx);
            timestamps_fit(chat_column_width(
                win,
                app.sidebar_hidden,
                app.sidebar_collapsed,
                app.sidebar_expanded_width,
                app.right_pane != crate::state::RightPane::Closed,
            ))
        };
        div()
            .id("chat-timestamp-peek")
            .size_full()
            .relative()
            // The gesture is taken in the capture phase over the whole transcript, so a
            // sideways swipe never reaches the list (no vertical creep) while a vertical one
            // passes through untouched. A canvas is the one element that can register a
            // capture-phase mouse handler from here.
            .child({
                let this = cx.entity().downgrade();
                gpui::canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _cx| {
                        let this = this.clone();
                        window.on_mouse_event(
                            move |event: &ScrollWheelEvent, phase, window, cx| {
                                if phase != gpui::DispatchPhase::Capture
                                    || !bounds.contains(&event.position)
                                {
                                    return;
                                }
                                let Some(this) = this.upgrade() else {
                                    return;
                                };
                                let delta = event.delta.pixel_delta(px(16.));
                                let dx = f32::from(delta.x);
                                let dy = f32::from(delta.y);
                                if !this.read(cx).is_peek_gesture(dx, dy) {
                                    return;
                                }
                                this.update(cx, |this, cx| this.peek_by(dx, window, cx));
                                cx.stop_propagation();
                            },
                        );
                    },
                )
                .absolute()
                .inset_0()
            })
            .child(
                MessageScroller::new(
                    "chat-messages",
                    self.scroller.clone(),
                    move |ix, window, cx| {
                        // Every row is laid out in the same centred column the transcript used to
                        // sit in, so bubbles and timestamps do not move — only the scrollbar did.
                        let row_body = |ix: usize,
                                        _window: &mut Window,
                                        cx: &mut App|
                         -> AnyElement {
                            let Some(row) = rows.get(ix) else {
                                return div().into_any_element();
                            };
                            if let Some(spec) = &row.widget {
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_start()
                                    .py(px(6.))
                                    .child(div().w_full().max_w(px(CHAT_CONTENT_MAX * 0.72)).child(
                                        render_ui_spec(
                                            spec,
                                            &row.source_id,
                                            Some(app_state.clone()),
                                            cx,
                                        ),
                                    ))
                                    .into_any_element();
                            }
                            if let Some(shot) = &row.screenshot {
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_start()
                                    .py(px(6.))
                                    .child(render_screenshot(shot, cx))
                                    .into_any_element();
                            }
                            if let Some(line) = &row.status_line {
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .py(px(8.))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(palette.secondary_foreground.opacity(0.7))
                                            .child(line.clone()),
                                    )
                                    .into_any_element();
                            }
                            if let Some(spec) = &row.approval {
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_start()
                                    .py(px(6.))
                                    .child(div().w_full().max_w(px(560.)).child(render_approval(
                                        spec,
                                        Some(app_state.clone()),
                                        cx,
                                    )))
                                    .into_any_element();
                            }
                            let highlight_color = if row.highlight_range.is_some() {
                                if row.highlight_native {
                                    Some(palette.yellow.opacity(0.4))
                                } else {
                                    Some(palette.green.opacity(0.4))
                                }
                            } else {
                                None
                            };
                            let (bg_color, text_color) = if row.is_me {
                                (palette.primary, palette.primary_foreground)
                            } else {
                                (palette.secondary, palette.secondary_foreground)
                            };
                            let state_entity = app_state.clone();
                            let source_id = row.source_id.clone();
                            let tts_text = row.tts_text.to_string();
                            let show_footer = row.show_footer;
                            let picker_open = picker_id.as_ref() == Some(&row.source_id);
                            let mut bubble = MessageBubble::new(row.content.to_string())
                                .message_id(row.id.clone())
                                .source_id(row.source_id.clone())
                                .is_me(row.is_me)
                                .bg_color(bg_color)
                                .text_color(text_color)
                                .timestamp(row.timestamp.to_string())
                                .debug_mode(debug_mode)
                                .can_read_aloud(can_read_aloud && show_footer)
                                .is_native_speaking(row.is_native_speaking)
                                .is_native_paused(row.is_native_paused)
                                .is_native_loading(row.is_native_loading)
                                .is_ai_speaking(row.is_ai_speaking)
                                .is_ai_paused(row.is_ai_paused)
                                .is_ai_loading(row.is_ai_loading)
                                .is_cached(row.is_cached)
                                .highlight_range(row.highlight_range.clone())
                                .highlight_color(highlight_color)
                                .find_marks(marks_for_row(ix, &find_hits, find_current))
                                .use_markdown(row.use_markdown)
                                .show_footer(show_footer)
                                .reply_preview(row.reply_preview.clone())
                                .reaction(row.reaction.clone())
                                .picker_open(picker_open)
                                .ts_peek(ts_peek)
                                .timestamps_ok(timestamps_ok)
                                .app_state(app_state.clone());
                            if show_footer {
                                let state_for_tts = state_entity.clone();
                                let source_for_tts = source_id.clone();
                                let tts = tts_text.clone();
                                bubble = bubble.copy_text(tts_text.clone()).on_read_aloud(
                                    move |_, cx| {
                                        state_for_tts.update(cx, |state, cx| {
                                            state.toggle_read_aloud(
                                                source_for_tts.clone(),
                                                tts.clone(),
                                                crate::actions::TtsSource::Native,
                                                cx,
                                            );
                                        });
                                    },
                                );
                                let input = input.clone();
                                bubble = bubble.on_reply(move |window, cx| {
                                    input.update(cx, |input, cx| {
                                        input.focus(window, cx);
                                    });
                                });
                            }
                            bubble.into_any_element()
                        };
                        div().w_full().flex().justify_center().px_4().child(
                            div()
                                .w_full()
                                .max_w(px(CHAT_CONTENT_MAX))
                                .child(row_body(ix, window, cx)),
                        )
                    },
                )
                // Straight under the title bar, which holds the chat's header.
                .pt(px(20.0))
                .with_jump_button_transition(Duration::ZERO),
            )
    }
}

pub struct ChatView {
    input: Entity<MessageInput>,
    state: Entity<AppState>,
    transcript: Entity<ChatTranscript>,
    last_conversation_id: Option<String>,
    bot_status: Option<String>,
    coworker_name: Option<String>,
    coworker_id: Option<String>,
    coworker_shape: Option<String>,
    coworker_color: Option<String>,
    emoji_search: Entity<InputState>,
    emoji_query: String,
    emoji_full: bool,
    emoji_category: usize,
    emoji_open: Option<EmojiPickerOpen>,
    find_open: bool,
    find_input: Entity<InputState>,
    find_pending_focus: bool,
}

impl ChatView {
    pub fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    /// The find bar, while a search is open: the title bar shows it in the chat's header.
    pub fn find_bar(&self, view: Entity<Self>, cx: &App) -> Option<AnyElement> {
        if !self.find_open {
            return None;
        }
        let (find_current, find_total, find_has_query) = self.transcript.read(cx).find_status();
        Some(
            find_bar_element(
                &self.find_input,
                find_current,
                find_total,
                find_has_query,
                {
                    let view = view.clone();
                    move |_, cx| {
                        view.update(cx, |this, cx| this.find_prev(cx));
                    }
                },
                {
                    let view = view.clone();
                    move |_, cx| {
                        view.update(cx, |this, cx| this.find_next(cx));
                    }
                },
                move |window, cx| {
                    view.update(cx, |this, cx| {
                        this.close_find(window, cx);
                    });
                },
                cx,
            )
            .into_any_element(),
        )
    }

    pub fn open_find(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.find_open = true;
        self.find_pending_focus = true;
        cx.notify();
    }

    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_open && self.find_input.read(cx).value().is_empty() {
            return;
        }
        self.find_open = false;
        self.find_pending_focus = false;
        self.find_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        self.transcript.update(cx, |transcript, cx| {
            transcript.clear_find(cx);
        });
        cx.notify();
    }

    pub fn find_next(&mut self, cx: &mut Context<Self>) {
        if !self.find_open {
            self.find_open = true;
            self.find_pending_focus = true;
            cx.notify();
            return;
        }
        self.transcript.update(cx, |transcript, cx| {
            transcript.step_find(1, cx);
        });
        cx.notify();
    }

    pub fn find_prev(&mut self, cx: &mut Context<Self>) {
        if !self.find_open {
            self.find_open = true;
            self.find_pending_focus = true;
            cx.notify();
            return;
        }
        self.transcript.update(cx, |transcript, cx| {
            transcript.step_find(-1, cx);
        });
        cx.notify();
    }

    fn render_emoji_overlay(
        &self,
        open: EmojiPickerOpen,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let app = self.state.clone();
        let search = self.emoji_search.clone();
        let query = self.emoji_query.clone();
        let category = self.emoji_category;
        let full = self.emoji_full;
        let view = cx.entity();
        let panel = if full {
            full_picker(
                app.clone(),
                open.message_id.clone(),
                search,
                query,
                category,
                Rc::new({
                    let view = view.clone();
                    move |ix, cx| {
                        view.update(cx, |this, cx| {
                            this.emoji_category = ix;
                            this.emoji_query.clear();
                            cx.notify();
                        });
                    }
                }),
                cx,
            )
            .into_any_element()
        } else {
            reaction_strip(
                app.clone(),
                open.message_id.clone(),
                Rc::new({
                    let view = view.clone();
                    move |_, cx| {
                        view.update(cx, |this, cx| {
                            this.emoji_full = true;
                            cx.notify();
                        });
                    }
                }),
                cx,
            )
            .into_any_element()
        };
        div()
            .id("emoji-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(MouseButton::Left, {
                let app = app.clone();
                move |_, _, cx| {
                    app.update(cx, |state, cx| state.close_emoji_picker(cx));
                }
            })
            .child(deferred(
                Positioner::side(open.bounds)
                    .placement(Placement::Bottom)
                    .align(Align::Start)
                    .offset(px(6.))
                    .occlude()
                    .child(
                        div()
                            .id("emoji-panel")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .child(panel),
                    ),
            ))
    }

    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            MessageInput::new(window, state.clone(), cx).on_submit({
                let state = state.clone();
                move |text, cx| {
                    state.update(cx, |state, cx| {
                        state.send_message(text, cx);
                    });
                }
            })
        });

        let last_conversation_id = state.read(cx).active_conversation_id.clone();

        let transcript = cx.new(|cx| ChatTranscript::new(state.clone(), input.clone(), cx));
        let emoji_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search emoji"));
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in chat"));

        let this = Self {
            input,
            state: state.clone(),
            transcript,
            last_conversation_id,
            bot_status: None,
            coworker_name: None,
            coworker_id: None,
            coworker_shape: None,
            coworker_color: None,
            emoji_search: emoji_search.clone(),
            emoji_query: String::new(),
            emoji_full: false,
            emoji_category: 0,
            emoji_open: None,
            find_open: false,
            find_input: find_input.clone(),
            find_pending_focus: false,
        };

        cx.observe(&state, |this: &mut Self, state, cx| {
            let current_id = state.read(cx).active_conversation_id.clone();
            if current_id != this.last_conversation_id {
                this.last_conversation_id = current_id;
                cx.notify();
            }
        })
        .detach();

        cx.observe(&state, |this, app, cx| {
            let app = app.read(cx);
            let label = app.visible_bot_status();
            let coworker = app
                .active_coworker_id
                .as_ref()
                .and_then(|id| app.coworkers.iter().find(|c| &c.id == id));
            let coworker_name = coworker.map(|c| c.name.clone());
            let coworker_id = coworker.map(|c| c.id.clone());
            let coworker_shape = coworker.and_then(|c| c.avatar_shape.clone());
            let coworker_color = coworker.and_then(|c| c.avatar_color.clone());
            let emoji_open = app.emoji_picker.clone();
            let mut changed = false;
            if this.bot_status != label {
                this.bot_status = label;
                changed = true;
            }
            if this.coworker_name != coworker_name {
                this.coworker_name = coworker_name;
                changed = true;
            }
            if this.coworker_id != coworker_id {
                this.coworker_id = coworker_id;
                changed = true;
            }
            if this.coworker_shape != coworker_shape {
                this.coworker_shape = coworker_shape;
                changed = true;
            }
            if this.coworker_color != coworker_color {
                this.coworker_color = coworker_color;
                changed = true;
            }
            let picker_changed = match (&this.emoji_open, &emoji_open) {
                (None, None) => false,
                (Some(a), Some(b)) => a.message_id != b.message_id,
                _ => true,
            };
            if picker_changed {
                this.emoji_open = emoji_open;
                this.emoji_full = false;
                this.emoji_category = 0;
                this.emoji_query.clear();
                changed = true;
            }
            if changed {
                cx.notify();
            }
        })
        .detach();

        cx.subscribe(
            &emoji_search,
            |this: &mut Self, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.emoji_query = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe(
            &find_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    this.transcript.update(cx, |transcript, cx| {
                        transcript.set_find_query(query, cx);
                    });
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    let delta = if *shift { -1 } else { 1 };
                    this.transcript.update(cx, |transcript, cx| {
                        transcript.step_find(delta, cx);
                    });
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        this
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.find_pending_focus {
            self.find_pending_focus = false;
            self.find_input.update(cx, |input, cx| {
                input.focus(window, cx);
                input.select_all(window, cx);
            });
        }
        let theme = cx.theme().clone();

        let app = self.state.clone();
        v_flex()
            .size_full()
            .bg(theme.background)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                app.update(cx, |state, cx| {
                    if state.model_picker_open || state.avatar_editor_open {
                        state.dismiss_popovers(cx);
                    }
                });
            })
            .on_action({
                let state = self.state.clone();
                move |action: &ToggleReadAloud, _window: &mut Window, cx: &mut App| {
                    println!(
                        "[Chat] ToggleReadAloud action received. ID: {}, Mode: {:?}",
                        action.message_id, action.mode
                    );
                    state.update(cx, |state, cx| {
                        state.toggle_read_aloud(
                            action.message_id.clone(),
                            action.text.clone(),
                            action.mode.clone(),
                            cx,
                        );
                    });
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &PauseReadAloud, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| {
                        state.pause_read_aloud(cx);
                    });
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &ResumeReadAloud, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| {
                        state.resume_read_aloud(cx);
                    });
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &StopReadAloud, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| {
                        state.stop_read_aloud(cx);
                    });
                }
            })
            .child(
                // Main Content Area (Header + Messages)
                div()
                    .id("chat-transcript-slot")
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        // The transcript spans the whole slot so its scrollbar sits on the
                        // pane's edge; each row centres itself inside (see `ChatTranscript`).
                        div().absolute().inset_0().child(
                            div().id("chat-content-col").size_full().child(
                                self.transcript
                                    .clone()
                                    .cached(StyleRefinement::default().size_full()),
                            ),
                        ),
                    )
                    .when_some(self.emoji_open.clone(), |this, open| {
                        this.child(self.render_emoji_overlay(open, cx))
                    }),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .w_full()
                    .items_center()
                    .px_4()
                    .pb_4()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(CHAT_CONTENT_MAX))
                            .gap_2()
                            .when_some(self.bot_status.clone(), |this, label| {
                                let name =
                                    self.coworker_name.clone().unwrap_or_else(|| "Agent".into());
                                let id = self.coworker_id.clone().unwrap_or_default();
                                this.child(
                                    h_flex()
                                        .id("bot-working")
                                        .gap(px(8.))
                                        .items_center()
                                        .px_1()
                                        .tooltip(move |w, cx| {
                                            Tooltip::new(label.clone()).build(w, cx)
                                        })
                                        .child(
                                            PersonaMark::new(id)
                                                .shape(self.coworker_shape.clone())
                                                .color(self.coworker_color.clone())
                                                .size(px(20.))
                                                .dark(theme.is_dark()),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(theme.muted_foreground)
                                                .child(format!("{name} is working")),
                                        ),
                                )
                            })
                            .child(self.input.clone()),
                    ),
            )
    }
}

/// One message can flush several text rows (text, then a card, then more
/// text). Their element ids key hover and menu state, so each row needs its
/// own. The first keeps the message id; later ones get an ordinal.
fn text_row_id(msg_id: &str, n: usize) -> String {
    if n == 0 {
        msg_id.to_string()
    } else {
        format!("{msg_id}-t{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::text_row_id;

    #[test]
    fn text_rows_of_one_message_get_distinct_ids() {
        assert_eq!(text_row_id("m1", 0), "m1");
        assert_eq!(text_row_id("m1", 1), "m1-t1");
        assert_ne!(text_row_id("m1", 1), text_row_id("m1", 2));
    }
}
