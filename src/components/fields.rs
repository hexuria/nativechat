use gpui_kit::component::input::{Input, InputState};
use gpui_kit::*;

/// Form field: 1px theme border, no extra focus ring.
///
/// Kit Input defaults `focus_bordered: true`, which paints both a ring-colored
/// border and `focus_ring_style` (the thick black halo). Agent settings already
/// opted out; every other form field should use this so focus stays a clean
/// 8px-radius input.
pub fn field_input(state: &Entity<InputState>) -> Input {
    Input::new(state).focus_bordered(false).rounded(px(8.))
}
