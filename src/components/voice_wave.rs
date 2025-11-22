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
    scroll_phase: f32,
    current_peak: f32,
    animation_offset: f32,
}

impl VoiceWave {
    pub fn new<P: 'static>(amplitude: Arc<AtomicU32>, cx: &mut Context<P>) -> Entity<Self> {
        cx.new(|cx| {
            cx.spawn(|view: WeakEntity<VoiceWave>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    loop {
                        // Run at 60fps (approx 16ms) for smooth animation
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;

                        // Update view state
                        if view
                            .update(&mut cx, |this, cx| {
                                let current_amp =
                                    f32::from_bits(this.amplitude.load(Ordering::Relaxed));

                                // Peak sampling: capture the highest amplitude since the last bar push
                                if current_amp > this.current_peak {
                                    this.current_peak = current_amp;
                                }

                                // Scroll speed in pixels per frame
                                // 2.0px per 16ms = ~120px per second (Faster, smoother scroll)
                                let speed = 2.0;
                                this.scroll_phase += speed;

                                // Animation speed for the "living" effect
                                this.animation_offset += 0.8;

                                let bar_width = 2.0;
                                let spacing = 3.0;
                                let stride = bar_width + spacing;

                                // When we've scrolled a full bar's width, push the peak to history
                                if this.scroll_phase >= stride {
                                    this.history.push_front(this.current_peak);
                                    this.current_peak = 0.0; // Reset peak for next bar
                                    this.scroll_phase -= stride; // Keep remainder for smooth continuity

                                    // Keep history size large enough
                                    if this.history.len() > 300 {
                                        this.history.pop_back();
                                    }
                                }

                                cx.notify();
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            })
            .detach();

            Self {
                amplitude,
                history: VecDeque::with_capacity(300),
                scroll_phase: 0.0,
                current_peak: 0.0,
                animation_offset: 0.0,
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
        let scroll_phase = self.scroll_phase;
        let animation_offset = self.animation_offset;

        canvas(
            move |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let spacing = px(3.0); // Space between bars
                let bar_width = px(2.0); // Width of each bar
                let stride = bar_width + spacing;

                // Calculate how many bars fit in the available width
                // Add 1 extra bar to cover the scrolling edge
                let bars = (bounds.size.width / stride).ceil() as usize + 1;

                // Center the waveform horizontally
                // We shift everything left by `scroll_phase` to create smooth motion
                let total_width = stride * bars as f32;
                let start_x = bounds.origin.x + (bounds.size.width - total_width) / 2.0;
                let center_y = bounds.origin.y + bounds.size.height / 2.0;
                let max_height = bounds.size.height - px(4.0); // Leave some padding

                for i in 0..bars {
                    // i=0 is the rightmost bar (Newest)

                    let (height, color) = if i < history.len() {
                        // Recorded History (Past) -> ALWAYS Black (foreground)
                        let amp = history[i];

                        // Determine if "active" (speaking) or "idle" (silence)
                        // Increased threshold to 0.05 to filter background noise
                        let is_active = amp > 0.05;

                        if is_active {
                            // Active Speech: Taller bar, foreground color
                            // Reduced scaling from 100.0 to 50.0 to prevent maxing out too easily
                            let scaled_height = px(2.0 + amp * 50.0);

                            // Apply "Living" Animation: Sine wave modulation
                            // i varies across bars, animation_offset varies over time
                            // This creates a ripple effect
                            let modulation =
                                1.0 + 0.3 * ((i as f32 * 0.2) + animation_offset).sin();
                            let modulated_height = scaled_height * modulation;

                            // Clamp between 2px and max_height
                            let final_height = if modulated_height > max_height {
                                max_height
                            } else {
                                modulated_height
                            };

                            (final_height, foreground)
                        } else {
                            // Silence in History: Small box/dot, foreground color
                            // User requested "black small box" for silence in recorded track
                            // No animation for silence
                            (px(2.0), foreground)
                        }
                    } else {
                        // Unrecorded Future -> ALWAYS Grey (muted_foreground)
                        // Fixed height of 2px (Circle/Dot)
                        (px(2.0), muted_foreground)
                    };

                    // Draw from Right to Left
                    // Position 0 is at the far right
                    // Apply scroll_phase to shift bars left smoothly
                    let x = start_x + stride * (bars - 1 - i) as f32 - px(scroll_phase);
                    let y = center_y - height / 2.0;

                    // Only draw if within bounds (optional optimization, canvas clips anyway)
                    window.paint_quad(
                        fill(Bounds::new(point(x, y), size(bar_width, height)), color)
                            .corner_radii(px(1.0)),
                    );
                }
            },
        )
        .w_full()
        .h(px(36.0))
    }
}
