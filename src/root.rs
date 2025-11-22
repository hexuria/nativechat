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
}

impl RootView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let layout = cx.new(|cx| Layout::new(window, state.clone(), cx));
        Self {
            layout,
            state,
            circular_viz: None,
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
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.layout.clone())
            // Voice Mode Modal Overlay
            .children(if is_voice_mode_open && viz.is_some() {
                Some(render_voice_mode_modal(app_state, viz.unwrap(), cx))
            } else {
                None
            })
            // Root overlay layers
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
