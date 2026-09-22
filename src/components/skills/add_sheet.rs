//! Create a skill: a modal over the page with the three fields and Cancel / Save.
//!
//! The name is what a person types after a slash, so the field says what a name may be rather
//! than letting the server refuse a sentence with spaces in it after the fact.

use super::SkillsPage;
use crate::components::fields::field_input;
use crate::state::AppState;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::component::{Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The sheet's fields.
#[derive(Clone)]
pub struct AddSheetInputs {
    pub name: Entity<InputState>,
    pub description: Entity<InputState>,
    pub body: Entity<TextareaState>,
}

impl AddSheetInputs {
    pub fn new(window: &mut Window, cx: &mut Context<SkillsPage>) -> Self {
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
    pub(super) fn clear(&self, window: &mut Window, cx: &mut App) {
        for input in [&self.name, &self.description] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.body
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// The dimmed background: the sheet goes and the fields keep what is in them.
    ///
    /// A click that lands wide of a sheet is not a decision to throw work away — it is most
    /// often a click that missed. Cancel is the decision, and Cancel empties them.
    fn close(&self, app: &Entity<AppState>, cx: &mut App) {
        app.update(cx, |state, cx| state.close_skill_add(cx));
    }

    /// Cancel: said plainly, so the words go with the sheet.
    fn cancel(&self, app: &Entity<AppState>, window: &mut Window, cx: &mut App) {
        self.clear(window, cx);
        self.close(app, cx);
    }
}

pub(super) fn render(
    inputs: AddSheetInputs,
    error: Option<String>,
    saving: bool,
    app: Entity<AppState>,
    theme: &Theme,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    div()
        .id("settings-skill-add-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::black().opacity(0.32))
        .on_mouse_down(MouseButton::Left, {
            let inputs = inputs.clone();
            let app = app.clone();
            move |_, _, cx| inputs.close(&app, cx)
        })
        .child(
            v_flex()
                .id("settings-skill-add-sheet")
                .w(px(480.))
                .bg(theme.popover)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.border)
                .rounded(px(14.))
                .shadow_lg()
                .px(px(20.))
                .py(px(18.))
                .gap(px(10.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("New skill"),
                )
                .child(div().text_xs().text_color(muted).child(
                    "Instructions your bot reads before it works. It reaches for one when \
                         the description fits what it has been asked to do.",
                ))
                .child(field(
                    "Name",
                    "settings-skill-add-name",
                    field_input(&inputs.name),
                    Some("Lowercase letters, digits, dots and dashes — it is typed after a slash."),
                    muted,
                ))
                .child(field(
                    "Description",
                    "settings-skill-add-description",
                    field_input(&inputs.description),
                    None,
                    muted,
                ))
                .child(field(
                    "Instructions",
                    "settings-skill-add-body",
                    Textarea::new(&inputs.body)
                        .appearance(false)
                        .w_full()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(theme.input)
                        .bg(theme.input_background()),
                    None,
                    muted,
                ))
                .when_some(error, |this, error| {
                    this.child(
                        div()
                            .id("settings-skill-add-error")
                            .text_xs()
                            .text_color(theme.danger)
                            .child(error),
                    )
                })
                .child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap(px(8.))
                        .pt(px(6.))
                        .child(
                            Button::new("settings-skill-add-cancel")
                                .label("Cancel")
                                .disabled(saving)
                                .on_click({
                                    let inputs = inputs.clone();
                                    let app = app.clone();
                                    move |_, window, cx| inputs.cancel(&app, window, cx)
                                }),
                        )
                        .child(
                            // Dead while the create is in flight. Pressed twice, the second
                            // try is a name the server has just taken, and the sheet would show
                            // "you already have a skill called that" about the skill it had
                            // itself just made.
                            Button::new("settings-skill-add-save")
                                .label(if saving { "Saving…" } else { "Save" })
                                .primary()
                                .disabled(saving)
                                .on_click(move |_, _, cx| {
                                    let name = inputs.name.read(cx).value().to_string();
                                    let description =
                                        inputs.description.read(cx).value().to_string();
                                    let body = inputs.body.read(cx).value().to_string();
                                    // The sheet stays up until the server has taken it, so a
                                    // refusal arrives with the words still in the fields.
                                    app.update(cx, |state, cx| {
                                        state.create_skill(name, description, body, cx)
                                    });
                                }),
                        ),
                ),
        )
}

fn field(
    label: &'static str,
    id: &'static str,
    input: impl IntoElement,
    hint: Option<&'static str>,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(px(4.))
        .child(div().text_xs().text_color(muted).child(label))
        .child(div().id(id).w_full().child(input))
        .when_some(hint, |this, hint| {
            this.child(div().text_xs().text_color(muted).child(hint))
        })
}
