//! Where the composer's panels get their rows.
//!
//! Every list sits behind one small type, so the rows can come from the server later without
//! the panel or the composer changing: `ToolSource` answers from a hardcoded roster of the
//! server's built-in tools until the composer can ask for the real one, and `SkillSource`
//! answers from the recipes the app has already loaded, one example skill that stands in until
//! the server has a skills registry, and a fixed roster of the app's own commands.
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
        /// The chip's own words: `Weekly report` for a skill, which is what goes into the
        /// message, and `@shell` for a tool, which is what the chip beside the "+" is named
        /// after. The `/` a person typed to open the panel is how they asked, not part of what
        /// they are saying, so a skill's chip does not carry it; the kind rides along in
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
    Skill,
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

/// The skills a bot can be pointed at: the recipes the app has, and the app's own commands.
pub struct SkillSource;

impl SkillSource {
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
                    .label("Skill"),
                    ComposerPick::Token {
                        kind: TokenKind::Skill,
                        id: recipe.id.clone(),
                        text: name,
                    },
                )
            })
            .collect();
        rows.push(example_skill());
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
        if recipe.parameters.is_empty() {
            return vec![(
                ComposerPanelRow::new(
                    "param:none",
                    "icons/record.svg",
                    format!("{} needs nothing told", recipe.name),
                    "Write the message and send it",
                )
                .element_id("composer-param-none")
                .note(),
                ComposerPick::Nothing,
            )];
        }
        recipe
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let filled = recipe.value(&parameter.name);
                (
                    ComposerPanelRow::new(
                        format!("param:{}", parameter.name),
                        parameter_icon(parameter, filled),
                        parameter_title(parameter, filled),
                        parameter_state(parameter, filled),
                    )
                    .element_id(format!("composer-param-{}", parameter.name))
                    .label(parameter_label(parameter)),
                    ComposerPick::Parameter { index },
                )
            })
            .collect()
    }
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

/// A filled parameter shows what it was told; an unfilled one shows only its name, so the two
/// are told apart at a glance rather than read for.
fn parameter_title(parameter: &RecipeParameter, filled: Option<&str>) -> String {
    match filled {
        Some(value) => format!("{} = {}", parameter.name, shorten(value)),
        None => parameter.name.clone(),
    }
}

/// Where a parameter stands, said outright rather than left to be worked out from what is
/// missing: a required one nobody has filled in is the thing stopping the message being sent.
fn parameter_state(parameter: &RecipeParameter, filled: Option<&str>) -> String {
    let said = parameter.description.trim();
    let standing = match (filled.is_some(), parameter.required) {
        (false, true) => "Not filled in yet",
        (false, false) => "Optional",
        (true, _) => {
            return if said.is_empty() {
                "Filled in".to_string()
            } else {
                said.to_string()
            };
        }
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

fn parameter_icon(parameter: &RecipeParameter, filled: Option<&str>) -> &'static str {
    match (filled.is_some(), parameter.required) {
        (true, _) => "icons/check.svg",
        (false, true) => "icons/report.svg",
        (false, false) => "icons/pencil.svg",
    }
}

/// What the field under a free parameter will and will not take.
fn typed_hint(parameter: &RecipeParameter) -> String {
    match parameter.kind {
        RecipeParameterKind::Number => "Digits only — letters are not a number".to_string(),
        _ => "Anything you like".to_string(),
    }
}

/// A value long enough to push the rest of the row off the end is cut, because the row is here
/// to say which parameter is filled rather than to be read as the value.
fn shorten(value: &str) -> String {
    const MOST: usize = 28;
    if value.chars().count() <= MOST {
        return value.to_string();
    }
    let kept: String = value.chars().take(MOST - 1).collect();
    format!("{kept}…")
}

/// PLACEHOLDER. One made-up skill, so the inline chip a skill leaves in the message can be seen
/// while the server has no skills registry to list. It is named and described as an example on
/// purpose: picking it puts its chip in the message the way a real skill would, and nothing else
/// happens. Delete this function and its call the day the server reports real skills.
fn example_skill() -> (ComposerPanelRow, ComposerPick) {
    (
        ComposerPanelRow::new(
            "skill:example",
            "icons/sparkles.svg",
            "example-skill",
            "Example only — a placeholder that does nothing yet, until your bots' skills are listed here",
        )
        .label("Skill"),
        ComposerPick::Token {
            kind: TokenKind::Skill,
            id: "example-skill".to_string(),
            text: "example-skill".to_string(),
        },
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
    use super::{ComposerPick, ParameterSource, SkillSource, TokenKind, ToolSource, ValueSource};
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
        let rows = SkillSource.rows(&[recipe]);
        assert_eq!(
            rows[0].1,
            ComposerPick::Token {
                kind: TokenKind::Skill,
                id: "rec_1".into(),
                // The run of spaces in the name is collapsed: a chip is one token. The `/` that
                // opened the panel is how it was asked for, not part of the name.
                text: "Weekly report".into(),
            }
        );
        assert_eq!(rows[0].0.label.as_deref(), Some("Skill"));
    }

    #[test]
    fn the_app_commands_come_after_the_recipes_and_say_so() {
        let rows = SkillSource.rows(&[]);
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
        let mut recipe = youtube();
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

        // Once it is filled in, the row shows what it was told, against its name.
        recipe.set_value("search_term", Some("mundo".to_string()));
        let rows = ParameterSource.rows(&recipe);
        assert_eq!(rows[0].0.title, "search_term = mundo");
        assert_eq!(rows[0].0.description, "What to search YouTube for");
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

    #[test]
    fn the_example_skill_is_a_chip_and_says_it_is_only_an_example() {
        let rows = SkillSource.rows(&[]);
        let (row, pick) = rows
            .iter()
            .find(|(row, _)| row.id == "skill:example")
            .expect("one example skill stands in until the server lists real ones");
        assert_eq!(row.label.as_deref(), Some("Skill"));
        assert!(
            row.description.to_lowercase().contains("example"),
            "the row has to read as an example rather than as a skill someone can count on"
        );
        assert_eq!(
            *pick,
            ComposerPick::Token {
                kind: TokenKind::Skill,
                id: "example-skill".into(),
                text: "example-skill".into(),
            },
            "picking it leaves the same inline chip a real skill would"
        );
    }
}
