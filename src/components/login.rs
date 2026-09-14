use crate::components::fields::field_input;
use crate::state::{AppState, AuthStatus};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputContentType, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct LoginView {
    state: Entity<AppState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
    email_error: Option<String>,
    password_error: Option<String>,
}

impl LoginView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("Email"));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        let this = Self {
            state,
            email,
            password,
            email_error: None,
            password_error: None,
        };
        cx.subscribe(&this.email, |this, _, event: &InputEvent, cx| {
            this.on_field_event(true, event, cx);
        })
        .detach();
        cx.subscribe(&this.password, |this, _, event: &InputEvent, cx| {
            this.on_field_event(false, event, cx);
        })
        .detach();
        this
    }

    fn on_field_event(&mut self, is_email: bool, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::Change => {
                if is_email {
                    self.email_error = None;
                } else {
                    self.password_error = None;
                }
                cx.notify();
            }
            InputEvent::PressEnter { shift, secondary } if !shift && !secondary => {
                self.try_submit(cx);
            }
            _ => {}
        }
    }

    fn try_submit(&mut self, cx: &mut Context<Self>) {
        let email = self.email.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        self.email_error = email_error(&email);
        self.password_error = password_error(&password);
        if self.email_error.is_some() || self.password_error.is_some() {
            cx.notify();
            return;
        }
        self.state.update(cx, |state, cx| {
            state.login(email, password, cx);
        });
    }

    fn dismiss_server_error(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.auth_error = None;
            cx.notify();
        });
    }
}

fn email_error(email: &str) -> Option<String> {
    let email = email.trim();
    if email.is_empty() {
        return Some("Enter your email.".into());
    }
    if !is_plausible_email(email) {
        return Some("Enter a valid email address.".into());
    }
    None
}

fn password_error(password: &str) -> Option<String> {
    if password.is_empty() {
        Some("Enter your password.".into())
    } else {
        None
    }
}

fn is_plausible_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
}

impl Render for LoginView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (busy, server_error) = {
            let state = self.state.read(cx);
            (
                state.auth_status == AuthStatus::SigningIn,
                state.auth_error.clone(),
            )
        };
        let theme = cx.theme();
        let danger = theme.danger;
        let email_error = self.email_error.clone();
        let password_error = self.password_error.clone();
        let view = cx.entity();

        div()
            .id("page-login")
            .relative()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.background)
            .when_some(server_error, |this, message| {
                this.child(server_alert(
                    message,
                    danger,
                    theme.background,
                    view.clone(),
                ))
            })
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
                        v_flex()
                            .id("login-email")
                            .w_full()
                            .gap(px(4.))
                            .child(field_input(&self.email))
                            .when_some(email_error, |this, message| {
                                this.child(
                                    div()
                                        .id("login-email-error")
                                        .text_xs()
                                        .text_color(danger)
                                        .child(message),
                                )
                            }),
                    )
                    .child(
                        v_flex()
                            .id("login-password")
                            .w_full()
                            .gap(px(4.))
                            .child(
                                field_input(&self.password)
                                    .mask_toggle()
                                    .content_type(InputContentType::Password),
                            )
                            .when_some(password_error, |this, message| {
                                this.child(
                                    div()
                                        .id("login-password-error")
                                        .text_xs()
                                        .text_color(danger)
                                        .child(message),
                                )
                            }),
                    )
                    .child(
                        div().id("login-submit").w_full().child(
                            Button::new("login-submit-btn")
                                .label(if busy { "Signing in…" } else { "Sign in" })
                                .primary()
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| this.try_submit(cx))),
                        ),
                    ),
            )
    }
}

fn server_alert(
    message: String,
    danger: Hsla,
    background: Hsla,
    view: Entity<LoginView>,
) -> impl IntoElement {
    div()
        .id("login-error")
        .absolute()
        .top(px(24.))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            h_flex()
                .id("login-alert")
                .w(px(380.))
                .items_start()
                .gap(px(10.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(12.))
                .border_1()
                .border_color(danger)
                .bg(background)
                .shadow_lg()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_sm()
                        .text_color(danger)
                        .child(message),
                )
                .child(
                    div()
                        .id("login-alert-close")
                        .size(px(24.))
                        .flex_shrink_0()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            view.update(cx, |this, cx| this.dismiss_server_error(cx));
                        })
                        .child(Icon::default().path("icons/close.svg").size(px(14.))),
                ),
        )
}
