//! The right pane: the skill that was picked — what it is for, the instructions themselves,
//! the files that go with them, and Delete.
//!
//! The pane is here only while a skill is open. The list is the page; this stands beside it.

use super::{NEVER_UPDATED, NOTHING_WRITTEN_YET, chip, short_relative_time, skill_icon, use_line};
use crate::chrome::TITLE_BAR_H;
use crate::opengrok::SkillDetail;
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, Icon, IconName, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub(super) fn render(
    id: &str,
    detail: Option<&SkillDetail>,
    error: Option<String>,
    switching: bool,
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
        // The row is on screen and its prose is not here yet — or it is not coming, and the
        // pane says which. A pane that waits forever at a request that was refused is a pane
        // nobody can tell from a slow one.
        return match error {
            Some(error) => pane
                .child(close_row(muted, app))
                .child(
                    div()
                        .id("settings-skill-error")
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error),
                )
                .into_any_element(),
            None => pane
                .child(close_row(muted, app))
                .child(
                    div()
                        .id("settings-skill-loading")
                        .text_sm()
                        .text_color(muted)
                        .child("Loading…"),
                )
                .into_any_element(),
        };
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
        NEVER_UPDATED.to_string()
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
                                // The switch, and beside it what its position MEANS: how to use
                                // the skill, or that nothing can — see [`use_line`]. The two
                                // belong together, because the line states the thing the switch
                                // is the answer to.
                                .child(
                                    Switch::new(SharedString::from(format!(
                                        "settings-skill-enabled-{skill_id}"
                                    )))
                                    .checked(skill.enabled)
                                    // Dead while the server is being told: pressed twice, two
                                    // answers about one flag are in the air and the slower one
                                    // is the one that stands.
                                    .disabled(switching)
                                    .tooltip(if skill.enabled {
                                        "Switch off: nothing will be able to run it"
                                    } else {
                                        "Switch on: your bot will be able to read it"
                                    })
                                    .on_click({
                                        let app = app.clone();
                                        let id = skill.id.clone();
                                        move |checked, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.set_skill_enabled(id.clone(), *checked, cx);
                                            });
                                        }
                                    }),
                                )
                                // On the slash itself, see the note beside the same promise on
                                // the list: it is wired by the branch this one stacks under,
                                // which lands before either reaches a person.
                                .children(
                                    use_line(&skill.name, skill.enabled, skill.approved_at_ms).map(
                                        |line| {
                                            div()
                                                .id(SharedString::from(format!(
                                                    "settings-skill-use-{skill_id}"
                                                )))
                                                .text_xs()
                                                .text_color(if skill.enabled {
                                                    muted
                                                } else {
                                                    theme.warning
                                                })
                                                .child(line)
                                        },
                                    ),
                                )
                                .children(label.map(|(word, tone)| chip(word, tone, theme))),
                        ),
                ),
        )
        // A change the server refused, under the control it was refused about. The switch has
        // already gone back to where it was, so without this the pane would show a switch that
        // sprang back for no stated reason.
        .children(error.map(|why| {
            div()
                .id("settings-skill-error")
                .text_xs()
                .text_color(theme.danger)
                .child(why)
        }))
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
                .child(
                    kv_row("Updated", updated, theme).id(SharedString::from(format!(
                        "settings-skill-updated-{skill_id}"
                    ))),
                )
                .child(divider(theme))
                // NOT YET: these are names, and reading them costs the whole bundle. The
                // detail route answers with every file's bytes base64 — there is no per-file
                // read route and no listing that stops at the names — so opening a skill with a
                // 256 KB bundle downloads 256 KB to print two lines. A `?files=names` on the
                // server, or a separate listing, is what would fix it.
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
                        // NOT YET: this is the Markdown as it is kept, drawn as one run of
                        // text. Its headings and lists are not rendered — nothing here renders
                        // Markdown — and its line breaks fall where the column ends rather than
                        // where they were typed.
                        .map(|this| {
                            if detail.body.trim().is_empty() {
                                return this.text_color(muted).child(NOTHING_WRITTEN_YET);
                            }
                            this.child(detail.body.clone())
                        }),
                ),
        )
        .child(
            h_flex().child(
                // Its own id, not the row menu's: two elements answering to one name is one
                // name a driver cannot use to say which of them it meant.
                Button::new(SharedString::from(format!(
                    "settings-skill-detail-delete-{skill_id}"
                )))
                .label("Delete")
                .danger()
                .small()
                .on_click(move |_, _, cx| {
                    app.update(cx, |state, cx| state.ask_skill_delete(skill_id.clone(), cx));
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
