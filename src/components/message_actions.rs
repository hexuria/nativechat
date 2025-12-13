use crate::{
    actions::{BranchInNewChat, ReportMessage, ToggleReadAloud},
    state::AppState,
};
use gpui::{prelude::FluentBuilder, *};
use std::rc::Rc;
use std::time::Duration;
use ui::{
    ActiveTheme, Icon, IconName, Sizable, Size,
    button::{Button, ButtonVariants},
    h_flex,
    tooltip::Tooltip,
};

#[derive(IntoElement)]
pub struct MessageActions {
    message_id: String,
    message_text: String,
    on_copy: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_read_aloud: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    can_read_aloud: bool,
    is_speaking: bool,
    is_paused: bool,
    is_loading: bool,
    is_cached: bool,
    active_tts_source: Option<crate::actions::TtsSource>,
    #[allow(dead_code)]
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
            on_read_aloud: None,
            can_read_aloud: false,
            is_speaking: false,
            is_paused: false,
            is_loading: false,
            is_cached: false,
            active_tts_source: None,
            state: None,
        }
    }

    /// Set the message text to copy when the copy button is clicked
    pub fn message_text(mut self, text: impl Into<String>) -> Self {
        self.message_text = text.into();
        self
    }

    pub fn on_copy(mut self, on_copy: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_copy = Some(Rc::new(on_copy));
        self
    }

    pub fn on_read_aloud(
        mut self,
        on_read_aloud: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_read_aloud = Some(Rc::new(on_read_aloud));
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

    pub fn is_cached(mut self, is_cached: bool) -> Self {
        self.is_cached = is_cached;
        self
    }

    pub fn active_tts_source(mut self, source: Option<crate::actions::TtsSource>) -> Self {
        self.active_tts_source = source;
        self
    }

    fn action_button(
        &self,
        id: &str,
        icon: IconSource,
        tooltip_text: &str,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        cx: &mut App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let secondary = theme.secondary;
        // matching copy_button icon color logic (using secondary_foreground as default)
        let icon_color = theme.secondary_foreground;

        let icon: Icon = match icon {
            IconSource::Name(name) => name.into(),
            IconSource::Path(path) => Icon::default().path(path),
        };

        let id = gpui::SharedString::from(id.to_string());
        let tooltip_text = tooltip_text.to_string();

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
            .tooltip(move |w, cx| Tooltip::new(tooltip_text.clone()).build(w, cx))
            .child(icon.size(px(18.0)).text_color(icon_color))
            .on_click(on_click)
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
        let (native_icon, native_tooltip) =
            if self.active_tts_source == Some(crate::actions::TtsSource::Native) {
                if self.is_speaking {
                    if self.is_paused {
                        (IconName::Play, "Resume (Native)")
                    } else {
                        (IconName::Pause, "Pause (Native)")
                    }
                } else if self.is_loading {
                    (IconName::Loader, "Loading...")
                } else {
                    (IconName::ReadAloud, "Read aloud (Native)")
                }
            } else {
                (IconName::ReadAloud, "Read aloud (Native)")
            };

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
                |_, _, _| {},
                cx,
            ))
            .child(self.action_button(
                "dislike",
                IconSource::Path("icons/thumbs_down.svg"),
                "Bad response",
                |_, _, _| {},
                cx,
            ))
            .child(self.action_button(
                "share",
                IconSource::Path("icons/share.svg"),
                "Share",
                |_, _, _| {},
                cx,
            ))
            .child(self.action_button(
                "regenerate",
                IconSource::Path("icons/reset.svg"),
                "Try again",
                |_, _, _| {},
                cx,
            ))
            .child(self.action_button(
                &format!("native-tts-{}", self.message_id),
                IconSource::Name(native_icon),
                native_tooltip,
                {
                    let on_read_aloud = self.on_read_aloud.clone();
                    let message_id = self.message_id.clone();
                    move |_, window, cx| {
                        println!(
                            "[MessageActions] Native TTS Button Clicked for message: {}",
                            message_id
                        );
                        if let Some(callback) = on_read_aloud.as_ref() {
                            callback(window, cx);
                        }
                    }
                },
                cx,
            ))
            .child(
                Button::new(ElementId::Name(format!("more-{}", self.message_id).into()))
                    .icon(IconName::Ellipsis)
                    .ghost()
                    .with_size(Size::Medium)
                    .compact()
                    .rounded(ui::button::ButtonRounded::Size(px(6.0)))
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
                                    IconName::Loader,
                                    Box::new(ToggleReadAloud {
                                        text: self.message_text.clone(),
                                        message_id: self.message_id.clone(),
                                        mode: crate::actions::TtsSource::AI,
                                    }),
                                )
                            } else if self.is_speaking {
                                if self.is_paused {
                                    menu.menu_with_icon(
                                        "Resume (AI)",
                                        IconName::Play,
                                        Box::new(ToggleReadAloud {
                                            text: self.message_text.clone(),
                                            message_id: self.message_id.clone(),
                                            mode: crate::actions::TtsSource::AI,
                                        }),
                                    )
                                } else {
                                    menu.menu_with_icon(
                                        "Pause (AI)",
                                        IconName::Pause,
                                        Box::new(ToggleReadAloud {
                                            text: self.message_text.clone(),
                                            message_id: self.message_id.clone(),
                                            mode: crate::actions::TtsSource::AI,
                                        }),
                                    )
                                }
                            } else {
                                menu.menu_with_icon(
                                    "Read with AI",
                                    IconName::Bot,
                                    Box::new(ToggleReadAloud {
                                        text: self.message_text.clone(),
                                        message_id: self.message_id.clone(),
                                        mode: crate::actions::TtsSource::AI,
                                    }),
                                )
                            }
                        })
                        .when(self.is_cached, |menu| {
                            menu.menu_with_icon(
                                "Regenerate Audio",
                                IconName::Replace,
                                Box::new(crate::actions::RegenerateAudio {
                                    text: self.message_text.clone(),
                                    message_id: self.message_id.clone(),
                                }),
                            )
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
