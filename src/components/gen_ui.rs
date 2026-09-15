//! Native AG-UI widgets. Not markdown, not KaTeX.

use crate::opengrok::{BarChartSpec, FormSpec, UiSpec};
use crate::state::AppState;
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
