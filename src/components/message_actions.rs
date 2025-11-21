use gpui::*;
use gpui_component::{
    IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
};

#[derive(IntoElement)]
pub struct MessageActions {
    message_id: String,
}

impl MessageActions {
    pub fn new(message_id: impl Into<String>) -> Self {
        Self {
            message_id: message_id.into(),
        }
    }
}

impl RenderOnce for MessageActions {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        // Use hash of message_id as unique number for ElementId
        let id_hash = self.message_id.len(); // Simple hash for now

        h_flex()
            .gap_1()
            .items_center()
            .child(
                Button::new(("copy", id_hash))
                    .icon(IconName::Copy)
                    .ghost()
                    .xsmall()
                    .tooltip("Copy"),
            )
            .child(
                Button::new(("like", id_hash))
                    .icon(IconName::ThumbsUp)
                    .ghost()
                    .xsmall()
                    .tooltip("Good response"),
            )
            .child(
                Button::new(("dislike", id_hash))
                    .icon(IconName::ThumbsDown)
                    .ghost()
                    .xsmall()
                    .tooltip("Bad response"),
            )
            .child(
                Button::new(("share", id_hash))
                    .icon(IconName::ArrowUp)
                    .ghost()
                    .xsmall()
                    .tooltip("Share"),
            )
            .child(
                Button::new(("regenerate", id_hash))
                    .icon(IconName::Settings)
                    .ghost()
                    .xsmall()
                    .tooltip("Try again"),
            )
            .child(
                Button::new(("more", id_hash))
                    .icon(IconName::Menu)
                    .ghost()
                    .xsmall()
                    .tooltip("More actions"),
            )
    }
}
