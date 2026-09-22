//! The right pane: the skill that was picked — what it is for, the instructions themselves,
//! the files that go with them, and Delete.
//!
//! The pane is here only while a skill is open. The list is the page; this stands beside it.

use super::{chip, short_relative_time, skill_icon};
use crate::chrome::TITLE_BAR_H;
use crate::opengrok::SkillDetail;
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, IconName, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub(super) fn render(
    id: &str,
    detail: Option<&SkillDetail>,
    theme: &Theme,
    app: Entity<AppState>,
) -> AnyElement {
    let muted = theme.muted_foreground;
    let pane = v_flex()
        .id(SharedString::from(format!("settings-skill-detail-{id}")))
        .w(px(400.))
        .flex_shrink_0()
        .h_full()
        .overflow_y_scroll()
        .border_l_1()
        .border_color(theme.border)
        .pt(px(TITLE_BAR_H))
        .px(px(20.))
        .pb(px(24.))
        .gap(px(16.));
    let Some(detail) = detail else {
        // The row is on screen and its prose is not here yet. Saying so keeps the pane from
        // reading as a skill with nothing in it.
        return pane
            .child(close_row(muted, app))
            .child(
                div()
                    .id("settings-skill-loading")
                    .text_sm()
                    .text_color(muted)
                    .child("Loading…"),
            )
            .into_any_element();
    };
    let skill = &detail.skill;
    let skill_id = skill.id.clone();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let name = if skill.name.trim().is_empty() {
        "Untitled skill".to_string()
    } else {
        skill.name.clone()
    };
    let label = if skill.draft {
        Some(("Draft", theme.warning))
    } else {
        skill.source.chip().map(|word| (word, muted))
    };
    let updated = if skill.updated_at_ms > 0 {
        short_relative_time(skill.updated_at_ms, now_ms)
    } else {
        "—".to_string()
    };
    let version = if detail.version == 0 {
        "None yet".to_string()
    } else {
        format!("v{}", detail.version)
    };
    pane.child(close_row(muted, app.clone()))
        .child(
            h_flex()
                .gap(px(12.))
                .items_center()
                .child(skill_icon(44., theme))
                .child(
                    v_flex()
                        .min_w(px(0.))
                        .gap(px(3.))
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(name),
                        )
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .items_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(format!("Type /{} to use it", skill.name)),
                                )
                                .children(label.map(|(word, tone)| chip(word, tone, theme))),
                        ),
                ),
        )
        .when(!skill.description.trim().is_empty(), |this| {
            this.child(
                div()
                    .id(SharedString::from(format!(
                        "settings-skill-description-{skill_id}"
                    )))
                    .text_sm()
                    .child(skill.description.clone()),
            )
        })
        .child(
            card(theme)
                .child(kv_row("Version", version, theme))
                .child(divider(theme))
                .child(kv_row("Updated", updated, theme))
                .child(divider(theme))
                .child(kv_row(
                    "Files",
                    if detail.files.is_empty() {
                        "None".to_string()
                    } else {
                        detail
                            .files
                            .iter()
                            .map(|file| file.path.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    },
                    theme,
                )),
        )
        .child(
            v_flex()
                .gap(px(6.))
                .child(div().text_xs().text_color(muted).child("Instructions"))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "settings-skill-body-{skill_id}"
                        )))
                        .w_full()
                        .rounded(px(10.))
                        .border_1()
                        .border_color(theme.border)
                        .px(px(12.))
                        .py(px(10.))
                        .text_sm()
                        // The prose is shown as it is kept, newlines and all, because it is
                        // Markdown a person wrote and reads back to check.
                        .whitespace_normal()
                        .map(|this| {
                            if detail.body.trim().is_empty() {
                                return this.text_color(muted).child(
                                    "Nothing written yet. This skill is a name with no \
                                     instructions under it.",
                                );
                            }
                            this.child(detail.body.clone())
                        }),
                ),
        )
        .child(
            h_flex().child(
                Button::new(SharedString::from(format!(
                    "settings-skill-delete-{skill_id}"
                )))
                .label("Delete")
                .danger()
                .small()
                .on_click(move |_, _, cx| {
                    app.update(cx, |state, cx| state.delete_skill(skill_id.clone(), cx));
                }),
            ),
        )
        .into_any_element()
}

/// The way back to the whole-width list.
fn close_row(muted: Hsla, app: Entity<AppState>) -> impl IntoElement {
    h_flex().w_full().justify_end().child(
        Button::new("settings-skill-close")
            .icon(Icon::new(IconName::Close).size(px(14.)))
            .ghost()
            .xsmall()
            .text_color(muted)
            .tooltip("Close")
            .on_click(move |_, _, cx| {
                app.update(cx, |state, cx| state.close_skill(cx));
            }),
    )
}

fn card(theme: &Theme) -> Div {
    v_flex()
        .w_full()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
}

fn divider(theme: &Theme) -> Div {
    div().h(px(1.)).bg(theme.border)
}

/// One line of the card: the key in the margin, the value beside it.
fn kv_row(key: &'static str, value: impl Into<SharedString>, theme: &Theme) -> Div {
    h_flex()
        .w_full()
        .px(px(14.))
        .py(px(9.))
        .gap(px(12.))
        .items_start()
        .child(
            div()
                .w(px(76.))
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(key),
        )
        .child(div().flex_1().min_w(px(0.)).text_sm().child(value.into()))
}
