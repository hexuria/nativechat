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
use crate::components::lightbox::LightboxView;
use crate::components::voice_mode_modal::render_voice_mode_modal;
use gpui_kit::component::{ActiveTheme, Root};

pub struct RootView {
    layout: Entity<Layout>,
    state: Entity<AppState>,
    /// The picture overlay is mounted here, not in the layout, because it covers the title
    /// bar and the panes alike.
    lightbox: Entity<LightboxView>,
    circular_viz: Option<Entity<CircularVoiceViz>>,
    pub focus_handle: FocusHandle,
    show_fps: bool,
    was_signed_in: bool,
    #[cfg(feature = "agent")]
    mailbox: Option<crate::agent::AgentMailbox>,
    /// Requests taken off the mailbox and not answered yet, because the keys of the op before
    /// them are still going in. They are held here rather than left in the mailbox because the
    /// mailbox is drained whole.
    #[cfg(feature = "agent")]
    agent_backlog: std::collections::VecDeque<gpui_agent::mailbox::MailboxRequest>,
    /// Keys a driver asked for, in the order they were asked for.
    #[cfg(feature = "agent")]
    agent_keys: std::collections::VecDeque<crate::agent::ComposePlan>,
    /// The caret has just been put in the composer and the frame that paints it there has not
    /// been drawn yet, so the keys wait for it.
    #[cfg(feature = "agent")]
    agent_focused: bool,
    /// Keys are going in right now, just after a frame. Pressing one can draw the window, and
    /// that draw renders this view again, so this is what keeps the second pass from answering
    /// the ops that are waiting on those keys.
    #[cfg(feature = "agent")]
    agent_pressing: bool,
}

impl RootView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let layout = cx.new(|cx| Layout::new(window, state.clone(), cx));
        let lightbox = cx.new(|cx| LightboxView::new(state.clone(), cx));
        let focus_handle = cx.focus_handle();
        // This is the window the pages are drawn in; a coworker's screen window hands its
        // "Recipes" over to it rather than drawing a page of its own.
        state.update(cx, |state, _| {
            state.set_main_window(window.window_handle());
        });

        Self {
            layout,
            state,
            lightbox,
            circular_viz: None,
            focus_handle,
            show_fps: true,
            was_signed_in: false,
            #[cfg(feature = "agent")]
            mailbox: None,
            #[cfg(feature = "agent")]
            agent_backlog: std::collections::VecDeque::new(),
            #[cfg(feature = "agent")]
            agent_keys: std::collections::VecDeque::new(),
            #[cfg(feature = "agent")]
            agent_focused: false,
            #[cfg(feature = "agent")]
            agent_pressing: false,
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

    /// Press the keys a driver asked for. Returns whether the composer is still owed some, in
    /// which case no other op may be answered yet.
    ///
    /// Two things are true of a keystroke and neither is obvious. It reaches a field down the
    /// dispatch tree of the last frame that was *painted*, and that same paint is what installs
    /// the field's input handler — so the caret has to be in the composer for one whole frame
    /// before anything typed into it can land, or the driver is told a key went in that went
    /// nowhere. And GPUI draws the window before dispatching a key whenever something has
    /// changed since that frame, which from inside this render would be a draw within a draw:
    /// the keys are therefore pressed just after this frame rather than in it.
    #[cfg(feature = "agent")]
    fn type_for_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(plan) = self.agent_keys.front() else {
            return false;
        };
        if plan.focus_composer && !self.agent_focused {
            self.agent_focused = true;
            let state = self.state.clone();
            let layout = self.layout.clone();
            let root_focus = self.focus_handle.clone();
            focus_composer(&state, &layout, &root_focus, window, cx);
            return true;
        }
        let Some(plan) = self.agent_keys.pop_front() else {
            return false;
        };
        self.agent_focused = false;
        self.agent_pressing = true;
        let this = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            for key in &plan.keys {
                match Keystroke::parse(key) {
                    // The call a real key arrives by. GPUI takes it down the focus path, so the
                    // composer's own capture listener gets `/` and `@` first and opens its
                    // panel, and everything else reaches the field as text.
                    Ok(keystroke) => {
                        window.dispatch_keystroke(keystroke, cx);
                    }
                    // Every key here came out of the host's own table, so one GPUI will not
                    // spell is a bug in that table rather than something the driver said.
                    Err(err) => eprintln!("agent: gpui would not parse the key `{key}`: {err}"),
                }
            }
            this.update(cx, |this, cx| {
                this.agent_pressing = false;
                cx.notify();
            })
            .ok();
        });
        true
    }

    #[cfg(feature = "agent")]
    fn drain_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mailbox) = self.mailbox.clone() else {
            return;
        };
        self.agent_backlog.extend(mailbox.take());
        // Keys go before anything else is answered. A snapshot taken between two of them would
        // show a half-typed draft, and one taken between the focus and the first key would show
        // a panel that is about to open as shut. The flag is read first because pressing a key
        // can draw the window, and that draw comes back through here.
        if self.agent_pressing || self.type_for_agent(window, cx) {
            cx.notify();
            return;
        }
        while let Some(posted) = self.agent_backlog.pop_front() {
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
                    gpui_agent::virtual_unavailable(
                        "virtual delivery is not wired: a semantic type or key already goes in \
                         as a GPUI keystroke, and a virtual click would need pointer synthesis \
                         this host does not do. Use delivery=semantic.",
                    ),
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
            if let Some(plan) = host.take_compose() {
                self.agent_keys.push_back(plan);
            }
            posted.reply(response);
            if shutdown {
                cx.quit();
            }
            cx.notify();
            // The rest of the backlog waits for those keys: the op after a `type` is nearly
            // always the one asking what the typing did.
            if !self.agent_keys.is_empty() {
                break;
            }
        }
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "agent")]
        self.drain_agent(window, cx);

        let (is_voice_mode_open, amplitude, ai_amplitude, signed_in, lightbox_open) = {
            let app_state = self.state.read(cx);
            (
                app_state.is_voice_mode_open,
                app_state.amplitude.clone(),
                app_state.ai_amplitude.clone(),
                app_state.is_signed_in(),
                app_state.lightbox.is_some(),
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

        div()
            .relative()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Root")
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // The title bar the app paints (see components/title_bar.rs), then the sidebar,
            // chat and right pane in what is left: the layout lays out both.
            .child(self.layout.clone())
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
            // A picture from the feed, over the whole window. It is mounted before the root
            // layers so that the note saying where a download landed still lands on top of it.
            .when(lightbox_open, |this| this.child(self.lightbox.clone()))
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
