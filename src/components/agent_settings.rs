use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct AgentSettings {
    state: Entity<AppState>,
    label_input: Entity<InputState>,
    role_input: Entity<InputState>,
    model_input: Entity<InputState>,
}

impl AgentSettings {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let label_input = cx.new(|cx| InputState::new(window, cx).placeholder("Research, marketing, admin"));
        let role_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("What this agent is for")
        });
        let model_input = cx.new(|cx| InputState::new(window, cx).placeholder("xai/grok-4.6@sub"));
        cx.observe(&state, |this, _, cx| cx.notify()).detach();
        Self {
            state,
            label_input,
            role_input,
            model_input,
        }
    }

    fn save(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let role = self.role_input.read(cx).value().to_string();
        let model = self.model_input.read(cx).value().to_string();
        self.state.update(cx, |state, cx| {
            let model = if model.trim().is_empty() {
                None
            } else {
                Some(model)
            };
            let role = Some(role);
            state.patch_active_coworker(model, role, cx);
        });
    }
}

impl Render for AgentSettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (name, model, role, catalogue, note, error) = {
            let state = self.state.read(cx);
            let coworker = state
                .active_coworker_id
                .as_ref()
                .and_then(|id| state.coworkers.iter().find(|c| &c.id == id));
            (
                coworker.map(|c| c.name.clone()).unwrap_or_else(|| "Agent".into()),
                coworker.map(|c| c.model.clone()).unwrap_or_default(),
                coworker
                    .and_then(|c| c.role.clone())
                    .unwrap_or_default(),
                state.model_catalogue.models.clone(),
                state.model_catalogue.note.clone(),
                state.auth_error.clone(),
            )
        };

        let glyph = name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into());

        v_flex()
            .id("agent-settings")
            .h_full()
            .w(px(320.))
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(rgb(0x1a1a1a))
            .text_color(white())
            .p_4()
            .gap_3()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .size_16()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(0x3ecfcf))
                            .text_color(white())
                            .text_lg()
                            .child(glyph),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child("Name"),
            )
            .child(div().id("agent-settings-name").text_sm().child(name))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child("Label (optional)"),
            )
            .child(
                div()
                    .id("agent-label")
                    .w_full()
                    .child(Input::new(&self.label_input)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child("Description"),
            )
            .child(
                div()
                    .id("agent-role")
                    .w_full()
                    .child(Input::new(&self.role_input)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0x888888))
                    .child(format!("Model · {model}")),
            )
            .child(
                div()
                    .id("agent-model")
                    .w_full()
                    .child(Input::new(&self.model_input)),
            )
            .when(!catalogue.is_empty(), |this| {
                this.child(
                    v_flex().gap_1().children(catalogue.into_iter().take(12).map(|m| {
                        let id = m.id.clone();
                        let app = self.state.clone();
                        div()
                            .id(SharedString::from(format!("model-{id}")))
                            .text_xs()
                            .cursor_pointer()
                            .text_color(theme.muted_foreground)
                            .child(id.clone())
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                let id = id.clone();
                                app.update(cx, |state, cx| {
                                    state.patch_active_coworker(Some(id), None, cx);
                                });
                            })
                    })),
                )
            })
            .when_some(note, |this, note| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(note),
                )
            })
            .when_some(error, |this, message| {
                this.child(
                    div()
                        .id("agent-settings-error")
                        .text_sm()
                        .text_color(theme.danger)
                        .child(message),
                )
            })
            .child(
                div().id("agent-save").child(
                    Button::new("agent-save-btn")
                        .label("Save")
                        .primary()
                        .on_click(cx.listener(Self::save)),
                ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(role),
            )
    }
}
