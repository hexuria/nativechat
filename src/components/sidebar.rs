use crate::actions::{
    CancelDeleteSession, CancelRenameSession, ConfirmDeleteSession, DeleteSession, SelectSession,
    StartRenameSession, SubmitRenameSession,
};
use crate::state::AppState;
use gpui_kit::{prelude::FluentBuilder, *};
use gpui_kit::component::{
    ActiveTheme, Collapsible, Icon, IconName, StyledExt, avatar::Avatar, h_flex, input::InputState,
    sidebar::*, v_flex,
};

#[derive(IntoElement)]
struct ChatList {
    collapsed: bool,
    children: Vec<gpui_kit::AnyElement>,
}

impl ChatList {
    fn new() -> Self {
        Self {
            collapsed: false,
            children: Vec::new(),
        }
    }

    fn children(mut self, children: impl IntoIterator<Item = gpui_kit::AnyElement>) -> Self {
        self.children = children.into_iter().collect();
        self
    }
}

impl Collapsible for ChatList {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl gpui_kit::RenderOnce for ChatList {
    fn render(self, _window: &mut Window, _cx: &mut gpui_kit::App) -> impl IntoElement {
        v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .children(self.children)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct SidebarRev {
    collapsed: bool,
    theme_mode: String,
    active_id: Option<String>,
    any_modal: bool,
    sessions: Vec<(String, String, String)>,
    coworkers: Vec<(String, String)>,
    signed_in: bool,
}

impl SidebarRev {
    fn from_state(state: &AppState) -> Self {
        Self {
            collapsed: state.sidebar_collapsed,
            theme_mode: state.theme_mode.clone(),
            active_id: state.active_coworker_id.clone().or(state.active_conversation_id.clone()),
            any_modal: state.is_voice_mode_open
                || state.is_account_settings_open
                || state.is_profile_settings_open
                || state.is_credentials_modal_open,
            sessions: state
                .conversations
                .iter()
                .map(|c| (c.id.clone(), c.title.clone(), c.created_at.clone()))
                .collect(),
            coworkers: state
                .coworkers
                .iter()
                .map(|c| (c.id.clone(), c.name.clone()))
                .collect(),
            signed_in: state.is_signed_in(),
        }
    }
}

pub struct SidebarView {
    state: Entity<AppState>,
    editing_session_id: Option<String>,
    rename_input: Option<Entity<InputState>>,
    delete_confirmation_id: Option<String>,
    list_scroll: ScrollHandle,
    rev: SidebarRev,
}

impl SidebarView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let rev = SidebarRev::from_state(&state.read(cx));
        cx.observe(&state, |this, state, cx| {
            let rev = SidebarRev::from_state(&state.read(cx));
            if this.rev != rev {
                this.rev = rev;
                cx.notify();
            }
        })
        .detach();

        Self {
            state,
            editing_session_id: None,
            rename_input: None,
            delete_confirmation_id: None,
            list_scroll: ScrollHandle::new(),
            rev,
        }
    }

    fn start_editing(
        &mut self,
        action: &StartRenameSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        println!(
            "[DEBUG] start_editing called for id: {}, title: {}",
            action.id, action.title
        );
        let input = cx.new(|cx| gpui_kit::component::input::InputState::new(window, cx));

        // Subscribe to input events to handle Enter key
        cx.subscribe_in(&input, window, |this, _state, event, window, cx| {
            match event {
                gpui_kit::component::input::InputEvent::PressEnter { secondary, .. } if !secondary => {
                    // Enter without Shift - submit the rename
                    println!("[DEBUG] Enter pressed in rename input");
                    this.submit_rename(&crate::actions::SubmitRenameSession, window, cx);
                }
                _ => {}
            }
        })
        .detach();

        input.update(cx, |state, cx| {
            state.set_value(action.title.clone(), window, cx);
            state.focus_handle(cx).focus(window, cx);
        });

        self.editing_session_id = Some(action.id.clone());
        self.rename_input = Some(input);
        self.delete_confirmation_id = None;
        cx.notify();
    }

    fn cancel_editing(
        &mut self,
        _: &CancelRenameSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_session_id = None;
        self.rename_input = None;
        cx.notify();
    }

    fn submit_rename(
        &mut self,
        _: &SubmitRenameSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.editing_session_id.clone() {
            if let Some(input) = self.rename_input.clone() {
                let new_title = input.read(cx).text().to_string();
                if !new_title.trim().is_empty() {
                    self.state.update(cx, |state, cx| {
                        state.rename_session(id, new_title, cx);
                    });
                }
            }
        }
        self.editing_session_id = None;
        self.rename_input = None;
        cx.notify();
    }

    fn show_delete_confirmation(
        &mut self,
        action: &DeleteSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        println!(
            "[DEBUG] show_delete_confirmation called for id: {}",
            action.id
        );
        self.delete_confirmation_id = Some(action.id.clone());
        self.editing_session_id = None;
        self.rename_input = None;
        cx.notify();
    }

    fn cancel_delete(
        &mut self,
        _: &CancelDeleteSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_confirmation_id = None;
        cx.notify();
    }

    fn confirm_delete(
        &mut self,
        _: &ConfirmDeleteSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.delete_confirmation_id.clone() {
            self.state.update(cx, |state, cx| {
                state.delete_session(id, cx);
            });
        }
        self.delete_confirmation_id = None;
        cx.notify();
    }

    fn select_session(
        &mut self,
        action: &SelectSession,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = action.id.clone();
        self.state.update(cx, |state, cx| {
            state.select_conversation(id, cx);
        });
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let collapsed = state.sidebar_collapsed;
        let active_id = state.active_conversation_id.clone();
        let conversations = state.conversations.clone();
        let coworkers = state.coworkers.clone();
        let signed_in = state.is_signed_in();
        let active_coworker = state.active_coworker_id.clone();
        let theme_mode = state.theme_mode.clone();
        let account_label = state
            .account
            .as_ref()
            .map(|a| a.display_name())
            .unwrap_or_else(|| "Account Settings".into());
        let any_modal_open = state.is_voice_mode_open
            || state.is_account_settings_open
            || state.is_profile_settings_open;

        let theme = cx.theme().clone();

        div()
            .size_full()
            .on_action(cx.listener(Self::select_session))
            .on_action(cx.listener(Self::start_editing))
            .on_action(cx.listener(Self::cancel_editing))
            .on_action(cx.listener(Self::submit_rename))
            .on_action(cx.listener(Self::show_delete_confirmation))
            .on_action(cx.listener(Self::cancel_delete))
            .on_action(cx.listener(Self::confirm_delete))
            .child(
                v_flex()
                    .id("sidebar")
                    .size_full()
                    .overflow_hidden()
                    .bg(theme.sidebar)
                    .border_r(px(0.))
                    .child(
                        h_flex()
                            .id("sidebar-header")
                            .w_full()
                            .flex_shrink_0()
                            .items_center()
                            .when(collapsed, |this| {
                                this.px_2().child(
                                    SidebarMenuItem::new("Expand Sidebar")
                                        .collapsed(true)
                                        .icon(IconName::ChevronRight)
                                        .disable(any_modal_open)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.state.update(cx, |state, cx| {
                                                state.toggle_sidebar(cx);
                                            });
                                        }))
                                        .render("expand-sidebar", window, cx),
                                )
                            })
                            .when(!collapsed, |this| {
                                this.justify_between()
                                    .px_2()
                                    .py_2()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .rounded(cx.theme().radius)
                                                    .bg(theme.primary)
                                                    .text_color(theme.primary_foreground)
                                                    .p_2()
                                                    .child(Icon::new(IconName::Bot).size_4()),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui_kit::FontWeight::BOLD)
                                                    .child("Native Chat"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("sidebar-collapse-button")
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded(cx.theme().radius)
                                            .when(!any_modal_open, |this| {
                                                this.cursor_pointer()
                                                    .hover(|this| {
                                                        this.bg(cx
                                                            .theme()
                                                            .sidebar_accent
                                                            .opacity(0.8))
                                                            .text_color(
                                                                cx.theme()
                                                                    .sidebar_accent_foreground,
                                                            )
                                                    })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.state.update(cx, |state, cx| {
                                                            state.toggle_sidebar(cx);
                                                        });
                                                    }))
                                            })
                                            .p_2()
                                            .child(Icon::new(IconName::ChevronLeft).size_4()),
                                    )
                            }),
                    )
                    .child(
                        div().id("sidebar-nav").flex_shrink_0().w_full().child(
                        SidebarMenu::new()
                            .collapsed(collapsed)
                            .child(
                                SidebarMenuItem::new("New Bot")
                                    .icon(IconName::Plus)
                                    .disable(any_modal_open)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |state, cx| {
                                            state.create_agent(cx);
                                        });
                                    })),
                            )
                            .child(
                                SidebarMenuItem::new("Search")
                                    .icon(IconName::Search)
                                    .disable(any_modal_open),
                            )
                            .child(
                                SidebarMenuItem::new("Library")
                                    .icon(IconName::BookOpen)
                                    .disable(any_modal_open),
                            )
                            .child(
                                SidebarMenuItem::new("Projects")
                                    .icon(IconName::Folder)
                                    .disable(any_modal_open),
                            )
                            .render("sidebar-menu", window, cx),
                        ),
                    )
                    .child(
                        div()
                            .id("sidebar-chat-list")
                            .flex_1()
                            .min_h(px(0.))
                            .w_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.list_scroll)
                            .child(
                            ChatList::new().collapsed(collapsed).children(
                                if signed_in {
                                    let view_entity = cx.entity().clone();
                                    coworkers
                                        .iter()
                                        .map(|c| {
                                            let id = c.id.clone();
                                            let name = c.name.clone();
                                            let is_active =
                                                Some(id.clone()) == active_coworker.clone();
                                            let glyph = name
                                                .chars()
                                                .next()
                                                .map(|ch| ch.to_string())
                                                .unwrap_or_else(|| "?".into());
                                            div()
                                                .id(SharedString::from(format!("coworker-{id}")))
                                                .w_full()
                                                .px_2()
                                                .py_1()
                                                .rounded_md()
                                                .cursor_pointer()
                                                .when(is_active, |this| this.bg(theme.accent))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    {
                                                        let id = id.clone();
                                                        let view = view_entity.clone();
                                                        move |_, _, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.state.update(cx, |state, cx| {
                                                                    state.select_coworker(
                                                                        id.clone(),
                                                                        cx,
                                                                    );
                                                                });
                                                            });
                                                        }
                                                    },
                                                )
                                                .child(
                                                    h_flex()
                                                        .gap_2()
                                                        .items_center()
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .justify_center()
                                                                .size_8()
                                                                .rounded_full()
                                                                .bg(theme.primary)
                                                                .text_color(theme.primary_foreground)
                                                                .text_sm()
                                                                .child(glyph),
                                                        )
                                                        .when(!collapsed, |this| {
                                                            this.child(
                                                                div().text_sm().child(name.clone()),
                                                            )
                                                        }),
                                                )
                                                .into_any_element()
                                        })
                                        .collect()
                                } else if conversations.is_empty() {
                                    vec![]
                                } else {
                                    use crate::components::sidebar_chat_item::ChatSessionItem;

                                    let view_entity = cx.entity().clone();

                                    conversations
                                        .iter()
                                        .map(|c| {
                                            let id = c.id.clone();
                                            let title = c.title.clone();
                                            let is_active = Some(id.clone()) == active_id.clone();
                                            let is_editing =
                                                self.editing_session_id.as_ref() == Some(&id);
                                            let is_deleting =
                                                self.delete_confirmation_id.as_ref() == Some(&id);

                                            let input = if is_editing {
                                                self.rename_input.clone()
                                            } else {
                                                None
                                            };

                                            ChatSessionItem::new(
                                                id.clone(),
                                                title.clone(),
                                                c.created_at.clone(),
                                                is_active,
                                            )
                                            .collapsed(collapsed)
                                            .is_editing(is_editing)
                                            .is_deleting(is_deleting)
                                            .input(input)
                                            .on_select({
                                                let id = id.clone();
                                                let view = view_entity.clone();
                                                move |window, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.select_session(
                                                            &SelectSession { id: id.clone() },
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            })
                                            .on_edit({
                                                let id = id.clone();
                                                let title = title.clone();
                                                let view = view_entity.clone();
                                                move |window, cx| {
                                                    println!(
                                                        "[DEBUG] on_edit callback in sidebar.rs"
                                                    );
                                                    view.update(cx, |this, cx| {
                                                        this.start_editing(
                                                            &StartRenameSession {
                                                                id: id.clone(),
                                                                title: title.clone(),
                                                            },
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            })
                                            .on_delete({
                                                let id = id.clone();
                                                let view = view_entity.clone();
                                                move |window, cx| {
                                                    println!(
                                                        "[DEBUG] on_delete callback in sidebar.rs"
                                                    );
                                                    view.update(cx, |this, cx| {
                                                        this.show_delete_confirmation(
                                                            &DeleteSession { id: id.clone() },
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            })
                                            .on_cancel_edit({
                                                let view = view_entity.clone();
                                                move |window, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.cancel_editing(
                                                            &CancelRenameSession,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            })
                                            .on_submit_edit({
                                                let view = view_entity.clone();
                                                move |window, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.submit_rename(
                                                            &SubmitRenameSession,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            })
                                            .on_cancel_delete({
                                                let view = view_entity.clone();
                                                move |window, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.cancel_delete(
                                                            &CancelDeleteSession,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            })
                                            .on_confirm_delete({
                                                let view = view_entity.clone();
                                                move |window, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.confirm_delete(
                                                            &ConfirmDeleteSession,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                            })
                                            .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                            ),
                    )
                    .child(
                            div()
                                .id("sidebar-footer")
                                .flex()
                                .flex_col()
                                .flex_shrink_0()
                                .w_full()
                                .overflow_hidden()
                                .bg(theme.sidebar)
                                .border_t_1()
                                .border_color(theme.border)
                                .gap_2()
                                .child(
                                    // User Profile Section
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .when(!collapsed, |this| this.p_2())
                                        .when(collapsed, |this| this.justify_center())
                                        .gap_3()
                                        .when(!collapsed, |this| {
                                            this.child(
                                                Avatar::new()
                                                    .src("images/buggy_d_clown.png")
                                                    .size(px(36.0))
                                                    .rounded_full(),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .child("Buggy")
                                                            .font_weight(gpui_kit::FontWeight::BOLD)
                                                            .text_sm(),
                                                    )
                                                    .child(
                                                        div()
                                                            .child("buggy.d.code@gmail.com")
                                                            .text_xs()
                                                            .text_color(theme.muted_foreground),
                                                    ),
                                            )
                                        })
                                        .when(collapsed, |this| {
                                            this.child(
                                                Avatar::new()
                                                    .src("images/buggy_d_clown.png")
                                                    .size(px(36.0))
                                                    .rounded_full(),
                                            )
                                        }),
                                )
                                .child(
                                    SidebarMenu::new()
                                        .collapsed(collapsed)
                                        .child({
                                            let (label, icon_name) = match theme_mode.as_str() {
                                                "dark" => ("Theme: Dark", IconName::Moon),
                                                _ => ("Theme: Light", IconName::Sun),
                                            };
                                            SidebarMenuItem::new(label)
                                                .icon(icon_name)
                                                .disable(any_modal_open)
                                                .on_click({
                                                    let state = self.state.clone();
                                                    move |_, _, cx| {
                                                        state.update(cx, |state, cx| {
                                                            state.toggle_theme(cx);
                                                        });
                                                    }
                                                })
                                        })
                                        .child(
                                            SidebarMenuItem::new(account_label.clone())
                                                .icon(IconName::Settings)
                                                .disable(any_modal_open)
                                                .on_click({
                                                    let state = self.state.clone();
                                                    move |_, _, cx| {
                                                        state.update(cx, |state, cx| {
                                                            state.toggle_account_settings(cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Credentials")
                                                .icon(IconName::Asterisk)
                                                .disable(any_modal_open)
                                                .on_click({
                                                    let state = self.state.clone();
                                                    move |_, _, cx| {
                                                        state.update(cx, |state, cx| {
                                                            state.toggle_credentials_modal(cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Profile Settings")
                                                .icon(IconName::User)
                                                .disable(any_modal_open)
                                                .on_click({
                                                    let state = self.state.clone();
                                                    move |_, _, cx| {
                                                        state.update(cx, |state, cx| {
                                                            state.toggle_profile_settings(cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .child(
                                            SidebarMenuItem::new("Sign Out")
                                                .icon(IconName::CircleX)
                                                .disable(any_modal_open)
                                                .on_click({
                                                    let state = self.state.clone();
                                                    move |_, _, cx| {
                                                        state.update(cx, |state, cx| {
                                                            state.logout(cx);
                                                        });
                                                    }
                                                }),
                                        )
                                        .render("sidebar-footer", window, cx),
                                ),
                    ),
            )
    }
}
