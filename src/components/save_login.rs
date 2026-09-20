//! Save-login offer and credential-request confirm cards.
//!
//! Never paint a password. `filled` is not claimed here — A.0 posts
//! missing/error/denied without typing into Box.

use crate::opengrok::{
    CredentialRequestSpec, SaveLoginSpec, credential_request_allow_id, credential_request_card_id,
    credential_request_deny_id, save_login_card_id, save_login_save_id, save_login_skip_id,
};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::*;

pub fn render_save_login(spec: &SaveLoginSpec, app: Entity<AppState>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let entry = spec.form_entry_id.clone();
    let save_entry = entry.clone();
    v_flex()
        .id(ElementId::Name(
            save_login_card_id(&spec.form_entry_id).into(),
        ))
        .w_full()
        .gap(px(10.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!(
                    "Save login for {} as {}?",
                    spec.origin, spec.username
                )),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("The password stays in the OS keychain. NativeChat never shows it again."),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(
                    Button::new(save_login_save_id(&spec.form_entry_id))
                        .label("Save")
                        .primary()
                        .on_click({
                            let app = app.clone();
                            move |_, _, cx| {
                                app.update(cx, |state, cx| {
                                    state.save_offered_login(save_entry.clone(), cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new(save_login_skip_id(&spec.form_entry_id))
                        .label("Not now")
                        .ghost()
                        .on_click(move |_, _, cx| {
                            app.update(cx, |state, cx| {
                                state.skip_save_login(entry.clone(), cx);
                            });
                        }),
                ),
        )
        .into_any_element()
}

pub fn render_credential_request(
    spec: &CredentialRequestSpec,
    app: Entity<AppState>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let title = match &spec.username {
        Some(username) => format!("Use saved login for {} as {}?", spec.origin, username),
        None => format!("Use a saved login for {}?", spec.origin),
    };
    let request_id = spec.request_id.clone();
    let deny_id = spec.request_id.clone();
    v_flex()
        .id(ElementId::Name(
            credential_request_card_id(&spec.request_id).into(),
        ))
        .w_full()
        .gap(px(10.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(
                    "NativeChat will not type this password into the computer. Confirming uses a saved login only after a session can be restored.",
                ),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(
                    Button::new(credential_request_allow_id(&spec.request_id))
                        .label("Use saved login")
                        .primary()
                        .on_click({
                            let app = app.clone();
                            move |_, _, cx| {
                                app.update(cx, |state, cx| {
                                    state.answer_credential_request(request_id.clone(), true, cx);
                                });
                            }
                        }),
                )
                .child(
                    Button::new(credential_request_deny_id(&spec.request_id))
                        .label("Not now")
                        .ghost()
                        .on_click(move |_, _, cx| {
                            app.update(cx, |state, cx| {
                                state.answer_credential_request(deny_id.clone(), false, cx);
                            });
                        }),
                ),
        )
        .into_any_element()
}
