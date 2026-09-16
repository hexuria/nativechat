use crate::actions::{
    About, ClearSearch, CloseBotFinder, CloseCommandPalette, CloseFind, CloseWindow, FindNext,
    FindPrev, FocusChatInput, Hide, HideOthers, Minimize, NavBack, NavForward, NewChat,
    OpenCommandPalette, OpenSettings, PickFinderItem, Search, ShowAll, ToggleAgentSettings,
    ToggleComputerPane, ToggleDebugMarkdown, ToggleFps, ToggleMiniSidebar, ToggleSidebar,
    ToggleTheme, Zoom,
};
use crate::components::layout::Layout;
use gpui_kit::prelude::*;
use gpui_kit::{InteractiveElement, *};

use crate::state::AppState;

use crate::components::circular_voice_viz::CircularVoiceViz;
use crate::components::voice_mode_modal::render_voice_mode_modal;
use gpui_kit::component::{ActiveTheme, Root, h_flex, v_flex};

/// The title bar the window paints for itself (the system one is transparent, see main.rs):
/// tall enough for the traffic lights, and the part the person drags the window by.
const TITLE_BAR_HEIGHT: f32 = 44.;
/// Past the traffic lights.
const TITLE_BAR_LEFT_PAD: f32 = 80.;

#[derive(Clone)]
pub struct RootView {
    layout: Entity<Layout>,
    state: Entity<AppState>,
    circular_viz: Option<Entity<CircularVoiceViz>>,
    pub focus_handle: FocusHandle,
    show_fps: bool,
    was_signed_in: bool,
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
            focus_handle,
            show_fps: true,
            was_signed_in: false,
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
                path, mode, target, ..
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

        let (is_voice_mode_open, amplitude, ai_amplitude, signed_in) = {
            let app_state = self.state.read(cx);
            (
                app_state.is_voice_mode_open,
                app_state.amplitude.clone(),
                app_state.ai_amplitude.clone(),
                app_state.is_signed_in(),
            )
        };
        if signed_in && !self.was_signed_in {
            click_away(window, &self.focus_handle, cx);
        }
        self.was_signed_in = signed_in;
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

        // The system title bar is transparent (see main.rs), so this strip IS the title bar:
        // the app's name past the traffic lights, in the app's colours, and the part the
        // person drags the window by.
        let theme = cx.theme().clone();
        let title_bar = h_flex()
            .id("main-window-header")
            .w_full()
            .h(px(TITLE_BAR_HEIGHT))
            .flex_shrink_0()
            .pl(px(TITLE_BAR_LEFT_PAD))
            .pr(px(12.))
            .items_center()
            .bg(theme.background)
            .text_color(theme.foreground)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("main-window-drag")
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("NativeChat")
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
            );

        div()
            .relative()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Root")
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // The title bar, then the sidebar, chat and right pane in what is left.
            .child(
                v_flex().size_full().child(title_bar).child(
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .child(self.layout.clone()),
                ),
            )
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &ToggleSidebar, window: &mut Window, cx: &mut App| {
                    click_away(window, &root_focus, cx);
                    state.update(cx, |state, cx| state.toggle_sidebar(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &ToggleMiniSidebar, window: &mut Window, cx: &mut App| {
                    click_away(window, &root_focus, cx);
                    state.update(cx, |state, cx| state.toggle_mini_sidebar(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &ToggleAgentSettings, window: &mut Window, cx: &mut App| {
                    click_away(window, &root_focus, cx);
                    state.update(cx, |state, cx| state.toggle_agent_settings(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &ToggleComputerPane, window: &mut Window, cx: &mut App| {
                    click_away(window, &root_focus, cx);
                    state.update(cx, |state, cx| state.toggle_computer_pane(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &OpenSettings, window: &mut Window, cx: &mut App| {
                    click_away(window, &root_focus, cx);
                    state.update(cx, |state, cx| state.toggle_app_settings(cx));
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
                let root_focus = self.focus_handle.clone();
                move |_: &NavBack, window: &mut Window, cx: &mut App| {
                    click_away(window, &root_focus, cx);
                    state.update(cx, |state, cx| state.nav_back(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &NavForward, window: &mut Window, cx: &mut App| {
                    click_away(window, &root_focus, cx);
                    state.update(cx, |state, cx| state.nav_forward(cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &NewChat, window: &mut Window, cx: &mut App| {
                    let open = state.read(cx).bot_finder_open;
                    click_away(window, &root_focus, cx);
                    if open {
                        state.update(cx, |state, cx| state.close_bot_finder(cx));
                    } else {
                        state.update(cx, |state, cx| state.open_bot_finder(cx));
                    }
                }
            })
            .on_action({
                let layout = self.layout.clone();
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |action: &PickFinderItem, window: &mut Window, cx: &mut App| {
                    layout.update(cx, |layout, cx| {
                        layout.pick_overlay_item(action.index, cx);
                    });
                    state.update(cx, |state, cx| {
                        state.close_bot_finder(cx);
                        state.close_command_palette(cx);
                    });
                    click_away(window, &root_focus, cx);
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &OpenCommandPalette, window: &mut Window, cx: &mut App| {
                    let open = state.read(cx).command_palette_open;
                    click_away(window, &root_focus, cx);
                    if open {
                        state.update(cx, |state, cx| state.close_command_palette(cx));
                    } else {
                        state.update(cx, |state, cx| state.open_command_palette(cx));
                    }
                }
            })
            .on_action({
                let state = self.state.clone();
                let layout = self.layout.clone();
                move |_: &Search, window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| {
                        state.close_bot_finder(cx);
                        state.close_command_palette(cx);
                    });
                    layout.update(cx, |layout, cx| {
                        layout.open_find_in_chat(window, cx);
                    });
                }
            })
            .on_action({
                let layout = self.layout.clone();
                move |_: &FindNext, _, cx: &mut App| {
                    layout.update(cx, |layout, cx| layout.find_next_in_chat(cx));
                }
            })
            .on_action({
                let layout = self.layout.clone();
                move |_: &FindPrev, _, cx: &mut App| {
                    layout.update(cx, |layout, cx| layout.find_prev_in_chat(cx));
                }
            })
            .on_action({
                let layout = self.layout.clone();
                move |_: &CloseFind, window: &mut Window, cx: &mut App| {
                    layout.update(cx, |layout, cx| layout.close_find_in_chat(window, cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                let layout = self.layout.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &FocusChatInput, window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| {
                        state.close_bot_finder(cx);
                        state.close_command_palette(cx);
                    });
                    focus_composer(&state, &layout, &root_focus, window, cx);
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &CloseBotFinder, window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| state.close_bot_finder(cx));
                    click_away(window, &root_focus, cx);
                }
            })
            .on_action({
                let state = self.state.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &CloseCommandPalette, window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| {
                        state.close_command_palette(cx);
                        state.close_hidden_bots(cx);
                    });
                    click_away(window, &root_focus, cx);
                }
            })
            .on_action({
                let layout = self.layout.clone();
                let root_focus = self.focus_handle.clone();
                move |_: &ClearSearch, window: &mut Window, cx: &mut App| {
                    layout.update(cx, |layout, cx| {
                        layout.clear_sidebar_search(window, cx);
                    });
                    click_away(window, &root_focus, cx);
                }
            })
            .on_action(|_: &Minimize, window: &mut Window, _cx: &mut App| {
                window.minimize_window();
            })
            .on_action(|_: &CloseWindow, window: &mut Window, _cx: &mut App| {
                // The main window has nowhere to go but the Dock.
                window.minimize_window();
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
            // Root overlay layers
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            .when(self.show_fps, |this| {
                this.child(gpui_fps::fps_monitor(window, cx))
            })
    }
}

/// Release whatever field had the caret (same as clicking empty chrome) and
/// park focus on Root so the next chrome shortcut still has a target.
fn click_away(window: &mut Window, root_focus: &FocusHandle, cx: &mut App) {
    window.blur(cx);
    root_focus.focus(window, cx);
}

fn focus_composer(
    state: &Entity<AppState>,
    layout: &Entity<Layout>,
    root_focus: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) {
    let has_agent = state.update(cx, |state, cx| {
        state.ensure_active_coworker(cx);
        state.active_coworker_id.is_some()
    });
    if has_agent {
        layout.update(cx, |layout, cx| {
            layout.focus_chat_input(window, cx);
        });
    } else {
        root_focus.focus(window, cx);
    }
}
