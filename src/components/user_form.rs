//! User-form transcript chrome. Separate from generative [`crate::opengrok::FormSpec`]
//! (choice chips → `send_message`).
//!
//! Continue → Submitting (fields hidden, spinner) → Submitted collapsed.
//! Dismiss → Dismissed. Open the screen → form stays in document order and a
//! Computer sibling paints Action needed / Take over / I'm done / Skip.
//! I'm done → form Dismissed + Computer Done. Skip → form Skipped + Computer
//! Skipped. Never morph the form into Computer, then resurrect **On the
//! computer** with the Computer card gone. fill_failed recovery stays on
//! the collapsed card: Try again / I'll do it on the computer / Stop for now.
//! Secrets collected here go only in the REST body, never `send_message` /
//! AG-UI `content` / sqlite.

use crate::components::fields::field_input;
use crate::opengrok::{
    BoxHandoffResolution, ComputerHandoffStatus, FormResolution, USER_FORM_SERVER_FILL_AVAILABLE,
    UserFormDismissMode, UserFormField, UserFormFieldKind, UserFormSpec, UserFormValues,
    computer_handoff_card_id, computer_handoff_done_id, computer_handoff_skip_id,
    computer_handoff_takeover_id, continue_enabled, user_form_card_id, user_form_continue_id,
    user_form_dismiss_id, user_form_field_id, user_form_pill_id, user_form_screen_id,
};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputContentType, InputState, Textarea, TextareaState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::HashMap;

type Theme = gpui_kit::component::Theme;

pub type UserFormInputMap = HashMap<String, Entity<InputState>>;
pub type UserFormTextareaMap = HashMap<String, Entity<TextareaState>>;

pub fn field_key(card_key: &str, field_id: &str) -> String {
    format!("{card_key}\u{1f}{field_id}")
}

pub fn render_user_form(
    spec: &UserFormSpec,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let form = match spec.effective_resolution() {
        Some(FormResolution::Sending) => render_submitting(spec, cx),
        Some(FormResolution::Escalated) => render_settled(
            spec,
            FormResolution::Dismissed,
            inputs,
            textareas,
            values,
            app.clone(),
            cx,
        ),
        Some(resolution) => {
            render_settled(spec, resolution, inputs, textareas, values, app.clone(), cx)
        }
        None => render_idle(spec, inputs, textareas, values, app.clone(), cx),
    };
    let mut stack = v_flex().w_full().gap(px(10.));
    if spec.shows_form_chrome() {
        stack = stack.child(form);
    }
    if let Some(status) = spec.computer_handoff {
        stack = stack.child(render_computer_handoff(spec, status, app, cx));
    }
    stack.into_any_element()
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
    let server_fill = app
        .as_ref()
        .map(|entity| entity.read(cx).user_form_verbs_available)
        .unwrap_or(USER_FORM_SERVER_FILL_AVAILABLE);
    let can_post = spec.can_post(server_fill);
    let mut picks = values.clone();
    if let Some(app) = &app {
        if let Some(typed) = app.read(cx).user_form_typed.get(spec.card_key()) {
            for (id, value) in typed {
                picks
                    .by_id
                    .entry(id.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    // Live InputState / TextareaState / picks / agent-typed — not a masked stub.
    let live = collect_submit_values(spec, inputs, textareas, &picks, cx);
    let can_continue = continue_enabled(spec, &live, server_fill);
    let mut body = v_flex()
        .id(ElementId::Name(user_form_card_id(spec.card_key()).into()))
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
    let key = spec.card_key().to_string();
    body.child(
        h_flex()
            .w_full()
            .justify_end()
            .gap(px(8.))
            .flex_wrap()
            .child(action_button(
                user_form_continue_id(&key),
                "Continue",
                ButtonKind::Primary,
                !can_continue,
                !can_post,
                {
                    let spec = spec.clone();
                    let inputs = inputs.clone();
                    let textareas = textareas.clone();
                    let picks = picks.clone();
                    let app = app.clone();
                    let key = key.clone();
                    can_continue.then_some(move |cx: &mut App| {
                        let values = collect_submit_values(&spec, &inputs, &textareas, &picks, cx);
                        if let Some(app) = &app {
                            app.update(cx, |state, cx| {
                                state.submit_user_form(key.clone(), values, cx);
                            });
                        }
                    })
                },
            ))
            .child(action_button(
                user_form_screen_id(&key),
                "Open the screen",
                ButtonKind::Secondary,
                !can_post,
                !can_post,
                {
                    let app = app.clone();
                    let key = key.clone();
                    can_post.then_some(move |cx: &mut App| {
                        if let Some(app) = &app {
                            app.update(cx, |state, cx| {
                                state.dismiss_user_form(
                                    key.clone(),
                                    UserFormDismissMode::Escalated,
                                    cx,
                                );
                            });
                        }
                    })
                },
            ))
            .child(action_button(
                user_form_dismiss_id(&key),
                "Dismiss",
                ButtonKind::Ghost,
                !can_post,
                !can_post,
                {
                    let app = app.clone();
                    can_post.then_some(move |cx: &mut App| {
                        if let Some(app) = &app {
                            app.update(cx, |state, cx| {
                                state.dismiss_user_form(
                                    key.clone(),
                                    UserFormDismissMode::Dismissed,
                                    cx,
                                );
                            });
                        }
                    })
                },
            )),
    )
    .into_any_element()
}

fn render_computer_handoff(
    spec: &UserFormSpec,
    status: ComputerHandoffStatus,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    match status {
        ComputerHandoffStatus::ActionNeeded => render_live_computer_handoff(spec, app, cx),
        ComputerHandoffStatus::Done | ComputerHandoffStatus::Skipped => {
            render_settled_computer_handoff(spec, status, cx)
        }
    }
}

fn render_live_computer_handoff(
    spec: &UserFormSpec,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    // Live Action needed: Skip / I'm done stay clickable even if a sibling
    // 404 flipped the old global verbs lock. Do not POST until we have a
    // sibling `handoffEntryId`.
    let can_resolve = true;
    let screen = app.as_ref().and_then(|entity| {
        let state = entity.read(cx);
        state
            .coworker_screen
            .clone()
            .or_else(|| state.last_box_shot.as_ref().map(|shot| shot.image.clone()))
    });
    let key = spec.card_key().to_string();
    let prompt = spec.handoff_prompt();
    let height = 512. * 800. / 1280.;
    v_flex()
        .id(ElementId::Name(computer_handoff_card_id(&key).into()))
        .w_full()
        .gap(px(10.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .occlude()
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
                        .child("Computer"),
                )
                .child(
                    h_flex()
                        .items_center()
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(999.))
                        .bg(theme.yellow.opacity(0.22))
                        .text_color(theme.yellow)
                        .text_xs()
                        .child("Action needed"),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(prompt),
        )
        .child(
            div()
                .id(ElementId::Name(
                    format!("computer-handoff-screen-{key}").into(),
                ))
                .w_full()
                .h(px(height))
                .rounded(px(10.))
                .border_1()
                .border_color(theme.border)
                .bg(rgb(0x2a2a2a))
                .overflow_hidden()
                .cursor_pointer()
                .when_some(app.clone(), |this, app| {
                    this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        app.update(cx, |state, cx| {
                            state.take_over_computer(cx);
                        });
                    })
                })
                .map(|this| match screen {
                    Some(image) => this.child(
                        img(image)
                            .size_full()
                            .rounded(px(10.))
                            .object_fit(ObjectFit::Fill),
                    ),
                    None => this.child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::default()
                                    .path("icons/monitor.svg")
                                    .size(px(28.))
                                    .text_color(rgb(0x888888)),
                            ),
                    ),
                }),
        )
        .child(
            h_flex()
                .w_full()
                .justify_end()
                .gap(px(8.))
                .flex_wrap()
                .child(action_button(
                    computer_handoff_takeover_id(&key),
                    "Take over",
                    ButtonKind::Primary,
                    false,
                    false,
                    {
                        let app = app.clone();
                        Some(move |cx: &mut App| {
                            if let Some(app) = &app {
                                app.update(cx, |state, cx| {
                                    state.take_over_computer(cx);
                                });
                            }
                        })
                    },
                ))
                .child(action_button(
                    computer_handoff_done_id(&key),
                    "I'm done",
                    ButtonKind::Secondary,
                    !can_resolve,
                    false,
                    {
                        let app = app.clone();
                        let key = key.clone();
                        can_resolve.then_some(move |cx: &mut App| {
                            if let Some(app) = &app {
                                app.update(cx, |state, cx| {
                                    state.resolve_user_form_handoff(
                                        key.clone(),
                                        BoxHandoffResolution::HandedBack,
                                        cx,
                                    );
                                });
                            }
                        })
                    },
                ))
                .child(action_button(
                    computer_handoff_skip_id(&key),
                    "Skip",
                    ButtonKind::Ghost,
                    !can_resolve,
                    false,
                    {
                        let app = app.clone();
                        can_resolve.then_some(move |cx: &mut App| {
                            if let Some(app) = &app {
                                app.update(cx, |state, cx| {
                                    state.resolve_user_form_handoff(
                                        key.clone(),
                                        BoxHandoffResolution::Declined,
                                        cx,
                                    );
                                });
                            }
                        })
                    },
                )),
        )
        .into_any_element()
}

fn render_settled_computer_handoff(
    spec: &UserFormSpec,
    status: ComputerHandoffStatus,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let key = spec.card_key().to_string();
    v_flex()
        .id(ElementId::Name(computer_handoff_card_id(&key).into()))
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
                        .child("Computer"),
                )
                .child(
                    h_flex()
                        .id(ElementId::Name(
                            format!("computer-handoff-badge-{key}").into(),
                        ))
                        .items_center()
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(999.))
                        .bg(theme.secondary)
                        .text_color(theme.muted_foreground)
                        .text_xs()
                        .child(status.pill()),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(status.body()),
        )
        .into_any_element()
}

fn collect_submit_values(
    spec: &UserFormSpec,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    picks: &UserFormValues,
    cx: &App,
) -> UserFormValues {
    let mut out = UserFormValues::default();
    for field in &spec.fields {
        let key = field_key(spec.card_key(), &field.id);
        let raw = match field.kind {
            UserFormFieldKind::Checkbox | UserFormFieldKind::Select => {
                picks.by_id.get(&field.id).cloned().unwrap_or_default()
            }
            UserFormFieldKind::Textarea => {
                let live = textareas
                    .get(&key)
                    .map(|state| state.read(cx).value().to_string())
                    .unwrap_or_default();
                if live.trim().is_empty() {
                    picks.by_id.get(&field.id).cloned().unwrap_or_default()
                } else {
                    live
                }
            }
            _ => {
                let live = inputs
                    .get(&key)
                    .map(|state| state.read(cx).value().to_string())
                    .unwrap_or_default();
                if live.trim().is_empty() {
                    picks.by_id.get(&field.id).cloned().unwrap_or_default()
                } else {
                    live
                }
            }
        };
        if field.kind == UserFormFieldKind::Checkbox {
            out.by_id.insert(
                field.id.clone(),
                UserFormField::checkbox_wire(raw == "true").to_string(),
            );
            continue;
        }
        if raw.trim().is_empty() {
            continue;
        }
        out.by_id.insert(field.id.clone(), raw);
    }
    out
}

fn render_submitting(spec: &UserFormSpec, cx: &App) -> AnyElement {
    let theme = cx.theme();
    collapsed_card(
        spec,
        FormResolution::Sending,
        theme.secondary,
        theme.muted_foreground,
        true,
        None,
        theme,
    )
}

fn render_settled(
    spec: &UserFormSpec,
    resolution: FormResolution,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let pill_fill = match resolution {
        FormResolution::Submitted => theme.green.opacity(0.18),
        FormResolution::FillFailed => theme.danger.opacity(0.16),
        FormResolution::Escalated
        | FormResolution::Sending
        | FormResolution::Dismissed
        | FormResolution::Skipped => theme.secondary,
    };
    let pill_text = match resolution {
        FormResolution::Submitted => theme.green,
        FormResolution::FillFailed => theme.danger,
        _ => theme.muted_foreground,
    };
    let actions = match resolution {
        FormResolution::FillFailed => Some(fill_failed_actions(
            spec, inputs, textareas, values, app, cx,
        )),
        _ => None,
    };
    collapsed_card(
        spec, resolution, pill_fill, pill_text, false, actions, theme,
    )
}

fn collapsed_card(
    spec: &UserFormSpec,
    resolution: FormResolution,
    pill_fill: Hsla,
    pill_text: Hsla,
    spinner: bool,
    actions: Option<AnyElement>,
    theme: &Theme,
) -> AnyElement {
    v_flex()
        .id(ElementId::Name(user_form_card_id(spec.card_key()).into()))
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
                        .id(ElementId::Name(user_form_pill_id(spec.card_key()).into()))
                        .items_center()
                        .gap(px(4.))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(999.))
                        .bg(pill_fill)
                        .text_color(pill_text)
                        .text_xs()
                        .when(spinner, |this| {
                            this.child(
                                Spinner::new()
                                    .with_size(px(12.))
                                    .color(theme.muted_foreground),
                            )
                        })
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
        .when_some(actions, |this, actions| this.child(actions))
        .into_any_element()
}

fn fill_failed_actions(
    spec: &UserFormSpec,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    values: &UserFormValues,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let server_fill = app
        .as_ref()
        .map(|entity| entity.read(cx).user_form_verbs_available)
        .unwrap_or(USER_FORM_SERVER_FILL_AVAILABLE);
    let can_post = spec.can_post(server_fill);
    let key = spec.card_key().to_string();
    h_flex()
        .w_full()
        .justify_end()
        .gap(px(8.))
        .flex_wrap()
        .child(action_button(
            format!("user-form-retry-{key}"),
            "Try again",
            ButtonKind::Primary,
            !can_post,
            !can_post,
            {
                let spec = spec.clone();
                let inputs = inputs.clone();
                let textareas = textareas.clone();
                let picks = values.clone();
                let app = app.clone();
                let key = key.clone();
                can_post.then_some(move |cx: &mut App| {
                    let values = collect_submit_values(&spec, &inputs, &textareas, &picks, cx);
                    if let Some(app) = &app {
                        app.update(cx, |state, cx| {
                            state.submit_user_form(key.clone(), values, cx);
                        });
                    }
                })
            },
        ))
        .child(action_button(
            format!("user-form-failed-screen-{key}"),
            "I'll do it on the computer",
            ButtonKind::Secondary,
            !can_post,
            !can_post,
            {
                let app = app.clone();
                let key = key.clone();
                can_post.then_some(move |cx: &mut App| {
                    if let Some(app) = &app {
                        app.update(cx, |state, cx| {
                            state.dismiss_user_form(
                                key.clone(),
                                UserFormDismissMode::Escalated,
                                cx,
                            );
                        });
                    }
                })
            },
        ))
        .child(action_button(
            format!("user-form-failed-stop-{key}"),
            "Stop for now",
            ButtonKind::Ghost,
            !can_post,
            !can_post,
            {
                let app = app.clone();
                can_post.then_some(move |cx: &mut App| {
                    if let Some(app) = &app {
                        app.update(cx, |state, cx| {
                            state.dismiss_user_form(
                                key.clone(),
                                UserFormDismissMode::Dismissed,
                                cx,
                            );
                        });
                    }
                })
            },
        ))
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
    let key = field_key(spec.card_key(), &field.id);
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
    let field_el = user_form_field_id(spec.card_key(), &field.id);
    let control = div()
        .id(ElementId::Name(field_el.into()))
        .w_full()
        .child(control);
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
    let card_key = spec.card_key().to_string();
    let field_id = field.id.clone();
    let next = if on { "false" } else { "true" };
    h_flex()
        .id(ElementId::Name(
            user_form_field_id(spec.card_key(), &field.id).into(),
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
                        card_key.clone(),
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
        let card_key = spec.card_key().to_string();
        let field_id = field.id.clone();
        let value = option.value.clone();
        let app = app.clone();
        chips = chips.child(
            div()
                .id(ElementId::Name(
                    format!("user-form-opt-{}-{field_id}-{value}", spec.card_key()).into(),
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
                                card_key.clone(),
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

fn action_button(
    id: String,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    disabled: bool,
    coming_from_server: bool,
    on_click: Option<impl Fn(&mut App) + 'static>,
) -> AnyElement {
    let button = Button::new(id).label(label).disabled(disabled);
    let button = match kind {
        ButtonKind::Primary => button.primary(),
        ButtonKind::Secondary => button.outline(),
        ButtonKind::Ghost => button.ghost(),
    };
    let button = match on_click {
        Some(on_click) => button.on_click(move |_, _, cx| on_click(cx)),
        None => button,
    };
    button
        .when(coming_from_server, |this| {
            this.tooltip("Coming from the server")
        })
        .into_any_element()
}
