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

use crate::chrome::BOX_SCREEN_ASPECT;
use crate::components::alert_chrome::{
    attention_cta, attention_ctas, attention_glass, attention_shadow,
};
use crate::components::fields::field_input;
use crate::opengrok::{
    BoxHandoffResolution, ComputerHandoffStatus, FormResolution, USER_FORM_SERVER_FILL_AVAILABLE,
    UserFormDismissMode, UserFormField, UserFormFieldKind, UserFormSpec, UserFormValues,
    computer_handoff_card_id, computer_handoff_done_id, computer_handoff_skip_id,
    computer_handoff_takeover_id, continue_enabled, user_form_card_id, user_form_continue_id,
    user_form_dismiss_id, user_form_field_id, user_form_pill_id, user_form_saved_clear_id,
    user_form_saved_list_id, user_form_saved_note_id, user_form_screen_id, user_form_use_saved_id,
};
use crate::site_login::SavedLoginUse;
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputContentType, InputState, Textarea, TextareaState};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};

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
    let can_dismiss = spec.can_dismiss();
    let mut picks = values.clone();
    if let Some(app) = &app
        && let Some(typed) = app.read(cx).user_form_typed.get(spec.card_key())
    {
        for (id, value) in typed {
            picks
                .by_id
                .entry(id.clone())
                .or_insert_with(|| value.clone());
        }
    }
    // Live InputState / TextareaState / picks / agent-typed — not a masked stub.
    let live = collect_submit_values(spec, inputs, textareas, &picks, cx);
    let saved = SavedLoginContext::for_card(spec, app.as_ref(), cx);
    let saved_busy = saved.current.as_ref().is_some_and(SavedLoginUse::is_busy);
    let held: Vec<&str> = saved.held_field().into_iter().collect();
    // A passkey card has no fields: the button lives once Touch ID passed.
    let ready = saved.current.as_ref().is_some_and(|c| c.ready().is_some());
    let can_continue = can_post
        && !saved_busy
        && if saved.is_passkey_card() {
            ready
        } else {
            spec.required_fields_filled_with(&live, &held)
        };
    let can_post = can_post && !saved_busy;
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
            &saved,
            app.clone(),
            cx,
        ));
    }
    let key = spec.card_key().to_string();
    if saved.is_passkey_card() {
        body = body.child(render_passkey_block(
            spec,
            &saved,
            can_post,
            app.clone(),
            cx,
        ));
    }
    if let Some(current) = saved.current.as_ref().filter(|c| c.ready().is_none()) {
        body = body.child(render_saved_login_note(&key, current, cx));
    }
    let can_dismiss = can_dismiss && !saved_busy;
    body.child(
        h_flex()
            .w_full()
            .justify_end()
            .gap(px(8.))
            .flex_wrap()
            .child(action_button(
                user_form_continue_id(&key),
                spec.continue_label(),
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
                !can_dismiss,
                !can_dismiss,
                {
                    let app = app.clone();
                    let key = key.clone();
                    can_dismiss.then_some(move |cx: &mut App| {
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
                !can_dismiss,
                !can_dismiss,
                {
                    let app = app.clone();
                    can_dismiss.then_some(move |cx: &mut App| {
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
    let dark = cx.theme().is_dark();
    let glass = attention_glass(dark);
    let ctas = attention_ctas(dark);
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
    v_flex()
        .id(ElementId::Name(computer_handoff_card_id(&key).into()))
        .w_full()
        .gap(px(12.))
        .p(px(16.))
        .rounded(px(14.))
        .border_1()
        .border_color(glass.card_border)
        .bg(glass.card_bg)
        .shadow(attention_shadow(dark))
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
                        .gap(px(4.))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(999.))
                        .bg(glass.badge_bg)
                        .text_color(glass.badge_fg)
                        .text_xs()
                        .child(
                            Icon::default()
                                .path("icons/sun.svg")
                                .size(px(12.))
                                .text_color(glass.badge_fg),
                        )
                        .child("Action needed"),
                ),
        )
        .child(div().text_sm().text_color(glass.body).child(prompt))
        .child(
            div()
                .id(ElementId::Name(
                    format!("computer-handoff-screen-{key}").into(),
                ))
                .w_full()
                .aspect_ratio(BOX_SCREEN_ASPECT)
                .flex_shrink_0()
                .rounded(px(12.))
                .border_1()
                .border_color(glass.card_border)
                .bg(glass.preview_bg)
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
                            .rounded(px(12.))
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
                .child(attention_cta(
                    computer_handoff_takeover_id(&key),
                    "Take over",
                    ctas.primary,
                    true,
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
                .child(attention_cta(
                    computer_handoff_done_id(&key),
                    "I'm done",
                    ctas.secondary,
                    true,
                    !can_resolve,
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
                .child(attention_cta(
                    computer_handoff_skip_id(&key),
                    "Skip",
                    ctas.tertiary,
                    false,
                    !can_resolve,
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
        | FormResolution::Skipped
        | FormResolution::Superseded => theme.secondary,
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
    // Same inputs as the idle card's Continue: the live InputState plus whatever
    // the agent typed, and enabled only when the required fields are present.
    // Gating on `can_post` alone let "Try again" go out with the password gone.
    let mut picks = values.clone();
    if let Some(app) = &app
        && let Some(typed) = app.read(cx).user_form_typed.get(spec.card_key())
    {
        for (id, value) in typed {
            picks
                .by_id
                .entry(id.clone())
                .or_insert_with(|| value.clone());
        }
    }
    let live = collect_submit_values(spec, inputs, textareas, &picks, cx);
    let can_post = continue_enabled(spec, &live, server_fill);
    let can_dismiss = spec.can_dismiss();
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
                let picks = picks.clone();
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
            !can_dismiss,
            !can_dismiss,
            {
                let app = app.clone();
                let key = key.clone();
                can_dismiss.then_some(move |cx: &mut App| {
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
            !can_dismiss,
            !can_dismiss,
            {
                let app = app.clone();
                can_dismiss.then_some(move |cx: &mut App| {
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

/// What the card knows about the person's vault for its site: what the card takes (a login,
/// a page of a two-step sign-in, a code, a passkey), the rows to list, and where a pick is.
#[derive(Default)]
struct SavedLoginContext {
    rows: Vec<crate::site_login::SiteLoginRecord>,
    target: Option<crate::site_login::CardTarget>,
    current: Option<SavedLoginUse>,
    /// The list is up: the person put the cursor in the field it belongs to. A card that
    /// has just arrived has it down.
    open: bool,
    /// Why a saved login cannot be used for this card's Bot, asked before Touch ID: the list
    /// says so in place of the logins, with a way on.
    blocked: Option<Blocked>,
    /// The card's Bot and the logins shared with it, once read: those come first, and the
    /// rest are offered to share with it.
    shares: Option<(String, HashSet<String>)>,
}

#[derive(Clone, Default)]
struct Blocked {
    reason: String,
    bot: String,
    bot_name: String,
    /// The server is making the Bot its own computer.
    changing: bool,
    refusal: Option<String>,
}

impl SavedLoginContext {
    fn for_card(spec: &UserFormSpec, app: Option<&Entity<AppState>>, cx: &App) -> Self {
        let Some(app) = app else {
            return Self::default();
        };
        let state = app.read(cx);
        let rows = state.saved_logins_for_form(spec);
        let current = state.saved_login_use.get(spec.card_key()).cloned();
        let open = state.user_form_list_open.contains(spec.card_key());
        let target = crate::site_login::card_target(spec);
        let target = if rows.is_empty()
            && current.is_none()
            && !matches!(target, Some(crate::site_login::CardTarget::Passkey { .. }))
        {
            None
        } else {
            target
        };
        let blocked = state.card_coworker(spec.card_key()).and_then(|bot| {
            let check = state.saved_login_checks.get(&bot).filter(|c| !c.usable)?;
            let bot_name = state
                .coworkers
                .iter()
                .find(|c| c.id == bot)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "this Bot".to_string());
            Some(Blocked {
                reason: check.reason.clone().unwrap_or_default(),
                changing: state.own_computer_changing.as_deref() == Some(bot.as_str()),
                refusal: state.own_computer_refusal.clone(),
                bot,
                bot_name,
            })
        });
        let shares = state.card_coworker(spec.card_key()).and_then(|bot| {
            let ids = state.site_login_shares.get(&bot)?.clone();
            let name = state
                .coworkers
                .iter()
                .find(|c| c.id == bot)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "this Bot".to_string());
            Some((name, ids))
        });
        // The Bot's own logins first, keeping their order within each group.
        let mut rows = rows;
        if let Some((_, ids)) = &shares {
            rows.sort_by_key(|row| !ids.contains(&row.id));
        }
        Self {
            rows,
            target,
            current,
            open,
            blocked,
            shares,
        }
    }

    /// The field the list sits under: the name field of a login or of a name page, the code
    /// field of a code card, the password field of a password page.
    fn list_field(&self) -> Option<&str> {
        self.target.as_ref()?.list_field()
    }

    /// The field a pick's name goes in: a login's, or a name page's only field.
    fn name_field(&self) -> Option<&str> {
        self.target.as_ref()?.name_field()
    }

    fn is_name_field(&self, field_id: &str) -> bool {
        self.name_field() == Some(field_id)
    }

    /// The secret field: the password of a login or of a password page, the code field of a
    /// code card.
    fn is_secret_field(&self, field_id: &str) -> bool {
        self.target
            .as_ref()
            .and_then(crate::site_login::CardTarget::secret_field)
            == Some(field_id)
    }

    /// The field a held pick fills, while one is held for the submit: the secret field, or on
    /// the name page of a two-step sign-in, which holds no secret, the name.
    fn held_field(&self) -> Option<&str> {
        let ready = self.current.as_ref()?.ready().is_some();
        if !ready {
            return None;
        }
        let target = self.target.as_ref()?;
        target.secret_field().or_else(|| target.name_field())
    }

    /// The name page of a two-step sign-in: the name alone, with the password on the next card.
    fn is_name_page(&self) -> bool {
        matches!(
            self.target,
            Some(crate::site_login::CardTarget::Username { .. })
        )
    }

    fn is_passkey_card(&self) -> bool {
        matches!(
            self.target,
            Some(crate::site_login::CardTarget::Passkey { .. })
        )
    }

    fn is_passkey_register(&self) -> bool {
        matches!(
            self.target,
            Some(crate::site_login::CardTarget::Passkey { register: true })
        )
    }

    fn is_code_card(&self) -> bool {
        matches!(
            self.target,
            Some(crate::site_login::CardTarget::Code { .. })
        )
    }

    /// The account list shows until a pick is under way or held.
    fn shows_list(&self) -> bool {
        self.open
            && !self.rows.is_empty()
            && !matches!(
                self.current,
                Some(SavedLoginUse::Confirming { .. })
                    | Some(SavedLoginUse::Ready { .. })
                    | Some(SavedLoginUse::NameReady { .. })
                    | Some(SavedLoginUse::Filling { .. })
            )
    }
}

#[allow(clippy::too_many_arguments)]
fn render_field(
    spec: &UserFormSpec,
    field: &UserFormField,
    inputs: &UserFormInputMap,
    textareas: &UserFormTextareaMap,
    values: &UserFormValues,
    saved: &SavedLoginContext,
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
    if saved.held_field().is_some() {
        if saved.is_secret_field(&field.id) {
            return render_locked_password(spec.card_key(), &label, saved, app, cx);
        }
        if saved.is_name_field(&field.id) {
            return render_locked_name(spec.card_key(), &field.id, &label, saved, app, cx);
        }
    }
    let control = match field.kind {
        UserFormFieldKind::Checkbox => render_checkbox(spec, field, values, app.clone(), cx),
        UserFormFieldKind::Select => render_select(spec, field, values, app.clone(), cx),
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
    let mut control = div()
        .id(ElementId::Name(field_el.into()))
        .relative()
        .w_full()
        .child(control);
    if saved.list_field() == Some(field.id.as_str()) && saved.shows_list() {
        control = control.child(floating_account_list(spec, saved, inputs, app.clone(), cx));
    }
    // A click in the field the accounts belong to brings the list up, whether or not the
    // cursor was already there. The focus event covers tabbing into it.
    if saved.list_field() == Some(field.id.as_str()) && !saved.shows_list() {
        let app = app.clone();
        let card_key = spec.card_key().to_string();
        let field_id = field.id.clone();
        control = control.on_mouse_down(MouseButton::Left, move |_, _, cx| {
            if let Some(app) = &app {
                app.update(cx, |state, cx| {
                    state.open_saved_login_list(card_key.clone(), &field_id, cx);
                });
            }
        });
    }
    let show_label = field.kind != UserFormFieldKind::Checkbox;
    let column = v_flex()
        .gap(px(6.))
        .when(show_label, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(label),
            )
        })
        .child(control);
    column.into_any_element()
}

/// A passkey card has no fields. It lists the person's passkeys for the site (or, when the
/// site offers to make one, a single row to confirm that), and once Touch ID passed it says
/// the passkey is ready for the button. The key never leaves the server.
fn render_passkey_block(
    spec: &UserFormSpec,
    saved: &SavedLoginContext,
    can_post: bool,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let card_key = spec.card_key().to_string();
    let site = spec
        .live_host
        .clone()
        .or(spec.domain.clone())
        .unwrap_or_default();
    if let Some((username, _)) = saved.current.as_ref().and_then(SavedLoginUse::ready) {
        let text = if saved.is_passkey_register() {
            format!(
                "Touch ID confirmed. Press Create passkey; {site} makes one in the bot's browser, \
                 for whichever account is signed in there."
            )
        } else {
            format!(
                "Passkey for {username} ready. Press Use passkey; the site's challenge is signed in the bot's browser."
            )
        };
        return h_flex()
            .id(ElementId::Name(
                user_form_field_id(&card_key, "passkey-ready").into(),
            ))
            .w_full()
            .items_center()
            .gap(px(10.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.4))
            .child(
                Icon::default()
                    .path("icons/key.svg")
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
            )
            .child(div().flex_1().text_sm().child(text))
            .child(
                Button::new(user_form_saved_clear_id(&card_key))
                    .label("Change")
                    .ghost()
                    .xsmall()
                    .on_click({
                        let app = app.clone();
                        let key = card_key.clone();
                        move |_, _, cx| {
                            if let Some(app) = &app {
                                app.update(cx, |state, cx| {
                                    state.clear_saved_login_pick(key.clone(), cx);
                                });
                            }
                        }
                    }),
            )
            .into_any_element();
    }
    let busy = saved.current.as_ref().is_some_and(SavedLoginUse::is_busy);
    if saved.is_passkey_register() {
        let id = format!("user-form-passkey-register-{card_key}");
        return h_flex()
            .id(ElementId::Name(id.into()))
            .w_full()
            .items_center()
            .gap(px(10.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .cursor_pointer()
            .hover(|this| this.bg(theme.muted))
            .child(
                Icon::default()
                    .path("icons/key.svg")
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
            )
            .child(
                v_flex()
                    .flex_1()
                    .child(div().text_sm().child(format!("Create a passkey for {site}")))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Touch ID confirms it; the site makes the key in the bot's browser and it is saved to your logins."),
                    ),
            )
            .on_click(move |_, _, cx| {
                if busy || !can_post {
                    return;
                }
                if let Some(app) = &app {
                    app.update(cx, |state, cx| {
                        state.confirm_passkey_register(card_key.clone(), cx);
                    });
                }
            })
            .into_any_element();
    }
    if saved.rows.is_empty() {
        return div()
            .id(ElementId::Name(user_form_saved_note_id(&card_key).into()))
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(format!("No passkey saved for {site}. Sign in another way, or let the site add one after you are in."))
            .into_any_element();
    }
    let mut list = v_flex()
        .id(ElementId::Name(user_form_saved_list_id(&card_key).into()))
        .w_full()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .overflow_hidden();
    for (i, row) in saved.rows.iter().enumerate() {
        if i > 0 {
            list = list.child(div().h(px(1.)).bg(theme.border));
        }
        let id = user_form_use_saved_id(&card_key, &row.id);
        let app = app.clone();
        let key = card_key.clone();
        let login_id = row.id.clone();
        list = list.child(
            h_flex()
                .id(ElementId::Name(id.into()))
                .w_full()
                .items_center()
                .gap(px(10.))
                .px(px(10.))
                .py(px(8.))
                .cursor_pointer()
                .hover(|this| this.bg(theme.muted))
                .child(
                    Icon::default()
                        .path("icons/key.svg")
                        .size(px(14.))
                        .text_color(theme.muted_foreground),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .child(div().text_sm().child(row.username.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("passkey · {}", row.origin)),
                        ),
                )
                .on_click(move |_, _, cx| {
                    if busy || !can_post {
                        return;
                    }
                    if let Some(app) = &app {
                        app.update(cx, |state, cx| {
                            state.pick_saved_login(key.clone(), login_id.clone(), cx);
                        });
                    }
                }),
        );
    }
    list.child(
        div()
            .px(px(10.))
            .py(px(6.))
            .text_xs()
            .text_color(theme.muted_foreground)
            .child("Choose a passkey. Touch ID confirms it; the key stays on the server."),
    )
    .into_any_element()
}

/// The name field once a pick is held: the picked name, read-only, so the name that is sent
/// is the one the pick belongs to. On a login, Change (on the password row) frees both; the
/// name page of a two-step sign-in has no password row, so its Change is here.
fn render_locked_name(
    card_key: &str,
    field_id: &str,
    label: &str,
    saved: &SavedLoginContext,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let username = saved
        .current
        .as_ref()
        .and_then(SavedLoginUse::ready)
        .map(|(u, _)| u.to_string())
        .unwrap_or_default();
    let key = card_key.to_string();
    v_flex()
        .gap(px(6.))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(
            h_flex()
                .id(ElementId::Name(
                    user_form_field_id(card_key, field_id).into(),
                ))
                .w_full()
                .items_center()
                .gap(px(10.))
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.4))
                .child(
                    Icon::default()
                        .path("icons/key.svg")
                        .size(px(14.))
                        .text_color(theme.muted_foreground),
                )
                .child(div().flex_1().text_sm().child(username))
                .when(saved.is_name_page(), |row| {
                    row.child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("From your saved logins"),
                    )
                    .child(
                        Button::new(user_form_saved_clear_id(&key))
                            .label("Change")
                            .ghost()
                            .xsmall()
                            .on_click(move |_, _, cx| {
                                if let Some(app) = &app {
                                    app.update(cx, |state, cx| {
                                        state.clear_saved_login_pick(key.clone(), cx);
                                    });
                                }
                            }),
                    )
                }),
        )
        .into_any_element()
}

/// The line under the fields while a pick is under way, or after one did not go through
/// (Touch ID cancelled, the keychain empty, the server's refusal). At card level so a
/// refusal shows on any card, not only one with a name field.
fn render_saved_login_note(card_key: &str, current: &SavedLoginUse, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let color = match current {
        SavedLoginUse::Refused { .. } | SavedLoginUse::Unavailable { .. } => theme.danger,
        _ => theme.muted_foreground,
    };
    div()
        .id(ElementId::Name(user_form_saved_note_id(card_key).into()))
        .text_xs()
        .text_color(color)
        .child(current.note())
        .into_any_element()
}

/// How wide the floating list is. It floats free of the card's own column, so it carries
/// its own width the way a menu does rather than stretching to the field.
const LIST_WIDTH: f32 = 300.;

/// The room the list keeps under itself. It does two things at once: the turn is decided
/// on the list plus this, so the list turns over while the composer's height still lies
/// between it and the window's edge; and once it has turned over, this is what lifts it
/// clear of the field it belongs to instead of leaving it sitting on top of it.
const FIELD_CLEARANCE: f32 = 64.;

/// The account list, floating over the card instead of pushing the password field and the
/// buttons down it. It hangs under the name field while there is room and turns over above
/// it when there is not — a card sitting near the composer, say. `anchored` makes that
/// choice itself, from the window's own edge, and the element left behind in the column has
/// no height, so nothing moves either way.
fn floating_account_list(
    spec: &UserFormSpec,
    saved: &SavedLoginContext,
    inputs: &UserFormInputMap,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let card_key = spec.card_key().to_string();
    let away = app.clone();
    let list = render_account_list(spec, saved, inputs, app, cx);
    // Pinned to the field's bottom edge and out of the flow: it adds no height, and no gap
    // to the column either, so the card is exactly as tall with the list up as without it.
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .w_full()
        .h(px(0.))
        .child(
            deferred(
                anchored()
                    .position_mode(AnchoredPositionMode::Local)
                    .position(point(px(0.), px(0.)))
                    .child(
                        // The tail is empty room, and it is what makes the turn land right:
                        // the right way up it keeps the list off the composer, and turned
                        // over it holds the list above the field rather than over it.
                        v_flex()
                            .w(px(LIST_WIDTH))
                            .child(list)
                            .child(div().w_full().h(px(FIELD_CLEARANCE)))
                            // A click anywhere else is not a choice: the list goes away and
                            // the field is free to type in. The cursor back in that field
                            // brings it back.
                            .on_mouse_down_out(move |_, window, cx| {
                                // The field lets go too, so putting the cursor back in it
                                // is a fresh focus and the list comes up again.
                                window.blur(cx);
                                if let Some(app) = &away {
                                    app.update(cx, |state, cx| {
                                        state.close_saved_login_list(card_key.clone(), cx);
                                    });
                                }
                            }),
                    ),
            )
            .with_priority(4),
        )
        .into_any_element()
}

/// The accounts saved for this site, listed under the name field the way a browser's
/// autofill does (under the one field of a code card or a password page): pick one, confirm
/// with Touch ID, and the fields fill. The password never appears here; each row is the
/// account's name, so on a password page the person knows whose password they are choosing.
/// In place of the logins, when this Bot cannot be given one: why, before any Touch ID, and the
/// way on. A shared computer can become the Bot's own; a Bot shown to an org cannot take a saved
/// login at all, so only typing is offered (8 Oct 2026).
fn blocked_panel(
    card_key: &str,
    blocked: &Blocked,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let name = &blocked.bot_name;
    let words = if blocked.reason == "shared-bot" {
        format!(
            "{name} is shared with your org, so your saved logins can't be used with it. Type \
             the login by hand."
        )
    } else {
        format!(
            "{name}'s computer is shared with other Bots, so a saved login can't be used on it. \
             Give {name} a computer of its own, or type the login by hand."
        )
    };
    let mut buttons = h_flex().gap(px(8.)).flex_wrap();
    if blocked.reason != "shared-bot" {
        let own = app.clone();
        let bot = blocked.bot.clone();
        buttons = buttons.child(
            Button::new(SharedString::from(
                crate::opengrok::user_form_own_computer_id(card_key),
            ))
            .primary()
            .small()
            .loading(blocked.changing)
            .disabled(blocked.changing)
            .label(if blocked.changing {
                "Making its computer…".to_string()
            } else {
                format!("Give {name} its own computer")
            })
            .on_click(move |_, _, cx| {
                if let Some(app) = &own {
                    app.update(cx, |state, cx| state.give_bot_own_computer(bot.clone(), cx));
                }
            }),
        );
    }
    let key = card_key.to_string();
    buttons = buttons.child(
        Button::new(SharedString::from(crate::opengrok::user_form_by_hand_id(
            card_key,
        )))
        .small()
        .label("Type it by hand")
        .on_click(move |_, _, cx| {
            if let Some(app) = &app {
                app.update(cx, |state, cx| state.type_login_by_hand(key.clone(), cx));
            }
        }),
    );
    v_flex()
        .gap(px(8.))
        .p(px(10.))
        .child(div().text_sm().child(words))
        .when(blocked.reason != "shared-bot", |this| {
            this.child(div().text_xs().text_color(theme.muted_foreground).child(
                "It starts on a fresh computer; what it left on the shared one stays there.",
            ))
        })
        .child(buttons)
        .when_some(blocked.refusal.clone(), |this, why| {
            this.child(div().text_xs().text_color(theme.danger).child(why))
        })
        .into_any_element()
}

fn render_account_list(
    spec: &UserFormSpec,
    saved: &SavedLoginContext,
    inputs: &UserFormInputMap,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let card_key = spec.card_key().to_string();
    // The password page has no name field: a pick there puts nothing in an input.
    let name_input = saved
        .name_field()
        .and_then(|id| inputs.get(&field_key(&card_key, id)).cloned());
    let mut list = v_flex()
        .id(ElementId::Name(user_form_saved_list_id(&card_key).into()))
        .w_full()
        .mt(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_lg()
        // It floats over the card: a click on it is for it, not for what lies under.
        .occlude()
        .overflow_hidden();
    if let Some(blocked) = &saved.blocked {
        return list
            .child(blocked_panel(&card_key, blocked, app, cx))
            .into_any_element();
    }
    let shared = |id: &str| {
        saved
            .shares
            .as_ref()
            .is_none_or(|(_, ids)| ids.contains(id))
    };
    let group = |words: String| {
        div()
            .px(px(10.))
            .pt(px(8.))
            .pb(px(2.))
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(words)
    };
    if let Some((name, _)) = &saved.shares
        && saved.rows.first().is_some_and(|row| shared(&row.id))
    {
        list = list.child(group(format!("{name}'s logins")));
    }
    for (i, row) in saved.rows.iter().enumerate() {
        let is_shared = shared(&row.id);
        let starts_others = !is_shared && (i == 0 || shared(&saved.rows[i - 1].id));
        if starts_others && saved.shares.is_some() {
            list = list.child(group("Your other logins".to_string()));
        } else if i > 0 {
            list = list.child(div().h(px(1.)).bg(theme.border));
        }
        let share_tag = (!is_shared)
            .then(|| {
                saved
                    .shares
                    .as_ref()
                    .map(|(name, _)| format!("Share with {name}"))
            })
            .flatten();
        let id = user_form_use_saved_id(&card_key, &row.id);
        let app = app.clone();
        let key = card_key.clone();
        let login_id = row.id.clone();
        let username = row.username.clone();
        let name_input = name_input.clone();
        list = list.child(
            h_flex()
                .id(ElementId::Name(id.into()))
                .w_full()
                .items_center()
                .gap(px(10.))
                .px(px(10.))
                .py(px(8.))
                .cursor_pointer()
                .hover(|this| this.bg(theme.muted))
                .child(
                    Icon::default()
                        .path("icons/key.svg")
                        .size(px(14.))
                        .text_color(theme.muted_foreground),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .child(div().text_sm().child(row.username.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(row.origin.clone()),
                        ),
                )
                .when_some(share_tag, |this, tag| {
                    this.child(
                        div()
                            .text_xs()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded_full()
                            .border_1()
                            .border_color(theme.border)
                            .text_color(theme.muted_foreground)
                            .child(tag),
                    )
                })
                .on_click(move |_, window, cx| {
                    if let Some(input) = &name_input {
                        input.update(cx, |input, cx| {
                            input.set_value(username.clone(), window, cx)
                        });
                    }
                    if let Some(app) = &app {
                        app.update(cx, |state, cx| {
                            state.pick_saved_login(key.clone(), login_id.clone(), cx);
                        });
                    }
                }),
        );
    }
    list.child(
        div()
            .px(px(10.))
            .py(px(6.))
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(match &saved.target {
                Some(crate::site_login::CardTarget::Code { .. }) => {
                    "Choose an account. Touch ID fills in its code."
                }
                Some(crate::site_login::CardTarget::Username { .. }) => {
                    "Choose an account. Touch ID fills in its name; the password comes next."
                }
                Some(crate::site_login::CardTarget::Password { .. }) => {
                    "Choose an account. Touch ID fills in its password."
                }
                _ => "Choose an account. Touch ID fills it in.",
            }),
    )
    .into_any_element()
}

/// The password field once a saved password is held: dots, a key, and a way to type one
/// instead. The value itself is not in any input.
fn render_locked_password(
    card_key: &str,
    label: &str,
    saved: &SavedLoginContext,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let username = saved
        .current
        .as_ref()
        .and_then(SavedLoginUse::ready)
        .map(|(u, _)| u.to_string())
        .unwrap_or_default();
    let key = card_key.to_string();
    v_flex()
        .gap(px(6.))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label.to_string()),
        )
        .child(
            h_flex()
                .id(ElementId::Name(
                    user_form_field_id(card_key, "password-from-keychain").into(),
                ))
                .w_full()
                .items_center()
                .gap(px(10.))
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.4))
                .child(
                    Icon::default()
                        .path("icons/key.svg")
                        .size(px(14.))
                        .text_color(theme.muted_foreground),
                )
                .child(div().flex_1().text_sm().child("••••••••••"))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(if saved.is_code_card() {
                            format!("Code from your keychain, for {username}; minted when you press Continue")
                        } else {
                            format!("From your keychain, for {username}")
                        }),
                )
                .child(
                    Button::new(user_form_saved_clear_id(&key))
                        .label("Change")
                        .ghost()
                        .xsmall()
                        .on_click(move |_, _, cx| {
                            if let Some(app) = &app {
                                app.update(cx, |state, cx| {
                                    state.clear_saved_login_pick(key.clone(), cx);
                                });
                            }
                        }),
                ),
        )
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
