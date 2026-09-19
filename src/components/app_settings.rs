use crate::actions::CloseSettings;
use crate::chrome::TITLE_BAR_H;
use crate::opengrok::LocalExecMode;
use crate::state::{AppSettingsTab, AppState, SubmitChord};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable as _, h_flex, v_flex};
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
        let (tab, chord, theme_mode, account_name, account_email, computers, bot_name, controls) = {
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
                state.computers.clone(),
                state.active_bot_name(),
                crate::components::computer::ComputerControls::from_state(state),
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
                                AppSettingsTab::Profile => {
                                    profile_page(account_name, account_email, muted, app.clone())
                                        .into_any_element()
                                }
                                AppSettingsTab::Appearance => appearance_page(
                                    &theme_mode,
                                    muted,
                                    theme.foreground,
                                    app.clone(),
                                )
                                .into_any_element(),
                                AppSettingsTab::Shortcuts => {
                                    shortcuts_page(chord, muted, &theme).into_any_element()
                                }
                                AppSettingsTab::Computer => {
                                    computer_page(computers, muted, app.clone(), cx)
                                        .into_any_element()
                                }
                                AppSettingsTab::Updates => {
                                    updates_page(&bot_name, &controls, muted, app.clone(), &theme)
                                        .into_any_element()
                                }
                                AppSettingsTab::Logins => logins_page(
                                    &app.read(cx).site_logins,
                                    app.read(cx).site_login_error.clone(),
                                    muted,
                                    app.clone(),
                                )
                                .into_any_element(),
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
            .pb(px(16.))
            .gap(px(4.))
            .child(div().id("app-settings-titlebar-spacer").h(px(TITLE_BAR_H)))
            .child(
                h_flex()
                    .id("app-settings-back")
                    .w_full()
                    .px(px(10.))
                    .py(px(12.))
                    .rounded(px(8.))
                    .items_center()
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
            .child(nav_item(
                "settings-tab-computer",
                "Computer",
                tab == AppSettingsTab::Computer,
                AppSettingsTab::Computer,
                cx,
            ))
            .child(nav_item(
                "settings-tab-updates",
                "Updates",
                tab == AppSettingsTab::Updates,
                AppSettingsTab::Updates,
                cx,
            ))
            .child(nav_item(
                "settings-tab-logins",
                "Logins",
                tab == AppSettingsTab::Logins,
                AppSettingsTab::Logins,
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
        AppSettingsTab::Computer => "Computer",
        AppSettingsTab::Updates => "Updates",
        AppSettingsTab::Logins => "Logins",
    }
}

fn logins_page(
    logins: &[crate::site_login::SiteLoginRecord],
    error: Option<String>,
    muted: Hsla,
    app: Entity<AppState>,
) -> impl IntoElement {
    v_flex()
        .id("settings-logins")
        .gap(px(12.))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Saved site logins on this Mac. Passwords stay in the OS keychain (not OpenGrok). Use saved login is offered only when a row here matches the site."),
        )
        .when_some(error, |this, error| {
            this.child(
                div()
                    .id("settings-logins-error")
                    .text_xs()
                    .text_color(rgb(0xcc4444))
                    .child(error),
            )
        })
        .child(if logins.is_empty() {
            div()
                .id("settings-logins-empty")
                .text_sm()
                .text_color(muted)
                .child("No saved logins yet.")
                .into_any_element()
        } else {
            let mut list = v_flex()
                .w_full()
                .rounded(px(12.))
                .border_1()
                .border_color(rgb(0x777777).opacity(0.24))
                .overflow_hidden();
            for (i, login) in logins.iter().enumerate() {
                if i > 0 {
                    list = list.child(div().h(px(1.)).bg(rgb(0x777777).opacity(0.16)));
                }
                let id = login.id.clone();
                list = list.child(
                    h_flex()
                        .id(format!("settings-login-row-{id}"))
                        .w_full()
                        .items_center()
                        .gap(px(16.))
                        .px(px(16.))
                        .py(px(12.))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(0.))
                                .gap(px(2.))
                                .child(div().text_sm().child(login.username.clone()))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(login.origin.clone()),
                                ),
                        )
                        .child(
                            Button::new(format!("settings-login-delete-{id}"))
                                .label("Delete")
                                .ghost()
                                .on_click({
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.delete_site_login(id.clone(), cx);
                                        });
                                    }
                                }),
                        ),
                );
            }
            list.into_any_element()
        })
}

/// The active bot's computer: Update (keeps files). Reset lives on the
/// Computer pane next to download — Settings no longer duplicates it.
fn updates_page(
    bot_name: &str,
    controls: &crate::components::computer::ComputerControls,
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    use crate::components::computer::{confirm_label, update_rest_label};
    let update_label = confirm_label(
        controls.updating,
        update_rest_label(controls.stale, controls.current),
    );
    let row = |title: String, detail: &'static str, button: Button| {
        h_flex()
            .w_full()
            .items_center()
            .gap(px(16.))
            .px(px(16.))
            .py(px(12.))
            .child(
                // `min_w(0)` lets the copy shrink and wrap instead of shoving the button out of
                // the card; the button never shrinks.
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(px(2.))
                    .child(div().text_sm().child(title))
                    .child(div().text_xs().text_color(muted).child(detail)),
            )
            .child(div().flex_shrink_0().child(button))
    };
    v_flex()
        .gap(px(12.))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!("{bot_name}'s Computer")),
        )
        .child(
            v_flex()
                .w_full()
                .rounded(px(12.))
                .border_1()
                .border_color(rgb(0x777777).opacity(0.24))
                .overflow_hidden()
                .child(row(
                    format!("Update {bot_name}'s Computer"),
                    "Rebuilds the computer on the newest image. Your files and logins stay, but installed apps and packages are removed.",
                    {
                        let button = Button::new("settings-computer-update")
                            .label(update_label)
                            .small()
                            .disabled(controls.update_disabled())
                            .on_click({
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.open_computer_confirm(crate::state::ComputerAction::Update, cx)
                                    });
                                }
                            });
                        if controls.stale { button.primary() } else { button }
                    },
                )),
        )
        .when_some(controls.error.clone(), |this, error| {
            this.child(div().text_xs().text_color(theme.danger).child(error))
        })
        .when(!controls.present, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("Open a bot's Computer pane first; these act on the active bot's computer."),
            )
        })
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
        .child(div().text_xs().text_color(muted).child("Chat"))
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
                .child(div().h(px(1.)).bg(rgb(0x777777).opacity(0.16)))
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
                .child(div().text_xs().text_color(rgb(0x888888)).child(subtitle)),
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
            this.child(div().size(px(8.)).rounded_full().bg(rgb(0x1084FE)))
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
        .child(div().text_xs().text_color(muted).child("Theme"))
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

fn computer_page(
    computers: Vec<crate::opengrok::ConnectedComputer>,
    muted: Hsla,
    app: Entity<AppState>,
    cx: &App,
) -> impl IntoElement {
    let show_route = app.read(cx).show_route_traffic_in_user_settings();
    let page = v_flex()
        .gap(px(12.))
        .when(show_route, |this| {
            this.child(settings_route_traffic_row(app.clone(), muted, cx))
        })
        .child(div().text_xs().text_color(muted).child("This Mac"))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(
                    "Local-exec enrolment and policy. Each bot's screen and image updates are on that bot's Computer pane.",
                ),
        );
    if computers.is_empty() {
        return page.child(
            div()
                .text_sm()
                .text_color(muted)
                .child("No computers yet. Stay signed in here and NativeChat enrols this Mac."),
        );
    }
    let mut card = v_flex()
        .w_full()
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(0x777777).opacity(0.24))
        .overflow_hidden();
    for (i, computer) in computers.into_iter().enumerate() {
        if i > 0 {
            card = card.child(div().h(px(1.)).bg(rgb(0x777777).opacity(0.16)));
        }
        card = card.child(computer_row(computer, muted, app.clone()));
    }
    page.child(card)
}

fn settings_route_traffic_row(app: Entity<AppState>, muted: Hsla, cx: &App) -> impl IntoElement {
    let enabled = app.read(cx).egress_tunnel_enabled;
    let description = if enabled {
        "New connections from Bots that share this computer go out through this desktop."
    } else {
        "Route web traffic from Bots that share this computer out through this desktop instead of the cloud. Applies to new connections."
    };
    v_flex()
        .id("route-traffic-this-computer")
        .w_full()
        .px(px(16.))
        .py(px(14.))
        .gap(px(6.))
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(0x777777).opacity(0.24))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .gap(px(2.))
                        .child(div().text_sm().child("Route traffic through this computer"))
                        .child(div().text_xs().text_color(muted).child(description)),
                )
                .child(
                    Switch::new("egress-tunnel-enabled")
                        .checked(enabled)
                        .on_click({
                            let app = app.clone();
                            move |checked, _, cx| {
                                app.update(cx, |state, cx| {
                                    state.set_egress_tunnel_enabled(*checked, cx);
                                });
                            }
                        }),
                ),
        )
}

fn computer_row(
    computer: crate::opengrok::ConnectedComputer,
    muted: Hsla,
    app: Entity<AppState>,
) -> impl IntoElement {
    let heading = if computer.this_machine {
        "Current computer"
    } else {
        "Computer"
    };
    let hint = if !computer.online {
        "Offline. Open NativeChat on this computer while it is online to run commands."
    } else if computer.this_machine {
        "This is the computer you are using now"
    } else {
        "Online. Agents can run commands here per the policy below."
    };
    let subtitle = if !computer.online {
        "Local execution needs this computer connected."
    } else {
        match computer.mode {
            LocalExecMode::Always => "Agents run commands on this computer without asking.",
            LocalExecMode::Ask => "Agents ask before every command on this computer.",
            LocalExecMode::Never => "Agents cannot run commands on this computer.",
        }
    };
    let status = if computer.online { "Online" } else { "Offline" };
    v_flex()
        .w_full()
        .px(px(16.))
        .py(px(14.))
        .gap(px(14.))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .gap(px(2.))
                        .child(
                            h_flex()
                                .gap(px(8.))
                                .items_center()
                                .child(div().text_sm().child(heading))
                                .child(div().text_xs().text_color(muted).child(status)),
                        )
                        .child(div().text_xs().text_color(muted).child(hint)),
                )
                .child(
                    div()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(0x777777).opacity(0.28))
                        .text_xs()
                        .child(computer.label.clone()),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .gap(px(2.))
                        .child(div().text_sm().child("Execution on this computer"))
                        .child(div().text_xs().text_color(muted).child(subtitle)),
                )
                .child(exec_mode_picker(
                    computer.machine_id.clone(),
                    computer.mode,
                    app,
                )),
        )
}

fn exec_mode_picker(
    machine_id: String,
    current: LocalExecMode,
    app: Entity<AppState>,
) -> impl IntoElement {
    Button::new(ElementId::Name(format!("exec-mode-{machine_id}").into()))
        .label(current.label())
        .ghost()
        .compact()
        .icon(IconName::ChevronDown)
        .dropdown_menu({
            let machine_id = machine_id.clone();
            move |menu, _, _| {
                menu.item(exec_menu_item(
                    machine_id.clone(),
                    LocalExecMode::Always,
                    current,
                    app.clone(),
                ))
                .item(exec_menu_item(
                    machine_id.clone(),
                    LocalExecMode::Ask,
                    current,
                    app.clone(),
                ))
                .item(exec_menu_item(
                    machine_id.clone(),
                    LocalExecMode::Never,
                    current,
                    app.clone(),
                ))
            }
        })
}

fn exec_menu_item(
    machine_id: String,
    mode: LocalExecMode,
    current: LocalExecMode,
    app: Entity<AppState>,
) -> PopupMenuItem {
    PopupMenuItem::new(mode.label())
        .checked(mode == current)
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            app.update(cx, |state, cx| {
                state.set_computer_exec_mode(machine_id.clone(), mode, cx);
            });
        })
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
                ("Command palette", "⌘K"),
                ("Next palette tab", "Tab"),
                ("Previous palette tab", "⇧Tab"),
                ("Palette down", "↓ / Ctrl+N"),
                ("Palette up", "↑ / Ctrl+P"),
                ("Focus chat input", "⌘L"),
                ("Close palette / finder", "Esc"),
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
                ("Toggle agent screen", "⌘⇧M"),
                ("Back", "⌘["),
                ("Forward", "⌘]"),
                ("Toggle theme", "⌘T"),
                ("Toggle FPS", "⌘⇧F"),
            ],
        ),
        ("App", &[("Settings", "⌘,"), ("Quit", "⌘Q")]),
    ];

    v_flex()
        .gap(px(20.))
        .children(groups.into_iter().map(|(title, rows)| {
            v_flex()
                .gap(px(8.))
                .child(div().text_xs().text_color(muted).child(title))
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
