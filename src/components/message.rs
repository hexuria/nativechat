use gpui::*;
use gpui_component::label::Label;

// Helper for v_flex if not available, or use div().flex().flex_col()
fn v_flex() -> Div {
    div().flex().flex_col()
}

#[derive(Clone, Debug)]
pub struct Message {
    pub id: usize,
    pub sender: String,
    pub content: String,
    pub timestamp: String,
    pub is_me: bool,
}

#[derive(Clone)]
pub struct MessageBubble {
    pub message: Message,
}

impl MessageBubble {
    pub fn new(message: Message) -> Self {
        Self { message }
    }
}

impl IntoElement for MessageBubble {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let is_me = self.message.is_me;

        // We can't access cx.theme() directly in into_element easily without a context.
        // However, gpui components usually use `Styled` or `theme()` helper if available.
        // Or we can pass theme colors or use standard theme accessors if they are global/thread-local.
        // Actually, `IntoElement` doesn't take `cx`.
        // We might need to use `RenderOnce` or similar if we need context.
        // Or, we can use `div().bg(gpui::white())` etc.
        // But we want to use the active theme.
        // `gpui_component` might have a way.

        // Alternative: Make MessageBubble a function that takes `&App` or `&Window`? No.
        // Let's look at how other components do it.
        // Usually they implement `RenderOnce`.

        // For now, let's use a simple approach:
        // We will use `div()` and standard styling.
        // Accessing theme might be tricky without `cx`.
        // But `gpui::theme()` might be available? No.

        // Let's use `RenderOnce` trait if it exists in GPUI 0.2.
        // Or `impl Render for MessageBubble` is for Views.

        // Wait, `gpui::Element`'s `render` takes `&mut Window, &mut Context`.
        // But `IntoElement` converts to an Element.

        // Let's try to use `impl RenderOnce for MessageBubble`.
        // If `RenderOnce` is not available, we can use a helper function that takes `cx`.

        // Let's change MessageBubble to be a helper function for now to avoid trait complexity.
        // `pub fn message_bubble(message: Message, cx: &App) -> impl IntoElement`

        // Actually, `chat.rs` has `cx`.
        // So we can pass `cx` or `theme` to `MessageBubble::new`.

        // Let's update MessageBubble to store the theme colors? No, that's inefficient.

        // Let's try to use `div()` and assume we can style it.
        // But we need theme.

        // Let's go with: `impl IntoElement` and use hardcoded colors for now or try to find a way to get theme.
        // Actually, `gpui_component::ActiveTheme` is a trait on `Context`.

        // Let's use a functional component approach.
        // `pub fn render_message(message: &Message, cx: &AppContext) -> impl IntoElement`

        // But `chat.rs` calls it in `render`.

        // Let's just implement `IntoElement` and use default colors for now to fix the build,
        // and then improve styling.

        let align_class = if is_me {
            div().flex().justify_end()
        } else {
            div().flex().justify_start()
        };

        align_class.child(
            div()
                .p_3()
                .rounded_md()
                .bg(gpui::white()) // Placeholder
                .border_1()
                .border_color(gpui::black()) // Placeholder
                .max_w_3_4()
                .child(
                    v_flex()
                        .gap_1()
                        .child(Label::new(self.message.content.clone()))
                        .child(div().text_xs().child(self.message.timestamp.clone())),
                ),
        )
    }
}
