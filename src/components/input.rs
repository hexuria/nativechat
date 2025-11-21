use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable,
    button::Button,
    h_flex,
    input::{Input, InputState},
};

actions!(chat, [SubmitMessage]);

pub struct MessageInput {
    input_state: Entity<InputState>,
    on_submit: Option<Box<dyn Fn(String, &mut Context<Self>)>>,
}

impl MessageInput {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Message NativeChat")
                .multi_line()
                .auto_grow(1, 10) // Min 1 row, max 10 rows
        });

        Self {
            input_state,
            on_submit: None,
        }
    }

    pub fn on_submit(mut self, handler: impl Fn(String, &mut Context<Self>) + 'static) -> Self {
        self.on_submit = Some(Box::new(handler));
        self
    }

    fn trigger_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).value();
        if !text.trim().is_empty() {
            if let Some(handler) = &self.on_submit {
                (handler)(text.to_string(), cx);
            }
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
        }
    }
}

impl Render for MessageInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .w_full()
            .bg(theme.secondary)
            .rounded_xl() // Rounded corners
            .border_1()
            .border_color(theme.border)
            .p_2()
            .child(
                h_flex()
                    .items_end() // Align input and button to bottom
                    .gap_2()
                    .on_action(cx.listener(|this, _: &SubmitMessage, window, cx| {
                        this.trigger_submit(window, cx);
                    }))
                    .child(
                        Input::new(&self.input_state).appearance(false), // Remove default input styling
                    )
                    .child(
                        Button::new("send")
                            .icon(IconName::ArrowUp)
                            .small()
                            .rounded_md() // Button styling
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.trigger_submit(window, cx);
                            })),
                    ),
            )
    }
}
