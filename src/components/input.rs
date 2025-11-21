use gpui::*;
use gpui_component::{
    IconName,
    button::Button,
    input::{Input, InputState},
};

#[derive(Clone)]
pub struct MessageInput {
    input_state: Entity<InputState>,
}

impl MessageInput {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| InputState::new(window, cx).placeholder("Type a message..."));

        Self { input_state }
    }

    fn on_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input_state.read(cx).value();
        if !text.is_empty() {
            println!("Sending message: {}", text);
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
        }
    }
}

impl Render for MessageInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .gap_2()
            .w_full()
            .child(Input::new(&self.input_state).w_full())
            .child(
                Button::new("send_button")
                    .icon(IconName::ArrowRight)
                    .on_click(cx.listener(|this, _, window, cx| this.on_submit(window, cx))),
            )
    }
}
