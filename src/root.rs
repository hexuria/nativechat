use crate::components::layout::Layout;
use gpui::*;

use crate::state::AppState;

use crate::components::voice_mode_modal::render_voice_mode_modal;
use crate::components::voice_wave::VoiceWave;
use gpui_component::{ActiveTheme, Root};

#[derive(Clone)]
pub struct RootView {
    layout: Entity<Layout>,
    state: Entity<AppState>,
    voice_wave: Option<Entity<VoiceWave>>,
}

impl RootView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let layout = cx.new(|cx| Layout::new(window, state.clone(), cx));
        Self {
            layout,
            state,
            voice_wave: None,
        }
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let is_voice_mode_open = state.is_voice_mode_open;
        let amplitude = state.amplitude.clone();
        let app_state = self.state.clone();

        // Manage VoiceWave lifecycle
        if is_voice_mode_open {
            if self.voice_wave.is_none() {
                self.voice_wave = Some(VoiceWave::new(amplitude, app_state.clone(), cx));
            }
        } else {
            self.voice_wave = None;
        }

        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.layout.clone())
            // Voice Mode Modal Overlay
            .children(if is_voice_mode_open {
                if let Some(voice_wave) = self.voice_wave.clone() {
                    Some(render_voice_mode_modal(app_state, voice_wave, cx))
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
