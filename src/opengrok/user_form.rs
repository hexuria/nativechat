//! User-form: in-chat credentials that fill the box page, never the bot.
//!
//! This is **not** generative UI [`super::gen_ui::FormSpec`] (choice chips that
//! dump values into `send_message`). It is **not** secret-request (a connector
//! vault with **Save securely**). It is **not** Computer handoff (`Take over` /
//! `I'm done` / `Skip`). Password and other secret fields typed here must never
//! enter AG-UI `content` or sqlite.
//!
//! # PR1 vs PR2
//!
//! PR1 paints idle and settled cards. Continue is gated: the server cannot yet
//! fill the box page ([opengrok-server#138](https://github.com/hexuria/opengrok-server/issues/138)).
//! NativeChat does not invent `submitUserForm` / `dismissUserForm` against a
//! 404. PR2 wires Continue / Open the screen / Dismiss after those verbs exist.
//!
//! [`USER_FORM_SERVER_FILL_AVAILABLE`] is the gate. It is `false` in this
//! build on purpose.
//!
//! # Mount (provisional CUSTOM name)
//!
//! Official desktop uses Electron `send-message` `type: "user-form"`. NativeChat
//! is AG-UI-first. Until the server emits a documented event, this client
//! mounts from **`CUSTOM` `name: "user-form"`** (`USER_FORM_CUSTOM`).
//!
//! Idle fixture:
//!
//! ```json
//! {
//!   "type": "CUSTOM",
//!   "name": "user-form",
//!   "value": {
//!     "entryId": "entry-1",
//!     "formRequest": {
//!       "title": "Google account email",
//!       "instruction": "Enter the other Gmail address you want to sign in with.",
//!       "fields": [
//!         { "id": "email", "label": "Email or phone", "type": "email", "required": true }
//!       ],
//!       "domain": "accounts.google.com",
//!       "liveHost": "accounts.google.com"
//!     },
//!     "formResolution": null
//!   }
//! }
//! ```
//!
//! The official send-message envelope is also accepted as `value` (`kind`,
//! `id`, `message.type: "user-form"`, `formResolution` as a sibling of
//! `message`). A later CUSTOM with the same `entryId` and a `formResolution`
//! settles the card.
//!
//! Also recognised, still provisional: `form-request`, `form-resolution`,
//! `request-user-form`, and a `TOOL_CALL_*` named `request_user_form` whose
//! arguments are a `formRequest`.
//!
//! `formFieldOutcomes` is hashed on the official client, not painted. We parse
//! it only so it cannot be mistaken for field values.

use serde_json::Value;

/// Provisional AG-UI CUSTOM `name` for a user-form card. Documented for PR2
/// and for whoever emits the fixture while the server catch-up is in flight.
pub const USER_FORM_CUSTOM: &str = "user-form";

/// Tool name official 0.29 uses while the bot waits on the card. Activity:
/// **Waiting for you**, distinct from **Waiting for approval**.
pub const REQUEST_USER_FORM_TOOL: &str = "request_user_form";

/// Working-line copy while an unresolved user-form is on screen.
pub const WAITING_FOR_YOU: &str = "Waiting for you";

/// Continue must not claim a fill until the server can do it. Flip this in PR2
/// when `submitUserForm` (or the AG-UI REST twin) exists and fills the box.
pub const USER_FORM_SERVER_FILL_AVAILABLE: bool = false;

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
    /// Optimistic local pill. Not durable server success. PR1 does not set this
    /// from Continue, because Continue never fires a fill.
    Sending,
    Submitted,
    FillFailed,
    Escalated,
    Dismissed,
}

impl FormResolution {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "sending" => Self::Sending,
            "submitted" => Self::Submitted,
            "fill_failed" | "fill-failed" | "not_filled" | "not-filled" => Self::FillFailed,
            "escalated" | "on_screen" | "on-screen" => Self::Escalated,
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

    /// Pill copy from NativeChat #17.
    pub fn pill(self) -> &'static str {
        match self {
            Self::Sending => "Sending",
            Self::Submitted => "Submitted",
            Self::FillFailed => "Not filled",
            Self::Escalated => "On screen",
            Self::Dismissed => "Dismissed",
        }
    }

    /// Body copy from NativeChat #17.
    pub fn body(self) -> &'static str {
        match self {
            Self::Sending => "Sending…",
            Self::Submitted => "Filled into the page. Secret values were never shown to your Bot.",
            Self::FillFailed => {
                "Could not fill into the page — it may have moved or changed. Secret values were never shown to your Bot."
            }
            Self::Escalated => "You chose to do this step on the screen instead.",
            Self::Dismissed => "Dismissed without filling anything.",
        }
    }
}

/// A user-form card in the transcript. Field values are not stored on this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFormSpec {
    pub entry_id: String,
    pub title: String,
    pub instruction: Option<String>,
    pub fields: Vec<UserFormField>,
    pub domain: Option<String>,
    pub live_host: Option<String>,
    pub resolution: Option<FormResolution>,
    pub widget_dismissed: bool,
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

    /// Continue is enabled only when the server can fill **and** every required
    /// field has a value. PR1 keeps the server gate off, so this is always false
    /// in the app; tests pass `server_fill` to cover both halves.
    pub fn continue_enabled(&self, values: &UserFormValues, server_fill: bool) -> bool {
        server_fill && self.required_fields_filled(values)
    }

    pub fn required_fields_filled(&self, values: &UserFormValues) -> bool {
        self.fields
            .iter()
            .filter(|field| field.required)
            .all(|field| values.filled(field))
    }

    /// Fold a later event for the same entry onto this card (resolution, or a
    /// fuller request). Secret values are not carried.
    pub fn merge(&mut self, incoming: UserFormSpec) {
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
    }

    pub fn from_custom_event(event: &Value) -> Option<Self> {
        let name = event.get("name").and_then(Value::as_str).unwrap_or("");
        let value = event.get("value").unwrap_or(event);
        if !is_user_form_event(name, value) {
            return None;
        }
        Self::parse(value, entry_id_hint(event, value))
    }

    pub fn from_tool_args(args: &Value, tool_call_id: &str) -> Option<Self> {
        if !looks_like_user_form_value(args) && args.get("fields").is_none() {
            return None;
        }
        let hint = if tool_call_id.is_empty() {
            None
        } else {
            Some(tool_call_id.to_string())
        };
        Self::parse(args, hint)
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
            .or_else(|| string_field(value, "entryId"))
            .or_else(|| string_field(value, "id"))
            .or_else(|| {
                value
                    .get("message")
                    .and_then(|message| string_field(message, "id"))
            })
            .unwrap_or_else(|| "user-form".to_string());
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
        })
    }
}

/// Values typed on the idle card. The transcript view stores only presence
/// (`"1"`) for masked fields. Tests may put a secret here to prove Debug and
/// `agui_messages` never echo it. Never copy this map onto a
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
}

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

fn entry_id_hint(event: &Value, value: &Value) -> Option<String> {
    string_field(value, "entryId")
        .or_else(|| string_field(value, "id"))
        .or_else(|| string_field(event, "entryId"))
        .or_else(|| string_field(event, "id"))
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| string_field(message, "id"))
        })
        .or_else(|| {
            event
                .get("toolCallId")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
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

/// Continue's two gates, for tests and for the renderer. Never "fill succeeded".
pub fn continue_enabled(spec: &UserFormSpec, values: &UserFormValues, server_fill: bool) -> bool {
    spec.continue_enabled(values, server_fill)
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

    #[test]
    fn official_send_message_envelope_parses_idle_card() {
        let event = json!({
            "type": "CUSTOM",
            "name": USER_FORM_CUSTOM,
            "value": {
                "kind": "send-message",
                "id": "entry-email",
                "message": {
                    "type": "user-form",
                    "formRequest": google_email_request()
                },
                "formResolution": null
            }
        });
        let spec = UserFormSpec::from_custom_event(&event).expect("idle card");
        assert_eq!(spec.entry_id, "entry-email");
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
    fn resolution_table_matches_issue_17() {
        let cases = [
            (FormResolution::Sending, "Sending", "Sending…"),
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
                "On screen",
                "You chose to do this step on the screen instead.",
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
    fn continue_needs_server_fill_and_required_fields() {
        let spec = UserFormSpec::parse(
            &json!({
                "entryId": "e1",
                "formRequest": google_email_request()
            }),
            None,
        )
        .unwrap();
        let empty = UserFormValues::default();
        let mut filled = UserFormValues::default();
        filled.by_id.insert("email".into(), "you@gmail.com".into());
        assert!(
            !continue_enabled(&spec, &filled, USER_FORM_SERVER_FILL_AVAILABLE),
            "PR1 must keep Continue gated"
        );
        assert!(!continue_enabled(&spec, &empty, true));
        assert!(continue_enabled(&spec, &filled, true));
        assert!(!USER_FORM_SERVER_FILL_AVAILABLE);
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
    fn tool_args_parse_request_user_form() {
        let spec = UserFormSpec::from_tool_args(
            &json!({
                "formRequest": google_email_request()
            }),
            "call-9",
        )
        .unwrap();
        assert_eq!(spec.entry_id, "call-9");
        assert!(spec.is_unresolved());
    }
}
