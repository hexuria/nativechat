//! The Add sheet: a modal over the page with the five fields and Cancel / Save.

use super::LoginsPage;
use crate::components::fields::field_input;
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::component::{Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The sheet's fields. The password one is masked and emptied whenever the sheet closes,
/// so a typed secret does not sit in a hidden field.
#[derive(Clone)]
pub struct AddSheetInputs {
    pub title: Entity<InputState>,
    pub username: Entity<InputState>,
    pub password: Entity<InputState>,
    pub website: Entity<InputState>,
    pub notes: Entity<TextareaState>,
}

impl AddSheetInputs {
    pub fn new(window: &mut Window, cx: &mut Context<LoginsPage>) -> Self {
        Self {
            title: cx.new(|cx| InputState::new(window, cx).placeholder("Title")),
            username: cx.new(|cx| InputState::new(window, cx).placeholder("Username or email")),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Password")
                    .masked(true)
            }),
            website: cx
                .new(|cx| InputState::new(window, cx).placeholder("Website, like facebook.com")),
            notes: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Notes")
                    .auto_grow(3, 8)
            }),
        }
    }

    fn clear(&self, window: &mut Window, cx: &mut App) {
        for input in [&self.title, &self.username, &self.password, &self.website] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.notes
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// Cancel, the dimmed background, or a Save that was taken: the fields empty and the
    /// sheet goes.
    fn close(&self, app: &Entity<AppState>, window: &mut Window, cx: &mut App) {
        self.clear(window, cx);
        app.update(cx, |state, cx| state.close_site_login_add(cx));
    }
}

pub(super) fn render(
    inputs: AddSheetInputs,
    error: Option<String>,
    app: Entity<AppState>,
    theme: &Theme,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    div()
        .id("settings-login-add-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::black().opacity(0.32))
        .on_mouse_down(MouseButton::Left, {
            let inputs = inputs.clone();
            let app = app.clone();
            move |_, window, cx| inputs.close(&app, window, cx)
        })
        .child(
            v_flex()
                .id("settings-login-add-sheet")
                .w(px(440.))
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
                        .child("New Login"),
                )
                .child(field(
                    "Title",
                    "settings-login-add-title",
                    field_input(&inputs.title),
                    muted,
                ))
                .child(field(
                    "User Name",
                    "settings-login-add-username",
                    field_input(&inputs.username),
                    muted,
                ))
                .child(field(
                    "Password",
                    "settings-login-add-password",
                    field_input(&inputs.password),
                    muted,
                ))
                .child(field(
                    "Website",
                    "settings-login-add-website",
                    field_input(&inputs.website),
                    muted,
                ))
                .child(field(
                    "Notes",
                    "settings-login-add-notes",
                    Textarea::new(&inputs.notes)
                        .appearance(false)
                        .w_full()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(theme.input)
                        .bg(theme.input_background()),
                    muted,
                ))
                .when_some(error, |this, error| {
                    this.child(
                        div()
                            .id("settings-login-add-error")
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
                            Button::new("settings-login-add-cancel")
                                .label("Cancel")
                                .on_click({
                                    let inputs = inputs.clone();
                                    let app = app.clone();
                                    move |_, window, cx| inputs.close(&app, window, cx)
                                }),
                        )
                        .child(
                            Button::new("settings-login-add-save")
                                .label("Save")
                                .primary()
                                .on_click(move |_, window, cx| {
                                    let title = inputs.title.read(cx).value().to_string();
                                    let username = inputs.username.read(cx).value().to_string();
                                    let password = inputs.password.read(cx).value().to_string();
                                    let website = inputs.website.read(cx).value().to_string();
                                    let notes = inputs.notes.read(cx).value().to_string();
                                    let taken = app.update(cx, |state, cx| {
                                        state.add_site_login(
                                            website, username, password, title, notes, cx,
                                        )
                                    });
                                    // The state closed the sheet; the fields follow.
                                    if taken {
                                        inputs.clear(window, cx);
                                    }
                                }),
                        ),
                ),
        )
}

fn field(
    label: &'static str,
    id: &'static str,
    input: impl IntoElement,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(px(4.))
        .child(div().text_xs().text_color(muted).child(label))
        .child(div().id(id).w_full().child(input))
}
