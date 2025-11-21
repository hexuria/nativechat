use crate::components::layout::Layout;
use gpui::Entity;
use gpui::*;
use gpui_component::{ActiveTheme, Root};

#[derive(Clone)]
pub struct RootView {
    layout: Entity<Layout>,
}

impl RootView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let layout = cx.new(|cx| Layout::new(window, cx));
        Self { layout }
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.theme().background) // Use theme background
            .child(self.layout.clone())
            // Root overlay layers
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
