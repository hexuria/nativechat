//! Skills in the Plugins window. Settings → Skills is gone (7 Oct 2026): a skill's page, your
//! skills and "+ New skill" live under Plugins → Installed (`components::marketplace`). What is
//! left here is the New skill page's three fields.
//!
//! A SKILL is written instructions — a `SKILL.md` with a name and a description in its
//! frontmatter — which the model reads before it starts, and which a person invokes by typing
//! `/name`. A RECIPE is a taped replay of clicks. They are two different kinds of thing that
//! happen to share a slash.

mod add_sheet;

pub use add_sheet::AddSheetInputs;
