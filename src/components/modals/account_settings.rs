use crate::state::AppState;
use gpui::InteractiveElement;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, IntoElement, Render, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme, Icon, IconName,
    button::Button,
    input::{Input, InputState},
};

pub struct AccountSettingsModal {
    state: Entity<AppState>,
    active_tab: usize, // 0: Profile, 1: Security
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
        foreground: gpui::Hsla,
        background: gpui::Hsla,
        _cx: &mut Context<Self>,
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
                            .child("Email cannot be changed")
                            .text_xs()
                            .text_color(gpui::red()),
                    ),
            )
            .child(
                Button::new("update-profile-btn")
                    .label("Update Profile")
                    .bg(foreground)
                    .text_color(background)
                    .w_full(),
            )
    }

    fn render_security_tab(
        &self,
        foreground: gpui::Hsla,
        background: gpui::Hsla,
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
                            .child(Icon::new(IconName::Check).text_color(gpui::green())),
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
                            .child(Icon::new(IconName::Check).text_color(gpui::green())),
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
                    .w_full(),
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
            .bg(gpui::black().opacity(0.5)) // Overlay
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
                                        gpui::div()
                                            .id("close-account-settings")
                                            .cursor_pointer()
                                            .child(Icon::new(IconName::Close))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.state.update(cx, |state, cx| {
                                                    state.toggle_account_settings(cx);
                                                });
                                            })),
                                    ),
                            )
                            .child(
                                // Tabs
                                div()
                                    .flex()
                                    .flex_row()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(
                                        gpui::div()
                                            .id("profile-tab")
                                            .p_2()
                                            .cursor_pointer()
                                            .child("Profile")
                                            .text_color(if self.active_tab == 0 {
                                                theme.foreground
                                            } else {
                                                theme.muted_foreground
                                            })
                                            .border_b_2()
                                            .border_color(if self.active_tab == 0 {
                                                theme.foreground
                                            } else {
                                                gpui::transparent_black()
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.active_tab = 0;
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        gpui::div()
                                            .id("security-tab")
                                            .p_2()
                                            .cursor_pointer()
                                            .child("Security")
                                            .text_color(if self.active_tab == 1 {
                                                theme.foreground
                                            } else {
                                                theme.muted_foreground
                                            })
                                            .border_b_2()
                                            .border_color(if self.active_tab == 1 {
                                                theme.foreground
                                            } else {
                                                gpui::transparent_black()
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.active_tab = 1;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                // Content
                                div().p_4().child(if self.active_tab == 0 {
                                    self.render_profile_tab(foreground, background, cx)
                                        .into_any_element()
                                } else {
                                    self.render_security_tab(foreground, background, cx)
                                        .into_any_element()
                                }),
                            ),
                    ),
            )
    }
}
