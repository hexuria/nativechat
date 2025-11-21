use crate::components::chat::ChatView;
use crate::components::sidebar::SidebarView;
use gpui::Entity;
use gpui::*;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};

#[derive(Clone)]
pub struct Layout {
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
    resizable_state: ResizableState,
}

impl Layout {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| SidebarView::new(cx));
        let chat = cx.new(|cx| ChatView::new(window, cx));
        let resizable_state = ResizableState::default();

        Self {
            sidebar,
            chat,
            resizable_state,
        }
    }
}

impl Render for Layout {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_resizable("main-layout")
            .child(
                resizable_panel()
                    .size(px(250.))
                    .size_range(px(200.)..px(400.))
                    .child(self.sidebar.clone()),
            )
            .child(resizable_panel().child(self.chat.clone()))
    }
}
