use crate::components::chat::ChatView;
use crate::components::modals::{
    account_settings::AccountSettingsModal, profile_settings::ProfileSettingsModal,
};
use crate::components::sidebar::SidebarView;
use gpui::prelude::FluentBuilder;
use gpui::*;
use ui::{PixelsExt, resizable::h_resizable, resizable::resizable_panel};

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
            .when(state.sidebar_collapsed, |this| {
                this.child(
                    div()
                        .size_full()
                        .flex()
                        .child(div().w(px(64.)).flex_shrink_0().child(self.sidebar.clone()))
                        .child(
                            div()
                                .size_full()
                                .flex_grow()
                                .overflow_hidden()
                                .child(self.chat.clone()),
                        ),
                )
            })
            .when(!state.sidebar_collapsed, |this| {
                this.child(
                    h_resizable("main-layout")
                        .child(
                            resizable_panel()
                                .size(px(280.))
                                .size_range(px(0.)..px(700.))
                                .child(self.sidebar.clone()),
                        )
                        .on_resize({
                            let state = self.state.clone();
                            move |resizable_state, _, cx| {
                                let sizes = resizable_state.read(cx).sizes();
                                if let Some(sidebar_width) = sizes.get(0) {
                                    if sidebar_width.as_f32() < 180.0 {
                                        state.update(cx, |state, cx| {
                                            if !state.sidebar_collapsed {
                                                state.toggle_sidebar(cx);
                                            }
                                        });
                                    }
                                }
                            }
                        })
                        .child(resizable_panel().child(self.chat.clone())),
                )
            })
            .children(
                state
                    .is_account_settings_open
                    .then(|| self.account_settings_modal.clone().into_any_element()),
            )
            .children(
                state
                    .is_profile_settings_open
                    .then(|| self.profile_settings_modal.clone().into_any_element()),
            )
    }
}
