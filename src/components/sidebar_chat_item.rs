use gpui::{
    App, Entity, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, RenderOnce,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
use std::rc::Rc;
use ui::{ActiveTheme, Icon, IconName, StyledExt, h_flex, input::Input, v_flex};

#[derive(IntoElement)]
pub struct ChatSessionItem {
    id: String,
    title: String,
    created_at: String,
    is_active: bool,
    is_editing: bool,
    is_deleting: bool,
    input: Option<Entity<ui::input::InputState>>,
    on_select: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_edit: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_delete: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_cancel_edit: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_submit_edit: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_cancel_delete: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_confirm_delete: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
}

impl ChatSessionItem {
    pub fn new(id: String, title: String, created_at: String, is_active: bool) -> Self {
        Self {
            id,
            title,
            created_at,
            is_active,
            is_editing: false,
            is_deleting: false,
            input: None,
            on_select: None,
            on_edit: None,
            on_delete: None,
            on_cancel_edit: None,
            on_submit_edit: None,
            on_cancel_delete: None,
            on_confirm_delete: None,
        }
    }

    pub fn is_editing(mut self, is_editing: bool) -> Self {
        self.is_editing = is_editing;
        self
    }

    pub fn is_deleting(mut self, is_deleting: bool) -> Self {
        self.is_deleting = is_deleting;
        self
    }

    pub fn input(mut self, input: Option<Entity<ui::input::InputState>>) -> Self {
        self.input = input;
        self
    }

    pub fn on_select(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(callback));
        self
    }

    pub fn on_edit(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_edit = Some(Rc::new(callback));
        self
    }

    pub fn on_delete(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_delete = Some(Rc::new(callback));
        self
    }

    pub fn on_cancel_edit(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel_edit = Some(Rc::new(callback));
        self
    }

    pub fn on_submit_edit(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_submit_edit = Some(Rc::new(callback));
        self
    }

    pub fn on_cancel_delete(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel_delete = Some(Rc::new(callback));
        self
    }

    pub fn on_confirm_delete(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_confirm_delete = Some(Rc::new(callback));
        self
    }

    fn relative_time(&self) -> String {
        use chrono::NaiveDateTime;
        use std::time::SystemTime;

        let now = SystemTime::now();
        let dt = NaiveDateTime::parse_from_str(&self.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| SystemTime::from(dt.and_utc()))
            .unwrap_or(SystemTime::now());

        let duration = now.duration_since(dt).unwrap_or_default();
        let secs = duration.as_secs();

        if secs < 60 {
            "Just now".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("{}m ago", mins)
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!("{}h ago", hours)
        } else if secs < 604800 {
            let days = secs / 86400;
            format!("{}d ago", days)
        } else if secs < 2592000 {
            let weeks = secs / 604800;
            format!("{}w ago", weeks)
        } else if secs < 31536000 {
            let months = secs / 2592000;
            format!("{}mo ago", months)
        } else {
            let years = secs / 31536000;
            format!("{}y ago", years)
        }
    }
}

impl RenderOnce for ChatSessionItem {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        if self.is_deleting {
            return div()
                .w_full()
                .p_2()
                .bg(theme.danger.opacity(0.1))
                .rounded_md()
                .border_1()
                .border_color(theme.danger)
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(theme.danger)
                                .child("Delete this chat?"),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .id("cancel-delete")
                                        .cursor_pointer()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.background)
                                        .border_1()
                                        .border_color(theme.border)
                                        .text_xs()
                                        .child("Cancel")
                                        .on_click({
                                            let callback = self.on_cancel_delete.clone();
                                            move |_, window, cx| {
                                                cx.stop_propagation();
                                                if let Some(cb) = callback.as_ref() {
                                                    cb(window, cx);
                                                }
                                            }
                                        }),
                                )
                                .child(
                                    div()
                                        .id("confirm-delete")
                                        .cursor_pointer()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .bg(theme.danger)
                                        .text_color(theme.danger_foreground)
                                        .text_xs()
                                        .child("Delete")
                                        .on_click({
                                            let callback = self.on_confirm_delete.clone();
                                            move |_, window, cx| {
                                                cx.stop_propagation();
                                                if let Some(cb) = callback.as_ref() {
                                                    cb(window, cx);
                                                }
                                            }
                                        }),
                                ),
                        ),
                )
                .into_any_element();
        }

        if self.is_editing {
            return div()
                .w_full()
                .p_1()
                .on_key_down({
                    let on_cancel = self.on_cancel_edit.clone();
                    move |event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key == "escape" {
                            cx.stop_propagation();
                            if let Some(cb) = on_cancel.as_ref() {
                                cb(window, cx);
                            }
                        }
                    }
                })
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().flex_1().when_some(self.input, |this, state| {
                            this.child(Input::new(&state).appearance(false))
                        }))
                        .child(
                            div()
                                .id("submit-rename")
                                .cursor_pointer()
                                .p_1()
                                .rounded_md()
                                .hover(|s| s.bg(theme.sidebar_accent))
                                .child(Icon::new(IconName::Check).size_3())
                                .on_click({
                                    let callback = self.on_submit_edit.clone();
                                    move |_, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(cb) = callback.as_ref() {
                                            cb(window, cx);
                                        }
                                    }
                                }),
                        )
                        .child(
                            div()
                                .id("cancel-rename")
                                .cursor_pointer()
                                .p_1()
                                .rounded_md()
                                .hover(|s| s.bg(theme.sidebar_accent))
                                .child(Icon::new(IconName::Close).size_3())
                                .on_click({
                                    let callback = self.on_cancel_edit.clone();
                                    move |_, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(cb) = callback.as_ref() {
                                            cb(window, cx);
                                        }
                                    }
                                }),
                        ),
                )
                .into_any_element();
        }

        let id = self.id.clone();

        div()
            .id(gpui::SharedString::from(id.clone()))
            .group("session-item")
            .w_full()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(theme.sidebar_accent))
            .when(self.is_active, |s| {
                s.bg(theme.sidebar_accent)
                    .text_color(theme.sidebar_accent_foreground)
            })
            .when(!self.is_active, |s| s.text_color(theme.sidebar_foreground))
            .when_some(self.on_select.clone(), |this, callback| {
                this.on_click(move |_, window, cx| callback(window, cx))
            })
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .p_2()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .overflow_hidden()
                            .child(
                                Icon::new(IconName::SquareTerminal)
                                    .size_4()
                                    .text_color(theme.muted_foreground),
                            )
                            .child(div().text_sm().truncate().child(self.title.clone())),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            // Show time by default
                            .child(
                                div()
                                    .group_hover("session-item", |s| s.hidden())
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(self.relative_time()),
                            )
                            // Show actions on hover
                            .child(
                                h_flex()
                                    .invisible()
                                    .group_hover("session-item", |s| s.visible())
                                    .gap_1()
                                    .child(
                                        div()
                                            .id("edit-session")
                                            .cursor_pointer()
                                            .p_1()
                                            .rounded_md()
                                            .hover(|s| s.bg(theme.background))
                                            .child(Icon::new(IconName::Replace).size_3())
                                            .on_click({
                                                let callback = self.on_edit.clone();
                                                move |_, window, cx| {
                                                    println!("[DEBUG] Edit button clicked");
                                                    cx.stop_propagation();
                                                    if let Some(cb) = callback.as_ref() {
                                                        cb(window, cx);
                                                        println!("[DEBUG] Edit callback invoked");
                                                    }
                                                }
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("delete-session")
                                            .cursor_pointer()
                                            .p_1()
                                            .rounded_md()
                                            .hover(|s| {
                                                s.bg(theme.danger)
                                                    .text_color(theme.danger_foreground)
                                            })
                                            .child(Icon::new(IconName::Delete).size_3())
                                            .on_click({
                                                let callback = self.on_delete.clone();
                                                move |_, window, cx| {
                                                    println!("[DEBUG] Delete button clicked");
                                                    cx.stop_propagation();
                                                    if let Some(cb) = callback.as_ref() {
                                                        cb(window, cx);
                                                        println!("[DEBUG] Delete callback invoked");
                                                    }
                                                }
                                            }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }
}
