use crate::components::input::MessageInput;
use crate::components::message::{Message, MessageBubble};
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable, StyledExt, avatar::Avatar, label::Label, scroll::ScrollbarAxis,
};

#[derive(Clone)]
pub struct ChatView {
    messages: Vec<Message>,
    input: Entity<MessageInput>,
}

impl ChatView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| MessageInput::new(window, cx));

        // Dummy data
        let messages = vec![
            Message {
                id: 1,
                sender: "Alice Johnson".to_string(),
                content: "Hey! How are you doing?".to_string(),
                timestamp: "10:00 AM".to_string(),
                is_me: false,
            },
            Message {
                id: 2,
                sender: "Me".to_string(),
                content: "I'm good, thanks! Working on this new chat app.".to_string(),
                timestamp: "10:01 AM".to_string(),
                is_me: true,
            },
            Message {
                id: 3,
                sender: "Alice Johnson".to_string(),
                content: "That sounds cool! Is it using GPUI?".to_string(),
                timestamp: "10:02 AM".to_string(),
                is_me: false,
            },
            Message {
                id: 4,
                sender: "Me".to_string(),
                content: "Yes, it is! It's pretty fast.".to_string(),
                timestamp: "10:03 AM".to_string(),
                is_me: true,
            },
        ];
        Self { messages, input }
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .child(
                // Header
                div()
                    .flex()
                    .items_center()
                    .p_4()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .items_center()
                            .child(Avatar::new().name("Alice Johnson").small())
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(Label::new("Alice Johnson").font_semibold())
                                    .child(
                                        Label::new("Online").text_xs().text_color(theme.success),
                                    ),
                            ),
                    ),
            )
            .child(
                // Message List
                div().flex_grow().child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .p_4()
                        .children(self.messages.iter().map(|msg| {
                            let (bg_color, text_color) = if msg.is_me {
                                (theme.primary, theme.primary_foreground)
                            } else {
                                (theme.secondary, theme.secondary_foreground)
                            };
                            MessageBubble::new(msg.clone(), bg_color, text_color)
                        }))
                        .scrollable(ScrollbarAxis::Vertical),
                ),
            )
            // Input
            .child(
                div()
                    .p_4()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(self.input.clone()),
            )
    }
}
