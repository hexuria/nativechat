use crate::components::chat::ChatView;
use crate::components::sidebar::SidebarView;
use gpui::*;
use gpui_component::h_flex;

use crate::state::AppState;

#[derive(Clone)]
pub struct Layout {
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
    state: Entity<AppState>,
}

impl Layout {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| SidebarView::new(state.clone(), cx));
        let chat = cx.new(|cx| ChatView::new(window, state.clone(), cx));
        Self {
            sidebar,
            chat,
            state,
        }
    }
}

impl Render for Layout {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h_full()
            .w_full()
            .children(if self.state.read(_cx).is_sidebar_open {
                Some(self.sidebar.clone())
            } else {
                None
            })
            .child(
                div()
                    .flex_grow()
                    .size_full() // Ensure it takes full height
                    .overflow_hidden() // Prevent layout expansion
                    .child(self.chat.clone()),
            )
    }
}
