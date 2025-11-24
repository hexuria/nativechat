use crate::components::chat_input::MessageInput;
use crate::components::message::MessageBubble;
use crate::state::AppState;
use gpui::*;
use ui::{
    ActiveTheme, StyledExt, avatar::Avatar, h_flex, label::Label, scroll::ScrollbarAxis, v_flex,
};

pub struct ChatView {
    input: Entity<MessageInput>,
    state: Entity<AppState>,
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

        cx.observe(&state, |_, _, cx| cx.notify()).detach();

        Self { input, state }
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
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
                        // Messages Area - Full width/height, scrollable
                        v_flex()
                            .size_full()
                            .scrollable(ScrollbarAxis::Vertical)
                            .pt(px(80.0)) // Padding top to clear the absolute header (60px header + 20px padding)
                            .pb_4()
                            .items_center() // Center the message content wrapper
                            .child(
                                // Message Content Wrapper - Max width constraint
                                div().w_full().max_w(px(800.0)).px_4().child(
                                    v_flex().gap_4().children(messages.into_iter().map(|msg| {
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
            .child(h_flex().flex_shrink_0().child(self.input.clone())) // Removed p_4 to avoid double padding
    }
}
