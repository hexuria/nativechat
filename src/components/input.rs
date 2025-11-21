use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
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
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            println!("Submitting message: {}", trimmed);
            if let Some(handler) = &self.on_submit {
                (handler)(trimmed.to_string(), cx);
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

        // ChatGPT-style: centered container with max-width
        h_flex().w_full().justify_center().p_4().child(
            // Input container - rounded pill shape with shadow
            h_flex()
                .max_w(px(800.0)) // Max width like ChatGPT
                .w_full()
                .items_center()
                .gap_2()
                .px_4()
                .py_3()
                .bg(theme.secondary)
                .border_1()
                .border_color(theme.border)
                .rounded(px(26.0)) // Rounded pill shape
                .shadow_sm()
                // Plus icon on the left
                .child(Button::new("attach").icon(IconName::Plus).ghost().small())
                // Input field (grows to fill space)
                .child(
                    div()
                        .flex_grow()
                        .child(Input::new(&self.input_state).appearance(false)),
                )
                // Right side icons
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        // Microphone icon
                        .child(
                            Button::new("voice")
                                .icon(IconName::Settings)
                                .ghost()
                                .small(),
                        )
                        // Send button
                        .child(
                            Button::new("send")
                                .icon(IconName::ArrowUp)
                                .small()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.trigger_submit(window, cx);
                                })),
                        ),
                ),
        )
    }
}
