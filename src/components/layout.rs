use crate::components::agent_settings::AgentSettings;
use crate::components::chat::ChatView;
use crate::components::login::LoginView;
use crate::components::modals::{
    account_settings::AccountSettingsModal, profile_settings::ProfileSettingsModal,
};
use crate::components::sidebar::SidebarView;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::component::{resizable::h_resizable, resizable::resizable_panel};

use crate::state::AppState;

fn cached_fill<V: Render>(view: Entity<V>) -> impl IntoElement {
    view.cached(StyleRefinement::default().absolute().size_full())
}

#[derive(Clone, PartialEq, Eq)]
struct ShellRev {
    collapsed: bool,
    auto_collapsed: bool,
    account: bool,
    profile: bool,
    signed_in: bool,
    signing_in: bool,
    auth_error: Option<String>,
    agent_settings: bool,
}

impl ShellRev {
    fn from_state(state: &AppState) -> Self {
        Self {
            collapsed: state.sidebar_collapsed,
            auto_collapsed: state.auto_collapsed,
            account: state.is_account_settings_open,
            profile: state.is_profile_settings_open,
            signed_in: state.is_signed_in(),
            signing_in: state.auth_status == crate::state::AuthStatus::SigningIn,
            auth_error: state.auth_error.clone(),
            agent_settings: state.is_agent_settings_open,
        }
    }
}

#[derive(Clone)]
pub struct Layout {
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
    login: Entity<LoginView>,
    agent_settings: Entity<AgentSettings>,
    account_settings_modal: Entity<AccountSettingsModal>,
    profile_settings_modal: Entity<ProfileSettingsModal>,
    state: Entity<AppState>,
    shell: ShellRev,
    last_window_width: Option<Pixels>,
}

impl Layout {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| SidebarView::new(state.clone(), cx));
        let chat = cx.new(|cx| ChatView::new(window, state.clone(), cx));
        let login = cx.new(|cx| LoginView::new(window, state.clone(), cx));
        let agent_settings = cx.new(|cx| AgentSettings::new(window, state.clone(), cx));
        let account_settings_modal =
            cx.new(|cx| AccountSettingsModal::new(window, state.clone(), cx));
        let profile_settings_modal =
            cx.new(|cx| ProfileSettingsModal::new(window, state.clone(), cx));
        let shell = ShellRev::from_state(&state.read(cx));

        cx.observe(&state, |this, state, cx| {
            let shell = ShellRev::from_state(&state.read(cx));
            if this.shell != shell {
                this.shell = shell;
                cx.notify();
            }
        })
        .detach();

        Self {
            sidebar,
            chat,
            login,
            agent_settings,
            account_settings_modal,
            profile_settings_modal,
            state,
            shell,
            last_window_width: None,
        }
    }
}

impl Render for Layout {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (sidebar_collapsed, auto_collapsed) = {
            let state = self.state.read(cx);
            (state.sidebar_collapsed, state.auto_collapsed)
        };
        let window_width = window.viewport_size().width;
        if self.last_window_width != Some(window_width) {
            self.last_window_width = Some(window_width);
            if window_width < px(800.0) {
                if !sidebar_collapsed {
                    let state_entity = self.state.clone();
                    cx.defer(move |cx| {
                        state_entity.update(cx, |state, cx| {
                            state.set_sidebar_collapsed(true, true, cx);
                        });
                    });
                }
            } else if sidebar_collapsed && auto_collapsed {
                let state_entity = self.state.clone();
                cx.defer(move |cx| {
                    state_entity.update(cx, |state, cx| {
                        state.set_sidebar_collapsed(false, false, cx);
                    });
                });
            }
        }

        let state = self.state.read(cx);
        if !state.is_signed_in() {
            return div().size_full().child(self.login.clone());
        }

        div()
            .size_full()
            .relative()
            .when(state.sidebar_collapsed, |this| {
                this.child(
                    div()
                        .size_full()
                        .flex()
                        .child(
                            div()
                                .id("sidebar-slot")
                                .w(px(64.))
                                .flex_shrink_0()
                                .relative()
                                .child(cached_fill(self.sidebar.clone())),
                        )
                        .child(
                            div()
                                .size_full()
                                .flex_grow(1.)
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
                                .child(
                                    div()
                                        .id("sidebar-slot")
                                        .size_full()
                                        .relative()
                                        .child(cached_fill(self.sidebar.clone())),
                                ),
                        )
                        .on_resize({
                            let state = self.state.clone();
                            move |resizable_state, _, cx| {
                                let sizes = resizable_state.read(cx).sizes();
                                if let Some(sidebar_width) = sizes.get(0) {
                                    if *sidebar_width < px(180.0) {
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
            .when(state.is_agent_settings_open, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .child(self.agent_settings.clone()),
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
