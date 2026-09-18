//! User-form: in-chat credentials that fill the box page, never the bot.
//!
//! This is **not** generative UI [`super::gen_ui::FormSpec`] (choice chips that
//! dump values into `send_message`). It is **not** secret-request (a connector
//! vault with **Save securely**). It is **not** Computer handoff (`Take over` /
//! `I'm done` / `Skip`). Password and other secret fields typed here must never
//! enter AG-UI `content` or sqlite.
//!
//! # Two channels ([opengrok-server#139](https://github.com/hexuria/opengrok-server/pull/139))
//!
//! 1. **AG-UI SSE** (`POST /ag-ui`): CUSTOM `name: "run-awaiting-approval"`,
//!    `reason: "user-form"`, `tool: "request_user_form"`, **top-level
//!    `entryId`** (alongside `callId` / `reason` — same as the gateway card
//!    id). `arguments` is the sanitized schema; `formRequest` is an alias of
//!    that object. There is **no** `message.type: "user-form"` on this
//!    stream. Paint from `arguments` / `formRequest`. Activity is **Waiting
//!    for you**. This is **not** an approval card and must not go through
//!    `POST /ag-ui/runs/{id}/answer`.
//! 2. **Gateway transcript**: `kind: send-message`, `id` = gateway card id
//!    (`e_{uuid}` === CUSTOM `entryId`), `message.type: user-form`,
//!    `formRequest`; sibling `formResolution` when settled. NativeChat is
//!    AG-UI-first and does not consume gateway `send-message` as the live
//!    turn path. The envelope is still parsed if it appears on a value we
//!    already accept.
//!
//! [`USER_FORM_CUSTOM`] (`CUSTOM` `name: "user-form"`) is a **test/fixture
//! alias only**. The server does not emit that name on AG-UI.
//!
//! # Fill verbs
//!
//! Continue / Open the screen / Dismiss / Hand back POST with an **account bearer**:
//!
//! - `POST /ag-ui/user-form/submit` `{entryId, agentId, values}`
//! - `POST /ag-ui/user-form/dismiss` `{entryId, agentId, mode: dismissed|escalated}`
//! - `POST /ag-ui/box-handoff/resolve` `{entryId: handoffEntryId, agentId, resolution}`
//!
//! **Form `entryId` is the gateway card id, never `callId`.** Open the screen
//! keeps that id for the pill and stores **`handoffEntryId`** from the dismiss
//! response. Hand back / decline POST that handoff id — **not** the form card
//! id, **not** `handBackForeverBox`, **not** Take over / I'm done / Skip.
//!
//! After Continue paints **Sending**, HTTP must not restore idle fields.
//! A body with `formResolution` (`submitted` / `fill_failed` / …) is merged.
//! **200 JSON `null` is not Submitted** — that body is disclosure (`may_use`
//! fail / unknown coworker) on [opengrok-server#139](https://github.com/hexuria/opengrok-server/pull/139)
//! @ `c09bc6c`. Paint **Not filled**. A 404 `{error: "form entry missing"}`
//! is a missing stamped card (Not filled), not a missing route. A 404 that
//! is not that sentence flips verbs off. Missing `entryId` (no POST) does
//! not paint Submitted.
//!
//! # [opengrok-server#140](https://github.com/hexuria/opengrok-server/issues/140) on #139 @ d12fffc
//!
//! `AgUiSink` mints the gateway card **before** the CUSTOM frame and stamps
//! top-level `entryId` (and `formRequest`, alias of sanitised `arguments`).
//! That id is what Continue / Dismiss POST. We never invent one: `callId`,
//! `toolCallId`, and a generic AG-UI event `id` are not `entryId`.
//!
//! When `entryId` is present and the verbs are up, Open the screen / Dismiss
//! are live; Continue also needs required fields filled. Missing `entryId`
//! keeps the card idle — **Submitted is not painted**. A 404 after a send
//! that reached the server is Not filled, not idle fields.
//!
//! [`USER_FORM_SERVER_FILL_AVAILABLE`] defaults true (#139 @ d12fffc+ has
//! the routes). AppState flips it off after a **missing-route** 404, not
//! after `{error: "form entry missing"}`. Fill still needs
//! [`UserFormSpec::has_gateway_entry_id`].
//!
//! `formFieldOutcomes` is hashed on the official client, not painted. We parse
//! it only so it cannot be mistaken for field values.

use serde_json::{Value, json};

/// Fixture CUSTOM `name` only. Live #139 HITL is `run-awaiting-approval`
/// with `reason: "user-form"` — see [`is_user_form_awaiting`].
pub const USER_FORM_CUSTOM: &str = "user-form";

/// Tool name official 0.29 uses while the bot waits on the card. Activity:
/// **Waiting for you**, distinct from **Waiting for approval**.
pub const REQUEST_USER_FORM_TOOL: &str = "request_user_form";

/// Working-line copy while an unresolved user-form is on screen.
pub const WAITING_FOR_YOU: &str = "Waiting for you";

/// AG-UI REST twins from opengrok-server#139. Account bearer, not the
/// Electron coordinator, and not `/ag-ui/runs/{id}/answer`.
pub const USER_FORM_SUBMIT_PATH: &str = "/ag-ui/user-form/submit";
pub const USER_FORM_DISMISS_PATH: &str = "/ag-ui/user-form/dismiss";
/// Hand back / decline / timeout. `entryId` is the handoff card, not the form.
pub const BOX_HANDOFF_RESOLVE_PATH: &str = "/ag-ui/box-handoff/resolve";

/// Default true: opengrok-server#139 @ d12fffc+ has submit/dismiss. AppState
/// sets this false after a **missing-route** 404 so we never POST against a
/// server that has no verbs. A 404 `{error: "form entry missing"}` is not
/// that: the routes exist, the stamped card does not. Per-card fill still
/// requires a real gateway `entryId`.
pub const USER_FORM_SERVER_FILL_AVAILABLE: bool = true;

/// OpenGrok #139 @ c09bc6c: stamped `entryId` that is gone from the
/// transcript. Distinct from a server that has no `/ag-ui/user-form/*`.
pub const FORM_ENTRY_MISSING: &str = "form entry missing";

/// Transcript display-map presence for a typed secret. Not a password: the
/// typed value stays in `InputState`. Continue must not treat this as filled.
pub const MASKED_PRESENCE_STUB: &str = "1";

/// Field types from NativeChat #17. Anything else is default text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFormFieldKind {
    Email,
    Tel,
    Password,
    Otp,
    Number,
    Date,
    Select,
    Textarea,
    Checkbox,
    Text,
}

impl UserFormFieldKind {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "email" => Self::Email,
            "tel" | "phone" => Self::Tel,
            "password" => Self::Password,
            "otp" => Self::Otp,
            "number" => Self::Number,
            "date" => Self::Date,
            "select" => Self::Select,
            "textarea" => Self::Textarea,
            "checkbox" => Self::Checkbox,
            _ => Self::Text,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Tel => "tel",
            Self::Password => "password",
            Self::Otp => "otp",
            Self::Number => "number",
            Self::Date => "date",
            Self::Select => "select",
            Self::Textarea => "textarea",
            Self::Checkbox => "checkbox",
            Self::Text => "text",
        }
    }

    /// Official: `password` / `otp` are secret even without `secret: true`.
    pub fn is_secret_kind(self) -> bool {
        matches!(self, Self::Password | Self::Otp)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFormSelectOption {
    pub value: String,
    pub label: String,
}

/// One field on the idle card. Live typed values do **not** live here — they
/// stay in the view, never on the part, never in sqlite.
#[derive(Clone, PartialEq, Eq)]
pub struct UserFormField {
    pub id: String,
    pub label: String,
    pub kind: UserFormFieldKind,
    pub required: bool,
    /// `secret: true` on the wire, or a password/otp type.
    pub secret: bool,
    pub options: Vec<UserFormSelectOption>,
    pub placeholder: Option<String>,
    /// Non-secret default only. Secret `value` / `default` on the wire are dropped.
    pub prefill: Option<String>,
}

impl std::fmt::Debug for UserFormField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserFormField")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("required", &self.required)
            .field("secret", &self.secret)
            .field("options", &self.options)
            .field("placeholder", &self.placeholder)
            .field(
                "prefill",
                &self
                    .prefill
                    .as_ref()
                    .map(|_| if self.masked() { "<redacted>" } else { "<set>" }),
            )
            .finish()
    }
}

impl UserFormField {
    pub fn masked(&self) -> bool {
        self.secret || self.kind.is_secret_kind()
    }

    /// Checkbox submit values are the strings `"true"` / `"false"`.
    pub fn checkbox_wire(checked: bool) -> &'static str {
        if checked { "true" } else { "false" }
    }
}

/// Settled `formResolution`, including local-only **Sending**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormResolution {
    /// Optimistic local pill. Not durable server success.
    Sending,
    Submitted,
    FillFailed,
    Escalated,
    Dismissed,
}

impl FormResolution {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "sending" | "submitting" => Self::Sending,
            "submitted" => Self::Submitted,
            "fill_failed" | "fill-failed" | "not_filled" | "not-filled" => Self::FillFailed,
            "escalated" | "on_screen" | "on-screen" | "on_the_computer" | "on-the-computer" => {
                Self::Escalated
            }
            _ => Self::Dismissed,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sending => "sending",
            Self::Submitted => "submitted",
            Self::FillFailed => "fill_failed",
            Self::Escalated => "escalated",
            Self::Dismissed => "dismissed",
        }
    }

    /// Pill copy. Continue paints **Submitting** (local) then **Submitted**.
    /// Open the screen paints **On the computer**, not OpenGrok Take over.
    pub fn pill(self) -> &'static str {
        match self {
            Self::Sending => "Submitting",
            Self::Submitted => "Submitted",
            Self::FillFailed => "Not filled",
            Self::Escalated => "On the computer",
            Self::Dismissed => "Dismissed",
        }
    }

    /// Body copy under the collapsed card.
    pub fn body(self) -> &'static str {
        match self {
            Self::Sending => "Submitting…",
            Self::Submitted => "Filled into the page. Secret values were never shown to your Bot.",
            Self::FillFailed => {
                "Could not fill into the page — it may have moved or changed. Secret values were never shown to your Bot."
            }
            Self::Escalated => "You chose to do this step on the computer.",
            Self::Dismissed => "Dismissed without filling anything.",
        }
    }
}

/// `POST /ag-ui/box-handoff/resolve` `resolution`. Not `handBackForeverBox`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxHandoffResolution {
    HandedBack,
    Declined,
    TimedOut,
}

impl BoxHandoffResolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HandedBack => "handed_back",
            Self::Declined => "declined",
            Self::TimedOut => "timed_out",
        }
    }
}

/// What `POST /ag-ui/box-handoff/resolve` meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxHandoffReply {
    Settled,
    AlreadyAnswered,
    Empty,
    MissingRoute,
    MissingEntryId,
}

/// `dismissUserForm` mode. Open the screen → [`Escalated`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFormDismissMode {
    Dismissed,
    Escalated,
}

impl UserFormDismissMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dismissed => "dismissed",
            Self::Escalated => "escalated",
        }
    }

    pub fn resolution(self) -> FormResolution {
        match self {
            Self::Dismissed => FormResolution::Dismissed,
            Self::Escalated => FormResolution::Escalated,
        }
    }
}

/// What `POST /ag-ui/user-form/submit|dismiss` meant. Bind paint to a real
/// `formResolution` ([`Settled`]). 200-null is [`Empty`] — disclosure, not a
/// fill. A miss (404 / no POST) is never Submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserFormActionReply {
    Settled(UserFormSpec),
    AlreadyAnswered,
    /// 200 JSON `null`, empty body, or an object with no `formResolution`.
    Empty,
    /// Route is not on this server.
    MissingRoute,
    /// 404 `{error: "form entry missing"}` — verbs exist, this card does not.
    MissingEntry,
    /// No gateway card id — we must not POST `callId` as `entryId`.
    MissingEntryId,
}

/// Continue vs Dismiss. Hand-back is a different route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFormVerb {
    Submit,
    Dismiss,
}

/// What the idle card becomes after the fill HTTP returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserFormHttpSettle {
    /// Body carried `formResolution` (or a parseable card).
    Merge(UserFormSpec),
    /// Keep collapsed without a body: after Continue, Not filled — never
    /// idle greyed fields. Dismiss does not use this arm.
    Paint(FormResolution),
    /// Dismiss already painted dismissed/escalated; 200-null keeps it.
    Keep,
    /// No POST, or dismiss 404. Undo the optimistic paint.
    Restore,
}

/// After Continue has painted **Sending**, never restore idle fields.
/// Prefer a body `formResolution` (Merge). 200-null / Empty / AlreadyAnswered
/// without a resolution is **Not filled** (disclosure, not a fill). 404
/// form-entry-missing and missing-route are Not filled. Dismiss 404 may
/// Restore; submit 404 must not.
pub fn settle_user_form_http(
    verb: UserFormVerb,
    reply: &UserFormActionReply,
) -> UserFormHttpSettle {
    match (verb, reply) {
        (_, UserFormActionReply::Settled(spec)) => UserFormHttpSettle::Merge(spec.clone()),
        (UserFormVerb::Submit, UserFormActionReply::Empty)
        | (UserFormVerb::Submit, UserFormActionReply::AlreadyAnswered)
        | (UserFormVerb::Submit, UserFormActionReply::MissingRoute)
        | (UserFormVerb::Submit, UserFormActionReply::MissingEntry)
        | (UserFormVerb::Submit, UserFormActionReply::MissingEntryId) => {
            UserFormHttpSettle::Paint(FormResolution::FillFailed)
        }
        (UserFormVerb::Dismiss, UserFormActionReply::Empty)
        | (UserFormVerb::Dismiss, UserFormActionReply::AlreadyAnswered) => UserFormHttpSettle::Keep,
        (UserFormVerb::Dismiss, UserFormActionReply::MissingRoute)
        | (UserFormVerb::Dismiss, UserFormActionReply::MissingEntry)
        | (UserFormVerb::Dismiss, UserFormActionReply::MissingEntryId) => {
            UserFormHttpSettle::Restore
        }
    }
}

/// Stable gpui-agent / GPUI ids. Master Tester looks for `user-form-*`.
pub fn user_form_card_id(card_key: &str) -> String {
    format!("user-form-{card_key}")
}

pub fn user_form_continue_id(card_key: &str) -> String {
    format!("user-form-continue-{card_key}")
}

pub fn user_form_dismiss_id(card_key: &str) -> String {
    format!("user-form-dismiss-{card_key}")
}

pub fn user_form_screen_id(card_key: &str) -> String {
    format!("user-form-screen-{card_key}")
}

pub fn user_form_field_id(card_key: &str, field_id: &str) -> String {
    format!("user-form-field-{card_key}-{field_id}")
}

/// A user-form card in the transcript. Field values are not stored on this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFormSpec {
    /// Gateway transcript card id (`e_{uuid}`). Same as top-level CUSTOM
    /// `entryId` from #139 @ d12fffc. Required to POST. Never invented from
    /// `callId`. Empty only if the frame omitted it.
    pub entry_id: String,
    /// AG-UI tool call id. Used to merge events. **Never** sent as `entryId`.
    pub call_id: String,
    /// AG-UI run to follow after a real settle. Empty when the event had none.
    pub run_id: String,
    pub title: String,
    pub instruction: Option<String>,
    pub fields: Vec<UserFormField>,
    pub domain: Option<String>,
    pub live_host: Option<String>,
    pub resolution: Option<FormResolution>,
    pub widget_dismissed: bool,
    /// HTTP convenience from dismiss `mode: escalated`. Not the form card id.
    /// Never sent as submit/dismiss `entryId`. Never a `boxRequestId` on this card.
    pub handoff_entry_id: Option<String>,
}

impl UserFormSpec {
    pub fn is_unresolved(&self) -> bool {
        self.effective_resolution().is_none()
    }

    pub fn effective_resolution(&self) -> Option<FormResolution> {
        self.resolution
            .or_else(|| self.widget_dismissed.then_some(FormResolution::Dismissed))
    }

    pub fn pill(&self) -> Option<&'static str> {
        self.effective_resolution().map(FormResolution::pill)
    }

    pub fn settled_body(&self) -> Option<&'static str> {
        self.effective_resolution().map(FormResolution::body)
    }

    /// Submit looks the gateway entry up by this id. `callId` is not a substitute.
    pub fn has_gateway_entry_id(&self) -> bool {
        !self.entry_id.is_empty()
    }

    /// Identity in the transcript view when the gateway id is missing (#140).
    pub fn card_key(&self) -> &str {
        if !self.entry_id.is_empty() {
            &self.entry_id
        } else if !self.call_id.is_empty() {
            &self.call_id
        } else {
            "user-form"
        }
    }

    pub fn same_card(&self, other: &Self) -> bool {
        if self.has_gateway_entry_id()
            && other.has_gateway_entry_id()
            && self.entry_id == other.entry_id
        {
            return true;
        }
        if !self.call_id.is_empty() && self.call_id == other.call_id {
            return true;
        }
        false
    }

    /// Call-only twin (`user-form-call-{callId}-*`) of a stamped entry card.
    pub fn shares_call_id(&self, call_id: &str) -> bool {
        !call_id.is_empty() && !self.call_id.is_empty() && self.call_id == call_id
    }

    /// Fold an awaiting CUSTOM onto a later send-message envelope (one id
    /// missing). A second OTP form with a different `entryId` / `callId` is a
    /// new card — do not merge it onto a password form that already settled.
    pub fn completes_with(&self, other: &Self) -> bool {
        if self.has_gateway_entry_id()
            && other.has_gateway_entry_id()
            && self.entry_id != other.entry_id
        {
            return false;
        }
        if !self.call_id.is_empty() && !other.call_id.is_empty() && self.call_id != other.call_id {
            return false;
        }
        true
    }

    /// Continue / Dismiss POST only when the route is present **and** we have
    /// a gateway `entryId`. Required fields are an extra Continue gate.
    pub fn continue_enabled(&self, values: &UserFormValues, server_fill: bool) -> bool {
        self.can_post(server_fill) && self.required_fields_filled(values)
    }

    pub fn can_post(&self, server_fill: bool) -> bool {
        server_fill && self.has_gateway_entry_id()
    }

    pub fn required_fields_filled(&self, values: &UserFormValues) -> bool {
        self.fields
            .iter()
            .filter(|field| field.required)
            .all(|field| values.filled(field))
    }

    /// Fold a later event for the same card onto this one (resolution, or a
    /// fuller request). Secret values are not carried.
    pub fn merge(&mut self, incoming: UserFormSpec) {
        if incoming.has_gateway_entry_id() {
            self.entry_id = incoming.entry_id;
        }
        if !incoming.call_id.is_empty() {
            self.call_id = incoming.call_id;
        }
        if !incoming.run_id.is_empty() {
            self.run_id = incoming.run_id;
        }
        if !incoming.title.is_empty() {
            self.title = incoming.title;
        }
        if incoming.instruction.is_some() {
            self.instruction = incoming.instruction;
        }
        if !incoming.fields.is_empty() {
            self.fields = incoming.fields;
        }
        if incoming.domain.is_some() {
            self.domain = incoming.domain;
        }
        if incoming.live_host.is_some() {
            self.live_host = incoming.live_host;
        }
        if incoming.resolution.is_some() {
            self.resolution = incoming.resolution;
        }
        self.widget_dismissed = self.widget_dismissed || incoming.widget_dismissed;
        if incoming.handoff_entry_id.is_some() {
            self.handoff_entry_id = incoming.handoff_entry_id;
        }
    }

    pub fn from_custom_event(event: &Value) -> Option<Self> {
        if is_user_form_awaiting(event)
            && let Some(spec) = Self::from_awaiting_event(event, None)
        {
            return Some(spec);
        }
        let name = event.get("name").and_then(Value::as_str).unwrap_or("");
        let value = event.get("value").unwrap_or(event);
        if !is_user_form_event(name, value) {
            return None;
        }
        let mut spec = Self::parse(value, gateway_entry_id(event, value))?;
        fill_run_and_call(&mut spec, event);
        Some(spec)
    }

    /// Live #139 HITL: CUSTOM `run-awaiting-approval` + `reason: user-form`.
    /// Paint from `arguments` (sanitized schema); `formRequest` is the same
    /// object at the top level. **`entryId` is top-level**, next to `callId`
    /// / `reason` — not nested under `value`. `callId` is merge-only.
    pub fn from_awaiting_event(event: &Value, args_fallback: Option<&Value>) -> Option<Self> {
        let arguments = arguments_object(event, args_fallback);
        let value = event.get("value").unwrap_or(&Value::Null);
        let hint = gateway_entry_id(event, &arguments).or_else(|| gateway_entry_id(event, event));
        let mut spec = Self::parse(&arguments, hint.clone())
            .or_else(|| Self::parse(&json!({ "formRequest": arguments.clone() }), hint.clone()))
            .or_else(|| Self::parse(event, hint.clone()))
            .or_else(|| {
                looks_like_user_form_value(value)
                    .then(|| Self::parse(value, hint.clone()))
                    .flatten()
            })?;
        fill_run_and_call(&mut spec, event);
        Some(spec)
    }

    pub fn from_tool_args(args: &Value, tool_call_id: &str) -> Option<Self> {
        if !looks_like_user_form_value(args) && args.get("fields").is_none() {
            return None;
        }
        let mut spec = Self::parse(args, gateway_entry_id_in(args))?;
        if spec.call_id.is_empty() && !tool_call_id.is_empty() {
            spec.call_id = tool_call_id.to_string();
        }
        Some(spec)
    }

    pub fn parse(value: &Value, entry_id: Option<String>) -> Option<Self> {
        let request = form_request_object(value);
        let resolution = parse_resolution(value);
        let widget_dismissed = bool_at(value, "widgetDismissed").unwrap_or(false);
        let fields = request
            .map(parse_fields)
            .or_else(|| value.get("fields").map(parse_fields_value))
            .unwrap_or_default();
        let title = request
            .and_then(|req| string_field(req, "title"))
            .or_else(|| string_field(value, "title"))
            .unwrap_or_default();
        let entry_id = entry_id
            .or_else(|| gateway_entry_id_in(value))
            .unwrap_or_default();
        if fields.is_empty() && title.is_empty() && resolution.is_none() && !widget_dismissed {
            return None;
        }
        // Outcomes are hashed, not painted — read so a payload that only has
        // them is not mistaken for values, then ignored.
        let _ = request
            .and_then(|req| req.get("formFieldOutcomes"))
            .or_else(|| value.get("formFieldOutcomes"));
        Some(Self {
            entry_id,
            call_id: String::new(),
            run_id: String::new(),
            // Empty when the event is resolution-only, so merge does not
            // clobber a title the request already set.
            title,
            instruction: request
                .and_then(|req| string_field(req, "instruction"))
                .or_else(|| string_field(value, "instruction")),
            fields,
            domain: request
                .and_then(|req| string_field(req, "domain"))
                .or_else(|| string_field(value, "domain")),
            live_host: request
                .and_then(|req| {
                    string_field(req, "liveHost").or_else(|| string_field(req, "live_host"))
                })
                .or_else(|| string_field(value, "liveHost")),
            resolution,
            widget_dismissed,
            handoff_entry_id: string_field(value, "handoffEntryId"),
        })
    }
}

/// Values typed on the idle card. The transcript display map may store only
/// [`MASKED_PRESENCE_STUB`] for masked fields; Continue must read live
/// InputState instead of that stub. Tests may put a secret here to prove
/// Debug and `agui_messages` never echo it. Never copy this map onto a
/// [`crate::opengrok::gen_ui::ChatPart`], into `Message.content`, or into sqlite.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct UserFormValues {
    pub by_id: std::collections::HashMap<String, String>,
}

impl std::fmt::Debug for UserFormValues {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserFormValues")
            .field("fields", &self.by_id.len())
            .finish()
    }
}

impl UserFormValues {
    pub fn filled(&self, field: &UserFormField) -> bool {
        let raw = self.by_id.get(&field.id).map(String::as_str).unwrap_or("");
        match field.kind {
            UserFormFieldKind::Checkbox => raw == "true",
            _ if field.masked() => {
                let trimmed = raw.trim();
                !trimmed.is_empty() && trimmed != MASKED_PRESENCE_STUB
            }
            _ => !raw.trim().is_empty(),
        }
    }

    pub fn debug_without_secrets(&self, fields: &[UserFormField]) -> String {
        let mut parts: Vec<String> = Vec::new();
        for field in fields {
            let present = self.by_id.get(&field.id).is_some_and(|v| !v.is_empty());
            let shown = if field.masked() {
                if present { "<masked>" } else { "<empty>" }
            } else if present {
                "<set>"
            } else {
                "<empty>"
            };
            parts.push(format!("{}:{shown}", field.id));
        }
        parts.join(",")
    }

    pub fn as_json_object(&self) -> Value {
        let mut map = serde_json::Map::new();
        for (id, value) in &self.by_id {
            map.insert(id.clone(), Value::String(value.clone()));
        }
        Value::Object(map)
    }
}

/// Fixture / optional CUSTOM names. Not what #139 emits on AG-UI SSE.
pub fn is_user_form_custom_name(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "user-form" | "form-request" | "form-resolution" | "request-user-form"
    )
}

pub fn is_user_form_tool(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "request-user-form" | "user-form"
    )
}

pub fn is_user_form_event(name: &str, value: &Value) -> bool {
    is_user_form_custom_name(name) || looks_like_user_form_value(value)
}

/// Live AG-UI HITL from #139: CUSTOM `run-awaiting-approval` whose reason
/// (or tool) is the user-form, not exec-consent.
pub fn is_user_form_awaiting(event: &Value) -> bool {
    let name = event
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            event
                .get("value")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    if name != "run-awaiting-approval" {
        return false;
    }
    let reason = string_field(event, "reason")
        .or_else(|| {
            event
                .get("value")
                .and_then(|value| string_field(value, "reason"))
        })
        .unwrap_or_default();
    if reason.eq_ignore_ascii_case("user-form") {
        return true;
    }
    let tool = string_field(event, "tool")
        .or_else(|| {
            event
                .get("value")
                .and_then(|value| string_field(value, "tool"))
        })
        .unwrap_or_default();
    is_user_form_tool(&tool)
}

pub fn submit_request_body(entry_id: &str, agent_id: &str, values: &UserFormValues) -> Value {
    json!({
        "entryId": entry_id,
        "agentId": agent_id,
        "values": values.as_json_object(),
    })
}

pub fn dismiss_request_body(entry_id: &str, agent_id: &str, mode: UserFormDismissMode) -> Value {
    json!({
        "entryId": entry_id,
        "agentId": agent_id,
        "mode": mode.as_str(),
    })
}

/// Hand back / decline / timeout. `entry_id` is [`UserFormSpec::handoff_entry_id`].
pub fn resolve_handoff_request_body(
    handoff_entry_id: &str,
    agent_id: &str,
    resolution: BoxHandoffResolution,
) -> Value {
    json!({
        "entryId": handoff_entry_id,
        "agentId": agent_id,
        "resolution": resolution.as_str(),
    })
}

/// Classify a submit/dismiss HTTP response. 404 `{error: "form entry missing"}`
/// is [`MissingEntry`]. Any other 404 is [`MissingRoute`]. 200-null is
/// [`Empty`] — after Continue, [`settle_user_form_http`] paints Not filled.
/// Prefer a body with `formResolution`.
pub fn user_form_action_from_http(status: u16, body: &Value) -> UserFormActionReply {
    if status == 404 {
        if is_form_entry_missing(body) {
            return UserFormActionReply::MissingEntry;
        }
        return UserFormActionReply::MissingRoute;
    }
    if status != 200 {
        return UserFormActionReply::Empty;
    }
    if body.is_null() {
        return UserFormActionReply::Empty;
    }
    let already = body
        .get("alreadyAnswered")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(spec) = UserFormSpec::parse(body, gateway_entry_id_in(body)) {
        if spec.effective_resolution().is_some() {
            return UserFormActionReply::Settled(spec);
        }
    }
    if already {
        return UserFormActionReply::AlreadyAnswered;
    }
    UserFormActionReply::Empty
}

/// 404 body OpenGrok #139 @ c09bc6c uses when the stamped card is gone.
pub fn is_form_entry_missing(body: &Value) -> bool {
    body.get("error")
        .and_then(Value::as_str)
        .is_some_and(|error| error.eq_ignore_ascii_case(FORM_ENTRY_MISSING))
}

/// Classify `POST /ag-ui/box-handoff/resolve`. Never treat a miss as handed back.
pub fn box_handoff_action_from_http(status: u16, body: &Value) -> BoxHandoffReply {
    if status == 404 {
        if is_form_entry_missing(body) {
            return BoxHandoffReply::Empty;
        }
        return BoxHandoffReply::MissingRoute;
    }
    if status != 200 {
        return BoxHandoffReply::Empty;
    }
    if body.is_null() {
        return BoxHandoffReply::Empty;
    }
    if body
        .get("alreadyAnswered")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return BoxHandoffReply::AlreadyAnswered;
    }
    if body
        .get("boxResolution")
        .and_then(Value::as_str)
        .is_some_and(|word| !word.is_empty())
    {
        return BoxHandoffReply::Settled;
    }
    BoxHandoffReply::Settled
}

fn looks_like_user_form_value(value: &Value) -> bool {
    if value.get("formRequest").is_some() || value.get("formResolution").is_some() {
        return true;
    }
    let message = value.get("message");
    if message
        .and_then(|m| m.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|ty| ty.eq_ignore_ascii_case("user-form"))
    {
        return true;
    }
    if message.and_then(|m| m.get("formRequest")).is_some() {
        return true;
    }
    false
}

fn form_request_object(value: &Value) -> Option<&Value> {
    value
        .get("formRequest")
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("formRequest"))
        })
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("type"))
                .and_then(Value::as_str)
                .is_some_and(|ty| ty.eq_ignore_ascii_case("user-form"))
                .then_some(value.get("message"))
                .flatten()
        })
}

/// Gateway card id only. `callId` / `toolCallId` / a generic AG-UI `id` are
/// not this. A send-message envelope may live on `event.value`.
fn gateway_entry_id(event: &Value, value: &Value) -> Option<String> {
    string_field(event, "entryId")
        .or_else(|| gateway_entry_id_in(value))
        .or_else(|| event.get("value").and_then(gateway_entry_id_in))
}

fn gateway_entry_id_in(value: &Value) -> Option<String> {
    string_field(value, "entryId").or_else(|| {
        // Only the official send-message envelope uses `id` as the gateway
        // card. A CUSTOM frame now carries top-level `formRequest` (alias of
        // arguments) and may also have an AG-UI event `id` that is not the card.
        let send_message = value.get("kind").and_then(Value::as_str) == Some("send-message");
        if send_message {
            string_field(value, "id")
        } else {
            None
        }
    })
}

fn fill_run_and_call(spec: &mut UserFormSpec, event: &Value) {
    if spec.run_id.is_empty() {
        spec.run_id = string_field(event, "runId")
            .or_else(|| {
                event
                    .get("value")
                    .and_then(|value| string_field(value, "runId"))
            })
            .unwrap_or_default();
    }
    if spec.call_id.is_empty() {
        spec.call_id = string_field(event, "callId")
            .or_else(|| string_field(event, "toolCallId"))
            .or_else(|| {
                event.get("value").and_then(|value| {
                    string_field(value, "callId").or_else(|| string_field(value, "toolCallId"))
                })
            })
            .unwrap_or_default();
    }
}

fn arguments_object(event: &Value, fallback: Option<&Value>) -> Value {
    let raw = event
        .get("arguments")
        .cloned()
        .or_else(|| {
            event
                .get("value")
                .and_then(|value| value.get("arguments"))
                .cloned()
        })
        .or_else(|| event.get("formRequest").cloned())
        .or_else(|| fallback.cloned())
        .unwrap_or(Value::Null);
    if raw.is_null() {
        return fallback.cloned().unwrap_or(Value::Null);
    }
    if let Some(s) = raw.as_str() {
        return serde_json::from_str(s).unwrap_or(raw);
    }
    raw
}

fn parse_resolution(value: &Value) -> Option<FormResolution> {
    let raw = value.get("formResolution").or_else(|| {
        value
            .get("message")
            .and_then(|message| message.get("formResolution"))
    })?;
    if raw.is_null() {
        return None;
    }
    if let Some(s) = raw.as_str() {
        return Some(FormResolution::parse(s));
    }
    if let Some(status) = raw
        .get("status")
        .or_else(|| raw.get("state"))
        .and_then(Value::as_str)
    {
        return Some(FormResolution::parse(status));
    }
    None
}

fn parse_fields(request: &Value) -> Vec<UserFormField> {
    request
        .get("fields")
        .map(parse_fields_value)
        .unwrap_or_default()
}

fn parse_fields_value(fields: &Value) -> Vec<UserFormField> {
    let Some(rows) = fields.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .filter_map(|(i, row)| parse_field(row, i))
        .collect()
}

fn parse_field(row: &Value, index: usize) -> Option<UserFormField> {
    let label = string_field(row, "label").or_else(|| string_field(row, "name"))?;
    let id = string_field(row, "id").unwrap_or_else(|| format!("field-{index}"));
    let kind = row
        .get("type")
        .and_then(Value::as_str)
        .map(UserFormFieldKind::parse)
        .unwrap_or(UserFormFieldKind::Text);
    let secret_flag = bool_at(row, "secret").unwrap_or(false);
    let secret = secret_flag || kind.is_secret_kind();
    let required = bool_at(row, "required").unwrap_or(false);
    let options = parse_options(row);
    let placeholder = string_field(row, "placeholder");
    let prefill = if secret {
        None
    } else {
        string_field(row, "value").or_else(|| string_field(row, "default"))
    };
    Some(UserFormField {
        id,
        label,
        kind,
        required,
        secret,
        options,
        placeholder,
        prefill,
    })
}

fn parse_options(row: &Value) -> Vec<UserFormSelectOption> {
    let Some(opts) = row.get("options").and_then(Value::as_array) else {
        return Vec::new();
    };
    opts.iter()
        .filter_map(|opt| {
            if let Some(value) = opt.as_str() {
                return Some(UserFormSelectOption {
                    value: value.to_string(),
                    label: value.to_string(),
                });
            }
            let value = string_field(opt, "value").or_else(|| string_field(opt, "id"))?;
            let label = string_field(opt, "label").unwrap_or_else(|| value.clone());
            Some(UserFormSelectOption { value, label })
        })
        .collect()
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn bool_at(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(|v| {
        v.as_bool().or_else(|| {
            v.as_str()
                .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
        })
    })
}

fn normalize_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
}

/// Continue's gates, for tests and for the renderer. Never "fill succeeded".
pub fn continue_enabled(spec: &UserFormSpec, values: &UserFormValues, server_fill: bool) -> bool {
    spec.continue_enabled(values, server_fill)
}

/// Settle every spec that shares `call_id` (stamped entry + call-only twin).
/// A later OTP with a different call stays unresolved.
pub fn bind_call_peers(specs: &mut [UserFormSpec], call_id: &str, resolution: FormResolution) {
    if call_id.is_empty() {
        return;
    }
    for spec in specs {
        if spec.shares_call_id(call_id) {
            spec.resolution = Some(resolution);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn google_email_request() -> Value {
        json!({
            "title": "Google account email",
            "instruction": "Enter the other Gmail address you want to sign in with.",
            "fields": [
                {
                    "id": "email",
                    "label": "Email or phone",
                    "type": "email",
                    "required": true
                }
            ],
            "domain": "accounts.google.com",
            "liveHost": "accounts.google.com"
        })
    }

    fn live_awaiting(entry_id: Option<&str>) -> Value {
        let mut event = json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": REQUEST_USER_FORM_TOOL,
            "reason": "user-form",
            "why": WAITING_FOR_YOU,
            "arguments": {
                "title": "Google account email",
                "instruction": "Enter the other Gmail address you want to sign in with.",
                "fields": [
                    {"id": "email", "label": "Email or phone", "type": "email", "required": true},
                    {"id": "password", "label": "Password", "type": "password", "required": true}
                ],
                "liveHost": "accounts.google.com"
            }
        });
        if let Some(id) = entry_id {
            event
                .as_object_mut()
                .unwrap()
                .insert("entryId".into(), json!(id));
        }
        event
    }

    /// Wire shape from opengrok-server#139 @ d12fffc (`a_user_form_custom_frame_carries_entry_id_at_the_top_level`).
    fn official_d12fffc_frame() -> Value {
        let schema = json!({
            "title": "Google account",
            "instruction": "Enter the address and password.",
            "liveHost": "accounts.google.com",
            "fields": [
                {
                    "id": "email",
                    "label": "Email",
                    "type": "email",
                    "required": true,
                    "secret": false
                },
                {
                    "id": "password",
                    "label": "Password",
                    "type": "password",
                    "required": true,
                    "secret": false
                }
            ]
        });
        json!({
            "type": "CUSTOM",
            "timestamp": 1_789_672_932_380u64,
            "name": "run-awaiting-approval",
            "threadId": "thr-1",
            "runId": "run-1",
            "callId": "mock-form-1",
            "tool": REQUEST_USER_FORM_TOOL,
            "reason": "user-form",
            "why": WAITING_FOR_YOU,
            "entryId": "e_form",
            "arguments": schema.clone(),
            "formRequest": schema
        })
    }

    #[test]
    fn official_send_message_envelope_parses_idle_card() {
        let event = json!({
            "type": "CUSTOM",
            "name": USER_FORM_CUSTOM,
            "value": {
                "kind": "send-message",
                "id": "e_entry-email",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": null
            }
        });
        let spec = UserFormSpec::from_custom_event(&event).expect("idle card");
        assert_eq!(spec.entry_id, "e_entry-email");
        assert!(spec.has_gateway_entry_id());
        assert_eq!(spec.title, "Google account email");
        assert_eq!(
            spec.instruction.as_deref(),
            Some("Enter the other Gmail address you want to sign in with.")
        );
        assert_eq!(spec.fields.len(), 1);
        assert_eq!(spec.fields[0].kind, UserFormFieldKind::Email);
        assert!(spec.fields[0].required);
        assert!(!spec.fields[0].masked());
        assert!(spec.is_unresolved());
        assert_eq!(spec.domain.as_deref(), Some("accounts.google.com"));
        assert_eq!(spec.live_host.as_deref(), Some("accounts.google.com"));
    }

    #[test]
    fn live_awaiting_paints_from_arguments_and_does_not_use_call_id_as_entry() {
        let spec = UserFormSpec::from_custom_event(&live_awaiting(None)).expect("card");
        assert!(is_user_form_awaiting(&live_awaiting(None)));
        assert_eq!(spec.call_id, "call-9");
        assert_eq!(spec.run_id, "run-1");
        assert!(
            spec.entry_id.is_empty(),
            "callId must not be sent as entryId: {}",
            spec.entry_id
        );
        assert!(!spec.has_gateway_entry_id());
        assert_eq!(spec.title, "Google account email");
        assert_eq!(spec.fields.len(), 2);
        assert!(spec.fields[1].masked());
        assert!(spec.is_unresolved());
        let mut filled = UserFormValues::default();
        filled.by_id.insert("email".into(), "you@gmail.com".into());
        filled.by_id.insert("password".into(), "s3cret-pass".into());
        assert!(
            !continue_enabled(&spec, &filled, true),
            "without a gateway entryId Continue stays gated"
        );
        assert!(!spec.can_post(true));
    }

    #[test]
    fn agui_event_id_is_not_the_gateway_card() {
        let mut event = live_awaiting(None);
        let arguments = event.get("arguments").cloned().unwrap();
        let object = event.as_object_mut().unwrap();
        object.insert("id".into(), json!("agui-event-1"));
        object.insert("formRequest".into(), arguments);
        let spec = UserFormSpec::from_custom_event(&event).expect("card");
        assert!(
            spec.entry_id.is_empty(),
            "CUSTOM id is not a gateway entryId even with formRequest: {}",
            spec.entry_id
        );
        assert_eq!(spec.call_id, "call-9");
    }

    #[test]
    fn awaiting_paints_arguments_and_takes_entry_id_from_send_message_value() {
        let mut event = live_awaiting(None);
        event.as_object_mut().unwrap().insert(
            "value".into(),
            json!({
                "kind": "send-message",
                "id": "e_from_gateway",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": null
            }),
        );
        let spec = UserFormSpec::from_custom_event(&event).expect("card");
        assert_eq!(spec.entry_id, "e_from_gateway");
        assert_eq!(spec.call_id, "call-9");
        assert!(spec.has_gateway_entry_id());
        assert_eq!(spec.fields.len(), 2, "arguments win over value for paint");
        assert!(spec.fields.iter().any(|f| f.masked()));
        let mut filled = UserFormValues::default();
        filled.by_id.insert("email".into(), "you@gmail.com".into());
        filled.by_id.insert("password".into(), "s3cret-pass".into());
        assert!(continue_enabled(&spec, &filled, true));
    }

    #[test]
    fn awaiting_without_arguments_still_paints_a_send_message_value() {
        let event = json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": REQUEST_USER_FORM_TOOL,
            "reason": "user-form",
            "value": {
                "kind": "send-message",
                "id": "e_entry-email",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": null
            }
        });
        let spec = UserFormSpec::from_custom_event(&event).expect("card");
        assert_eq!(spec.entry_id, "e_entry-email");
        assert_eq!(spec.call_id, "call-9");
        assert_eq!(spec.title, "Google account email");
        assert!(spec.is_unresolved());
        assert!(spec.has_gateway_entry_id());
    }

    #[test]
    fn awaiting_with_entry_id_can_post() {
        let spec = UserFormSpec::from_custom_event(&live_awaiting(Some("e_form"))).expect("card");
        assert_eq!(spec.entry_id, "e_form");
        assert_eq!(spec.call_id, "call-9");
        assert!(spec.has_gateway_entry_id());
        let mut filled = UserFormValues::default();
        filled.by_id.insert("email".into(), "you@gmail.com".into());
        filled.by_id.insert("password".into(), "s3cret-pass".into());
        assert!(continue_enabled(&spec, &filled, true));
        assert!(USER_FORM_SERVER_FILL_AVAILABLE);
    }

    #[test]
    fn d12fffc_top_level_entry_id_makes_buttons_live() {
        let spec = UserFormSpec::from_custom_event(&official_d12fffc_frame()).expect("card");
        assert!(is_user_form_awaiting(&official_d12fffc_frame()));
        assert_eq!(spec.entry_id, "e_form");
        assert_eq!(spec.call_id, "mock-form-1");
        assert_eq!(spec.run_id, "run-1");
        assert_ne!(spec.entry_id, spec.call_id);
        assert!(spec.has_gateway_entry_id());
        assert_eq!(spec.title, "Google account");
        assert_eq!(spec.fields.len(), 2);
        assert!(spec.fields[1].masked(), "password is secret by type");
        assert!(spec.can_post(true), "Open the screen / Dismiss go live");
        let empty = UserFormValues::default();
        assert!(
            !continue_enabled(&spec, &empty, true),
            "Continue stays gated while required fields are empty"
        );
        let mut filled = UserFormValues::default();
        filled
            .by_id
            .insert("email".into(), "ada@example.com".into());
        filled.by_id.insert("password".into(), "s3cret-pass".into());
        assert!(continue_enabled(&spec, &filled, true));
        assert!(
            !spec.can_post(false),
            "404 feature-detect keeps Open/Dismiss gated"
        );
        let submit = submit_request_body(&spec.entry_id, "cw_1", &filled);
        assert_eq!(submit["entryId"], "e_form");
        assert_ne!(submit["entryId"], spec.call_id);
        assert_eq!(submit["values"]["password"], "s3cret-pass");
        let dismiss = dismiss_request_body(&spec.entry_id, "cw_1", UserFormDismissMode::Dismissed);
        assert_eq!(dismiss["entryId"], "e_form");
        assert_eq!(dismiss["mode"], "dismissed");
        let dump = format!("{spec:?}");
        assert!(!dump.contains("s3cret-pass"), "{dump}");
    }

    #[test]
    fn form_request_alias_paints_when_arguments_are_absent() {
        let event = json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "mock-form-1",
            "tool": REQUEST_USER_FORM_TOOL,
            "reason": "user-form",
            "entryId": "e_form",
            "formRequest": {
                "title": "Google account",
                "fields": [
                    {"id": "email", "label": "Email", "type": "email", "required": true}
                ]
            }
        });
        let spec = UserFormSpec::from_custom_event(&event).expect("card");
        assert_eq!(spec.entry_id, "e_form");
        assert_eq!(spec.title, "Google account");
        assert!(spec.can_post(true));
    }

    #[test]
    fn exec_consent_awaiting_is_not_a_user_form() {
        let event = json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": "user_machine_shell",
            "reason": "exec-consent",
            "arguments": {"command": "ls"}
        });
        assert!(!is_user_form_awaiting(&event));
        assert!(UserFormSpec::from_custom_event(&event).is_none());
    }

    #[test]
    fn form_request_sibling_resolution_settles_the_card() {
        let idle = json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "entryId": "e1",
                "formRequest": google_email_request()
            }
        });
        let mut spec = UserFormSpec::from_custom_event(&idle).unwrap();
        let settled = json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "entryId": "e1",
                "formResolution": "submitted"
            }
        });
        spec.merge(UserFormSpec::from_custom_event(&settled).unwrap());
        assert_eq!(spec.title, "Google account email");
        assert_eq!(spec.effective_resolution(), Some(FormResolution::Submitted));
        assert_eq!(spec.pill(), Some("Submitted"));
        assert_eq!(
            spec.settled_body(),
            Some("Filled into the page. Secret values were never shown to your Bot.")
        );
    }

    #[test]
    fn resolution_table_matches_grok_bot_chrome() {
        let cases = [
            (FormResolution::Sending, "Submitting", "Submitting…"),
            (
                FormResolution::Submitted,
                "Submitted",
                "Filled into the page. Secret values were never shown to your Bot.",
            ),
            (
                FormResolution::FillFailed,
                "Not filled",
                "Could not fill into the page — it may have moved or changed. Secret values were never shown to your Bot.",
            ),
            (
                FormResolution::Escalated,
                "On the computer",
                "You chose to do this step on the computer.",
            ),
            (
                FormResolution::Dismissed,
                "Dismissed",
                "Dismissed without filling anything.",
            ),
        ];
        for (resolution, pill, body) in cases {
            assert_eq!(resolution.pill(), pill);
            assert_eq!(resolution.body(), body);
        }
    }

    #[test]
    fn widget_dismissed_without_resolution_is_dismissed() {
        let event = json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "entryId": "e1",
                "formRequest": google_email_request(),
                "widgetDismissed": true
            }
        });
        let spec = UserFormSpec::from_custom_event(&event).unwrap();
        assert!(!spec.is_unresolved());
        assert_eq!(spec.effective_resolution(), Some(FormResolution::Dismissed));
    }

    #[test]
    fn every_field_type_from_issue_17() {
        let event = json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "entryId": "kitchen",
                "formRequest": {
                    "title": "Kitchen sink",
                    "fields": [
                        {"id": "e", "label": "Email", "type": "email", "required": true},
                        {"id": "t", "label": "Phone", "type": "tel"},
                        {"id": "p", "label": "Password", "type": "password", "required": true, "value": "s3cret-pass"},
                        {"id": "o", "label": "Code", "type": "otp", "value": "123456"},
                        {"id": "n", "label": "Age", "type": "number"},
                        {"id": "d", "label": "When", "type": "date"},
                        {"id": "s", "label": "Pick", "type": "select", "options": ["A", {"label": "Bee", "value": "b"}]},
                        {"id": "x", "label": "Notes", "type": "textarea"},
                        {"id": "c", "label": "Remember", "type": "checkbox"},
                        {"id": "u", "label": "Name"},
                        {"id": "k", "label": "Token", "type": "text", "secret": true, "default": "leak-me"}
                    ]
                }
            }
        });
        let spec = UserFormSpec::from_custom_event(&event).unwrap();
        let kinds: Vec<_> = spec.fields.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            vec![
                UserFormFieldKind::Email,
                UserFormFieldKind::Tel,
                UserFormFieldKind::Password,
                UserFormFieldKind::Otp,
                UserFormFieldKind::Number,
                UserFormFieldKind::Date,
                UserFormFieldKind::Select,
                UserFormFieldKind::Textarea,
                UserFormFieldKind::Checkbox,
                UserFormFieldKind::Text,
                UserFormFieldKind::Text,
            ]
        );
        let password = spec.fields.iter().find(|f| f.id == "p").unwrap();
        assert!(password.masked());
        assert!(
            password.prefill.is_none(),
            "password default must be dropped"
        );
        let otp = spec.fields.iter().find(|f| f.id == "o").unwrap();
        assert!(otp.masked());
        assert!(otp.prefill.is_none());
        let token = spec.fields.iter().find(|f| f.id == "k").unwrap();
        assert!(token.secret);
        assert!(token.masked());
        assert!(token.prefill.is_none());
        let select = spec.fields.iter().find(|f| f.id == "s").unwrap();
        assert_eq!(select.options.len(), 2);
        assert_eq!(select.options[1].label, "Bee");
        let dump = format!("{spec:?}");
        assert!(
            !dump.contains("s3cret-pass"),
            "password must not appear in Debug: {dump}"
        );
        assert!(!dump.contains("123456"), "otp value in Debug: {dump}");
        assert!(!dump.contains("leak-me"), "secret default in Debug: {dump}");
    }

    #[test]
    fn continue_needs_server_fill_entry_id_and_required_fields() {
        let spec = UserFormSpec::parse(
            &json!({
                "entryId": "e1",
                "formRequest": google_email_request()
            }),
            None,
        )
        .unwrap();
        assert!(spec.has_gateway_entry_id());
        let empty = UserFormValues::default();
        let mut filled = UserFormValues::default();
        filled.by_id.insert("email".into(), "you@gmail.com".into());
        assert!(!continue_enabled(&spec, &empty, true));
        assert!(continue_enabled(
            &spec,
            &filled,
            USER_FORM_SERVER_FILL_AVAILABLE
        ));
        assert!(!continue_enabled(&spec, &filled, false));
        assert!(USER_FORM_SERVER_FILL_AVAILABLE);
    }

    #[test]
    fn required_checkbox_is_empty_until_true() {
        let spec = UserFormSpec::parse(
            &json!({
                "entryId": "e1",
                "formRequest": {
                    "title": "Agree",
                    "fields": [{"id": "ok", "label": "OK", "type": "checkbox", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        let mut values = UserFormValues::default();
        assert!(!spec.required_fields_filled(&values));
        values
            .by_id
            .insert("ok".into(), UserFormField::checkbox_wire(false).into());
        assert!(!spec.required_fields_filled(&values));
        values
            .by_id
            .insert("ok".into(), UserFormField::checkbox_wire(true).into());
        assert!(spec.required_fields_filled(&values));
    }

    #[test]
    fn masked_presence_stub_does_not_enable_continue_without_input_state() {
        let spec = UserFormSpec::from_custom_event(&official_d12fffc_frame()).expect("card");
        assert!(
            spec.fields
                .iter()
                .any(|field| field.masked() && field.required)
        );

        let mut display = UserFormValues::default();
        display
            .by_id
            .insert("email".into(), "ada@example.com".into());
        display
            .by_id
            .insert("password".into(), MASKED_PRESENCE_STUB.into());
        let empty_input_state = UserFormValues::default();
        assert!(
            !continue_enabled(&spec, &display, true),
            "display stub \"1\" is not a typed password"
        );
        assert!(
            !continue_enabled(&spec, &empty_input_state, true),
            "empty InputState keeps Continue gated"
        );

        let mut live = UserFormValues::default();
        live.by_id.insert("email".into(), "ada@example.com".into());
        live.by_id
            .insert("password".into(), "typed-in-input-state".into());
        assert!(
            continue_enabled(&spec, &live, true),
            "a real InputState value enables Continue"
        );
    }

    #[test]
    fn submit_after_sending_binds_form_resolution_never_idle() {
        assert_eq!(
            user_form_action_from_http(200, &Value::Null),
            UserFormActionReply::Empty,
            "classifier still reports 200-null as Empty"
        );
        assert_eq!(
            settle_user_form_http(UserFormVerb::Submit, &UserFormActionReply::Empty),
            UserFormHttpSettle::Paint(FormResolution::FillFailed),
            "200-null is disclosure, not Submitted"
        );
        assert_eq!(
            settle_user_form_http(UserFormVerb::Submit, &UserFormActionReply::AlreadyAnswered),
            UserFormHttpSettle::Paint(FormResolution::FillFailed)
        );
        let submitted = user_form_action_from_http(
            200,
            &json!({
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": "submitted"
            }),
        );
        match settle_user_form_http(UserFormVerb::Submit, &submitted) {
            UserFormHttpSettle::Merge(spec) => {
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Submitted));
                assert!(!spec.is_unresolved(), "collapsed: no idle fields");
            }
            other => panic!("expected Merge submitted, got {other:?}"),
        }
        let failed = user_form_action_from_http(
            200,
            &json!({
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": "fill_failed"
            }),
        );
        match settle_user_form_http(UserFormVerb::Submit, &failed) {
            UserFormHttpSettle::Merge(spec) => {
                assert_eq!(
                    spec.effective_resolution(),
                    Some(FormResolution::FillFailed)
                );
                assert!(!spec.is_unresolved());
            }
            other => panic!("expected Merge fill_failed, got {other:?}"),
        }
        let missing_entry =
            user_form_action_from_http(404, &json!({ "error": "form entry missing" }));
        assert_eq!(missing_entry, UserFormActionReply::MissingEntry);
        assert_eq!(
            settle_user_form_http(UserFormVerb::Submit, &missing_entry),
            UserFormHttpSettle::Paint(FormResolution::FillFailed),
            "404 form entry missing is Not filled, not idle fields"
        );
        for reply in [
            UserFormActionReply::Empty,
            UserFormActionReply::AlreadyAnswered,
            UserFormActionReply::MissingRoute,
            UserFormActionReply::MissingEntry,
            UserFormActionReply::MissingEntryId,
            submitted,
            failed,
        ] {
            assert!(
                !matches!(
                    settle_user_form_http(UserFormVerb::Submit, &reply),
                    UserFormHttpSettle::Restore
                ),
                "after Sending, never restore idle fields: {reply:?}"
            );
        }
        assert_eq!(
            settle_user_form_http(UserFormVerb::Submit, &UserFormActionReply::MissingEntryId),
            UserFormHttpSettle::Paint(FormResolution::FillFailed)
        );
        assert_eq!(
            settle_user_form_http(UserFormVerb::Submit, &UserFormActionReply::MissingRoute),
            UserFormHttpSettle::Paint(FormResolution::FillFailed),
            "404 after Continue is Not filled, not idle fields"
        );
        let mut sending = UserFormSpec::parse(
            &json!({
                "entryId": "e_form",
                "formRequest": google_email_request()
            }),
            None,
        )
        .unwrap();
        sending.resolution = Some(FormResolution::Sending);
        assert!(
            !sending.is_unresolved(),
            "Sending is collapsed: no idle email/password fields"
        );
        sending.resolution = Some(FormResolution::Submitted);
        assert!(!sending.is_unresolved());
        assert_eq!(FormResolution::Submitted.pill(), "Submitted");
        assert!(
            FormResolution::Submitted
                .body()
                .contains("Filled into the page")
        );
        assert_eq!(FormResolution::FillFailed.pill(), "Not filled");
    }

    #[test]
    fn values_debug_does_not_print_secrets() {
        let spec = UserFormSpec::parse(
            &json!({
                "entryId": "e1",
                "formRequest": {
                    "title": "Login",
                    "fields": [
                        {"id": "email", "label": "Email", "type": "email"},
                        {"id": "password", "label": "Password", "type": "password"}
                    ]
                }
            }),
            None,
        )
        .unwrap();
        let mut values = UserFormValues::default();
        values.by_id.insert("email".into(), "a@b.com".into());
        values.by_id.insert("password".into(), "s3cret-pass".into());
        let dump = values.debug_without_secrets(&spec.fields);
        assert!(!dump.contains("s3cret-pass"));
        assert!(!dump.contains("a@b.com"));
        assert!(dump.contains("password:<masked>"));
        let debug = format!("{values:?}");
        assert!(
            !debug.contains("s3cret-pass"),
            "UserFormValues Debug leaked a secret: {debug}"
        );
        assert!(!debug.contains("a@b.com"));
    }

    #[test]
    fn a_generative_form_payload_is_not_a_user_form() {
        let value = json!({
            "component": "form",
            "title": "Next",
            "fields": [{"id": "go", "label": "Go", "options": ["Yes", "No"]}],
            "submit": "Send"
        });
        assert!(!is_user_form_event("ui", &value));
        assert!(!is_user_form_event("form", &value));
        assert!(
            UserFormSpec::from_custom_event(&json!({
                "type": "CUSTOM",
                "name": "ui",
                "value": value
            }))
            .is_none()
        );
    }

    #[test]
    fn tool_args_keep_call_id_off_entry_id() {
        let spec = UserFormSpec::from_tool_args(
            &json!({
                "formRequest": google_email_request()
            }),
            "call-9",
        )
        .unwrap();
        assert_eq!(spec.call_id, "call-9");
        assert!(
            spec.entry_id.is_empty(),
            "tool call id is not the gateway card: {}",
            spec.entry_id
        );
        assert!(!spec.has_gateway_entry_id());
        assert!(spec.is_unresolved());
    }

    #[test]
    fn http_404_entry_missing_is_not_missing_route() {
        assert_eq!(
            user_form_action_from_http(404, &Value::Null),
            UserFormActionReply::MissingRoute
        );
        assert_eq!(
            user_form_action_from_http(404, &json!({ "error": "form entry missing" })),
            UserFormActionReply::MissingEntry
        );
        assert_eq!(
            settle_user_form_http(UserFormVerb::Dismiss, &UserFormActionReply::MissingEntry),
            UserFormHttpSettle::Restore
        );
        assert_eq!(
            box_handoff_action_from_http(404, &json!({ "error": "form entry missing" })),
            BoxHandoffReply::Empty
        );
        assert_eq!(
            user_form_action_from_http(200, &Value::Null),
            UserFormActionReply::Empty
        );
        assert_eq!(
            user_form_action_from_http(200, &json!({ "alreadyAnswered": true })),
            UserFormActionReply::AlreadyAnswered
        );
        match user_form_action_from_http(
            200,
            &json!({
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": "submitted"
            }),
        ) {
            UserFormActionReply::Settled(spec) => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Submitted));
            }
            other => panic!("expected Settled, got {other:?}"),
        }
    }

    #[test]
    fn submit_body_carries_values_that_must_not_become_chat_text() {
        let mut values = UserFormValues::default();
        values
            .by_id
            .insert("email".into(), "ada@example.com".into());
        values.by_id.insert("password".into(), "s3cret-pass".into());
        let body = submit_request_body("e_form", "cw_1", &values);
        assert_eq!(body["entryId"], "e_form");
        assert_eq!(body["agentId"], "cw_1");
        assert_eq!(body["values"]["password"], "s3cret-pass");
        let dismiss = dismiss_request_body("e_form", "cw_1", UserFormDismissMode::Escalated);
        assert_eq!(dismiss["mode"], "escalated");
        assert!(dismiss.get("values").is_none());
    }

    #[test]
    fn dismiss_escalated_stores_handoff_entry_id_not_as_form_id() {
        match user_form_action_from_http(
            200,
            &json!({
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": "escalated",
                "widgetDismissed": true,
                "handoffEntryId": "e_hand"
            }),
        ) {
            UserFormActionReply::Settled(spec) => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.handoff_entry_id.as_deref(), Some("e_hand"));
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Escalated));
                assert_eq!(spec.pill(), Some("On the computer"));
                let resolve = resolve_handoff_request_body(
                    spec.handoff_entry_id.as_deref().unwrap(),
                    "cw_1",
                    BoxHandoffResolution::HandedBack,
                );
                assert_eq!(resolve["entryId"], "e_hand");
                assert_ne!(resolve["entryId"], spec.entry_id);
                assert_eq!(resolve["resolution"], "handed_back");
                assert!(resolve.get("values").is_none());
            }
            other => panic!("expected Settled, got {other:?}"),
        }
    }

    #[test]
    fn bind_call_peers_settles_the_call_twin_not_a_later_otp() {
        let call_twin = UserFormSpec::from_tool_args(
            &json!({ "formRequest": google_email_request() }),
            "call-9",
        )
        .unwrap();
        let entry = UserFormSpec::from_custom_event(&live_awaiting(Some("e_form"))).unwrap();
        let otp = UserFormSpec::from_custom_event(&json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "call-otp",
            "entryId": "e_otp",
            "reason": "user-form",
            "arguments": {
                "title": "Enter the code",
                "fields": [{"id": "otp", "label": "Code", "type": "otp", "required": true}]
            }
        }))
        .unwrap();
        assert_eq!(call_twin.card_key(), "call-9");
        assert_eq!(entry.card_key(), "e_form");
        assert!(entry.shares_call_id("call-9"));
        assert!(!otp.shares_call_id("call-9"));
        let mut specs = [call_twin, entry, otp];
        bind_call_peers(&mut specs, "call-9", FormResolution::Submitted);
        assert_eq!(
            specs[0].effective_resolution(),
            Some(FormResolution::Submitted)
        );
        assert!(!specs[0].is_unresolved(), "call twin must collapse");
        assert_eq!(
            specs[1].effective_resolution(),
            Some(FormResolution::Submitted)
        );
        assert!(specs[2].is_unresolved(), "OTP is a different call");
        assert_eq!(specs[2].entry_id, "e_otp");
    }

    #[test]
    fn resolve_handoff_never_posts_the_form_card_id() {
        let body = resolve_handoff_request_body("e_hand", "cw_1", BoxHandoffResolution::Declined);
        assert_eq!(body["entryId"], "e_hand");
        assert_eq!(body["agentId"], "cw_1");
        assert_eq!(body["resolution"], "declined");
        assert_ne!(body["entryId"], "e_form");
        assert_eq!(
            box_handoff_action_from_http(404, &Value::Null),
            BoxHandoffReply::MissingRoute
        );
        assert_eq!(
            box_handoff_action_from_http(200, &Value::Null),
            BoxHandoffReply::Empty
        );
        assert_eq!(
            box_handoff_action_from_http(200, &json!({ "alreadyAnswered": true })),
            BoxHandoffReply::AlreadyAnswered
        );
        assert_eq!(
            box_handoff_action_from_http(
                200,
                &json!({ "id": "e_hand", "boxResolution": "handed_back" })
            ),
            BoxHandoffReply::Settled
        );
    }
}
