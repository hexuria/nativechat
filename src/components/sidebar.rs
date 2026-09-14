use crate::chrome::{
    AVATAR_PX, MASCOT_BOX_PX, RAIL_HOVER, RAIL_HOVER_ALPHA, SIDEBAR_GAP, SIDEBAR_ROW,
};
use crate::components::persona::PersonaMark;
use crate::icons::NativeIcon;
use crate::state::{AppSettingsTab, AppState, Conversation};
use chrono::NaiveDateTime;
use gpui_kit::assets::IconNamed;
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::{Duration, SystemTime};

#[derive(Clone, PartialEq, Eq)]
struct RailCoworker {
    id: String,
    name: String,
    shape: Option<String>,
    color: Option<String>,
    preview: String,
    time: String,
}

#[derive(Clone, PartialEq, Eq)]
struct SidebarRev {
    collapsed: bool,
    hidden: bool,
    expanded_width: i32,
    theme_mode: String,
    active_id: Option<String>,
    any_modal: bool,
    coworkers: Vec<RailCoworker>,
    pinned: Vec<String>,
    hidden_ids: Vec<String>,
    renaming: Option<String>,
    account_label: String,
    account_email: String,
}

impl SidebarRev {
    fn from_state(state: &AppState) -> Self {
        Self {
            collapsed: state.sidebar_collapsed,
            hidden: state.sidebar_hidden,
            expanded_width: state.sidebar_expanded_width.round() as i32,
            theme_mode: state.theme_mode.clone(),
            active_id: state.active_coworker_id.clone(),
            any_modal: state.is_voice_mode_open
                || state.is_app_settings_open
                || state.is_profile_settings_open
                || state.is_credentials_modal_open,
            coworkers: state
                .coworkers
                .iter()
                .map(|c| {
                    let (preview, time) = rail_preview(
                        state.conversations.iter().find(|conv| conv.id == c.id),
                    );
                    RailCoworker {
                        id: c.id.clone(),
                        name: c.name.clone(),
                        shape: c.avatar_shape.clone(),
                        color: c.avatar_color.clone(),
                        preview,
                        time,
                    }
                })
                .collect(),
            pinned: {
                let mut ids: Vec<_> = state.pinned_coworker_ids.iter().cloned().collect();
                ids.sort();
                ids
            },
            hidden_ids: {
                let mut ids: Vec<_> = state.hidden_coworker_ids.iter().cloned().collect();
                ids.sort();
                ids
            },
            renaming: state.renaming_coworker_id.clone(),
            account_label: state
                .account
                .as_ref()
                .map(|a| a.display_name())
                .unwrap_or_else(|| "Account".into()),
            account_email: state
                .account
                .as_ref()
                .map(|a| a.email.clone())
                .unwrap_or_default(),
        }
    }
}

pub struct SidebarView {
    state: Entity<AppState>,
    list_scroll: ScrollHandle,
    rename_input: Entity<InputState>,
    rev: SidebarRev,
}

impl SidebarView {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let rename_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let rev = SidebarRev::from_state(&state.read(cx));
        cx.observe(&state, |this, state, cx| {
            let rev = SidebarRev::from_state(&state.read(cx));
            if this.rev != rev {
                this.rev = rev;
                cx.notify();
            }
        })
        .detach();
        cx.subscribe_in(&rename_input, window, |this, input, event, _window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let name = input.read(cx).value().to_string();
                this.state.update(cx, |state, cx| {
                    state.commit_rename_coworker(name, cx);
                });
            }
        })
        .detach();

        Self {
            state,
            list_scroll: ScrollHandle::new(),
            rename_input,
            rev,
        }
    }
}

fn hover_fill() -> Hsla {
    rgb(RAIL_HOVER).opacity(RAIL_HOVER_ALPHA).into()
}

fn row() -> Div {
    div()
        .h(px(SIDEBAR_ROW))
        .min_h(px(SIDEBAR_ROW))
        .flex()
        .items_center()
        .flex_shrink_0()
}

fn kit_icon(path: &'static str, size: f32, color: Hsla) -> Icon {
    Icon::default().path(path).size(px(size)).text_color(color)
}

impl Render for SidebarView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let collapsed = state.sidebar_collapsed;
        let hidden = state.sidebar_hidden;
        if hidden {
            return div().id("sidebar").w(px(0.)).h_full().overflow_hidden();
        }
        let theme = cx.theme().clone();
        let theme_mode = state.theme_mode.clone();
        let coworkers = state.coworkers.clone();
        let conversations = state.conversations.clone();
        let rail_nudge = if collapsed {
            SIDEBAR_ROW + 8.0
        } else {
            (state.sidebar_expanded_width - 16.0).max(SIDEBAR_ROW + 8.0)
        };
        let active_coworker = state.active_coworker_id.clone();
        let any_modal_open = state.is_voice_mode_open
            || state.is_app_settings_open
            || state.is_profile_settings_open;
        let account_label = state
            .account
            .as_ref()
            .map(|a| a.display_name())
            .unwrap_or_else(|| "Account".into());
        let account_email = state
            .account
            .as_ref()
            .map(|a| a.email.clone())
            .unwrap_or_default();
        let account_id = state
            .account
            .as_ref()
            .map(|a| a.id.clone())
            .unwrap_or_else(|| "account".into());
        let view = cx.entity().clone();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let icon_color = fg;
        let rail_hover = hover_fill();

        let app = self.state.clone();
        v_flex()
            .id("sidebar")
            .size_full()
            .flex_shrink_0()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                app.update(cx, |state, cx| {
                    if state.model_picker_open || state.avatar_editor_open {
                        state.dismiss_popovers(cx);
                    }
                });
            })
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .id("sidebar-main")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .gap(px(SIDEBAR_GAP))
                    .child(self.brand_row(collapsed, any_modal_open, fg, muted, rail_hover, view.clone()))
                    .child(self.search_row(collapsed, icon_color, rail_hover))
                    .child(
                        div()
                            .id("sidebar-chat-list")
                            .flex_1()
                            .min_h(px(0.))
                            .w_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.list_scroll)
                            .child(
                                v_flex()
                                    .w_full()
                                    .gap(px(SIDEBAR_GAP))
                                    .when(collapsed, |this| this.items_center())
                                    .when(!collapsed, |this| this.px(px(12.)))
                                    .pb(px(12.))
                                    .children({
                                        let mut rows: Vec<_> = coworkers.into_iter().collect();
                                        let hidden = state.hidden_coworker_ids.clone();
                                        let pinned = state.pinned_coworker_ids.clone();
                                        let renaming = state.renaming_coworker_id.clone();
                                        let rename_input = self.rename_input.clone();
                                        let app = self.state.clone();
                                        let list_view = view.clone();
                                        rows.retain(|c| !hidden.contains(&c.id));
                                        rows.sort_by_key(|c| !pinned.contains(&c.id));
                                        rows.into_iter().map(move |c| {
                                        let id = c.id.clone();
                                        let name = c.name.clone();
                                        let is_active = Some(id.clone()) == active_coworker;
                                        let is_renaming = renaming.as_ref() == Some(&id);
                                        let is_pinned = pinned.contains(&id);
                                        let (preview, when) = rail_preview(
                                            conversations.iter().find(|conv| conv.id == id),
                                        );
                                        let view = list_view.clone();
                                        let app = app.clone();
                                        let row = row()
                                            .id(SharedString::from(format!("coworker-{id}")))
                                            .when(collapsed, |this| {
                                                this.w(px(SIDEBAR_ROW))
                                                    .justify_center()
                                                    .px(px(9.))
                                                    .rounded(px(10.))
                                            })
                                            .when(!collapsed, |this| {
                                                this.w_full()
                                                    .px(px(9.))
                                                    .gap(px(9.))
                                                    .rounded(px(10.))
                                            })
                                            .cursor_pointer()
                                            .when(!collapsed, |this| {
                                                this.hover(|s| s.bg(rail_hover)).when(is_active, |this| {
                                                    this.bg(rail_hover)
                                                })
                                            })
                                            .on_mouse_down(MouseButton::Left, {
                                                let id = id.clone();
                                                let view = view.clone();
                                                move |_, _, cx| {
                                                    view.update(cx, |this, cx| {
                                                        this.state.update(cx, |state, cx| {
                                                            if state.renaming_coworker_id.as_ref()
                                                                != Some(&id)
                                                            {
                                                                state.cancel_rename_coworker(cx);
                                                            }
                                                            state.select_coworker(id.clone(), cx);
                                                        });
                                                    });
                                                }
                                            })
                                            .context_menu({
                                                let app = app.clone();
                                                let view = view.clone();
                                                let id = id.clone();
                                                let name = name.clone();
                                                move |menu, _, _cx| {
                                                    agent_menu(
                                                        menu,
                                                        app.clone(),
                                                        view.clone(),
                                                        id.clone(),
                                                        name.clone(),
                                                        is_pinned,
                                                    )
                                                }
                                            })
                                            .child(
                                                PersonaMark::new(id.clone())
                                                    .shape(c.avatar_shape.clone())
                                                    .color(c.avatar_color.clone())
                                                    .size(px(AVATAR_PX))
                                                    .lit(is_active),
                                            );
                                        let trigger = if collapsed {
                                            row.into_any_element()
                                        } else if is_renaming {
                                            row.child(
                                                div()
                                                    .min_w(px(0.))
                                                    .flex_1()
                                                    .child(
                                                        Input::new(&rename_input)
                                                            .appearance(false)
                                                            .focus_bordered(false)
                                                            .h(px(28.)),
                                                    ),
                                            )
                                            .into_any_element()
                                        } else {
                                            row.child(
                                                div()
                                                    .min_w(px(0.))
                                                    .flex_1()
                                                    .text_sm()
                                                    .truncate()
                                                    .child(name.clone()),
                                            )
                                            .into_any_element()
                                        };
                                        HoverCard::new(SharedString::from(format!(
                                            "coworker-hover-{id}"
                                        )))
                                        .anchor(Anchor::TopLeft)
                                        .appearance(false)
                                        .open_delay(Duration::from_millis(80))
                                        .close_delay(Duration::from_millis(140))
                                        .when(!collapsed, |this| this.w_full())
                                        .trigger(trigger)
                                        .content({
                                            let id = id.clone();
                                            let name = name.clone();
                                            let shape = c.avatar_shape.clone();
                                            let color = c.avatar_color.clone();
                                            move |_, _, cx| {
                                                let theme = cx.theme();
                                                agent_hover_card(
                                                    id.clone(),
                                                    name.clone(),
                                                    shape.clone(),
                                                    color.clone(),
                                                    preview.clone(),
                                                    when.clone(),
                                                    rail_nudge,
                                                    theme.is_dark(),
                                                    theme.muted_foreground,
                                                    theme.foreground,
                                                    theme.border,
                                                )
                                            }
                                        })
                                        .into_any_element()
                                    })}),
                            ),
                    ),
            )
            .child(self.dock(
                collapsed,
                theme_mode,
                account_id,
                account_label,
                account_email,
                any_modal_open,
                fg,
                muted,
                rail_hover,
                view,
            ))
    }
}

impl SidebarView {
    fn brand_row(
        &self,
        collapsed: bool,
        any_modal_open: bool,
        fg: Hsla,
        muted: Hsla,
        hover: Hsla,
        view: Entity<Self>,
    ) -> impl IntoElement {
        row()
            .id("sidebar-header")
            .w_full()
            .when(collapsed, |this| this.justify_center())
            .when(!collapsed, |this| this.px(px(12.)).pl(px(16.)).gap(px(8.)))
            .when(collapsed, |this| {
                this.child(
                    div()
                        .id("nav-toggle-sidebar")
                        .w(px(SIDEBAR_ROW))
                        .h(px(SIDEBAR_ROW))
                        .rounded(px(10.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(hover))
                        .on_mouse_down(MouseButton::Left, {
                            let view = view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.state.update(cx, |state, cx| {
                                        state.set_sidebar_collapsed(false, false, cx);
                                    });
                                });
                            }
                        })
                        .child(kit_icon("icons/panel-left.svg", 16.0, fg)),
                )
            })
            .when(!collapsed, |this| {
                this.child(
                    div()
                        .size(px(MASCOT_BOX_PX))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .child(
                            PersonaMark::new("nativechat-brand")
                                .shape(Some("blob"))
                                .color(Some("cyan"))
                                .size(px(AVATAR_PX)),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("NativeChat"),
                )
                .child(
                    div()
                        .id("nav-new-chat")
                        .ml_auto()
                        .size(px(28.))
                        .rounded(px(8.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(muted)
                        .cursor_pointer()
                        .hover(|s| s.bg(hover).text_color(fg))
                        .when(!any_modal_open, |this| {
                            this.on_mouse_down(MouseButton::Left, {
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.state.update(cx, |state, cx| {
                                            state.create_agent(cx);
                                        });
                                    });
                                }
                            })
                        })
                        .child(Icon::new(IconName::Plus).size(px(16.))),
                )
            })
    }

    fn search_row(&self, collapsed: bool, icon_color: Hsla, hover: Hsla) -> impl IntoElement {
        if collapsed {
            row()
                .id("nav-search")
                .w_full()
                .justify_center()
                .child(
                    div()
                        .w(px(SIDEBAR_ROW))
                        .h(px(SIDEBAR_ROW))
                        .rounded(px(10.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(hover))
                        .child(Icon::new(IconName::Search).size(px(16.)).text_color(icon_color)),
                )
        } else {
            row()
                .id("nav-search")
                .w_full()
                .px(px(12.))
                .child(
                    h_flex()
                        .w_full()
                        .h(px(36.))
                        .px(px(8.))
                        .gap(px(8.))
                        .rounded(px(10.))
                        .border_1()
                        .border_color(rgb(0xfcfcfc).opacity(0.15))
                        .items_center()
                        .cursor_pointer()
                        .child(Icon::new(IconName::Search).size(px(16.)).text_color(icon_color))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xfcfcfc).opacity(0.4))
                                .child("Search"),
                        ),
                )
        }
    }

    fn dock(
        &self,
        collapsed: bool,
        theme_mode: String,
        account_id: String,
        account_label: String,
        account_email: String,
        _any_modal_open: bool,
        fg: Hsla,
        muted: Hsla,
        hover: Hsla,
        view: Entity<Self>,
    ) -> impl IntoElement {
        let (theme_label, theme_icon) = match theme_mode.as_str() {
            "dark" => ("Theme: Dark", "icons/moon.svg"),
            _ => ("Theme: Light", "icons/sun.svg"),
        };

        v_flex()
            .id("sidebar-footer")
            .flex_shrink_0()
            .w_full()
            .gap(px(SIDEBAR_GAP))
            .when(collapsed, |this| this.pb(px(8.)).items_center())
            .when(!collapsed, |this| this.px(px(12.)).pb(px(8.)))
            .child(self.dock_item(
                "nav-library",
                collapsed,
                NativeIcon::Collections.path(),
                "Collections",
                fg,
                hover,
                None,
            ))
            .child(self.dock_item(
                "nav-projects",
                collapsed,
                "icons/groups.svg",
                "Groups",
                fg,
                hover,
                None,
            ))
            .child(self.dock_item(
                "footer-plugins",
                collapsed,
                NativeIcon::Plugins.path(),
                "Plugins",
                fg,
                hover,
                None,
            ))
            .child({
                let view = view.clone();
                self.dock_item(
                    "footer-theme",
                    collapsed,
                    theme_icon,
                    theme_label,
                    fg,
                    hover,
                    Some(Box::new(move |cx: &mut App| {
                        view.update(cx, |this, cx| {
                            this.state.update(cx, |state, cx| {
                                state.toggle_theme(cx);
                            });
                        });
                    })),
                )
            })
            .child({
                let view = view.clone();
                let label = account_label.clone();
                let email = account_email.clone();
                row()
                    .id("footer-account")
                    .when(collapsed, |this| {
                        this.w(px(SIDEBAR_ROW))
                            .justify_center()
                            .px(px(9.))
                            .rounded(px(10.))
                    })
                    .when(!collapsed, |this| {
                        this.w_full().px(px(8.)).gap(px(8.)).rounded(px(10.))
                    })
                    .cursor_pointer()
                    .when(!collapsed, |this| this.hover(|s| s.bg(hover)))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.update(cx, |state, cx| {
                                state.open_app_settings(AppSettingsTab::Profile, cx);
                            });
                        });
                    })
                    .child(
                        PersonaMark::new(account_id)
                            .shape(Some("squircle"))
                            .color(Some("gray"))
                            .size(px(AVATAR_PX)),
                    )
                    .when(!collapsed, |this| {
                        this.child(
                            v_flex()
                                .min_w(px(0.))
                                .flex_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .truncate()
                                        .child(label),
                                )
                                .when(!email.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .truncate()
                                            .child(email),
                                    )
                                }),
                        )
                    })
            })
    }

    fn dock_item(
        &self,
        id: &'static str,
        collapsed: bool,
        icon: impl Into<SharedString>,
        label: &'static str,
        fg: Hsla,
        hover: Hsla,
        on_click: Option<Box<dyn Fn(&mut App) + 'static>>,
    ) -> impl IntoElement {
        let icon = icon.into();
        row()
            .id(id)
            .when(collapsed, |this| {
                this.w(px(SIDEBAR_ROW))
                    .justify_center()
                    .px(px(9.))
                    .rounded(px(10.))
            })
            .when(!collapsed, |this| {
                this.w_full().px(px(8.)).gap(px(10.)).rounded(px(10.))
            })
            .cursor_pointer()
            .hover(|s| s.bg(hover))
            .when_some(on_click, |this, on_click| {
                this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    on_click(cx);
                })
            })
            .child(Icon::default().path(icon).size(px(16.)).text_color(fg))
            .when(!collapsed, |this| this.child(div().text_sm().child(label)))
    }
}

fn rail_preview(conversation: Option<&Conversation>) -> (String, String) {
    let Some(conversation) = conversation else {
        return ("No messages yet".into(), String::new());
    };
    let last = conversation
        .messages
        .iter()
        .rev()
        .find(|message| !message.content.trim().is_empty());
    let preview = last
        .map(|message| rail_preview_text(&message.content))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "No messages yet".into());
    let time = match last {
        Some(message) => rail_card_time(message.sent_at),
        None => NaiveDateTime::parse_from_str(&conversation.created_at, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|dt| rail_card_time(SystemTime::from(dt.and_utc())))
            .unwrap_or_default(),
    };
    (preview, time)
}

fn rail_preview_text(content: &str) -> String {
    let collapsed = content.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX: usize = 140;
    let count = collapsed.chars().count();
    if count > MAX {
        format!("{}…", collapsed.chars().take(MAX).collect::<String>())
    } else {
        collapsed
    }
}

fn rail_card_time(at: SystemTime) -> String {
    let at = chrono::DateTime::<chrono::Local>::from(at);
    let today = chrono::Local::now().date_naive();
    let then = at.date_naive();
    match (today - then).num_days() {
        0 => at.format("%l:%M %p").to_string().trim().to_string(),
        1 => "Yesterday".into(),
        2..=6 => at.format("%A").to_string(),
        _ => at.format("%b %d").to_string(),
    }
}

fn agent_hover_card(
    id: String,
    name: String,
    shape: Option<String>,
    color: Option<String>,
    preview: String,
    time: String,
    nudge: f32,
    dark: bool,
    muted: Hsla,
    foreground: Hsla,
    border: Hsla,
) -> impl IntoElement {
    let panel_bg = if dark {
        rgb(0x1c1c1c)
    } else {
        rgb(0xffffff)
    };
    h_flex()
        .child(div().w(px(nudge)).h(px(1.)))
        .child(
            v_flex()
                .id(SharedString::from(format!("coworker-preview-{id}")))
                .w(px(260.))
                .p(px(10.))
                .gap(px(4.))
                .rounded(px(10.))
                .border_1()
                .border_color(border)
                .bg(panel_bg)
                .text_color(foreground)
                .shadow_lg()
                .occlude()
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            PersonaMark::new(id)
                                .shape(shape)
                                .color(color)
                                .size(px(16.))
                                .dark(dark),
                        )
                        .child(
                            div()
                                .min_w(px(0.))
                                .flex_1()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(name),
                        )
                        .when(!time.is_empty(), |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(time),
                            )
                        }),
                )
                .child(
                    div()
                        .w_full()
                        .text_xs()
                        .text_color(muted)
                        .child(preview),
                ),
        )
}

fn agent_menu(
    menu: PopupMenu,
    app: Entity<AppState>,
    view: Entity<SidebarView>,
    id: String,
    name: String,
    pinned: bool,
) -> PopupMenu {
    menu.item(
        PopupMenuItem::new("Pin")
            .checked(pinned)
            .on_click({
                let app = app.clone();
                let id = id.clone();
                move |_, _, cx| {
                    app.update(cx, |state, cx| state.toggle_pin_coworker(id.clone(), cx));
                }
            }),
    )
    .item(PopupMenuItem::new("Move to new section").disabled(true))
    .item(PopupMenuItem::new("Mark as Read").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.mark_coworker_read(&id, cx));
        }
    }))
    .separator()
    .item(PopupMenuItem::new("Rename Bot").on_click({
        let app = app.clone();
        let id = id.clone();
        let name = name.clone();
        move |_, window, cx| {
            view.update(cx, |this, cx| {
                this.rename_input.update(cx, |input, cx| {
                    input.set_value(name.clone(), window, cx);
                    input.focus_handle(cx).focus(window, cx);
                });
            });
            app.update(cx, |state, cx| {
                state.begin_rename_coworker(id.clone(), cx);
            });
        }
    }))
    .item(PopupMenuItem::new("Edit Profile").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.open_agent_profile(id.clone(), cx));
        }
    }))
    .item(PopupMenuItem::new("Duplicate").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.duplicate_coworker(id.clone(), cx));
        }
    }))
    .item(PopupMenuItem::new("Copy conversation ID").on_click({
        let id = id.clone();
        move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(id.clone()));
        }
    }))
    .separator()
    .item(PopupMenuItem::new("Hide from sidebar").on_click({
        let app = app.clone();
        let id = id.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.hide_coworker(id.clone(), cx));
        }
    }))
    .item(PopupMenuItem::new("Delete").on_click({
        let app = app.clone();
        move |_, _, cx| {
            app.update(cx, |state, cx| state.delete_coworker(id.clone(), cx));
        }
    }))
}
