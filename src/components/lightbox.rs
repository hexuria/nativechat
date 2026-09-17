//! A turn's pictures, full window.
//!
//! The transcript paints a screenshot small, because a feed of full-size screens is
//! unreadable. This is where a picture is actually looked at: one set at a time, the one
//! being shown, and a filmstrip to step through the rest. The overlay is near-black in
//! both themes — a screenshot reads against dark, whatever the app is wearing.

use crate::actions::{CloseLightbox, LightboxNext, LightboxPrev};
use crate::chrome::TITLE_BAR_H;
use crate::opengrok::ScreenshotSpec;
use crate::state::AppState;
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{Icon, WindowExt, h_flex, v_flex};
use gpui_kit::*;

/// What the lightbox is showing: the set a tile in the transcript opened, and which of
/// its pictures is on screen.
#[derive(Clone)]
pub struct Lightbox {
    pub shots: Vec<ScreenshotSpec>,
    pub index: usize,
}

impl Lightbox {
    pub fn current(&self) -> Option<&ScreenshotSpec> {
        self.shots.get(self.index)
    }
}

/// Room kept clear above the picture: the app paints its own title bar in the first 52px,
/// and the overlay's own buttons sit up there too.
const TOP_ROOM: f32 = TITLE_BAR_H + 28.0;
/// Room kept clear below the picture for the caption line and the filmstrip.
const BOTTOM_ROOM: f32 = 150.0;
const SIDE_ROOM: f32 = 72.0;
const STRIP_TILE: f32 = 46.0;

/// The line under the picture: what the tool said about it, then where it sits in the set.
/// A screenshot that arrived without a caption is still worth numbering.
pub fn caption_line(caption: &str, index: usize, total: usize) -> String {
    let position = format!("{} / {}", index + 1, total.max(1));
    let caption = caption.trim();
    if caption.is_empty() {
        position
    } else {
        format!("{caption} · {position}")
    }
}

/// The picture's size in the window: as large as the free space allows, its own shape kept,
/// and never blown up past the pixels it actually has.
fn fitted_size(natural: (f32, f32), room: (f32, f32)) -> (f32, f32) {
    let (width, height) = (natural.0.max(1.0), natural.1.max(1.0));
    let (room_w, room_h) = (room.0.max(1.0), room.1.max(1.0));
    let scale = (room_w / width).min(room_h / height).min(1.0);
    (width * scale, height * scale)
}

/// Close and hand the keyboard back to the app, which parks it on Root.
fn close(state: &Entity<AppState>, window: &mut Window, cx: &mut App) {
    state.update(cx, |state, cx| state.close_lightbox(cx));
    window.blur(cx);
}

/// One of the two round buttons in the top-right corner.
fn corner_button(
    id: &'static str,
    icon: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(34.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::white().opacity(0.12))
        .hover(|style| style.bg(gpui::white().opacity(0.24)))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(
            Icon::default()
                .path(icon)
                .size(px(16.))
                .text_color(gpui::white()),
        )
}

pub struct LightboxView {
    state: Entity<AppState>,
    focus_handle: FocusHandle,
    was_open: bool,
    pending_focus: bool,
}

impl LightboxView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.observe(&state, |this, state, cx| {
            let open = state.read(cx).lightbox.is_some();
            // Escape and the arrows are bound in this overlay's own key context, which is
            // only in the dispatch path while it holds focus — so it takes focus the moment
            // it opens rather than waiting for a click on itself.
            if open && !this.was_open {
                this.pending_focus = true;
            }
            this.was_open = open;
            cx.notify();
        })
        .detach();
        Self {
            state,
            focus_handle,
            was_open: false,
            pending_focus: false,
        }
    }
}

impl Render for LightboxView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_focus {
            self.pending_focus = false;
            self.focus_handle.focus(window, cx);
        }
        let Some(open) = self.state.read(cx).lightbox.clone() else {
            return div().into_any_element();
        };
        let Some(shot) = open.current().cloned() else {
            return div().into_any_element();
        };
        let viewport = window.viewport_size();
        let (width, height) = fitted_size(
            (shot.width.max(1) as f32, shot.height.max(1) as f32),
            (
                f32::from(viewport.width) - SIDE_ROOM * 2.0,
                f32::from(viewport.height) - TOP_ROOM - BOTTOM_ROOM,
            ),
        );
        let total = open.shots.len();
        let strip = (total > 1).then(|| {
            let mut row = h_flex().gap(px(8.)).justify_center().flex_wrap();
            for (ix, item) in open.shots.iter().enumerate() {
                let current = ix == open.index;
                let state = self.state.clone();
                row = row.child(
                    div()
                        .id(ElementId::Name(format!("lightbox-strip-{ix}").into()))
                        .size(px(STRIP_TILE))
                        .rounded(px(8.))
                        .border_2()
                        .border_color(if current {
                            gpui::white()
                        } else {
                            gpui::transparent_black()
                        })
                        .overflow_hidden()
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            state.update(cx, |state, cx| state.show_lightbox_image(ix, cx));
                        })
                        .child(
                            img(item.image.clone())
                                .size_full()
                                .rounded(px(6.))
                                .object_fit(ObjectFit::Cover),
                        ),
                );
            }
            row
        });

        div()
            .id("lightbox")
            .track_focus(&self.focus_handle)
            .key_context("Lightbox")
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::black().opacity(0.88))
            // Anywhere that is not the picture or a control is a way out.
            .on_mouse_down(MouseButton::Left, {
                let state = self.state.clone();
                move |_, window, cx| close(&state, window, cx)
            })
            .on_action({
                let state = self.state.clone();
                move |_: &CloseLightbox, window: &mut Window, cx: &mut App| {
                    close(&state, window, cx)
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &LightboxPrev, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| state.step_lightbox(-1, cx));
                }
            })
            .on_action({
                let state = self.state.clone();
                move |_: &LightboxNext, _window: &mut Window, cx: &mut App| {
                    state.update(cx, |state, cx| state.step_lightbox(1, cx));
                }
            })
            .child(
                v_flex()
                    .size_full()
                    .pt(px(TOP_ROOM))
                    .pb(px(24.))
                    .px(px(SIDE_ROOM))
                    .items_center()
                    .justify_center()
                    .gap(px(16.))
                    .child(
                        div()
                            .id("lightbox-image")
                            .rounded(px(10.))
                            .overflow_hidden()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(img(shot.image.clone()).w(px(width)).h(px(height))),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(gpui::white().opacity(0.62))
                            .child(caption_line(&shot.caption, open.index, total)),
                    )
                    .children(strip),
            )
            .child(
                h_flex()
                    .absolute()
                    .top(px(16.))
                    .right(px(16.))
                    .gap(px(8.))
                    .child(corner_button("lightbox-download", "icons/download.svg", {
                        let state = self.state.clone();
                        move |window: &mut Window, cx: &mut App| {
                            let saved = state.read(cx).download_lightbox_image();
                            match saved {
                                Ok(path) => window.push_notification(
                                    (NotificationType::Success, format!("Saved to {path}")),
                                    cx,
                                ),
                                Err(error) => {
                                    window.push_notification((NotificationType::Error, error), cx)
                                }
                            }
                        }
                    }))
                    .child(corner_button("lightbox-close", "icons/close.svg", {
                        let state = self.state.clone();
                        move |window: &mut Window, cx: &mut App| close(&state, window, cx)
                    })),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{caption_line, fitted_size};

    #[test]
    fn the_caption_carries_the_place_in_the_set() {
        assert_eq!(
            caption_line("screenshot of the desk", 2, 4),
            "screenshot of the desk · 3 / 4"
        );
    }

    /// A tool that sent a picture and said nothing still tells you where you are in the set.
    #[test]
    fn a_picture_without_words_is_still_numbered() {
        assert_eq!(caption_line("", 0, 3), "1 / 3");
        assert_eq!(caption_line("   ", 0, 3), "1 / 3");
    }

    #[test]
    fn the_only_picture_of_a_set_is_the_first_of_one() {
        assert_eq!(caption_line("one screen", 0, 1), "one screen · 1 / 1");
    }

    #[test]
    fn a_picture_fills_the_window_without_losing_its_shape() {
        let (width, height) = fitted_size((1280.0, 800.0), (800.0, 800.0));
        assert_eq!(width, 800.0);
        assert_eq!(height, 500.0);
    }

    /// Blown up past its own pixels a screenshot is a blur, so it stops at its natural size.
    #[test]
    fn a_small_picture_is_never_stretched_past_itself() {
        assert_eq!(fitted_size((320.0, 200.0), (1600.0, 900.0)), (320.0, 200.0));
    }
}
