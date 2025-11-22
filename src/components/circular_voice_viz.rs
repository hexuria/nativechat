use crate::state::AppState;
use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;
use rand::Rng;
use std::f32::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const CIRCLE_RADIUS: f32 = 150.0;

struct ThemeConfig {
    primary: Hsla,
    secondary: Hsla,
    accent: Hsla,
    bg_color: Hsla,
    is_light: bool,
}

impl ThemeConfig {
    fn hud() -> Self {
        Self {
            primary: hsla(0.0, 0.0, 1.0, 1.0),   // #ffffff Pure White
            secondary: hsla(0.0, 0.0, 0.2, 1.0), // #333333 Dark Grey
            accent: hsla(0.0, 0.0, 0.53, 1.0),   // #888888 Mid Grey
            bg_color: hsla(0.0, 0.0, 0.0, 1.0),  // #000000 Pure Black
            is_light: false,
        }
    }

    fn light() -> Self {
        Self {
            primary: hsla(222.0 / 360.0, 0.47, 0.11, 1.0), // #0f172a Slate 900
            secondary: hsla(210.0 / 360.0, 0.16, 0.83, 1.0), // #cbd5e1 Slate 300
            accent: hsla(215.0 / 360.0, 0.16, 0.47, 1.0),  // #64748b Slate 500
            bg_color: hsla(0.0, 0.0, 1.0, 1.0),            // #ffffff White
            is_light: true,
        }
    }
}

pub struct CircularVoiceViz {
    amplitude: Arc<AtomicU32>,
    ai_amplitude: Arc<AtomicU32>,
    // Animation state
    smoothed_amplitude: f32,
    smoothed_ai_amplitude: f32,
    rotation: f32,
    velocity: f32,
}

impl CircularVoiceViz {
    pub fn new<P: 'static>(
        amplitude: Arc<AtomicU32>,
        ai_amplitude: Arc<AtomicU32>,
        _state: Entity<AppState>,
        cx: &mut Context<P>,
    ) -> Entity<Self> {
        cx.new(|cx| {
            // Start animation loop at 60fps
            cx.spawn(
                move |view: WeakEntity<CircularVoiceViz>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        loop {
                            // 60 FPS target
                            cx.background_executor()
                                .timer(Duration::from_millis(16))
                                .await;

                            let result = view.update(&mut cx, |this, cx| {
                                this.update_animation(cx);
                                cx.notify();
                            });

                            if result.is_err() {
                                // View dropped
                                break;
                            }
                        }
                    }
                },
            )
            .detach();

            Self {
                amplitude,
                ai_amplitude,
                smoothed_amplitude: 0.0,
                smoothed_ai_amplitude: 0.0,
                rotation: 0.0,
                velocity: 0.002,
            }
        })
    }

    fn update_animation(&mut self, cx: &mut Context<Self>) {
        // 1. Read Amplitude
        let raw_amplitude = f32::from_bits(self.amplitude.load(Ordering::Relaxed));
        let raw_ai_amplitude = f32::from_bits(self.ai_amplitude.load(Ordering::Relaxed));

        // Noise Gate: Ignore very low levels (background noise)
        const NOISE_GATE: f32 = 0.02; // 2% threshold

        let gated_amplitude = if raw_amplitude > NOISE_GATE {
            (raw_amplitude - NOISE_GATE) * 2.0 // Re-scale and boost slightly
        } else {
            0.0
        };

        let gated_ai_amplitude = if raw_ai_amplitude > NOISE_GATE {
            (raw_ai_amplitude - NOISE_GATE) * 2.0
        } else {
            0.0
        };

        // Clamp to 0.0 - 1.0
        let target_amplitude = gated_amplitude.clamp(0.0, 1.0);
        let target_ai_amplitude = gated_ai_amplitude.clamp(0.0, 1.0);

        // Smooth amplitude
        if target_amplitude > self.smoothed_amplitude {
            self.smoothed_amplitude = target_amplitude;
        } else {
            self.smoothed_amplitude += (target_amplitude - self.smoothed_amplitude) * 0.1;
        }

        if target_ai_amplitude > self.smoothed_ai_amplitude {
            self.smoothed_ai_amplitude = target_ai_amplitude;
        } else {
            self.smoothed_ai_amplitude += (target_ai_amplitude - self.smoothed_ai_amplitude) * 0.1;
        }

        // 2. Physics Engine
        const MIN_VELOCITY: f32 = 0.002;
        const MAX_VELOCITY: f32 = 0.15;

        // "Is Talking" threshold (combined)
        let is_talking = self.smoothed_amplitude > 0.05 || self.smoothed_ai_amplitude > 0.05;

        if is_talking {
            // Accelerate
            self.velocity = (self.velocity + 0.0005).min(MAX_VELOCITY);
        } else {
            // Friction
            self.velocity = (self.velocity * 0.98).max(MIN_VELOCITY);
        }

        // Apply velocity to rotation
        self.rotation += self.velocity;

        cx.notify();
    }
}

impl Render for CircularVoiceViz {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_mode = cx.theme().mode;
        let theme = if theme_mode.is_dark() {
            ThemeConfig::hud()
        } else {
            ThemeConfig::light()
        };

        let rotation = self.rotation;
        let velocity = self.velocity;
        let amplitude = self.smoothed_amplitude;
        let ai_amplitude = self.smoothed_ai_amplitude;

        // Calculate speed ratio for gauge (0.0 to 1.0)
        const MIN_VELOCITY: f32 = 0.002;
        const MAX_VELOCITY: f32 = 0.15;
        let speed_ratio =
            ((velocity - MIN_VELOCITY) / (MAX_VELOCITY - MIN_VELOCITY)).clamp(0.0, 1.0);

        // Combined volume for display (0-100)
        let _display_vol = ((amplitude + ai_amplitude) * 100.0).min(100.0) as u32;
        let _rpm = (speed_ratio * 10000.0) as u32;

        div()
            .size_full()
            .relative() // Needed for absolute children
            .bg(theme.bg_color)
            // Layer 1: Full Screen Canvas (Grid + Gauge)
            .child(
                canvas(
                    move |bounds, _, _| bounds,
                    move |bounds, _, window, _| {
                        let center = bounds.center();
                        let mut rng = rand::thread_rng();

                        // --- 1. Background Grid (Full Screen) ---
                        let grid_color =
                            theme
                                .secondary
                                .opacity(if theme.is_light { 0.5 } else { 0.2 });
                        let mut grid_path = PathBuilder::stroke(px(1.0));
                        let grid_step = 50.0;
                        let width = bounds.size.width;
                        let height = bounds.size.height;

                        // Draw vertical lines
                        let mut x = px(0.0);
                        while x < width {
                            grid_path.move_to(point(bounds.origin.x + x, bounds.origin.y));
                            grid_path.line_to(point(bounds.origin.x + x, bounds.origin.y + height));
                            x = x + px(grid_step);
                        }
                        // Draw horizontal lines
                        let mut y = px(0.0);
                        while y < height {
                            grid_path.move_to(point(bounds.origin.x, bounds.origin.y + y));
                            grid_path.line_to(point(bounds.origin.x + width, bounds.origin.y + y));
                            y = y + px(grid_step);
                        }
                        window.paint_path(grid_path.build().unwrap(), grid_color);

                        // --- 2. Outer Speedometer Gauge (Centered) ---
                        let gauge_radius = CIRCLE_RADIUS + 60.0;
                        let start_angle = PI * 0.75; // 135 deg
                        let end_angle = PI * 2.25; // 405 deg
                        let total_range = end_angle - start_angle;

                        // Track (Background)
                        let mut track_path =
                            PathBuilder::stroke(px(if theme.is_light { 8.0 } else { 4.0 }));
                        let segments = 60;
                        for i in 0..=segments {
                            let t = i as f32 / segments as f32;
                            let angle = start_angle + t * total_range;
                            let p = point(
                                center.x + px(angle.cos() * gauge_radius),
                                center.y + px(angle.sin() * gauge_radius),
                            );
                            if i == 0 {
                                track_path.move_to(p);
                            } else {
                                track_path.line_to(p);
                            }
                        }
                        window.paint_path(track_path.build().unwrap(), theme.secondary);

                        // Ticks
                        let mut ticks_path =
                            PathBuilder::stroke(px(if theme.is_light { 2.0 } else { 1.0 }));
                        for i in 0..=20 {
                            let t = i as f32 / 20.0;
                            let angle = start_angle + t * total_range;
                            let is_major = i % 5 == 0;
                            let tick_len = if is_major { 15.0 } else { 8.0 };

                            let r1 = gauge_radius - tick_len;
                            let r2 = gauge_radius + if is_major { 5.0 } else { 0.0 };

                            ticks_path.move_to(point(
                                center.x + px(angle.cos() * r1),
                                center.y + px(angle.sin() * r1),
                            ));
                            ticks_path.line_to(point(
                                center.x + px(angle.cos() * r2),
                                center.y + px(angle.sin() * r2),
                            ));
                        }
                        window.paint_path(ticks_path.build().unwrap(), theme.accent);

                        // Active Needle / Bar
                        if speed_ratio > 0.01 {
                            let current_fill_angle = start_angle + (total_range * speed_ratio);
                            let mut needle_path = PathBuilder::stroke(px(8.0));
                            let fill_segments = (segments as f32 * speed_ratio).ceil() as usize;

                            for i in 0..=fill_segments {
                                let t = i as f32 / segments as f32;
                                let angle = start_angle + t * total_range;
                                if angle > current_fill_angle {
                                    break;
                                }

                                let p = point(
                                    center.x + px(angle.cos() * gauge_radius),
                                    center.y + px(angle.sin() * gauge_radius),
                                );
                                if i == 0 {
                                    needle_path.move_to(p);
                                } else {
                                    needle_path.line_to(p);
                                }
                            }
                            // Add the final point exactly at current_fill_angle
                            let p_end = point(
                                center.x + px(current_fill_angle.cos() * gauge_radius),
                                center.y + px(current_fill_angle.sin() * gauge_radius),
                            );
                            needle_path.line_to(p_end);

                            window.paint_path(needle_path.build().unwrap(), theme.primary);
                        }

                        // --- 3. Rotating Inner Ring (Dotted) ---
                        let dash_radius = CIRCLE_RADIUS + 20.0;
                        let mut dash_path = PathBuilder::stroke(px(4.0));
                        let num_dashes = 24;
                        let ring_rotation = -rotation;

                        for i in 0..num_dashes {
                            let angle_start =
                                (i as f32 / num_dashes as f32) * 2.0 * PI + ring_rotation;
                            let angle_end = angle_start + (2.0 * PI / num_dashes as f32) * 0.5; // 50% fill

                            let seg_steps = 5;
                            for j in 0..=seg_steps {
                                let t = j as f32 / seg_steps as f32;
                                let a = angle_start + t * (angle_end - angle_start);
                                let p = point(
                                    center.x + px(a.cos() * dash_radius),
                                    center.y + px(a.sin() * dash_radius),
                                );
                                if j == 0 {
                                    dash_path.move_to(p);
                                } else {
                                    dash_path.line_to(p);
                                }
                            }
                        }
                        window.paint_path(dash_path.build().unwrap(), theme.accent);

                        // --- 4. Waveform Circle ---
                        let mut wave_path = PathBuilder::stroke(px(3.0));
                        let num_points = 100;
                        let base_radius = CIRCLE_RADIUS;
                        let wave_amp = (amplitude + ai_amplitude).min(1.0);
                        let mut first_p = point(px(0.0), px(0.0));

                        for i in 0..=num_points {
                            let angle =
                                (i as f32 / num_points as f32) * 2.0 * PI + (rotation * 0.5);
                            let noise = (rng.r#gen::<f32>() - 0.5) * 2.0;
                            let displacement = noise * 30.0 * wave_amp;
                            let r = base_radius + displacement;
                            let p = point(
                                center.x + px(angle.cos() * r),
                                center.y + px(angle.sin() * r),
                            );

                            if i == 0 {
                                first_p = p;
                                wave_path.move_to(p);
                            } else {
                                wave_path.line_to(p);
                            }
                        }
                        wave_path.line_to(first_p);
                        wave_path.close();

                        window.paint_path(wave_path.build().unwrap(), theme.primary);
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
            // Layer 2: Centered Text Overlays (kept near the gauge)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center(),
            )
    }
}
