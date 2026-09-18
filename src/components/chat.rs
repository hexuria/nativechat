use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::actions::{PauseReadAloud, ResumeReadAloud, StopReadAloud, ToggleReadAloud};
use crate::chrome::{CHAT_CONTENT_MAX, chat_column_width, timestamps_fit};
use crate::components::chat_find::find_bar_element;
use crate::components::chat_input::MessageInput;
use crate::components::emoji_picker::{full_picker, reaction_strip};
use crate::components::gen_ui::{render_approval, render_screenshots, render_ui_spec};
use crate::components::message::{MessageBubble, TS_PEEK_MAX};
use crate::components::persona::PersonaMark;
use crate::components::save_login::{render_credential_request, render_save_login};
use crate::components::user_form::{
    UserFormInputMap, UserFormTextareaMap, field_key, render_user_form,
};
use crate::find_text::{FindHit, marks_for_row, project_hits};
use crate::opengrok::{
    ApprovalSpec, ChatPart, CredentialRequestSpec, SaveLoginSpec, ScreenshotSpec, UiSpec,
    UserFormSpec, UserFormValues, collapse_open_approvals,
};
use crate::state::{
    AppState, EmojiPickerOpen, STOPPED_TURN_NOTE, bot_status_line, is_status_line, is_tool_standin,
    is_unsent_turn_note,
};
use crate::tts_text::{looks_like_markdown, map_utf16_range_to_utf8};
use gpui_kit::base::{Align, Placement, Positioner};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

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
    user_form_picks: Vec<(String, String, String)>,
    user_forms: Vec<(String, String, String)>,
    user_form_verbs: bool,
    user_form_handoffs: Vec<(String, String, bool)>,
    save_logins: Vec<(String, String)>,
    credential_requests: Vec<(String, String)>,
    box_screen: bool,
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
            user_form_picks: {
                let mut picks: Vec<(String, String, String)> = state
                    .user_form_picks
                    .iter()
                    .flat_map(|(entry, fields)| {
                        fields
                            .iter()
                            .map(|(field, value)| (entry.clone(), field.clone(), value.clone()))
                    })
                    .collect();
                picks.sort();
                picks
            },
            user_forms: {
                let mut cards: Vec<(String, String, String)> = conv
                    .map(|c| {
                        c.messages
                            .iter()
                            .flat_map(|m| m.parts.iter())
                            .filter_map(|part| match part {
                                ChatPart::UserForm(spec) => Some((
                                    spec.card_key().to_string(),
                                    spec.effective_resolution()
                                        .map(|r| r.as_str().to_string())
                                        .unwrap_or_else(|| {
                                            if spec.has_gateway_entry_id() {
                                                "idle".into()
                                            } else {
                                                "idle-no-entry".into()
                                            }
                                        }),
                                    spec.computer_handoff
                                        .map(|status| status.as_str().to_string())
                                        .unwrap_or_else(|| "none".into()),
                                )),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                cards.sort();
                cards
            },
            user_form_verbs: state.user_form_verbs_available,
            user_form_handoffs: {
                let mut rows: Vec<(String, String, bool)> = conv
                    .map(|c| {
                        c.messages
                            .iter()
                            .flat_map(|m| m.parts.iter())
                            .filter_map(|part| match part {
                                ChatPart::UserForm(spec) => {
                                    let id = spec
                                        .handoff_entry_id
                                        .clone()
                                        .or_else(|| state.user_form_handoff_id(spec.card_key()))
                                        .unwrap_or_default();
                                    Some((
                                        spec.card_key().to_string(),
                                        id,
                                        state.user_form_handoff_resolved(spec.card_key()),
                                    ))
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                rows.sort();
                rows
            },
            save_logins: {
                let mut cards: Vec<(String, String)> = conv
                    .map(|c| {
                        c.messages
                            .iter()
                            .flat_map(|m| m.parts.iter())
                            .filter_map(|part| match part {
                                ChatPart::SaveLogin(spec) => {
                                    Some((spec.form_entry_id.clone(), spec.username.clone()))
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                cards.sort();
                cards
            },
            credential_requests: {
                let mut cards: Vec<(String, String)> = conv
                    .map(|c| {
                        c.messages
                            .iter()
                            .flat_map(|m| m.parts.iter())
                            .filter_map(|part| match part {
                                ChatPart::CredentialRequest(spec) => {
                                    Some((spec.request_id.clone(), spec.origin.clone()))
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                cards.sort();
                cards
            },
            box_screen: state.coworker_screen.is_some() || state.last_box_shot.is_some(),
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
    /// A status line about a run that failed, painted in the danger colour rather than dimmed.
    status_failed: bool,
    /// A status line for a turn that never left, which carries the offer to send it again.
    status_retry: bool,
    /// The pictures of one stretch of a turn, which the row paints as one strip and the
    /// lightbox pages through as one set.
    screenshots: Vec<ScreenshotSpec>,
    user_form: Option<UserFormSpec>,
    save_login: Option<SaveLoginSpec>,
    credential_request: Option<CredentialRequestSpec>,
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
            status_failed: false,
            status_retry: false,
            screenshots: Vec::new(),
            user_form: None,
            save_login: None,
            credential_request: None,
        }
    }
}

/// Whether a picture belongs to the strip the row before it already holds. Pictures are one
/// set when they came from the same turn with nothing but pictures between them — that is
/// what makes a strip, rather than a column of full-width screens nobody scrolls past.
fn joins_previous_set(rows: &[ChatRow], message_id: &str) -> bool {
    rows.last()
        .is_some_and(|last| !last.screenshots.is_empty() && last.source_id == message_id)
}

fn snapshot_rows(state: &AppState) -> Arc<Vec<ChatRow>> {
    let Some(conv) = state
        .active_conversation_id
        .as_ref()
        .and_then(|id| state.conversations.iter().find(|c| &c.id == id))
    else {
        return Arc::new(Vec::new());
    };
    // The one turn the thread would send again, if it has one. Asked once rather than per row,
    // and by id, because only the thread's last turn is the one a retry would be about.
    let retryable = state.retryable_turn();
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
            // What the app says about a turn, and the stand-in a wordless turn leaves, are not
            // speech: they get the quiet centred line, not a bubble with a toolbar on it.
            if !msg.is_me && (is_status_line(&text) || is_tool_standin(&text)) {
                rows.push(ChatRow {
                    // Red is for a turn that went wrong. A turn the person stopped went exactly
                    // as they asked, and a turn that never left did not go wrong either — it did
                    // not go — so both get the quiet grey the stand-ins get; painting either in
                    // the colour of a failure would send someone looking for what broke. A turn
                    // the app would not send while it had no session is the second of those: the
                    // red line about spend limits is exactly what this replaces.
                    status_failed: is_status_line(&text)
                        && text.trim() != STOPPED_TURN_NOTE
                        && !is_unsent_turn_note(&text),
                    status_retry: retryable.as_ref() == Some(&msg.id),
                    content: SharedString::from(text.clone()),
                    status_line: Some(text),
                    ..ChatRow::slot(id, msg.id.clone())
                });
                return;
            }
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
                    if joins_previous_set(&rows, &msg.id) {
                        if let Some(last) = rows.last_mut() {
                            last.screenshots.push(spec);
                        }
                        continue;
                    }
                    rows.push(ChatRow {
                        screenshots: vec![spec],
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
                ChatPart::UserForm(spec) => {
                    flush_text(&mut rows, &mut text_buf, &mut text_n);
                    rows.push(ChatRow {
                        user_form: Some(spec),
                        ..ChatRow::slot(format!("{}-user-form-{ui_n}", msg.id), msg.id.clone())
                    });
                    ui_n += 1;
                }
                ChatPart::SaveLogin(spec) => {
                    flush_text(&mut rows, &mut text_buf, &mut text_n);
                    rows.push(ChatRow {
                        save_login: Some(spec),
                        ..ChatRow::slot(format!("{}-save-login-{ui_n}", msg.id), msg.id.clone())
                    });
                    ui_n += 1;
                }
                ChatPart::CredentialRequest(spec) => {
                    flush_text(&mut rows, &mut text_buf, &mut text_n);
                    rows.push(ChatRow {
                        credential_request: Some(spec),
                        ..ChatRow::slot(
                            format!("{}-credential-request-{ui_n}", msg.id),
                            msg.id.clone(),
                        )
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
    danger: Hsla,
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
            danger: theme.danger,
        }
    }
}

/// The air the transcript keeps at either end: above the first bubble, and between the last
/// one and the composer floating over it, on top of the composer's own height.
///
/// `MessageScroller` gives its list this much as `py_2`, and the rows have to carry it
/// instead, because the list must have no padding at all. GPUI measures the list's scroll
/// twice and only one of the two counts that padding: the wheel reads the offset in the items'
/// own space, where the floor is `items + padding - viewport`, while the mask that turns the
/// wheel into an offset clamps against `items - viewport` and writes the clamped value back.
/// Room held in the padding is therefore a teleport of exactly that much on the first scroll
/// away from the bottom. Room held inside a row is part of `items` and both readings agree.
const TRANSCRIPT_EDGE_GAP: f32 = 8.0;

/// The room a row keeps beneath itself, which only the last one has any of.
///
/// The composer floats over the transcript's bottom, so the final bubble has to be able to
/// come to rest above it rather than under it, and that room has to be part of the row's own
/// measured height — see [`TRANSCRIPT_EDGE_GAP`] for why it cannot be the list's padding.
fn tail_room(ix: usize, row_count: usize, composer_height: Pixels) -> Option<Pixels> {
    (row_count > 0 && ix + 1 == row_count).then(|| composer_height + px(TRANSCRIPT_EDGE_GAP))
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
    /// How tall the composer floating over the transcript's bottom stands right now, as the
    /// composer itself last measured it (see `ChatView::render`). Zero until that first
    /// measurement lands, which is one frame.
    composer_height: Pixels,
    user_form_inputs: UserFormInputMap,
    user_form_textareas: UserFormTextareaMap,
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
            composer_height: px(0.),
            user_form_inputs: HashMap::new(),
            user_form_textareas: HashMap::new(),
        };
        if this.feed_rev.native_speaking_id.is_some() {
            this.start_highlight_pump(cx);
        }
        this
    }

    /// The composer floats over the transcript's bottom, so the rows have to come to rest a
    /// composer's height above the pane's floor: a last bubble that stopped at the floor
    /// would be read through the pill, and no amount of scrolling could free it. The composer
    /// measures itself and tells us, because how tall it stands is the draft's business — a
    /// second line, a recipe bar or the working line each move it.
    fn set_composer_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        // Every layout pass the composer takes reports again, and each notify buys another
        // frame: only a height that has really moved is worth one. Half a pixel is under what
        // a row can show and over what rounding can invent.
        if (self.composer_height - height).abs() < px(0.5) {
            return;
        }
        self.composer_height = height;
        // The room belongs to the last row's own height, so the list is holding a measurement
        // of that row taken against the composer as it used to stand. Nothing else about the
        // row changed, so only that one is worth taking again.
        self.scroller.update(cx, |scroller, cx| {
            if let Some(last) = scroller.item_count().checked_sub(1) {
                let _ = scroller.remeasure_items(last..last + 1, cx);
            }
        });
        cx.notify();
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

    fn sync_user_form_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut needed: HashMap<String, (bool, bool, Option<String>, Option<String>)> =
            HashMap::new();
        for row in self.rows.iter() {
            let Some(spec) = &row.user_form else {
                continue;
            };
            if !spec.is_unresolved() {
                continue;
            }
            for field in &spec.fields {
                match field.kind {
                    crate::opengrok::UserFormFieldKind::Checkbox
                    | crate::opengrok::UserFormFieldKind::Select => {}
                    crate::opengrok::UserFormFieldKind::Textarea => {
                        needed.insert(
                            field_key(spec.card_key(), &field.id),
                            (
                                true,
                                field.masked(),
                                field.placeholder.clone(),
                                field.prefill.clone(),
                            ),
                        );
                    }
                    _ => {
                        needed.insert(
                            field_key(spec.card_key(), &field.id),
                            (
                                false,
                                field.masked(),
                                field.placeholder.clone(),
                                field.prefill.clone(),
                            ),
                        );
                    }
                }
            }
        }
        self.user_form_inputs
            .retain(|key, _| needed.get(key).is_some_and(|(textarea, _, _, _)| !textarea));
        self.user_form_textareas
            .retain(|key, _| needed.get(key).is_some_and(|(textarea, _, _, _)| *textarea));
        for (key, (textarea, masked, placeholder, prefill)) in needed {
            if textarea {
                if self.user_form_textareas.contains_key(&key) {
                    continue;
                }
                let placeholder = placeholder.unwrap_or_default();
                let prefill = prefill.unwrap_or_default();
                let state = cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx);
                    if !placeholder.is_empty() {
                        state = state.placeholder(placeholder);
                    }
                    if !prefill.is_empty() {
                        state = state.default_value(prefill);
                    }
                    state
                });
                cx.subscribe(&state, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
                .detach();
                self.user_form_textareas.insert(key, state);
            } else if !self.user_form_inputs.contains_key(&key) {
                let placeholder = placeholder.unwrap_or_default();
                let prefill = prefill.unwrap_or_default();
                let state = cx.new(|cx| {
                    let mut state = InputState::new(window, cx);
                    if !placeholder.is_empty() {
                        state = state.placeholder(placeholder);
                    }
                    if masked {
                        state = state.masked(true);
                    }
                    if !prefill.is_empty() {
                        state = state.default_value(prefill);
                    }
                    state
                });
                cx.subscribe(&state, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
                .detach();
                self.user_form_inputs.insert(key, state);
            }
        }
    }
}

impl Render for ChatTranscript {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_user_form_fields(window, cx);
        let rows = self.rows.clone();
        let app_state = self.app_state.clone();
        let user_form_inputs = self.user_form_inputs.clone();
        let user_form_textareas = self.user_form_textareas.clone();
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
        let composer_height = self.composer_height;
        let timestamps_ok = {
            let win = f32::from(window.viewport_size().width);
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
                            if !row.screenshots.is_empty() {
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_start()
                                    .py(px(6.))
                                    .child(render_screenshots(
                                        &row.screenshots,
                                        Some(app_state.clone()),
                                        cx,
                                    ))
                                    .into_any_element();
                            }
                            if let Some(line) = &row.status_line {
                                // A run that failed said nothing the person can act on: the line
                                // that explains it is worth seeing, not worth reading as speech.
                                let color = if row.status_failed {
                                    palette.danger
                                } else {
                                    palette.secondary_foreground.opacity(0.7)
                                };
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .items_center()
                                    .gap(px(8.))
                                    .py(px(8.))
                                    .child(div().text_sm().text_color(color).child(line.clone()))
                                    // A turn that never left is the one status line worth
                                    // answering: retyping the message was the only way back
                                    // from it, and the message is still right there.
                                    .when(row.status_retry, |this| {
                                        let state = app_state.clone();
                                        this.child(
                                            div()
                                                .id("retry-turn")
                                                .text_sm()
                                                .text_color(palette.primary)
                                                .cursor_pointer()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    move |_, _, cx| {
                                                        state.update(cx, |state, cx| {
                                                            state.retry_turn(cx);
                                                        });
                                                    },
                                                )
                                                .child("Try again"),
                                        )
                                    })
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
                            if let Some(spec) = &row.user_form {
                                let picks = app_state
                                    .read(cx)
                                    .user_form_picks
                                    .get(spec.card_key())
                                    .cloned()
                                    .unwrap_or_default();
                                let mut values = UserFormValues { by_id: picks };
                                for field in &spec.fields {
                                    if values.by_id.contains_key(&field.id) {
                                        continue;
                                    }
                                    let key = field_key(spec.card_key(), &field.id);
                                    let raw = if let Some(state) = user_form_textareas.get(&key) {
                                        Some(state.read(cx).value().to_string())
                                    } else {
                                        user_form_inputs
                                            .get(&key)
                                            .map(|state| state.read(cx).value().to_string())
                                    };
                                    let Some(raw) = raw else {
                                        continue;
                                    };
                                    // Secrets stay in InputState. Do not put a presence stub
                                    // in this map: Continue reads live InputState, and a
                                    // non-empty stub would look like a filled password.
                                    if field.masked() {
                                        continue;
                                    }
                                    if raw.trim().is_empty() {
                                        continue;
                                    }
                                    values.by_id.insert(field.id.clone(), raw);
                                }
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_start()
                                    .py(px(6.))
                                    .child(div().w_full().max_w(px(560.)).child(render_user_form(
                                        spec,
                                        &user_form_inputs,
                                        &user_form_textareas,
                                        &values,
                                        Some(app_state.clone()),
                                        cx,
                                    )))
                                    .into_any_element();
                            }
                            if let Some(spec) = &row.save_login {
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_start()
                                    .py(px(6.))
                                    .child(div().w_full().max_w(px(560.)).child(render_save_login(
                                        spec,
                                        app_state.clone(),
                                        cx,
                                    )))
                                    .into_any_element();
                            }
                            if let Some(spec) = &row.credential_request {
                                return div()
                                    .id(ElementId::Name(row.id.clone().into()))
                                    .w_full()
                                    .flex()
                                    .justify_start()
                                    .py(px(6.))
                                    .child(div().w_full().max_w(px(560.)).child(
                                        render_credential_request(spec, app_state.clone(), cx),
                                    ))
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
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .px_4()
                            // The transcript's own edges, carried by the rows that sit against
                            // them rather than by the list's padding (see `tail_room`).
                            .when(ix == 0, |this| this.pt(px(TRANSCRIPT_EDGE_GAP)))
                            .when_some(tail_room(ix, rows.len(), composer_height), |this, room| {
                                this.pb(room)
                            })
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(CHAT_CONTENT_MAX))
                                    .child(row_body(ix, window, cx)),
                            )
                    },
                )
                // Straight under the title bar, which holds the chat's header.
                .pt(px(20.0))
                // No padding on the list. The room at both ends travels with the rows, so
                // that the height of the items is the whole of the transcript and the two
                // ways GPUI measures the scroll cannot disagree — see `TRANSCRIPT_EDGE_GAP`.
                .with_list_style(StyleRefinement::default().py(px(0.)))
                // The chevron belongs over the chat, not behind the composer, so lift it off
                // the scroller's floor by exactly what the composer covers; the rem the
                // scroller already holds it by then reads from the composer's top edge.
                .with_jump_button_style(StyleRefinement::default().mb(composer_height))
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
                // The transcript is the whole column: the composer that follows is out of the
                // flow, so nothing takes a strip off the bottom of this.
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
                // The composer floats over the transcript instead of standing beside it. As a
                // row of its own it took its height out of the transcript's, and the last
                // bubble was sliced off at the row's top edge with nothing able to scroll past
                // that line; and the band it held, opaque or not, read as a wall across the
                // column. Nothing here paints: the chat runs on underneath, and only the pill
                // and the working line are solid. The transcript holds its rows a composer's
                // height clear of the floor, so the last one can still be read in full.
                v_flex()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .items_center()
                    .px_4()
                    .pb_4()
                    .child({
                        // How far the transcript has to hold back is whatever the composer
                        // grew to, and it grows with the draft, the recipe bar and the working
                        // line, so it is taken from the laid-out box rather than guessed. The
                        // canvas is absolute, so asking costs the composer no room, and it
                        // spans the padding box, so the 16px below the pill is counted too.
                        let transcript = self.transcript.clone();
                        gpui::canvas(
                            move |bounds, _, cx| {
                                let height = bounds.size.height;
                                if transcript.read(cx).composer_height == height {
                                    return;
                                }
                                // A notify raised here would be swallowed: the window clears
                                // its dirty flag when the frame begins, so nothing would ask
                                // for the next one and the transcript would keep the old floor
                                // until something else redrew it. Handing the height over once
                                // the frame is off leaves the notify a frame to buy.
                                let transcript = transcript.clone();
                                cx.defer(move |cx| {
                                    transcript.update(cx, |transcript, cx| {
                                        transcript.set_composer_height(height, cx);
                                    });
                                });
                            },
                            |_, _, _, _| (),
                        )
                        .absolute()
                        .inset_0()
                    })
                    .child(
                        // Everything the composer is made of is in this column, and it is
                        // exactly as wide as the pill, so this is the one box that may take
                        // the mouse. The chat runs on behind the band to either side of it,
                        // and a click there is a click on the chat: the band itself must stay
                        // as invisible to the mouse as it is to the eye, or it would be the
                        // wall again with nothing to show for it. Occluding is what makes a
                        // click on the pill the field's own — without it the click also
                        // reaches whatever row of the transcript happens to be underneath,
                        // which opened pictures in the lightbox while the person was only
                        // trying to type.
                        v_flex()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, {
                                // Taking the click also takes it from the chat's own surface,
                                // and what that surface does with one is shut the right pane's
                                // popovers — see this view's root, and the sidebar's.
                                let app = self.state.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        if state.model_picker_open || state.avatar_editor_open {
                                            state.dismiss_popovers(cx);
                                        }
                                    });
                                }
                            })
                            .w_full()
                            .max_w(px(CHAT_CONTENT_MAX))
                            .gap_2()
                            .when_some(self.bot_status.clone(), |this, label| {
                                let name =
                                    self.coworker_name.clone().unwrap_or_else(|| "Agent".into());
                                let id = self.coworker_id.clone().unwrap_or_default();
                                let tooltip_label = label.clone();
                                this.child(
                                    h_flex()
                                        .id("bot-working")
                                        .gap(px(8.))
                                        .items_center()
                                        .px_1()
                                        .tooltip(move |w, cx| {
                                            Tooltip::new(tooltip_label.clone()).build(w, cx)
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
                                                .child(bot_status_line(&name, &label)),
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
    use super::{
        ChatRow, ScreenshotSpec, TRANSCRIPT_EDGE_GAP, joins_previous_set, tail_room, text_row_id,
    };
    use gpui_kit::px;

    /// The room is the last bubble's alone: give it to every row and the transcript would be
    /// mostly air, and the rows above the composer would each hold a composer's worth of it.
    #[test]
    fn only_the_last_bubble_keeps_room_for_the_composer() {
        let composer = px(96.);
        assert_eq!(tail_room(0, 3, composer), None);
        assert_eq!(tail_room(1, 3, composer), None);
        assert_eq!(
            tail_room(2, 3, composer),
            Some(composer + px(TRANSCRIPT_EDGE_GAP))
        );
    }

    /// The composer grows with the draft, the recipe bar and the working line, and the room
    /// under the last bubble is what keeps it clear of all of that.
    #[test]
    fn the_room_under_the_last_bubble_follows_the_composers_height() {
        let short = tail_room(0, 1, px(72.));
        let tall = tail_room(0, 1, px(220.));
        assert_eq!(short, Some(px(72. + TRANSCRIPT_EDGE_GAP)));
        assert_eq!(tall, Some(px(220. + TRANSCRIPT_EDGE_GAP)));
        assert!(tall > short, "a taller composer has to take more room");
    }

    /// A transcript with nothing in it has no last bubble to hold anything off the floor.
    #[test]
    fn an_empty_transcript_holds_nothing_back() {
        assert_eq!(tail_room(0, 0, px(96.)), None);
    }

    #[test]
    fn text_rows_of_one_message_get_distinct_ids() {
        assert_eq!(text_row_id("m1", 0), "m1");
        assert_eq!(text_row_id("m1", 1), "m1-t1");
        assert_ne!(text_row_id("m1", 1), text_row_id("m1", 2));
    }

    fn shot(call_id: &str) -> ScreenshotSpec {
        ScreenshotSpec {
            call_id: call_id.to_string(),
            caption: String::new(),
            image: std::sync::Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Png,
                Vec::new(),
            )),
            width: 1280,
            height: 800,
            visibility: None,
        }
    }

    fn picture_row(message_id: &str, call_id: &str) -> ChatRow {
        ChatRow {
            screenshots: vec![shot(call_id)],
            ..ChatRow::slot(format!("{message_id}-shot-0"), message_id.to_string())
        }
    }

    fn word_row(message_id: &str) -> ChatRow {
        ChatRow::slot(message_id.to_string(), message_id.to_string())
    }

    /// Several pictures in a row from one turn are one set, so the transcript shows a strip
    /// and the lightbox can page through all of them.
    #[test]
    fn pictures_that_follow_one_another_in_a_turn_are_one_set() {
        let rows = vec![word_row("m1"), picture_row("m1", "call-1")];
        assert!(joins_previous_set(&rows, "m1"));
    }

    /// A turn's pictures are its own: the next turn starts a strip of its own, however many
    /// pictures the one before it ended on.
    #[test]
    fn the_next_turns_pictures_start_their_own_set() {
        let rows = vec![picture_row("m1", "call-1")];
        assert!(!joins_previous_set(&rows, "m2"));
    }

    /// Words between two pictures break the strip: what the bot said belongs between them.
    #[test]
    fn words_between_two_pictures_end_the_set() {
        let rows = vec![picture_row("m1", "call-1"), word_row("m1")];
        assert!(!joins_previous_set(&rows, "m1"));
    }

    #[test]
    fn the_first_picture_of_the_transcript_has_nothing_to_join() {
        assert!(!joins_previous_set(&[], "m1"));
    }
}
