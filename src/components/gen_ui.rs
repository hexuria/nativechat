//! Native AG-UI widgets. Not markdown, not KaTeX.

use crate::opengrok::{
    ApprovalSpec, BarChartSpec, FormSpec, LocalExecResolution, ScreenshotSpec, UiSpec,
};
use crate::state::{AppState, ApprovalDecision};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub fn render_ui_spec(
    spec: &UiSpec,
    message_id: &str,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    match spec {
        UiSpec::BarChart(chart) => render_bar_chart(chart, cx),
        UiSpec::Form(form) => render_form(form, message_id, app, cx),
    }
}

/// The bot's screen as a card: the picture at the feed's width, its own words underneath.
pub fn render_screenshot(spec: &ScreenshotSpec, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let width = 520.0_f32;
    let height = width * spec.height.max(1) as f32 / spec.width.max(1) as f32;
    v_flex()
        .gap(px(6.))
        .child(
            img(spec.image.clone())
                .w(px(width))
                .h(px(height))
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border),
        )
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(spec.caption.clone()),
        )
        .into_any_element()
}

pub fn render_approval(spec: &ApprovalSpec, app: Option<Entity<AppState>>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (decision, bot, machine) = app
        .as_ref()
        .map(|entity| {
            let state = entity.read(cx);
            let decision = state
                .approval_decisions
                .get(&spec.call_id)
                .cloned()
                .unwrap_or(ApprovalDecision::Pending);
            (
                decision,
                state.active_bot_name(),
                state.local_exec_machine_id.clone().unwrap_or_default(),
            )
        })
        .unwrap_or((
            ApprovalDecision::Pending,
            "this agent".into(),
            String::new(),
        ));
    if let Some(line) = decision.outcome_line(&bot, spec.place()) {
        return div()
            .w_full()
            .py(px(8.))
            .flex()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(line),
            )
            .into_any_element();
    }
    if matches!(decision, ApprovalDecision::Failed(ref message) if !message.is_empty()) {
        let message = match &decision {
            ApprovalDecision::Failed(message) => message.clone(),
            _ => String::new(),
        };
        return div()
            .w_full()
            .py(px(8.))
            .flex()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(message),
            )
            .into_any_element();
    }
    let mut body = v_flex()
        .w_full()
        .gap(px(8.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background);
    body = body.child(
        h_flex()
            .w_full()
            .items_start()
            .justify_between()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if spec.runs_on_this_mac() {
                        format!("Allow {bot} and all Bots to run commands on your local computer?")
                    } else {
                        format!("Allow {bot} to run {} on its computer?", spec.tool)
                    }),
            )
            .child(dismiss_button(spec, app.clone(), theme.muted_foreground)),
    );
    if spec.runs_on_this_mac() {
        if !machine.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(machine),
            );
        }
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "This applies to {bot} and every Bot. It can always be changed in Settings."
                )),
        );
    }
    let command = if spec.command.trim().is_empty() {
        "Command was not included with this request.".to_string()
    } else {
        spec.command.clone()
    };
    body = body.child(
        div()
            .w_full()
            .px(px(8.))
            .py(px(6.))
            .rounded(px(6.))
            .bg(theme.secondary)
            .text_xs()
            .text_color(theme.secondary_foreground)
            .child(command),
    );
    if matches!(decision, ApprovalDecision::Sending) {
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Sending…"),
        );
    } else {
        // Always/Never set this Mac's policy, so only the local-shell tool
        // offers them. A box tool is answered one request at a time.
        let local = spec.runs_on_this_mac();
        let (primary, plain) = (
            (theme.primary, theme.primary_foreground, theme.primary),
            (theme.border, theme.foreground, theme.background),
        );
        let allow_once = if local { plain } else { primary };
        let mut row = h_flex().w_full().justify_end().gap(px(8.)).flex_wrap();
        if local {
            row = row.child(approval_button(
                spec,
                "Always allow",
                LocalExecResolution::Always,
                app.clone(),
                primary.0,
                primary.1,
                primary.2,
            ));
        }
        row = row
            .child(approval_button(
                spec,
                "Allow once",
                LocalExecResolution::AllowOnce,
                app.clone(),
                allow_once.0,
                allow_once.1,
                allow_once.2,
            ))
            .child(approval_button(
                spec,
                "Deny once",
                LocalExecResolution::DenyOnce,
                app.clone(),
                plain.0,
                plain.1,
                plain.2,
            ));
        if local {
            row = row.child(approval_button(
                spec,
                "Never",
                LocalExecResolution::Never,
                app,
                plain.0,
                plain.1,
                plain.2,
            ));
        }
        body = body.child(row);
    }
    body.into_any_element()
}

fn dismiss_button(spec: &ApprovalSpec, app: Option<Entity<AppState>>, color: Hsla) -> AnyElement {
    let spec = spec.clone();
    div()
        .id(ElementId::Name(
            format!("approval-dismiss-{}", spec.call_id).into(),
        ))
        .cursor_pointer()
        .text_color(color)
        .child("×")
        .tooltip(|w, cx| Tooltip::new("Deny this time").build(w, cx))
        .when_some(app, |this, app| {
            this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                app.update(cx, |state, cx| {
                    state.answer_approval(spec.clone(), LocalExecResolution::DenyOnce, cx);
                });
            })
        })
        .into_any_element()
}

fn approval_button(
    spec: &ApprovalSpec,
    label: &'static str,
    resolution: LocalExecResolution,
    app: Option<Entity<AppState>>,
    border: Hsla,
    text: Hsla,
    fill: Hsla,
) -> AnyElement {
    let spec = spec.clone();
    div()
        .id(ElementId::Name(
            format!("approval-{}-{label}", spec.call_id).into(),
        ))
        .px(px(9.))
        .py(px(6.))
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(fill)
        .text_color(text)
        .text_xs()
        .cursor_pointer()
        .child(label)
        .when_some(app, |this, app| {
            this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                app.update(cx, |state, cx| {
                    state.answer_approval(spec.clone(), resolution, cx);
                });
            })
        })
        .into_any_element()
}

fn render_bar_chart(chart: &BarChartSpec, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let max = chart
        .bars
        .iter()
        .map(|bar| bar.value.abs())
        .fold(0.0_f32, f32::max)
        .max(1.0);
    let bars = chart.bars.clone();
    let columns = bars.into_iter().enumerate().map(|(i, bar)| {
        let frac = (bar.value.abs() / max).clamp(0.08, 1.0);
        let fill = if i % 2 == 0 {
            theme.primary
        } else {
            theme.primary.opacity(0.55)
        };
        v_flex()
            .items_center()
            .gap(px(6.))
            .child(div().w(px(28.)).h(px(120. * frac)).rounded(px(6.)).bg(fill))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(bar.label.clone()),
            )
            .into_any_element()
    });
    v_flex()
        .w_full()
        .gap(px(8.))
        .when_some(chart.title.clone(), |this, title| {
            this.child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
        })
        .child(
            h_flex()
                .w_full()
                .items_end()
                .justify_around()
                .h(px(148.))
                .gap(px(10.))
                .children(columns),
        )
        .into_any_element()
}

fn render_form(
    form: &FormSpec,
    message_id: &str,
    app: Option<Entity<AppState>>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let picks = app
        .as_ref()
        .map(|entity| entity.read(cx).form_picks.clone());
    let mut body = v_flex().w_full().gap(px(10.));
    if let Some(title) = &form.title {
        body = body.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.clone()),
        );
    }
    if let Some(prompt) = &form.prompt {
        body = body.child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(prompt.clone()),
        );
    }
    for field in &form.fields {
        let selected = picks
            .as_ref()
            .and_then(|all| all.get(message_id))
            .and_then(|fields| fields.get(&field.id))
            .cloned();
        let mut chips = h_flex().gap(px(6.)).flex_wrap();
        for option in &field.options {
            let on = selected.as_deref() == Some(option.as_str());
            let message_id = message_id.to_string();
            let field_id = field.id.clone();
            let value = option.clone();
            let app = app.clone();
            chips = chips.child(
                div()
                    .id(ElementId::Name(
                        format!("form-{message_id}-{field_id}-{option}").into(),
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
                    .child(option.clone())
                    .when_some(app, |this, app| {
                        this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            app.update(cx, |state, cx| {
                                state.pick_form_option(
                                    message_id.clone(),
                                    field_id.clone(),
                                    value.clone(),
                                    cx,
                                );
                            });
                        })
                    }),
            );
        }
        body = body.child(
            v_flex()
                .gap(px(6.))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(field.label.clone()),
                )
                .child(chips),
        );
    }
    let spec = form.clone();
    let message_id = message_id.to_string();
    let app_submit = app.clone();
    body.child(
        div()
            .id(ElementId::Name(format!("form-submit-{message_id}").into()))
            .px(px(12.))
            .py(px(6.))
            .rounded(px(8.))
            .bg(theme.primary)
            .text_color(theme.primary_foreground)
            .text_xs()
            .cursor_pointer()
            .child(form.submit.clone())
            .when_some(app_submit, |this, app| {
                this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    app.update(cx, |state, cx| {
                        state.submit_form(message_id.clone(), spec.clone(), cx);
                    });
                })
            }),
    )
    .into_any_element()
}
