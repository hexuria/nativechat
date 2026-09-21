//! OpenGrok HTTP client. No GPUI. Cookies are the session.

mod activity;
mod client;
mod credential;
mod error;
mod gen_ui;
mod local_exec;
mod types;
mod user_form;
mod visibility;

pub use activity::{
    ActivityTick, BotActivity, ToolCallTracker, WAKING_COMPUTER, activity_from_agui,
    activity_from_replay, deeds_from_replay, tool_standin,
};
pub use credential::{
    CREDENTIAL_OFFER_SAVE, CREDENTIAL_REQUEST, CREDENTIAL_RESULT, CREDENTIAL_RESULT_PATH,
    CredentialRequestResolution, CredentialRequestSpec, CredentialResultStatus, SaveLoginSpec,
    credential_request_allow_id, credential_request_card_id, credential_request_deny_id,
    credential_request_pill_id, credential_result_body, fold_credential_answer,
    keep_credential_request_offer, keep_local_save_offer, result_without_broker,
    save_login_card_id, save_login_from_local, save_login_save_id, save_login_skip_id,
};
pub use gen_ui::{
    ApprovalSpec, BarChartSpec, BarItem, ChatPart, CompletedUiTool, FormField, FormSpec,
    LocalExecResolution, MAX_TURN_CONTINUES, ScreenshotSpec, TurnAssembler, UI_TOOL_RESULT,
    USER_MACHINE_SHELL, UiSpec, agui_tools, approval_from_event, collapse_open_approvals,
    command_from_args, command_from_replay_events, local_exec_outcome,
    place_hitl_cards_in_document_order, policy_answer,
};
pub use local_exec::{enrol_this_machine, serve_local_exec, stored_machine_id};
pub use user_form::{
    BOX_HANDOFF_RESOLVE_PATH, BoxHandoffReply, BoxHandoffResolution, ComputerHandoffSpec,
    ComputerHandoffStatus, FORM_ENTRY_MISSING, FormResolution, MASKED_PRESENCE_STUB,
    REQUEST_USER_FORM_TOOL, USER_FORM_CUSTOM, USER_FORM_DISMISS_PATH,
    USER_FORM_SERVER_FILL_AVAILABLE, USER_FORM_SUBMIT_PATH, UserFormActionReply,
    UserFormDismissMode, UserFormField, UserFormFieldKind, UserFormHttpSettle, UserFormSpec,
    UserFormValues, UserFormVerb, WAITING_FOR_YOU, bind_call_peers, box_handoff_action_from_http,
    box_handoff_resolve_entry_id, box_handoff_settles_locally, computer_attention_done_id,
    computer_attention_id, computer_attention_skip_id, computer_handoff_card_id,
    computer_handoff_done_id, computer_handoff_skip_id, computer_handoff_takeover_id,
    computer_window_attention_done_id, computer_window_attention_id,
    computer_window_attention_skip_id, continue_enabled, dismiss_request_body,
    is_form_entry_missing, is_user_form_awaiting, is_user_form_custom_name, is_user_form_event,
    is_user_form_tool, resolve_handoff_request_body, settle_user_form_http, submit_request_body,
    user_form_action_from_http, user_form_card_id, user_form_continue_id, user_form_dismiss_id,
    user_form_field_id, user_form_pill_id, user_form_saved_note_id, user_form_screen_id,
    user_form_use_saved_id,
};
pub use visibility::ImageVisibility;

pub use client::{
    BoxShareScope, ConnectedComputer, CoworkerComputer, EgressTunnel, ImageStatus, LocalExecMode,
    NewSchedule, OpenGrokClient, QueuedApproval, RecipeBot, RecipeDetail, RecipeGrant, RecipeKind,
    RecipeParameter, RecipeParameterKind, RecipeRelation, RecipeRun, RecipeRunResult, RecipeScreen,
    RecipeShare, RecipeShareState, RecipeShareTarget, RecipeStep, RecipeSummary, RecipeTape,
    RecipeTapeEvent, RecipeVersion, RunReplay, ScheduleKind, ScheduleRow, StopReply, ThreadReplay,
    ThreadRun, TurnRecipe, UpdateStatus, WebhookInfo, collapse_computer_roster,
    collapse_computers_by_machine_id, env_egress_tunnel_enabled, host_egress_tunnel_available,
    host_egress_tunnel_enabled, host_egress_tunnel_flag, thin_tape,
};
pub use error::{Failure, OpenGrokError, Unreachable, reads_as_gateway_unreachable};
pub use types::{
    Account, AguiMessage, Coworker, CoworkerPatch, ModelCatalogue, ModelEntry, ProfileUpdate,
    ReplyQuote, assistant_text_from_sse,
};
