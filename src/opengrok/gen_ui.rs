//! Generative UI on an AG-UI stream.
//!
//! Text (`TEXT_MESSAGE_*`) paints as it arrives. A bar chart or form is a
//! *different* event (tool call, CUSTOM, or a complete `{"ui":...}` object)
//! and mounts only when its JSON is whole. Incomplete JSON is held, not
//! rendered as markdown, and does not stall the prose around it.

use std::collections::HashSet;

use serde_json::Value;

use super::client::LocalExecMode;
use super::credential::{CredentialRequestSpec, SaveLoginSpec};
use super::user_form::{
    ComputerHandoffSpec, UserFormSpec, is_user_form_awaiting, is_user_form_tool,
};
use super::visibility::{ImageVisibility, pin_shot_at_turn_end, pin_shot_now};
use crate::threads::{conversation_for_thread, is_mcp_thread};

#[derive(Debug, Clone, PartialEq)]
pub enum ChatPart {
    Text(String),
    Ui(UiSpec),
    /// A tool the person must allow or refuse before the run continues.
    Approval(ApprovalSpec),
    /// The bot's screen when it belongs in the feed (`image.visibility` =
    /// `transcript` | `failure` | `end`, or an untagged turn-end/failure pin).
    /// `agent` shots stay off this list and paint in the Computer pane.
    Screenshot(ScreenshotSpec),
    /// In-chat credentials that fill the box page. Not [`UiSpec::Form`], not a vault.
    UserForm(UserFormSpec),
    /// Opt-in save prompt after Continue. Origin + username only — never a password.
    SaveLogin(SaveLoginSpec),
    /// Confirm-to-reuse a saved login. A.0 answers without typing into Box.
    CredentialRequest(CredentialRequestSpec),
}

fn is_hitl_card(part: &ChatPart) -> bool {
    matches!(
        part,
        ChatPart::UserForm(_) | ChatPart::SaveLogin(_) | ChatPart::CredentialRequest(_)
    )
}

/// Paint HITL cards in the **settled** document slot from the first frame.
///
/// Live SSE used to append the awaiting form after tool screenshots, then
/// replay/`overlay_server_cards` put it under the intro and above the shot
/// block — the card jumped on Dismiss / Continue / Open the screen. Insert
/// before the first [`ChatPart::Screenshot`] (or at the end when there is
/// none) so the open Form Label already sits where Dismissed ends up.
pub fn place_hitl_cards_in_document_order(parts: &mut Vec<ChatPart>) {
    if parts.len() < 2 {
        return;
    }
    let mut hitl = Vec::new();
    let mut rest = Vec::new();
    for part in parts.drain(..) {
        if is_hitl_card(&part) {
            hitl.push(part);
        } else {
            rest.push(part);
        }
    }
    if hitl.is_empty() {
        *parts = rest;
        return;
    }
    let insert_at = rest
        .iter()
        .position(|part| matches!(part, ChatPart::Screenshot(_)))
        .unwrap_or(rest.len());
    rest.splice(insert_at..insert_at, hitl);
    *parts = rest;
}

/// A screenshot the run produced, decoded once and shared by every row that paints it.
#[derive(Debug, Clone)]
pub struct ScreenshotSpec {
    pub call_id: String,
    /// The tool's own words, e.g. "clicking at 120,40; screenshot of the 1280x800 screen attached".
    pub caption: String,
    pub image: std::sync::Arc<gpui_kit::Image>,
    pub width: u32,
    pub height: u32,
    /// `None` on frames that predate `image.visibility`.
    pub visibility: Option<ImageVisibility>,
}

impl PartialEq for ScreenshotSpec {
    /// One screenshot per call: the bytes need not be compared to know it is the same one.
    fn eq(&self, other: &Self) -> bool {
        self.call_id == other.call_id
            && self.caption == other.caption
            && self.width == other.width
            && self.height == other.height
    }
}

impl ScreenshotSpec {
    /// From a `TOOL_CALL_RESULT` frame's `image`
    /// (`{mime, base64, width, height, visibility?}`), or `None` when it is
    /// not a PNG we can paint. Journalled `agent` frames drop `base64`.
    pub fn from_frame(call_id: &str, caption: &str, image: &Value) -> Option<Self> {
        use base64::Engine as _;
        let mime = image
            .get("mime")
            .and_then(Value::as_str)
            .unwrap_or("image/png");
        if mime != "image/png" {
            return None;
        }
        let encoded = image.get("base64").and_then(Value::as_str)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        let width = image.get("width").and_then(Value::as_u64)? as u32;
        let height = image.get("height").and_then(Value::as_u64)? as u32;
        let visibility = image
            .get("visibility")
            .and_then(Value::as_str)
            .and_then(ImageVisibility::parse);
        Some(Self {
            call_id: call_id.to_string(),
            caption: caption.to_string(),
            image: std::sync::Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Png,
                bytes,
            )),
            width,
            height,
            visibility,
        })
    }

    /// Pin into chat now. Tagged `transcript`/`failure`/`end` yes; `agent` no.
    /// Untagged: only a failed tool (`ok == false`), matching the pre-tag heuristic.
    pub fn pin_now(&self, ok: Option<bool>) -> bool {
        pin_shot_now(self.visibility, ok)
    }

    /// Turn-end pin. Tagged `agent` stays off the feed; everything else may
    /// already be pinned (`pin_now`) and this is idempotent by `call_id`.
    pub fn pin_at_turn_end(&self) -> bool {
        pin_shot_at_turn_end(self.visibility)
    }
}

/// The fields `POST /ag-ui/runs/{runId}/answer` needs, plus what the card shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalSpec {
    pub run_id: String,
    /// The thread the run belongs to, when the server said which. An `mcp-…`
    /// thread is the MCP door auditing a call the coworker made somewhere
    /// else, and its cards belong in that coworker's conversation.
    pub thread_id: Option<String>,
    pub call_id: String,
    pub tool: String,
    pub command: String,
    pub why: String,
    pub reason: String,
    /// Tool result after the command ran (`exit 0` + stdout/stderr).
    pub output: Option<String>,
    pub ok: Option<bool>,
}

/// The harness tool that runs on the person's own machine, through this
/// app's local-exec daemon. Every other tool runs on the coworker's box.
pub const USER_MACHINE_SHELL: &str = "user_machine_shell";

/// The person's answer on a permission card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalExecResolution {
    Always,
    AllowOnce,
    Never,
    DenyOnce,
}

impl ApprovalSpec {
    pub fn runs_on_this_mac(&self) -> bool {
        self.tool == USER_MACHINE_SHELL
    }

    /// The card came off the MCP door rather than out of a turn of the chat.
    pub fn is_mcp(&self) -> bool {
        self.thread_id.as_deref().is_some_and(is_mcp_thread)
    }

    /// The conversation this card belongs in, when the server said which
    /// thread raised it.
    pub fn conversation_id(&self) -> Option<&str> {
        self.thread_id.as_deref().map(conversation_for_thread)
    }

    /// Where the command runs, the way the card and its outcome line say it.
    pub fn place(&self) -> &'static str {
        if self.runs_on_this_mac() {
            "your computer"
        } else {
            "its computer"
        }
    }

    /// Grok Bot Steps 3–4: tunnel / auto-review HITL. Exec-consent stays
    /// Allow/Deny once. OpenGrok stamps this reason only when
    /// `isEgressTunnelAvailable` (env `OG_*`/`SAND_*_EGRESS_TUNNEL_ENABLED=1`
    /// or host `egressTunnelEnabled`). We do not invent it.
    ///
    /// Never for an MCP card. The door reuses the same words for why it
    /// stopped a call — `auto-review` among them — and Review chrome would
    /// put an Always allow on a card that has no standing policy behind it:
    /// there is nothing on this Mac for Always to write to.
    pub fn is_review_an_action(&self) -> bool {
        !self.is_mcp()
            && matches!(
                self.reason.trim().to_ascii_lowercase().as_str(),
                "auto-review" | "review-an-action" | "computer-action" | "egress"
            )
    }

    /// The centered line this card leaves behind once it is answered, in the
    /// words its own kind of card uses.
    pub fn outcome(&self, bot: &str, resolution: LocalExecResolution) -> String {
        if self.is_mcp() {
            mcp_call_outcome(bot, resolution, &self.tool)
        } else {
            local_exec_outcome(bot, resolution, self.place())
        }
    }
}

/// What this Mac's Always/Never setting answers on its own. Only the
/// local-shell tool is covered; a box tool always gets its card.
pub fn policy_answer(
    spec: &ApprovalSpec,
    mode: Option<LocalExecMode>,
) -> Option<LocalExecResolution> {
    if !spec.runs_on_this_mac() {
        return None;
    }
    match mode? {
        LocalExecMode::Always => Some(LocalExecResolution::Always),
        LocalExecMode::Never => Some(LocalExecResolution::Never),
        LocalExecMode::Ask => None,
    }
}

impl ChatPart {
    pub fn is_widget(&self) -> bool {
        !matches!(self, Self::Text(_))
    }
}

/// At most one open permission card. Older unanswered host-shell runs stay
/// off this bubble so a turn does not look like it needs two yeses.
pub fn collapse_open_approvals(
    parts: &[ChatPart],
    open_call_ids: &HashSet<String>,
) -> Vec<ChatPart> {
    let keep = parts.iter().rev().find_map(|part| match part {
        ChatPart::Approval(spec) if open_call_ids.contains(&spec.call_id) => {
            Some(spec.call_id.clone())
        }
        _ => None,
    });
    let mut kept = false;
    parts
        .iter()
        .filter(|part| match part {
            ChatPart::Approval(spec) if open_call_ids.contains(&spec.call_id) => {
                if keep.as_ref() == Some(&spec.call_id) && !kept {
                    kept = true;
                    true
                } else {
                    false
                }
            }
            _ => true,
        })
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiSpec {
    BarChart(BarChartSpec),
    Form(FormSpec),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarChartSpec {
    pub title: Option<String>,
    pub bars: Vec<BarItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarItem {
    pub label: String,
    pub value: f32,
}

/// Generative choice-chip form. Answers go through `send_message` as chat text.
///
/// This is **not** user-form credentials. Password / otp / `secret` fields must
/// never be routed here — those are [`UserFormSpec`].
#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub fields: Vec<FormField>,
    pub submit: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub id: String,
    pub label: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedUiTool {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct TurnAssembler {
    committed: Vec<ChatPart>,
    text: String,
    tool: Option<OpenTool>,
    waiting_approval: bool,
    completed_ui: Vec<CompletedUiTool>,
    shell_args: std::collections::HashMap<String, String>,
    /// `request_user_form` args by call id, so an awaiting CUSTOM with empty
    /// `arguments` can still paint the schema.
    form_args: std::collections::HashMap<String, String>,
    /// Newest tool PNG. Computer pane / last-screen thumb; chat only on pin.
    latest_shot: Option<ScreenshotSpec>,
}

#[derive(Debug)]
struct OpenTool {
    id: String,
    name: String,
    args: String,
}

impl TurnAssembler {
    pub fn push_event(&mut self, event: &Value) {
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "TEXT_MESSAGE_CONTENT" | "TEXT_MESSAGE_CHUNK" => {
                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                    self.text.push_str(delta);
                    if self.tool.is_none() {
                        self.flush_text();
                    }
                }
            }
            "TOOL_CALL_START" | "TOOL_CALL_CHUNK" => {
                let name = event
                    .get("toolCallName")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let id = event
                    .get("toolCallId")
                    .or_else(|| event.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if name == USER_MACHINE_SHELL && !id.is_empty() {
                    self.shell_args.entry(id.clone()).or_default();
                }
                if kind == "TOOL_CALL_CHUNK" {
                    if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                        if let Some(buf) = self.shell_args.get_mut(&id) {
                            buf.push_str(delta);
                        }
                    }
                    if let Some(args) = event.get("arguments") {
                        let command = command_from_args(args);
                        if !command.is_empty() && !id.is_empty() {
                            self.shell_args.insert(id.clone(), args.to_string());
                        }
                    }
                }
                if is_ui_tool(name) || is_user_form_tool(name) {
                    self.flush_text();
                    if is_user_form_tool(name) && !id.is_empty() {
                        self.form_args.entry(id.clone()).or_default();
                    }
                    self.tool = Some(OpenTool {
                        id,
                        name: name.to_string(),
                        args: String::new(),
                    });
                }
            }
            "TOOL_CALL_ARGS" => {
                let id = event
                    .get("toolCallId")
                    .or_else(|| event.get("id"))
                    .and_then(Value::as_str);
                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                    if let Some(id) = id {
                        self.shell_args
                            .entry(id.to_string())
                            .or_default()
                            .push_str(delta);
                        if let Some(buf) = self.form_args.get_mut(id) {
                            buf.push_str(delta);
                        }
                    }
                    if let Some(tool) = self.tool.as_mut() {
                        tool.args.push_str(delta);
                    }
                }
            }
            "TOOL_CALL_END" => {
                self.flush_text();
                if let Some(tool) = self.tool.take() {
                    self.close_tool(tool);
                }
            }
            "CUSTOM" => {
                let name = event.get("name").and_then(Value::as_str).unwrap_or("");
                if is_user_form_awaiting(event) {
                    self.flush_text();
                    let call_id = event
                        .get("callId")
                        .or_else(|| event.get("toolCallId"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let fallback = self
                        .form_args
                        .get(call_id)
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
                    if let Some(spec) = UserFormSpec::from_awaiting_event(event, fallback.as_ref())
                    {
                        self.push_user_form(spec);
                    }
                } else if name == "run-awaiting-approval" {
                    self.flush_text();
                    if let Some(mut spec) = approval_from_event(event) {
                        if spec.command.is_empty() {
                            if let Some(raw) = self.shell_args.get(&spec.call_id) {
                                if let Ok(value) = serde_json::from_str::<Value>(raw) {
                                    spec.command = command_from_args(&value);
                                } else if !raw.trim().is_empty() {
                                    spec.command = raw.clone();
                                }
                            }
                        }
                        self.committed.retain(|part| {
                            !matches!(part, ChatPart::Approval(existing) if existing.call_id == spec.call_id)
                        });
                        self.committed.push(ChatPart::Approval(spec));
                        self.waiting_approval = true;
                    }
                } else if let Some(spec) = SaveLoginSpec::from_event(event) {
                    self.flush_text();
                    self.push_save_login(spec);
                } else if let Some(spec) = CredentialRequestSpec::from_event(event) {
                    self.flush_text();
                    self.push_credential_request(spec);
                } else if let Some(spec) = UserFormSpec::from_custom_event(event) {
                    self.flush_text();
                    self.push_user_form(spec);
                } else if let Some(handoff) = ComputerHandoffSpec::from_event(event) {
                    self.flush_text();
                    self.push_user_form(handoff.as_escalated_form());
                } else {
                    let value = event.get("value").cloned().unwrap_or(Value::Null);
                    if let Some(spec) = UiSpec::from_custom(name, &value) {
                        self.flush_text();
                        self.committed.push(ChatPart::Ui(spec));
                    }
                }
            }
            "TOOL_CALL_RESULT" => {
                self.waiting_approval = false;
                self.flush_text();
                self.attach_tool_result(event);
            }
            "RUN_FINISHED" | "RUN_ERROR" => {
                self.waiting_approval = false;
            }
            _ => {}
        }
    }

    pub fn snapshot(&self) -> (String, Vec<ChatPart>) {
        if self.holding_ui() {
            let mut parts = self.committed.clone();
            place_hitl_cards_in_document_order(&mut parts);
            let plain = plain_text(&parts);
            return (plain, parts);
        }
        let mut parts = self.committed.clone();
        if !self.text.is_empty() {
            parts.push(ChatPart::Text(self.text.clone()));
        }
        place_hitl_cards_in_document_order(&mut parts);
        let plain = plain_text(&parts);
        (plain, parts)
    }

    pub fn waiting_approval(&self) -> bool {
        self.waiting_approval
    }

    /// An unresolved user-form card or live Computer sibling is on the turn.
    /// Distinct from a permission card. Local Skip/Dismiss that already
    /// settled the grafted card is the AppState map's job, not this flag.
    pub fn waiting_user_form(&self) -> bool {
        self.committed.iter().any(|part| match part {
            ChatPart::UserForm(spec) if spec.is_unresolved() || spec.live_computer_handoff() => {
                true
            }
            ChatPart::CredentialRequest(spec) if spec.is_unresolved() => true,
            _ => false,
        })
    }

    fn attach_tool_result(&mut self, event: &Value) {
        let call_id = event
            .get("toolCallId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if call_id.is_empty() {
            return;
        }
        let content = event
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ok = event.get("ok").and_then(Value::as_bool);
        if let Some(ChatPart::Approval(spec)) = self.committed.iter_mut().find(
            |part| matches!(part, ChatPart::Approval(existing) if existing.call_id == call_id),
        ) {
            spec.output = Some(content.clone());
            spec.ok = ok;
        }
        // Keep every PNG with bytes for the Computer pane / last-screen thumb.
        // Chat row follows `image.visibility` (opengrok-server#139). Untagged
        // frames keep the old heuristic: failure now, last shot at turn-end.
        if let Some(shot) = event
            .get("image")
            .and_then(|image| ScreenshotSpec::from_frame(&call_id, &content, image))
        {
            let pin_now = shot.pin_now(ok);
            self.latest_shot = Some(shot.clone());
            if pin_now {
                self.pin_shot(shot);
            } else {
                self.break_feed_paragraph();
            }
        }
    }

    fn pin_shot(&mut self, shot: ScreenshotSpec) {
        self.committed.retain(|part| {
            !matches!(part, ChatPart::Screenshot(existing) if existing.call_id == shot.call_id)
        });
        self.committed.push(ChatPart::Screenshot(shot));
    }

    fn pin_latest_shot(&mut self) {
        let Some(shot) = self.latest_shot.clone() else {
            return;
        };
        if !shot.pin_at_turn_end() {
            return;
        }
        if self.committed.iter().any(|part| {
            matches!(part, ChatPart::Screenshot(existing) if existing.call_id == shot.call_id)
        }) {
            return;
        }
        self.committed.push(ChatPart::Screenshot(shot));
    }

    /// A tool PNG that stays off the feed still splits the words around it.
    fn break_feed_paragraph(&mut self) {
        let Some(ChatPart::Text(text)) = self.committed.last_mut() else {
            return;
        };
        if text.trim().is_empty() || text.ends_with("\n\n") {
            return;
        }
        text.truncate(text.trim_end().len());
        text.push_str("\n\n");
    }

    /// Stream ended. Mount a UI tool if its args already parse; otherwise release held text.
    /// Pins the last computer PNG when `image.visibility` is not `agent` (turn-end
    /// `end` / untagged heuristic). Tagged `agent` stays Computer-pane only.
    pub fn finish(&mut self) {
        self.flush_text();
        if let Some(tool) = self.tool.take() {
            self.close_tool(tool);
        }
        self.pin_latest_shot();
    }

    /// Newest tool PNG, including steps that did not earn a chat row.
    pub fn latest_screenshot(&self) -> Option<&ScreenshotSpec> {
        self.latest_shot.as_ref()
    }

    fn close_tool(&mut self, tool: OpenTool) {
        let Ok(value) = serde_json::from_str::<Value>(&tool.args) else {
            return;
        };
        if is_user_form_tool(&tool.name) {
            self.form_args.insert(tool.id.clone(), tool.args.clone());
            if let Some(spec) = UserFormSpec::from_tool_args(&value, &tool.id) {
                self.push_user_form(spec);
            }
            return;
        }
        if let Some(spec) = UiSpec::from_tool(&tool.name, &value) {
            self.push_ui(spec, tool.id, tool.name);
        }
    }

    fn push_user_form(&mut self, spec: UserFormSpec) {
        if let Some(existing) = self.committed.iter_mut().find_map(|part| match part {
            ChatPart::UserForm(existing) if existing.same_card(&spec) => Some(existing),
            _ => None,
        }) {
            existing.merge(spec);
            place_hitl_cards_in_document_order(&mut self.committed);
            return;
        }
        // Awaiting CUSTOM has callId; a later send-message envelope has the
        // gateway id and no callId — fold those. A second OTP form after a
        // password Submitted is a different entry / call: push a new card.
        let mut unresolved_ix = None;
        let mut unresolved_count = 0usize;
        for (i, part) in self.committed.iter().enumerate() {
            if let ChatPart::UserForm(existing) = part
                && existing.is_unresolved()
            {
                unresolved_count += 1;
                unresolved_ix = Some(i);
            }
        }
        if unresolved_count == 1
            && let Some(i) = unresolved_ix
            && let ChatPart::UserForm(existing) = &mut self.committed[i]
            && existing.completes_with(&spec)
        {
            existing.merge(spec);
            place_hitl_cards_in_document_order(&mut self.committed);
            return;
        }
        self.committed.push(ChatPart::UserForm(spec));
        place_hitl_cards_in_document_order(&mut self.committed);
    }

    fn push_save_login(&mut self, spec: SaveLoginSpec) {
        if let Some(existing) = self.committed.iter_mut().find_map(|part| match part {
            ChatPart::SaveLogin(existing)
                if existing.form_entry_id == spec.form_entry_id
                    || (existing.origin == spec.origin && existing.username == spec.username) =>
            {
                Some(existing)
            }
            _ => None,
        }) {
            *existing = spec;
            return;
        }
        self.committed.push(ChatPart::SaveLogin(spec));
        place_hitl_cards_in_document_order(&mut self.committed);
    }

    fn push_credential_request(&mut self, spec: CredentialRequestSpec) {
        if let Some(existing) = self.committed.iter_mut().find_map(|part| match part {
            ChatPart::CredentialRequest(existing) if existing.request_id == spec.request_id => {
                Some(existing)
            }
            _ => None,
        }) {
            if existing.is_settled() {
                return;
            }
            let resolution = existing.resolution;
            *existing = spec;
            if existing.resolution.is_none() {
                existing.resolution = resolution;
            }
            return;
        }
        self.committed.push(ChatPart::CredentialRequest(spec));
        place_hitl_cards_in_document_order(&mut self.committed);
    }

    pub fn take_completed_ui_tools(&mut self) -> Vec<CompletedUiTool> {
        std::mem::take(&mut self.completed_ui)
    }

    fn push_ui(&mut self, spec: UiSpec, id: String, name: String) {
        let chart = matches!(spec, UiSpec::BarChart(_));
        self.committed.retain(|part| match part {
            ChatPart::Ui(UiSpec::BarChart(_)) => !chart,
            ChatPart::Ui(UiSpec::Form(_)) => chart,
            ChatPart::Text(_)
            | ChatPart::Approval(_)
            | ChatPart::Screenshot(_)
            | ChatPart::UserForm(_)
            | ChatPart::SaveLogin(_)
            | ChatPart::CredentialRequest(_) => true,
        });
        self.committed.push(ChatPart::Ui(spec));
        self.completed_ui.retain(|tool| tool.name != name);
        self.completed_ui.push(CompletedUiTool { id, name });
    }

    fn holding_ui(&self) -> bool {
        self.tool.is_some() || ui_object_incomplete(&self.text)
    }

    fn flush_text(&mut self) {
        drain_complete_ui(&mut self.text, &mut self.committed);
    }
}

impl UiSpec {
    /// The widget as a value `from_value` reads back unchanged — what the database keeps, so a
    /// thread read back off disk mounts the same form or chart the stream did.
    pub fn to_value(&self) -> Value {
        match self {
            Self::Form(form) => serde_json::json!({
                "component": "form",
                "title": form.title,
                "prompt": form.prompt,
                "submit": form.submit,
                "fields": form.fields.iter().map(|field| serde_json::json!({
                    "id": field.id,
                    "label": field.label,
                    "options": field.options,
                })).collect::<Vec<_>>(),
            }),
            Self::BarChart(chart) => serde_json::json!({
                "component": "bar-chart",
                "title": chart.title,
                "bars": chart.bars.iter().map(|bar| serde_json::json!({
                    "label": bar.label,
                    "value": bar.value,
                })).collect::<Vec<_>>(),
            }),
        }
    }

    pub fn from_value(value: &Value) -> Option<Self> {
        let kind = ui_kind(value)?;
        match kind.as_str() {
            "bar-chart" | "barchart" => Some(Self::BarChart(BarChartSpec::from_value(value)?)),
            "form" => Some(Self::Form(FormSpec::from_value(value)?)),
            _ => None,
        }
    }

    fn from_tool(name: &str, value: &Value) -> Option<Self> {
        if let Some(spec) = Self::from_value(value) {
            return Some(spec);
        }
        match normalize_name(name).as_str() {
            "bar-chart" | "barchart" => Some(Self::BarChart(BarChartSpec::from_value(value)?)),
            "form" => Some(Self::Form(FormSpec::from_value(value)?)),
            _ => None,
        }
    }

    fn from_custom(name: &str, value: &Value) -> Option<Self> {
        if normalize_name(name) == "ui" || name.is_empty() {
            return Self::from_value(value);
        }
        Self::from_tool(name, value)
    }
}

impl BarChartSpec {
    fn from_value(value: &Value) -> Option<Self> {
        let data = value.get("data").unwrap_or(value);
        let rows = data
            .get("bars")
            .or_else(|| data.get("values"))
            .or_else(|| value.get("bars"))?;
        let bars = parse_bars(rows);
        if bars.is_empty() {
            return None;
        }
        Some(Self {
            title: string_field(data, "title").or_else(|| string_field(value, "title")),
            bars,
        })
    }
}

impl FormSpec {
    fn from_value(value: &Value) -> Option<Self> {
        let data = value.get("data").unwrap_or(value);
        let fields = data
            .get("fields")
            .or_else(|| value.get("fields"))
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .enumerate()
                    .filter_map(|(i, row)| parse_field(row, i))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if fields.is_empty() {
            return None;
        }
        Some(Self {
            title: string_field(data, "title").or_else(|| string_field(value, "title")),
            prompt: string_field(data, "prompt").or_else(|| string_field(value, "prompt")),
            fields,
            submit: string_field(data, "submit")
                .or_else(|| string_field(value, "submit"))
                .unwrap_or_else(|| "Send".to_string()),
        })
    }
}

fn parse_field(row: &Value, index: usize) -> Option<FormField> {
    let label = string_field(row, "label").or_else(|| string_field(row, "name"))?;
    let id = string_field(row, "id").unwrap_or_else(|| format!("field-{index}"));
    let options = row
        .get("options")
        .and_then(Value::as_array)
        .map(|opts| {
            opts.iter()
                .filter_map(|opt| {
                    opt.as_str()
                        .map(str::to_string)
                        .or_else(|| string_field(opt, "label"))
                        .or_else(|| string_field(opt, "value"))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(FormField { id, label, options })
}

fn parse_bars(rows: &Value) -> Vec<BarItem> {
    let Some(rows) = rows.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            if let Some(arr) = row.as_array() {
                let label = arr.first().and_then(Value::as_str)?.to_string();
                let value = arr.get(1).and_then(json_f32)?;
                return Some(BarItem { label, value });
            }
            let label = string_field(row, "label")
                .or_else(|| string_field(row, "name"))
                .unwrap_or_default();
            let value = row
                .get("value")
                .or_else(|| row.get("y"))
                .and_then(json_f32)?;
            if label.is_empty() {
                return None;
            }
            Some(BarItem { label, value })
        })
        .collect()
}

fn json_f32(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|n| n as f32)
        .or_else(|| value.as_i64().map(|n| n as f32))
        .or_else(|| value.as_u64().map(|n| n as f32))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn ui_kind(value: &Value) -> Option<String> {
    if let Some(kind) = value
        .get("ui")
        .or_else(|| value.get("component"))
        .and_then(Value::as_str)
    {
        return Some(normalize_name(kind));
    }
    let ty = value.get("type").and_then(Value::as_str)?;
    if ty.eq_ignore_ascii_case("ui") {
        return value
            .get("name")
            .and_then(Value::as_str)
            .map(normalize_name);
    }
    Some(normalize_name(ty))
}

fn is_ui_tool(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "bar-chart"
            | "barchart"
            | "show-bar-chart"
            | "render-bar-chart"
            | "form"
            | "show-form"
            | "render-form"
    )
}

fn normalize_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
}

fn drain_complete_ui(buf: &mut String, out: &mut Vec<ChatPart>) {
    loop {
        let Some(start) = find_ui_object(buf) else {
            if !buf.is_empty() {
                out.push(ChatPart::Text(std::mem::take(buf)));
            }
            return;
        };
        let Some(len) = complete_json_len(&buf[start..]) else {
            return;
        };
        if start > 0 {
            out.push(ChatPart::Text(buf[..start].to_string()));
        }
        let end = start + len;
        let raw = buf[start..end].to_string();
        buf.replace_range(..end, "");
        match serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|v| UiSpec::from_value(&v))
        {
            Some(spec) => out.push(ChatPart::Ui(spec)),
            None => out.push(ChatPart::Text(raw)),
        }
    }
}

fn ui_object_incomplete(buf: &str) -> bool {
    match find_ui_object(buf) {
        None => false,
        Some(start) => complete_json_len(&buf[start..]).is_none(),
    }
}

fn find_ui_object(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let rest = s[i..].trim_start();
            if rest.starts_with("{\"ui\"")
                || rest.starts_with("{ \"ui\"")
                || rest.starts_with("{\"component\"")
                || rest.starts_with("{ \"component\"")
            {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn complete_json_len(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'{') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if escape {
                escape = false;
                continue;
            }
            if b == b'\\' {
                escape = true;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// A turn's words, with the things that are not words left out.
///
/// Text arrives in deltas and each one is committed as its own part, so neighbouring text parts
/// are halves of the same sentence and are joined with nothing between them. A picture or a card
/// between them is not: the stream broke the text there, and the person read what came after as
/// a new bubble, so those keep a blank line between them. Joining those too was what turned a
/// recipe run into "…using the taught recipe.YouTube is open on my box…", one run-on paragraph
/// in the feed and in the history the model is sent next turn.
///
/// A chart is the exception: it is cut out of the middle of a sentence that was streamed whole,
/// and "See this <chart> and more" is one sentence with a picture in it.
fn plain_text(parts: &[ChatPart]) -> String {
    let mut out = String::new();
    let mut run = String::new();
    for part in parts {
        match part {
            ChatPart::Text(text) => run.push_str(text),
            ChatPart::Ui(_) => {}
            ChatPart::Approval(_)
            | ChatPart::Screenshot(_)
            | ChatPart::UserForm(_)
            | ChatPart::SaveLogin(_)
            | ChatPart::CredentialRequest(_) => push_run(&mut out, std::mem::take(&mut run)),
        }
    }
    push_run(&mut out, run);
    out
}

/// One bubble's worth of words onto the end of the turn, a blank line after the last.
fn push_run(out: &mut String, run: String) {
    if run.trim().is_empty() {
        return;
    }
    if out.is_empty() {
        out.push_str(&run);
        return;
    }
    out.truncate(out.trim_end().len());
    out.push_str("\n\n");
    out.push_str(run.trim_start());
}

pub fn approval_from_event(event: &Value) -> Option<ApprovalSpec> {
    let run_id = string_at(event, "runId")?;
    let call_id = string_at(event, "callId")?;
    if run_id.is_empty() || call_id.is_empty() {
        return None;
    }
    let tool = string_at(event, "tool").unwrap_or_else(|| "a tool".to_string());
    let arguments = event
        .get("arguments")
        .cloned()
        .or_else(|| {
            event
                .get("value")
                .and_then(|value| value.get("arguments"))
                .cloned()
        })
        .unwrap_or(Value::Null);
    Some(ApprovalSpec {
        run_id,
        thread_id: string_at(event, "threadId").filter(|id| !id.trim().is_empty()),
        call_id,
        command: command_from_args(&arguments),
        why: string_at(event, "why").unwrap_or_default(),
        reason: string_at(event, "reason").unwrap_or_else(|| "exec-consent".to_string()),
        tool,
        output: None,
        ok: None,
    })
}

/// Centered status Grok paints after the permission card leaves the transcript.
/// `place` is [`ApprovalSpec::place`].
pub fn local_exec_outcome(bot: &str, resolution: LocalExecResolution, place: &str) -> String {
    match resolution {
        LocalExecResolution::Always => format!("{bot} can run commands on {place}."),
        LocalExecResolution::Never => format!("{bot} cannot run commands on {place}."),
        LocalExecResolution::DenyOnce => {
            format!("{bot} was not allowed to run commands on {place}.")
        }
        LocalExecResolution::AllowOnce => {
            format!("{bot} can run commands on {place} this time.")
        }
    }
}

/// The same line for a card the MCP door raised.
///
/// The local-shell wording is about a machine and a policy that outlives the
/// answer — "can run commands on your computer" — and none of that is what was
/// answered here: one call, by one tool, once. There is no policy to move
/// either, which is why an MCP card has no Always and no Never to reach this
/// with; the two that cannot happen read as the once they would have been.
pub fn mcp_call_outcome(bot: &str, resolution: LocalExecResolution, tool: &str) -> String {
    match resolution {
        LocalExecResolution::AllowOnce | LocalExecResolution::Always => {
            format!("{bot} may run {tool} once.")
        }
        LocalExecResolution::DenyOnce | LocalExecResolution::Never => {
            format!("{bot} was told no.")
        }
    }
}

pub fn command_from_replay_events(events: &[Value], call_id: &str) -> String {
    let mut args = String::new();
    for event in events {
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        let tool_id = event
            .get("toolCallId")
            .or_else(|| event.get("id"))
            .or_else(|| event.get("callId"))
            .and_then(Value::as_str);
        if matches!(kind, "TOOL_CALL_ARGS" | "TOOL_CALL_CHUNK") && tool_id == Some(call_id) {
            if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                args.push_str(delta);
            }
            let from_args = event
                .get("arguments")
                .map(command_from_args)
                .unwrap_or_default();
            if !from_args.is_empty() {
                return from_args;
            }
        }
        if kind == "CUSTOM"
            && event.get("name").and_then(Value::as_str) == Some("run-awaiting-approval")
            && event.get("callId").and_then(Value::as_str) == Some(call_id)
        {
            let from_args = event
                .get("arguments")
                .or_else(|| event.get("args"))
                .map(command_from_args)
                .unwrap_or_default();
            if !from_args.is_empty() {
                return from_args;
            }
        }
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(&args) {
        let command = command_from_args(&parsed);
        if !command.is_empty() {
            return command;
        }
    }
    args.trim().to_string()
}

pub fn command_from_args(arguments: &Value) -> String {
    for key in ["command", "cmd", "shell", "script"] {
        if let Some(command) = arguments.get(key).and_then(Value::as_str) {
            let command = command.trim();
            if !command.is_empty() {
                return command.to_string();
            }
        }
    }
    if let Some(raw) = arguments.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
            return command_from_args(&parsed);
        }
        return raw.to_string();
    }
    if arguments.is_null() || arguments.as_object().is_some_and(|map| map.is_empty()) {
        return String::new();
    }
    arguments.to_string()
}

fn string_at(event: &Value, key: &str) -> Option<String> {
    event
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            event
                .get("value")
                .and_then(|value| value.get(key))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

/// What the next `/ag-ui` run is told after NativeChat paints a frontend tool.
pub const UI_TOOL_RESULT: &str = "It is now on screen for the person.";

pub const MAX_TURN_CONTINUES: usize = 8;

pub fn agui_tools() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name": "bar_chart",
            "description": "Render an interactive bar chart in the chat instead of markdown or ASCII.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "bars": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string" },
                                "value": { "type": "number" }
                            },
                            "required": ["label", "value"]
                        }
                    }
                },
                "required": ["bars"]
            }
        }),
        serde_json::json!({
            "name": "form",
            "description": "Render an interactive choice form in the chat instead of a bullet list.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "prompt": { "type": "string" },
                    "submit": { "type": "string" },
                    "fields": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "label": { "type": "string" },
                                "options": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["label", "options"]
                        }
                    }
                },
                "required": ["fields"]
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(delta: &str) -> Value {
        json!({"type":"TEXT_MESSAGE_CONTENT","delta":delta})
    }

    /// A generative form or chart is saved as the value it was parsed from, so a thread read
    /// back from the database mounts the same widget the stream did.
    #[test]
    fn a_form_and_a_chart_survive_the_trip_through_their_own_value() {
        let form = UiSpec::Form(FormSpec {
            title: Some("QA collapse remasure".into()),
            prompt: Some("Enter a label.".into()),
            fields: vec![
                FormField {
                    id: "label".into(),
                    label: "Label".into(),
                    options: vec![],
                },
                FormField {
                    id: "size".into(),
                    label: "Size".into(),
                    options: vec!["S".into(), "M".into()],
                },
            ],
            submit: "Submit".into(),
        });
        assert_eq!(UiSpec::from_value(&form.to_value()), Some(form));

        let chart = UiSpec::BarChart(BarChartSpec {
            title: Some("Visits".into()),
            bars: vec![BarItem {
                label: "Mon".into(),
                value: 3.5,
            }],
        });
        assert_eq!(UiSpec::from_value(&chart.to_value()), Some(chart));
    }

    #[test]
    fn text_deltas_stream_before_the_run_ends() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("Hello "));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "Hello ");
        assert_eq!(parts, vec![ChatPart::Text("Hello ".into())]);
    }

    #[test]
    fn incomplete_ui_json_is_held_not_painted() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("See this {\"ui\":\"bar-chart\",\"bars\":["));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "");
        assert!(parts.is_empty());
    }

    #[test]
    fn a_ui_tool_in_flight_does_not_stream() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type":"TOOL_CALL_START",
            "toolCallId":"c1",
            "toolCallName":"bar_chart"
        }));
        turn.push_event(&text("should not appear yet"));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "");
        assert!(parts.is_empty());
        turn.push_event(&json!({
            "type":"TOOL_CALL_ARGS",
            "toolCallId":"c1",
            "delta":"{\"title\":\"Q3\",\"bars\":[{\"label\":\"A\",\"value\":3}]}"
        }));
        turn.push_event(&json!({"type":"TOOL_CALL_END","toolCallId":"c1"}));
        let (_, parts) = turn.snapshot();
        assert!(matches!(parts.first(), Some(ChatPart::Text(t)) if t == "should not appear yet"));
        assert!(matches!(
            parts.get(1),
            Some(ChatPart::Ui(UiSpec::BarChart(_)))
        ));
        let done = turn.take_completed_ui_tools();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, "c1");
    }

    #[test]
    fn a_complete_bar_chart_object_mounts_without_waiting_for_later_text() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text(
            "See this {\"ui\":\"bar-chart\",\"title\":\"Q3\",\"bars\":[{\"label\":\"A\",\"value\":10}]} and more",
        ));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "See this  and more");
        assert!(matches!(parts[0], ChatPart::Text(ref t) if t == "See this "));
        assert!(matches!(parts[1], ChatPart::Ui(UiSpec::BarChart(_))));
        assert!(matches!(parts[2], ChatPart::Text(ref t) if t == " and more"));
    }

    #[test]
    fn a_tool_call_chart_mounts_when_args_are_complete() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("Here:"));
        turn.push_event(&json!({
            "type":"TOOL_CALL_START",
            "toolCallId":"c1",
            "toolCallName":"bar_chart"
        }));
        turn.push_event(&json!({
            "type":"TOOL_CALL_ARGS",
            "toolCallId":"c1",
            "delta":"{\"title\":\"Q3\",\"bars\":[{\"label\":\"A\",\"value\":3}]}"
        }));
        let (_, mid) = turn.snapshot();
        assert!(mid.iter().all(|p| !matches!(p, ChatPart::Ui(_))));
        turn.push_event(&json!({"type":"TOOL_CALL_END","toolCallId":"c1"}));
        let (_, parts) = turn.snapshot();
        assert!(matches!(
            parts.last(),
            Some(ChatPart::Ui(UiSpec::BarChart(_)))
        ));
    }

    #[test]
    fn a_custom_form_event_mounts_as_a_widget() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type":"CUSTOM",
            "name":"ui",
            "value":{
                "component":"form",
                "title":"Next",
                "fields":[{"id":"go","label":"Go","options":["Yes","No"]}],
                "submit":"Send"
            }
        }));
        let (_, parts) = turn.snapshot();
        assert!(matches!(parts.as_slice(), [ChatPart::Ui(UiSpec::Form(_))]));
    }

    #[test]
    fn curly_braces_in_prose_are_not_held() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("The set {a, b} is finite."));
        let (plain, _) = turn.snapshot();
        assert_eq!(plain, "The set {a, b} is finite.");
    }

    #[test]
    fn command_from_replay_reads_tool_args_and_custom() {
        let events = vec![
            json!({
                "type": "TOOL_CALL_ARGS",
                "toolCallId": "c1",
                "delta": "{\"command\":\"ls /Volumes/goldcoders\"}"
            }),
            json!({
                "type": "CUSTOM",
                "name": "run-awaiting-approval",
                "callId": "c1",
                "arguments": null
            }),
        ];
        assert_eq!(
            command_from_replay_events(&events, "c1"),
            "ls /Volumes/goldcoders"
        );
        let custom = vec![json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "c2",
            "arguments": {"command": "pwd"}
        })];
        assert_eq!(command_from_replay_events(&custom, "c2"), "pwd");
    }

    #[test]
    fn shell_args_fill_an_approval_card_command() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TOOL_CALL_START",
            "toolCallId": "call-9",
            "toolCallName": "user_machine_shell"
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_ARGS",
            "toolCallId": "call-9",
            "delta": "{\"command\":\"ls /Volumes/goldcoders\"}"
        }));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": "user_machine_shell"
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::Approval(spec)] => {
                assert_eq!(spec.command, "ls /Volumes/goldcoders");
                assert_eq!(spec.run_id, "run-1");
            }
            other => panic!("expected approval with command, got {other:?}"),
        }
    }

    #[test]
    fn run_awaiting_approval_is_a_card_not_a_sentence() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("I'll check that path."));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "threadId": "t1",
            "callId": "call-9",
            "tool": "user_machine_shell",
            "arguments": {"command": "ls /Volumes/goldcoders/examples"},
            "reason": "exec-consent",
            "why": "your machine's owner must approve this command"
        }));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "I'll check that path.");
        assert!(turn.waiting_approval());
        match parts.as_slice() {
            [ChatPart::Text(text), ChatPart::Approval(spec)] => {
                assert_eq!(text, "I'll check that path.");
                assert_eq!(spec.run_id, "run-1");
                assert_eq!(spec.call_id, "call-9");
                assert_eq!(spec.tool, "user_machine_shell");
                assert_eq!(spec.command, "ls /Volumes/goldcoders/examples");
                assert_eq!(spec.reason, "exec-consent");
            }
            other => panic!("expected text + approval card, got {other:?}"),
        }
    }

    #[test]
    fn a_finished_run_is_no_longer_waiting() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": "user_machine_shell",
            "arguments": {"command": "ls"}
        }));
        assert!(turn.waiting_approval());
        turn.push_event(&json!({"type":"RUN_FINISHED"}));
        assert!(!turn.waiting_approval());
    }

    fn ask(call_id: &str, command: &str) -> ChatPart {
        ChatPart::Approval(ApprovalSpec {
            run_id: "r".into(),
            thread_id: None,
            call_id: call_id.into(),
            tool: "user_machine_shell".into(),
            command: command.into(),
            why: String::new(),
            reason: "exec-consent".into(),
            output: None,
            ok: None,
        })
    }

    #[test]
    fn extra_open_approval_cards_are_dropped() {
        let parts = vec![
            ChatPart::Text("ok".into()),
            ask("old", "ls"),
            ask("new", "uname"),
        ];
        let open = HashSet::from(["old".to_string(), "new".to_string()]);
        let out = collapse_open_approvals(&parts, &open);
        assert_eq!(out.len(), 2);
        assert!(matches!(&out[0], ChatPart::Text(t) if t == "ok"));
        assert!(matches!(&out[1], ChatPart::Approval(s) if s.call_id == "new"));
    }

    #[test]
    fn a_refused_shell_result_is_not_a_permission_card() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TOOL_CALL_START",
            "toolCallId": "call_missing_args",
            "toolCallName": "user_machine_shell"
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_END",
            "toolCallId": "call_missing_args"
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "call_missing_args",
            "ok": false,
            "content": "refused: bad arguments: missing field `command`"
        }));
        let (_, parts) = turn.snapshot();
        assert!(
            parts
                .iter()
                .all(|part| !matches!(part, ChatPart::Approval(_))),
            "a refusal is not a command to approve, got {parts:?}"
        );
        assert!(!turn.waiting_approval());
    }

    #[test]
    fn tool_result_stdout_lands_on_the_approval_card() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": "user_machine_shell",
            "arguments": {"command": "ls"}
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "call-9",
            "ok": true,
            "content": "exit 0\n--- stdout ---\nhello\n"
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::Approval(spec)] => {
                assert_eq!(spec.ok, Some(true));
                assert_eq!(
                    spec.output.as_deref(),
                    Some("exit 0\n--- stdout ---\nhello\n")
                );
            }
            other => panic!("expected approval with stdout, got {other:?}"),
        }
        assert!(!turn.waiting_approval());
    }

    #[test]
    fn local_exec_outcome_matches_grok_copy() {
        let here = "your computer";
        assert_eq!(
            local_exec_outcome("Hexuria", LocalExecResolution::AllowOnce, here),
            "Hexuria can run commands on your computer this time."
        );
        assert_eq!(
            local_exec_outcome("Hexuria", LocalExecResolution::Always, here),
            "Hexuria can run commands on your computer."
        );
        assert_eq!(
            local_exec_outcome("Hexuria", LocalExecResolution::DenyOnce, here),
            "Hexuria was not allowed to run commands on your computer."
        );
        assert_eq!(
            local_exec_outcome("Hexuria", LocalExecResolution::Never, here),
            "Hexuria cannot run commands on your computer."
        );
        assert_eq!(
            local_exec_outcome("Hexuria", LocalExecResolution::AllowOnce, "its computer"),
            "Hexuria can run commands on its computer this time."
        );
    }

    fn approval_for(tool: &str) -> ApprovalSpec {
        ApprovalSpec {
            run_id: "r1".into(),
            thread_id: None,
            call_id: "c1".into(),
            tool: tool.into(),
            command: "ls".into(),
            why: String::new(),
            reason: "exec-consent".into(),
            output: None,
            ok: None,
        }
    }

    #[test]
    fn only_the_local_shell_tool_runs_on_this_mac() {
        let local = approval_for(USER_MACHINE_SHELL);
        let boxed = approval_for("Shell");
        assert!(local.runs_on_this_mac());
        assert!(!boxed.runs_on_this_mac());
        assert_eq!(local.place(), "your computer");
        assert_eq!(boxed.place(), "its computer");
        let mut review = boxed.clone();
        review.reason = "auto-review".into();
        assert!(review.is_review_an_action());
        assert!(!local.is_review_an_action());
    }

    fn mcp_card() -> ApprovalSpec {
        let mut spec = approval_for("read_file");
        spec.thread_id = Some("mcp-cw_1".into());
        spec.reason = "policy-approval".into();
        spec
    }

    /// An MCP card is one call being let through, not a machine being let
    /// loose, and it says so where the card was.
    #[test]
    fn an_mcp_card_leaves_behind_what_was_actually_answered() {
        let spec = mcp_card();
        assert!(spec.is_mcp());
        assert_eq!(
            spec.outcome("Hexuria", LocalExecResolution::AllowOnce),
            "Hexuria may run read_file once."
        );
        assert_eq!(
            spec.outcome("Hexuria", LocalExecResolution::DenyOnce),
            "Hexuria was told no."
        );
        // The local shell keeps every word it had.
        let local = approval_for(USER_MACHINE_SHELL);
        assert!(!local.is_mcp());
        assert_eq!(
            local.outcome("Hexuria", LocalExecResolution::AllowOnce),
            "Hexuria can run commands on your computer this time."
        );
        assert_eq!(
            local.outcome("Hexuria", LocalExecResolution::Always),
            "Hexuria can run commands on your computer."
        );
    }

    /// Review chrome is Always allow, and Always writes a standing policy for
    /// this Mac. An MCP card has no such thing behind it, whatever word the
    /// door used for why it stopped the call.
    #[test]
    fn an_mcp_card_is_never_review_an_action() {
        let mut spec = mcp_card();
        spec.reason = "auto-review".into();
        assert!(!spec.is_review_an_action());

        let mut box_tool = approval_for("Shell");
        box_tool.reason = "auto-review".into();
        assert!(box_tool.is_review_an_action());
    }

    #[test]
    fn machine_policy_never_answers_for_a_box_tool() {
        let local = approval_for(USER_MACHINE_SHELL);
        let boxed = approval_for("Shell");
        assert_eq!(
            policy_answer(&local, Some(LocalExecMode::Always)),
            Some(LocalExecResolution::Always)
        );
        assert_eq!(
            policy_answer(&local, Some(LocalExecMode::Never)),
            Some(LocalExecResolution::Never)
        );
        assert_eq!(policy_answer(&local, Some(LocalExecMode::Ask)), None);
        assert_eq!(policy_answer(&local, None), None);
        assert_eq!(policy_answer(&boxed, Some(LocalExecMode::Always)), None);
        assert_eq!(policy_answer(&boxed, Some(LocalExecMode::Never)), None);
    }

    /// A 1x1 transparent PNG: enough bytes to be a picture, small enough to read.
    const TINY_PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    #[test]
    fn a_tool_result_with_a_picture_is_a_screenshot_row() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({"type":"TEXT_MESSAGE_CONTENT","delta":"Looking."}));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "c9",
            "content": "clicking at 10,10; screenshot of the 1280x800 screen attached",
            "ok": true,
            "image": {"mime": "image/png", "base64": TINY_PNG, "width": 1280, "height": 800}
        }));
        turn.finish();
        let (_, parts) = turn.snapshot();
        let shot = parts
            .iter()
            .find_map(|part| match part {
                ChatPart::Screenshot(spec) => Some(spec),
                _ => None,
            })
            .expect("a screenshot part");
        assert_eq!(shot.call_id, "c9");
        assert_eq!((shot.width, shot.height), (1280, 800));
        assert!(shot.caption.starts_with("clicking at 10,10"));
        assert!(!shot.image.bytes.is_empty());
        // Words before the picture stay words.
        assert!(matches!(parts.first(), Some(ChatPart::Text(text)) if text.contains("Looking.")));
    }

    #[test]
    fn success_tool_images_wait_until_turn_end() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({"type":"TEXT_MESSAGE_CONTENT","delta":"Working."}));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "c1",
            "content": "click 1",
            "ok": true,
            "image": {"mime": "image/png", "base64": TINY_PNG, "width": 1280, "height": 800}
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "c2",
            "content": "click 2",
            "ok": true,
            "image": {"mime": "image/png", "base64": TINY_PNG, "width": 640, "height": 480}
        }));
        let (_, mid) = turn.snapshot();
        assert!(
            !mid.iter()
                .any(|part| matches!(part, ChatPart::Screenshot(_))),
            "intermediate success PNGs are Computer-pane only: {mid:?}"
        );
        assert_eq!(
            turn.latest_screenshot().map(|s| s.call_id.as_str()),
            Some("c2")
        );
        turn.finish();
        let (_, parts) = turn.snapshot();
        let shots: Vec<_> = parts
            .iter()
            .filter_map(|part| match part {
                ChatPart::Screenshot(spec) => Some(spec.call_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            shots,
            vec!["c2"],
            "turn end pins the last screen, not every step"
        );
    }

    #[test]
    fn a_failed_tool_image_is_a_screenshot_row() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "c-fail",
            "content": "click missed",
            "ok": false,
            "image": {"mime": "image/png", "base64": TINY_PNG, "width": 10, "height": 10}
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::Screenshot(spec)] => assert_eq!(spec.call_id, "c-fail"),
            other => panic!("failure pins immediately, got {other:?}"),
        }
    }

    /// The giveaway of the bug the person reported: two things the coworker said either side of
    /// a picture were glued into "…using the taught recipe.YouTube is open…" the moment the turn
    /// was flattened to text. Deltas of one sentence still join with nothing between them.
    #[test]
    fn words_either_side_of_a_picture_are_two_paragraphs() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TEXT_MESSAGE_CONTENT", "delta": "I'll open YouTube on my box "
        }));
        turn.push_event(&json!({
            "type": "TEXT_MESSAGE_CONTENT", "delta": "using the taught recipe."
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "c1",
            "content": "ran recipe \"youtube\" (v2): 6 steps",
            "ok": true,
            "image": {"mime": "image/png", "base64": TINY_PNG, "width": 1280, "height": 800}
        }));
        turn.push_event(&json!({
            "type": "TEXT_MESSAGE_CONTENT", "delta": "YouTube is open on my box."
        }));
        turn.finish();
        let (plain, _) = turn.snapshot();
        assert_eq!(
            plain,
            "I'll open YouTube on my box using the taught recipe.\n\nYouTube is open on my box."
        );
    }

    #[test]
    fn a_result_without_a_png_is_not_a_screenshot() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "c1",
            "content": "wrote /tmp/x",
            "ok": true,
            "image": {"mime": "image/jpeg", "base64": TINY_PNG, "width": 1, "height": 1}
        }));
        turn.finish();
        let (_, parts) = turn.snapshot();
        assert!(
            !parts
                .iter()
                .any(|part| matches!(part, ChatPart::Screenshot(_)))
        );
    }

    fn png_image(visibility: Option<&str>) -> Value {
        let mut image = json!({
            "mime": "image/png",
            "base64": TINY_PNG,
            "width": 1280,
            "height": 800
        });
        if let Some(visibility) = visibility {
            image["visibility"] = json!(visibility);
        }
        image
    }

    #[test]
    fn agent_visibility_stays_off_the_feed_and_on_the_computer_pane() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({"type":"TEXT_MESSAGE_CONTENT","delta":"Working."}));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "step-1",
            "content": "click 1",
            "ok": true,
            "image": png_image(Some("agent"))
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "step-2",
            "content": "click 2",
            "ok": true,
            "image": png_image(Some("agent"))
        }));
        let (_, mid) = turn.snapshot();
        assert!(
            !mid.iter()
                .any(|part| matches!(part, ChatPart::Screenshot(_))),
            "agent shots are Computer-pane only: {mid:?}"
        );
        assert_eq!(
            turn.latest_screenshot().map(|s| s.call_id.as_str()),
            Some("step-2")
        );
        assert_eq!(
            turn.latest_screenshot().and_then(|s| s.visibility),
            Some(ImageVisibility::Agent)
        );
        turn.finish();
        let (_, parts) = turn.snapshot();
        assert!(
            !parts
                .iter()
                .any(|part| matches!(part, ChatPart::Screenshot(_))),
            "finish must not pin agent: {parts:?}"
        );
    }

    #[test]
    fn transcript_failure_end_visibility_pin_immediately() {
        for (visibility, call_id) in [
            ("transcript", "observe"),
            ("failure", "stuck"),
            ("end", "turn-end"),
        ] {
            let mut turn = TurnAssembler::default();
            turn.push_event(&json!({
                "type": "TOOL_CALL_RESULT",
                "toolCallId": call_id,
                "content": visibility,
                "ok": true,
                "image": png_image(Some(visibility))
            }));
            let (_, parts) = turn.snapshot();
            match parts.as_slice() {
                [ChatPart::Screenshot(spec)] => {
                    assert_eq!(spec.call_id, call_id);
                    assert_eq!(
                        spec.visibility,
                        ImageVisibility::parse(visibility),
                        "{visibility}"
                    );
                }
                other => panic!("{visibility} should pin before finish, got {other:?}"),
            }
        }
    }

    #[test]
    fn journalled_agent_frame_without_bytes_is_not_a_chat_row() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "step-1",
            "content": "click",
            "ok": true,
            "image": {
                "mime": "image/png",
                "width": 1280,
                "height": 800,
                "visibility": "agent"
            }
        }));
        turn.finish();
        let (_, parts) = turn.snapshot();
        assert!(
            !parts
                .iter()
                .any(|part| matches!(part, ChatPart::Screenshot(_)))
        );
        assert!(turn.latest_screenshot().is_none());
    }

    fn user_form_custom(entry: &str, request: Value, resolution: Value) -> Value {
        json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "entryId": entry,
                "formRequest": request,
                "formResolution": resolution
            }
        })
    }

    fn password_request() -> Value {
        json!({
            "title": "Google password",
            "instruction": "Enter the password for that account.",
            "fields": [
                {
                    "id": "password",
                    "label": "Password",
                    "type": "password",
                    "required": true,
                    "value": "s3cret-pass"
                }
            ]
        })
    }

    #[test]
    fn a_user_form_custom_is_not_a_generative_form() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&user_form_custom(
            "entry-pw",
            password_request(),
            Value::Null,
        ));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "", "field values must not become chat text");
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.title, "Google password");
                assert!(spec.fields[0].masked());
                assert!(spec.fields[0].prefill.is_none());
                assert!(spec.is_unresolved());
            }
            other => panic!("expected UserForm, got {other:?}"),
        }
        assert!(
            parts.iter().all(|part| !matches!(part, ChatPart::Ui(_))),
            "user-form must not mount as UiSpec::Form: {parts:?}"
        );
        let dump = format!("{parts:?}");
        assert!(
            !dump.contains("s3cret-pass"),
            "password must not appear in the assembler snapshot: {dump}"
        );
        assert!(turn.waiting_user_form());
        assert!(!turn.waiting_approval());
    }

    #[test]
    fn form_resolution_settles_the_same_entry() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&user_form_custom(
            "entry-pw",
            password_request(),
            Value::Null,
        ));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": { "entryId": "entry-pw", "formResolution": "fill_failed" }
        }));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "");
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.pill(), Some("Not filled"));
                assert!(!spec.is_unresolved());
            }
            other => panic!("expected one settled card, got {other:?}"),
        }
        assert!(!turn.waiting_user_form());
    }

    #[test]
    fn request_user_form_tool_mounts_as_user_form() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TOOL_CALL_START",
            "toolCallId": "c-form",
            "toolCallName": "request_user_form"
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_ARGS",
            "toolCallId": "c-form",
            "delta": "{\"formRequest\":{\"title\":\"Code\",\"fields\":[{\"id\":\"otp\",\"label\":\"Code\",\"type\":\"otp\",\"required\":true,\"value\":\"654321\"}]}}"
        }));
        turn.push_event(&json!({"type":"TOOL_CALL_END","toolCallId":"c-form"}));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "");
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.fields[0].kind, crate::opengrok::UserFormFieldKind::Otp);
                assert!(spec.fields[0].prefill.is_none());
            }
            other => panic!("expected UserForm from tool, got {other:?}"),
        }
        let dump = format!("{parts:?}");
        assert!(
            !dump.contains("654321"),
            "otp must not appear in snapshot: {dump}"
        );
    }

    #[test]
    fn a_custom_form_event_is_still_generative_ui() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type":"CUSTOM",
            "name":"ui",
            "value":{
                "component":"form",
                "title":"Next",
                "fields":[{"id":"go","label":"Go","options":["Yes","No"]}],
                "submit":"Send"
            }
        }));
        let (_, parts) = turn.snapshot();
        assert!(matches!(parts.as_slice(), [ChatPart::Ui(UiSpec::Form(_))]));
        assert!(!turn.waiting_user_form());
    }

    #[test]
    fn user_form_does_not_use_formspec_submit_copy() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&user_form_custom(
            "e1",
            json!({
                "title": "Email",
                "fields": [{"id":"email","label":"Email","type":"email","required":true}]
            }),
            Value::Null,
        ));
        let (_, parts) = turn.snapshot();
        let dump = format!("{parts:?}");
        assert!(
            !dump.contains("Fill in the required field to continue"),
            "must not steal plugin-setup copy: {dump}"
        );
        assert!(!matches!(parts.as_slice(), [ChatPart::Ui(UiSpec::Form(_))]));
    }

    #[test]
    fn a_form_named_custom_is_still_generative_choice_chips() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type":"CUSTOM",
            "name":"form",
            "value":{
                "title":"Next",
                "fields":[{"id":"go","label":"Go","options":["Yes","No"]}],
                "submit":"Send"
            }
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::Ui(UiSpec::Form(spec))] => {
                assert_eq!(spec.title.as_deref(), Some("Next"));
                assert_eq!(spec.fields[0].options, vec!["Yes", "No"]);
            }
            other => panic!("expected FormSpec, got {other:?}"),
        }
        assert!(!turn.waiting_user_form());
    }

    #[test]
    fn official_send_message_envelope_mounts_as_user_form() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "kind": "send-message",
                "id": "entry-email",
                "message": {
                    "type": "user-form",
                    "formRequest": {
                        "title": "Google account email",
                        "instruction": "Enter the other Gmail address you want to sign in with.",
                        "fields": [
                            {"id": "email", "label": "Email or phone", "type": "email", "required": true}
                        ],
                        "domain": "accounts.google.com",
                        "liveHost": "accounts.google.com"
                    }
                },
                "formResolution": null
            }
        }));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "");
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.entry_id, "entry-email");
                assert_eq!(spec.title, "Google account email");
                assert!(spec.is_unresolved());
            }
            other => panic!("expected UserForm from official envelope, got {other:?}"),
        }
    }

    #[test]
    fn live_awaiting_user_form_is_not_an_approval_card() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": "request_user_form",
            "reason": "user-form",
            "why": "Waiting for you",
            "arguments": {
                "title": "Google account email",
                "fields": [
                    {"id": "email", "label": "Email", "type": "email", "required": true},
                    {"id": "password", "label": "Password", "type": "password", "required": true, "value": "s3cret-pass"}
                ],
                "liveHost": "accounts.google.com"
            }
        }));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "", "field values must not become chat text");
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.title, "Google account email");
                assert_eq!(spec.call_id, "call-9");
                assert_eq!(spec.run_id, "run-1");
                assert!(
                    spec.entry_id.is_empty(),
                    "AG-UI callId is not the gateway entryId: {}",
                    spec.entry_id
                );
                assert!(!spec.has_gateway_entry_id());
                assert!(spec.fields.iter().any(|f| f.masked()));
                assert!(spec.is_unresolved());
            }
            other => panic!("expected UserForm from awaiting, got {other:?}"),
        }
        assert!(turn.waiting_user_form());
        assert!(
            !turn.waiting_approval(),
            "user-form HITL is not a permission card"
        );
        let dump = format!("{parts:?}");
        assert!(
            !dump.contains("s3cret-pass"),
            "password must not appear in the assembler snapshot: {dump}"
        );
        assert!(
            parts
                .iter()
                .all(|part| !matches!(part, ChatPart::Approval(_))),
            "must not mount as Approval: {parts:?}"
        );
        assert!(
            parts.iter().all(|part| !matches!(part, ChatPart::Ui(_))),
            "must not mount as UiSpec::Form: {parts:?}"
        );
    }

    #[test]
    fn awaiting_with_entry_id_keeps_the_gateway_card_id() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "entryId": "e_form",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Sign in",
                "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
            }
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.call_id, "call-9");
                assert!(spec.has_gateway_entry_id());
            }
            other => panic!("expected UserForm with gateway id, got {other:?}"),
        }
    }

    #[test]
    fn d12fffc_custom_entry_id_is_not_call_id() {
        let schema = json!({
            "title": "Google account",
            "instruction": "Enter the address and password.",
            "liveHost": "accounts.google.com",
            "fields": [
                {"id": "email", "label": "Email", "type": "email", "required": true, "secret": false},
                {"id": "password", "label": "Password", "type": "password", "required": true, "secret": false, "value": "s3cret-pass"}
            ]
        });
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "threadId": "thr-1",
            "runId": "run-1",
            "callId": "mock-form-1",
            "tool": "request_user_form",
            "reason": "user-form",
            "why": "Waiting for you",
            "entryId": "e_form",
            "arguments": schema.clone(),
            "formRequest": schema
        }));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "", "field values must not become chat text");
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.call_id, "mock-form-1");
                assert_ne!(spec.entry_id, spec.call_id);
                assert!(spec.has_gateway_entry_id());
                assert!(spec.can_post(true));
                assert!(spec.fields.iter().any(|f| f.masked()));
                assert!(spec.fields.iter().all(|f| f.prefill.is_none()));
            }
            other => panic!("expected UserForm from d12fffc CUSTOM, got {other:?}"),
        }
        assert!(turn.waiting_user_form());
        assert!(!turn.waiting_approval());
        let dump = format!("{parts:?}");
        assert!(!dump.contains("s3cret-pass"), "{dump}");
        assert!(
            parts
                .iter()
                .all(|part| !matches!(part, ChatPart::Approval(_) | ChatPart::Ui(_))),
            "must not mount as Approval or FormSpec: {parts:?}"
        );
    }

    #[test]
    #[ignore = "product question: when a narrower send-message envelope folds onto the awaiting card, which side owns the field list? merge takes the incoming list whole and drops the password field; decide before un-ignoring"]
    fn send_message_envelope_folds_onto_the_awaiting_card() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Sign in",
                "fields": [
                    {"id": "email", "label": "Email", "type": "email", "required": true},
                    {"id": "password", "label": "Password", "type": "password", "required": true}
                ]
            }
        }));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": {
                        "title": "Sign in",
                        "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                    }
                },
                "formResolution": null
            }
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.call_id, "call-9");
                assert!(spec.has_gateway_entry_id());
                assert!(spec.fields.iter().any(|f| f.masked()));
            }
            other => panic!("expected one merged card, got {other:?}"),
        }
    }

    #[test]
    fn a_second_otp_form_mounts_after_password_submitted() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-pw",
            "entryId": "e_pw",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Google password",
                "fields": [{"id": "password", "label": "Password", "type": "password", "required": true}]
            }
        }));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": { "entryId": "e_pw", "formResolution": "submitted" }
        }));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-otp",
            "entryId": "e_otp",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Enter the code",
                "fields": [{"id": "otp", "label": "Code", "type": "otp", "required": true}]
            }
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::UserForm(password), ChatPart::UserForm(otp)] => {
                assert_eq!(password.entry_id, "e_pw");
                assert_eq!(
                    password.effective_resolution(),
                    Some(crate::opengrok::FormResolution::Submitted)
                );
                assert_eq!(otp.entry_id, "e_otp");
                assert!(otp.is_unresolved());
                assert_eq!(otp.fields[0].kind, crate::opengrok::UserFormFieldKind::Otp);
            }
            other => panic!("expected password Submitted then OTP idle, got {other:?}"),
        }
        assert!(turn.waiting_user_form());
    }

    #[test]
    fn a_second_otp_form_is_not_folded_onto_an_open_password_card() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-pw",
            "entryId": "e_pw",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Google password",
                "fields": [{"id": "password", "label": "Password", "type": "password", "required": true}]
            }
        }));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-otp",
            "entryId": "e_otp",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Enter the code",
                "fields": [{"id": "otp", "label": "Code", "type": "otp", "required": true}]
            }
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::UserForm(password), ChatPart::UserForm(otp)] => {
                assert_eq!(password.entry_id, "e_pw");
                assert!(password.is_unresolved());
                assert!(
                    password.can_post(false),
                    "stamped sibling must stay Continue-able; merge must not steal its entryId"
                );
                assert_eq!(otp.entry_id, "e_otp");
                assert!(otp.is_unresolved());
                assert!(otp.can_post(true));
                assert!(
                    otp.can_post(false),
                    "server_fill on another card must not gray this one"
                );
            }
            other => panic!("expected two cards, got {other:?}"),
        }
    }

    #[test]
    fn credential_offer_save_and_request_are_protocol_parts_without_passwords() {
        let mut assembler = TurnAssembler::default();
        assembler.push_event(&json!({
            "type": "CUSTOM",
            "name": "credential.offer_save",
            "value": {
                "origin": "google.com",
                "username": "ada@example.com",
                "formEntryId": "e_form",
                "password": "s3cret-pass"
            }
        }));
        assembler.push_event(&json!({
            "type": "CUSTOM",
            "name": "credential.request",
            "runId": "run-1",
            "requestId": "req-9",
            "value": { "origin": "google.com", "username": "ada@example.com", "password": "nope" }
        }));
        let (_, parts) = assembler.snapshot();
        match parts.as_slice() {
            [ChatPart::SaveLogin(offer), ChatPart::CredentialRequest(req)] => {
                assert_eq!(offer.origin, "google.com");
                assert_eq!(offer.username, "ada@example.com");
                assert_eq!(req.request_id, "req-9");
                assert!(req.is_unresolved());
                assert!(assembler.waiting_user_form());
            }
            other => panic!("expected save+request, got {other:?}"),
        }
        let dump = format!("{parts:?}");
        assert!(!dump.contains("s3cret-pass"));
        assert!(!dump.contains("nope"));
    }

    #[test]
    fn settled_send_message_form_resolution_folds_onto_awaiting() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "entryId": "e_form",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Sign in",
                "fields": [
                    {"id": "email", "label": "Email", "type": "email", "required": true},
                    {"id": "password", "label": "Password", "type": "password", "required": true, "value": "s3cret-pass"}
                ]
            }
        }));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": {
                        "title": "Sign in",
                        "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                    }
                },
                "formResolution": "submitted"
            }
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.call_id, "call-9");
                assert_eq!(
                    spec.effective_resolution(),
                    Some(crate::opengrok::FormResolution::Submitted)
                );
                assert!(!spec.is_unresolved());
            }
            other => panic!("expected one settled card, got {other:?}"),
        }
        let dump = format!("{parts:?}");
        assert!(!dump.contains("s3cret-pass"));
    }

    fn form_label_card() -> ChatPart {
        ChatPart::UserForm(
            UserFormSpec::parse(
                &json!({
                    "entryId": "e_form",
                    "formRequest": {
                        "title": "Form Label",
                        "instruction": "I'll raise a single in-chat form.",
                        "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                    }
                }),
                None,
            )
            .unwrap(),
        )
    }

    fn dummy_shot(call_id: &str) -> ChatPart {
        ChatPart::Screenshot(ScreenshotSpec {
            call_id: call_id.into(),
            caption: "screenshot".into(),
            image: std::sync::Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Png,
                vec![0],
            )),
            width: 1280,
            height: 800,
            visibility: Some(ImageVisibility::Transcript),
        })
    }

    #[test]
    fn place_hitl_puts_the_open_form_where_dismissed_ends_up() {
        let mut live = vec![
            ChatPart::Text("I'll raise a single in-chat form…".into()),
            dummy_shot("obs-1"),
            ChatPart::Text("more.".into()),
            form_label_card(),
        ];
        place_hitl_cards_in_document_order(&mut live);
        match live.as_slice() {
            [
                ChatPart::Text(intro),
                ChatPart::UserForm(spec),
                ChatPart::Screenshot(_),
                ChatPart::Text(after),
            ] => {
                assert!(intro.contains("in-chat form"));
                assert_eq!(spec.title, "Form Label");
                assert!(spec.is_unresolved());
                assert_eq!(after, "more.");
            }
            other => panic!("open form must sit above the screenshot block, got {other:?}"),
        }
        let mut settled = live.clone();
        if let ChatPart::UserForm(spec) = &mut settled[1] {
            spec.resolution = Some(crate::opengrok::FormResolution::Dismissed);
        }
        place_hitl_cards_in_document_order(&mut settled);
        assert_eq!(
            live.iter()
                .map(|p| std::mem::discriminant(p))
                .collect::<Vec<_>>(),
            settled
                .iter()
                .map(|p| std::mem::discriminant(p))
                .collect::<Vec<_>>(),
            "settle must not move the card"
        );
    }

    #[test]
    fn live_awaiting_form_is_not_appended_under_the_screenshot_block() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "TEXT_MESSAGE_CONTENT",
            "delta": "I'll raise a single in-chat form…"
        }));
        turn.push_event(&json!({
            "type": "TOOL_CALL_RESULT",
            "toolCallId": "obs-1",
            "content": "observe",
            "ok": true,
            "image": png_image(Some("transcript"))
        }));
        turn.push_event(&json!({
            "type": "TEXT_MESSAGE_CONTENT",
            "delta": "Working on the page."
        }));
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "entryId": "e_form",
            "tool": "request_user_form",
            "reason": "user-form",
            "arguments": {
                "title": "Form Label",
                "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
            }
        }));
        let (_, open) = turn.snapshot();
        match open.as_slice() {
            [
                ChatPart::Text(_),
                ChatPart::UserForm(spec),
                ChatPart::Screenshot(_),
                ChatPart::Text(_),
            ] => {
                assert_eq!(spec.title, "Form Label");
                assert!(spec.is_unresolved());
            }
            other => {
                panic!("PASS: open Form Label already sits where Dismissed ends up; got {other:?}")
            }
        }
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": { "title": "Form Label" }
                },
                "formResolution": "dismissed"
            }
        }));
        let (_, settled) = turn.snapshot();
        match settled.as_slice() {
            [
                ChatPart::Text(_),
                ChatPart::UserForm(spec),
                ChatPart::Screenshot(_),
                ChatPart::Text(_),
            ] => {
                assert_eq!(
                    spec.effective_resolution(),
                    Some(crate::opengrok::FormResolution::Dismissed)
                );
            }
            other => panic!("Dismissed must stay in the same slot, got {other:?}"),
        }
    }

    #[test]
    fn computer_handoff_custom_paints_an_escalated_form() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type": "CUSTOM",
            "name": "computer-handoff-card",
            "value": {
                "entryId": "e_form",
                "boxRequestId": "box-9",
                "boxInstruction": "Sign in on the computer."
            }
        }));
        let (_, parts) = turn.snapshot();
        match parts.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert!(spec.shows_computer_handoff());
                assert_eq!(spec.handoff_prompt(), "Sign in on the computer.");
            }
            other => panic!("expected Computer handoff card, got {other:?}"),
        }
    }
}
