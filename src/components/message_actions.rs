use crate::icons::NativeIcon;
use crate::state::{AppState, ReplyTo};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, Size,
    button::{Button, ButtonVariants},
    h_flex,
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::FluentBuilder, prelude::*, *};
use std::rc::Rc;

/// Every control in the toolbar is this square, whatever it holds.
pub const CONTROL_PX: f32 = 28.0;
const CONTROL_GAP: f32 = 2.0;
/// The toolbar's own width: three controls and the gaps between them, nothing more.
pub const TOOLBAR_W: f32 = 3.0 * CONTROL_PX + 2.0 * CONTROL_GAP;

#[derive(Clone, Copy)]
struct BtnBounds {
    bounds: Bounds<Pixels>,
}

#[derive(IntoElement)]
pub struct MessageToolbar {
    message_id: String,
    source_id: String,
    message_text: String,
    preview: String,
    is_me: bool,
    app: Entity<AppState>,
    on_read_aloud: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_reply: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    on_menu_open: Option<Rc<dyn Fn(bool, &mut App)>>,
    /// This message is the one being read aloud (and whether it is paused), so the menu says
    /// what a click will do instead of "Read aloud" again.
    reading: bool,
    paused: bool,
}

impl MessageToolbar {
    pub fn new(
        app: Entity<AppState>,
        message_id: impl Into<String>,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            message_id: message_id.into(),
            source_id: source_id.into(),
            message_text: String::new(),
            preview: String::new(),
            is_me: false,
            app,
            on_read_aloud: None,
            on_reply: None,
            on_menu_open: None,
            reading: false,
            paused: false,
        }
    }

    pub fn reading(mut self, reading: bool, paused: bool) -> Self {
        self.reading = reading;
        self.paused = paused;
        self
    }

    pub fn message_text(mut self, text: impl Into<String>) -> Self {
        self.message_text = text.into();
        self
    }

    pub fn preview(mut self, preview: impl Into<String>) -> Self {
        self.preview = preview.into();
        self
    }

    pub fn is_me(mut self, is_me: bool) -> Self {
        self.is_me = is_me;
        self
    }

    pub fn on_read_aloud(
        mut self,
        on_read_aloud: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_read_aloud = Some(Rc::new(on_read_aloud));
        self
    }

    pub fn on_reply(mut self, on_reply: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_reply = Some(Rc::new(on_reply));
        self
    }

    pub fn on_menu_open(mut self, on_menu_open: impl Fn(bool, &mut App) + 'static) -> Self {
        self.on_menu_open = Some(Rc::new(on_menu_open));
        self
    }

    /// One control of the toolbar. All three come from here so they cannot drift: the same
    /// square, radius, icon size and hover, whatever the control does.
    fn control(id: SharedString, icon: impl Into<Icon>, tooltip: &'static str, cx: &App) -> Button {
        let color = cx.theme().muted_foreground;
        Button::new(ElementId::Name(id))
            .icon(icon.into().text_color(color))
            .ghost()
            // Medium is the 16px icon; the square itself is set below, over the preset.
            .with_size(Size::Medium)
            .size(px(CONTROL_PX))
            .p_0()
            .rounded(px(6.))
            .tooltip(tooltip)
    }
}

impl RenderOnce for MessageToolbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let message_id = self.message_id.clone();
        let source_id = self.source_id.clone();
        let app = self.app.clone();
        let bounds_state = window.use_keyed_state(
            ElementId::Name(format!("emoji-bounds-{message_id}").into()),
            cx,
            |_, _| BtnBounds {
                bounds: Bounds::default(),
            },
        );

        h_flex()
            .id(SharedString::from(format!("toolbar-{message_id}")))
            .gap(px(CONTROL_GAP))
            .items_center()
            .flex_shrink_0()
            .child({
                let bounds_state = bounds_state.clone();
                let app = app.clone();
                let source_id = source_id.clone();
                Self::control(
                    SharedString::from(format!("emoji-{message_id}")),
                    NativeIcon::Smile,
                    "Add reaction",
                    cx,
                )
                .on_prepaint({
                    let bounds_state = bounds_state.clone();
                    move |bounds, _, cx| {
                        bounds_state.update(cx, |state, _| {
                            state.bounds = bounds;
                        });
                    }
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let bounds = bounds_state.read(cx).bounds;
                    app.update(cx, |state, cx| {
                        state.open_emoji_picker(source_id.clone(), bounds, cx);
                    });
                })
            })
            .child(
                Self::control(
                    SharedString::from(format!("reply-{message_id}")),
                    NativeIcon::Reply,
                    "Reply",
                    cx,
                )
                .on_click({
                    let app = app.clone();
                    let source_id = source_id.clone();
                    let preview = self.preview.clone();
                    let is_me = self.is_me;
                    let on_reply = self.on_reply.clone();
                    move |_, window, cx| {
                        cx.stop_propagation();
                        app.update(cx, |state, cx| {
                            state.set_reply_to(
                                ReplyTo {
                                    message_id: source_id.clone(),
                                    preview: preview.clone(),
                                    is_me,
                                },
                                cx,
                            );
                        });
                        if let Some(cb) = on_reply.as_ref() {
                            cb(window, cx);
                        }
                    }
                }),
            )
            .child(
                Self::control(
                    SharedString::from(format!("more-{message_id}")),
                    IconName::Ellipsis,
                    "More actions",
                    cx,
                )
                .dropdown_menu_with_anchor(Anchor::BottomLeft, {
                    let app = app.clone();
                    let source_id = source_id.clone();
                    let text = self.message_text.clone();
                    let on_read_aloud = self.on_read_aloud.clone();
                    let (reading, paused) = (self.reading, self.paused);
                    let (read_label, read_icon) = match (reading, paused) {
                        (true, true) => ("Resume reading", NativeIcon::Play),
                        (true, false) => ("Pause reading", NativeIcon::Pause),
                        _ => ("Read aloud", NativeIcon::ReadAloud),
                    };
                    move |menu, _, _| {
                        menu.item(
                            PopupMenuItem::new("Delete")
                                .icon(Icon::new(NativeIcon::Trash))
                                .on_click({
                                    let app = app.clone();
                                    let source_id = source_id.clone();
                                    move |_, _, cx| {
                                        cx.stop_propagation();
                                        app.update(cx, |state, cx| {
                                            state.delete_message(&source_id, cx);
                                        });
                                    }
                                }),
                        )
                        .item(
                            PopupMenuItem::new(read_label)
                                .icon(Icon::new(read_icon))
                                .on_click({
                                    let on_read_aloud = on_read_aloud.clone();
                                    move |_, window, cx| {
                                        cx.stop_propagation();
                                        if let Some(cb) = on_read_aloud.as_ref() {
                                            cb(window, cx);
                                        }
                                    }
                                }),
                        )
                        .when(reading, |menu| {
                            menu.item(
                                PopupMenuItem::new("Stop reading")
                                    .icon(Icon::new(NativeIcon::ReadAloud))
                                    .on_click({
                                        let app = app.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            app.update(cx, |state, cx| state.stop_read_aloud(cx));
                                        }
                                    }),
                            )
                        })
                        .item(
                            PopupMenuItem::new("Copy")
                                .icon(Icon::new(IconName::Copy))
                                .on_click({
                                    let text = text.clone();
                                    move |_, _, cx| {
                                        cx.stop_propagation();
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            text.clone(),
                                        ));
                                    }
                                }),
                        )
                    }
                })
                .on_open_change({
                    let on_menu_open = self.on_menu_open.clone();
                    move |open, _, cx| {
                        if let Some(cb) = on_menu_open.as_ref() {
                            cb(*open, cx);
                        }
                    }
                }),
            )
    }
}
