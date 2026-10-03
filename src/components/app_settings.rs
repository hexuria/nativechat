use crate::actions::CloseSettings;
use crate::chrome::TITLE_BAR_H;
use crate::components::computers::{self, ComputerCard, OpencodexFields};
use crate::components::default_models::DefaultModels;
use crate::components::logins::LoginsPage;
use crate::components::skills::SkillsPage;
use crate::components::switch::Switch;
use crate::opengrok::LocalExecMode;
use crate::send_policy::OnSend;
use crate::state::{AppSettingsTab, AppState, LocalRuleRow, LocalRules, RuleKind, SubmitChord};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct AppSettings {
    state: Entity<AppState>,
    /// Settings → Logins, made on the first render of that tab (its fields need a window).
    logins: Option<Entity<LoginsPage>>,
    /// Settings → Skills, made on the first render of that tab, for the same reason.
    skills: Option<Entity<SkillsPage>>,
    /// This computer's opencodex address and key, on its card in Settings → Computer, made on the
    /// first render of that tab, for the same reason.
    opencodex: Option<Entity<OpencodexFields>>,
    /// Settings → General's Default models, made on the first render of that tab: its pickers
    /// need a window too.
    default_models: Option<Entity<DefaultModels>>,
}

impl AppSettings {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        Self {
            state,
            logins: None,
            skills: None,
            opencodex: None,
            default_models: None,
        }
    }

    fn logins_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<LoginsPage> {
        if let Some(page) = &self.logins {
            return page.clone();
        }
        let state = self.state.clone();
        let page = cx.new(|cx| LoginsPage::new(window, state, cx));
        self.logins = Some(page.clone());
        page
    }

    fn skills_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<SkillsPage> {
        if let Some(page) = &self.skills {
            return page.clone();
        }
        let state = self.state.clone();
        let page = cx.new(|cx| SkillsPage::new(window, state, cx));
        self.skills = Some(page.clone());
        page
    }

    fn opencodex_fields(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<OpencodexFields> {
        if let Some(fields) = &self.opencodex {
            return fields.clone();
        }
        let state = self.state.clone();
        let fields = cx.new(|cx| OpencodexFields::new(window, state, cx));
        self.opencodex = Some(fields.clone());
        fields
    }

    fn default_models(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DefaultModels> {
        if let Some(section) = &self.default_models {
            return section.clone();
        }
        let state = self.state.clone();
        let section = cx.new(|cx| DefaultModels::new(window, state, cx));
        self.default_models = Some(section.clone());
        section
    }
}

impl Render for AppSettings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let (
            tab,
            chord,
            on_send,
            show_turn_timing,
            theme_mode,
            account_name,
            account_email,
            cards,
            bot_name,
            controls,
        ) = {
            let state = self.state.read(cx);
            let (name, email) = state
                .account
                .as_ref()
                .map(|a| (a.display_name(), a.email.clone()))
                .unwrap_or_else(|| ("Not signed in".into(), String::new()));
            (
                state.app_settings_tab,
                state.submit_chord,
                state.on_send,
                state.show_turn_timing,
                state.theme_mode.clone(),
                name,
                email,
                computers::cards(state),
                state.active_bot_name(),
                crate::components::computer::ComputerControls::from_state(state),
            )
        };
        let app = self.state.clone();

        // Every tab but Logins and Skills is a titled column of cards. Those two are panes edge
        // to edge, like a passwords app: each takes the whole body and scrolls on its own.
        let cards: Option<AnyElement> = match tab {
            AppSettingsTab::General => Some(
                general_page(
                    self.default_models(window, cx),
                    chord,
                    on_send,
                    show_turn_timing,
                    muted,
                    app.clone(),
                )
                .into_any_element(),
            ),
            AppSettingsTab::Profile => Some(
                profile_page(account_name, account_email, muted, app.clone()).into_any_element(),
            ),
            AppSettingsTab::Appearance => Some(
                appearance_page(&theme_mode, muted, theme.foreground, app.clone())
                    .into_any_element(),
            ),
            AppSettingsTab::Shortcuts => {
                Some(shortcuts_page(chord, muted, &theme).into_any_element())
            }
            AppSettingsTab::Computer => {
                let fields = self.opencodex_fields(window, cx);
                Some(computer_page(cards, fields, muted, app.clone(), cx).into_any_element())
            }
            AppSettingsTab::Updates => Some(
                updates_page(&bot_name, &controls, muted, app.clone(), &theme).into_any_element(),
            ),
            AppSettingsTab::Connections => Some(
                crate::components::connections::connections_page(app.clone(), cx)
                    .into_any_element(),
            ),
            AppSettingsTab::Logins | AppSettingsTab::Skills => None,
        };
        let body = match cards {
            Some(page) => div()
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
                        .child(page),
                )
                .into_any_element(),
            None => {
                let pane: AnyElement = if tab == AppSettingsTab::Skills {
                    self.skills_page(window, cx).into_any_element()
                } else {
                    self.logins_page(window, cx).into_any_element()
                };
                div()
                    .id("app-settings-body")
                    .flex_1()
                    .h_full()
                    .min_w(px(0.))
                    .child(pane)
                    .into_any_element()
            }
        };

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
            .child(body)
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
            .child(nav_item(
                crate::components::connections::SETTINGS_TAB,
                "Connections",
                tab == AppSettingsTab::Connections,
                AppSettingsTab::Connections,
                cx,
            ))
            .child(nav_item(
                "settings-tab-skills",
                "Skills",
                tab == AppSettingsTab::Skills,
                AppSettingsTab::Skills,
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
        AppSettingsTab::Connections => "Connections",
        AppSettingsTab::Skills => "Skills",
    }
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
        .debug_selector(move || id.to_string())
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

/// Settings → General: Default models first, where a Bot's model starts out, then how a message
/// is sent, what a send does while the coworker is busy, and the turn timing switch.
fn general_page(
    default_models: Entity<DefaultModels>,
    chord: SubmitChord,
    on_send: OnSend,
    show_turn_timing: bool,
    muted: Hsla,
    app: Entity<AppState>,
) -> impl IntoElement {
    let card = || {
        v_flex()
            .w_full()
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(0x777777).opacity(0.24))
            .overflow_hidden()
    };
    let divider = || div().h(px(1.)).bg(rgb(0x777777).opacity(0.16));
    v_flex()
        .gap(px(12.))
        .child(default_models)
        .child(div().text_xs().text_color(muted).child("Chat"))
        .child(
            card()
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
                .child(divider())
                .child(choice_row(
                    "settings-send-cmd-enter",
                    "⌘Enter to send",
                    "Enter inserts a newline",
                    chord == SubmitChord::CommandEnter,
                    {
                        let app = app.clone();
                        move |cx| {
                            app.update(cx, |state, cx| {
                                state.set_submit_chord(SubmitChord::CommandEnter, cx);
                            });
                        }
                    },
                )),
        )
        // What a plain send does while the coworker is mid-turn. A card waiting on the person
        // is not this case: a send over it always steers, whatever is picked here.
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("While the coworker is busy"),
        )
        .child(
            card()
                .child(choice_row(
                    "settings-on-send-queue",
                    "Queue it",
                    "Sends when the current turn ends. ⌘⇧Enter sends now.",
                    on_send == OnSend::Queue,
                    {
                        let app = app.clone();
                        move |cx| {
                            app.update(cx, |state, cx| {
                                state.set_on_send(OnSend::Queue, cx);
                            });
                        }
                    },
                ))
                .child(divider())
                .child(choice_row(
                    "settings-on-send-steer",
                    "Interrupt and send",
                    "Stops the current turn at its next step, then sends.",
                    on_send == OnSend::Steer,
                    {
                        let app = app.clone();
                        move |cx| {
                            app.update(cx, |state, cx| {
                                state.set_on_send(OnSend::Steer, cx);
                            });
                        }
                    },
                )),
        )
        .child(div().text_xs().text_color(muted).child("Debug"))
        .child(
            card().child(
                h_flex()
                    .id("settings-show-turn-timing")
                    .w_full()
                    .px(px(16.))
                    .py(px(14.))
                    .gap(px(12.))
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(px(2.))
                            .child(div().text_sm().child("Show turn timing"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x888888))
                                    .child("Phases of each assistant run: model, each tool, auto-review. Off for demos; the timestamp still shows how long a turn took."),
                            ),
                    )
                    .child(
                        Switch::new("show-turn-timing")
                            .checked(show_turn_timing)
                            .on_click({
                                let app = app.clone();
                                move |checked, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.set_show_turn_timing(*checked, cx);
                                    });
                                }
                            }),
                    ),
            ),
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
        .debug_selector(move || id.to_string())
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

/// Settings → Computer: Route traffic and its network choice where they apply, then "Your
/// computers", one card for each computer the person has enrolled, this one first. Each card is
/// the computer's Relay your plan switch with where its relay stands (`components::computers`), and
/// below it that computer's local-exec mode; this computer's alone also holds opencodex's address
/// and key for its relay and its standing rules.
fn computer_page(
    cards: Vec<ComputerCard>,
    fields: Entity<OpencodexFields>,
    muted: Hsla,
    app: Entity<AppState>,
    cx: &App,
) -> impl IntoElement {
    let show_route = app.read(cx).show_route_traffic_in_user_settings();
    let egress_policy = app
        .read(cx)
        .show_egress_policy_in_user_settings()
        .then(|| app.read(cx).egress_policy())
        .flatten();
    let stopped = app
        .read(cx)
        .local_exec_stopped
        .as_ref()
        .map(crate::opengrok::LocalExecStopped::sentence);
    let page = v_flex()
        .gap(px(12.))
        .when(show_route, |this| {
            this.child(settings_route_traffic_row(app.clone(), muted, cx))
        })
        .when_some(egress_policy, |this, current| {
            let org = app.read(cx).computer_is_org_shared();
            this.child(settings_egress_policy_row(app.clone(), current, org, muted))
        })
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(computers::SECTION_TITLE),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(computers::SECTION_INTRO),
        )
        // Why this Mac stopped running commands for the server by itself, until the next
        // sign-in. Its row only says Offline, which reads as NativeChat not being open here.
        .when_some(stopped, |this, stopped| {
            this.child(div().text_xs().text_color(cx.theme().danger).child(stopped))
        });
    if cards.is_empty() {
        return page.child(
            div()
                .text_sm()
                .text_color(muted)
                .child(computers::NO_COMPUTERS),
        );
    }
    let rules = app.read(cx).this_mac_rules();
    let theme = cx.theme();
    page.children(cards.into_iter().map(|card| {
        // This Mac's rules go in its own card and no other: they were read for its machine, and
        // the app keeps no other machine's. So do opencodex's address and key, which are this
        // computer's.
        let rules = rules
            .filter(|rules| card.this_computer && rules.machine_id == card.machine_id)
            .map(|rules| local_rules_block(rules, theme, app.clone()).into_any_element());
        let fields = card
            .this_computer
            .then(|| fields.clone().into_any_element());
        computer_card(card, fields, rules, muted, theme, app.clone())
    }))
}

/// What this Mac's rules say when there are none.
pub(crate) const NO_LOCAL_RULES: &str = "No commands are always allowed or never allowed yet. \
     Always allow or Never on a command's card keeps one here.";

/// Above this Mac's rules: where they come from, and when they count. The gate reads them only
/// while the machine is on Ask (opengrok-server `decide`): Always allow skips both lists and
/// Never allow refuses everything, so a rule under either is kept but not read.
const LOCAL_RULES_NOTE: &str = "Always allow and Never on a command's card keep the command \
     here. They apply only while this computer is set to Ask every time.";

/// The heading over each of this Mac's two lists.
pub(crate) fn rule_list_title(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::Allow => "Always allowed",
        RuleKind::Deny => "Never allowed",
    }
}

/// What a rule's Remove says, and what it says while the server has it.
pub(crate) fn remove_label(removing: bool) -> &'static str {
    if removing { "Removing…" } else { "Remove" }
}

/// The line under an allow the gate can never match: that it is not in effect, then the
/// server's reason as `inert` gives it (opengrok-server `standing_rule_refusal`, #246).
///
/// The reason is given whole. It is the rule endpoint's own sentence for what an allow may be,
/// read here by somebody deciding whether to take the rule off, and the long one is the list
/// of what a plain command may not contain.
pub(crate) fn not_in_effect_line(reason: &str) -> String {
    let reason = reason.trim();
    if reason.is_empty() {
        "Not in effect.".to_string()
    } else if reason.ends_with(['.', '!', '?']) {
        format!("Not in effect: {reason}")
    } else {
        format!("Not in effect: {reason}.")
    }
}

/// The Remove on this Mac's `n`th rule of `kind`. The same id is the one gpui-agent clicks, so
/// the control a driver presses is the control a person presses.
pub(crate) fn local_rule_remove_id(kind: RuleKind, n: usize) -> String {
    format!("settings-local-rule-remove-{}-{n}", kind.word())
}

/// This Mac's standing rules, under its mode: the commands a local-shell card's Always allow
/// and Never kept, in two lists, each with a Remove.
fn local_rules_block(
    rules: &LocalRules,
    theme: &gpui_kit::component::Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let mut block = v_flex().w_full().gap(px(10.));
    if let Some(error) = &rules.error {
        block = block.child(
            div()
                .text_xs()
                .text_color(theme.danger)
                .child(error.clone()),
        );
    }
    if rules.is_empty() {
        // A list that could not be read is not a list with nothing on it.
        if rules.listed {
            block = block.child(div().text_xs().text_color(muted).child(NO_LOCAL_RULES));
        }
        return block;
    }
    block = block.child(div().text_xs().text_color(muted).child(LOCAL_RULES_NOTE));
    for kind in RuleKind::ALL {
        let rows = rules.rows(kind);
        if rows.is_empty() {
            continue;
        }
        let mut list = v_flex()
            .w_full()
            .gap(px(6.))
            .child(div().text_sm().child(rule_list_title(kind)));
        for (n, row) in rows.iter().enumerate() {
            list = list.child(local_rule_row(kind, n, row, rules, theme, app.clone()));
        }
        block = block.child(list);
    }
    block
}

/// One rule: the command as the server keeps it, whether the gate can ever match it, why its
/// last Remove did not go through, and Remove.
fn local_rule_row(
    kind: RuleKind,
    n: usize,
    row: &LocalRuleRow,
    rules: &LocalRules,
    theme: &gpui_kit::component::Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let removing = rules.is_removing(kind, &row.pattern);
    let not_removed = rules.not_removed(kind, &row.pattern).map(str::to_string);
    let pattern = row.pattern.clone();
    h_flex()
        .w_full()
        .items_start()
        .justify_between()
        .gap(px(12.))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(4.))
                .child(
                    div()
                        .text_sm()
                        .font_family(theme.mono_font_family.clone())
                        // An allow that is never read is drawn like one: there, and dimmed.
                        .when(row.inert.is_some(), |this| this.text_color(muted))
                        .child(row.pattern.clone()),
                )
                .when_some(
                    row.inert.as_deref().map(not_in_effect_line),
                    |this, line| this.child(div().text_xs().text_color(muted).child(line)),
                )
                .when_some(not_removed, |this, why| {
                    this.child(div().text_xs().text_color(theme.danger).child(why))
                }),
        )
        .child(
            div().flex_shrink_0().child(
                Button::new(ElementId::Name(local_rule_remove_id(kind, n).into()))
                    .label(remove_label(removing))
                    .ghost()
                    .small()
                    .disabled(removing)
                    .on_click(move |_, _, cx| {
                        app.update(cx, |state, cx| {
                            state.remove_local_rule(kind, pattern.clone(), cx);
                        });
                    }),
            ),
        )
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

/// The standing answer to the tunnel's card for the computer the open bot shares: sits under
/// Route traffic, because it only matters while traffic is routed through this desktop.
fn settings_egress_policy_row(
    app: Entity<AppState>,
    current: LocalExecMode,
    org: bool,
    muted: Hsla,
) -> impl IntoElement {
    let (title, description) = if org {
        (
            "Use your network from the organization's computer",
            "Whether Bots on the organization's shared computer may reach the web through this desktop without asking each time. Set by the organization's admin for every member.",
        )
    } else {
        (
            "Use your network from this computer",
            "Whether Bots on this computer may reach the web through this desktop without asking each time. Never allow keeps their browser off while traffic is routed here.",
        )
    };
    h_flex()
        .id("egress-policy-row")
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .px(px(16.))
        .py(px(14.))
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(0x777777).opacity(0.24))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(div().text_sm().child(title))
                .child(div().text_xs().text_color(muted).child(description)),
        )
        .child(egress_policy_picker(current, app))
}

/// The same three-way dropdown this Mac's local-exec policy uses, for a computer's network use.
pub(crate) fn egress_policy_picker(
    current: LocalExecMode,
    app: Entity<AppState>,
) -> impl IntoElement {
    Button::new("egress-policy-menu")
        .label(current.label())
        .ghost()
        .compact()
        .icon(IconName::ChevronDown)
        .dropdown_menu(move |menu, _, _| {
            menu.item(egress_menu_item(
                LocalExecMode::Always,
                current,
                app.clone(),
            ))
            .item(egress_menu_item(LocalExecMode::Ask, current, app.clone()))
            .item(egress_menu_item(LocalExecMode::Never, current, app.clone()))
        })
}

fn egress_menu_item(
    mode: LocalExecMode,
    current: LocalExecMode,
    app: Entity<AppState>,
) -> PopupMenuItem {
    PopupMenuItem::new(mode.label())
        .checked(mode == current)
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            app.update(cx, |state, cx| {
                state.set_egress_policy(mode, cx);
            });
        })
}

/// One computer's card: its Relay your plan section (`components::computers::relay_section`), and
/// under it what commands may run on it: its mode. `fields` is this computer's opencodex address
/// and key, and `rules` its standing rules, drawn under the mode; only this computer's card has
/// either.
fn computer_card(
    card: ComputerCard,
    fields: Option<AnyElement>,
    rules: Option<AnyElement>,
    muted: Hsla,
    theme: &gpui_kit::component::Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let subtitle = if !card.online {
        "Local execution needs this computer connected."
    } else {
        match card.mode {
            LocalExecMode::Always => "Agents run commands on this computer without asking.",
            LocalExecMode::Ask => "Agents ask before every command on this computer.",
            LocalExecMode::Never => "Agents cannot run commands on this computer.",
        }
    };
    let id = computers::card_id(&card.machine_id);
    v_flex()
        .id(ElementId::Name(id.clone().into()))
        .debug_selector(move || id)
        .w_full()
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(0x777777).opacity(0.24))
        .overflow_hidden()
        .child(computers::relay_section(&card, fields, theme, app.clone()))
        .child(div().h(px(1.)).bg(rgb(0x777777).opacity(0.16)))
        .child(
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
                                .child(div().text_sm().child("Execution on this computer"))
                                .child(div().text_xs().text_color(muted).child(subtitle)),
                        )
                        .child(exec_mode_picker(card.machine_id.clone(), card.mode, app)),
                )
                .when_some(rules, |this, rules| this.child(rules)),
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
                ("Send now, interrupting the turn", "⌘⇧Enter"),
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

#[cfg(test)]
mod tests {
    use super::not_in_effect_line;

    /// The window's own drawing of `id`, after a redraw: its bounds, or none.
    fn drawn(
        cx: &mut gpui_kit::VisualTestContext,
        id: &str,
    ) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.debug_bounds(Box::leak(id.to_string().into_boxed_str()))
    }

    /// `inner` lies within `outer`.
    fn within(
        inner: gpui_kit::Bounds<gpui_kit::Pixels>,
        outer: gpui_kit::Bounds<gpui_kit::Pixels>,
    ) -> bool {
        outer.left() <= inner.left()
            && inner.right() <= outer.right()
            && outer.top() <= inner.top()
            && inner.bottom() <= outer.bottom()
    }

    /// The account's setting as a server that knows the relay answers it.
    fn relay_setting() -> crate::opengrok::InferenceSource {
        serde_json::from_value(serde_json::json!({
            "kind": "local_proxy", "via": "mac", "baseUrl": null, "localModel": null,
            "healthy": true, "hasApiKey": false,
            "relay": {"connected": false, "machineId": null, "machineLabel": null,
                      "localModel": null}
        }))
        .expect("a setting")
    }

    /// A computer as the roster holds it.
    fn computer(
        id: &str,
        this_machine: bool,
        relay_enabled: bool,
        relaying: bool,
        online: bool,
    ) -> crate::opengrok::ConnectedComputer {
        crate::opengrok::ConnectedComputer {
            machine_id: id.into(),
            label: format!("NativeChat on {id}"),
            mode: crate::opengrok::LocalExecMode::Ask,
            this_machine,
            online,
            relay_enabled,
            relaying,
        }
    }

    /// Settings → General opens with Default models, over Chat: the default for new Bots and the
    /// Relay-off fallback, each in the same card a Bot's model is picked in, drawn dead while the
    /// server keeps neither. Settings → Computer holds neither.
    #[gpui_kit::test]
    fn general_opens_with_the_default_models_and_computer_holds_none(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::state::{AppSettingsTab, AppState};
        use gpui_kit::AppContext as _;
        cx.update(gpui_kit::init);
        let (settings, cx) = cx.add_window_view(|_, cx| {
            let state = cx.new(|_| {
                let mut state = AppState::new();
                state.is_app_settings_open = true;
                state.app_settings_tab = AppSettingsTab::General;
                state
            });
            super::AppSettings::new(state, cx)
        });
        let models = drawn(cx, "settings-default-models").expect("Default models is on General");
        let chat = drawn(cx, "settings-send-enter").expect("so is Chat");
        assert!(models.bottom() <= chat.top(), "{models:?} over {chat:?}");
        let new_bots = drawn(cx, "settings-new-bots-card").expect("the default for new Bots");
        let fallback = drawn(cx, "settings-plan-fallback-card").expect("the Relay-off fallback");
        for card in [new_bots, fallback] {
            assert!(
                models.top() <= card.top() && card.bottom() <= models.bottom(),
                "{card:?} in {models:?}"
            );
        }
        assert!(
            new_bots.bottom() <= fallback.top(),
            "the default for new Bots first"
        );

        settings.update(cx, |settings, cx| {
            settings.state.update(cx, |state, cx| {
                state.app_settings_tab = AppSettingsTab::Computer;
                cx.notify();
            });
        });
        assert!(drawn(cx, "settings-default-models").is_none());
        for card in ["settings-new-bots-card", "settings-plan-fallback-card"] {
            assert!(drawn(cx, card).is_none(), "{card} is not on Computer");
        }
    }

    /// The Relay tab is gone: Settings' list of pages has General, Profile, Appearance, Keyboard
    /// shortcuts, Computer, Updates, Logins, Connections and Skills, and nothing at the id the Relay
    /// tab had. The relay is on Computer, in each computer's card.
    #[gpui_kit::test]
    fn the_relay_tab_is_gone_from_settings(cx: &mut gpui_kit::TestAppContext) {
        use crate::state::{AppSettingsTab, AppState};
        use gpui_kit::AppContext as _;
        cx.update(gpui_kit::init);
        let (_, cx) = cx.add_window_view(|_, cx| {
            let state = cx.new(|_| {
                let mut state = AppState::new();
                state.is_app_settings_open = true;
                state.app_settings_tab = AppSettingsTab::General;
                state
            });
            super::AppSettings::new(state, cx)
        });
        for tab in [
            "general",
            "profile",
            "appearance",
            "shortcuts",
            "computer",
            "updates",
            "logins",
            "connections",
            "skills",
        ] {
            let id = format!("settings-tab-{tab}");
            assert!(drawn(cx, &id).is_some(), "{id} is in the list of pages");
        }
        assert!(
            drawn(cx, "settings-tab-reply-source").is_none(),
            "the Relay tab is gone"
        );
    }

    /// Settings → Computer is "Your computers": one card for each enrolled computer, this one
    /// first, each with its label, its Relay your plan switch and where its relay stands, and the
    /// three lines come out as the state says them: relaying, on but asleep (on, not relaying, and
    /// the server cannot reach it), and not relaying (off). Only this computer's card has the
    /// "This computer" pill and what its relay needs of this computer, opencodex's address and key
    /// with Save; Remove key there is the card's own control.
    #[gpui_kit::test]
    fn computer_shows_your_computers_and_only_this_ones_card_holds_the_opencodex_fields(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::state::{AppSettingsTab, AppState, ReplySourceRead};
        use gpui_kit::{AppContext as _, Modifiers, px, size};
        cx.update(gpui_kit::init);
        let (settings, cx) = cx.add_window_view(|_, cx| {
            let state = cx.new(|_| {
                let mut state = AppState::new();
                state.is_app_settings_open = true;
                state.app_settings_tab = AppSettingsTab::Computer;
                state.local_exec_machine_id = Some("mac_1".into());
                state.computers = vec![
                    computer("mac_1", true, true, true, true),
                    computer("mac_2", false, true, false, false),
                    computer("mac_3", false, false, false, true),
                ];
                state.reply_source.kept = Some(ReplySourceRead::Read(relay_setting()));
                state.relay_mac.has_key = true;
                state
            });
            super::AppSettings::new(state, cx)
        });
        cx.simulate_resize(size(px(1200.), px(2400.)));

        let cards: Vec<_> = ["mac_1", "mac_2", "mac_3"]
            .into_iter()
            .map(|id| {
                drawn(cx, &format!("settings-computer-{id}"))
                    .unwrap_or_else(|| panic!("a card for {id}"))
            })
            .collect();
        assert!(
            cards[0].bottom() <= cards[1].top() && cards[1].bottom() <= cards[2].top(),
            "this computer first, then the others as listed, one card each: {cards:?}"
        );
        for (id, card, says) in [
            ("mac_1", cards[0], "relaying"),
            ("mac_2", cards[1], "asleep"),
            ("mac_3", cards[2], "not-relaying"),
        ] {
            let switch = drawn(cx, &format!("settings-computer-{id}-relay"))
                .unwrap_or_else(|| panic!("{id} has its switch"));
            assert!(within(switch, card), "{id}'s switch is in its card");
            let status = drawn(cx, &format!("settings-computer-{id}-status-says-{says}"))
                .unwrap_or_else(|| panic!("{id} says {says}"));
            assert!(within(status, card), "{id}'s status is in its card");
            assert!(
                drawn(cx, &format!("settings-computer-{id}-error")).is_none(),
                "nothing refused, nothing said"
            );
        }

        let pill = drawn(cx, "settings-computer-mac_1-pill").expect("This computer");
        assert!(within(pill, cards[0]));
        for other in ["mac_2", "mac_3"] {
            assert!(drawn(cx, &format!("settings-computer-{other}-pill")).is_none());
        }
        for field in [
            "settings-relay-addr",
            "settings-relay-key",
            "settings-relay-key-remove",
            "settings-reply-source-save",
        ] {
            let bounds = drawn(cx, field).unwrap_or_else(|| panic!("{field} is on the page"));
            assert!(
                within(bounds, cards[0]),
                "{field} is in this computer's card"
            );
            for other in &cards[1..] {
                assert!(!within(bounds, *other), "{field} is in another card");
            }
        }
        // Nothing of the old page: no section, no card of its own, no switch of its own.
        for gone in [
            "settings-reply-source",
            "settings-relay",
            "settings-relay-switch",
            "settings-relay-status",
        ] {
            assert!(drawn(cx, gone).is_none(), "{gone} is gone");
        }

        // Remove key is this computer's card's own control, and takes a click.
        let remove = drawn(cx, "settings-relay-key-remove").unwrap();
        cx.simulate_mouse_move(remove.center(), None, Modifiers::none());
        cx.simulate_click(remove.center(), Modifiers::none());
        assert!(settings.update(cx, |settings, cx| {
            settings.state.read(cx).reply_source.relay_remove_key
        }));
    }

    /// A card's switch is the computer's own, and takes a click from this window whichever
    /// computer it is: the click puts the switch where it asked, with the server, and the card
    /// takes no second click meanwhile. Another card's is free.
    #[gpui_kit::test]
    fn a_click_on_a_cards_switch_sends_that_computers_own_switch(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::state::{AppSettingsTab, AppState, AuthStatus, ReplySourceRead};
        use gpui_kit::{AppContext as _, Modifiers, px, size};
        cx.update(gpui_kit::init);
        // A runtime that is entered and never driven: the request for the switch stays with the
        // server, which is where this test wants it. It outlives the test body, since the window's
        // tasks are polled again as the test ends.
        let runtime: &'static tokio::runtime::Runtime = Box::leak(Box::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime"),
        ));
        std::mem::forget(runtime.enter());
        let (settings, cx) = cx.add_window_view(|_, cx| {
            let state = cx.new(|_| {
                let mut state = AppState::new();
                state.opengrok = Some(
                    crate::opengrok::OpenGrokClient::new("http://127.0.0.1:9").expect("a URL"),
                );
                state.account = Some(
                    serde_json::from_value(
                        serde_json::json!({ "id": "acc_1", "email": "ada@example.com" }),
                    )
                    .expect("an account"),
                );
                state.auth_status = AuthStatus::SignedIn;
                state.is_app_settings_open = true;
                state.app_settings_tab = AppSettingsTab::Computer;
                state.local_exec_machine_id = Some("mac_1".into());
                state.computers = vec![
                    computer("mac_1", true, true, true, true),
                    computer("mac_2", false, false, false, true),
                    computer("mac_3", false, true, false, true),
                ];
                state.reply_source.kept = Some(ReplySourceRead::Read(relay_setting()));
                state
            });
            super::AppSettings::new(state, cx)
        });
        cx.simulate_resize(size(px(1200.), px(2400.)));
        let asked = |settings: &gpui_kit::Entity<super::AppSettings>,
                     cx: &mut gpui_kit::VisualTestContext,
                     id: &str| {
            settings.update(cx, |settings, cx| {
                settings
                    .state
                    .read(cx)
                    .computer_relay_switch(id)
                    .map(|switch| switch.on)
            })
        };
        let press = |cx: &mut gpui_kit::VisualTestContext, id: &str| {
            let switch = drawn(cx, &format!("settings-computer-{id}-relay")).unwrap();
            cx.simulate_mouse_move(switch.center(), None, Modifiers::none());
            cx.simulate_click(switch.center(), Modifiers::none());
        };
        assert_eq!(asked(&settings, cx, "mac_2"), None);
        // Off to on for mac_2, and on to off for mac_3: each asks for the other way.
        press(cx, "mac_2");
        assert_eq!(asked(&settings, cx, "mac_2"), Some(true));
        assert_eq!(
            asked(&settings, cx, "mac_3"),
            None,
            "another card's is free"
        );
        press(cx, "mac_3");
        assert_eq!(asked(&settings, cx, "mac_3"), Some(false));
        // With the server, a card takes no second click.
        press(cx, "mac_2");
        assert_eq!(asked(&settings, cx, "mac_2"), Some(true), "not asked again");
        assert_eq!(
            asked(&settings, cx, "mac_1"),
            None,
            "this computer's is untouched"
        );
    }

    /// An allow the gate never reads says so, in the server's sentence given whole and closed
    /// with a full stop: the long one lists everything a plain command may not have in it, and
    /// clipping it would clip off the part that says which one this rule has.
    #[test]
    fn an_allow_that_is_never_read_says_why_in_the_servers_words() {
        assert_eq!(
            not_in_effect_line("sudo cannot be a standing allow"),
            "Not in effect: sudo cannot be a standing allow."
        );
        let whole = "an allow rule must be one plain command: no ; && || | & or newline, no $( ) \
                     or backticks, no redirection to a path, no VAR= in front, and not a program \
                     that runs another (sh, eval, env, sudo, xargs…)";
        assert_eq!(
            not_in_effect_line(whole),
            format!("Not in effect: {whole}.")
        );
        assert_eq!(
            not_in_effect_line("  it was kept before the refusal. \n"),
            "Not in effect: it was kept before the refusal."
        );
        assert_eq!(not_in_effect_line("   "), "Not in effect.");
    }
}
