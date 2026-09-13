use crate::chrome::{
    persona_shape_path, resolve_persona_color, resolve_persona_shape, AVATAR_PX,
};
use gpui_kit::component::Icon;
use gpui_kit::*;

const HALO_PAD: f32 = 8.0;

#[derive(IntoElement)]
pub struct PersonaMark {
    agent_id: SharedString,
    shape: Option<SharedString>,
    color: Option<SharedString>,
    size: Pixels,
    dark: bool,
    lit: bool,
    group: Option<SharedString>,
}

impl PersonaMark {
    pub fn new(agent_id: impl Into<SharedString>) -> Self {
        Self {
            agent_id: agent_id.into(),
            shape: None,
            color: None,
            size: px(AVATAR_PX),
            dark: true,
            lit: false,
            group: None,
        }
    }

    pub fn shape(mut self, shape: Option<impl Into<SharedString>>) -> Self {
        self.shape = shape.map(Into::into);
        self
    }

    pub fn color(mut self, color: Option<impl Into<SharedString>>) -> Self {
        self.color = color.map(Into::into);
        self
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into();
        self
    }

    pub fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    pub fn lit(mut self, lit: bool) -> Self {
        self.lit = lit;
        self
    }

    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }
}

impl RenderOnce for PersonaMark {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let shape = resolve_persona_shape(
            self.agent_id.as_ref(),
            self.shape.as_ref().map(|s| s.as_ref()),
        );
        let color = resolve_persona_color(
            self.agent_id.as_ref(),
            self.color.as_ref().map(|s| s.as_ref()),
        );
        let fill = if self.dark { color.dark } else { color.light };
        let path = persona_shape_path(shape);
        let halo = px(f32::from(self.size) + HALO_PAD);
        let group = self
            .group
            .unwrap_or_else(|| SharedString::from(format!("persona-{}", self.agent_id)));
        let halo_fill: Hsla = rgb(fill).opacity(0.32).into();
        div()
            .relative()
            .flex_shrink_0()
            .size(halo)
            .flex()
            .items_center()
            .justify_center()
            .group(group.clone())
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(if self.lit { 1. } else { 0. })
                    .group_hover(group, |s| s.opacity(1.))
                    .child(
                        Icon::default()
                            .path(path)
                            .size(halo)
                            .text_color(halo_fill),
                    ),
            )
            .child(
                Icon::default()
                    .path(path)
                    .size(self.size)
                    .text_color(rgb(fill)),
            )
    }
}
