//! The left pane: the title row with Add beside it, the Yours / Discover toggle, the search
//! field, and the rows the search leaves.

use super::{chip, short_relative_time, skill_icon};
use crate::chrome::TITLE_BAR_H;
use crate::components::fields::field_input;
use crate::opengrok::SkillSummary;
use crate::state::{AppState, SkillCounts, SkillScope};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    search: &Entity<InputState>,
    rows: &[&SkillSummary],
    listed: usize,
    counts: SkillCounts,
    scope: SkillScope,
    loading: bool,
    uploading: bool,
    error: Option<String>,
    open: Option<&str>,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let refused = error.is_some();
    v_flex()
        .id("settings-skills-list-pane")
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .pt(px(TITLE_BAR_H))
        .px(px(28.))
        .pb(px(20.))
        .gap(px(12.))
        .child(
            h_flex()
                .id("settings-skills-title-row")
                .w_full()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Skills"),
                )
                .child(add_menu(app.clone())),
        )
        .child(
            // The slash is not wired on this branch: typing `/name` reaches for a recipe and
            // finds no skill. It is wired by the branch this one stacks under, which lands
            // before either reaches a person — so the sentence is true by the time it is read,
            // and watering it down now would leave a half-sentence to put back later.
            div()
                .text_sm()
                .text_color(muted)
                .child("Written instructions your bot reads before it works. Type / to use one."),
        )
        .child(
            h_flex().id("settings-skills-scopes").gap(px(6.)).children(
                SkillScope::ALL
                    .into_iter()
                    .map(|side| scope_chip(side, scope, counts, app.clone())),
            ),
        )
        .child(
            div()
                .id("settings-skills-search")
                .w_full()
                .child(field_input(search).cleanable(true)),
        )
        .when_some(error, |this, error| {
            this.child(
                div()
                    .id("settings-skills-error")
                    .w_full()
                    .text_xs()
                    .text_color(theme.danger)
                    .child(error),
            )
        })
        // An upload has no sheet to say it is working in, and reading a folder and sending it
        // takes long enough that a page which said nothing would read as a click that missed.
        .when(uploading, |this| {
            this.child(
                div()
                    .id("settings-skills-saving")
                    .w_full()
                    .text_xs()
                    .text_color(muted)
                    .child("Uploading…"),
            )
        })
        .child(
            v_flex()
                .id("settings-skills-rows")
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .overflow_y_scroll()
                .gap(px(2.))
                .map(|this| {
                    if rows.is_empty() {
                        return this.children(empty_line(loading, listed, scope, refused).map(
                            |line| {
                                div()
                                    .id("settings-skills-empty")
                                    .px(px(8.))
                                    .py(px(10.))
                                    .text_sm()
                                    .text_color(muted)
                                    .child(line)
                            },
                        ));
                    }
                    this.children(rows.iter().map(|skill| {
                        row(skill, open == Some(&skill.id), now_ms, theme, app.clone())
                    }))
                }),
        )
}

/// What the list says when it has nothing to show: still fetching, nothing here at all, or a
/// search that matched none of what is here. Three different facts, never the same sentence.
///
/// `None` when the library was refused — signed out, or a server that would not answer. There
/// is a red line on the page saying so, and "No skills yet" under it would be the page telling
/// somebody their skills are gone.
pub(crate) fn empty_line(
    loading: bool,
    listed: usize,
    scope: SkillScope,
    refused: bool,
) -> Option<&'static str> {
    if refused {
        return None;
    }
    if loading && listed == 0 {
        return Some("Loading…");
    }
    if listed > 0 {
        return Some("No skills match.");
    }
    Some(match scope {
        SkillScope::Yours => "No skills yet. Add one to teach your bot how something is done.",
        SkillScope::Discover => "Nobody in your org has shared a skill yet.",
    })
}

/// One side of the toggle, with how many skills are on it.
fn scope_chip(
    side: SkillScope,
    current: SkillScope,
    counts: SkillCounts,
    app: Entity<AppState>,
) -> Button {
    let button = Button::new(side.element_id())
        .small()
        .label(format!("{} {}", side.label(), counts.of(side)))
        .on_click(move |_, _, cx| {
            app.update(cx, |state, cx| state.set_skills_scope(side, cx));
        });
    if side == current {
        button.primary()
    } else {
        button
    }
}

/// Add: the four ways a skill gets made, two of which are not built.
///
/// The two that are not say what would have to be built, in the menu, where the wish occurs. A
/// dead "coming soon" would leave somebody waiting for something nobody has started.
fn add_menu(app: Entity<AppState>) -> impl IntoElement {
    Button::new("settings-skill-add")
        .small()
        .primary()
        .label("Add")
        .icon(Icon::new(IconName::Plus).size(px(14.)))
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            let upload = app.clone();
            let write = app.clone();
            menu.item(
                PopupMenuItem::element(|_, _| {
                    div().id("settings-skill-upload").child("Upload skill")
                })
                .on_click(move |_, _, cx| {
                    upload.update(cx, |state, cx| state.pick_skill_upload(cx));
                }),
            )
            .item(
                PopupMenuItem::element(|_, _| {
                    div().id("settings-skill-write").child("Create a skill")
                })
                .on_click(move |_, _, cx| {
                    write.update(cx, |state, cx| state.open_skill_add(cx));
                }),
            )
            .item(
                PopupMenuItem::element(|_, cx| {
                    not_yet(
                        "settings-skill-with-bot",
                        "Create with your bot",
                        NOT_YET_WITH_BOT,
                        cx.theme().muted_foreground,
                    )
                })
                .disabled(true),
            )
            .item(
                PopupMenuItem::element(|_, cx| {
                    not_yet(
                        "settings-skill-record",
                        "Record your screen",
                        NOT_YET_RECORDING,
                        cx.theme().muted_foreground,
                    )
                })
                .disabled(true),
            )
        })
}

/// Why a skill cannot be written by a bot yet, in the words the menu shows.
pub(crate) const NOT_YET_WITH_BOT: &str = "Not yet — nothing here turns a conversation into a skill. It needs a turn that drafts the \
     instructions and hands them back for you to keep or throw away.";

/// Why this page cannot record a screen, and where the thing it is reaching for now lives.
///
/// The half of this that used to be missing is built: a task taped on a bot's screen is read
/// into prose by a model and kept as a skill. What is still missing is this Mac — nothing here
/// records the screen somebody is sitting in front of — so the sentence stops asking for a model
/// that exists and names the way that works instead.
pub(crate) const NOT_YET_RECORDING: &str = "Not yet — nothing here records this Mac. A task taped on your bot's screen can become a \
     skill: open the screen, press Teach a task, and choose Skill when you stop.";

/// A row of the Add menu that cannot be taken: what it would be, and what would have to be
/// built for it to work.
fn not_yet(id: &'static str, title: &'static str, why: &'static str, muted: Hsla) -> AnyElement {
    v_flex()
        .id(id)
        .max_w(px(280.))
        .gap(px(2.))
        .child(div().child(title))
        .child(div().text_xs().text_color(muted).child(why))
        .into_any_element()
}

/// One skill: its mark, its name and where it came from, what it is for, how stale it is, and
/// the menu that opens or drops it.
fn row(
    skill: &SkillSummary,
    open: bool,
    now_ms: i64,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let id = skill.id.clone();
    // A draft is a skill with no instructions in it yet, so that is what its chip says: where
    // it came from is not the useful fact about a skill that cannot be used.
    let label = if skill.draft {
        Some(("Draft", theme.warning))
    } else {
        skill.source.chip().map(|word| (word, muted))
    };
    let name = if skill.name.trim().is_empty() {
        "Untitled skill".to_string()
    } else {
        skill.name.clone()
    };
    let description = skill.description.trim().to_string();
    h_flex()
        .id(SharedString::from(format!("settings-skill-row-{id}")))
        .w_full()
        .items_center()
        .gap(px(10.))
        .px(px(8.))
        .py(px(9.))
        .rounded(px(10.))
        .cursor_pointer()
        .when(open, |this| this.bg(theme.list_active))
        .when(!open, |this| this.hover(|s| s.bg(theme.list_hover)))
        .on_mouse_down(MouseButton::Left, {
            let app = app.clone();
            let id = id.clone();
            move |_, _, cx| {
                app.update(cx, |state, cx| state.open_skill(id.clone(), cx));
            }
        })
        .child(skill_icon(30., theme))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(1.))
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .min_w(px(0.))
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .child(name),
                        )
                        .children(label.map(|(word, tone)| chip(word, tone, theme))),
                )
                .when(!description.is_empty(), |this| {
                    this.child(
                        div()
                            .w_full()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(description),
                    )
                }),
        )
        .when(skill.updated_at_ms > 0, |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(short_relative_time(skill.updated_at_ms, now_ms)),
            )
        })
        // The menu sits inside the row, and the whole row is the way into the skill. Without
        // this, clicking "…" would open the skill behind the menu it just opened.
        .child(
            div()
                // Not `settings-skill-menu-…`: that is the button inside it, and two ids one
                // prefix apart are two ids a driver's error message cannot tell apart.
                .id(SharedString::from(format!("settings-skill-actions-{id}")))
                .flex_shrink_0()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(row_menu(&id, app)),
        )
}

/// The "…" at the end of a row: open it, or drop it.
fn row_menu(id: &str, app: Entity<AppState>) -> impl IntoElement {
    let open_id = id.to_string();
    let delete_id = id.to_string();
    let open_element = id.to_string();
    let delete_element = id.to_string();
    Button::new(SharedString::from(format!("settings-skill-menu-{id}")))
        .icon(Icon::new(IconName::Ellipsis).size(px(14.)))
        .ghost()
        .xsmall()
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            let open_app = app.clone();
            let delete_app = app.clone();
            let open_id = open_id.clone();
            let delete_id = delete_id.clone();
            let open_element = open_element.clone();
            let delete_element = delete_element.clone();
            menu.item(
                PopupMenuItem::element(move |_, _| {
                    div()
                        .id(SharedString::from(format!(
                            "settings-skill-open-{open_element}"
                        )))
                        .child("Open")
                })
                .on_click(move |_, _, cx| {
                    let id = open_id.clone();
                    open_app.update(cx, |state, cx| state.open_skill(id, cx));
                }),
            )
            .item(
                PopupMenuItem::element(move |_, _| {
                    div()
                        .id(SharedString::from(format!(
                            "settings-skill-delete-{delete_element}"
                        )))
                        .child("Delete")
                })
                .on_click(move |_, _, cx| {
                    let id = delete_id.clone();
                    delete_app.update(cx, |state, cx| state.ask_skill_delete(id, cx));
                }),
            )
        })
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob: `use super::*` would pull GPUI's `test` attribute in over the
    // one the test harness wants.
    use super::super::matching_skills;
    use super::{SkillScope, SkillSummary, empty_line};

    fn rows(count: usize) -> Vec<SkillSummary> {
        (0..count)
            .map(|index| SkillSummary {
                id: format!("skl_{index}"),
                name: format!("skill-{index}"),
                description: String::new(),
                source: crate::opengrok::SkillSource::Authored,
                updated_at_ms: 0,
                version_count: 1,
                draft: false,
                enabled: true,
            })
            .collect()
    }

    /// Three different facts, three different sentences: still fetching, a library with nothing
    /// in it, and a search that matched none of what is there. One sentence for all three is how
    /// somebody comes to believe their skills are gone.
    #[test]
    fn an_empty_list_says_which_kind_of_empty_it_is() {
        assert_eq!(
            empty_line(true, 0, SkillScope::Yours, false),
            Some("Loading…")
        );
        assert!(
            empty_line(false, 0, SkillScope::Yours, false)
                .unwrap()
                .starts_with("No skills yet")
        );
        assert!(
            empty_line(false, 0, SkillScope::Discover, false)
                .unwrap()
                .contains("org")
        );
        let listed = rows(3).len();
        assert_eq!(
            empty_line(false, listed, SkillScope::Yours, false),
            Some("No skills match.")
        );
        assert_eq!(
            empty_line(true, listed, SkillScope::Yours, false),
            Some("No skills match."),
            "a refresh over a list that is already there is not an empty library"
        );
        assert_eq!(
            empty_line(false, 0, SkillScope::Yours, true),
            None,
            "a library nobody could fetch is not a library with nothing in it"
        );
        // A search that matches nothing leaves no rows, which is what puts the line on screen.
        assert!(matching_skills(&rows(3), "nothing").is_empty());
    }
}
