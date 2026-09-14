use crate::components::agent_settings::AgentSettings;
use crate::components::app_settings::AppSettings;
use crate::components::computer::ComputerPane;
use crate::state::RightPane;
use crate::components::chat::ChatView;
use crate::components::login::LoginView;
use crate::components::modals::profile_settings::ProfileSettingsModal;
use crate::components::sidebar::SidebarView;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Disableable, v_flex};

use crate::chrome::{sidebar_width, INFO_PANE_WIDTH};
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
    profile: bool,
    signed_in: bool,
    signing_in: bool,
    auth_error: Option<String>,
    right_pane: u8,
    computer_editor: bool,
    model_picker: bool,
    avatar_editor: bool,
    app_settings: bool,
    has_agent: bool,
    hiring: bool,
}

impl ShellRev {
    fn from_state(state: &AppState) -> Self {
        Self {
            collapsed: state.sidebar_collapsed,
            hidden: state.sidebar_hidden,
            expanded_width: state.sidebar_expanded_width.round() as i32,
            auto_collapsed: state.auto_collapsed,
            profile: state.is_profile_settings_open,
            signed_in: state.is_signed_in(),
            signing_in: state.auth_status == crate::state::AuthStatus::SigningIn,
            auth_error: state.auth_error.clone(),
            right_pane: match state.right_pane {
                RightPane::Closed => 0,
                RightPane::Settings => 1,
                RightPane::Computer => 2,
            },
            computer_editor: matches!(state.computer_view, crate::state::ComputerView::Editor { .. }),
            model_picker: state.model_picker_open,
            avatar_editor: state.avatar_editor_open,
            app_settings: state.is_app_settings_open,
            has_agent: state.active_coworker_id.is_some(),
            hiring: state.hiring,
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
    profile_settings_modal: Entity<ProfileSettingsModal>,
    app_settings: Entity<AppSettings>,
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
        let profile_settings_modal =
            cx.new(|cx| ProfileSettingsModal::new(window, state.clone(), cx));
        let app_settings = cx.new(|cx| AppSettings::new(state.clone(), cx));
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
            profile_settings_modal,
            app_settings,
            state,
            shell,
            last_window_width: None,
            resize_drag: None,
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
        if !state.is_signed_in() {
            return div()
                .size_full()
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
                });
        }
        let has_agent = state.active_coworker_id.is_some();
        let hiring = state.hiring;
        let hire_error = state.auth_error.clone();
        let hidden = state.sidebar_hidden;
        let collapsed = state.sidebar_collapsed;
        let expanded_width = state.sidebar_expanded_width;
        let right_pane = state.right_pane;
        let theme = cx.theme().clone();
        let main = if has_agent {
            self.chat.clone().into_any_element()
        } else {
            empty_agent_pane(self.state.clone(), hire_error, hiring, &theme)
        };
        let left = sidebar_width(hidden, collapsed, expanded_width);
        let dragging = self.resize_drag.is_some();

        div()
            .size_full()
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
            .when(left > 0.0, |this| {
                this.child(
                    div()
                        .id("sidebar-slot")
                        .w(px(left))
                        .flex_shrink_0()
                        .h_full()
                        .relative()
                        .child(cached_fill(self.sidebar.clone()))
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
                                        this.resize_drag =
                                            Some((f32::from(ev.position.x), left));
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .overflow_hidden()
                    .child(main),
            )
            .when(right_pane != RightPane::Closed, |this| {
                this.child(
                    div()
                        .id("right-pane-slot")
                        .w(px(INFO_PANE_WIDTH))
                        .flex_shrink_0()
                        .h_full()
                        .overflow_hidden()
                        .child(match right_pane {
                            RightPane::Settings => self.agent_settings.clone().into_any_element(),
                            RightPane::Computer => self.computer.clone().into_any_element(),
                            RightPane::Closed => div().into_any_element(),
                        }),
                )
            })
            .children(
                state
                    .is_profile_settings_open
                    .then(|| self.profile_settings_modal.clone().into_any_element()),
            )
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
    }
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
                    .label(if hiring {
                        "Creating…"
                    } else {
                        "New Bot"
                    })
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
