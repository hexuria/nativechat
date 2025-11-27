use crate::state::AppState;
use gpui::{
    Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use ui::{ActiveTheme, Collapsible, Icon, IconName, Side, avatar::Avatar, h_flex, sidebar::*};

pub struct SidebarView {
    state: Entity<AppState>,
}

impl SidebarView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();

        Self { state }
    }
}

impl Render for SidebarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let collapsed = state.sidebar_collapsed;
        let active_id = state.active_conversation_id;

        let theme = cx.theme();

        // Check if any modal is open
        // Check if any modal is open
        let any_modal_open = state.is_voice_mode_open
            || state.is_account_settings_open
            || state.is_profile_settings_open;

        let max_height = window.viewport_size().height - px(360.0);
        let min_height = px(180.);

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
                                                this.bg(cx.theme().sidebar_accent.opacity(0.8))
                                                    .text_color(
                                                        cx.theme().sidebar_accent_foreground,
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
                    .child(
                        SidebarGroup::new("Menu").collapsed(collapsed).child(
                            SidebarMenu::new()
                                .collapsed(collapsed)
                                .child(
                                    SidebarMenuItem::new("New Chat")
                                        .icon(IconName::Plus)
                                        .disable(any_modal_open),
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
                    .child(SidebarMenu::new().collapsed(collapsed).children(
                        if state.conversations.is_empty() {
                            vec![]
                        } else {
                            state
                                .conversations
                                .iter()
                                .map(|c| {
                                    let id = c.id;
                                    let is_active = Some(id) == active_id;
                                    SidebarMenuItem::new(&c.title)
                                        .icon(IconName::Dash)
                                        .active(is_active)
                                        .disable(any_modal_open)
                                        .on_click({
                                            let state = self.state.clone();
                                            move |_, _, cx| {
                                                state.update(cx, |state, cx| {
                                                    state.select_conversation(id, cx);
                                                });
                                            }
                                        })
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
                                    let (label, icon_name) = match state.theme_mode.as_str() {
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
            )
    }
}
