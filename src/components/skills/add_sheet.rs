//! The New skill page's fields: Name, Description and Instructions. They outlive the page, so
//! what was typed is kept as a draft when the person goes back, and are emptied once the server
//! has taken the skill.

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::*;

/// The New skill page's fields.
#[derive(Clone)]
pub struct AddSheetInputs {
    pub name: Entity<InputState>,
    pub description: Entity<InputState>,
    pub body: Entity<TextareaState>,
}

impl AddSheetInputs {
    pub fn new<T>(window: &mut Window, cx: &mut Context<T>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("expense-report")),
            description: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("When your bot should reach for this, in one line")
            }),
            body: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("How the task is done, in your own words.")
                    .auto_grow(6, 16)
            }),
        }
    }

    /// Empty the fields. Called when the sheet goes, never while it is up: what is in them is
    /// what somebody typed, and a refusal is not a reason to take it away.
    pub(crate) fn clear(&self, window: &mut Window, cx: &mut App) {
        for input in [&self.name, &self.description] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.body
            .update(cx, |input, cx| input.set_value("", window, cx));
    }
}
