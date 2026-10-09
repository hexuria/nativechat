//! Faults: reads that failed at one place in the app, kept as notifications.
//!
//! A card never prints a failure's own text. Its title row carries a ⚠ badge while the place has
//! an unread fault, and a click on the badge opens the fault window (`components::fault_window`),
//! which shows the place, the Bot, the request, the status, where in the source it was raised and
//! the whole text, in a box of a fixed ten lines. The badge is read off the notifications, so the
//! two can never disagree: marking a fault's notification read hides the badge, marking it unread
//! brings it back, and with several the newest shows first. A read that works resolves them.

/// Where a fault shows: a card or a page, each with a badge on its title row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    /// A Bot's Usage card and the Usage modal.
    Usage,
    /// A Bot's tools, on its settings and in the Plugins window.
    Tools,
    /// The model picker's list.
    Models,
    /// A Bot's Skills card.
    BotSkills,
    /// The skills library in the Plugins window.
    Skills,
    /// One skill's page in the Plugins window.
    Skill,
    /// The Recipes page's list.
    Recipes,
    /// One recipe's page.
    Recipe,
    /// Settings → Saved logins.
    Logins,
    /// The server itself not answering: the reconnect banner.
    Server,
}

impl Place {
    pub const ALL: [Place; 10] = [
        Place::Usage,
        Place::Tools,
        Place::Models,
        Place::BotSkills,
        Place::Skills,
        Place::Skill,
        Place::Recipes,
        Place::Recipe,
        Place::Logins,
        Place::Server,
    ];

    /// The word kept in the notifications table and used in stable ids: `fault-usage`.
    pub fn word(self) -> &'static str {
        match self {
            Place::Usage => "usage",
            Place::Tools => "tools",
            Place::Models => "models",
            Place::BotSkills => "bot-skills",
            Place::Skills => "skills",
            Place::Skill => "skill",
            Place::Recipes => "recipes",
            Place::Recipe => "recipe",
            Place::Logins => "logins",
            Place::Server => "server",
        }
    }

    pub fn from_word(word: &str) -> Option<Place> {
        Place::ALL.into_iter().find(|place| place.word() == word)
    }

    /// The place as the person knows it: the card's or the page's own title.
    pub fn label(self) -> &'static str {
        match self {
            Place::Usage => "Usage",
            Place::Tools => "Tools",
            Place::Models => "Models",
            Place::BotSkills => "Skills",
            Place::Skills => "Skills library",
            Place::Skill => "Skill",
            Place::Recipes => "Recipes",
            Place::Recipe => "Recipe",
            Place::Logins => "Saved logins",
            Place::Server => "Server",
        }
    }

    /// Whether a fault here belongs to one Bot, so the badge shows only on that Bot's card.
    pub fn per_bot(self) -> bool {
        matches!(self, Place::Usage | Place::Tools | Place::BotSkills)
    }

    /// The badge's stable id, for gpui-agent.
    pub fn badge_id(self) -> String {
        format!("fault-{}", self.word())
    }
}

/// What a fault is raised with: what the failure itself says, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaultInput {
    pub raw: String,
    pub endpoint: Option<String>,
    pub status: Option<u16>,
}

impl FaultInput {
    pub fn from_error(error: &crate::opengrok::OpenGrokError) -> Self {
        Self {
            raw: error.detail(),
            endpoint: error.endpoint().map(str::to_string),
            status: error.status,
        }
    }

    /// A failure that is no request's: a vault's, a file's.
    pub fn local(raw: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            endpoint: None,
            status: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_place_is_found_again_by_its_word() {
        for place in Place::ALL {
            assert_eq!(Place::from_word(place.word()), Some(place));
        }
    }
}
