//! OpenGrok HTTP client. No GPUI. Cookies are the session.

mod activity;
mod client;
mod error;
mod gen_ui;
mod types;

pub use activity::{ActivityTick, BotActivity, activity_from_agui};
pub use gen_ui::{
    agui_tools, BarChartSpec, BarItem, ChatPart, CompletedUiTool, FormField, FormSpec,
    TurnAssembler, UiSpec, MAX_TURN_CONTINUES, UI_TOOL_RESULT,
};

pub use client::OpenGrokClient;
pub use error::OpenGrokError;
pub use types::{
    Account, AguiMessage, Coworker, CoworkerPatch, ModelCatalogue, ModelEntry, ProfileUpdate,
    assistant_text_from_sse,
};
