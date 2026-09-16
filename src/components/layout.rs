use crate::components::agent_settings::AgentSettings;
use crate::components::app_settings::AppSettings;
use crate::components::bot_finder::BotFinder;
use crate::components::chat::ChatView;
use crate::components::command_palette::CommandPalette;
use crate::components::computer::ComputerPane;
use crate::components::hidden_bots::hidden_bots_overlay;
use crate::components::login::LoginView;
use crate::components::recipes::{RecipesView, recipe_delete_overlay};
use crate::components::sidebar::SidebarView;
use crate::components::title_bar::TitleBar;
use crate::state::{MainPage, RightPane};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Disableable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::chrome::{INFO_PANE_WIDTH, chrome_floats, sidebar_width};
use crate::state::AppState;

fn cached_fill<V: Render>(view: Entity<V>) -> impl IntoElement {
    view.cached(StyleRefinement::default().absolute().size_full())
}

#[derive(Clone, PartialEq, Eq)]
struct ShellRev {
    collapsed: bool,
    hidden: bool,
    expanded_width: i32,
    auto_collapsed: bool,
    signed_in: bool,
    signing_in: bool,
    auth_error: Option<String>,
    right_pane: u8,
    computer_editor: bool,
    model_picker: bool,
    avatar_editor: bool,
    app_settings: bool,
    bot_finder: bool,
    command_palette: bool,
    hidden_bots: bool,
    has_agent: bool,
    hiring: bool,
    recipes_page: bool,
    recipe_delete: bool,
}

impl ShellRev {
    fn from_state(state: &AppState) -> Self {
        Self {
            collapsed: state.sidebar_collapsed,
            hidden: state.sidebar_hidden,
            expanded_width: state.sidebar_expanded_width.round() as i32,
            auto_collapsed: state.auto_collapsed,
            signed_in: state.is_signed_in(),
            signing_in: state.auth_status == crate::state::AuthStatus::SigningIn,
            auth_error: state.auth_error.clone(),
            right_pane: match state.right_pane {
                RightPane::Closed => 0,
                RightPane::Settings => 1,
                RightPane::Computer => 2,
            },
            computer_editor: matches!(
                state.computer_view,
                crate::state::ComputerView::Editor { .. }
            ),
            model_picker: state.model_picker_open,
            avatar_editor: state.avatar_editor_open,
            app_settings: state.is_app_settings_open,
            bot_finder: state.bot_finder_open,
            command_palette: state.command_palette_open,
            hidden_bots: state.hidden_bots_open,
            has_agent: state.active_coworker_id.is_some(),
            hiring: state.hiring,
            recipes_page: state.page == MainPage::Recipes,
            recipe_delete: state.recipe_delete_confirm,
        }
    }
}

#[derive(Clone)]
pub struct Layout {
    sidebar: Entity<SidebarView>,
    chat: Entity<ChatView>,
    login: Entity<LoginView>,
    agent_settings: Entity<AgentSettings>,
    computer: Entity<ComputerPane>,
    title_bar: Entity<TitleBar>,
    app_settings: Entity<AppSettings>,
    bot_finder: Entity<BotFinder>,
    command_palette: Entity<CommandPalette>,
    recipes: Entity<RecipesView>,
    state: Entity<AppState>,
    shell: ShellRev,
    last_window_width: Option<Pixels>,
    resize_drag: Option<(f32, f32)>,
}

impl Layout {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let sidebar = cx.new(|cx| SidebarView::new(window, state.clone(), cx));
        let chat = cx.new(|cx| ChatView::new(window, state.clone(), cx));
        let login = cx.new(|cx| LoginView::new(window, state.clone(), cx));
        let agent_settings = cx.new(|cx| AgentSettings::new(window, state.clone(), cx));
        let computer = cx.new(|cx| ComputerPane::new(window, state.clone(), cx));
        let recipes = cx.new(|cx| RecipesView::new(window, state.clone(), cx));
        // The bar draws the Recipes page's header, so the page comes before it.
        let title_bar = cx.new(|cx| {
            TitleBar::new(
                state.clone(),
                chat.clone(),
                computer.clone(),
                recipes.clone(),
                cx,
            )
        });
        let app_settings = cx.new(|cx| AppSettings::new(state.clone(), cx));
        let bot_finder = cx.new(|cx| BotFinder::new(window, state.clone(), cx));
        let command_palette = cx.new(|cx| CommandPalette::new(window, state.clone(), cx));
        let shell = ShellRev::from_state(&state.read(cx));

        cx.observe(&state, |this, state, cx| {
            let shell = ShellRev::from_state(&state.read(cx));
            if this.shell != shell {
                this.shell = shell;
                cx.notify();
            }
        })
        .detach();

        Self {
            sidebar,
            chat,
            login,
            agent_settings,
            computer,
            title_bar,
            app_settings,
            bot_finder,
            command_palette,
            recipes,
            state,
            shell,
            last_window_width: None,
            resize_drag: None,
        }
    }

    pub fn focus_sidebar_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.focus_search(window, cx);
        });
    }

    pub fn sidebar_search_focused(&self, window: &Window, cx: &App) -> bool {
        self.sidebar.read(cx).search_is_focused(window, cx)
    }

    pub fn clear_sidebar_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.clear_search(window, cx);
        });
    }

    pub fn focus_chat_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.update(cx, |chat, cx| {
            chat.focus_input(window, cx);
        });
    }

    pub fn open_find_in_chat(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.update(cx, |chat, cx| {
            chat.open_find(window, cx);
        });
    }

    pub fn close_find_in_chat(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.update(cx, |chat, cx| {
            chat.close_find(window, cx);
        });
    }

    pub fn find_next_in_chat(&self, cx: &mut Context<Self>) {
        self.chat.update(cx, |chat, cx| {
            chat.find_next(cx);
        });
    }

    pub fn find_prev_in_chat(&self, cx: &mut Context<Self>) {
        self.chat.update(cx, |chat, cx| {
            chat.find_prev(cx);
        });
    }

    pub fn blur_chat_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.blur(cx);
    }

    pub fn pick_overlay_item(&self, index: usize, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if state.command_palette_open {
            self.command_palette.update(cx, |palette, cx| {
                palette.activate_shortcut(index, cx);
            });
        } else if state.bot_finder_open {
            self.bot_finder.update(cx, |finder, cx| {
                finder.activate_shortcut(index, cx);
            });
        }
    }
}

impl Render for Layout {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_width = window.viewport_size().width;
        if self.last_window_width != Some(window_width) {
            self.last_window_width = Some(window_width);
            let state_entity = self.state.clone();
            cx.defer(move |cx| {
                state_entity.update(cx, |state, cx| {
                    state.apply_responsive_sidebar(f32::from(window_width), cx);
                });
            });
        }

        let state = self.state.read(cx);
        let app_settings_open = state.is_app_settings_open;
        let bot_finder_open = state.bot_finder_open;
        let command_palette_open = state.command_palette_open;
        let hidden_bots_open = state.hidden_bots_open;
        if !state.is_signed_in() {
            return v_flex().size_full().child(self.title_bar.clone()).child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(self.login.clone())
                    .when(app_settings_open, |this| {
                        this.child(
                            div()
                                .id("app-settings-overlay")
                                .absolute()
                                .inset_0()
                                .occlude()
                                .child(self.app_settings.clone()),
                        )
                    })
                    .when(command_palette_open, |this| {
                        this.child(self.command_palette.clone())
                    }),
            );
        }
        let has_agent = state.active_coworker_id.is_some();
        let hiring = state.hiring;
        let hire_error = state.auth_error.clone();
        let hidden = state.sidebar_hidden;
        let collapsed = state.sidebar_collapsed;
        let expanded_width = state.sidebar_expanded_width;
        let right_pane = state.right_pane;
        let banner = state.computer_banner();
        let computer_confirm = state
            .computer_confirm
            .map(|action| (action, state.active_bot_name()));
        let recipes_page = state.page == MainPage::Recipes;
        let recipe_delete = state.recipe_delete_prompt();
        let theme = cx.theme().clone();
        // A page from the dock takes the chat's slot; the chat is back when a bot is chosen.
        let main = if recipes_page {
            self.recipes.clone().into_any_element()
        } else if has_agent {
            self.chat.clone().into_any_element()
        } else {
            empty_agent_pane(self.state.clone(), hire_error, hiring, &theme)
        };
        let left = sidebar_width(hidden, collapsed, expanded_width);
        let floats = chrome_floats(f32::from(window_width));
        let right_open = right_pane != RightPane::Closed;
        let dragging = self.resize_drag.is_some() && !floats;
        let show_scrim = floats && (right_open || (left > 0.0 && !collapsed));
        let sidebar_view = self.sidebar.clone();
        let right_child = match right_pane {
            RightPane::Settings => self.agent_settings.clone().into_any_element(),
            RightPane::Computer => self.computer.clone().into_any_element(),
            RightPane::Closed => div().into_any_element(),
        };

        let row = div()
            .w_full()
            .flex_1()
            .min_h_0()
            .flex()
            .relative()
            .when(dragging, |this| {
                this.cursor(CursorStyle::ResizeLeftRight)
                    .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                        let Some((origin_x, origin_w)) = this.resize_drag else {
                            return;
                        };
                        let width = origin_w + f32::from(ev.position.x) - origin_x;
                        this.state.update(cx, |state, cx| {
                            state.resize_sidebar(width, cx);
                        });
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resize_drag = None;
                            cx.notify();
                        }),
                    )
            })
            .when(!floats && left > 0.0, |this| {
                this.child(
                    div()
                        .id("sidebar-slot")
                        .w(px(left))
                        .flex_shrink_0()
                        .h_full()
                        .relative()
                        .child(cached_fill(sidebar_view.clone()))
                        .child(
                            div()
                                .id("sidebar-resize")
                                .absolute()
                                .top_0()
                                .right_0()
                                .bottom_0()
                                .w(px(6.))
                                .cursor(CursorStyle::ResizeLeftRight)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                        this.resize_drag = Some((f32::from(ev.position.x), left));
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
            })
            .child(
                div()
                    .id("chat-slot")
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .relative()
                    .overflow_hidden()
                    .child(main)
                    .when(bot_finder_open, |this| this.child(self.bot_finder.clone())),
            )
            .when(!floats && right_open, |this| {
                this.child(
                    div()
                        .id("right-pane-slot")
                        .w(px(INFO_PANE_WIDTH))
                        .flex_shrink_0()
                        .h_full()
                        .overflow_hidden()
                        .child(right_child),
                )
            })
            .when(show_scrim, |this| {
                this.child(
                    div()
                        .id("chrome-scrim")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .bg(gpui::black().opacity(0.28))
                        .on_mouse_down(MouseButton::Left, {
                            let state = self.state.clone();
                            move |_, _, cx| {
                                state.update(cx, |state, cx| {
                                    if state.right_pane != RightPane::Closed {
                                        state.close_right_pane(cx);
                                    } else {
                                        state.set_sidebar_collapsed(true, false, cx);
                                    }
                                });
                            }
                        }),
                )
            })
            .when(floats && left > 0.0, |this| {
                this.child(
                    div()
                        .id("sidebar-slot")
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(left))
                        .occlude()
                        .child(cached_fill(sidebar_view)),
                )
            })
            .when(floats && right_open, |this| {
                this.child(
                    div()
                        .id("right-pane-slot")
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom_0()
                        .w(px(INFO_PANE_WIDTH))
                        .occlude()
                        .overflow_hidden()
                        .child(match right_pane {
                            RightPane::Settings => self.agent_settings.clone().into_any_element(),
                            RightPane::Computer => self.computer.clone().into_any_element(),
                            RightPane::Closed => div().into_any_element(),
                        }),
                )
            })
            .when(app_settings_open, |this| {
                this.child(
                    div()
                        .id("app-settings-overlay")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .child(self.app_settings.clone()),
                )
            })
            .when(command_palette_open, |this| {
                this.child(self.command_palette.clone())
            })
            .when(hidden_bots_open, |this| {
                this.child(hidden_bots_overlay(self.state.clone(), cx))
            })
            .when_some(banner, |this, (title, detail)| {
                this.child(update_banner(title, detail, &theme))
            })
            .when_some(computer_confirm, |this, (action, name)| {
                this.child(computer_confirm_overlay(
                    self.state.clone(),
                    action,
                    name,
                    &theme,
                ))
            })
            .when_some(recipe_delete, |this, name| {
                this.child(recipe_delete_overlay(self.state.clone(), name, &theme))
            });

        // The title bar, then the sidebar, chat and right pane in what is left.
        v_flex()
            .size_full()
            .child(self.title_bar.clone())
            .child(row)
    }
}

/// "Update Hexuria's computer?" — the question, what it means, Cancel and Confirm. Click
/// outside or Cancel closes it; Confirm does the thing.
fn computer_confirm_overlay(
    app: Entity<AppState>,
    action: crate::state::ComputerAction,
    name: String,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    use crate::state::ComputerAction;
    let (title, detail, verb) = match action {
        ComputerAction::Update => (
            format!("Update {name}'s computer?"),
            "Rebuilds it on the newest image. Files and logins stay; installed apps and packages are removed. The computer is briefly unavailable while its data moves.",
            "Update",
        ),
        ComputerAction::Reset => (
            format!("Reset {name}'s computer?"),
            "Starts fresh. Everything on this computer — files, logins, installed apps — is lost.",
            "Reset",
        ),
    };
    let confirm = Button::new("computer-confirm-yes").label(verb).on_click({
        let app = app.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.confirm_computer_action(cx));
        }
    });
    let confirm = match action {
        ComputerAction::Update => confirm.primary(),
        ComputerAction::Reset => confirm.danger(),
    };
    div()
        .id("computer-confirm-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::black().opacity(0.32))
        .on_mouse_down(MouseButton::Left, {
            let app = app.clone();
            move |_, _, cx| {
                app.update(cx, |state, cx| state.close_computer_confirm(cx));
            }
        })
        .child(
            v_flex()
                .id("computer-confirm")
                .w(px(420.))
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
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(detail),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap(px(8.))
                        .pt(px(6.))
                        .child(
                            Button::new("computer-confirm-cancel")
                                .label("Cancel")
                                .on_click({
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.close_computer_confirm(cx)
                                        });
                                    }
                                }),
                        )
                        .child(confirm),
                ),
        )
}

/// The pill over the app while a computer is being updated — title and the current phase.
fn update_banner(
    title: String,
    detail: String,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    div()
        .id("update-banner")
        .absolute()
        .top(px(12.))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(
            h_flex()
                .items_center()
                .gap(px(10.))
                .px(px(14.))
                .py(px(8.))
                .rounded(px(12.))
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .shadow_md()
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("⟳"),
                )
                .child(
                    v_flex()
                        .gap(px(1.))
                        .child(div().text_sm().child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(detail),
                        ),
                ),
        )
}

fn empty_agent_pane(
    state: Entity<AppState>,
    error: Option<String>,
    hiring: bool,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    v_flex()
        .id("empty-roster")
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .bg(theme.background)
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Create your first Bot"),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("No agent yet. Hire one to start a chat."),
        )
        .when_some(error, |this, message| {
            this.child(
                div()
                    .id("empty-roster-error")
                    .max_w(px(420.))
                    .text_sm()
                    .text_color(theme.danger)
                    .child(message),
            )
        })
        .child(
            div().id("create-first-bot").child(
                Button::new("create-first-bot-btn")
                    .label(if hiring { "Creating…" } else { "New Bot" })
                    .primary()
                    .disabled(hiring)
                    .on_click(move |_, _, cx| {
                        state.update(cx, |state, cx| {
                            state.create_agent(cx);
                        });
                    }),
            ),
        )
        .into_any_element()
}
