//! The effort control uses the kit's slider interaction and accessibility, with a larger pill
//! and thumb. Its colour follows the model's own ordered stops; the last stop gets the glow,
//! whether that model calls it Max, Ultra, or something else.

use gpui_kit::base::{Slider as BaseSlider, SliderIndicator, SliderThumb, SliderTrack};
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::{ActiveTheme, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub(crate) const TRACK_HEIGHT: f32 = 24.;
pub(crate) const THUMB_SIZE: f32 = 28.;
pub(crate) const CONTROL_HEIGHT: f32 = 36.;

/// The same blue-to-violet scale colours the live effort name and the slider. Text uses a
/// darker shade in light mode so the label remains readable on the white popover.
pub(crate) fn effort_colour(stop: usize, stops: usize, theme: &Theme) -> Hsla {
    let progress = stop as f32 / stops.saturating_sub(1).max(1) as f32;
    hsla(
        0.61 + 0.13 * progress,
        0.75,
        if theme.is_dark() { 0.72 } else { 0.48 },
        1.,
    )
}

#[derive(IntoElement)]
pub(crate) struct EffortSlider {
    state: Entity<SliderState>,
    id: &'static str,
    stops: usize,
    active: bool,
    disabled: bool,
}

impl EffortSlider {
    pub(crate) fn new(
        state: Entity<SliderState>,
        id: &'static str,
        stops: usize,
        active: bool,
        disabled: bool,
    ) -> Self {
        Self {
            state,
            id,
            stops,
            active,
            disabled,
        }
    }
}

/// Fixed positions avoid random flicker as a drag redraws the control. Small dots and a few
/// crosses reproduce the bright flecks in the strongest effort's violet fill.
const FLECKS: &[(f32, f32, f32)] = &[
    (0.07, 0.68, 1.5),
    (0.12, 0.28, 2.),
    (0.18, 0.78, 1.),
    (0.24, 0.46, 1.5),
    (0.33, 0.24, 2.),
    (0.39, 0.73, 1.),
    (0.46, 0.40, 1.5),
    (0.53, 0.78, 2.),
    (0.61, 0.24, 1.5),
    (0.68, 0.57, 2.),
    (0.77, 0.80, 1.),
    (0.85, 0.32, 1.5),
    (0.92, 0.62, 2.),
];

impl RenderOnce for EffortSlider {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let progress = self.state.read(cx).percentage().end.clamp(0., 1.);
        let strongest = self.active
            && self.stops > 1
            && self.state.read(cx).value().end() == (self.stops - 1) as f32;
        let tone = hsla(0.61 + 0.13 * progress, 0.82, 0.62, 1.);
        let bright = hsla(tone.h, tone.s, tone.l + 0.12 * progress, 1.);
        let fill_id = format!("{}-fill", self.id);
        let glitter_id = format!("{}-glitter", self.id);
        let thumb_id = format!("{}-thumb", self.id);
        let track_id = format!("{}-track", self.id);
        let filled = div()
            .id(SharedString::from(fill_id.clone()))
            .debug_selector(move || fill_id)
            .absolute()
            .left_0()
            .top_0()
            .h_full()
            .w(relative(progress))
            .rounded_full()
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .h_full()
                    .w(relative(0.55))
                    .rounded_l_full()
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(rgb(0x356bf5), 0.),
                        linear_color_stop(bright, 1.),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(0.55))
                    .right_0()
                    .top_0()
                    .h_full()
                    .rounded_r_full()
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(bright, 0.),
                        linear_color_stop(tone, 1.),
                    )),
            )
            .when(strongest, |this| {
                this.child(
                    div()
                        .id(SharedString::from(glitter_id.clone()))
                        .debug_selector(move || glitter_id)
                        .absolute()
                        .inset_0()
                        .children(FLECKS.iter().map(|&(x, y, size)| {
                            div()
                                .absolute()
                                .left(relative(x))
                                .top(px(y * TRACK_HEIGHT))
                                .size(px(size))
                                .rounded_full()
                                .bg(gpui::white().opacity(0.68))
                        }))
                        .children([0.28, 0.56, 0.81].into_iter().map(|x| {
                            div()
                                .absolute()
                                .left(relative(x))
                                .top(px(11.))
                                .w(px(4.))
                                .h(px(1.))
                                .bg(gpui::white().opacity(0.75))
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(1.5))
                                        .top(px(-1.5))
                                        .w(px(1.))
                                        .h(px(4.))
                                        .bg(gpui::white().opacity(0.75)),
                                )
                        })),
                )
            });
        BaseSlider::new(&self.state)
            .disabled(self.disabled)
            .flex()
            .items_center()
            .w_full()
            .h(px(CONTROL_HEIGHT))
            .child(
                SliderTrack::new(&self.state)
                    .disabled(self.disabled)
                    .flex()
                    .items_center()
                    .w_full()
                    .h(px(CONTROL_HEIGHT))
                    .child(
                        SliderIndicator::new(&self.state)
                            .debug_selector(move || track_id)
                            .relative()
                            .flex_1()
                            .min_w(px(0.))
                            .mx(px(THUMB_SIZE / 2.))
                            .h(px(TRACK_HEIGHT))
                            .rounded_full()
                            .bg(theme.muted_foreground.opacity(0.26))
                            .when(strongest, |this| {
                                this.shadow(vec![BoxShadow {
                                    color: rgb(0x9c69f6).opacity(0.28).into(),
                                    offset: point(px(0.), px(0.)),
                                    blur_radius: px(10.),
                                    spread_radius: px(1.),
                                    inset: false,
                                }])
                            })
                            .child(filled)
                            .children((0..self.stops).map(|stop| {
                                let fraction =
                                    stop as f32 / self.stops.saturating_sub(1).max(1) as f32;
                                div()
                                    .absolute()
                                    .left(relative(fraction))
                                    .top(px(TRACK_HEIGHT / 2. - 1.5))
                                    .ml(px(-1.5))
                                    .size(px(3.))
                                    .rounded_full()
                                    .bg(if fraction <= progress {
                                        gpui::white().opacity(0.48)
                                    } else {
                                        theme.muted_foreground.opacity(0.4)
                                    })
                            }))
                            .child(
                                SliderThumb::new(&self.state)
                                    .disabled(self.disabled)
                                    .debug_selector(move || thumb_id)
                                    .absolute()
                                    .left(relative(progress))
                                    .top(px((TRACK_HEIGHT - THUMB_SIZE) / 2.))
                                    .ml(px(-THUMB_SIZE / 2.))
                                    .size(px(THUMB_SIZE))
                                    .rounded_full()
                                    .bg(gpui::white())
                                    .border_1()
                                    .border_color(gpui::black().opacity(0.12))
                                    .shadow_sm()
                                    .cursor_pointer()
                                    .hover(|style| style.border_color(rgb(0x9c69f6).opacity(0.6))),
                            ),
                    ),
            )
    }
}
