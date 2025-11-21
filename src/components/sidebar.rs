use gpui::*;
use gpui_component::{
    IconName, Side, StyledExt,
    sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
};

#[derive(Clone)]
pub struct SidebarView {
    // State can be added here
}

impl SidebarView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Render for SidebarView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Sidebar::new(Side::Left)
            .header(SidebarHeader::new().child(div().p_2().font_bold().child("Messages")))
            .child(
                SidebarGroup::new("Conversations").child(
                    SidebarMenu::new()
                        .child(
                            SidebarMenuItem::new("Alice Johnson")
                                .icon(IconName::User)
                                .active(true),
                        )
                        .child(SidebarMenuItem::new("Bob Smith").icon(IconName::User))
                        .child(SidebarMenuItem::new("Team Chat").icon(IconName::User)),
                ),
            )
    }
}
