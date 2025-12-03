use crate::components::chat_input::MessageInput;
use crate::components::message::MessageBubble;
use crate::state::AppState;
use gpui::*;
use ui::{ActiveTheme, avatar::Avatar, h_flex, label::Label, v_flex};

pub struct ChatView {
    input: Entity<MessageInput>,
    state: Entity<AppState>,
    scroll_handle: ScrollHandle,
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

        Self {
            input,
            state,
            scroll_handle,
        }
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = self.state.read(cx);

        let active_conversation = state
            .active_conversation_id
            .and_then(|id| state.conversations.iter().find(|c| c.id == id));

        let messages = if let Some(conversation) = active_conversation {
            conversation.messages.clone()
        } else {
            vec![]
        };

        let title = active_conversation
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "Select a conversation".to_string());

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
                                            .is_me(msg.is_me)
                                            .bg_color(bg_color)
                                            .text_color(text_color)
                                            .timestamp(msg.formatted_time())
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
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(Avatar::new())
                                    .child(Label::new(title)),
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
