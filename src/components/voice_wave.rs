use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AsyncApp, Bounds, Context, Entity, IntoElement, Render, WeakEntity, Window, canvas, fill,
    point, px, size,
};
use gpui_component::ActiveTheme;

pub struct VoiceWave {
    amplitude: Arc<AtomicU32>,
    history: VecDeque<f32>,
}

impl VoiceWave {
    pub fn new<P: 'static>(amplitude: Arc<AtomicU32>, cx: &mut Context<P>) -> Entity<Self> {
        cx.new(|cx| {
            cx.spawn(|view: WeakEntity<VoiceWave>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;

                        // Update history in the view
                        let _ = view.update(&mut cx, |this, cx| {
                            let current_amp =
                                f32::from_bits(this.amplitude.load(Ordering::Relaxed));

                            // Add new sample
                            this.history.push_front(current_amp);

                            // Keep history size fixed (e.g., 60 samples)
                            if this.history.len() > 60 {
                                this.history.pop_back();
                            }

                            cx.notify();
                        });
                    }
                }
            })
            .detach();

            Self {
                amplitude,
                history: VecDeque::with_capacity(60),
            }
        })
    }
}

impl Render for VoiceWave {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let foreground = theme.foreground;
        let muted_foreground = theme.muted_foreground;
        let history = self.history.clone();

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
                    // Get amplitude from history, default to 0.0 if not enough history yet
                    // History is stored newest first (push_front), so index 0 is the rightmost bar
                    let amp = history.get(i).copied().unwrap_or(0.0);

                    // Determine if "active" (speaking) or "idle" (silence)
                    // Threshold can be tuned. 0.01 is a reasonable noise floor.
                    let is_active = amp > 0.01;

                    let (height, color) = if is_active {
                        // Active: Taller bar, foreground color
                        // Scale amplitude for visibility
                        (px(12.0 + amp * 40.0), foreground)
                    } else {
                        // Idle: Small dot, muted color
                        (px(4.0), muted_foreground)
                    };

                    // Draw from right to left to simulate scrolling
                    // i=0 is newest (rightmost), i=59 is oldest (leftmost)
                    // We want newest on the right side.
                    let x = start_x + (bar_width + spacing) * (bars - 1 - i) as f32;
                    let y = center_y - height / 2.0;

                    window.paint_quad(
                        fill(Bounds::new(point(x, y), size(bar_width, height)), color)
                            .corner_radii(px(1.0)),
                    );
                }
            },
        )
        .w_full()
        .h_full()
    }
}
