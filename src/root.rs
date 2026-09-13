use crate::actions::{
    About, Hide, HideOthers, Minimize, NewChat, ShowAll, ToggleDebugMarkdown, ToggleFps,
    ToggleSidebar, ToggleTheme, Zoom,
};
use crate::components::layout::Layout;
use gpui_kit::prelude::*;
use gpui_kit::{InteractiveElement, *};

use crate::state::AppState;

use crate::components::circular_voice_viz::CircularVoiceViz;
use crate::components::modals::credentials_modal::CredentialsModal;
use crate::components::modals::profile_settings::ProfileSettingsModal;
use crate::components::voice_mode_modal::render_voice_mode_modal;
use gpui_kit::component::{ActiveTheme, Root};

#[derive(Clone)]
pub struct RootView {
    layout: Entity<Layout>,
    state: Entity<AppState>,
    circular_viz: Option<Entity<CircularVoiceViz>>,
    credentials_modal: Option<Entity<CredentialsModal>>,
    profile_settings_modal: Option<Entity<ProfileSettingsModal>>,
    pub focus_handle: FocusHandle,
    show_fps: bool,
    #[cfg(feature = "agent")]
    mailbox: Option<crate::agent::AgentMailbox>,
}

impl RootView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let layout = cx.new(|cx| Layout::new(window, state.clone(), cx));
        let focus_handle = cx.focus_handle();

        Self {
            layout,
            state,
            circular_viz: None,
            credentials_modal: None,
            profile_settings_modal: None,
            focus_handle,
            show_fps: true,
            #[cfg(feature = "agent")]
            mailbox: None,
        }
    }

    #[cfg(feature = "agent")]
    pub fn attach_agent(
        mut self,
        mailbox: Option<crate::agent::AgentMailbox>,
        cx: &mut Context<Self>,
    ) -> Self {
        let poll = mailbox.clone();
        if poll.is_some() {
            // Do not lease RootView on the empty-mailbox poll. `update` every
            // 16ms dirties the window and rebuilds the chat (~30fps cap).
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(16))
                        .await;
                    let pending = poll.as_ref().is_some_and(|mailbox| !mailbox.is_empty());
                    if pending {
                        if this.update(cx, |_, cx| cx.notify()).is_err() {
                            break;
                        }
                    } else if this.upgrade().is_none() {
                        break;
                    }
                }
            })
            .detach();
        }
        self.mailbox = mailbox;
        self
    }

    #[cfg(feature = "agent")]
    fn drain_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mailbox) = self.mailbox.clone() else {
            return;
        };
        for posted in mailbox.take() {
            if let gpui_agent::Op::Screenshot {
                path,
                mode,
                target,
                ..
            } = &posted.request.op
            {
                let response = if mode.is_scrolled() {
                    gpui_agent::Response::err(
                        &posted.request.id,
                        gpui_agent::screenshot_unavailable(format!(
                            "scrolled screenshot is not wired (target={})",
                            target.as_deref().unwrap_or("?")
                        )),
                    )
                } else {
                    match crate::agent::screenshot_this_window(window, path.as_deref()) {
                        Ok(result) => {
                            let mut resp = gpui_agent::Response::ok(&posted.request.id);
                            resp.result = result.value;
                            resp
                        }
                        Err(error) => gpui_agent::Response::err(&posted.request.id, error),
                    }
                };
                posted.reply(response);
                cx.notify();
                continue;
            }

            if posted.request.op.is_virtual_input() {
                let id = posted.request.id.clone();
                posted.reply(gpui_agent::Response::err(
                    id,
                    gpui_agent::virtual_unavailable("NativeChat agent host is semantic-only"),
                ));
                continue;
            }

            let shutdown = matches!(posted.request.op, gpui_agent::Op::Shutdown);
            let mut host = crate::agent::NativeChatHost::from_app(self.state.read(cx));
            let response =
                gpui_agent::handle_request(&mut host, posted.request.clone(), None, None);
            if let Some(cmd) = host.take_command() {
                let quit = matches!(cmd, crate::agent::Command::Shutdown);
                self.state.update(cx, |state, cx| cmd.apply(state, cx));
                if quit {
                    cx.quit();
                }
            }
            posted.reply(response);
            if shutdown {
                cx.quit();
            }
            cx.notify();
        }
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "agent")]
        self.drain_agent(window, cx);

        let (
            is_voice_mode_open,
            amplitude,
            ai_amplitude,
            is_credentials_modal_open,
            is_profile_settings_open,
        ) = {
            let app_state = self.state.read(cx);
            (
                app_state.is_voice_mode_open,
                app_state.amplitude.clone(),
                app_state.ai_amplitude.clone(),
                app_state.is_credentials_modal_open,
                app_state.is_profile_settings_open,
            )
        };
        let app_state_entity = self.state.clone();

        // Manage CircularVoiceViz lifecycle
        if is_voice_mode_open {
            if self.circular_viz.is_none() {
                self.circular_viz = Some(CircularVoiceViz::new(
                    amplitude,
                    ai_amplitude,
                    app_state_entity.clone(),
                    cx,
                ));
            }
        } else {
            self.circular_viz = None;
        }

        let viz = self.circular_viz.clone();

        div()
            .relative()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Root")
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.layout.clone())
            .on_action({
                let state = self.state.clone();
                move |_: &ToggleSidebar, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| state.toggle_sidebar(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &ToggleTheme, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| state.toggle_theme(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &NewChat, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| state.create_new_session(cx));
                }
            })
            .on_action(|_: &Minimize, _window: &mut Window, _cx: &mut App| {
                println!("Minimize action triggered");
            })
            .on_action(|_: &Zoom, _window: &mut Window, _cx: &mut App| {
                println!("Zoom action triggered");
            })
            .on_action(|_: &Hide, _window: &mut Window, cx: &mut App| {
                cx.hide();
            })
            .on_action(|_: &HideOthers, _window: &mut Window, _cx: &mut App| {
                println!("Hide Others action triggered");
            })
            .on_action(|_: &ShowAll, _window: &mut Window, _cx: &mut App| {
                println!("Show All action triggered");
            })
            .on_action(|_: &About, _window: &mut Window, _cx: &mut App| {
                println!("About NativeChat");
            })
            .on_action({
                let state = self.state.clone();
                move |_: &crate::actions::ToggleCredentialsModal,
                      _window: &mut Window,
                      cx: &mut App| {
                    state.update(cx, |state, cx| state.toggle_credentials_modal(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &ToggleDebugMarkdown, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| state.toggle_debug_markdown(cx));
                }
            })
            .on_action(cx.listener(|this, _: &ToggleFps, _, cx| {
                this.show_fps = !this.show_fps;
                cx.notify();
            }))
            // Voice Mode Modal Overlay
            .children(if is_voice_mode_open {
                if let Some(viz) = viz {
                    Some(render_voice_mode_modal(app_state_entity.clone(), viz, cx))
                } else {
                    None
                }
            } else {
                None
            })
            // Credentials Modal
            .children(if is_credentials_modal_open {
                if self.credentials_modal.is_none() {
                    self.credentials_modal =
                        Some(CredentialsModal::new(app_state_entity.clone(), window, cx));
                }
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .bg(cx.theme().background.opacity(0.8))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .w_4_5()
                                .h_4_5()
                                .bg(cx.theme().background)
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded_lg()
                                .shadow_lg()
                                .child(self.credentials_modal.clone().unwrap())
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                        ),
                )
            } else {
                self.credentials_modal = None;
                None
            })
            // Profile Settings Modal
            .children(if is_profile_settings_open {
                if self.profile_settings_modal.is_none() {
                    self.profile_settings_modal =
                        Some(cx.new(|cx| {
                            ProfileSettingsModal::new(window, app_state_entity.clone(), cx)
                        }));
                }
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .bg(cx.theme().background.opacity(0.8))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .w_4_5()
                                .h_4_5()
                                .bg(cx.theme().background)
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded_lg()
                                .shadow_lg()
                                .child(self.profile_settings_modal.clone().unwrap())
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                        ),
                )
            } else {
                self.profile_settings_modal = None;
                None
            })
            // Root overlay layers
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            .when(self.show_fps, |this| {
                this.child(gpui_fps::fps_monitor(window, cx))
            })
    }
}
