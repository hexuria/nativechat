use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, h_flex, tooltip::Tooltip};

#[derive(IntoElement)]
pub struct MessageActions {
    _message_id: String,
}

impl MessageActions {
    pub fn new(message_id: impl Into<String>) -> Self {
        Self {
            _message_id: message_id.into(),
        }
    }

    fn action_button(
        &self,
        id: impl Into<ElementId>,
        icon: IconName,
        tooltip_text: &'static str,
        cx: &mut App,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;

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
            .child(
                Icon::new(icon)
                    .size(px(16.0))
                    .text_color(secondary_foreground),
            )
    }
}

impl RenderOnce for MessageActions {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_1()
            .items_center()
            .child(self.action_button("copy", IconName::Copy, "Copy", cx))
            .child(self.action_button("like", IconName::ThumbsUp, "Good response", cx))
            .child(self.action_button("dislike", IconName::ThumbsDown, "Bad response", cx))
            .child(self.action_button("share", IconName::ArrowUp, "Share", cx))
            .child(self.action_button("regenerate", IconName::Settings, "Try again", cx))
            .child(self.action_button("more", IconName::Menu, "More actions", cx))
    }
}
