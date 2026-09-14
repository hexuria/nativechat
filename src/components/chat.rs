use std::sync::Arc;
use std::time::Duration;

use crate::actions::{PauseReadAloud, ResumeReadAloud, StopReadAloud, ToggleReadAloud};
use crate::components::chat_input::MessageInput;
use crate::components::message::MessageBubble;
use crate::services::tts_service::TtsService;
use crate::components::persona::PersonaMark;
use crate::state::AppState;
use crate::tts_text::{
    CHAT_ROW_CHUNK_BYTES, chunk_text, highlight_in_chunk, looks_like_markdown,
    map_utf16_range_to_utf8,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::FontWeight;
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};
use gpui_kit::component::{ActiveTheme, Icon, IndexPath, h_flex, v_flex};

/// Cheap fingerprint so ChatView does not rebuild markdown on unrelated AppState
/// changes (sidebar toggle, theme, amplitude, etc.).
#[derive(Clone, PartialEq, Eq)]
struct ChatFeedRev {
    conversation_id: Option<String>,
    message_count: usize,
    last_id: Option<String>,
    last_len: usize,
    is_ai_responding: bool,
    debug_mode: bool,
    can_read_aloud: bool,
    theme_mode: String,
    native_speaking_id: Option<String>,
    native_paused: bool,
    native_loading: bool,
    ai_speaking_id: Option<String>,
    ai_paused: bool,
    ai_loading: bool,
    highlight: Option<(usize, usize)>,
}

impl ChatFeedRev {
    fn from_state(state: &AppState) -> Self {
        let conv = state.active_conversation_id.as_ref().and_then(|id| {
            state.conversations.iter().find(|c| &c.id == id)
        });
        let last = conv.and_then(|c| c.messages.last());
        let highlight = state
            .active_highlight_range()
            .map(|range| (range.start, range.end));
        Self {
            conversation_id: state.active_conversation_id.clone(),
            message_count: conv.map(|c| c.messages.len()).unwrap_or(0),
            last_id: last.map(|m| m.id.clone()),
            last_len: last.map(|m| m.content.len()).unwrap_or(0),
            is_ai_responding: state.is_ai_responding,
            debug_mode: state.debug_markdown_disabled,
            can_read_aloud: state
                .active_profile()
                .and_then(|p| p.tts_model_id.as_ref())
                .is_some(),
            theme_mode: state.theme_mode.clone(),
            native_speaking_id: state.native_tts.message_id.clone(),
            native_paused: state.native_tts.is_paused,
            native_loading: state.native_tts.is_loading,
            ai_speaking_id: state.ai_tts.message_id.clone(),
            ai_paused: state.ai_tts.is_paused,
            ai_loading: state.ai_tts.is_loading,
            highlight,
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
}

fn snapshot_rows(state: &AppState) -> Arc<Vec<ChatRow>> {
    let Some(conv) = state.active_conversation_id.as_ref().and_then(|id| {
        state.conversations.iter().find(|c| &c.id == id)
    }) else {
        return Arc::new(Vec::new());
    };
    let mut rows = Vec::new();
    for msg in &conv.messages {
        let is_native_speaking = state.native_tts.message_id.as_ref() == Some(&msg.id);
        let is_ai_speaking = state.ai_tts.message_id.as_ref() == Some(&msg.id);
        let full_highlight = if is_native_speaking {
            state
                .active_highlight_range()
                .and_then(|range| map_utf16_range_to_utf8(&msg.content, range))
        } else if is_ai_speaking {
            state.active_highlight_range()
        } else {
            None
        };
        let use_markdown = !msg.is_me && looks_like_markdown(&msg.content);
        let chunks = chunk_text(&msg.content, CHAT_ROW_CHUNK_BYTES);
        let last = chunks.len().saturating_sub(1);
        for (ix, chunk) in chunks.into_iter().enumerate() {
            let highlight_range = full_highlight.as_ref().and_then(|range| {
                highlight_in_chunk(range, chunk.byte_start, chunk.text.len())
            });
            let highlight_native = is_native_speaking && highlight_range.is_some();
            rows.push(ChatRow {
                id: if ix == 0 {
                    msg.id.clone()
                } else {
                    format!("{}:{ix}", msg.id)
                },
                content: SharedString::from(chunk.text),
                is_me: msg.is_me,
                timestamp: SharedString::from(msg.formatted_time()),
                is_native_speaking: is_native_speaking && ix == last,
                is_native_paused: state.native_tts.is_paused && is_native_speaking && ix == last,
                is_native_loading: state.native_tts.is_loading && is_native_speaking && ix == last,
                is_ai_speaking: is_ai_speaking && ix == last,
                is_ai_paused: state.ai_tts.is_paused && is_ai_speaking && ix == last,
                is_ai_loading: state.ai_tts.is_loading && is_ai_speaking && ix == last,
                is_cached: TtsService::is_cached(&msg.id),
                highlight_range,
                highlight_native,
                use_markdown,
                show_footer: ix == last,
                source_id: msg.id.clone(),
                tts_text: if ix == last {
                    SharedString::from(msg.content.clone())
                } else {
                    SharedString::default()
                },
            });
        }
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
    scroller: Entity<MessageScrollerState>,
    rows: Arc<Vec<ChatRow>>,
    feed_rev: ChatFeedRev,
    last_conversation_id: Option<String>,
    debug_mode: bool,
    can_read_aloud: bool,
    palette: ChatPalette,
    highlight_pump: bool,
}

impl ChatTranscript {
    fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
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
            if this.feed_rev.native_speaking_id.is_some()
                || this.feed_rev.ai_speaking_id.is_some()
            {
                this.start_highlight_pump(cx);
            }
            cx.notify();
        })
        .detach();

        let mut this = Self {
            app_state: state,
            scroller,
            rows,
            feed_rev,
            last_conversation_id,
            debug_mode,
            can_read_aloud,
            palette: ChatPalette::from_cx(cx),
            highlight_pump: false,
        };
        if this.feed_rev.native_speaking_id.is_some() || this.feed_rev.ai_speaking_id.is_some()
        {
            this.start_highlight_pump(cx);
        }
        this
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
                        let speaking = app.native_tts.message_id.is_some()
                            || app.ai_tts.message_id.is_some();
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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        let app_state = self.app_state.clone();
        let debug_mode = self.debug_mode;
        let can_read_aloud = self.can_read_aloud;
        let palette = self.palette.clone();
        MessageScroller::new("chat-messages", self.scroller.clone(), move |ix, _, _cx| {
            let Some(row) = rows.get(ix) else {
                return div().into_any_element();
            };
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
            let mut bubble = MessageBubble::new(row.content.to_string())
                .message_id(row.id.clone())
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
                .use_markdown(row.use_markdown)
                .show_footer(show_footer);
            if show_footer {
                bubble = bubble.copy_text(tts_text.clone()).on_read_aloud(move |_, cx| {
                    state_entity.update(cx, |state, cx| {
                        state.toggle_read_aloud(
                            source_id.clone(),
                            tts_text.clone(),
                            crate::actions::TtsSource::Native,
                            cx,
                        );
                    });
                });
            }
            bubble.into_any_element()
        })
        .pt(px(80.0))
        .with_jump_button_transition(Duration::ZERO)
    }
}

#[derive(Clone, PartialEq, Debug)]
struct ProfileItem {
    id: Option<i64>, // None for "Create New Profile", Some(id) for actual profiles
    name: String,
}

impl SelectItem for ProfileItem {
    type Value = Option<i64>;

    fn title(&self) -> SharedString {
        SharedString::from(self.name.clone())
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

pub struct ChatView {
    input: Entity<MessageInput>,
    state: Entity<AppState>,
    transcript: Entity<ChatTranscript>,
    profile_select: Entity<SelectState<SearchableVec<ProfileItem>>>,
    cached_profiles: Vec<ProfileItem>,
    should_focus_input: bool,
    profiles_need_sync: bool,
    selection_need_sync: bool,
    active_profile_id: Option<i64>,
    last_conversation_id: Option<String>,
    bot_status: Option<String>,
    coworker_name: Option<String>,
    coworker_id: Option<String>,
    coworker_shape: Option<String>,
    coworker_color: Option<String>,
}

impl ChatView {
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

        let (last_conversation_id, profile_items, initial_selection, active_profile_id) = {
            let app_state = state.read(cx);
            let last_conversation_id = app_state.active_conversation_id.clone();
            let active_profile_id = app_state.active_profile_id;
            let mut profile_items: Vec<ProfileItem> = app_state
                .db_profiles
                .iter()
                .map(|p| ProfileItem {
                    id: Some(p.id),
                    name: p.name.chars().take(30).collect::<String>(),
                })
                .collect();
            profile_items.push(ProfileItem {
                id: None,
                name: "Create New Profile...".to_string(),
            });
            let initial_selection = if let Some(active_id) = app_state.active_profile_id {
                profile_items
                    .iter()
                    .position(|p| p.id == Some(active_id))
                    .map(IndexPath::new)
            } else {
                None
            };
            (
                last_conversation_id,
                profile_items,
                initial_selection,
                active_profile_id,
            )
        };

        let transcript = cx.new(|cx| ChatTranscript::new(state.clone(), cx));
        let profile_items_vec = SearchableVec::new(profile_items.clone());
        let profile_select = cx.new(|cx| {
            SelectState::new(profile_items_vec, initial_selection, window, cx).searchable(true)
        });

        let this = Self {
            input,
            state: state.clone(),
            transcript,
            profile_select: profile_select.clone(),
            cached_profiles: profile_items,
            should_focus_input: false,
            profiles_need_sync: false,
            selection_need_sync: false,
            active_profile_id,
            last_conversation_id,
            bot_status: None,
            coworker_name: None,
            coworker_id: None,
            coworker_shape: None,
            coworker_color: None,
        };

        cx.observe(&state, |this: &mut Self, state, cx| {
            let current_id = state.read(cx).active_conversation_id.clone();
            if current_id != this.last_conversation_id {
                this.last_conversation_id = current_id;
                this.should_focus_input = true;
                cx.notify();
            }
        })
        .detach();

        // Subscribe to state changes to update cached values and notify only when relevant fields change
        cx.observe(&state, |this: &mut Self, state, cx| {
            let mut changed = false;
            let profiles;
            let active_id;

            {
                let state = state.read(cx);
                // Clone profiles to use after dropping state read lock
                profiles = state.db_profiles.clone();
                active_id = state.active_profile_id;
            }

            // Sync profiles
            let new_profile_items: Vec<ProfileItem> = profiles
                .iter()
                .map(|p| ProfileItem {
                    id: Some(p.id),
                    name: p.name.chars().take(30).collect::<String>(),
                })
                .collect();

            let cached_len = this.cached_profiles.len();
            let profiles_changed = if cached_len > 0 {
                let cached_real_profiles = &this.cached_profiles[0..cached_len - 1];
                if cached_real_profiles.len() != new_profile_items.len() {
                    true
                } else {
                    cached_real_profiles
                        .iter()
                        .zip(new_profile_items.iter())
                        .any(|(a, b)| a != b)
                }
            } else {
                true
            };

            if profiles_changed {
                let mut full_items = new_profile_items.clone();
                full_items.push(ProfileItem {
                    id: None,
                    name: "Create New Profile...".to_string(),
                });

                this.cached_profiles = full_items;
                this.profiles_need_sync = true;
                changed = true;
            }

            if this.active_profile_id != active_id {
                this.active_profile_id = active_id;
                this.selection_need_sync = true;
                changed = true;
            }

            if changed {
                cx.notify();
            }
        })
        .detach();

        // Subscribe to profile select events
        cx.subscribe(
            &profile_select,
            |this: &mut Self, _, event: &SelectEvent<SearchableVec<ProfileItem>>, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    if let Some(profile_id) = value {
                        // Selected an existing profile
                        this.state.update(cx, |state, cx| {
                            state.select_db_profile(*profile_id, cx);
                        });
                    } else {
                        // Selected "Create New Profile..."
                        this.state.update(cx, |state, cx| {
                            if !state.is_profile_settings_open {
                                state.toggle_profile_settings(cx);
                            }
                        });
                    }
                }
            },
        )
        .detach();

        cx.observe(&state, |this, app, cx| {
            let app = app.read(cx);
            let label = app.bot_status.clone();
            let coworker = app
                .active_coworker_id
                .as_ref()
                .and_then(|id| app.coworkers.iter().find(|c| &c.id == id));
            let coworker_name = coworker.map(|c| c.name.clone());
            let coworker_id = coworker.map(|c| c.id.clone());
            let coworker_shape = coworker.and_then(|c| c.avatar_shape.clone());
            let coworker_color = coworker.and_then(|c| c.avatar_color.clone());
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
            if changed {
                cx.notify();
            }
        })
        .detach();

        this
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        // Handle auto-focus
        if self.should_focus_input {
            self.should_focus_input = false;
            self.input.update(cx, |input, cx| {
                input.focus(window, cx);
            });
        }

        if self.profiles_need_sync {
            let items = SearchableVec::new(self.cached_profiles.clone());
            self.profile_select.update(cx, |select, cx| {
                select.set_items(items, window, cx);
            });
            self.profiles_need_sync = false;
        }

        if self.selection_need_sync {
            let active_id = self.active_profile_id;
            self.profile_select.update(cx, |select, cx| {
                select.set_selected_value(&active_id, window, cx);
            });
            self.selection_need_sync = false;
        }

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
                move |action: &crate::actions::RegenerateAudio,
                      _window: &mut Window,
                      cx: &mut App| {
                    state.update(cx, |state, cx| {
                        state.regenerate_audio(action.message_id.clone(), action.text.clone(), cx);
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
                        self.transcript
                            .clone()
                            .cached(StyleRefinement::default().absolute().size_full()),
                    )
                    .child(
                        // Header - Absolute positioned at top
                        h_flex()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(60.0))
                            .pt(px(20.0))
                            .pb_5()
                            .items_center()
                            .justify_between()
                            .px_4()
                            .bg(theme.background.opacity(0.9)) // Slight transparency for glass effect if desired, or solid
                            .child(
                                h_flex().gap_2().items_center().child(
                                    div()
                                        .id("header-coworker")
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .cursor_pointer()
                                        .on_mouse_down(MouseButton::Left, {
                                            let state = self.state.clone();
                                            move |_, _, cx| {
                                                state.update(cx, |state, cx| {
                                                    state.toggle_agent_settings(cx);
                                                });
                                            }
                                        })
                                        .when_some(self.coworker_id.clone(), |this, id| {
                                            this.child(
                                                PersonaMark::new(id)
                                                    .shape(self.coworker_shape.clone())
                                                    .color(self.coworker_color.clone())
                                                    .size(px(24.))
                                                    .dark(theme.is_dark()),
                                            )
                                        })
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(
                                                    self.coworker_name
                                                        .clone()
                                                        .unwrap_or_else(|| "Native Chat".into()),
                                                ),
                                        ),
                                ),
                            )
                            .child(
                                h_flex().gap_2().items_center().child(
                                    div()
                                        .id("header-monitor")
                                        .size(px(28.))
                                        .rounded(px(8.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                                        .on_mouse_down(MouseButton::Left, {
                                            let state = self.state.clone();
                                            move |_, _, cx| {
                                                state.update(cx, |state, cx| {
                                                    state.toggle_computer_pane(cx);
                                                });
                                            }
                                        })
                                        .child(
                                            Icon::default()
                                                .path("icons/monitor.svg")
                                                .size(px(16.)),
                                        ),
                                ),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .px_4()
                    .pb_4()
                    .gap_2()
                    .when_some(self.bot_status.clone(), |this, label| {
                        this.child(
                            h_flex()
                                .id("bot-status")
                                .gap_2()
                                .items_center()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(label),
                        )
                    })
                    .child(self.input.clone()),
            )
    }
}


