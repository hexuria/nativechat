use crate::{
    actions::{BranchInNewChat, ReportMessage, ToggleReadAloud},
    state::AppState,
};
use crate::icons::NativeIcon;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, Size,
    button::{Button, ButtonVariants},
    h_flex,
    menu::DropdownMenu,
    tooltip::Tooltip,
};
use gpui_kit::{prelude::FluentBuilder, prelude::*, *};
use std::rc::Rc;
use std::time::Duration;

#[derive(IntoElement)]
pub struct MessageActions {
    message_id: String,
    message_text: String,
    on_copy: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_read_aloud: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    can_read_aloud: bool,
    // Native State
    is_native_speaking: bool,
    is_native_paused: bool,
    is_native_loading: bool,
    // AI State
    is_ai_speaking: bool,
    is_ai_paused: bool,
    is_ai_loading: bool,

    is_cached: bool,
    #[allow(dead_code)]
    state: Option<WeakEntity<AppState>>,
}

enum IconSource {
    Icon(Icon),
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
            is_native_speaking: false,
            is_native_paused: false,
            is_native_loading: false,
            is_ai_speaking: false,
            is_ai_paused: false,
            is_ai_loading: false,
            is_cached: false,
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

    pub fn is_native_speaking(mut self, is: bool) -> Self {
        self.is_native_speaking = is;
        self
    }
    pub fn is_native_paused(mut self, is: bool) -> Self {
        self.is_native_paused = is;
        self
    }
    pub fn is_native_loading(mut self, is: bool) -> Self {
        self.is_native_loading = is;
        self
    }

    pub fn is_ai_speaking(mut self, is: bool) -> Self {
        self.is_ai_speaking = is;
        self
    }
    pub fn is_ai_paused(mut self, is: bool) -> Self {
        self.is_ai_paused = is;
        self
    }
    pub fn is_ai_loading(mut self, is: bool) -> Self {
        self.is_ai_loading = is;
        self
    }

    pub fn is_cached(mut self, is_cached: bool) -> Self {
        self.is_cached = is_cached;
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
            IconSource::Icon(icon) => icon,
            IconSource::Path(path) => Icon::default().path(path),
        };

        let id = gpui_kit::SharedString::from(id.to_string());
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
        let (native_icon, native_tooltip) = if self.is_native_speaking {
            if self.is_native_paused {
                (Icon::new(IconName::Play), "Resume (Native)")
            } else {
                (Icon::new(IconName::Pause), "Pause (Native)")
            }
        } else if self.is_native_loading {
            (Icon::new(IconName::Loader), "Loading...")
        } else {
            (Icon::new(NativeIcon::ReadAloud), "Read aloud (Native)")
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
                IconSource::Icon(native_icon),
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
                    .rounded(px(6.0))
                    .tooltip("More actions")
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                        menu.menu_with_icon(
                            "Branch in new chat",
                            NativeIcon::Branch,
                            Box::new(BranchInNewChat),
                        )
                        .when(self.can_read_aloud, |menu| {
                            if self.is_ai_loading {
                                menu.menu_with_icon(
                                    "Loading...",
                                    IconName::Loader,
                                    Box::new(ToggleReadAloud {
                                        text: self.message_text.clone(),
                                        message_id: self.message_id.clone(),
                                        mode: crate::actions::TtsSource::AI,
                                    }),
                                )
                            } else if self.is_ai_speaking {
                                if self.is_ai_paused {
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
                            NativeIcon::Report,
                            Box::new(ReportMessage),
                        )
                    }),
            )
    }
}
