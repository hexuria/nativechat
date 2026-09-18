//! OpenGrok HTTP client. No GPUI. Cookies are the session.

mod activity;
mod client;
mod error;
mod gen_ui;
mod local_exec;
mod types;
mod user_form;

pub use activity::{
    ActivityTick, BotActivity, ToolCallTracker, activity_from_agui, activity_from_replay,
    deeds_from_replay, tool_standin,
};
pub use gen_ui::{
    ApprovalSpec, BarChartSpec, BarItem, ChatPart, CompletedUiTool, FormField, FormSpec,
    LocalExecResolution, MAX_TURN_CONTINUES, ScreenshotSpec, TurnAssembler, UI_TOOL_RESULT,
    USER_MACHINE_SHELL, UiSpec, agui_tools, approval_from_event, collapse_open_approvals,
    command_from_args, command_from_replay_events, local_exec_outcome, policy_answer,
};
pub use local_exec::{enrol_this_machine, serve_local_exec, stored_machine_id};
pub use user_form::{
    BOX_HANDOFF_RESOLVE_PATH, BoxHandoffReply, BoxHandoffResolution, FormResolution,
    MASKED_PRESENCE_STUB, REQUEST_USER_FORM_TOOL, USER_FORM_CUSTOM, USER_FORM_DISMISS_PATH,
    USER_FORM_SERVER_FILL_AVAILABLE, USER_FORM_SUBMIT_PATH, UserFormActionReply,
    UserFormDismissMode, UserFormField, UserFormFieldKind, UserFormSpec, UserFormValues,
    WAITING_FOR_YOU, box_handoff_action_from_http, continue_enabled, dismiss_request_body,
    is_user_form_awaiting, is_user_form_custom_name, is_user_form_event, is_user_form_tool,
    resolve_handoff_request_body, submit_request_body, user_form_action_from_http,
};

pub use client::{
    ConnectedComputer, CoworkerComputer, ImageStatus, LocalExecMode, OpenGrokClient,
    QueuedApproval, RecipeBot, RecipeDetail, RecipeGrant, RecipeKind, RecipeParameter,
    RecipeParameterKind, RecipeRelation, RecipeRun, RecipeRunResult, RecipeScreen, RecipeShare,
    RecipeShareState, RecipeShareTarget, RecipeStep, RecipeSummary, RecipeTape, RecipeTapeEvent,
    RecipeVersion, RunReplay, StopReply, ThreadReplay, ThreadRun, TurnRecipe, UpdateStatus,
    thin_tape,
};
pub use error::{Failure, OpenGrokError, Unreachable, reads_as_gateway_unreachable};
pub use types::{
    Account, AguiMessage, Coworker, CoworkerPatch, ModelCatalogue, ModelEntry, ProfileUpdate,
    ReplyQuote, assistant_text_from_sse,
};
