//! Save-login protocol. The one locked CUSTOM name is `credential.offer_save`.
//!
//! Its payload never carries a password. The host saves from values already in
//! the local user-form. Using a saved login again is the login card's own
//! offer (see `user_form.rs`), not a protocol event.

use serde_json::Value;

/// CUSTOM `name` — offer to save. Value: `{ origin, username, formEntryId }`.
pub const CREDENTIAL_OFFER_SAVE: &str = "credential.offer_save";

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

pub fn save_login_card_id(form_entry_id: &str) -> String {
    format!("save-login-{form_entry_id}")
}

pub fn save_login_save_id(form_entry_id: &str) -> String {
    format!("save-login-save-{form_entry_id}")
}

pub fn save_login_skip_id(form_entry_id: &str) -> String {
    format!("save-login-skip-{form_entry_id}")
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
    use serde_json::json;

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
    fn locked_event_name_is_exact() {
        assert_eq!(CREDENTIAL_OFFER_SAVE, "credential.offer_save");
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
