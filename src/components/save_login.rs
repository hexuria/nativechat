//! Save-login offer and credential-request confirm cards.
//!
//! Never paint a password. `filled` is not claimed here — A.0 posts
//! missing/error/denied without typing into Box.

use crate::opengrok::{
    CredentialRequestResolution, CredentialRequestSpec, SaveLoginSpec, credential_request_allow_id,
    credential_request_card_id, credential_request_deny_id, credential_request_pill_id,
    save_login_card_id, save_login_save_id, save_login_skip_id,
};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
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
    match spec.resolution {
        Some(resolution) => render_settled_credential_request(spec, resolution, cx),
        None => render_idle_credential_request(spec, app, cx),
    }
}

fn credential_title(spec: &CredentialRequestSpec) -> String {
    match &spec.username {
        Some(username) => format!("Use saved login for {} as {}?", spec.origin, username),
        None => format!("Use a saved login for {}?", spec.origin),
    }
}

fn render_idle_credential_request(
    spec: &CredentialRequestSpec,
    app: Entity<AppState>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
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
        .occlude()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(credential_title(spec)),
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

fn render_settled_credential_request(
    spec: &CredentialRequestSpec,
    resolution: CredentialRequestResolution,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let (pill_fill, pill_text) = match resolution {
        CredentialRequestResolution::Used | CredentialRequestResolution::Filled => {
            (theme.green.opacity(0.18), theme.green)
        }
        CredentialRequestResolution::Denied | CredentialRequestResolution::Missing => {
            (theme.secondary, theme.muted_foreground)
        }
    };
    v_flex()
        .id(ElementId::Name(
            credential_request_card_id(&spec.request_id).into(),
        ))
        .w_full()
        .gap(px(8.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(credential_title(spec)),
                )
                .child(
                    h_flex()
                        .id(ElementId::Name(
                            credential_request_pill_id(&spec.request_id).into(),
                        ))
                        .items_center()
                        .gap(px(4.))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(999.))
                        .bg(pill_fill)
                        .text_color(pill_text)
                        .text_xs()
                        .when(
                            matches!(
                                resolution,
                                CredentialRequestResolution::Used
                                    | CredentialRequestResolution::Filled
                            ),
                            // Missing / Denied: no check — must not look like success.
                            |this| this.child(Icon::new(IconName::Check).size(px(12.))),
                        )
                        .child(resolution.pill()),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(resolution.body()),
        )
        .into_any_element()
}
