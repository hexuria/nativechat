use crate::actions::{
    About, Hide, HideOthers, Minimize, ShowAll, ToggleSidebar, ToggleTheme, Zoom,
};
use crate::components::layout::Layout;
use gpui::*;

use crate::state::AppState;

use crate::components::circular_voice_viz::CircularVoiceViz;
use crate::components::voice_mode_modal::render_voice_mode_modal;
use gpui_component::{ActiveTheme, Root};

#[derive(Clone)]
pub struct RootView {
    layout: Entity<Layout>,
    state: Entity<AppState>,
    circular_viz: Option<Entity<CircularVoiceViz>>,
    pub focus_handle: FocusHandle,
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
        }
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let is_voice_mode_open = state.is_voice_mode_open;
        let amplitude = state.amplitude.clone();
        let ai_amplitude = state.ai_amplitude.clone();
        let app_state = self.state.clone();

        // Manage CircularVoiceViz lifecycle
        if is_voice_mode_open {
            if self.circular_viz.is_none() {
                self.circular_viz = Some(CircularVoiceViz::new(
                    amplitude,
                    ai_amplitude,
                    app_state.clone(),
                    cx,
                ));
            }
        } else {
            self.circular_viz = None;
        }

        let viz = self.circular_viz.clone();

        div()
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
            .on_action(|_: &Minimize, _window: &mut Window, _cx: &mut App| {
                // cx.minimize_window(); // Not available on App
                println!("Minimize action triggered");
            })
            .on_action(|_: &Zoom, _window: &mut Window, _cx: &mut App| {
                // cx.zoom_window(); // Not available on App
                println!("Zoom action triggered");
            })
            .on_action(|_: &Hide, _window: &mut Window, cx: &mut App| {
                cx.hide();
            })
            .on_action(|_: &HideOthers, _window: &mut Window, _cx: &mut App| {
                // cx.hide_others();
                println!("Hide Others action triggered");
            })
            .on_action(|_: &ShowAll, _window: &mut Window, _cx: &mut App| {
                // cx.show_all();
                println!("Show All action triggered");
            })
            .on_action(|_: &About, _window: &mut Window, _cx: &mut App| {
                println!("About NativeChat");
            })
            // Voice Mode Modal Overlay
            .children(if is_voice_mode_open {
                if let Some(viz) = viz {
                    Some(render_voice_mode_modal(app_state, viz, cx))
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
    }
}
