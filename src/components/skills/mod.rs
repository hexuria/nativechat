//! Settings → Skills: the library of prose a bot reads before it works.
//!
//! A SKILL is written instructions — a `SKILL.md` with a name and a description in its
//! frontmatter — which the model reads before it starts, and which a person invokes by typing
//! `/name`. A RECIPE is a taped replay of clicks. They are two different kinds of thing that
//! happen to share a slash, and nothing on this page calls one by the other's name.
//!
//! The page is two panes, as Settings → Logins is: the library at the left, and the skill that
//! was picked beside it. There is no ticker here — a skill counts nothing down.

mod add_sheet;
mod detail;
mod list;

use crate::opengrok::SkillSummary;
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub use add_sheet::AddSheetInputs;
pub(crate) use list::{NOT_YET_RECORDING, NOT_YET_WITH_BOT, empty_line};

pub struct SkillsPage {
    state: Entity<AppState>,
    search: Entity<InputState>,
    /// The Create sheet's fields, made the first time it opens (an input needs a window).
    add: Option<AddSheetInputs>,
}

impl SkillsPage {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search skills"));
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        // What is typed in the field is what the list filters by; the state keeps the copy so
        // the driver can write it too.
        cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                this.state
                    .update(cx, |state, cx| state.set_skills_query(query, cx));
            }
        })
        .detach();
        Self {
            state,
            search,
            add: None,
        }
    }

    /// The field follows the state, so a search the driver wrote lands in it.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (query, add_open, taken) = {
            let state = self.state.read(cx);
            (
                state.skills_query.clone(),
                state.skill_add_open,
                state.skill_add_taken,
            )
        };
        if self.search.read(cx).value().as_ref() != query.as_str() {
            self.search
                .update(cx, |input, cx| input.set_value(query, window, cx));
        }
        if add_open && self.add.is_none() {
            self.add = Some(AddSheetInputs::new(window, cx));
        }
        // The server took the last create, so what is in the fields is kept somewhere and the
        // next sheet opens clean. Nothing else empties them: a refusal leaves the words where
        // they were typed, and so does a click that landed wide of the sheet.
        if taken {
            if let Some(add) = &self.add {
                add.clear(window, cx);
            }
            self.state
                .update(cx, |state, _| state.skill_add_fields_cleared());
        }
    }
}

impl Render for SkillsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        let theme = cx.theme().clone();
        let app = self.state.clone();
        let page = {
            let state = self.state.read(cx);
            PageState {
                skills: state.skills.clone(),
                query: state.skills_query.clone(),
                counts: state.skills_counts,
                scope: state.skills_scope,
                loading: state.skills_loading,
                error: state.skills_error.clone(),
                open: state.skill_open.clone(),
                open_id: state.skill_open_id.clone(),
                open_error: state.skill_error.clone(),
                add_open: state.skill_add_open,
                add_error: state.skill_add_error.clone(),
                saving: state.skill_saving,
                delete_confirm: state.skill_delete_prompt(),
            }
        };
        let PageState {
            skills,
            query,
            counts,
            scope,
            loading,
            error,
            open,
            open_id,
            open_error,
            add_open,
            add_error,
            saving,
            delete_confirm,
        } = page;
        let rows = matching_skills(&skills, &query);
        let sheet = add_open.then(|| self.add.clone()).flatten();

        div()
            .id("settings-skills")
            .flex()
            .flex_row()
            .size_full()
            .relative()
            .child(list::render(
                &self.search,
                &rows,
                skills.len(),
                counts,
                scope,
                loading,
                // The list's own slot: a refresh, a picker, a delete. What the sheet and the
                // open skill were refused is shown where each of them is.
                error,
                open_id.as_deref(),
                &theme,
                app.clone(),
            ))
            .when_some(open_id, |this, id| {
                this.child(detail::render(
                    &id,
                    open.as_ref(),
                    open_error,
                    &theme,
                    app.clone(),
                ))
            })
            .when_some(sheet, |this, inputs| {
                this.child(add_sheet::render(
                    inputs,
                    add_error,
                    saving,
                    app.clone(),
                    &theme,
                ))
            })
            .when_some(delete_confirm, |this, name| {
                this.child(confirm_delete(name, app, &theme))
            })
    }
}

/// Everything one paint of the page reads off the state, named, because a tuple of thirteen is
/// a tuple nobody can add a field to without moving twelve others.
struct PageState {
    skills: Vec<SkillSummary>,
    query: String,
    counts: crate::state::SkillCounts,
    scope: crate::state::SkillScope,
    loading: bool,
    error: Option<String>,
    open: Option<crate::opengrok::SkillDetail>,
    open_id: Option<String>,
    open_error: Option<String>,
    add_open: bool,
    add_error: Option<String>,
    saving: bool,
    delete_confirm: Option<String>,
}

/// Delete asks first. The dialog names the skill, because the menu it was chosen from opens
/// under the pointer and Delete sits one row under Open.
fn confirm_delete(name: String, app: Entity<AppState>, theme: &Theme) -> impl IntoElement {
    div()
        .id("settings-skill-delete-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::black().opacity(0.32))
        .on_mouse_down(MouseButton::Left, {
            let app = app.clone();
            move |_, _, cx| {
                app.update(cx, |state, cx| state.close_skill_delete_confirm(cx));
            }
        })
        .child(
            v_flex()
                .id("settings-skill-delete-sheet")
                .w(px(380.))
                .bg(theme.popover)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.border)
                .rounded(px(14.))
                .shadow_lg()
                .px(px(20.))
                .py(px(18.))
                .gap(px(8.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("Delete {name}?")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Your bot stops reading it. Runs that used it keep what they read."),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_end()
                        .gap(px(8.))
                        .pt(px(6.))
                        .child(
                            Button::new("settings-skill-delete-cancel")
                                .label("Cancel")
                                .on_click({
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.close_skill_delete_confirm(cx)
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("settings-skill-delete-confirm")
                                .label("Delete")
                                .danger()
                                .on_click(move |_, _, cx| {
                                    app.update(cx, |state, cx| state.confirm_skill_delete(cx));
                                }),
                        ),
                ),
        )
}

/// The rows the search leaves.
///
/// Both the name and the description are searched, because somebody looking for "the expenses
/// one" may remember either — and a skill's description is the sentence the model is given to
/// decide by, so it is the likeliest thing to be remembered.
pub(crate) fn matching_skills<'a>(
    skills: &'a [SkillSummary],
    query: &str,
) -> Vec<&'a SkillSummary> {
    skills
        .iter()
        .filter(|skill| skill_matches(&skill.name, &skill.description, query))
        .collect()
}

/// Whether one skill is left by the search. The rule itself, apart from the rows, because the
/// page filters the server's summaries and the driver's tree filters its own snapshots of them
/// — and a tree that listed a different set of rows than the screen would be worse than none.
pub(crate) fn skill_matches(name: &str, description: &str, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty()
        || name.to_lowercase().contains(&needle)
        || description.to_lowercase().contains(&needle)
}

/// "6d ago", for the one-line column at the end of a row.
///
/// Shorter than [`crate::site_login::relative_time`] on purpose: this sits at the end of a row
/// that already holds a name, a chip and a description, and "6 days ago" there is three words
/// where the question was only how stale the thing is.
pub(crate) fn short_relative_time(at_ms: i64, now_ms: i64) -> String {
    let secs = (now_ms - at_ms).max(0) / 1000;
    if secs < 60 {
        return "Just now".to_string();
    }
    let (count, unit) = if secs < 3_600 {
        (secs / 60, "m")
    } else if secs < 86_400 {
        (secs / 3_600, "h")
    } else if secs < 604_800 {
        (secs / 86_400, "d")
    } else if secs < 2_592_000 {
        (secs / 604_800, "w")
    } else if secs < 31_536_000 {
        (secs / 2_592_000, "mo")
    } else {
        (secs / 31_536_000, "y")
    };
    format!("{count}{unit} ago")
}

/// The mark a skill wears wherever it is listed: an open book, which is what a skill is.
pub(crate) fn skill_icon(size: f32, theme: &Theme) -> AnyElement {
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(size * 0.26))
        .bg(theme.muted)
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::default()
                .path("icons/study.svg")
                .size(px(size * 0.56))
                .text_color(theme.muted_foreground),
        )
        .into_any_element()
}

/// One word about a row, in a rounded pill: where its prose came from, or that it has none yet.
pub(crate) fn chip(label: impl Into<SharedString>, tone: Hsla, theme: &Theme) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .px(px(7.))
        .py(px(1.))
        .rounded_full()
        .bg(theme.muted)
        .text_xs()
        .text_color(tone)
        .child(label.into())
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob: `use super::*` would pull GPUI's `test` attribute in over the
    // one the test harness wants.
    use super::{SkillSummary, matching_skills, short_relative_time};

    fn skill(id: &str, name: &str, description: &str) -> SkillSummary {
        SkillSummary {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            source: crate::opengrok::SkillSource::Authored,
            updated_at_ms: 0,
            version_count: 1,
            draft: false,
            enabled: true,
        }
    }

    /// The search reads the description as well as the name: the name is a slug somebody typed
    /// once, and what they remember a month later is what the thing was for.
    #[test]
    fn a_search_finds_a_skill_by_what_it_is_for() {
        let skills = vec![
            skill("skl_1", "expense-report", "How we file expenses"),
            skill(
                "skl_2",
                "new-hire",
                "First week for somebody who just joined",
            ),
        ];
        let names = |query: &str| -> Vec<String> {
            matching_skills(&skills, query)
                .iter()
                .map(|skill| skill.name.clone())
                .collect()
        };
        assert_eq!(names(""), vec!["expense-report", "new-hire"]);
        assert_eq!(names("expense"), vec!["expense-report"]);
        assert_eq!(
            names("JOINED"),
            vec!["new-hire"],
            "case is not the question"
        );
        assert_eq!(names("  hire "), vec!["new-hire"]);
        assert!(names("payroll").is_empty());
    }

    /// The column is one line wide, so the unit is one letter and the number is whole.
    #[test]
    fn a_row_says_how_stale_it_is_in_two_characters() {
        let now = 1_700_000_000_000i64;
        let ago = |secs: i64| short_relative_time(now - secs * 1000, now);
        assert_eq!(ago(3), "Just now");
        assert_eq!(ago(90), "1m ago");
        assert_eq!(ago(7_200), "2h ago");
        assert_eq!(ago(6 * 86_400), "6d ago");
        assert_eq!(ago(20 * 86_400), "2w ago");
        assert_eq!(ago(90 * 86_400), "3mo ago");
        assert_eq!(ago(800 * 86_400), "2y ago");
        assert_eq!(
            short_relative_time(now + 5_000, now),
            "Just now",
            "a clock that is ahead of ours is not a skill from the future"
        );
    }
}
