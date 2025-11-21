use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable,
    button::Button,
    form::{field, v_form},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
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
                .placeholder("Type a message...")
                .multi_line()
                .auto_grow(1, 10)
                .clean_on_escape()
        });

        // Subscribe to input events to handle Enter key
        cx.subscribe_in(&input_state, window, |this, _state, event, window, cx| {
            match event {
                InputEvent::PressEnter { secondary } => {
                    if !secondary {
                        // Enter without Shift - submit the message
                        this.trigger_submit(window, cx);
                    }
                    // Shift+Enter is handled by the editor (newline)
                }
                _ => {}
            }
        })
        .detach();

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
        println!("Triggering submit...");
        let text = self.input_state.read(cx).value();
        if !text.trim().is_empty() {
            println!("Submitting message: {}", text);
            if let Some(handler) = &self.on_submit {
                (handler)(text.to_string(), cx);
            }
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
            // Focus is handled by the input state usually, or we might need to re-focus
        } else {
            println!("Message is empty, ignoring.");
        }
    }
}

impl Render for MessageInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .w_full()
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.border)
            .p_2()
            .child(
                v_form().w_full().child(
                    field().child(
                        h_flex()
                            .id("message-input")
                            .items_end()
                            .gap_2()
                            .child(div().flex_grow().child(Input::new(&self.input_state)))
                            .child(
                                Button::new("send")
                                    .icon(IconName::ArrowUp)
                                    .small()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.trigger_submit(window, cx);
                                    })),
                            ),
                    ),
                ),
            )
    }
}
