use crate::actions::{BranchInNewChat, ReportMessage, ToggleReadAloud};
use gpui::{prelude::FluentBuilder, *};
use std::rc::Rc;
use std::time::Duration;
use ui::{
    ActiveTheme, Icon, IconName, Sizable, Size,
    button::{Button, ButtonVariants},
    h_flex,
    menu::DropdownMenu,
    tooltip::Tooltip,
};

#[derive(IntoElement)]
pub struct MessageActions {
    message_id: String,
    message_text: String,
    on_copy: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    can_read_aloud: bool,
    is_speaking: bool,
    is_paused: bool,
    is_loading: bool,
    state: Option<WeakEntity<AppState>>,
}

enum IconSource {
    Name(IconName),
    Path(&'static str),
}

impl MessageActions {
    pub fn new(message_id: impl Into<String>) -> Self {
        Self {
            message_id: message_id.into(),
            message_text: String::new(),
            on_copy: None,
            can_read_aloud: false,
            is_speaking: false,
            is_paused: false,
            is_loading: false,
        }
    }

    /// Set the message text to copy when the copy button is clicked
    pub fn message_text(mut self, text: impl Into<String>) -> Self {
        self.message_text = text.into();
        self
    }

    /// Set a callback to be invoked after copying
    pub fn on_copy<F>(mut self, handler: F) -> Self
    where
        F: Fn(&mut Window, &mut App) + 'static,
    {
        self.on_copy = Some(Rc::new(handler));
        self
    }

    pub fn can_read_aloud(mut self, can: bool) -> Self {
        self.can_read_aloud = can;
        self
    }

    pub fn is_speaking(mut self, is_speaking: bool) -> Self {
        self.is_speaking = is_speaking;
        self
    }

    pub fn is_paused(mut self, is_paused: bool) -> Self {
        self.is_paused = is_paused;
        self
    }

    pub fn is_loading(mut self, is_loading: bool) -> Self {
        self.is_loading = is_loading;
        self
    }

    fn action_button(
        &self,
        id: impl Into<ElementId>,
        icon: IconSource,
        tooltip_text: &'static str,
        cx: &mut App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;

        let icon_element = match icon {
            IconSource::Name(name) => Icon::new(name)
                .size(px(18.0))
                .text_color(secondary_foreground)
                .into_any_element(),
            IconSource::Path(path) => svg()
                .path(path)
                .size(px(18.0))
                .text_color(secondary_foreground)
                .into_any_element(),
        };

        div()
            .id(id)
            .w(px(32.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_color(secondary_foreground)
            .hover(move |style| style.bg(secondary))
            .cursor_pointer()
            .tooltip(move |w, cx| Tooltip::new(tooltip_text).build(w, cx))
            .child(icon_element)
    }

    fn copy_button(
        &self,
        id: impl Into<ElementId>,
        tooltip_text: &'static str,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;
        let success = theme.success;

        let id = id.into();
        let state = window.use_keyed_state(id.clone(), cx, |_, _| CopyState::default());
        let copied = state.read(cx).copied;

        let icon_name = if copied {
            IconName::Check
        } else {
            IconName::Copy
        };
        let icon_color = if copied {
            success
        } else {
            secondary_foreground
        };

        let message_text = self.message_text.clone();
        let on_copy = self.on_copy.clone();

        div()
            .id(id)
            .w(px(32.0))
            .h(px(32.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .text_color(icon_color)
            .hover(move |style| style.bg(secondary))
            .cursor_pointer()
            .tooltip(move |w, cx| {
                Tooltip::new(if copied { "Copied!" } else { tooltip_text }).build(w, cx)
            })
            .child(Icon::new(icon_name).size(px(18.0)).text_color(icon_color))
            .when(!copied, move |this| {
                this.on_click({
                    let state = state.clone();
                    let message_text = message_text.clone();
                    let on_copy = on_copy.clone();
                    move |_, window, cx| {
                        cx.stop_propagation();
                        cx.write_to_clipboard(ClipboardItem::new_string(message_text.clone()));

                        state.update(cx, |state, cx| {
                            state.copied = true;
                            cx.notify();
                        });

                        // Reset after 2 seconds
                        let state = state.clone();
                        cx.spawn(async move |cx| {
                            cx.background_executor().timer(Duration::from_secs(2)).await;
                            _ = state.update(cx, |state, cx| {
                                state.copied = false;
                                cx.notify();
                            });
                        })
                        .detach();

                        if let Some(on_copy) = &on_copy {
                            on_copy(window, cx);
                        }
                    }
                })
            })
    }
}

#[derive(Default)]
struct CopyState {
    copied: bool,
}

impl RenderOnce for MessageActions {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_1()
            .items_center()
            .child(self.copy_button(
                ElementId::Name(format!("copy-{}", self.message_id).into()),
                "Copy",
                window,
                cx,
            ))
            .child(self.action_button(
                "like",
                IconSource::Path("icons/thumbs_up.svg"),
                "Good response",
                cx,
            ))
            .child(self.action_button(
                "dislike",
                IconSource::Path("icons/thumbs_down.svg"),
                "Bad response",
                cx,
            ))
            .child(self.action_button("share", IconSource::Path("icons/share.svg"), "Share", cx))
            .child(self.action_button(
                "regenerate",
                IconSource::Path("icons/reset.svg"),
                "Try again",
                cx,
            ))
            .child(
                Button::new(ElementId::Name(format!("more-{}", self.message_id).into()))
                    .icon(IconName::Ellipsis)
                    .ghost()
                    .with_size(Size::Size(px(32.0)))
                    .rounded(px(6.0))
                    .tooltip("More actions")
                    .dropdown_menu_with_anchor(Corner::BottomLeft, move |menu, _, _| {
                        menu.menu_with_icon(
                            "Branch in new chat",
                            IconName::Branch,
                            Box::new(BranchInNewChat),
                        )
                        .when(self.can_read_aloud, |menu| {
                            if self.is_loading {
                                menu.menu_with_icon(
                                    "Loading...",
                                    IconName::Loader, // Assuming Loader icon exists, or use another
                                    Box::new(ToggleReadAloud {
                                        text: self.message_text.clone(),
                                        message_id: self.message_id.clone(),
                                    }),
                                )
                            } else if self.is_speaking {
                                if self.is_paused {
                                    menu.menu_with_icon(
                                        "Resume",
                                        IconName::Play,
                                        Box::new(ToggleReadAloud {
                                            text: self.message_text.clone(),
                                            message_id: self.message_id.clone(),
                                        }),
                                    )
                                } else {
                                    menu.menu_with_icon(
                                        "Pause",
                                        IconName::Pause,
                                        Box::new(ToggleReadAloud {
                                            text: self.message_text.clone(),
                                            message_id: self.message_id.clone(),
                                        }),
                                    )
                                }
                            } else {
                                menu.menu_with_icon(
                                    "Read aloud",
                                    IconName::ReadAloud,
                                    Box::new(ToggleReadAloud {
                                        text: self.message_text.clone(),
                                        message_id: self.message_id.clone(),
                                    }),
                                )
                            }
                        })
                        .separator()
                        .menu_with_icon(
                            "Report message",
                            IconName::Report,
                            Box::new(ReportMessage),
                        )
                    }),
            )
    }
}
