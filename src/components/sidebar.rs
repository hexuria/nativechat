use crate::state::AppState;
use gpui::*;
use gpui_component::{
    IconName, Side,
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
};

pub struct SidebarView {
    state: Entity<AppState>,
}

impl SidebarView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for SidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let active_id = state.active_conversation_id;

        Sidebar::new(Side::Left)
            .header(SidebarHeader::new().child("NativeChat"))
            .child(
                SidebarGroup::new("Conversations").child(SidebarMenu::new().children(
                    state.conversations.iter().map(|c| {
                        let id = c.id;
                        SidebarMenuItem::new(c.title.clone())
                            .icon(IconName::ArrowRight)
                            .active(Some(id) == active_id)
                            .on_click({
                                let state = self.state.clone();
                                move |_, _, cx| {
                                    state.update(cx, |state, cx| {
                                        state.select_conversation(id, cx);
                                    });
                                }
                            })
                    }),
                )),
            )
    }
}
