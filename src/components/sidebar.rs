use crate::state::AppState;
use gpui::*;
use gpui_component::{
    IconName, Side,
    button::ButtonVariants,
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
        let selected_profile = state.selected_profile.clone();

        div()
            .child(
                Sidebar::new(Side::Left)
                    .header(
                        SidebarHeader::new().child(
                            gpui_component::button::DropdownButton::new("profile-picker")
                                .button(
                                    gpui_component::button::Button::new("profile-btn")
                                        .label(
                                            selected_profile
                                                .map(|p| p.name)
                                                .unwrap_or_else(|| "Select Profile".to_string()),
                                        )
                                        .icon(IconName::User)
                                        .ghost()
                                        .w_full()
                                        .justify_between(),
                                )
                                .dropdown_menu(move |menu, _, _cx| {
                                    menu.menu("John Doe", Box::new(crate::actions::SelectProfile1))
                                        .menu(
                                            "Jane Smith",
                                            Box::new(crate::actions::SelectProfile2),
                                        )
                                }),
                        ),
                    )
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
                    ),
            )
            .on_action({
                let state = self.state.clone();
                move |_: &crate::actions::SelectProfile1, _, cx| {
                    state.update(cx, |state, cx| {
                        state.select_profile(1, cx);
                    });
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &crate::actions::SelectProfile2, _, cx| {
                    state.update(cx, |state, cx| {
                        state.select_profile(2, cx);
                    });
                }
            })
    }
}
