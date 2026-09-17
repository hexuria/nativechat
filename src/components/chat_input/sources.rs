//! Where the composer's panels get their rows.
//!
//! Every list sits behind one small type, so the rows can come from the server later without
//! the panel or the composer changing: `ToolSource` answers from a hardcoded roster of the
//! server's built-in tools until the composer can ask for the real one, and `SlashSource`
//! answers from the recipes the app has already loaded, a note standing in for the skills
//! nothing lists yet, and a fixed roster of the app's own actions.
//!
//! FOUR WORDS, ONE THING EACH. A RECIPE is a taped sequence, replayed exactly by the box alone.
//! A WORKFLOW is a decision tree that drives recipes, walked by the server. A SKILL is a lesson —
//! written notes on how a task is done, which the model reads. An ACTION is something the app
//! itself does. Until this change `/` called a recipe a skill, which left one noun standing for
//! two things that cost wildly different amounts to have: a recipe is free and instant, and a
//! lesson costs the model some reading every time it is used.
//!
//! `ParameterSource` and `ValueSource` are the two the composer shows once a recipe is on the
//! draft: what that recipe needs told, and what one of those things may be told.

use crate::components::composer_panel::ComposerPanelRow;
use crate::opengrok::{RecipeParameter, RecipeParameterKind, RecipeSummary};
use crate::state::{ActiveRecipe, AppSettingsTab};

/// What a picked row stands for. The panel only says which row it was; this says what to do
/// about it, and it is the composer that does it.
#[derive(Clone, Debug, PartialEq)]
pub enum ComposerPick {
    /// The native picker, for images to put above the message.
    AttachFiles,
    /// The bot's screen, with a tape running.
    TeachTask,
    /// A chip, in the message, at the caret.
    Token {
        kind: TokenKind,
        /// What the thing is called where it lives: a tool's name, a recipe's id.
        id: String,
        /// The chip's own words: `Weekly report` for a recipe, which is what goes into the
        /// message, and `@shell` for a tool, which is what the chip beside the "+" is named
        /// after. The `/` a person typed to open the panel is how they asked, not part of what
        /// they are saying, so a recipe's chip does not carry it; the kind rides along in
        /// [`TokenKind`] instead.
        text: String,
    },
    /// One of the app's own commands, run now.
    Command(AppCommand),
    /// One parameter of the active recipe, by its place in the declaration, to be given a value.
    Parameter { index: usize },
    /// What the parameter whose panel is open is worth. `None` takes its value away.
    Value(Option<String>),
    /// A row that only says something.
    Nothing,
}

/// What a chip in the message stands for, kept beside the chip so the next step can send the
/// message as structured data rather than as a string someone has to parse back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Tool,
    /// A taped sequence the box replays exactly. It was called a skill here until the four
    /// words were settled, which left the one noun standing for two very different prices.
    Recipe,
}

/// One of the app's own commands. The ones that have an action in [`crate::actions`] are
/// dispatched as that action, so they go the same way as the menu bar and the keyboard; the
/// rest call the state method the rest of the app calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCommand {
    Settings,
    SettingsTab(AppSettingsTab),
    NewChat,
    ToggleTheme,
    Recipes,
    Collections,
    Groups,
}

/// The tools a bot has. Hardcoded to the server's built-ins for now; a later change swaps
/// [`ToolSource::rows`] for the list the server actually reports, and the plugins-and-connectors
/// notice for the plugins it finds.
pub struct ToolSource;

impl ToolSource {
    pub fn rows(&self) -> Vec<(ComposerPanelRow, ComposerPick)> {
        let mut rows: Vec<(ComposerPanelRow, ComposerPick)> = BUILTIN_TOOLS
            .iter()
            .map(|(name, icon, description)| {
                (
                    ComposerPanelRow::new(format!("tool:{name}"), *icon, *name, *description)
                        .label("Tool"),
                    ComposerPick::Token {
                        kind: TokenKind::Tool,
                        id: (*name).to_string(),
                        text: format!("@{name}"),
                    },
                )
            })
            .collect();
        rows.push((
            ComposerPanelRow::new(
                "tool:plugins",
                "icons/plugins.svg",
                "Plugins and connectors",
                "Not listed yet — only the bot's built-in tools are here",
            )
            .note(),
            ComposerPick::Nothing,
        ));
        rows
    }
}

/// The server's built-in tools: what each is called, its icon, and one line on what it does.
const BUILTIN_TOOLS: &[(&str, &str, &str)] = &[
    (
        "shell",
        "icons/wrench.svg",
        "Run a command on the bot's computer",
    ),
    (
        "read_file",
        "icons/library.svg",
        "Read a file on the bot's computer",
    ),
    (
        "write_file",
        "icons/pencil.svg",
        "Write a file on the bot's computer",
    ),
    ("open_url", "icons/globe.svg", "Open a page in the browser"),
    (
        "computer",
        "icons/monitor.svg",
        "See and work the bot's screen",
    ),
    (
        "run_recipe",
        "icons/record.svg",
        "Play a task the bot has been taught",
    ),
];

/// What `/` lists: the recipes the app has, and the app's own commands.
///
/// Named after the key that opens it, the way [`crate::components::chat_input::PanelMode::Plus`]
/// is named after the button, because what it lists is more than one kind of thing and no one
/// noun covers them.
pub struct SlashSource;

impl SlashSource {
    /// Recipes first, because they are what `/` is mostly for, then the commands.
    pub fn rows(&self, recipes: &[RecipeSummary]) -> Vec<(ComposerPanelRow, ComposerPick)> {
        let mut rows: Vec<(ComposerPanelRow, ComposerPick)> = recipes
            .iter()
            .map(|recipe| {
                let name = recipe_name(recipe);
                (
                    ComposerPanelRow::new(
                        format!("recipe:{}", recipe.id),
                        "icons/record.svg",
                        name.clone(),
                        if recipe.description.trim().is_empty() {
                            "A task one of your bots was taught".to_string()
                        } else {
                            recipe.description.trim().to_string()
                        },
                    )
                    .label("Recipe"),
                    ComposerPick::Token {
                        kind: TokenKind::Recipe,
                        id: recipe.id.clone(),
                        text: name,
                    },
                )
            })
            .collect();
        rows.push(no_skills_yet());
        rows.extend(
            APP_COMMANDS
                .iter()
                .map(|(key, icon, title, description, command)| {
                    (
                        // The chord is left empty here and filled in from the keymap the app
                        // registered, so a row shows the keys that really work or none at all.
                        ComposerPanelRow::new(format!("action:{key}"), *icon, *title, *description)
                            .label("Action"),
                        ComposerPick::Command(*command),
                    )
                }),
        );
        rows
    }
}

/// What the recipe on the draft needs told. This is what `@` offers in place of the bot's
/// tools while a recipe is active: a turn that is already a recipe run is not looking for a
/// tool, it is looking for the things the recipe cannot run without.
pub struct ParameterSource;

impl ParameterSource {
    pub fn rows(&self, recipe: &ActiveRecipe) -> Vec<(ComposerPanelRow, ComposerPick)> {
        let unfilled = recipe.unfilled();
        if unfilled.is_empty() {
            return vec![nothing_left(recipe)];
        }
        unfilled
            .into_iter()
            .map(|(index, parameter)| {
                (
                    ComposerPanelRow::new(
                        format!("param:{}", parameter.name),
                        parameter_icon(parameter),
                        parameter.name.clone(),
                        parameter_state(parameter),
                    )
                    .element_id(format!("composer-param-{}", parameter.name))
                    .label(parameter_label(parameter)),
                    ComposerPick::Parameter { index },
                )
            })
            .collect()
    }
}

/// The one row a recipe with nothing left to fill in shows. A panel with no rows at all reads
/// as a list that failed to load, so the good news is said outright instead — and the two ways
/// of having nothing to do are told apart, because "there was never anything to tell it" and
/// "you have told it everything" are different things to have just learned.
fn nothing_left(recipe: &ActiveRecipe) -> (ComposerPanelRow, ComposerPick) {
    let (title, standing) = if recipe.parameters.is_empty() {
        (
            format!("{} needs nothing told", recipe.name),
            "Write the message and send it",
        )
    } else {
        (
            format!("{} has everything it needs", recipe.name),
            "Every value is in the bar below, where it can still be changed",
        )
    };
    (
        ComposerPanelRow::new("param:none", "icons/check.svg", title, standing)
            .element_id("composer-param-none")
            .note(),
        ComposerPick::Nothing,
    )
}

/// What one parameter may be told: the values its declaration allows, the yes and the no of a
/// boolean, and a way to leave it unfilled again. A parameter the declaration does not narrow
/// is typed into the panel's own field, and the row there only says so.
pub struct ValueSource;

impl ValueSource {
    pub fn rows(
        &self,
        parameter: &RecipeParameter,
        filled: Option<&str>,
    ) -> Vec<(ComposerPanelRow, ComposerPick)> {
        let mut rows: Vec<(ComposerPanelRow, ComposerPick)> = Vec::new();
        match (parameter.allowed(), parameter.kind) {
            (Some(allowed), _) => rows.extend(allowed.iter().enumerate().map(|(index, value)| {
                (
                    ComposerPanelRow::new(
                        format!("value:{value}"),
                        "icons/check.svg",
                        value.clone(),
                        format!("Use this for {}", parameter.name),
                    )
                    .element_id(format!("composer-value-choice-{index}")),
                    ComposerPick::Value(Some(value.clone())),
                )
            })),
            (None, RecipeParameterKind::Boolean) => {
                rows.extend([("Yes", "true"), ("No", "false")].into_iter().map(
                    |(title, value)| {
                        (
                            ComposerPanelRow::new(
                                format!("value:{value}"),
                                "icons/check.svg",
                                title,
                                format!("Set {} to {value}", parameter.name),
                            )
                            .element_id(format!("composer-value-choice-{value}")),
                            ComposerPick::Value(Some(value.to_string())),
                        )
                    },
                ));
            }
            (None, _) => rows.push((
                ComposerPanelRow::new(
                    "value:typed",
                    "icons/pencil.svg",
                    format!("Type the value for {}, then press ↵", parameter.name),
                    typed_hint(parameter),
                )
                .element_id("composer-value-typed")
                .always()
                .note(),
                ComposerPick::Nothing,
            )),
        }
        if filled.is_some() {
            rows.push((
                ComposerPanelRow::new(
                    "value:clear",
                    "icons/trash.svg",
                    format!("Clear {}", parameter.name),
                    "Leave it unfilled",
                )
                .element_id("composer-value-clear"),
                ComposerPick::Value(None),
            ));
        }
        rows
    }
}

/// Where a parameter stands, said outright rather than left to be worked out from what is
/// missing: a required one nobody has filled in is the thing stopping the message being sent.
/// Every row in this list is unfilled, so there is no third case to say.
fn parameter_state(parameter: &RecipeParameter) -> String {
    let said = parameter.description.trim();
    let standing = if parameter.required {
        "Not filled in yet"
    } else {
        "Optional"
    };
    if said.is_empty() {
        standing.to_string()
    } else {
        format!("{standing} — {said}")
    }
}

fn parameter_label(parameter: &RecipeParameter) -> String {
    if parameter.required {
        format!("Required · {}", parameter.kind.label())
    } else {
        parameter.kind.label().to_string()
    }
}

fn parameter_icon(parameter: &RecipeParameter) -> &'static str {
    if parameter.required {
        "icons/report.svg"
    } else {
        "icons/pencil.svg"
    }
}

/// What the field under a free parameter will and will not take.
fn typed_hint(parameter: &RecipeParameter) -> String {
    match parameter.kind {
        RecipeParameterKind::Number => "Digits only — letters are not a number".to_string(),
        _ => "Anything you like".to_string(),
    }
}

/// The one row standing in for skills, which nothing lists yet.
///
/// It was a made-up `example-skill` that could be picked, and picking it put a chip in the
/// message that stood for nothing anywhere: no lesson was written, and nothing read one. A row
/// that can be taken and does nothing is worse than a row that says it is not ready, so this is
/// a notice — shown, dimmed, never picked — the same as the plugins line above the tools.
///
/// A skill is a lesson: written notes on how a task is done, which the model reads. There is
/// nowhere to keep one yet, so what this row promises is a word, not a feature. Delete it the
/// day a lesson has a home.
fn no_skills_yet() -> (ComposerPanelRow, ComposerPick) {
    (
        ComposerPanelRow::new(
            "skill:none",
            "icons/study.svg",
            "Skills",
            "A lesson your bot reads before it works — not written or kept anywhere yet",
        )
        .element_id("composer-skills-none")
        .label("Skill")
        .note(),
        ComposerPick::Nothing,
    )
}

/// A recipe with no name still has to be readable in a list.
fn recipe_name(recipe: &RecipeSummary) -> String {
    let name = recipe.name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        "Untitled recipe".to_string()
    } else {
        name
    }
}

/// The app's own commands, as the person meets them: what it is called, where it lives, and
/// which command it is.
const APP_COMMANDS: &[(&str, &str, &str, &str, AppCommand)] = &[
    (
        "settings",
        "icons/wrench.svg",
        "Settings",
        "Open the app's settings",
        AppCommand::Settings,
    ),
    (
        "settings-general",
        "icons/wrench.svg",
        "Settings: General",
        "Settings",
        AppCommand::SettingsTab(AppSettingsTab::General),
    ),
    (
        "settings-profile",
        "icons/account_settings.svg",
        "Settings: Profile",
        "Settings",
        AppCommand::SettingsTab(AppSettingsTab::Profile),
    ),
    (
        "settings-appearance",
        "icons/sun.svg",
        "Settings: Appearance",
        "Settings",
        AppCommand::SettingsTab(AppSettingsTab::Appearance),
    ),
    (
        "settings-shortcuts",
        "icons/session.svg",
        "Settings: Keyboard shortcuts",
        "Settings",
        AppCommand::SettingsTab(AppSettingsTab::Shortcuts),
    ),
    (
        "settings-computer",
        "icons/monitor.svg",
        "Settings: Computer",
        "Settings",
        AppCommand::SettingsTab(AppSettingsTab::Computer),
    ),
    (
        "new-chat",
        "icons/apps.svg",
        "New chat",
        "Start a chat with a bot",
        AppCommand::NewChat,
    ),
    (
        "toggle-theme",
        "icons/moon.svg",
        "Toggle theme",
        "Switch between light and dark",
        AppCommand::ToggleTheme,
    ),
    (
        "recipes",
        "icons/record.svg",
        "Recipes",
        "The tasks your bots have been taught",
        AppCommand::Recipes,
    ),
    (
        "collections",
        "icons/collections.svg",
        "Collections",
        "Your saved things",
        AppCommand::Collections,
    ),
    (
        "groups",
        "icons/groups.svg",
        "Groups",
        "Bots that work together",
        AppCommand::Groups,
    ),
];

#[cfg(test)]
mod tests {
    use super::{ComposerPick, ParameterSource, SlashSource, TokenKind, ToolSource, ValueSource};
    use crate::opengrok::{RecipeParameter, RecipeSummary};
    use crate::state::ActiveRecipe;

    /// The recipe the owner hit this on: one required text parameter and nothing else.
    fn youtube() -> ActiveRecipe {
        let recipe: RecipeSummary = serde_json::from_value(serde_json::json!({
            "id": "rcp_1",
            "name": "youtube",
            "parameters": [
                { "name": "search_term", "description": "What to search YouTube for",
                  "required": true, "kind": "text", "default": null, "values": null },
                { "name": "count", "description": "How many to bring back",
                  "required": false, "kind": "number", "default": null, "values": null }
            ]
        }))
        .expect("the declaration the server sends");
        ActiveRecipe::from_summary(&recipe)
    }

    #[test]
    fn a_tool_becomes_an_at_chip_and_the_notice_becomes_nothing() {
        let rows = ToolSource.rows();
        let shell = rows
            .iter()
            .find(|(row, _)| row.id == "tool:shell")
            .expect("the built-in shell tool is offered");
        assert_eq!(
            shell.1,
            ComposerPick::Token {
                kind: TokenKind::Tool,
                id: "shell".into(),
                text: "@shell".into(),
            }
        );
        let notice = rows.last().expect("the plugins notice closes the list");
        assert_eq!(notice.1, ComposerPick::Nothing);
        assert!(!notice.0.selectable);
    }

    #[test]
    fn a_recipe_becomes_a_chip_named_after_it_without_the_slash() {
        let recipe: RecipeSummary =
            serde_json::from_value(serde_json::json!({ "id": "rec_1", "name": "Weekly  report" }))
                .expect("a recipe needs nothing but an id and a name");
        let rows = SlashSource.rows(&[recipe]);
        assert_eq!(
            rows[0].1,
            ComposerPick::Token {
                kind: TokenKind::Recipe,
                id: "rec_1".into(),
                // The run of spaces in the name is collapsed: a chip is one token. The `/` that
                // opened the panel is how it was asked for, not part of the name.
                text: "Weekly report".into(),
            }
        );
        assert_eq!(
            rows[0].0.label.as_deref(),
            Some("Recipe"),
            "a taped sequence is a recipe; calling it a skill left one noun standing for two \
             very different things to have"
        );
    }

    #[test]
    fn the_app_commands_come_after_the_recipes_and_say_so() {
        let rows = SlashSource.rows(&[]);
        let commands: Vec<_> = rows
            .iter()
            .skip_while(|(row, _)| !row.id.starts_with("action:"))
            .collect();
        assert!(
            commands
                .iter()
                .all(|(row, _)| row.id.starts_with("action:"))
        );
        assert!(
            commands
                .iter()
                .all(|(row, _)| row.label.as_deref() == Some("Action")),
            "a command is an Action, so the list says which rows do something to the app"
        );
        assert!(
            commands.iter().all(|(row, _)| row.shortcut.is_none()),
            "the chord comes from the keymap the app registered, not from this table"
        );
    }

    #[test]
    fn a_parameter_row_says_what_it_is_and_whether_it_is_still_needed() {
        let recipe = youtube();
        let rows = ParameterSource.rows(&recipe);
        let (search, pick) = &rows[0];
        assert_eq!(search.title, "search_term");
        assert!(
            search.description.starts_with("Not filled in yet"),
            "a required parameter nobody has filled in has to read as unfilled at a glance, \
             and this one read {:?}",
            search.description
        );
        assert!(search.description.contains("What to search YouTube for"));
        assert_eq!(search.label.as_deref(), Some("Required · text"));
        assert_eq!(*pick, ComposerPick::Parameter { index: 0 });
        assert_eq!(
            rows[1].0.label.as_deref(),
            Some("number"),
            "one that is not required says what it takes and nothing about being needed"
        );
        assert_eq!(rows[1].0.description, "Optional — How many to bring back");
    }

    /// The list is what is left to do. A value that has been given is shown in the bar over the
    /// composer, and repeating it here only pads the list someone is reading to find their next
    /// move; the pick that is left still points at the parameter's place in the declaration.
    #[test]
    fn a_filled_parameter_leaves_the_list_and_the_rest_keep_their_places() {
        let mut recipe = youtube();
        recipe.set_value("search_term", Some("mundo".to_string()));
        let rows = ParameterSource.rows(&recipe);
        assert_eq!(
            rows.iter()
                .map(|(row, _)| row.title.as_ref())
                .collect::<Vec<_>>(),
            vec!["count"],
            "the one that was told something has nothing left to ask"
        );
        assert_eq!(
            rows[0].1,
            ComposerPick::Parameter { index: 1 },
            "count is still the second parameter declared, wherever it sits in the list"
        );
    }

    #[test]
    fn what_is_required_is_asked_for_before_what_is_optional() {
        let recipe: RecipeSummary = serde_json::from_value(serde_json::json!({
            "id": "rcp_3",
            "name": "report",
            "parameters": [
                { "name": "format", "required": false, "kind": "text" },
                { "name": "since", "required": true, "kind": "text" },
                { "name": "tone", "required": false, "kind": "text" },
                { "name": "until", "required": true, "kind": "text" }
            ]
        }))
        .unwrap();
        let rows = ParameterSource.rows(&ActiveRecipe::from_summary(&recipe));
        assert_eq!(
            rows.iter()
                .map(|(row, _)| row.title.as_ref())
                .collect::<Vec<_>>(),
            vec!["since", "until", "format", "tone"],
            "what stops the message being sent comes first, and the declaration's own order \
             holds within each group so the list does not reshuffle as values come in"
        );
    }

    #[test]
    fn a_recipe_that_needs_nothing_says_so_rather_than_showing_an_empty_list() {
        let recipe: RecipeSummary =
            serde_json::from_value(serde_json::json!({ "id": "rcp_2", "name": "Mail" })).unwrap();
        let rows = ParameterSource.rows(&ActiveRecipe::from_summary(&recipe));
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].0.selectable, "there is nothing there to pick");
        assert!(rows[0].0.title.contains("needs nothing"));
    }

    #[test]
    fn a_recipe_told_everything_says_so_rather_than_showing_an_empty_list() {
        let mut recipe = youtube();
        recipe.set_value("search_term", Some("mundo".to_string()));
        recipe.set_value("count", Some("5".to_string()));
        let rows = ParameterSource.rows(&recipe);
        assert_eq!(rows.len(), 1, "an empty panel reads as a list that broke");
        assert!(!rows[0].0.selectable, "there is nothing there to pick");
        assert!(
            rows[0].0.title.contains("has everything it needs"),
            "being done is different from never having been asked, and this one read {:?}",
            rows[0].0.title
        );
        assert!(
            rows[0].0.description.contains("bar below"),
            "the values did not vanish, and the row says where they went"
        );
    }

    #[test]
    fn a_value_is_picked_from_the_declared_set_and_typed_when_there_is_none() {
        let narrowed: RecipeParameter = serde_json::from_value(serde_json::json!({
            "name": "lang", "kind": "text", "values": ["en", "es"]
        }))
        .unwrap();
        let rows = ValueSource.rows(&narrowed, None);
        assert_eq!(
            rows.iter()
                .map(|(row, _)| row.title.as_ref())
                .collect::<Vec<_>>(),
            vec!["en", "es"],
            "a narrowed parameter is chosen from, so the allowed values are the rows"
        );
        assert_eq!(rows[1].1, ComposerPick::Value(Some("es".to_string())));

        let flag: RecipeParameter =
            serde_json::from_value(serde_json::json!({"name": "shorts", "kind": "boolean"}))
                .unwrap();
        let rows = ValueSource.rows(&flag, Some("true"));
        assert_eq!(
            rows.iter()
                .map(|(row, _)| row.title.as_ref())
                .collect::<Vec<_>>(),
            vec!["Yes", "No", "Clear shorts"],
            "a yes or no is a choice rather than a field to type into"
        );
        assert_eq!(
            rows[2].1,
            ComposerPick::Value(None),
            "a value that has been given can be taken away again"
        );

        let free: RecipeParameter =
            serde_json::from_value(serde_json::json!({"name": "search_term", "kind": "text"}))
                .unwrap();
        let rows = ValueSource.rows(&free, None);
        assert_eq!(rows.len(), 1);
        assert!(
            !rows[0].0.selectable,
            "the row is the instruction, not a value"
        );
        assert!(
            rows[0].0.always,
            "the line telling someone to type a value must not vanish as they type one"
        );
    }

    /// A skill is a lesson the model reads, and nothing writes or keeps one yet. The row that
    /// used to stand here could be picked and left a chip standing for nothing; a row that does
    /// nothing has to say so rather than look like a row that works.
    #[test]
    fn skills_are_a_notice_rather_than_a_row_that_can_be_taken() {
        let rows = SlashSource.rows(&[]);
        let (row, pick) = rows
            .iter()
            .find(|(row, _)| row.id == "skill:none")
            .expect("the `/` list says what a skill is even while nothing lists one");
        assert_eq!(row.label.as_deref(), Some("Skill"));
        assert!(!row.selectable, "there is nothing there to take");
        assert_eq!(*pick, ComposerPick::Nothing);
        assert!(
            row.description.to_lowercase().contains("not"),
            "the row has to read as something that is not ready, and it read {:?}",
            row.description
        );
    }
}
