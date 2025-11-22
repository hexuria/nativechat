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
                let bars = 6;
                let bar_width = px(6.0);
                let spacing = px(10.0);

                // Center the waveform
                // Total width = (bars - 1) * spacing + bar_width
                let total_width = spacing * (bars - 1) as f32 + bar_width;
                let start_x = bounds.origin.x + (bounds.size.width - total_width) / 2.0;
                let center_y = bounds.origin.y + bounds.size.height / 2.0;

                for i in 0..bars {
                    let offset = i as f32 * 0.35;
                    let wave = ((t + offset).sin() * 0.5 + 0.5) * amplitude;
                    let height = px(18.0 + wave * 36.0);

                    let x = start_x + spacing * i as f32;
                    let y = center_y - height / 2.0;

                    window.paint_quad(
                        fill(
                            Bounds::new(point(x, y), size(bar_width, height)),
                            foreground,
                        )
                        .corner_radii(px(3.0)),
                    );
                }
            },
        )
        .w_full()
        .h_full()
    }
}
