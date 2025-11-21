use crate::components::input::MessageInput;
use crate::components::message::MessageBubble;
use crate::state::AppState;
use gpui::*;
use gpui_component::{
    ActiveTheme, StyledExt, avatar::Avatar, h_flex, label::Label, scroll::ScrollbarAxis, v_flex,
};

pub struct ChatView {
    input: Entity<MessageInput>,
    state: Entity<AppState>,
}

impl ChatView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            MessageInput::new(window, cx).on_submit({
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
                h_flex()
                    .h(px(60.0)) // Increased height for window controls
                    .pt(px(20.0)) // Top padding for "traffic lights"
                    .flex_shrink_0()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.border)
                    .px_4()
                    .gap_2()
                    .child(Avatar::new())
                    .child(Label::new(title)),
            )
            .child(
                v_flex()
                    .flex_grow()
                    .overflow_hidden() // Ensure scrollable area is contained
                    .child(
                        v_flex()
                            .size_full()
                            .p_4()
                            .gap_4()
                            .scrollable(ScrollbarAxis::Vertical)
                            .children(messages.into_iter().map(|msg| {
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
                h_flex()
                    .flex_shrink_0() // Ensure footer doesn't shrink
                    .p_4()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(self.input.clone()),
            )
    }
}
