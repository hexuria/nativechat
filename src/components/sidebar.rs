use crate::actions::{
    CancelDeleteSession, CancelRenameSession, ConfirmDeleteSession, DeleteSession, SelectSession,
    StartRenameSession, SubmitRenameSession,
};
use crate::state::AppState;
use gpui::{Axis, prelude::FluentBuilder, *};
use ui::{
    ActiveTheme, Collapsible, Icon, IconName, Side, StyledExt, avatar::Avatar, h_flex,
    input::InputState, sidebar::*, v_flex,
};

#[derive(IntoElement)]
struct ChatList {
    collapsed: bool,
    children: Vec<gpui::AnyElement>,
}

impl ChatList {
    fn new() -> Self {
        Self {
            collapsed: false,
            children: Vec::new(),
        }
    }

    fn children(mut self, children: impl IntoIterator<Item = gpui::AnyElement>) -> Self {
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

impl gpui::RenderOnce for ChatList {
    fn render(self, _window: &mut Window, _cx: &mut gpui::App) -> impl IntoElement {
        v_flex().gap_1().children(self.children)
    }
}

pub struct SidebarView {
    state: Entity<AppState>,
    editing_session_id: Option<String>,
    rename_input: Option<Entity<InputState>>,
    delete_confirmation_id: Option<String>,
}

impl SidebarView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();

        Self {
            state,
            editing_session_id: None,
            rename_input: None,
            delete_confirmation_id: None,
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
        let input = cx.new(|cx| ui::input::InputState::new(window, cx));

        // Subscribe to input events to handle Enter key
        cx.subscribe_in(&input, window, |this, _state, event, window, cx| {
            match event {
                ui::input::InputEvent::PressEnter { secondary } if !secondary => {
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
            state.focus_handle().focus(window);
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

        let theme = cx.theme();

        // Check if any modal is open
        let any_modal_open = state.is_voice_mode_open
            || state.is_account_settings_open
            || state.is_profile_settings_open;

        let max_height = window.viewport_size().height - px(360.0);
        let min_height = px(180.);

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
                Sidebar::<ui::resizable::ResizablePanel>::new(Side::Left)
                    .collapsed(collapsed)
                    .border_r(px(0.)) // Remove border to let resize handle act as border
                    .header(
                        h_flex()
                            .w_full()
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
                                        })),
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
                                                    .font_weight(gpui::FontWeight::BOLD)
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
                        ui::resizable::resizable_panel()
                            .size(min_height)
                            .size_range(min_height..max_height)
                            .fixed_width(true) // Ensure fixed width behavior
                            .child(
                                SidebarGroup::new("Menu").collapsed(collapsed).child(
                                    SidebarMenu::new()
                                        .collapsed(collapsed)
                                        .child(
                                            SidebarMenuItem::new("New Chat")
                                                .icon(IconName::Plus)
                                                .disable(any_modal_open)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.state.update(cx, |state, cx| {
                                                        state.create_new_session(cx);
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
                                        ),
                                ),
                            )
                            .into(),
                    )
                    .child(
                        SidebarGroup::new("Chat History")
                            .collapsed(collapsed)
                            .child(ChatList::new().collapsed(collapsed).children(
                                if state.conversations.is_empty() {
                                    vec![]
                                } else {
                                    use crate::components::sidebar_chat_item::ChatSessionItem;

                                    let view_entity = cx.entity().clone();

                                    state
                                        .conversations
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
                            ))
                            .into(),
                    )
                    .footer(
                        SidebarFooter::new().child(
                            div()
                                .flex()
                                .flex_col()
                                .w_full()
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
                                                            .font_weight(gpui::FontWeight::BOLD)
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
                                            let (label, icon_name) = match state.theme_mode.as_str()
                                            {
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
                                            SidebarMenuItem::new("Account Settings")
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
                                                .on_click(|_, _, _| {
                                                    println!("Sign out clicked");
                                                }),
                                        ),
                                ),
                        ),
                    ),
            )
    }
}
