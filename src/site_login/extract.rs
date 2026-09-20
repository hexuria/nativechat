//! Pull origin / username / password from the **local** user-form.
//!
//! The save prompt uses these values. The server must not echo a password.
//! Password is never copied onto a `ChatPart`.

use crate::opengrok::{
    MASKED_PRESENCE_STUB, UserFormField, UserFormFieldKind, UserFormSpec, UserFormValues,
};

use super::origin::registrable_origin;

/// In-memory secret waiting on Save / Not now. Never sqlite, never the tree.
pub struct PendingSave {
    pub form_entry_id: String,
    pub origin: String,
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for PendingSave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingSave")
            .field("form_entry_id", &self.form_entry_id)
            .field("origin", &self.origin)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Username + password + origin from the card the person just continued.
/// None when there is no password, no username, or no origin to hang metadata on.
pub fn save_candidate(spec: &UserFormSpec, values: &UserFormValues) -> Option<PendingSave> {
    let origin = spec
        .domain
        .as_deref()
        .and_then(registrable_origin)
        .or_else(|| spec.live_host.as_deref().and_then(registrable_origin))?;
    let username = username_from(spec, values)?;
    let password = password_from(spec, values)?;
    let form_entry_id = if spec.has_gateway_entry_id() {
        spec.entry_id.clone()
    } else {
        spec.card_key().to_string()
    };
    Some(PendingSave {
        form_entry_id,
        origin,
        username,
        password,
    })
}

fn live(field: &UserFormField, values: &UserFormValues) -> Option<String> {
    let raw = values.by_id.get(&field.id)?.trim();
    if raw.is_empty() || raw == MASKED_PRESENCE_STUB {
        return None;
    }
    Some(raw.to_string())
}

fn password_from(spec: &UserFormSpec, values: &UserFormValues) -> Option<String> {
    spec.fields.iter().find_map(|field| {
        if field.kind != UserFormFieldKind::Password {
            return None;
        }
        live(field, values)
    })
}

fn username_from(spec: &UserFormSpec, values: &UserFormValues) -> Option<String> {
    let mut email = None;
    let mut named = None;
    let mut first_text = None;
    for field in &spec.fields {
        if field.kind == UserFormFieldKind::Password
            || field.kind == UserFormFieldKind::Otp
            || field.kind == UserFormFieldKind::Checkbox
            || field.masked()
        {
            continue;
        }
        let Some(value) = live(field, values) else {
            continue;
        };
        if field.kind == UserFormFieldKind::Email && email.is_none() {
            email = Some(value.clone());
        }
        let key = format!("{} {}", field.id, field.label).to_ascii_lowercase();
        if named.is_none()
            && (key.contains("email")
                || key.contains("user")
                || key.contains("login")
                || key.contains("phone"))
        {
            named = Some(value.clone());
        }
        if first_text.is_none() {
            first_text = Some(value);
        }
    }
    email.or(named).or(first_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn google_form() -> UserFormSpec {
        UserFormSpec::parse(
            &json!({
                "entryId": "e_form",
                "formRequest": {
                    "title": "Google account",
                    "domain": "https://accounts.google.com",
                    "fields": [
                        {"id": "email", "label": "Email", "type": "email", "required": true},
                        {"id": "password", "label": "Password", "type": "password", "required": true}
                    ]
                }
            }),
            None,
        )
        .expect("form")
    }

    #[test]
    fn candidate_uses_local_values_and_redacts_debug() {
        let spec = google_form();
        let mut values = UserFormValues::default();
        values
            .by_id
            .insert("email".into(), "ada@example.com".into());
        values.by_id.insert("password".into(), "s3cret-pass".into());
        let pending = save_candidate(&spec, &values).expect("candidate");
        assert_eq!(pending.origin, "google.com");
        assert_eq!(pending.username, "ada@example.com");
        assert_eq!(pending.password, "s3cret-pass");
        assert_eq!(pending.form_entry_id, "e_form");
        assert!(!format!("{pending:?}").contains("s3cret-pass"));
    }

    #[test]
    fn presence_stub_is_not_a_password() {
        let spec = google_form();
        let mut values = UserFormValues::default();
        values
            .by_id
            .insert("email".into(), "ada@example.com".into());
        values
            .by_id
            .insert("password".into(), MASKED_PRESENCE_STUB.into());
        assert!(save_candidate(&spec, &values).is_none());
    }
}
