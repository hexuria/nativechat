//! Where the composer's panels get their rows.
//!
//! Every list sits behind one small type, so the rows can come from the server later without
//! the panel or the composer changing: `ToolSource` answers from a hardcoded roster of the
//! server's built-in tools until the composer can ask for the real one, and `SkillSource`
//! answers from the recipes the app has already loaded, one example skill that stands in until
//! the server has a skills registry, and a fixed roster of the app's own commands.

use crate::components::composer_panel::ComposerPanelRow;
use crate::opengrok::RecipeSummary;
use crate::state::AppSettingsTab;

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
    use super::{ComposerPick, SkillSource, TokenKind, ToolSource};
    use crate::opengrok::RecipeSummary;

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
