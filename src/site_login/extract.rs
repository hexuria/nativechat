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

/// The site a card belongs to, as the vault keys it: the registrable origin of the
/// card's domain, else of the live host.
pub fn login_origin(spec: &UserFormSpec) -> Option<String> {
    spec.domain
        .as_deref()
        .and_then(registrable_origin)
        .or_else(|| spec.live_host.as_deref().and_then(registrable_origin))
}

/// The two fields a saved login is typed into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginFields {
    pub username_id: String,
    pub password_id: String,
}

/// What a card can take from the vault: a login (name and password on one page), an
/// authenticator code (one otp field), or a passkey (no fields at all, marked by the Bot).
///
/// A sign-in that asks for the name and the password on two pages (Google's) is two cards,
/// and each takes half of a login. `Username` is the name page: one field a name could go in
/// and no password, where a pick fills the name alone. `Password` is the page after it: one
/// password field and nothing a name could go in, where a pick fills the password the way a
/// login's is filled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardTarget {
    Login(LoginFields),
    Username { username_id: String },
    Password { password_id: String },
    Code { code_id: String },
    Passkey { register: bool },
}

impl CardTarget {
    /// The field a pick's name is typed into, where the card asks for one.
    pub fn name_field(&self) -> Option<&str> {
        match self {
            Self::Login(fields) => Some(&fields.username_id),
            Self::Username { username_id } => Some(username_id),
            _ => None,
        }
    }

    /// The field a pick's secret goes to: a password, or the code minted from a seed. The
    /// card shows dots there, never the secret.
    pub fn secret_field(&self) -> Option<&str> {
        match self {
            Self::Login(fields) => Some(&fields.password_id),
            Self::Password { password_id } => Some(password_id),
            Self::Code { code_id } => Some(code_id),
            _ => None,
        }
    }

    /// The field the account list belongs to: the name where the card asks for one, else the
    /// one field a pick fills. A passkey card has no field and lists its passkeys on the card.
    pub fn list_field(&self) -> Option<&str> {
        self.name_field().or_else(|| self.secret_field())
    }
}

pub fn card_target(spec: &UserFormSpec) -> Option<CardTarget> {
    if spec.challenge_kind.as_deref() == Some("passkey") {
        return Some(CardTarget::Passkey {
            register: spec.passkey_mode.as_deref() == Some("register"),
        });
    }
    if let Some(fields) = login_fields(spec) {
        return Some(CardTarget::Login(fields));
    }
    let of_kind = |kind: UserFormFieldKind| {
        spec.fields
            .iter()
            .filter(|field| field.kind == kind)
            .collect::<Vec<_>>()
    };
    match (
        of_kind(UserFormFieldKind::Otp).as_slice(),
        of_kind(UserFormFieldKind::Password).as_slice(),
    ) {
        ([code], []) => Some(CardTarget::Code {
            code_id: code.id.clone(),
        }),
        // One password field, and (as `login_fields` found) nothing a name could go in.
        ([], [password]) if !another_challenge(spec) => Some(CardTarget::Password {
            password_id: password.id.clone(),
        }),
        ([], []) if !another_challenge(spec) => {
            name_page_field(spec).map(|username_id| CardTarget::Username { username_id })
        }
        // Two codes, a code beside a password, or two passwords (a sign-up or a change).
        _ => None,
    }
}

/// A card takes a whole saved login when it has one password field and a field for the name
/// beside it (`name_field_in`). A card with only one of the two is a page of a two-step
/// sign-in instead ([`card_target`]).
pub fn login_fields(spec: &UserFormSpec) -> Option<LoginFields> {
    // Two password fields is a sign-up or a change, not a login.
    let mut passwords = spec
        .fields
        .iter()
        .filter(|field| field.kind == UserFormFieldKind::Password);
    let password_id = passwords.next()?.id.clone();
    if passwords.next().is_some() {
        return None;
    }
    let username_id = name_field_in(spec)?;
    Some(LoginFields {
        username_id,
        password_id,
    })
}

/// The fields a name could go in: email, plain text or phone, and not masked.
fn name_candidates(spec: &UserFormSpec) -> impl Iterator<Item = &UserFormField> {
    spec.fields.iter().filter(|field| {
        matches!(
            field.kind,
            UserFormFieldKind::Email | UserFormFieldKind::Text | UserFormFieldKind::Tel
        ) && !field.masked()
    })
}

/// The field for the name: an email field, else one named like one (email, user, login,
/// phone), else the first plain text or phone field.
fn name_field_in(spec: &UserFormSpec) -> Option<String> {
    let mut email = None;
    let mut named = None;
    let mut first = None;
    for field in name_candidates(spec) {
        if field.kind == UserFormFieldKind::Email && email.is_none() {
            email = Some(field.id.clone());
        }
        let key = format!("{} {}", field.id, field.label).to_ascii_lowercase();
        if named.is_none()
            && (key.contains("email")
                || key.contains("user")
                || key.contains("login")
                || key.contains("phone"))
        {
            named = Some(field.id.clone());
        }
        if first.is_none() {
            first = Some(field.id.clone());
        }
    }
    email.or(named).or(first)
}

/// The name page's one field. Exactly one field a name could go in, so the same rules as a
/// login's pick it with nothing to choose between: two such fields are a form (a first and a
/// last name, say), not the page that asks who is signing in.
fn name_page_field(spec: &UserFormSpec) -> Option<String> {
    if name_candidates(spec).count() != 1 {
        return None;
    }
    name_field_in(spec)
}

/// The Bot marks a code, a captcha or a page outside its box with a challenge kind of its own,
/// and neither page of a sign-in is one of those. A code the Bot put in a text or password
/// field is still a code, and is offered no name and no password.
fn another_challenge(spec: &UserFormSpec) -> bool {
    matches!(
        spec.challenge_kind.as_deref(),
        Some("otp" | "captcha" | "outside_sandbox")
    )
}

/// Username + password + origin from the card the person just continued.
/// None when there is no password, no username, or no origin to hang metadata on.
pub fn save_candidate(spec: &UserFormSpec, values: &UserFormValues) -> Option<PendingSave> {
    let origin = login_origin(spec)?;
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

    fn field(id: &str, kind: UserFormFieldKind) -> serde_json::Value {
        let kind = match kind {
            UserFormFieldKind::Email => "email",
            UserFormFieldKind::Password => "password",
            UserFormFieldKind::Otp => "otp",
            UserFormFieldKind::Tel => "tel",
            UserFormFieldKind::Checkbox => "checkbox",
            _ => "text",
        };
        json!({"id": id, "label": id, "type": kind, "required": true})
    }

    fn card_with(fields: Vec<serde_json::Value>) -> UserFormSpec {
        UserFormSpec::parse(
            &json!({
                "entryId": "e_login",
                "formRequest": {
                    "title": "Log in",
                    "domain": "The-Internet.herokuapp.com",
                    "fields": fields
                }
            }),
            None,
        )
        .expect("form")
    }

    #[test]
    fn a_login_card_names_its_two_fields() {
        let spec = card_with(vec![
            field("username", UserFormFieldKind::Text),
            field("password", UserFormFieldKind::Password),
        ]);
        assert_eq!(
            login_fields(&spec),
            Some(LoginFields {
                username_id: "username".to_string(),
                password_id: "password".to_string()
            })
        );
        assert_eq!(
            login_origin(&spec).as_deref(),
            Some("the-internet.herokuapp.com")
        );
    }

    #[test]
    fn the_email_field_wins_over_a_plain_text_field() {
        let spec = card_with(vec![
            field("nickname", UserFormFieldKind::Text),
            field("email", UserFormFieldKind::Email),
            field("password", UserFormFieldKind::Password),
        ]);
        assert_eq!(
            login_fields(&spec).map(|f| f.username_id).as_deref(),
            Some("email")
        );
    }

    #[test]
    fn a_sign_up_card_with_two_password_fields_takes_no_saved_login() {
        let spec = card_with(vec![
            field("email", UserFormFieldKind::Email),
            field("password", UserFormFieldKind::Password),
            field("confirm", UserFormFieldKind::Password),
        ]);
        assert_eq!(login_fields(&spec), None);
    }

    #[test]
    fn a_code_card_and_a_passkey_card_have_their_own_targets() {
        let code = card_with(vec![field("code", UserFormFieldKind::Otp)]);
        assert_eq!(
            card_target(&code),
            Some(CardTarget::Code {
                code_id: "code".to_string()
            })
        );
        let mut passkey = card_with(vec![]);
        passkey.challenge_kind = Some("passkey".to_string());
        passkey.passkey_mode = Some("register".to_string());
        assert_eq!(
            card_target(&passkey),
            Some(CardTarget::Passkey { register: true })
        );
        let login = card_with(vec![
            field("username", UserFormFieldKind::Text),
            field("password", UserFormFieldKind::Password),
        ]);
        assert!(matches!(card_target(&login), Some(CardTarget::Login(_))));
        assert_eq!(
            card_target(&card_with(vec![
                field("first", UserFormFieldKind::Text),
                field("last", UserFormFieldKind::Text)
            ])),
            None
        );
    }

    /// The name and the password of one login, on one page. A card with only one of the two
    /// is a page of a two-step sign-in, and takes that half ([`card_target`]).
    #[test]
    fn a_whole_login_needs_the_name_and_one_password() {
        assert_eq!(
            login_fields(&card_with(vec![field(
                "password",
                UserFormFieldKind::Password
            )])),
            None
        );
        assert_eq!(
            login_fields(&card_with(vec![
                field("code", UserFormFieldKind::Otp),
                field("password", UserFormFieldKind::Password)
            ])),
            None
        );
        assert_eq!(
            login_fields(&card_with(vec![field("username", UserFormFieldKind::Text)])),
            None
        );
    }

    /// Google asks for the email on one page and the password on the next, and the Bot raises
    /// a card for each; the owner's first card was "Google sign-in — email", one "Email or
    /// phone" field. Neither card holds a whole login. Each is one page of one, its list sits
    /// under its one field, and a pick fills that field and no other.
    #[test]
    fn the_two_pages_of_a_two_step_sign_in_are_told_apart() {
        let email = UserFormSpec::parse(
            &json!({
                "entryId": "e_email",
                "formRequest": {
                    "title": "Google sign-in — email",
                    "domain": "accounts.google.com",
                    "fields": [
                        {"id": "email", "label": "Email or phone", "type": "email", "required": true}
                    ]
                }
            }),
            None,
        )
        .expect("form");
        let name_page = card_target(&email).expect("the name page");
        assert_eq!(
            name_page,
            CardTarget::Username {
                username_id: "email".to_string()
            }
        );
        assert_eq!(
            (
                name_page.list_field(),
                name_page.name_field(),
                name_page.secret_field()
            ),
            (Some("email"), Some("email"), None)
        );
        assert_eq!(login_origin(&email).as_deref(), Some("google.com"));

        // The Bot marks the second page as a password challenge; unmarked it is the same page.
        let mut password = card_with(vec![field("password", UserFormFieldKind::Password)]);
        for mark in [None, Some("password")] {
            password.challenge_kind = mark.map(str::to_string);
            let password_page = card_target(&password).expect("the password page");
            assert_eq!(
                password_page,
                CardTarget::Password {
                    password_id: "password".to_string()
                }
            );
            assert_eq!(
                (
                    password_page.list_field(),
                    password_page.name_field(),
                    password_page.secret_field()
                ),
                (Some("password"), None, Some("password"))
            );
        }

        // The name page's field is found the way a login's name field is: named like a name,
        // or the only field there is, whatever the Bot typed it as. A box to tick beside it
        // does not change the page.
        for (id, kind) in [
            ("username", UserFormFieldKind::Text),
            ("phone", UserFormFieldKind::Tel),
            ("account-id", UserFormFieldKind::Text),
        ] {
            assert_eq!(
                card_target(&card_with(vec![
                    field(id, kind),
                    field("remember", UserFormFieldKind::Checkbox)
                ])),
                Some(CardTarget::Username {
                    username_id: id.to_string()
                }),
                "{id}"
            );
        }
    }

    /// Only a page of a sign-in is read as one. Two fields a name could go in are a form, not
    /// the page asking who is signing in; and a card the Bot marked as a code, a captcha or a
    /// page outside its box is that, whatever its one field was typed as.
    #[test]
    fn a_card_that_is_no_page_of_a_sign_in_takes_neither_half() {
        assert_eq!(
            card_target(&card_with(vec![
                field("email", UserFormFieldKind::Email),
                field("nickname", UserFormFieldKind::Text)
            ])),
            None
        );
        for mark in ["otp", "captcha", "outside_sandbox"] {
            for kind in [UserFormFieldKind::Text, UserFormFieldKind::Password] {
                let mut card = card_with(vec![field("answer", kind)]);
                card.challenge_kind = Some(mark.to_string());
                assert_eq!(card_target(&card), None, "{mark} in a {kind:?} field");
            }
        }
    }

    /// What the two-step reading leaves as it was: a sign-up or a password change (two
    /// password fields) takes nothing, with a name beside them or without; a code card takes a
    /// code, with a name beside it too; two codes, or a code beside a password, take nothing.
    #[test]
    fn sign_up_change_and_code_cards_keep_their_targets() {
        let sign_up = card_with(vec![
            field("email", UserFormFieldKind::Email),
            field("password", UserFormFieldKind::Password),
            field("confirm", UserFormFieldKind::Password),
        ]);
        assert_eq!(card_target(&sign_up), None);
        let change = card_with(vec![
            field("current", UserFormFieldKind::Password),
            field("new", UserFormFieldKind::Password),
        ]);
        assert_eq!(card_target(&change), None);
        let code = Some(CardTarget::Code {
            code_id: "code".to_string(),
        });
        assert_eq!(
            card_target(&card_with(vec![field("code", UserFormFieldKind::Otp)])),
            code
        );
        assert_eq!(
            card_target(&card_with(vec![
                field("email", UserFormFieldKind::Email),
                field("code", UserFormFieldKind::Otp)
            ])),
            code
        );
        assert_eq!(
            card_target(&card_with(vec![
                field("code", UserFormFieldKind::Otp),
                field("password", UserFormFieldKind::Password)
            ])),
            None
        );
        assert_eq!(
            card_target(&card_with(vec![
                field("one", UserFormFieldKind::Otp),
                field("two", UserFormFieldKind::Otp)
            ])),
            None
        );
    }
}
