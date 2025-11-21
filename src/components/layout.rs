use crate::components::chat::ChatView;
use crate::components::sidebar::SidebarView;
use gpui::Entity;
use gpui::*;
use gpui_component::h_flex;

#[derive(Clone)]
pub struct Layout {
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
}

impl Layout {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| SidebarView::new(cx));
        let chat = cx.new(|cx| ChatView::new(window, cx));
        Self { sidebar, chat }
    }
}

impl Render for Layout {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .child(self.sidebar.clone())
            .child(div().flex_grow().child(self.chat.clone()))
    }
}
