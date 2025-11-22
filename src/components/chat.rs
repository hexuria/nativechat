use crate::components::input::MessageInput;
use crate::components::message::MessageBubble;
use crate::state::AppState;
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, StyledExt,
    avatar::Avatar,
    button::{Button, ButtonVariants},
    h_flex,
    label::Label,
    scroll::ScrollbarAxis,
    v_flex,
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
                h_flex()
                    .h(px(60.0)) // Increased height for window controls
                    .pt(px(20.0)) // Top padding for "traffic lights"
                    .pb_5() // Bottom padding to separate from border
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .px_4()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Avatar::new())
                            .child(Label::new(title)),
                    )
                    .child(
                        Button::new("theme-toggle")
                            .icon(match self.state.read(cx).theme_mode.as_str() {
                                "light" => IconName::Moon,
                                "dark" => IconName::Settings,
                                _ => IconName::Sun,
                            })
                            .ghost()
                            .tooltip(match self.state.read(cx).theme_mode.as_str() {
                                "light" => "Switch to Dark theme",
                                "dark" => "Switch to System theme",
                                _ => "Switch to Light theme",
                            })
                            .on_click({
                                let app_state = self.state.clone();
                                cx.listener(move |_, _, _, cx| {
                                    app_state.update(cx, |state, cx| {
                                        state.toggle_theme(cx);
                                    });
                                })
                            }),
                    ),
            )
            .child(
                // Messages area - centered with max-width like ChatGPT
                h_flex()
                    .flex_grow()
                    .w_full()
                    .justify_center() // Center the content
                    .overflow_hidden()
                    .px_4() // Add horizontal padding to parent
                    .child(
                        div()
                            .w_full()
                            .max_w(px(800.0)) // Max width constraint on wrapper
                            .h_full()
                            .child(
                                v_flex()
                                    .size_full()
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
                    ),
            )
            .child(h_flex().flex_shrink_0().child(self.input.clone())) // Removed p_4 to avoid double padding
    }
}
