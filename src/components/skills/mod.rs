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
                // ANY switch, not this skill's: one is allowed in flight at a time, and a
                // control that stays live while the app will refuse it is a control that moves
                // under the finger and reports nothing.
                switching: state.skill_enabling.is_some(),
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
            switching,
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
                // Saving with no sheet up is an upload: the sheet's own Save says so while it
                // is the sheet that is saving.
                saving && !add_open,
                // The list's own slot: a refresh, a picker, an upload, a delete. What the sheet
                // and the open skill were refused is shown where each of them is.
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
                    switching,
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
    /// A switch — any skill's — is with the server. Every switch is dead while one is.
    switching: bool,
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

/// What the pane says where the prose would be, for a skill that has a name and nothing under
/// it. Shared with the driver's tree, so a driver reads the sentence the person reads rather
/// than inferring a draft from an empty string.
pub(crate) const NOTHING_WRITTEN_YET: &str =
    "Nothing written yet. This skill is a name with no instructions under it.";

/// What the pane says for a row the server sent no timestamp on. One word in both places: the
/// tree said "never" while the screen said "—", which is two answers to one question.
pub(crate) const NEVER_UPDATED: &str = "Never";

/// The line under an open skill's name: how to use it, or why it cannot be used.
///
/// A SWITCHED-OFF SKILL HAS NO SLASH. The server refuses one even to the person who owns it, so
/// "Type /name to use it" over a skill that is off is an instruction that does not work — and
/// off is where every skill a model wrote from a recording starts, because nobody has read it
/// yet. Reading it is the thing to do about that, and this pane is where it is read.
///
/// OFF IS TWO DIFFERENT SITUATIONS, and `approved_at_ms` is what tells them apart: never
/// stamped is a lesson nobody has read, stamped is one somebody read and then switched off.
/// Saying "until somebody reads it" about the second would be asking a person to do again the
/// thing they did just before they switched it off.
///
/// A skill with no name has no slash either, and nothing to say about one: the server will not
/// make one nameless, but a row from somewhere else still can be.
pub(crate) fn use_line(name: &str, enabled: bool, approved_at_ms: Option<i64>) -> Option<String> {
    if !enabled {
        return Some(if waiting_to_be_read(enabled, approved_at_ms) {
            "Switched off — your bot cannot use it until somebody reads it".to_string()
        } else {
            "Switched off — somebody read this one and switched it off".to_string()
        });
    }
    (!name.trim().is_empty()).then(|| format!("Type /{name} to use it"))
}

/// Whether this skill is off and has never been switched on by anybody — the state the pane
/// above says in words, and the one a lesson a model wrote is born in.
///
/// SHARED WITH THE DRIVER, which reads it as a state on the switch. The two used to work it out
/// separately and disagreed: the screen only tells the two kinds of off apart while a skill IS
/// off, and the tree called every skill on a server that sends no stamp unread, including the
/// ones that were switched on.
pub(crate) fn waiting_to_be_read(enabled: bool, approved_at_ms: Option<i64>) -> bool {
    !enabled && approved_at_ms.is_none()
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
    use super::{SkillSummary, matching_skills, short_relative_time, use_line};

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
            approved_at_ms: None,
        }
    }

    /// A skill a model wrote from a recording arrives switched off, and the pane somebody is
    /// sent to in order to read it must not tell them to type a slash that the server refuses.
    /// Off is not a detail of the row: it is the reason they were sent there.
    #[test]
    fn a_switched_off_skill_says_so_instead_of_offering_a_slash() {
        let unread = use_line("invoice-lookup", false, None).expect("off is always worth saying");
        assert!(
            unread.starts_with("Switched off"),
            "the first words, because it is the first thing to know: {unread:?}"
        );
        assert!(
            unread.contains("reads it"),
            "and what makes it not off, which is somebody reading it: {unread:?}"
        );
        assert!(
            !unread.contains('/'),
            "no slash is offered for a skill that has none: {unread:?}"
        );
        assert_eq!(
            use_line("invoice-lookup", true, None).as_deref(),
            Some("Type /invoice-lookup to use it"),
            "a skill that is on is invoked by its name"
        );
        assert_eq!(
            use_line("  ", true, None),
            None,
            "a nameless skill has no slash to offer and nothing to say about one"
        );
        assert!(
            use_line("", false, None).is_some(),
            "but a nameless skill that is off is still off"
        );
    }

    /// Off twice over: a lesson nobody has read, and one somebody read and switched off. The
    /// server stamps the reading and never unstamps it, which is the only way to tell them
    /// apart — and telling somebody to read a skill they switched off after reading it is the
    /// app arguing with them about something they already decided.
    #[test]
    fn a_skill_read_and_switched_off_is_not_a_skill_waiting_to_be_read() {
        let unread = use_line("invoice-lookup", false, None).expect("off says so");
        let read = use_line("invoice-lookup", false, Some(1_758_000_000_000)).expect("off says so");
        assert_ne!(unread, read);
        assert!(read.starts_with("Switched off"), "{read:?}");
        assert!(
            read.contains("read this one") && !read.contains("until"),
            "it has been read, so nothing is waiting on a reading: {read:?}"
        );
        assert_eq!(
            use_line("invoice-lookup", true, Some(1_758_000_000_000)).as_deref(),
            Some("Type /invoice-lookup to use it"),
            "and a skill that is on is a skill that is on, stamped or not"
        );
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
