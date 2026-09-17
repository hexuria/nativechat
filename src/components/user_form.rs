//! User-form transcript chrome. Separate from generative [`crate::opengrok::FormSpec`]
//! (choice chips → `send_message`), and from future secret-request / Computer
//! handoff cards.
//!
//! Continue is gated in PR1: it must not claim a fill, and it must not POST a
//! verb the server does not have. Open the screen / Dismiss are painted and
//! similarly not wired. Settled pills come from parsed `formResolution`.

use crate::components::fields::field_input;
use crate::opengrok::{
    FormResolution, USER_FORM_SERVER_FILL_AVAILABLE, UserFormField, UserFormFieldKind,
    UserFormSpec, UserFormValues, continue_enabled,
};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputContentType, InputState, Textarea, TextareaState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::HashMap;

pub type UserFormInputMap = HashMap<String, Entity<InputState>>;
pub type UserFormTextareaMap = HashMap<String, Entity<TextareaState>>;

pub fn field_key(entry_id: &str, field_id: &str) -> String {
    format!("{entry_id}\u{1f}{field_id}")
}

pub fn render_user_form(
    spec: &UserFormSpec,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    if let Some(resolution) = spec.effective_resolution() {
        return render_settled(spec, resolution, cx);
    }
    render_idle(spec, inputs, textareas, values, app, cx)
}

fn render_idle(
    spec: &UserFormSpec,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let can_continue = continue_enabled(spec, values, USER_FORM_SERVER_FILL_AVAILABLE);
    let mut body = v_flex()
        .w_full()
        .gap(px(10.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .occlude();
    let title = if spec.title.is_empty() {
        "Form".to_string()
    } else {
        spec.title.clone()
    };
    body = body.child(
        div()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title),
    );
    if let Some(instruction) = &spec.instruction {
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(instruction.clone()),
        );
    }
    if let Some(host) = spec.live_host.as_deref().or(spec.domain.as_deref()) {
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(host.to_string()),
        );
    }
    for field in &spec.fields {
        body = body.child(render_field(
            spec,
            field,
            inputs,
            textareas,
            values,
            app.clone(),
            cx,
        ));
    }
    let entry = spec.entry_id.clone();
    body.child(
        h_flex()
            .w_full()
            .justify_end()
            .gap(px(8.))
            .flex_wrap()
            .child(gated_button(
                format!("user-form-continue-{entry}"),
                "Continue",
                ButtonKind::Primary,
                !can_continue,
            ))
            .child(gated_button(
                format!("user-form-screen-{entry}"),
                "Open the screen",
                ButtonKind::Secondary,
                true,
            ))
            .child(gated_button(
                format!("user-form-dismiss-{entry}"),
                "Dismiss",
                ButtonKind::Ghost,
                true,
            )),
    )
    .into_any_element()
}

fn render_settled(spec: &UserFormSpec, resolution: FormResolution, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let pill_fill = match resolution {
        FormResolution::Submitted => theme.green.opacity(0.18),
        FormResolution::FillFailed => theme.danger.opacity(0.16),
        FormResolution::Escalated | FormResolution::Sending | FormResolution::Dismissed => {
            theme.secondary
        }
    };
    let pill_text = match resolution {
        FormResolution::Submitted => theme.green,
        FormResolution::FillFailed => theme.danger,
        _ => theme.muted_foreground,
    };
    v_flex()
        .w_full()
        .gap(px(8.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if spec.title.is_empty() {
                            "Form".to_string()
                        } else {
                            spec.title.clone()
                        }),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(4.))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(999.))
                        .bg(pill_fill)
                        .text_color(pill_text)
                        .text_xs()
                        .when(resolution == FormResolution::Submitted, |this| {
                            this.child(Icon::new(IconName::Check).size(px(12.)))
                        })
                        .child(resolution.pill()),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(resolution.body()),
        )
        .into_any_element()
}

fn render_field(
    spec: &UserFormSpec,
    field: &UserFormField,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let label = if field.required {
        format!("{} *", field.label)
    } else {
        field.label.clone()
    };
    let key = field_key(&spec.entry_id, &field.id);
    let control = match field.kind {
        UserFormFieldKind::Checkbox => render_checkbox(spec, field, values, app, cx),
        UserFormFieldKind::Select => render_select(spec, field, values, app, cx),
        UserFormFieldKind::Textarea => {
            if let Some(state) = textareas.get(&key) {
                Textarea::new(state)
                    .w_full()
                    .h(px(72.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border)
                    .into_any_element()
            } else {
                placeholder_box(field, cx)
            }
        }
        _ => {
            if let Some(state) = inputs.get(&key) {
                let mut input = field_input(state);
                if field.masked() {
                    let content = if field.kind == UserFormFieldKind::Otp {
                        InputContentType::OneTimeCode
                    } else {
                        InputContentType::Password
                    };
                    input = input.mask_toggle().content_type(content);
                } else {
                    input = match field.kind {
                        UserFormFieldKind::Email => {
                            input.content_type(InputContentType::EmailAddress)
                        }
                        UserFormFieldKind::Tel => {
                            input.content_type(InputContentType::TelephoneNumber)
                        }
                        UserFormFieldKind::Date => input.content_type(InputContentType::DateTime),
                        _ => input,
                    };
                }
                input.into_any_element()
            } else {
                placeholder_box(field, cx)
            }
        }
    };
    let show_label = field.kind != UserFormFieldKind::Checkbox;
    v_flex()
        .gap(px(6.))
        .when(show_label, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(label),
            )
        })
        .child(control)
        .into_any_element()
}

fn render_checkbox(
    spec: &UserFormSpec,
    field: &UserFormField,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let on = values.by_id.get(&field.id).map(String::as_str) == Some("true");
    let entry_id = spec.entry_id.clone();
    let field_id = field.id.clone();
    let next = if on { "false" } else { "true" };
    h_flex()
        .id(ElementId::Name(
            format!("user-form-check-{}-{field_id}", spec.entry_id).into(),
        ))
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .child(
            div()
                .size(px(16.))
                .rounded(px(4.))
                .border_1()
                .border_color(if on { theme.primary } else { theme.border })
                .bg(if on { theme.primary } else { theme.background })
                .flex()
                .items_center()
                .justify_center()
                .when(on, |this| {
                    this.child(
                        Icon::new(IconName::Check)
                            .size(px(12.))
                            .text_color(theme.primary_foreground),
                    )
                }),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.foreground)
                .child(if field.required {
                    format!("{} *", field.label)
                } else {
                    field.label.clone()
                }),
        )
        .when_some(app, |this, app| {
            this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                app.update(cx, |state, cx| {
                    state.pick_user_form_option(
                        entry_id.clone(),
                        field_id.clone(),
                        next.to_string(),
                        cx,
                    );
                });
            })
        })
        .into_any_element()
}

fn render_select(
    spec: &UserFormSpec,
    field: &UserFormField,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let selected = values.by_id.get(&field.id).cloned();
    let mut chips = h_flex().gap(px(6.)).flex_wrap();
    for option in &field.options {
        let on = selected.as_deref() == Some(option.value.as_str());
        let entry_id = spec.entry_id.clone();
        let field_id = field.id.clone();
        let value = option.value.clone();
        let app = app.clone();
        chips = chips.child(
            div()
                .id(ElementId::Name(
                    format!("user-form-opt-{}-{field_id}-{value}", spec.entry_id).into(),
                ))
                .px(px(10.))
                .py(px(4.))
                .rounded(px(999.))
                .border_1()
                .border_color(if on { theme.primary } else { theme.border })
                .bg(if on { theme.primary } else { theme.background })
                .text_color(if on {
                    theme.primary_foreground
                } else {
                    theme.foreground
                })
                .text_xs()
                .cursor_pointer()
                .child(option.label.clone())
                .when_some(app, |this, app| {
                    this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        app.update(cx, |state, cx| {
                            state.pick_user_form_option(
                                entry_id.clone(),
                                field_id.clone(),
                                value.clone(),
                                cx,
                            );
                        });
                    })
                }),
        );
    }
    chips.into_any_element()
}

fn placeholder_box(field: &UserFormField, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let hint = field
        .placeholder
        .clone()
        .unwrap_or_else(|| field.label.clone());
    div()
        .w_full()
        .px(px(10.))
        .py(px(8.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(if field.masked() {
            "••••••••".to_string()
        } else {
            hint
        })
        .into_any_element()
}

enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
}

fn gated_button(id: String, label: &'static str, kind: ButtonKind, disabled: bool) -> AnyElement {
    let why = "Coming from the server";
    let button = Button::new(id).label(label).disabled(disabled).small();
    let button = match kind {
        ButtonKind::Primary => button.primary(),
        ButtonKind::Secondary => button.outline(),
        ButtonKind::Ghost => button.ghost(),
    };
    div()
        .tooltip(move |window, cx| Tooltip::new(why).build(window, cx))
        .child(button)
        .into_any_element()
}
