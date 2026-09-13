//! OpenGrok HTTP client. No GPUI. Cookies are the session.

mod activity;
mod client;
mod error;
mod types;

pub use activity::{activity_from_agui, ActivityTick, BotActivity};

pub use client::OpenGrokClient;
pub use error::OpenGrokError;
pub use types::{
    assistant_text_from_sse, Account, AguiMessage, Coworker, CoworkerPatch, ModelCatalogue,
    ModelEntry, ProfileUpdate,
};
