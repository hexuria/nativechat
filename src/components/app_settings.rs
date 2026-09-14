use crate::actions::CloseSettings;
use crate::state::{AppSettingsTab, AppState, SubmitChord};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct AppSettings {
    state: Entity<AppState>,
}

impl AppSettings {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for AppSettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let (tab, chord, theme_mode, account_name, account_email) = {
            let state = self.state.read(cx);
            let (name, email) = state
                .account
                .as_ref()
                .map(|a| (a.display_name(), a.email.clone()))
                .unwrap_or_else(|| ("Not signed in".into(), String::new()));
            (
                state.app_settings_tab,
                state.submit_chord,
                state.theme_mode.clone(),
                name,
                email,
            )
        };
        let app = self.state.clone();

        h_flex()
            .id("app-settings")
            .key_context("AppSettings")
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_action({
                let app = app.clone();
                move |_: &CloseSettings, _, cx| {
                    app.update(cx, |state, cx| {
                        if state.is_app_settings_open {
                            state.toggle_app_settings(cx);
                        }
                    });
                }
            })
            .child(self.nav(tab, &theme, cx))
            .child(
                div()
                    .id("app-settings-body")
                    .flex_1()
                    .h_full()
                    .min_w(px(0.))
                    .overflow_y_scroll()
                    .px(px(48.))
                    .py(px(36.))
                    .child(
                        v_flex()
                            .max_w(px(720.))
                            .w_full()
                            .gap(px(20.))
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(tab_title(tab)),
                            )
                            .child(match tab {
                                AppSettingsTab::General => {
                                    general_page(chord, muted, app.clone()).into_any_element()
                                }
                                AppSettingsTab::Profile => profile_page(
                                    account_name,
                                    account_email,
                                    muted,
                                    app.clone(),
                                )
                                .into_any_element(),
                                AppSettingsTab::Appearance => {
                                    appearance_page(
                                        &theme_mode,
                                        muted,
                                        theme.foreground,
                                        app.clone(),
                                    )
                                    .into_any_element()
                                }
                                AppSettingsTab::Shortcuts => {
                                    shortcuts_page(chord, muted, &theme).into_any_element()
                                }
                            }),
                    ),
            )
    }
}

impl AppSettings {
    fn nav(
        &self,
        tab: AppSettingsTab,
        theme: &gpui_kit::component::Theme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let app = self.state.clone();
        v_flex()
            .id("app-settings-nav")
            .w(px(240.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .px(px(12.))
            .py(px(16.))
            .gap(px(4.))
            .child(
                div()
                    .id("app-settings-back")
                    .px(px(10.))
                    .py(px(8.))
                    .mb(px(8.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0x777777).opacity(0.16)))
                    .on_mouse_down(MouseButton::Left, {
                        let app = app.clone();
                        move |_, _, cx| {
                            app.update(cx, |state, cx| {
                                state.toggle_app_settings(cx);
                            });
                        }
                    })
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("← Back to app"),
                    ),
            )
            .child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Personal"),
            )
            .child(nav_item(
                "settings-tab-general",
                "General",
                tab == AppSettingsTab::General,
                AppSettingsTab::General,
                cx,
            ))
            .child(nav_item(
                "settings-tab-profile",
                "Profile",
                tab == AppSettingsTab::Profile,
                AppSettingsTab::Profile,
                cx,
            ))
            .child(nav_item(
                "settings-tab-appearance",
                "Appearance",
                tab == AppSettingsTab::Appearance,
                AppSettingsTab::Appearance,
                cx,
            ))
            .child(nav_item(
                "settings-tab-shortcuts",
                "Keyboard shortcuts",
                tab == AppSettingsTab::Shortcuts,
                AppSettingsTab::Shortcuts,
                cx,
            ))
    }
}

fn tab_title(tab: AppSettingsTab) -> &'static str {
    match tab {
        AppSettingsTab::General => "General",
        AppSettingsTab::Profile => "Profile",
        AppSettingsTab::Appearance => "Appearance",
        AppSettingsTab::Shortcuts => "Keyboard shortcuts",
    }
}

fn nav_item(
    id: &'static str,
    label: &'static str,
    selected: bool,
    tab: AppSettingsTab,
    cx: &mut Context<AppSettings>,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(10.))
        .py(px(7.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(selected, |this| this.bg(rgb(0x777777).opacity(0.18)))
        .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |state, cx| {
                    state.set_app_settings_tab(tab, cx);
                });
            }),
        )
        .child(div().text_sm().child(label))
}

fn general_page(chord: SubmitChord, muted: Hsla, app: Entity<AppState>) -> impl IntoElement {
    v_flex()
        .gap(px(12.))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Chat"),
        )
        .child(
            v_flex()
                .w_full()
                .rounded(px(12.))
                .border_1()
                .border_color(rgb(0x777777).opacity(0.24))
                .overflow_hidden()
                .child(choice_row(
                    "settings-send-enter",
                    "Enter to send",
                    "Shift+Enter inserts a newline",
                    chord == SubmitChord::Enter,
                    {
                        let app = app.clone();
                        move |cx| {
                            app.update(cx, |state, cx| {
                                state.set_submit_chord(SubmitChord::Enter, cx);
                            });
                        }
                    },
                ))
                .child(
                    div()
                        .h(px(1.))
                        .bg(rgb(0x777777).opacity(0.16)),
                )
                .child(choice_row(
                    "settings-send-cmd-enter",
                    "⌘Enter to send",
                    "Enter inserts a newline",
                    chord == SubmitChord::CommandEnter,
                    move |cx| {
                        app.update(cx, |state, cx| {
                            state.set_submit_chord(SubmitChord::CommandEnter, cx);
                        });
                    },
                )),
        )
}

fn choice_row(
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    selected: bool,
    on_pick: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .w_full()
        .px(px(16.))
        .py(px(14.))
        .gap(px(12.))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.08)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| on_pick(cx))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(div().text_sm().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x888888))
                        .child(subtitle),
                ),
        )
        .child(radio_dot(selected))
}

fn radio_dot(on: bool) -> impl IntoElement {
    div()
        .size(px(16.))
        .rounded_full()
        .border_1()
        .border_color(rgb(0x888888))
        .flex()
        .items_center()
        .justify_center()
        .when(on, |this| {
            this.child(
                div()
                    .size(px(8.))
                    .rounded_full()
                    .bg(rgb(0x1084FE)),
            )
        })
}

fn profile_page(
    name: String,
    email: String,
    muted: Hsla,
    app: Entity<AppState>,
) -> impl IntoElement {
    v_flex()
        .gap(px(16.))
        .child(
            v_flex()
                .w_full()
                .rounded(px(12.))
                .border_1()
                .border_color(rgb(0x777777).opacity(0.24))
                .px(px(16.))
                .py(px(14.))
                .gap(px(10.))
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(div().text_xs().text_color(muted).child("Name"))
                        .child(div().text_sm().child(name)),
                )
                .when(!email.is_empty(), |this| {
                    this.child(
                        v_flex()
                            .gap(px(2.))
                            .child(div().text_xs().text_color(muted).child("Email"))
                            .child(div().text_sm().child(email)),
                    )
                }),
        )
        .child(
            Button::new("settings-sign-out")
                .label("Sign out")
                .danger()
                .on_click(move |_, _, cx| {
                    app.update(cx, |state, cx| {
                        state.is_app_settings_open = false;
                        state.logout(cx);
                    });
                }),
        )
}

fn appearance_page(
    theme_mode: &str,
    muted: Hsla,
    foreground: Hsla,
    app: Entity<AppState>,
) -> impl IntoElement {
    v_flex()
        .gap(px(12.))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Theme"),
        )
        .child(
            h_flex()
                .gap(px(10.))
                .child(theme_chip(
                    "light",
                    "Light",
                    theme_mode,
                    foreground,
                    app.clone(),
                ))
                .child(theme_chip(
                    "dark",
                    "Dark",
                    theme_mode,
                    foreground,
                    app.clone(),
                ))
                .child(theme_chip("system", "System", theme_mode, foreground, app)),
        )
}

fn theme_chip(
    mode: &'static str,
    label: &'static str,
    current: &str,
    foreground: Hsla,
    app: Entity<AppState>,
) -> impl IntoElement {
    let selected = current == mode;
    let icon = match mode {
        "light" => "icons/sun.svg",
        "dark" => "icons/moon.svg",
        _ => "icons/system_theme.svg",
    };
    v_flex()
        .id(SharedString::from(format!("theme-{mode}")))
        .w(px(120.))
        .px(px(14.))
        .py(px(12.))
        .gap(px(8.))
        .rounded(px(12.))
        .border_1()
        .border_color(if selected {
            foreground
        } else {
            rgb(0x777777).opacity(0.3).into()
        })
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.1)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            app.update(cx, |state, cx| {
                state.set_theme_mode(mode, cx);
            });
        })
        .child(Icon::default().path(icon).size(px(16.)))
        .child(div().text_sm().child(label))
}

fn shortcuts_page(
    chord: SubmitChord,
    muted: Hsla,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    let send = match chord {
        SubmitChord::Enter => "Enter",
        SubmitChord::CommandEnter => "⌘Enter",
    };
    let newline = match chord {
        SubmitChord::Enter => "Shift+Enter",
        SubmitChord::CommandEnter => "Enter",
    };
    let groups: [(&str, &[(&str, &str)]); 3] = [
        (
            "Chat",
            &[
                ("New Bot", "⌘N"),
                ("Send message", send),
                ("Insert newline", newline),
            ],
        ),
        (
            "View",
            &[
                ("Toggle sidebar", "⌘B"),
                ("Toggle mini sidebar", "⌘⇧H"),
                ("Toggle agent settings", "⌘⇧B"),
                ("Toggle theme", "⌘T"),
                ("Toggle FPS", "⌘⇧F"),
            ],
        ),
        (
            "App",
            &[
                ("Settings", "⌘,"),
                ("Quit", "⌘Q"),
            ],
        ),
    ];

    v_flex()
        .gap(px(20.))
        .children(groups.into_iter().map(|(title, rows)| {
            v_flex()
                .gap(px(8.))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(title),
                )
                .child(
                    v_flex()
                        .w_full()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(theme.border)
                        .children(rows.iter().enumerate().map(|(i, (name, keys))| {
                            h_flex()
                                .id(SharedString::from(format!("shortcut-{title}-{i}")))
                                .w_full()
                                .px(px(16.))
                                .py(px(12.))
                                .justify_between()
                                .when(i + 1 < rows.len(), |this| {
                                    this.border_b_1().border_color(theme.border)
                                })
                                .child(div().text_sm().child(*name))
                                .child(
                                    div()
                                        .px(px(8.))
                                        .py(px(3.))
                                        .rounded(px(6.))
                                        .bg(rgb(0x777777).opacity(0.16))
                                        .text_xs()
                                        .child(*keys),
                                )
                        })),
                )
        }))
}
