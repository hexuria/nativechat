use crate::state::AppState;
use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;
use std::f32::consts::PI;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const CIRCLE_RADIUS: f32 = 150.0;

struct ThemeConfig {
    primary: Hsla,
    secondary: Hsla,
    accent: Hsla,
    is_light: bool,
}

impl ThemeConfig {
    fn hud() -> Self {
        Self {
            primary: hsla(0.0, 0.0, 1.0, 1.0),   // #ffffff Pure White
            secondary: hsla(0.0, 0.0, 0.2, 1.0), // #333333 Dark Grey
            accent: hsla(0.0, 0.0, 0.53, 1.0),   // #888888 Mid Grey
            is_light: false,
        }
    }

    fn light() -> Self {
        Self {
            primary: hsla(222.0 / 360.0, 0.47, 0.11, 1.0), // #0f172a Slate 900
            secondary: hsla(210.0 / 360.0, 0.16, 0.83, 1.0), // #cbd5e1 Slate 300
            accent: hsla(215.0 / 360.0, 0.16, 0.47, 1.0),  // #64748b Slate 500
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
            // Gentler re-scale to preserve dynamics
            (raw_amplitude - NOISE_GATE) * 1.2
        } else {
            0.0
        };

        let gated_ai_amplitude = if raw_ai_amplitude > NOISE_GATE {
            (raw_ai_amplitude - NOISE_GATE) * 1.2
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
            // Layer 1: Full Screen Canvas (Grid + Gauge + Gradient Background)
            .child(
                canvas(
                    move |bounds, _, _| bounds,
                    move |bounds, _, window, _| {
                        let center = bounds.center();

                        // --- 0. Radial Gradient Background ---
                        // Paint concentric circles to simulate radial gradient
                        let max_width: f32 = bounds.size.width.into();
                        let max_height: f32 = bounds.size.height.into();
                        // Reference uses a large radial gradient.
                        // In CSS: radial-gradient(circle, rgba(20,20,20,1) 0%, rgba(0,0,0,1) 100%)
                        // This implies a very soft, wide gradient.
                        let max_radius_f32 = (max_width.max(max_height) / 2.0) * 1.5;
                        let num_gradient_steps = 100;

                        for i in 0..num_gradient_steps {
                            let t = i as f32 / num_gradient_steps as f32;
                            let radius_f32 = max_radius_f32 * (1.0 - t);

                            // Use cubic easing for a more natural, softer falloff than quadratic
                            let eased_t = 1.0 - (1.0 - t).powf(3.0);

                            let gradient_color = if theme.is_light {
                                // Light theme: White (100%) to Very Light Grey (96%)
                                let lightness = 1.0 - (eased_t * 0.04);
                                gpui::hsla(210.0 / 360.0, 0.2, lightness, 1.0)
                            } else {
                                // Dark theme: Dark Grey (8%) to Pure Black (0%)
                                // 20/255 = ~0.08
                                let lightness = (1.0 - eased_t) * 0.08;
                                gpui::hsla(0.0, 0.0, lightness, 1.0)
                            };

                            // Draw filled circle
                            let mut circle_path = PathBuilder::fill();
                            let segments = 60;
                            for j in 0..=segments {
                                let angle = (j as f32 / segments as f32) * 2.0 * PI;
                                let p = point(
                                    center.x + px(angle.cos() * radius_f32),
                                    center.y + px(angle.sin() * radius_f32),
                                );
                                if j == 0 {
                                    circle_path.move_to(p);
                                } else {
                                    circle_path.line_to(p);
                                }
                            }
                            circle_path.close();
                            window.paint_path(circle_path.build().unwrap(), gradient_color);
                        }

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
                            x += px(grid_step);
                        }
                        // Draw horizontal lines
                        let mut y = px(0.0);
                        while y < height {
                            grid_path.move_to(point(bounds.origin.x, bounds.origin.y + y));
                            grid_path.line_to(point(bounds.origin.x + width, bounds.origin.y + y));
                            y += px(grid_step);
                        }
                        window.paint_path(grid_path.build().unwrap(), grid_color);

                        // --- 2. Outer Speedometer Gauge (Centered) ---
                        let gauge_radius = CIRCLE_RADIUS + 60.0;
                        let start_angle = PI * 0.75; // 135 degrees
                        let end_angle = PI * 2.25; // 405 degrees
                        let total_range = end_angle - start_angle;

                        // Arc drawing helper
                        let draw_arc = |path: &mut PathBuilder, r: f32, start: f32, end: f32| {
                            let segments = 60;
                            for i in 0..=segments {
                                let t = i as f32 / segments as f32;
                                let angle = start + (end - start) * t;
                                let p = point(
                                    center.x + px(angle.cos() * r),
                                    center.y + px(angle.sin() * r),
                                );
                                if i == 0 {
                                    path.move_to(p);
                                } else {
                                    path.line_to(p);
                                }
                            }
                        };

                        // Gauge Track (Background)
                        let mut track_path =
                            PathBuilder::stroke(px(if theme.is_light { 12.0 } else { 4.0 }));
                        draw_arc(&mut track_path, gauge_radius, start_angle, end_angle);
                        let track_color = if theme.is_light {
                            gpui::hsla(210.0 / 360.0, 0.16, 0.83, 0.4) // Faint slate
                        } else {
                            theme.secondary
                        };
                        window.paint_path(track_path.build().unwrap(), track_color);

                        // Gauge Ticks
                        let tick_color = if theme.is_light {
                            theme.primary
                        } else {
                            theme.accent
                        };
                        let tick_width = if theme.is_light { 1.5 } else { 1.0 };
                        let mut ticks_path = PathBuilder::stroke(px(tick_width));

                        for i in 0..=20 {
                            let r_ratio = i as f32 / 20.0;
                            let angle = start_angle + (total_range * r_ratio);
                            let is_major = i % 5 == 0;
                            let tick_len = if is_major { 15.0 } else { 8.0 };

                            let r1 =
                                gauge_radius - tick_len + if theme.is_light { -5.0 } else { 0.0 };
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
                        window.paint_path(ticks_path.build().unwrap(), tick_color);

                        // Active Needle / Bar
                        let current_fill_angle = start_angle + (total_range * speed_ratio);

                        if speed_ratio > 0.01 {
                            // GLOW EFFECT (Multi-layered for soft blur simulation)
                            // Layer 1: Wide, very transparent
                            let glow_width_1 = if theme.is_light { 24.0 } else { 20.0 };
                            let glow_color_1 = if theme.is_light {
                                gpui::hsla(215.0 / 360.0, 0.16, 0.47, 0.1)
                            } else {
                                theme.primary.opacity(0.1)
                            };
                            let mut glow_path_1 = PathBuilder::stroke(px(glow_width_1));
                            draw_arc(
                                &mut glow_path_1,
                                gauge_radius,
                                start_angle,
                                current_fill_angle,
                            );
                            window.paint_path(glow_path_1.build().unwrap(), glow_color_1);

                            // Layer 2: Medium, semi-transparent
                            let glow_width_2 = if theme.is_light { 16.0 } else { 12.0 };
                            let glow_color_2 = if theme.is_light {
                                gpui::hsla(215.0 / 360.0, 0.16, 0.47, 0.2)
                            } else {
                                theme.primary.opacity(0.2)
                            };
                            let mut glow_path_2 = PathBuilder::stroke(px(glow_width_2));
                            draw_arc(
                                &mut glow_path_2,
                                gauge_radius,
                                start_angle,
                                current_fill_angle,
                            );
                            window.paint_path(glow_path_2.build().unwrap(), glow_color_2);

                            // Main Stroke
                            let stroke_width = if theme.is_light { 12.0 } else { 8.0 };
                            let mut needle_path = PathBuilder::stroke(px(stroke_width));
                            draw_arc(
                                &mut needle_path,
                                gauge_radius,
                                start_angle,
                                current_fill_angle,
                            );

                            let needle_color = if theme.is_light {
                                gpui::hsla(215.0 / 360.0, 0.16, 0.47, 1.0)
                            } else {
                                theme.primary
                            };
                            window.paint_path(needle_path.build().unwrap(), needle_color);
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

                        // --- 4. Waveform Circle (Innermost) ---
                        // This matches the "squiggly" circle in the screenshot
                        let mut wave_path = PathBuilder::stroke(px(3.0));
                        let num_points = 128; // Match buffer size roughly
                        let base_radius = CIRCLE_RADIUS;
                        // Use amplitude to drive the "squiggles" with more reasonable scaling
                        let wave_amp = (amplitude + ai_amplitude).min(1.0) * 25.0;
                        let mut first_p = point(px(0.0), px(0.0));

                        for i in 0..=num_points {
                            // Slow spin for waveform itself: rotation * 0.5
                            let angle =
                                (i as f32 / num_points as f32) * 2.0 * PI + (rotation * 0.5);

                            // Generate consistent noise based on angle and time (rotation)
                            // We don't have per-frequency data here easily without FFT,
                            // so we simulate the "jagged" look with high-frequency noise
                            let noise_seed = (i as f32 * 0.5) + (rotation * 2.0);
                            let noise = (noise_seed.sin() + (noise_seed * 2.7).cos()) * 0.5;

                            let r = base_radius + (noise * wave_amp);

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
