//! OpenGrok HTTP client. No GPUI. Cookies are the session.

mod activity;
mod client;
mod error;
mod gen_ui;
mod local_exec;
mod types;

pub use activity::{ActivityTick, BotActivity, activity_from_agui, visible_bot_status};
pub use gen_ui::{
    ApprovalSpec, BarChartSpec, BarItem, ChatPart, CompletedUiTool, FormField, FormSpec,
    LocalExecVerdict, MAX_TURN_CONTINUES, TurnAssembler, UI_TOOL_RESULT, UiSpec, agui_tools,
    approval_from_event, collapse_open_approvals, command_from_args, command_from_replay_events,
    local_exec_outcome,
};
pub use local_exec::{enrol_this_machine, serve_local_exec, stored_machine_id};

pub use client::{ConnectedComputer, LocalExecMode, OpenGrokClient, QueuedApproval, RunReplay};
pub use error::OpenGrokError;
pub use types::{
    Account, AguiMessage, Coworker, CoworkerPatch, ModelCatalogue, ModelEntry, ProfileUpdate,
    assistant_text_from_sse,
};
