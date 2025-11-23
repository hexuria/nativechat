use crate::state::AppState;
use gpui::prelude::*;
use gpui::{
    App, ClickEvent, Context, Entity, FontWeight, InteractiveElement, IntoElement, Render,
    SharedString, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, StyledExt, Theme, avatar::Avatar, scroll::ScrollbarAxis,
    sidebar::SidebarMenuItem,
};

pub struct SidebarView {
    state: Entity<AppState>,
}

impl SidebarView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();

        Self { state }
    }

    fn render_custom_item(
        &self,
        label: impl Into<SharedString>,
        path: &str,
        active: bool,
        theme: &Theme,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let label = label.into();
        div()
            .id(label.clone())
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .rounded_md()
            .hover(|s| s.bg(theme.accent))
            .cursor_pointer()
            .bg(if active {
                theme.accent
            } else {
                gpui::transparent_black()
            })
            .on_click(on_click)
            .child(
                gpui::svg()
                    .path(path.to_string())
                    .size(px(16.0))
                    .text_color(theme.foreground),
            )
            .child(div().child(label).text_sm())
    }

    fn render_menu_item(
        &self,
        label: impl Into<SharedString>,
        icon: IconName,
        theme: &Theme,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let label = label.into();
        div()
            .id(label.clone())
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .rounded_md()
            .hover(|s| s.bg(theme.accent))
            .cursor_pointer()
            .on_click(on_click)
            .child(Icon::new(icon).size(px(16.0)).text_color(theme.foreground))
            .child(div().child(label).text_sm())
    }
}

impl Render for SidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let active_id = state.active_conversation_id;
        let theme = cx.theme();

        // Mock user data
        let _user_name = "Buggy";
        let _user_email = "buggy.d.code@gmail.com";
        let _user_type = "user";

        div()
            .h_full()
            .w(px(250.0))
            .bg(theme.background)
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(
                // Header
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .p_4()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .child("Native Chat")
                            .font_weight(FontWeight::BOLD)
                            .text_lg(),
                    )
                    .child(Icon::new(IconName::ChevronLeft).size(px(16.0))),
            )
            .child(
                // Main Menu
                div()
                    .flex()
                    .flex_col()
                    .p_2()
                    .gap_1()
                    .child(SidebarMenuItem::new("New Chat").icon(IconName::Plus))
                    .child(SidebarMenuItem::new("Search").icon(IconName::Search))
                    .child(self.render_custom_item(
                        "Library",
                        "icons/library.svg",
                        false,
                        theme,
                        |_, _, _| {},
                    ))
                    .child(SidebarMenuItem::new("Projects").icon(IconName::Folder)),
            )
            .child(
                // Chat History Header
                div()
                    .p_2()
                    .pb(px(0.0))
                    .child("Chat History")
                    .font_weight(FontWeight::BOLD)
                    .text_xs()
                    .text_color(theme.muted_foreground),
            )
            .child(
                // Chat History List
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .scrollable(ScrollbarAxis::Vertical)
                    .child(if state.conversations.is_empty() {
                        div()
                            .child("No chat history yet")
                            .text_color(theme.muted_foreground)
                            .p_2()
                            .text_sm()
                    } else {
                        div().children(state.conversations.iter().map(|c| {
                            let id = c.id;
                            self.render_custom_item(
                                &c.title,
                                "icons/session.svg",
                                Some(id) == active_id,
                                theme,
                                {
                                    let state = self.state.clone();
                                    move |_, _, cx| {
                                        state.update(cx, |state, cx| {
                                            state.select_conversation(id, cx);
                                        });
                                    }
                                },
                            )
                            .into_any_element()
                        }))
                    }),
            )
            .child(
                // Footer
                div()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(theme.border)
                    // User Profile Section
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .p_4()
                            .gap_3()
                            .child(
                                // Avatar
                                Avatar::new()
                                    .src("https://avatars.githubusercontent.com/u/1?v=4")
                                    .size(px(36.0))
                                    .rounded_full(),
                            )
                            .child(
                                // Text Info
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .child("Buggy")
                                            .font_weight(FontWeight::BOLD)
                                            .text_sm(),
                                    )
                                    .child(
                                        div()
                                            .child("buggy.d.code@gmail.com")
                                            .text_xs()
                                            .text_color(theme.muted_foreground),
                                    ),
                            ),
                    )
                    // Menu Items
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .p_2()
                            .pt(px(0.0))
                            .child({
                                let (label, icon_name, custom_icon) =
                                    match state.theme_mode.as_str() {
                                        "dark" => ("Theme: Dark", Some(IconName::Moon), None),
                                        "system" => {
                                            ("Theme: System", None, Some("icons/system_theme.svg"))
                                        }
                                        _ => ("Theme: Light", Some(IconName::Sun), None),
                                    };

                                if let Some(icon) = icon_name {
                                    self.render_menu_item(label, icon, theme, {
                                        let state = self.state.clone();
                                        move |_, _, cx| {
                                            println!(
                                                "[SIDEBAR] Theme toggle (render_menu_item) clicked"
                                            );
                                            state.update(cx, |state, cx| {
                                                state.toggle_theme(cx);
                                            });
                                        }
                                    })
                                    .into_any_element()
                                } else if let Some(path) = custom_icon {
                                    self.render_custom_item(
                                        label,
                                        path,
                                        false, // Theme toggle doesn't show "active" state in the same way as nav items
                                        theme,
                                        {
                                            let state = self.state.clone();
                                            move |_, _, cx| {
                                                println!("[SIDEBAR] Theme toggle (Custom) clicked");
                                                state.update(cx, |state, cx| {
                                                    state.toggle_theme(cx);
                                                });
                                            }
                                        },
                                    )
                                    .into_any_element()
                                } else {
                                    div().into_any_element()
                                }
                            })
                            .child(self.render_custom_item(
                                "Account Settings",
                                "icons/account_settings.svg",
                                false,
                                theme,
                                {
                                    let state = self.state.clone();
                                    move |_, _, cx| {
                                        state.update(cx, |state, cx| {
                                            state.toggle_account_settings(cx);
                                        });
                                    }
                                },
                            ))
                            .child(self.render_menu_item(
                                "Profile Settings",
                                IconName::Settings,
                                theme,
                                {
                                    let state = self.state.clone();
                                    move |_, _, cx| {
                                        state.update(cx, |state, cx| {
                                            state.toggle_profile_settings(cx);
                                        });
                                    }
                                },
                            ))
                            .child(self.render_custom_item(
                                "Sign Out",
                                "icons/signout.svg",
                                false,
                                theme,
                                |_, _, _| {
                                    // TODO: Implement sign out logic
                                    println!("Sign out clicked");
                                },
                            )),
                    ),
            )
    }
}
