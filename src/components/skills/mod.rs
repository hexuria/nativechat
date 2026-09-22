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
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Icon, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub use add_sheet::AddSheetInputs;

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
        let (query, add_open) = {
            let state = self.state.read(cx);
            (state.skills_query.clone(), state.skill_add_open)
        };
        if self.search.read(cx).value().as_ref() != query.as_str() {
            self.search
                .update(cx, |input, cx| input.set_value(query, window, cx));
        }
        if add_open && self.add.is_none() {
            self.add = Some(AddSheetInputs::new(window, cx));
        }
    }
}

impl Render for SkillsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        let theme = cx.theme().clone();
        let app = self.state.clone();
        let (skills, query, counts, scope, loading, error, open, open_id, add_open) = {
            let state = self.state.read(cx);
            (
                state.skills.clone(),
                state.skills_query.clone(),
                state.skills_counts,
                state.skills_scope,
                state.skills_loading,
                state.skills_error.clone(),
                state.skill_open.clone(),
                state.skill_open_id.clone(),
                state.skill_add_open,
            )
        };
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
                // While the Create sheet is up it shows the error itself; the list does not
                // repeat it behind the dimmed page.
                error.clone().filter(|_| !add_open),
                open_id.as_deref(),
                &theme,
                app.clone(),
            ))
            .when_some(open_id, |this, id| {
                this.child(detail::render(&id, open.as_ref(), &theme, app.clone()))
            })
            .when_some(sheet, |this, inputs| {
                this.child(add_sheet::render(inputs, error, app, &theme))
            })
    }
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
    let needle = query.trim().to_lowercase();
    skills
        .iter()
        .filter(|skill| {
            needle.is_empty()
                || skill.name.to_lowercase().contains(&needle)
                || skill.description.to_lowercase().contains(&needle)
        })
        .collect()
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
