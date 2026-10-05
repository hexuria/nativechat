//! Embedded pack data (`include_str!`). Changing these files changes [`crate::Autosteer`]'s
//! [`pua_core::DataVersion`].

/// Lexicon TOML (`data/lexicon.toml`).
pub(crate) const LEXICON_TOML: &str = include_str!("../data/lexicon.toml");
/// Rules TOML (`data/rules.toml`).
pub(crate) const RULES_TOML: &str = include_str!("../data/rules.toml");
