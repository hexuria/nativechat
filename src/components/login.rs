use crate::state::{AppState, AuthStatus};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputContentType, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct LoginView {
    state: Entity<AppState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
}

impl LoginView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("Email"));
        let password = cx.new(|cx| InputState::new(window, cx).placeholder("Password"));
        Self {
            state,
            email,
            password,
        }
    }

    fn submit(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let email = self.email.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        self.state.update(cx, |state, cx| {
            state.login(email, password, cx);
        });
    }
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (busy, error, base) = {
            let state = self.state.read(cx);
            (
                state.auth_status == AuthStatus::SigningIn,
                state.auth_error.clone(),
                state
                    .config
                    .as_ref()
                    .map(|c| c.opengrok_base_url.clone())
                    .unwrap_or_else(|| "http://127.0.0.1:1447".into()),
            )
        };
        let theme = cx.theme();

        div()
            .id("page-login")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.background)
            .child(
                v_flex()
                    .id("login-card")
                    .w(px(380.))
                    .gap_3()
                    .p_6()
                    .rounded_xl()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::BOLD)
                            .child("Sign in to OpenGrok"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(base),
                    )
                    .child(
                        div()
                            .id("login-email")
                            .w_full()
                            .child(Input::new(&self.email)),
                    )
                    .child(
                        div()
                            .id("login-password")
                            .w_full()
                            .child(
                                Input::new(&self.password)
                                    .mask_toggle()
                                    .content_type(InputContentType::Password),
                            ),
                    )
                    .when_some(error, |this, message| {
                        this.child(
                            div()
                                .id("login-error")
                                .text_sm()
                                .text_color(theme.danger)
                                .child(message),
                        )
                    })
                    .child(
                        div().id("login-submit").w_full().child(
                            Button::new("login-submit-btn")
                                .label(if busy { "Signing in…" } else { "Sign in" })
                                .primary()
                                .disabled(busy)
                                .on_click(cx.listener(Self::submit)),
                        ),
                    ),
            )
    }
}
