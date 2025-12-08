use crate::{ActiveTheme, Icon, IconName};
use gpui::{div, prelude::*, App, ElementId, IntoElement, MouseButton, RenderOnce, Styled, Window};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Unselected,
    Selected,
    Indeterminate,
}

impl Selection {
    pub fn inverse(&self) -> Self {
        match self {
            Self::Unselected => Self::Selected,
            Self::Selected => Self::Unselected,
            Self::Indeterminate => Self::Unselected,
        }
    }
}

#[derive(Clone, IntoElement)]
pub struct Checkbox {
    id: ElementId,
    checked: Selection,
    on_click: Option<std::rc::Rc<dyn Fn(&Selection, &mut Window, &mut App) + 'static>>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            checked: Selection::Unselected,
            on_click: None,
        }
    }

    pub fn checked(mut self, checked: Selection) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&Selection, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let selection = self.checked;
        let on_click = self.on_click;
        let theme = cx.theme();

        div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .size_5()
            .rounded_sm()
            .border_1()
            .border_color(theme.border)
            .bg(if selection == Selection::Selected {
                theme.accent
            } else {
                theme.background
            })
            .hover(|s| s.border_color(theme.accent))
            .cursor_pointer()
            .child(if selection == Selection::Selected {
                div().child(
                    Icon::new(IconName::Check)
                        .size_3()
                        .text_color(theme.accent_foreground),
                )
            } else {
                div()
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                if let Some(handler) = &on_click {
                    handler(&selection.inverse(), window, cx);
                }
            })
    }
}
