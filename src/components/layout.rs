use crate::components::chat::ChatView;
use crate::components::modals::{
    account_settings::AccountSettingsModal, profile_settings::ProfileSettingsModal,
};
use crate::components::sidebar::SidebarView;
use gpui::*;
use ui::h_flex;

use crate::state::AppState;

#[derive(Clone)]
pub struct Layout {
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
    account_settings_modal: Entity<AccountSettingsModal>,
    profile_settings_modal: Entity<ProfileSettingsModal>,
    state: Entity<AppState>,
}

impl Layout {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| SidebarView::new(state.clone(), cx));
        let chat = cx.new(|cx| ChatView::new(window, state.clone(), cx));
        let account_settings_modal =
            cx.new(|cx| AccountSettingsModal::new(window, state.clone(), cx));
        let profile_settings_modal =
            cx.new(|cx| ProfileSettingsModal::new(window, state.clone(), cx));

        Self {
            sidebar,
            chat,
            account_settings_modal,
            profile_settings_modal,
            state,
        }
    }
}

impl Render for Layout {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);

        div()
            .size_full()
            .child(
                h_flex()
                    .h_full()
                    .w_full()
                    .children(if state.is_sidebar_open {
                        Some(self.sidebar.clone())
                    } else {
                        None
                    })
                    .child(
                        div()
                            .flex_grow()
                            .size_full()
                            .overflow_hidden()
                            .child(self.chat.clone()),
                    ),
            )
            .children(if state.is_account_settings_open {
                Some(self.account_settings_modal.clone().into_any_element())
            } else {
                None
            })
            .children(if state.is_profile_settings_open {
                Some(self.profile_settings_modal.clone().into_any_element())
            } else {
                None
            })
    }
}
