//! Site-login protocol. Locked CUSTOM names:
//! `credential.offer_save`, `credential.request`, `credential.result`.
//!
//! Payloads never carry a password. The host saves from values already in the
//! local user-form. `credential.result.status = filled` means a **session
//! broker** put cookies/profile on Box — not that a password was typed into
//! agent-observable Chromium (OpenMausBot #255 / PLAN rev2: CDP and
//! screenshots can read `input.value`).
//!
//! A.0 posts `missing` / `error` / `denied` when there is no broker. A.1 is
//! the broker. This module does not type into Box.

use serde_json::{Value, json};

/// CUSTOM `name` — offer to save. Value: `{ origin, username, formEntryId }`.
pub const CREDENTIAL_OFFER_SAVE: &str = "credential.offer_save";
/// CUSTOM `name` — reuse a saved login. Value: `{ origin, username? }`.
pub const CREDENTIAL_REQUEST: &str = "credential.request";
/// Outbound CUSTOM / REST name. Value: `{ status, credentialId?, requestId? }`.
pub const CREDENTIAL_RESULT: &str = "credential.result";

/// Provisional AG-UI path. Documented until OpenGrok pins one.
pub const CREDENTIAL_RESULT_PATH: &str = "/ag-ui/credential/result";

/// `filled` is session-on-Box after the broker, never a typed password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialResultStatus {
    Filled,
    Denied,
    Missing,
    Error,
}

/// Local transcript settle for `credential.request`, independent of the
/// REST `credential.result` status. A.0 never paints [`Self::Filled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialRequestResolution {
    /// Not now.
    Denied,
    /// Use saved login — confirmed locally. Broker-off A.0 stays here even
    /// when REST posts `error` (login exists, no session broker).
    Used,
    /// Use saved with no matching Settings→Logins row, or metadata without
    /// a Keychain hit. Must not look like success. REST posts `missing`.
    Missing,
    /// Session broker put cookies/profile on Box (A.1).
    Filled,
    /// A later message moved the thread on; the server closed the request
    /// when that message arrived. Nothing was denied.
    Superseded,
}

impl CredentialRequestResolution {
    pub fn from_allow(allow: bool) -> Self {
        if allow { Self::Used } else { Self::Denied }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Used => "used",
            Self::Missing => "missing",
            Self::Filled => "filled",
            Self::Superseded => "superseded",
        }
    }

    /// Pill copy on the folded card — same chrome as user-form Submitted /
    /// Dismissed.
    pub fn pill(self) -> &'static str {
        match self {
            Self::Denied => "Dismissed",
            Self::Used => "Used saved login",
            Self::Missing => "None saved",
            Self::Filled => "Filled",
            Self::Superseded => "Superseded",
        }
    }

    pub fn body(self) -> &'static str {
        match self {
            Self::Denied => "Dismissed without using a saved login.",
            Self::Used => {
                "Confirmed a saved login. NativeChat did not type a password into the computer."
            }
            Self::Missing => "No saved login for this site.",
            Self::Filled => "Session restored on the computer. NativeChat did not type a password.",
            Self::Superseded => "Moved on to your next message.",
        }
    }
}

impl CredentialResultStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filled => "filled",
            Self::Denied => "denied",
            Self::Missing => "missing",
            Self::Error => "error",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "filled" => Some(Self::Filled),
            "denied" => Some(Self::Denied),
            "missing" => Some(Self::Missing),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// Transcript save prompt. No password field — the secret stays in host memory
/// until Keychain write, then is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveLoginSpec {
    pub form_entry_id: String,
    pub origin: String,
    pub username: String,
}

impl SaveLoginSpec {
    pub fn from_event(event: &Value) -> Option<Self> {
        if custom_name(event) != CREDENTIAL_OFFER_SAVE {
            return None;
        }
        let value = event_value(event);
        let origin = string_at(&value, "origin").or_else(|| string_at(event, "origin"))?;
        let username = string_at(&value, "username").or_else(|| string_at(event, "username"))?;
        let form_entry_id = string_at(&value, "formEntryId")
            .or_else(|| string_at(&value, "form_entry_id"))
            .or_else(|| string_at(event, "formEntryId"))
            .or_else(|| string_at(event, "entryId"))
            .unwrap_or_default();
        if origin.is_empty() || username.is_empty() {
            return None;
        }
        Some(Self {
            form_entry_id,
            origin,
            username,
        })
    }

    pub fn card_id(&self) -> String {
        save_login_card_id(&self.form_entry_id)
    }
}

/// Confirm-to-reuse card. No password. A.0 answers this without filling Box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRequestSpec {
    pub request_id: String,
    pub origin: String,
    pub username: Option<String>,
    pub run_id: String,
    /// None = idle (Use saved / Not now still on screen).
    pub resolution: Option<CredentialRequestResolution>,
}

impl CredentialRequestSpec {
    pub fn from_event(event: &Value) -> Option<Self> {
        if custom_name(event) != CREDENTIAL_REQUEST {
            return None;
        }
        let value = event_value(event);
        let origin = string_at(&value, "origin").or_else(|| string_at(event, "origin"))?;
        if origin.is_empty() {
            return None;
        }
        let request_id = string_at(&value, "requestId")
            .or_else(|| string_at(&value, "request_id"))
            .or_else(|| string_at(event, "requestId"))
            .or_else(|| string_at(event, "callId"))
            .or_else(|| string_at(event, "id"))
            .unwrap_or_default();
        if request_id.is_empty() {
            return None;
        }
        let username = string_at(&value, "username").or_else(|| string_at(event, "username"));
        let run_id = string_at(event, "runId").unwrap_or_default();
        Some(Self {
            request_id,
            origin,
            username: username.filter(|name| !name.is_empty()),
            run_id,
            resolution: None,
        })
    }

    pub fn card_id(&self) -> String {
        credential_request_card_id(&self.request_id)
    }

    pub fn is_unresolved(&self) -> bool {
        self.resolution.is_none()
    }

    pub fn is_settled(&self) -> bool {
        self.resolution.is_some()
    }

    pub fn pill(&self) -> Option<&'static str> {
        self.resolution.map(CredentialRequestResolution::pill)
    }

    pub fn body(&self) -> Option<&'static str> {
        self.resolution.map(CredentialRequestResolution::body)
    }
}

/// Body for `POST /ag-ui/credential/result`. Never a password.
pub fn credential_result_body(
    status: CredentialResultStatus,
    request_id: &str,
    credential_id: Option<&str>,
    agent_id: &str,
) -> Value {
    let mut body = json!({
        "status": status.as_str(),
        "requestId": request_id,
        "agentId": agent_id,
    });
    if let Some(id) = credential_id.filter(|id| !id.is_empty()) {
        body["credentialId"] = json!(id);
    }
    body
}

pub fn save_login_card_id(form_entry_id: &str) -> String {
    format!("save-login-{form_entry_id}")
}

pub fn save_login_save_id(form_entry_id: &str) -> String {
    format!("save-login-save-{form_entry_id}")
}

pub fn save_login_skip_id(form_entry_id: &str) -> String {
    format!("save-login-skip-{form_entry_id}")
}

pub fn credential_request_card_id(request_id: &str) -> String {
    format!("credential-request-{request_id}")
}

pub fn credential_request_allow_id(request_id: &str) -> String {
    format!("credential-request-allow-{request_id}")
}

pub fn credential_request_deny_id(request_id: &str) -> String {
    format!("credential-request-deny-{request_id}")
}

pub fn credential_request_pill_id(request_id: &str) -> String {
    format!("credential-request-pill-{request_id}")
}

pub fn is_credential_custom_name(name: &str) -> bool {
    matches!(
        name.trim(),
        CREDENTIAL_OFFER_SAVE | CREDENTIAL_REQUEST | CREDENTIAL_RESULT
    )
}

fn custom_name(event: &Value) -> &str {
    event.get("name").and_then(Value::as_str).unwrap_or("")
}

fn event_value(event: &Value) -> Value {
    event.get("value").cloned().unwrap_or(Value::Null)
}

fn string_at(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// What A.0 may post after the person answers `credential.request`.
///
/// Never `filled`: that status is the session broker (A.1). Confirming while
/// the broker is off is `error` when a login is stored, `missing` when it is
/// not. Deny is `denied`. This function does not take a password.
pub fn result_without_broker(
    allow: bool,
    have_metadata: bool,
    have_secret: bool,
) -> CredentialResultStatus {
    if !allow {
        return CredentialResultStatus::Denied;
    }
    if !have_metadata || !have_secret {
        return CredentialResultStatus::Missing;
    }
    CredentialResultStatus::Error
}

/// Transcript fold after Use saved / Not now.
///
/// No matching Settings→Logins row → [`CredentialRequestResolution::Missing`]
/// (not Used). A.0 still never returns [`CredentialRequestResolution::Filled`].
pub fn fold_credential_answer(
    allow: bool,
    have_matching_login: bool,
) -> CredentialRequestResolution {
    if !allow {
        return CredentialRequestResolution::Denied;
    }
    if have_matching_login {
        CredentialRequestResolution::Used
    } else {
        CredentialRequestResolution::Missing
    }
}

/// Paint an idle Use-saved card only when Settings→Logins has a row for the
/// request origin (and username, when named). Empty vault → do not offer.
/// `logins_ready` is false until the first sqlite list lands, so a request
/// that races startup is not auto-missed against an empty in-memory vec.
pub fn keep_credential_request_offer(
    settled: bool,
    have_matching_login: bool,
    auto_missed: bool,
    logins_ready: bool,
) -> bool {
    if auto_missed {
        return false;
    }
    if settled {
        return true;
    }
    if !logins_ready {
        return true;
    }
    have_matching_login
}

/// Build the save prompt from values the host already has. Never takes a
/// password — the secret stays in `PendingSave` until Keychain write.
///
/// `follow_run` / SSE overwrite chat parts from the assembler. Call this so
/// the prompt does not wait for `credential.offer_save` (and never for a
/// server-echoed password, which must not exist).
pub fn save_login_from_local(
    form_entry_id: &str,
    origin: &str,
    username: &str,
    already_saved: bool,
    form_submitted: bool,
    already_has_card: bool,
) -> Option<SaveLoginSpec> {
    if already_saved || !form_submitted || already_has_card {
        return None;
    }
    if form_entry_id.is_empty() || origin.is_empty() || username.is_empty() {
        return None;
    }
    Some(SaveLoginSpec {
        form_entry_id: form_entry_id.to_string(),
        origin: origin.to_string(),
        username: username.to_string(),
    })
}

/// Keep a journalled `credential.offer_save` only when the host still has
/// the local secret. No pending secret → drop the card.
pub fn keep_local_save_offer(have_pending: bool, already_saved: bool) -> bool {
    have_pending && !already_saved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_save_drops_password_even_if_the_frame_smuggled_one() {
        let event = json!({
            "type": "CUSTOM",
            "name": CREDENTIAL_OFFER_SAVE,
            "value": {
                "origin": "google.com",
                "username": "ada@example.com",
                "formEntryId": "e_form",
                "password": "s3cret-pass"
            }
        });
        let spec = SaveLoginSpec::from_event(&event).expect("offer");
        assert_eq!(spec.origin, "google.com");
        assert_eq!(spec.username, "ada@example.com");
        assert_eq!(spec.form_entry_id, "e_form");
        let debug = format!("{spec:?}");
        assert!(!debug.contains("s3cret-pass"));
        assert!(
            !format!("{event:?}").is_empty(),
            "the fixture itself still has the smuggled field"
        );
    }

    #[test]
    fn request_has_no_password_and_needs_request_id() {
        let event = json!({
            "type": "CUSTOM",
            "name": CREDENTIAL_REQUEST,
            "runId": "run-1",
            "callId": "req-9",
            "value": {
                "origin": "github.com",
                "username": "ada",
                "password": "nope"
            }
        });
        let spec = CredentialRequestSpec::from_event(&event).expect("request");
        assert_eq!(spec.request_id, "req-9");
        assert_eq!(spec.origin, "github.com");
        assert_eq!(spec.username.as_deref(), Some("ada"));
        assert!(spec.is_unresolved());
        assert!(!format!("{spec:?}").contains("nope"));
    }

    #[test]
    fn request_without_id_is_dropped() {
        let event = json!({
            "type": "CUSTOM",
            "name": CREDENTIAL_REQUEST,
            "value": { "origin": "example.com" }
        });
        assert!(CredentialRequestSpec::from_event(&event).is_none());
    }

    #[test]
    fn result_body_never_includes_a_password_key() {
        let body = credential_result_body(
            CredentialResultStatus::Error,
            "req-1",
            Some("cred-1"),
            "cw_1",
        );
        assert_eq!(body["status"], "error");
        assert_eq!(body["requestId"], "req-1");
        assert_eq!(body["credentialId"], "cred-1");
        assert!(body.get("password").is_none());
        assert!(body.get("secret").is_none());
        assert!(body.get("values").is_none());
    }

    #[test]
    fn a0_without_broker_never_returns_filled() {
        for allow in [true, false] {
            for meta in [true, false] {
                for secret in [true, false] {
                    let status = result_without_broker(allow, meta, secret);
                    assert_ne!(
                        status,
                        CredentialResultStatus::Filled,
                        "allow={allow} meta={meta} secret={secret}"
                    );
                }
            }
        }
        assert_eq!(
            result_without_broker(true, true, true),
            CredentialResultStatus::Error
        );
        assert_eq!(
            result_without_broker(true, false, false),
            CredentialResultStatus::Missing
        );
        assert_eq!(
            result_without_broker(false, true, true),
            CredentialResultStatus::Denied
        );
    }

    #[test]
    fn locked_event_names_are_exact() {
        assert_eq!(CREDENTIAL_OFFER_SAVE, "credential.offer_save");
        assert_eq!(CREDENTIAL_REQUEST, "credential.request");
        assert_eq!(CREDENTIAL_RESULT, "credential.result");
    }

    #[test]
    fn folded_pills_match_user_form_chrome_words() {
        assert_eq!(CredentialRequestResolution::Denied.pill(), "Dismissed");
        assert_eq!(CredentialRequestResolution::Used.pill(), "Used saved login");
        assert_eq!(CredentialRequestResolution::Missing.pill(), "None saved");
        assert_eq!(CredentialRequestResolution::Filled.pill(), "Filled");
        assert_eq!(
            CredentialRequestResolution::from_allow(true),
            CredentialRequestResolution::Used,
            "A.0 Use saved folds Used, never Filled without the broker"
        );
        assert_eq!(
            CredentialRequestResolution::from_allow(false),
            CredentialRequestResolution::Denied
        );
        assert_eq!(
            credential_request_pill_id("req-9"),
            "credential-request-pill-req-9"
        );
        assert_eq!(
            fold_credential_answer(true, true),
            CredentialRequestResolution::Used
        );
        assert_eq!(
            fold_credential_answer(true, false),
            CredentialRequestResolution::Missing,
            "Use saved with no row must not look like success"
        );
        assert_eq!(
            fold_credential_answer(false, false),
            CredentialRequestResolution::Denied
        );
        assert_ne!(
            fold_credential_answer(true, false),
            CredentialRequestResolution::Filled
        );
        assert_ne!(
            fold_credential_answer(true, true),
            CredentialRequestResolution::Filled
        );
        assert!(!keep_credential_request_offer(false, false, false, true));
        assert!(keep_credential_request_offer(false, false, false, false));
        assert!(keep_credential_request_offer(false, true, false, true));
        assert!(keep_credential_request_offer(true, false, false, true));
        assert!(!keep_credential_request_offer(false, true, true, true));
    }

    #[test]
    fn save_prompt_comes_from_local_values_not_a_server_password() {
        let spec = save_login_from_local(
            "e_form",
            "google.com",
            "ada@example.com",
            false,
            true,
            false,
        )
        .expect("offer");
        assert_eq!(spec.origin, "google.com");
        assert_eq!(spec.username, "ada@example.com");
        assert_eq!(spec.form_entry_id, "e_form");
        assert!(save_login_from_local("e_form", "google.com", "ada", true, true, false).is_none());
        assert!(
            save_login_from_local("e_form", "google.com", "ada", false, false, false).is_none()
        );
        assert!(save_login_from_local("e_form", "google.com", "ada", false, true, true).is_none());
        assert!(!keep_local_save_offer(false, false));
        assert!(keep_local_save_offer(true, false));
        assert!(!keep_local_save_offer(true, true));
    }
}
