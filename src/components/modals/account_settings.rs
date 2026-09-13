use crate::state::AppState;
use gpui_kit::InteractiveElement;
use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, FontWeight, IntoElement, MouseButton, Render, Styled, Window, div, px,
};
use crate::icons::NativeIcon;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName,
    button::Button,
    input::{Input, InputState},
    v_flex,
};

pub struct AccountSettingsModal {
    state: Entity<AppState>,
    active_tab: usize, // 0: Profile, 1: Security, 2: Advanced
    name_input: Entity<InputState>,
    email_input: Entity<InputState>,
    current_password_input: Entity<InputState>,
    new_password_input: Entity<InputState>,
    confirm_password_input: Entity<InputState>,
}

impl AccountSettingsModal {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let email_input = cx.new(|cx| InputState::new(window, cx).placeholder("Email"));
        let current_password_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Current password"));
        let new_password_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("New password"));
        let confirm_password_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Confirm new password"));

        Self {
            state,
            active_tab: 0,
            name_input,
            email_input,
            current_password_input,
            new_password_input,
            confirm_password_input,
        }
    }

    fn render_profile_tab(
        &self,
        foreground: gpui_kit::Hsla,
        background: gpui_kit::Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().child("Name").font_weight(FontWeight::BOLD))
                    .child(Input::new(&self.name_input)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().child("Email").font_weight(FontWeight::BOLD))
                    .child(Input::new(&self.email_input)) // Disabled not supported on Input view yet?
                    .child(
                        div()
                            .child("Email cannot be changed (OpenGrok identity)")
                            .text_xs()
                            .text_color(gpui_kit::red()),
                    )
                    .when_some(
                        self.state.read(cx).account.as_ref().map(|a| a.email.clone()),
                        |this, email| this.child(div().text_xs().child(email)),
                    ),
            )
            .child(
                Button::new("update-profile-btn")
                    .label("Update Profile")
                    .bg(foreground)
                    .text_color(background)
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let name = this.name_input.read(cx).value().to_string();
                        if name.trim().is_empty() {
                            return;
                        }
                        let (first, last) = split_display_name(&name);
                        this.state.update(cx, |state, cx| {
                            state.update_opengrok_profile(first, last, cx);
                        });
                    })),
            )
    }

    fn render_security_tab(
        &self,
        foreground: gpui_kit::Hsla,
        background: gpui_kit::Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().child("Linked Accounts").font_weight(FontWeight::BOLD))
                    // Mock linked accounts
                    .child(
                        div()
                            .p_2()
                            .bg(cx.theme().secondary)
                            .rounded_md()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(div().child("Email & Password"))
                            .child(Icon::new(IconName::Check).text_color(gpui_kit::green())),
                    )
                    .child(
                        div()
                            .p_2()
                            .bg(cx.theme().secondary)
                            .rounded_md()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(div().child("Google"))
                            .child(Icon::new(IconName::Check).text_color(gpui_kit::green())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().child("Change Password").font_weight(FontWeight::BOLD))
                    .child(Input::new(&self.current_password_input))
                    .child(Input::new(&self.new_password_input))
                    .child(Input::new(&self.confirm_password_input)),
            )
            .child(
                Button::new("change-password-btn")
                    .label("Change Password")
                    .bg(foreground)
                    .text_color(background)
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let current = this.current_password_input.read(cx).value().to_string();
                        let new_password = this.new_password_input.read(cx).value().to_string();
                        let confirm = this.confirm_password_input.read(cx).value().to_string();
                        if new_password != confirm {
                            this.state.update(cx, |state, cx| {
                                state.auth_error =
                                    Some("New password and confirmation do not match".into());
                                cx.notify();
                            });
                            return;
                        }
                        this.state.update(cx, |state, cx| {
                            state.change_opengrok_password(current, new_password, cx);
                        });
                    })),
            )
    }

    fn render_advanced_tab(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().gap_4().child(
            div().flex().flex_col().gap_1().child(
                div()
                    .child("No advanced settings available")
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .text_color(gpui_kit::white()),
            ),
        )
    }
}

impl Render for AccountSettingsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let foreground = theme.foreground.clone();
        let background = theme.background.clone();

        div()
            .absolute()
            .inset_0()
            .bg(gpui_kit::black().opacity(0.5)) // Overlay
            // Prevent clicks from passing through to elements behind the modal
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                cx.stop_propagation();
            })
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(500.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_xl()
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                // Header
                                div()
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .items_center()
                                    .p_4()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(
                                        div()
                                            .child("Account Settings")
                                            .font_weight(FontWeight::BOLD)
                                            .text_lg(),
                                    )
                                    .child(
                                        gpui_kit::div()
                                            .id("close-account-settings")
                                            .cursor_pointer()
                                            .child(Icon::new(NativeIcon::Close))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.state.update(cx, |state, cx| {
                                                    state.toggle_account_settings(cx);
                                                });
                                            })),
                                    ),
                            )
                            .child(
                                // Tabs - using segmented tabs with filling space
                                div().px_4().pt_4().child(
                                    gpui_kit::component::tab::TabBar::new("account-settings-tabs")
                                        .w_full()
                                        .segmented()
                                        .selected_index(self.active_tab)
                                        .on_click(cx.listener(|this, ix: &usize, _, cx| {
                                            this.active_tab = *ix;
                                            cx.notify();
                                        }))
                                        .child(gpui_kit::component::tab::Tab::new().flex_1().label("Profile"))
                                        .child(gpui_kit::component::tab::Tab::new().flex_1().label("Security"))
                                        .child(gpui_kit::component::tab::Tab::new().flex_1().label("Advanced")),
                                ),
                            )
                            .child(
                                // Content
                                div().p_4().child(
                                    v_flex()
                                        .gap_3()
                                        .when_some(
                                            self.state.read(cx).auth_error.clone(),
                                            |this, message| {
                                                this.child(
                                                    div()
                                                        .id("account-error")
                                                        .text_sm()
                                                        .text_color(cx.theme().danger)
                                                        .child(message),
                                                )
                                            },
                                        )
                                        .child(match self.active_tab {
                                    0 => self
                                        .render_profile_tab(foreground, background, cx)
                                        .into_any_element(),
                                    1 => self
                                        .render_security_tab(foreground, background, cx)
                                        .into_any_element(),
                                    _ => self.render_advanced_tab(cx).into_any_element(),
                                }),
                                ),
                            ),
                    ),
            )
    }
}

fn split_display_name(name: &str) -> (String, String) {
    let name = name.trim();
    match name.split_once(' ') {
        Some((first, last)) => (first.to_string(), last.trim().to_string()),
        None => (name.to_string(), String::new()),
    }
}
