use gpui::AppContext;
use gpui::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

pub struct VoiceWave {
    amplitude: Arc<AtomicU32>,
    t: f32,
}

impl VoiceWave {
    pub fn new<P>(cx: &mut Context<P>, amplitude: Arc<AtomicU32>) -> Entity<Self> {
        cx.new(|cx| {
            // Animation loop
            cx.spawn(|view: WeakEntity<VoiceWave>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                        // Update view state
                        let _ = view.update(&mut cx, |this, cx| {
                            this.t += 0.08;
                            cx.notify();
                        });
                    }
                }
            })
            .detach();

            Self { amplitude, t: 0.0 }
        })
    }
}

use gpui_component::ActiveTheme;

impl Render for VoiceWave {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.t;
        let amplitude = f32::from_bits(self.amplitude.load(Ordering::Relaxed));
        let theme = cx.theme();
        let foreground = theme.foreground;

        canvas(
            move |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let bars = 60;
                let spacing = px(3.0); // Space between bars
                let bar_width = px(2.0); // Width of each bar

                // Total width of the wave
                let total_width = (bar_width + spacing) * bars as f32;

                // Center the waveform horizontally
                let start_x = bounds.origin.x + (bounds.size.width - total_width) / 2.0;
                let center_y = bounds.origin.y + bounds.size.height / 2.0;

                for i in 0..bars {
                    // Normalized position (0.0 to 1.0)
                    let normalized_i = i as f32 / bars as f32;

                    // Window function (Hanning-like) to taper edges
                    // sin(pi * x)^2
                    let window_val = (std::f32::consts::PI * normalized_i).sin().powi(2);

                    // Scrolling phase
                    let phase = t * 2.0;

                    // Organic wave composition
                    // Base wave + faster ripples
                    let wave = ((normalized_i * 10.0 + phase).sin()) * 0.5
                        + ((normalized_i * 23.0 - phase * 1.5).sin()) * 0.3
                        + ((normalized_i * 47.0 + phase * 0.5).sin()) * 0.2;

                    // Combine amplitude, wave, and window
                    // Base height + dynamic height
                    // Idle state: amplitude is low, but we still want some movement
                    let effective_amp = amplitude.max(0.1);
                    let height_val = 4.0 + (wave * 0.5 + 0.5) * effective_amp * 24.0;

                    // Apply windowing to height
                    let height = px(height_val * window_val);

                    let x = start_x + (bar_width + spacing) * i as f32;
                    let y = center_y - height / 2.0;

                    window.paint_quad(
                        fill(
                            Bounds::new(point(x, y), size(bar_width, height)),
                            foreground,
                        )
                        .corner_radii(px(1.0)),
                    );
                }
            },
        )
        .w_full()
        .h_full()
    }
}
