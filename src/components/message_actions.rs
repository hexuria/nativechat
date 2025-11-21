use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName, h_flex, tooltip::Tooltip};

#[derive(IntoElement)]
pub struct MessageActions {
    _message_id: String,
}

enum IconSource {
    Name(IconName),
    Path(&'static str),
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
}

impl RenderOnce for MessageActions {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_1()
            .items_center()
            .child(self.action_button("copy", IconSource::Name(IconName::Copy), "Copy", cx))
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
            .child(self.action_button("more", IconSource::Name(IconName::Menu), "More actions", cx))
    }
}
