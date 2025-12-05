use crate::components::chat_input::MessageInput;
use crate::components::message::MessageBubble;
use crate::state::AppState;
use gpui::*;
use ui::select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState};
use ui::{ActiveTheme, IndexPath, h_flex, v_flex};

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
    scroll_handle: ScrollHandle,
    profile_select: Entity<SelectState<SearchableVec<ProfileItem>>>,
    cached_profiles: Vec<ProfileItem>,
    should_focus_input: bool,
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

        let scroll_handle = ScrollHandle::new();

        // Initialize profile select items
        let app_state = state.read(cx);
        let mut profile_items: Vec<ProfileItem> = app_state
            .db_profiles
            .iter()
            .map(|p| ProfileItem {
                id: Some(p.id),
                name: p.name.chars().take(30).collect::<String>(),
            })
            .collect();

        // Add "Create New Profile" option
        profile_items.push(ProfileItem {
            id: None,
            name: "Create New Profile...".to_string(),
        });

        let profile_items_vec = SearchableVec::new(profile_items.clone());

        // Determine initial selection
        let initial_selection = if let Some(active_id) = app_state.active_profile_id {
            profile_items_vec
                .items()
                .iter()
                .position(|p| p.id == Some(active_id))
                .map(|ix| IndexPath::default().row(ix))
        } else {
            None
        };

        let profile_select = cx.new(|cx| {
            SelectState::new(profile_items_vec, initial_selection, window, cx).searchable(true)
        });

        let this = Self {
            input,
            state: state.clone(),
            scroll_handle: scroll_handle.clone(),
            profile_select: profile_select.clone(),
            cached_profiles: profile_items,
            should_focus_input: false,
        };

        cx.observe(&state, {
            let scroll_handle = scroll_handle.clone();
            move |_, state, cx| {
                let state = state.read(cx);
                if state.is_ai_responding {
                    // Scroll to bottom during streaming
                    scroll_handle.scroll_to_bottom();
                }
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

                this.cached_profiles = full_items.clone();
                let profile_items_vec = SearchableVec::new(full_items);
                this.profile_select.update(cx, |select, cx| {
                    select.set_items(profile_items_vec, cx);
                });
                changed = true;
            }

            // Sync active profile selection
            let current_index = this.profile_select.read(cx).selected_index(cx);
            let current_profile = current_index.and_then(|ix| this.cached_profiles.get(ix.row));

            let expected_selection = if let Some(id) = active_id {
                profiles.iter().find(|p| p.id == id).map(|p| ProfileItem {
                    id: Some(p.id),
                    name: p.name.chars().take(30).collect::<String>(),
                })
            } else {
                None
            };

            if let Some(expected) = &expected_selection {
                if current_profile != Some(expected) {
                    if let Some(index) = this.cached_profiles.iter().position(|p| p == expected) {
                        this.profile_select.update(cx, |select, cx| {
                            select.set_selected_index_deferred(
                                Some(IndexPath::default().row(index)),
                                cx,
                            );
                        });
                        changed = true;
                    }
                }
            } else if current_profile.is_some() {
                this.profile_select.update(cx, |select, cx| {
                    select.set_selected_index_deferred(None, cx);
                });
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

        // Focus input when conversation changes (especially on new chat creation)
        cx.observe(&state, {
            let mut last_conversation_id: Option<String> =
                state.read(cx).active_conversation_id.clone();

            move |this: &mut Self, state, cx| {
                let state = state.read(cx);
                let current_id = state.active_conversation_id.clone();

                // If conversation changed, trigger focus on next render
                if current_id != last_conversation_id {
                    last_conversation_id = current_id;
                    this.should_focus_input = true;
                    cx.notify();
                }
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

        // Sync active profile selection if needed
        let active_id = self.state.read(cx).active_profile_id;
        let current_selection = self
            .profile_select
            .read(cx)
            .selected_value()
            .cloned()
            .flatten();
        if active_id != current_selection {
            self.profile_select.update(cx, |select, cx| {
                select.set_selected_value(&active_id, window, cx);
            });
        }

        let state = self.state.read(cx);
        let active_conversation = state
            .active_conversation_id
            .as_ref()
            .and_then(|id| state.conversations.iter().find(|c| &c.id == id));

        let messages = if let Some(conversation) = active_conversation {
            conversation.messages.clone()
        } else {
            vec![]
        };

        let debug_mode = state.debug_markdown_disabled;

        v_flex()
            .size_full()
            .bg(theme.background)
            .child(
                // Main Content Area (Header + Messages)
                div()
                    .flex_grow()
                    .min_h(px(0.0)) // Ensure it can shrink/scroll properly
                    .relative()
                    .child(
                        // Messages Area - simple scrollable list (testing)
                        div()
                            .id("chat-scroll-container")
                            .track_scroll(&self.scroll_handle)
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .overflow_y_scroll()
                            .px_4()
                            .child(
                                v_flex()
                                    .w_full()
                                    .max_w(px(800.0))
                                    .mx_auto()
                                    .pt(px(80.0))
                                    .pb(px(20.0))
                                    .gap_4()
                                    .children(messages.iter().map(|msg| {
                                        let (bg_color, text_color) = if msg.is_me {
                                            (theme.primary, theme.primary_foreground)
                                        } else {
                                            (theme.secondary, theme.secondary_foreground)
                                        };

                                        MessageBubble::new(msg.content.clone())
                                            .message_id(msg.id.clone())
                                            .is_me(msg.is_me)
                                            .bg_color(bg_color)
                                            .text_color(text_color)
                                            .timestamp(msg.formatted_time())
                                            .debug_mode(debug_mode)
                                    })),
                            ),
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
                                    div().w(px(200.0)).child(
                                        Select::new(&self.profile_select)
                                            .id("profile-select")
                                            .placeholder("Select Profile")
                                            .search_placeholder("Search profile...")
                                            .anchor(Corner::TopLeft)
                                            .w_full(),
                                    ),
                                ),
                            )
                            .child(
                                h_flex().gap_2().items_center(), // Add other header actions here if needed
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .px_4()
                    .pb_4()
                    .child(self.input.clone()),
            )
    }
}
