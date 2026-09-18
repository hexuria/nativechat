use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{ACCEPT, CACHE_CONTROL, HeaderValue};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::error::OpenGrokError;
use super::types::{
    Account, AguiMessage, Coworker, CoworkerPatch, ModelCatalogue, ProfileUpdate,
    error_message_from_body,
};

/// The cookie the server puts the access JWT in. It is also what goes out as the Bearer.
const ACCESS_COOKIE: &str = "og_access";

/// The cookie that can be traded for a new access token, and therefore the difference between a
/// session the app can still rescue by itself and one only a person can.
const REFRESH_COOKIE: &str = "og_refresh";

/// How close to its expiry an access token is refreshed rather than used. Enough for a request
/// that is slow to leave; short enough that a token is not thrown away while it still works.
const REFRESH_SLACK: std::time::Duration = std::time::Duration::from_secs(30);

/// What the app says when the session is gone and the server sent no sentence of its own.
///
/// Addressed to the person, because signing in is a thing only a person can do, and it names no
/// machine: nothing is broken and nothing is out of reach.
const SIGNED_OUT_MESSAGE: &str = "OpenGrok does not know who this app is signed in as.";

/// Whether the access token needs replacing before the next request goes out.
///
/// The argument is seconds left on the token, and `None` is the answer that matters: it means
/// the question could not be answered at all, which almost always means there is no token to
/// ask about. This used to read `None` as "no deadline, carry on", and that one reading is the
/// bug — the cookie jar drops a cookie the moment it expires, so a token that has run out does
/// not present as a token with no time left, it presents as nothing at all. The request then
/// went out with no `Authorization` header and the server, with no principal to bill, held it.
fn needs_refresh(seconds_left: Option<i64>) -> bool {
    match seconds_left {
        Some(left) => left < REFRESH_SLACK.as_secs() as i64,
        None => true,
    }
}

#[derive(Clone)]
pub struct OpenGrokClient {
    base: Url,
    http: Client,
    jar: Arc<Jar>,
    session_path: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct StoredSession {
    base_url: String,
    cookies: Vec<(String, String)>,
}

impl OpenGrokClient {
    pub fn new(base_url: &str) -> Result<Self, OpenGrokError> {
        let base = Url::parse(base_url)
            .map_err(|e| OpenGrokError::message(format!("invalid OpenGrok URL {base_url}: {e}")))?;
        let jar = Arc::new(Jar::default());
        let http = Client::builder()
            .cookie_provider(jar.clone())
            .tcp_nodelay(true)
            .build()
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
        Ok(Self {
            base,
            http,
            jar,
            session_path: None,
        })
    }

    pub fn with_session_file(mut self, path: PathBuf) -> Self {
        self.session_path = Some(path);
        self
    }

    fn cookie_pairs(&self) -> Vec<(String, String)> {
        let Some(header) = CookieStore::cookies(self.jar.as_ref(), &self.base) else {
            return Vec::new();
        };
        let Ok(raw) = header.to_str() else {
            return Vec::new();
        };
        raw.split(';')
            .filter_map(|pair| {
                let pair = pair.trim();
                let (name, value) = pair.split_once('=')?;
                let name = name.trim();
                if name.is_empty() {
                    return None;
                }
                Some((name.to_string(), value.trim().to_string()))
            })
            .collect()
    }

    fn restore_cookies(&self, pairs: &[(String, String)]) {
        let headers: Vec<HeaderValue> = pairs
            .iter()
            .filter_map(|(name, value)| {
                HeaderValue::from_str(&format!("{name}={value}; Path=/")).ok()
            })
            .collect();
        if headers.is_empty() {
            return;
        }
        CookieStore::set_cookies(self.jar.as_ref(), &mut headers.iter(), &self.base);
    }

    pub fn load_session(&self) -> bool {
        let Some(path) = self.session_path.as_ref() else {
            return false;
        };
        let Ok(bytes) = fs::read(path) else {
            return false;
        };
        let Ok(stored) = serde_json::from_slice::<StoredSession>(&bytes) else {
            return false;
        };
        if stored.base_url.trim_end_matches('/') != self.base.as_str().trim_end_matches('/') {
            return false;
        }
        if stored.cookies.is_empty() {
            return false;
        }
        self.restore_cookies(&stored.cookies);
        true
    }

    pub fn save_session(&self) {
        let Some(path) = self.session_path.as_ref() else {
            return;
        };
        let cookies = self.cookie_pairs();
        if cookies.is_empty() {
            self.clear_session();
            return;
        }
        let stored = StoredSession {
            base_url: self.base.as_str().to_string(),
            cookies,
        };
        let Ok(json) = serde_json::to_vec(&stored) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = write_private(path, &json);
    }

    pub fn clear_session(&self) {
        if let Some(path) = self.session_path.as_ref() {
            let _ = fs::remove_file(path);
        }
    }

    /// One cookie out of the jar, by name.
    ///
    /// The jar only ever hands back cookies that have not expired yet — `Jar::cookies` goes
    /// through `cookie_store`'s `matches`, which filters on `is_expired` — so a name that is
    /// missing here means one of two things that look identical from the outside: it was never
    /// set, or it was set and its moment has passed. Both are "the app is holding nothing".
    fn cookie(&self, name: &str) -> Option<String> {
        let header = CookieStore::cookies(self.jar.as_ref(), &self.base)?;
        let raw = header.to_str().ok()?;
        for pair in raw.split(';') {
            let pair = pair.trim();
            if let Some((found, value)) = pair.split_once('=')
                && found.trim() == name
            {
                return Some(value.trim().to_string());
            }
        }
        None
    }

    /// AG-UI `principal_from_bearer` only reads `Authorization`, not cookies.
    /// The console login stores the same JWT as `og_access`; send it as Bearer
    /// the way the desktop sends it as `x-opengrok-account` on Seam A.
    fn access_token(&self) -> Option<String> {
        self.cookie(ACCESS_COOKIE)
    }

    /// Whether the app is holding anything at all that could authenticate a request: a token to
    /// send, or the refresh cookie that can still be traded for one.
    ///
    /// This is not "is the person signed in". That is the app's belief about itself, and the
    /// belief is exactly what went wrong: the roster stayed on screen, the composer stayed
    /// ready, and the jar had nothing left in it. This question is about what is actually there
    /// to put in the header, and it is answerable without a round trip.
    pub fn has_session(&self) -> bool {
        self.access_token().is_some() || self.cookie(REFRESH_COOKIE).is_some()
    }

    /// Seconds until the access token expires, read from the JWT's `exp` without verifying
    /// it — the server verifies; this only decides whether to refresh first.
    ///
    /// `None` does not mean "plenty of time". It means the question could not be answered, and
    /// the commonest reason for that is that there is no token to ask about — see
    /// [`Self::ensure_fresh_token`], where reading `None` as "nothing to do" is the bug.
    fn token_seconds_left(&self) -> Option<i64> {
        use base64::Engine as _;
        let token = self.access_token()?;
        let payload = token.split('.').nth(1)?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .ok()?;
        let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let exp = claims.get("exp")?.as_i64()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        Some(exp - now)
    }

    /// Refresh before a token dies, not after. An expired bearer is not always a 401: `/ag-ui`
    /// runs it as nobody and the turn is held with no word to the person. [`REFRESH_SLACK`] of
    /// slack covers a request that is slow to leave. Best effort; the request goes out either
    /// way and a 401 gets one more chance below.
    ///
    /// A token that could not be read at all counts as a token that needs replacing, and that
    /// sentence is the whole of today's bug. The check used to be "some time left, and less than
    /// the slack" — which is never true once the token is *gone*, because the jar drops a cookie
    /// the moment it expires. So an app left idle past the access token's lifetime read `None`
    /// here, did nothing about it, and sent the next turn with no `Authorization` header at all.
    /// The server had no principal to bill and held the turn, and its sentence about spend
    /// arrived in the transcript as a red line.
    async fn ensure_fresh_token(&self, path: &str) {
        if path.starts_with("/auth/") {
            return;
        }
        // Nothing in the jar to trade. This runs before every request, so asking anyway would
        // be a round trip per request whose answer is already here.
        if !self.has_session() {
            return;
        }
        if needs_refresh(self.token_seconds_left()) {
            let _ = self.refresh().await;
        }
    }

    fn url(&self, path: &str) -> Result<Url, OpenGrokError> {
        self.base
            .join(path)
            .map_err(|e| OpenGrokError::message(e.to_string()))
    }

    async fn send_json<T: Serialize>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&T>,
    ) -> Result<reqwest::Response, OpenGrokError> {
        let url = self.url(path)?;
        self.ensure_fresh_token(path).await;
        let build = |token: Option<String>| {
            let mut req = self.http.request(method.clone(), url.clone());
            if let Some(token) = token {
                req = req.bearer_auth(token);
            }
            if let Some(body) = body {
                req = req.json(body);
            }
            req
        };
        let mut response = build(self.access_token())
            .send()
            .await
            .map_err(|e| OpenGrokError::transport(&e))?;
        // A 401 on a signed-in session is a token that died between checks: refresh once and
        // send again. Auth routes are exempt, or a bad password would loop here.
        if response.status() == StatusCode::UNAUTHORIZED && !path.starts_with("/auth/") {
            if self.refresh().await.is_ok() {
                response = build(self.access_token())
                    .send()
                    .await
                    .map_err(|e| OpenGrokError::transport(&e))?;
            }
            // Still refused after the one thing the app can do about it on its own. The session
            // is gone, and that is decided here because here is where the route is known: the
            // same status on `/auth/login` is a wrong password, which is a verdict about what
            // somebody typed and belongs under the field they typed it in.
            if response.status() == StatusCode::UNAUTHORIZED {
                return Err(Self::signed_out_error(response).await);
            }
        }
        Ok(response)
    }

    /// A `401` read as the session being gone, keeping the server's own sentence.
    ///
    /// The sentence is kept and not matched on. The server has one for this case today and it
    /// may have a different one tomorrow; what makes this a signed-out failure is the status and
    /// the route, both of which the caller already knows.
    async fn signed_out_error(response: reqwest::Response) -> OpenGrokError {
        let body = response.text().await.unwrap_or_default();
        let message = error_message_from_body(&body);
        if message.trim().is_empty() {
            OpenGrokError::signed_out(SIGNED_OUT_MESSAGE)
        } else {
            OpenGrokError::signed_out(message)
        }
    }

    async fn read_error(response: reqwest::Response) -> OpenGrokError {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        // `from_server` rather than `status`, because some of what the server refuses with is
        // not a refusal at all: "the gateway could not be reached" is the server reporting a
        // machine it could not get to, which is a state and not a verdict about the request.
        OpenGrokError::from_server(Some(status), error_message_from_body(&body))
    }

    /// The body as `T` on a 2xx, the server's error otherwise.
    async fn json_or_error<T: serde::de::DeserializeOwned>(
        response: reqwest::Response,
    ) -> Result<T, OpenGrokError> {
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| OpenGrokError::transport(&e))
    }

    pub async fn health(&self) -> Result<(), OpenGrokError> {
        let url = self.url("/health")?;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| OpenGrokError::transport(&e))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    /// Cookie session. Request body is `{ email, password }`; tokens stay in the jar.
    pub async fn login(&self, email: &str, password: &str) -> Result<(), OpenGrokError> {
        let response = self
            .send_json(
                reqwest::Method::POST,
                "/auth/login",
                Some(&json!({ "email": email, "password": password })),
            )
            .await?;
        if response.status() == StatusCode::OK {
            self.save_session();
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    /// Trade the refresh cookie for a new access token. Sent directly rather than through
    /// `send_json`, which would refresh before refreshing.
    pub async fn refresh(&self) -> Result<(), OpenGrokError> {
        let url = self.url("/auth/refresh")?;
        let mut req = self.http.post(url);
        if let Some(token) = self.access_token() {
            req = req.bearer_auth(token);
        }
        let response = req.send().await.map_err(|e| OpenGrokError::transport(&e))?;
        if response.status().is_success() {
            self.save_session();
            Ok(())
        } else {
            let error = Self::read_error(response).await;
            // "session expired": the refresh token is gone, so the saved session is worthless
            // and every later request would try this again. Forget it; the next request fails
            // plainly with 401 and the person signs in.
            if error.is_unauthorized() {
                self.clear_session();
            }
            Err(error)
        }
    }

    pub async fn logout(&self) -> Result<(), OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::POST, "/auth/logout", None)
            .await?;
        self.clear_session();
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    pub async fn me(&self) -> Result<Account, OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::GET, "/account", None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn update_profile(&self, update: &ProfileUpdate) -> Result<Account, OpenGrokError> {
        let response = self
            .send_json(reqwest::Method::POST, "/account/profile", Some(update))
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn change_password(
        &self,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), OpenGrokError> {
        let response = self
            .send_json(
                reqwest::Method::POST,
                "/account/password",
                Some(&json!({
                    "currentPassword": current_password,
                    "newPassword": new_password,
                })),
            )
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    pub async fn list_coworkers(&self) -> Result<Vec<Coworker>, OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::GET, "/coworkers", None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn hire(&self, name: &str, model: Option<&str>) -> Result<Coworker, OpenGrokError> {
        let mut body = json!({ "name": name });
        if let Some(model) = model.filter(|m| !m.is_empty()) {
            body["model"] = json!(model);
        }
        let response = self
            .send_json(reqwest::Method::POST, "/coworkers", Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn list_models(&self) -> Result<ModelCatalogue, OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::GET, "/models", None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn delete_coworker(&self, coworker_id: &str) -> Result<(), OpenGrokError> {
        let path = format!("/coworkers/{coworker_id}");
        let response = self
            .send_json::<()>(reqwest::Method::DELETE, &path, None)
            .await?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(Self::read_error(response).await)
    }

    pub async fn patch_coworker(
        &self,
        coworker_id: &str,
        patch: &CoworkerPatch,
    ) -> Result<Coworker, OpenGrokError> {
        if patch.is_empty() {
            return Err(OpenGrokError::message("nothing to change".to_string()));
        }
        let path = format!("/coworkers/{coworker_id}");
        let response = self
            .send_json(reqwest::Method::PATCH, &path, Some(patch))
            .await?;
        Self::json_or_error(response).await
    }

    /// One turn. Desktop Grok Bot POSTs `/api/sendPrompt` then paints from `GET /events`.
    /// NativeChat is a new client: same coworker + transcript, `POST /ag-ui` SSE instead.
    ///
    /// A recipe the person put on this turn goes in `forwardedProps` beside the coworker, with
    /// the values its parameters were given. The messages are untouched by it.
    ///
    /// The run id is the caller's. The server keeps every frame a run emits under it and will
    /// hand the whole lot back from `GET /ag-ui/runs/{run_id}`, which is of no use whatever to a
    /// client that only learns the id from the frames it already saw: the one moment the id is
    /// needed is the moment the stream has been lost. So it is minted before the turn is sent,
    /// by whoever will have to ask about it later.
    pub async fn run_turn<F>(
        &self,
        coworker_id: &str,
        thread_id: &str,
        run_id: &str,
        messages: &[AguiMessage],
        recipe: Option<&TurnRecipe>,
        mut on_event: F,
    ) -> Result<String, OpenGrokError>
    where
        F: FnMut(&serde_json::Value),
    {
        let mut forwarded = json!({ "coworkerId": coworker_id });
        if let Some(recipe) = recipe {
            forwarded["recipe"] = Value::String(recipe.id.clone());
            forwarded["recipeValues"] = Value::Object(recipe.values.clone());
        }
        let body = json!({
            "threadId": thread_id,
            "runId": run_id,
            "messages": messages,
            "tools": super::gen_ui::agui_tools(),
            "forwardedProps": forwarded,
        });
        let url = self.url("/ag-ui")?;
        self.ensure_fresh_token("/ag-ui").await;
        // The turn does not leave without a credential on it. It used to: the header was
        // attached only `if let Some(token)`, and the `else` was to send the turn anyway. A turn
        // with no principal on it is a turn the server cannot bill to anybody, so it held it and
        // said so — a round trip, and a red line in the transcript, for something knowable here.
        let Some(token) = self.access_token() else {
            return Err(OpenGrokError::signed_out(SIGNED_OUT_MESSAGE));
        };
        let response = self
            .http
            .post(url)
            .header(ACCEPT, "text/event-stream")
            .header(CACHE_CONTROL, "no-cache")
            .json(&body)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| OpenGrokError::transport(&e))?;
        // `ensure_fresh_token` has already spent the app's one refresh, so a 401 here is the
        // session being gone rather than a token that aged out mid-flight. Read before the
        // general failure path, which would file it as a verdict about the turn.
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(Self::signed_out_error(response).await);
        }
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut assistant = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| OpenGrokError::transport(&e))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(idx) = buf.find("\n\n") {
                let frame = buf[..idx].to_string();
                buf = buf[idx + 2..].to_string();
                for line in frame.lines() {
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
                        continue;
                    };
                    let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if kind == "RUN_ERROR" {
                        let message = value
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("run failed");
                        // The stream itself is a `200`: the run began and the server ended it
                        // badly, and the sentence it ends with is the only thing that says
                        // whether the model refused or the gateway was never reached.
                        return Err(OpenGrokError::from_server(None, message));
                    }
                    if kind == "TEXT_MESSAGE_CONTENT" || kind == "TEXT_MESSAGE_CHUNK" {
                        if let Some(delta) = value.get("delta").and_then(|v| v.as_str()) {
                            assistant.push_str(delta);
                        }
                    }
                    on_event(&value);
                }
            }
        }
        Ok(assistant)
    }

    pub async fn answer_run(
        &self,
        run_id: &str,
        call_id: &str,
        approved: bool,
    ) -> Result<AnswerReply, OpenGrokError> {
        let path = format!("/ag-ui/runs/{run_id}/answer");
        let body = json!({ "call_id": call_id, "approved": approved });
        let response = self
            .send_json(reqwest::Method::POST, &path, Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    /// Fill the box page from the in-chat card. Account bearer. Not
    /// `/ag-ui/runs/{id}/answer` and not `submitSecret`. `entryId` is the
    /// gateway card id — an empty id is not sent as `callId`.
    pub async fn submit_user_form(
        &self,
        entry_id: &str,
        agent_id: &str,
        values: &super::user_form::UserFormValues,
    ) -> Result<super::user_form::UserFormActionReply, OpenGrokError> {
        if entry_id.trim().is_empty() {
            return Ok(super::user_form::UserFormActionReply::MissingEntryId);
        }
        let body = super::user_form::submit_request_body(entry_id, agent_id, values);
        let response = self
            .send_json(
                reqwest::Method::POST,
                super::user_form::USER_FORM_SUBMIT_PATH,
                Some(&body),
            )
            .await?;
        Self::user_form_action_response(response).await
    }

    /// Dismiss or escalate the card. `mode` is `dismissed` or `escalated`
    /// (Open the screen). Same `entryId` rule as submit.
    pub async fn dismiss_user_form(
        &self,
        entry_id: &str,
        agent_id: &str,
        mode: super::user_form::UserFormDismissMode,
    ) -> Result<super::user_form::UserFormActionReply, OpenGrokError> {
        if entry_id.trim().is_empty() {
            return Ok(super::user_form::UserFormActionReply::MissingEntryId);
        }
        let body = super::user_form::dismiss_request_body(entry_id, agent_id, mode);
        let response = self
            .send_json(
                reqwest::Method::POST,
                super::user_form::USER_FORM_DISMISS_PATH,
                Some(&body),
            )
            .await?;
        Self::user_form_action_response(response).await
    }

    /// Hand back / decline / timeout. `entry_id` is the handoff card from
    /// dismiss `handoffEntryId`, never the user-form id.
    pub async fn resolve_box_handoff(
        &self,
        handoff_entry_id: &str,
        agent_id: &str,
        resolution: super::user_form::BoxHandoffResolution,
    ) -> Result<super::user_form::BoxHandoffReply, OpenGrokError> {
        if handoff_entry_id.trim().is_empty() {
            return Ok(super::user_form::BoxHandoffReply::MissingEntryId);
        }
        let body =
            super::user_form::resolve_handoff_request_body(handoff_entry_id, agent_id, resolution);
        let response = self
            .send_json(
                reqwest::Method::POST,
                super::user_form::BOX_HANDOFF_RESOLVE_PATH,
                Some(&body),
            )
            .await?;
        Self::box_handoff_action_response(response).await
    }

    async fn user_form_action_response(
        response: reqwest::Response,
    ) -> Result<super::user_form::UserFormActionReply, OpenGrokError> {
        let status = response.status().as_u16();
        if status == 404 {
            return Ok(super::user_form::UserFormActionReply::MissingRoute);
        }
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        let text = response
            .text()
            .await
            .map_err(|e| OpenGrokError::transport(&e))?;
        let value: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::Null)
        };
        Ok(super::user_form::user_form_action_from_http(status, &value))
    }

    async fn box_handoff_action_response(
        response: reqwest::Response,
    ) -> Result<super::user_form::BoxHandoffReply, OpenGrokError> {
        let status = response.status().as_u16();
        if status == 404 {
            return Ok(super::user_form::BoxHandoffReply::MissingRoute);
        }
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        let text = response
            .text()
            .await
            .map_err(|e| OpenGrokError::transport(&e))?;
        let value: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::Null)
        };
        Ok(super::user_form::box_handoff_action_from_http(
            status, &value,
        ))
    }

    /// Stop a run that is still going.
    ///
    /// The turn is not the app's to abandon. A run drives a box — it opens pages and types into
    /// them — and closing the stream here would only stop the app watching it do that, which is
    /// the difference between a stop and looking away. So "stop" is a thing said to the server,
    /// and this is the saying of it.
    ///
    /// Idempotent by the route's own contract: stopping a run that has already ended answers as
    /// a success, so nothing has to be checked about the run before asking. A `404` is a run the
    /// server has never heard of or one belonging to somebody else, which from here means the
    /// same thing — there is nothing of ours left running under that id.
    pub async fn stop_run(&self, run_id: &str) -> Result<StopReply, OpenGrokError> {
        let path = format!("/ag-ui/runs/{run_id}/stop");
        let response = self
            .send_json::<()>(reqwest::Method::POST, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn replay_run(&self, run_id: &str) -> Result<RunReplay, OpenGrokError> {
        let path = format!("/ag-ui/runs/{run_id}");
        let response = self
            .send_json::<()>(reqwest::Method::GET, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    /// Every run this thread has, oldest first, with the frames each one emitted.
    ///
    /// This is the thread as the server has it, which is the only copy that survives the app
    /// being closed. `limit` is not politeness: a run carries every frame it emitted and a
    /// computer-use run's frames carry screenshots, so asking for a whole thread's history is
    /// asking for megabytes. Only the newest runs can disagree with what is already on disk.
    pub async fn replay_thread(
        &self,
        thread_id: &str,
        limit: usize,
    ) -> Result<ThreadReplay, OpenGrokError> {
        let path = format!("/ag-ui/threads/{thread_id}?limit={limit}");
        let response = self
            .send_json::<()>(reqwest::Method::GET, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn list_approvals(&self) -> Result<Vec<QueuedApproval>, OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::GET, "/ag-ui/approvals", None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn list_computers(&self) -> Result<Vec<ConnectedComputer>, OpenGrokError> {
        let machines = self.list_daemons().await?;
        let mut computers = Vec::new();
        for machine in machines {
            if machine.revoked {
                continue;
            }
            let stored = self
                .local_exec_mode(&machine.machine_id)
                .await
                .unwrap_or_default();
            let label = if machine.label.trim().is_empty() {
                "Computer".to_string()
            } else {
                machine.label
            };
            computers.push(ConnectedComputer {
                machine_id: machine.machine_id,
                label,
                mode: LocalExecMode::from_stored(&stored),
                this_machine: false,
                online: machine.connected,
            });
        }
        Ok(computers)
    }

    pub async fn list_daemons(&self) -> Result<Vec<DaemonMachine>, OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::GET, "/local-exec/daemon", None)
            .await?;
        let body: DaemonList = Self::json_or_error(response).await?;
        Ok(body.machines)
    }

    pub async fn coworker_computer(
        &self,
        coworker_id: &str,
    ) -> Result<CoworkerComputer, OpenGrokError> {
        let path = format!("/coworkers/{coworker_id}/computer");
        let response = self
            .send_json::<()>(reqwest::Method::GET, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    /// The coworker's screen right now: `{mime, base64, width, height}`, the same shape as the
    /// `image` on a `TOOL_CALL_RESULT`, so `ScreenshotSpec::from_frame` decodes both.
    pub async fn coworker_screen(&self, coworker_id: &str) -> Result<Value, OpenGrokError> {
        let path = format!("/coworkers/{coworker_id}/screen");
        let response = self
            .send_json::<()>(reqwest::Method::GET, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    /// Rebuild the coworker's computer on the newest image, keeping its files. The server
    /// answers at once; `coworker_computer` carries the phases.
    pub async fn update_coworker_computer(
        &self,
        coworker_id: &str,
    ) -> Result<CoworkerComputer, OpenGrokError> {
        let path = format!("/coworkers/{coworker_id}/computer/update");
        let response = self
            .send_json::<()>(reqwest::Method::POST, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    /// Destroy the coworker's computer, data and all, and start fresh.
    pub async fn reset_coworker_computer(
        &self,
        coworker_id: &str,
    ) -> Result<CoworkerComputer, OpenGrokError> {
        let path = format!("/coworkers/{coworker_id}/computer/reset");
        let response = self
            .send_json::<()>(reqwest::Method::POST, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn ensure_coworker_computer(
        &self,
        coworker_id: &str,
    ) -> Result<CoworkerComputer, OpenGrokError> {
        let path = format!("/coworkers/{coworker_id}/computer");
        let response = self
            .send_json::<()>(reqwest::Method::POST, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    /// Upload a taught task as a recipe: the tape (thinned first, see [`thin_tape`]) with a
    /// name and a description. The server keeps the tape as v1 and writes the filtered steps
    /// as v2. A tape over the size limit is refused here, before any of it goes out.
    pub async fn create_recipe(
        &self,
        name: &str,
        description: &str,
        raw: &[Value],
    ) -> Result<RecipeDetail, OpenGrokError> {
        let body = json!({
            "name": name,
            "description": description,
            "screen": { "width": RECIPE_SCREEN.0, "height": RECIPE_SCREEN.1 },
            "raw": raw,
        });
        let size = serde_json::to_vec(&body)
            .map(|bytes| bytes.len())
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
        if size > RECIPE_UPLOAD_LIMIT {
            return Err(OpenGrokError::message(format!(
                "The recording is {:.1} MB; a recipe can be at most 5 MB. Teach a shorter task.",
                size as f64 / (1024. * 1024.)
            )));
        }
        let response = self
            .send_json(reqwest::Method::POST, "/recipes", Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    /// The recipes the person can see: `mine`, `shared` (with them) or `org`; everything when
    /// `filter` is `None`.
    pub async fn list_recipes(
        &self,
        filter: Option<&str>,
    ) -> Result<Vec<RecipeSummary>, OpenGrokError> {
        let path = match filter {
            Some(filter) => format!("/recipes?filter={filter}"),
            None => "/recipes".to_string(),
        };
        let response = self
            .send_json::<()>(reqwest::Method::GET, &path, None)
            .await?;
        let body: RecipeList = Self::json_or_error(response).await?;
        Ok(body.recipes)
    }

    pub async fn recipe(&self, id: &str) -> Result<RecipeDetail, OpenGrokError> {
        let path = format!("/recipes/{id}");
        let response = self
            .send_json::<()>(reqwest::Method::GET, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn rename_recipe(
        &self,
        id: &str,
        name: &str,
        description: &str,
    ) -> Result<RecipeDetail, OpenGrokError> {
        let path = format!("/recipes/{id}");
        let body = json!({ "name": name, "description": description });
        let response = self
            .send_json(reqwest::Method::PUT, &path, Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    /// The edited steps as a new version of the recipe (the owner's).
    pub async fn add_recipe_version(
        &self,
        id: &str,
        steps: &[RecipeStep],
        note: &str,
    ) -> Result<RecipeDetail, OpenGrokError> {
        let path = format!("/recipes/{id}/versions");
        let body = json!({ "steps": steps, "note": note });
        let response = self
            .send_json(reqwest::Method::POST, &path, Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn share_recipe(
        &self,
        id: &str,
        target: &RecipeShareTarget,
    ) -> Result<RecipeDetail, OpenGrokError> {
        let path = format!("/recipes/{id}/share");
        let response = self
            .send_json(reqwest::Method::POST, &path, Some(target))
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn unshare_recipe(
        &self,
        id: &str,
        scope: &str,
        scope_id: &str,
    ) -> Result<RecipeDetail, OpenGrokError> {
        let path = format!("/recipes/{id}/share/{scope}/{scope_id}");
        let response = self
            .send_json::<()>(reqwest::Method::DELETE, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn accept_recipe(&self, id: &str) -> Result<RecipeDetail, OpenGrokError> {
        self.recipe_post(&format!("/recipes/{id}/accept")).await
    }

    pub async fn decline_recipe(&self, id: &str) -> Result<RecipeDetail, OpenGrokError> {
        self.recipe_post(&format!("/recipes/{id}/decline")).await
    }

    /// A bodiless POST that answers with the recipe's detail.
    async fn recipe_post(&self, path: &str) -> Result<RecipeDetail, OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::POST, path, None)
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn grant_recipe(
        &self,
        id: &str,
        coworker_id: &str,
    ) -> Result<RecipeDetail, OpenGrokError> {
        let path = format!("/recipes/{id}/grants");
        let body = json!({ "coworkerId": coworker_id });
        let response = self
            .send_json(reqwest::Method::POST, &path, Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn revoke_recipe_grant(
        &self,
        id: &str,
        coworker_id: &str,
    ) -> Result<RecipeDetail, OpenGrokError> {
        let path = format!("/recipes/{id}/grants/{coworker_id}");
        let response = self
            .send_json::<()>(reqwest::Method::DELETE, &path, None)
            .await?;
        Self::json_or_error(response).await
    }

    /// Play the recipe's current version on one of the person's bots and wait for the outcome.
    pub async fn run_recipe(
        &self,
        id: &str,
        coworker_id: &str,
    ) -> Result<RecipeRunResult, OpenGrokError> {
        let path = format!("/recipes/{id}/run");
        let body = json!({ "coworkerId": coworker_id });
        let response = self
            .send_json(reqwest::Method::POST, &path, Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn delete_recipe(&self, id: &str) -> Result<(), OpenGrokError> {
        let path = format!("/recipes/{id}");
        let response = self
            .send_json::<()>(reqwest::Method::DELETE, &path, None)
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    /// Remove one edited version and the runs that played it. The tape (v1) and the steps the
    /// server filtered from it (v2) are what the recipe is, so the server refuses those with a
    /// sentence saying as much; it is that sentence the page shows.
    pub async fn delete_recipe_version(&self, id: &str, version: u32) -> Result<(), OpenGrokError> {
        let path = format!("/recipes/{id}/versions/{version}");
        let response = self
            .send_json::<()>(reqwest::Method::DELETE, &path, None)
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    pub async fn enrol_daemon(
        &self,
        label: &str,
        machine_id: Option<&str>,
    ) -> Result<DaemonEnrol, OpenGrokError> {
        let mut body = json!({ "label": label });
        if let Some(machine_id) = machine_id.filter(|id| !id.is_empty()) {
            body["machineId"] = json!(machine_id);
        }
        let response = self
            .send_json(reqwest::Method::POST, "/local-exec/daemon", Some(&body))
            .await?;
        Self::json_or_error(response).await
    }

    pub async fn local_exec_mode(&self, machine_id: &str) -> Result<String, OpenGrokError> {
        let path = format!("/local-exec/policy?machine={machine_id}");
        let response = self
            .send_json::<()>(reqwest::Method::GET, &path, None)
            .await?;
        let body: LocalExecPolicyView = Self::json_or_error(response).await?;
        Ok(body.mode)
    }

    pub async fn set_local_exec_mode(
        &self,
        machine_id: &str,
        mode: &str,
    ) -> Result<(), OpenGrokError> {
        let body = json!({ "machineId": machine_id, "mode": mode });
        let response = self
            .send_json(reqwest::Method::PUT, "/local-exec/policy", Some(&body))
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    pub async fn add_local_exec_rule(
        &self,
        machine_id: &str,
        kind: &str,
        pattern: &str,
    ) -> Result<(), OpenGrokError> {
        let body = json!({
            "machineId": machine_id,
            "kind": kind,
            "pattern": pattern,
        });
        let response = self
            .send_json(
                reqwest::Method::POST,
                "/local-exec/policy/rule",
                Some(&body),
            )
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    pub async fn post_local_exec_responses(
        &self,
        daemon_token: &str,
        body: &serde_json::Value,
    ) -> Result<(), OpenGrokError> {
        let url = self.url("/local-exec/responses")?;
        let response = self
            .http
            .post(url)
            .bearer_auth(daemon_token)
            .json(body)
            .send()
            .await
            .map_err(|e| OpenGrokError::transport(&e))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    /// Hold `GET /local-exec/requests` and call `on_frame` for each JSON `data:` event.
    pub async fn stream_local_exec_requests<F>(
        &self,
        daemon_token: &str,
        mut on_frame: F,
    ) -> Result<(), OpenGrokError>
    where
        F: FnMut(serde_json::Value) + Send,
    {
        let url = self.url("/local-exec/requests")?;
        let response = self
            .http
            .get(url)
            .header(ACCEPT, "text/event-stream")
            .header(CACHE_CONTROL, "no-cache")
            .bearer_auth(daemon_token)
            .send()
            .await
            .map_err(|e| OpenGrokError::transport(&e))?;
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| OpenGrokError::transport(&e))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(idx) = buf.find("\n\n") {
                let frame = buf[..idx].to_string();
                buf = buf[idx + 2..].to_string();
                for line in frame.lines() {
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data.is_empty() {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                        on_frame(value);
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnswerReply {
    #[serde(rename = "alreadyAnswered", default)]
    pub already_answered: bool,
    #[serde(default)]
    pub continuing: bool,
}

/// What `POST /ag-ui/runs/{run_id}/stop` answers with: the run, and what it is now.
#[derive(Debug, Clone, Deserialize)]
pub struct StopReply {
    #[serde(rename = "runId", default)]
    pub run_id: String,
    /// `stopped`, including for a run that had already ended — the route is idempotent, so this
    /// says what is true of the run now rather than whether this call is what made it true.
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunReplay {
    #[serde(rename = "runId", default)]
    pub run_id: String,
    #[serde(default)]
    pub status: String,
    /// Why a `failed` run failed, in the server's words. A turn that died after the app stopped
    /// listening has no error to report from its own stream, so this is the only account of it.
    #[serde(default)]
    pub failure: Option<String>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub pending: Option<serde_json::Value>,
}

/// A thread's runs as the server kept them, from `GET /ag-ui/threads/{thread_id}`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThreadReplay {
    #[serde(rename = "threadId", default)]
    pub thread_id: String,
    /// Oldest first, so the list reads in the order things happened.
    #[serde(default)]
    pub runs: Vec<ThreadRun>,
}

/// One turn of a thread, as the server kept it.
///
/// `events` are the frames the run *emitted*, which is to say the coworker's side of the turn.
/// What the person typed was consumed by the run and never journaled, so a thread rebuilt from
/// these alone would be a conversation with one voice in it.
#[derive(Debug, Clone, Deserialize)]
pub struct ThreadRun {
    #[serde(rename = "runId", default)]
    pub run_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "startedAtMs", default)]
    pub started_at_ms: i64,
    #[serde(rename = "updatedAtMs", default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub failure: Option<String>,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
}

impl ThreadRun {
    /// The run has not ended: it is still working, or parked on a permission card.
    pub fn is_live(&self) -> bool {
        self.status == "running" || self.status == "awaiting-approval"
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueuedApproval {
    #[serde(rename = "runId")]
    pub run_id: String,
    #[serde(rename = "threadId", default)]
    pub thread_id: String,
    #[serde(rename = "callId")]
    pub call_id: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

impl QueuedApproval {
    /// One suspended run at a time in the transcript. Older unanswered
    /// host-shell runs stay on the server; they are not stacked on this turn.
    pub fn latest_for_thread<'a>(queue: &'a [Self], thread_id: &str) -> Option<&'a Self> {
        queue.iter().rev().find(|item| item.thread_id == thread_id)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DaemonList {
    #[serde(default)]
    machines: Vec<DaemonMachine>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonMachine {
    #[serde(rename = "machineId")]
    pub machine_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub connected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalExecMode {
    Ask,
    Always,
    Never,
}

impl LocalExecMode {
    pub fn from_stored(mode: &str) -> Self {
        match mode {
            "ask" => Self::Ask,
            "bypass" => Self::Always,
            _ => Self::Never,
        }
    }

    pub fn as_stored(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Always => "bypass",
            Self::Never => "never",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask every time",
            Self::Always => "Always allow",
            Self::Never => "Never allow",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CoworkerComputer {
    #[serde(rename = "agentId", default)]
    pub agent_id: String,
    #[serde(default)]
    pub state: String,
    #[serde(rename = "vncUrl", default)]
    pub vnc_url: Option<String>,
    /// The scope's live box — after an update or a heal it differs from the id on the
    /// coworker's own row, and it is the one a person is looking at.
    #[serde(rename = "boxId", default)]
    pub box_id: Option<String>,
    /// What the box runs against what a new one would get; `None` when the provider cannot say.
    #[serde(default)]
    pub image: Option<ImageStatus>,
    /// An update in flight, or the failure the last one ended in.
    #[serde(default)]
    pub update: Option<UpdateStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ImageStatus {
    #[serde(default)]
    pub running: String,
    #[serde(default)]
    pub latest: String,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpdateStatus {
    /// `pulling` | `transferring` | `starting` | `failed`.
    pub phase: String,
    #[serde(rename = "startedAtMs", default)]
    pub started_at_ms: i64,
    #[serde(default)]
    pub error: Option<String>,
}

impl UpdateStatus {
    pub fn in_flight(&self) -> bool {
        self.phase != "failed"
    }

    /// The banner's second line for this phase.
    pub fn detail(&self) -> String {
        match self.phase.as_str() {
            "pulling" => "Fetching the newest image".to_string(),
            "transferring" => "Transferring your data".to_string(),
            "starting" => "Starting the new computer".to_string(),
            "failed" => self
                .error
                .clone()
                .unwrap_or_else(|| "The update failed".to_string()),
            other => other.to_string(),
        }
    }
}

impl CoworkerComputer {
    pub fn vnc_url(&self) -> Option<&str> {
        self.vnc_url.as_deref().filter(|url| !url.is_empty())
    }

    pub fn updating(&self) -> bool {
        self.update.as_ref().is_some_and(UpdateStatus::in_flight)
    }

    pub fn image_stale(&self) -> bool {
        self.image.as_ref().is_some_and(|image| image.stale)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedComputer {
    pub machine_id: String,
    pub label: String,
    pub mode: LocalExecMode,
    pub this_machine: bool,
    pub online: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DaemonEnrol {
    #[serde(rename = "machineId")]
    pub machine_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LocalExecPolicyView {
    #[serde(default)]
    mode: String,
}

/// The screen every recipe is taught on and played back on, in CSS pixels.
pub const RECIPE_SCREEN: (u32, u32) = (1280, 800);
/// The most a recipe upload may weigh. The server refuses more, so say so before sending.
pub const RECIPE_UPLOAD_LIMIT: usize = 5 * 1024 * 1024;
/// The least time between two `move` events that both go up: the mouse reports far more often
/// than a step needs, and a long tape must fit the upload limit.
pub const RECIPE_MOVE_INTERVAL_MS: i64 = 40;

/// The tape with its `move` events thinned to one per [`RECIPE_MOVE_INTERVAL_MS`]; every other
/// event stays, in order. The first move after a gap always goes.
pub fn thin_tape(events: &[Value]) -> Vec<Value> {
    let mut last_move_at: Option<i64> = None;
    events
        .iter()
        .filter(|event| {
            if event.get("kind").and_then(Value::as_str) != Some("move") {
                return true;
            }
            let at = event.get("at").and_then(Value::as_i64).unwrap_or(0);
            if last_move_at.is_some_and(|last| at - last < RECIPE_MOVE_INTERVAL_MS) {
                return false;
            }
            last_move_at = Some(at);
            true
        })
        .cloned()
        .collect()
}

/// A field the server may send as `null`: read it as the type's default.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Debug, Clone, Deserialize)]
struct RecipeList {
    #[serde(default)]
    recipes: Vec<RecipeSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeScreen {
    pub width: u32,
    pub height: u32,
}

impl Default for RecipeScreen {
    fn default() -> Self {
        Self {
            width: RECIPE_SCREEN.0,
            height: RECIPE_SCREEN.1,
        }
    }
}

/// How the person stands to a recipe: theirs, shared with them and taken, shared and not yet
/// answered, or nothing in particular (an org listing they may look at).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeRelation {
    Mine,
    Shared,
    Invited,
    #[default]
    #[serde(other)]
    None,
}

/// Which of the two things a row on the listing is.
///
/// The server keeps both on one table: a workflow is a recipe version of kind `workflow`, so
/// ownership, versions, sharing, grants and run history are one set of rules rather than two
/// that drift. The listing therefore carries this word on every row — `GET /recipes` brings
/// back both and `?kind=` only drops rows it has already built — which is why the app fetches
/// once and reads the word instead of asking twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeKind {
    /// A decision tree that drives recipes, walked by the server, which asks Jev at each branch.
    Workflow,
    /// A taped sequence, replayed exactly by the box alone.
    ///
    /// Last because the catch-all has to be, and a word this client has no name for is read as
    /// one — the way an unknown parameter kind is read as text: a third kind arriving one day
    /// must not fail the listing and take every recipe down with it.
    #[default]
    #[serde(other)]
    Recipe,
}

impl RecipeKind {
    /// What this kind is called where a person reads it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Recipe => "Recipe",
            Self::Workflow => "Workflow",
        }
    }

    /// The same word mid-sentence, for a line like "Drop the workflow".
    pub fn word(self) -> &'static str {
        match self {
            Self::Recipe => "recipe",
            Self::Workflow => "workflow",
        }
    }

    /// The icon a row of this kind carries: a tape for a recipe, a fork in the road for a tree.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Recipe => "icons/record.svg",
            Self::Workflow => "icons/branch.svg",
        }
    }
}

/// Where a share stands with the person it went to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeShareState {
    Accepted,
    Declined,
    Pending,
    #[serde(other)]
    Unknown,
}

/// One row of the Recipes list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeSummary {
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub owner_id: String,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub description: String,
    #[serde(default)]
    pub screen: RecipeScreen,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub deleted_at_ms: Option<i64>,
    #[serde(default)]
    pub latest_version: u32,
    #[serde(default)]
    pub relation: RecipeRelation,
    #[serde(default)]
    pub share_state: Option<RecipeShareState>,
    /// A taped sequence or a decision tree. A server that has never heard of workflows leaves
    /// the word off the row, or sends it empty; either way every row it sends is a recipe, which
    /// is what the default says.
    #[serde(default, deserialize_with = "null_as_default")]
    pub kind: RecipeKind,
    /// What the runnable version needs told before it runs. It rides on the summary so the
    /// composer knows what a recipe wants the moment it is picked, with no second fetch: a
    /// list that arrives one keystroke after the person needs it is a list they type past.
    #[serde(default)]
    pub parameters: Vec<RecipeParameter>,
}

impl RecipeSummary {
    pub fn is_mine(&self) -> bool {
        self.relation == RecipeRelation::Mine
    }

    /// A decision tree rather than a tape. What it changes is what the row is called and what
    /// can be done with it, never how its parameters are read: a workflow declares them exactly
    /// as a recipe does, on the same field of the same row.
    pub fn is_workflow(&self) -> bool {
        self.kind == RecipeKind::Workflow
    }

    /// A share the person has not answered: Accept or Decline comes before anything else.
    pub fn is_pending_invite(&self) -> bool {
        self.relation == RecipeRelation::Invited
            && matches!(self.share_state, Some(RecipeShareState::Pending) | None)
    }
}

/// What a parameter takes. The server names only these three, and a fourth this client has no
/// word for is read as text: an unknown kind should leave a field a person can still type into
/// rather than fail the whole listing and take every other recipe down with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecipeParameterKind {
    Number,
    Boolean,
    #[default]
    #[serde(other)]
    Text,
}

impl RecipeParameterKind {
    /// What this kind is called where a person reads it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Boolean => "yes or no",
            Self::Text => "text",
        }
    }
}

/// One thing a recipe needs told before it runs, as its runnable version declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeParameter {
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub kind: RecipeParameterKind,
    /// What it stands at when nobody says otherwise. A number or a boolean default arrives as
    /// itself in JSON and is kept as the words a person would type for it, because that is what
    /// it is shown and compared as; [`RecipeParameter::encode`] turns it back on the way out.
    #[serde(default, deserialize_with = "scalar_text")]
    pub default: Option<String>,
    /// The only values this parameter takes, when the declaration narrows it.
    #[serde(default, deserialize_with = "scalar_text_list")]
    pub values: Option<Vec<String>>,
}

impl RecipeParameter {
    /// The values this parameter is narrowed to, if it is narrowed at all.
    pub fn allowed(&self) -> Option<&[String]> {
        self.values.as_deref().filter(|values| !values.is_empty())
    }

    /// The declared value this text names, in the declaration's own spelling. Case is not what
    /// a person is choosing between, so "Mundo" takes the declared "mundo" rather than being
    /// refused for a difference they cannot see the point of.
    pub fn declared(&self, text: &str) -> Option<&str> {
        self.allowed()?
            .iter()
            .find(|value| value.eq_ignore_ascii_case(text))
            .map(String::as_str)
    }

    /// Why this text is not a value this parameter takes, in words for the person, or `None`
    /// when it is one. The text is what they typed, trimmed and not empty — an empty field is
    /// no value at all rather than a bad one.
    ///
    /// The server checks again and is the authority; this only catches it while the composer is
    /// still open and the answer can be fixed without losing the turn.
    pub fn reject(&self, text: &str) -> Option<String> {
        if let Some(allowed) = self.allowed() {
            if self.declared(text).is_none() {
                return Some(format!(
                    "{} takes one of: {}.",
                    self.name,
                    allowed.join(", ")
                ));
            }
            return None;
        }
        match self.kind {
            RecipeParameterKind::Number => text
                .parse::<f64>()
                .is_err()
                .then(|| format!("{} takes a number, and \"{text}\" is not one.", self.name)),
            RecipeParameterKind::Boolean => boolean_text(text)
                .is_none()
                .then(|| format!("{} is a yes or a no, so pick one.", self.name)),
            RecipeParameterKind::Text => None,
        }
    }

    /// The value as it travels to the server: a declared `number` as a number and a `boolean`
    /// as a boolean, so `recipeValues` carries what the declaration asked for rather than the
    /// words that stood for it in the composer.
    pub fn encode(&self, text: &str) -> Value {
        let text = self.declared(text).unwrap_or(text);
        match self.kind {
            RecipeParameterKind::Number => text
                .parse::<i64>()
                .map(Value::from)
                .or_else(|_| text.parse::<f64>().map(Value::from))
                .unwrap_or_else(|_| Value::String(text.to_string())),
            RecipeParameterKind::Boolean => match boolean_text(text) {
                Some(flag) => Value::Bool(flag),
                None => Value::String(text.to_string()),
            },
            RecipeParameterKind::Text => Value::String(text.to_string()),
        }
    }
}

/// The yes or the no a person may have written, whichever way they wrote it.
fn boolean_text(text: &str) -> Option<bool> {
    match text.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" | "1" => Some(true),
        "false" | "no" | "n" | "0" => Some(false),
        _ => None,
    }
}

/// A scalar the server may send as a string, a number, a boolean or `null`, read as the words a
/// person would type for it.
fn scalar_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Value>::deserialize(deserializer)?.and_then(|value| value_text(&value)))
}

/// The same, for the list of values a parameter may be narrowed to.
fn scalar_text_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(values) = Option::<Vec<Value>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    Ok(Some(values.iter().filter_map(value_text).collect()))
}

/// One JSON scalar as text. An object or an array is not something a person types into a field,
/// so it stands for no value at all.
fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// The recipe a turn runs, and what its parameters were filled in with.
///
/// It travels in `forwardedProps` beside the coworker and never in the message: a parameter
/// value is not prose, and pasting it into the sentence would change what the person said.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnRecipe {
    pub id: String,
    pub values: serde_json::Map<String, Value>,
}

/// One version of a recipe: the tape (v1), the steps the server filtered from it (v2), or
/// steps a person edited.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeVersion {
    pub version: u32,
    /// `raw`, `filtered` or `edited`.
    #[serde(default, deserialize_with = "null_as_default")]
    pub kind: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at_ms: i64,
    /// What this version needs told before it runs. See [`RecipeVersion::declared`] for why it
    /// is read from here and from the body both.
    #[serde(default)]
    pub parameters: Vec<RecipeParameter>,
    #[serde(default)]
    pub body: RecipeVersionBody,
}

impl RecipeVersion {
    /// The tape is kept, not run; every other kind carries steps.
    pub fn is_runnable(&self) -> bool {
        self.kind != "raw"
    }

    /// What this version declares it needs told, which is the declaration a run of it binds
    /// values to. It arrives beside the version's own fields or inside its body, and either
    /// shape reads the same here, the way a tape does: the declaration belongs to the version
    /// whichever half of the row the server writes it on.
    pub fn declared(&self) -> &[RecipeParameter] {
        if !self.parameters.is_empty() {
            return &self.parameters;
        }
        &self.body.parameters
    }

    /// A version someone wrote by editing, which is the only kind the server will delete on
    /// its own: the tape is v1 and the steps filtered from it are v2, and those two are what
    /// the recipe is. A server that names no kind still numbers them, so the number stands in.
    pub fn is_edited(&self) -> bool {
        self.kind == "edited" || (self.version > 2 && self.is_runnable())
    }

    /// How many tape events a raw version holds: the server sends the count, or the events.
    pub fn event_count(&self) -> u64 {
        self.body.events.as_ref().map_or(0, RecipeTape::count)
    }

    /// The tape itself, when the server sent it rather than only its size. It arrives under
    /// `tape` beside the count; a body that puts the events under `events` reads the same.
    pub fn tape_events(&self) -> Option<&[RecipeTapeEvent]> {
        if !self.body.tape.is_empty() {
            return Some(&self.body.tape);
        }
        self.body.events.as_ref().and_then(RecipeTape::events)
    }

    /// Whether the tape that arrived is only the front of a longer one.
    pub fn tape_truncated(&self) -> bool {
        self.body.truncated
    }
}

/// A version's body: the tape for v1, the steps for every version after it. The keys are the
/// server's snake_case.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RecipeVersionBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<RecipeTape>,
    /// The tape itself. The server sends the count under `events` and the events under `tape`,
    /// cut at a cap with `truncated` saying so; a body that carries the events under `events`
    /// instead is read the same way, so either shape shows a tape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tape: Vec<RecipeTapeEvent>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub steps: Vec<RecipeStep>,
    /// The version's declaration, when the server writes it inside the body rather than beside
    /// it. Read through [`RecipeVersion::declared`], which takes either.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<RecipeParameter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_on_error: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<Value>,
}

/// What a raw version carries under `events`: how many events were taped, or the tape itself.
/// Both shapes parse, and an unexpected third one must not fail the whole detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RecipeTape {
    Count(u64),
    Events(Vec<RecipeTapeEvent>),
    Other(Value),
}

impl RecipeTape {
    /// How many events were taped, whether the tape came with them or only with its size.
    pub fn count(&self) -> u64 {
        match self {
            Self::Count(count) => *count,
            Self::Events(events) => events.len() as u64,
            Self::Other(_) => 0,
        }
    }

    pub fn events(&self) -> Option<&[RecipeTapeEvent]> {
        match self {
            Self::Events(events) => Some(events),
            _ => None,
        }
    }
}

/// One event off a taught tape, mirroring the server's `TapeEvent`. `kind` decides which of the
/// rest matter: `down` and `up` carry a place and a button, `move` a place, `wheel` a place and
/// an amount, `keydown` and `keyup` a key. `at` is the wall clock in milliseconds, so a row's
/// offset is its own `at` less the tape's first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RecipeTapeEvent {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub button: i32,
    #[serde(default)]
    pub dx: i32,
    #[serde(default)]
    pub dy: i32,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub at: i64,
}

/// One thing a bot does when a recipe runs. Coordinates are on the recipe's 1280×800 screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RecipeStep {
    Click {
        x: i64,
        y: i64,
        /// The server's word or number for the button, kept as sent so an edit sends it back.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        button: Option<Value>,
    },
    DoubleClick {
        x: i64,
        y: i64,
    },
    Drag {
        x1: i64,
        y1: i64,
        x2: i64,
        y2: i64,
    },
    Type {
        text: String,
    },
    Key {
        key: String,
    },
    Scroll {
        x: i64,
        y: i64,
        dx: i64,
        dy: i64,
    },
    Wait {
        ms: u64,
    },
}

impl RecipeStep {
    /// The step as one readable line: `click (412, 88)`, `type "example.com"`, `wait 500 ms`.
    pub fn describe(&self) -> String {
        match self {
            Self::Click { x, y, button } => {
                // The left button is the one a click means; only another is worth a word.
                let other = button.as_ref().and_then(|button| match button {
                    Value::String(name) if name != "left" => Some(name.clone()),
                    Value::Number(number) if number.as_i64() != Some(0) => Some(number.to_string()),
                    _ => None,
                });
                match other {
                    Some(button) => format!("click ({x}, {y}) button {button}"),
                    None => format!("click ({x}, {y})"),
                }
            }
            Self::DoubleClick { x, y } => format!("double-click ({x}, {y})"),
            Self::Drag { x1, y1, x2, y2 } => format!("drag ({x1}, {y1}) to ({x2}, {y2})"),
            Self::Type { text } => format!("type {text:?}"),
            Self::Key { key } => format!("key {key}"),
            Self::Scroll { x, y, dx, dy } => format!("scroll ({x}, {y}) by ({dx}, {dy})"),
            Self::Wait { ms } => format!("wait {ms} ms"),
        }
    }
}

/// Who a recipe has been shared with (only the owner sees these).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeShare {
    #[serde(default, deserialize_with = "null_as_default")]
    pub recipe_id: String,
    /// `org` or `account`.
    #[serde(default, deserialize_with = "null_as_default")]
    pub scope: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub scope_id: String,
    #[serde(default)]
    pub granted_by: Option<String>,
    #[serde(default)]
    pub granted_at_ms: i64,
    #[serde(default)]
    pub accepted_at_ms: Option<i64>,
    #[serde(default)]
    pub declined_at_ms: Option<i64>,
}

impl RecipeShare {
    /// What the person it went to has done with it.
    pub fn state_label(&self) -> &'static str {
        if self.declined_at_ms.is_some() {
            "declined"
        } else if self.accepted_at_ms.is_some() {
            "accepted"
        } else {
            "pending"
        }
    }
}

/// A bot that may run the recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeGrant {
    #[serde(default, deserialize_with = "null_as_default")]
    pub recipe_id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub coworker_id: String,
    #[serde(default)]
    pub granted_by: Option<String>,
    #[serde(default)]
    pub granted_at_ms: i64,
}

/// One past run of the recipe (newest first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeRun {
    /// The run's own id, `rrun_…`, which is what the history names its rows by.
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub recipe_id: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default, deserialize_with = "null_as_default")]
    pub coworker_id: String,
    #[serde(default)]
    pub ok: bool,
    /// The step the run stopped at, when it did not finish.
    #[serde(default)]
    pub stopped_at: Option<u64>,
    /// What the box said the run came to, step by step: `{ok, ran, stopped_at, steps}`. The
    /// server keeps it without its picture, so a stored run is what happened, not a gallery.
    #[serde(default)]
    pub receipt: Value,
    #[serde(default)]
    pub at_ms: i64,
}

impl RecipeRun {
    /// Every step of the run as the box reported it: whether it did what it was asked, and
    /// what went wrong where it did not.
    pub fn receipt_steps(&self) -> Vec<(bool, Option<String>)> {
        self.receipt
            .get("steps")
            .and_then(Value::as_array)
            .map(|steps| {
                steps
                    .iter()
                    .map(|step| {
                        let ok = step.get("ok").and_then(Value::as_bool).unwrap_or(false);
                        let error = step
                            .get("error")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .filter(|error| !error.trim().is_empty());
                        (ok, error)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Why the run stopped, in the box's own words.
    pub fn error(&self) -> Option<String> {
        self.receipt_steps()
            .into_iter()
            .find_map(|(_, error)| error)
            .or_else(|| {
                self.receipt
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .filter(|error| !error.trim().is_empty())
    }
}

/// One of the person's own bots, as the detail lists them for grants and runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeBot {
    pub id: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub name: String,
}

/// Everything the detail view shows about one recipe.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeDetail {
    pub recipe: RecipeSummary,
    #[serde(default)]
    pub versions: Vec<RecipeVersion>,
    #[serde(default)]
    pub shares: Vec<RecipeShare>,
    #[serde(default)]
    pub grants: Vec<RecipeGrant>,
    #[serde(default)]
    pub runs: Vec<RecipeRun>,
    #[serde(default)]
    pub my_bots: Vec<RecipeBot>,
}

impl RecipeDetail {
    /// The version a run plays: the newest one with steps.
    pub fn runnable_version(&self) -> Option<&RecipeVersion> {
        self.versions
            .iter()
            .filter(|version| version.is_runnable())
            .max_by_key(|version| version.version)
    }

    pub fn is_granted(&self, coworker_id: &str) -> bool {
        self.grants
            .iter()
            .any(|grant| grant.coworker_id == coworker_id)
    }

    /// The bot's name, or its id when it is not one of the person's.
    pub fn bot_name(&self, coworker_id: &str) -> String {
        self.my_bots
            .iter()
            .find(|bot| bot.id == coworker_id)
            .map(|bot| bot.name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| coworker_id.to_string())
    }
}

/// Who a recipe goes to: the whole org, or one person by email (same org only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "scope", rename_all = "lowercase")]
pub enum RecipeShareTarget {
    Org,
    Account { email: String },
}

/// What `POST /recipes/{id}/run` came back with.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeRunResult {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub ok: bool,
    /// How many steps ran: a count, or the steps themselves.
    #[serde(default)]
    pub ran: Option<Value>,
    #[serde(default)]
    pub stopped_at: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    /// The screen after the run, `{mime, base64, width, height}`, when the box has one.
    #[serde(default)]
    pub image: Option<Value>,
}

impl RecipeRunResult {
    pub fn ran_count(&self) -> Option<u64> {
        match &self.ran {
            Some(Value::Number(count)) => count.as_u64(),
            Some(Value::Array(steps)) => Some(steps.len() as u64),
            _ => None,
        }
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::assistant_text_from_sse;
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A URL nothing is listening on. These tests only ever read the jar, and a base that
    /// cannot be connected to is the guarantee that they never do anything else.
    const NOWHERE: &str = "http://127.0.0.1:1/";

    /// The one word that tells the two apart on a listing that carries both. A kind this client
    /// has never heard of has to land somewhere readable rather than fail the whole listing and
    /// take every recipe on it down with a row nobody asked about.
    #[test]
    fn a_row_says_which_of_the_two_it_is_and_an_unknown_word_is_still_a_row() {
        let listing: Vec<RecipeSummary> = serde_json::from_value(json!([
            { "id": "rcp_1", "name": "tape", "kind": "recipe" },
            { "id": "rcp_2", "name": "tree", "kind": "workflow" },
            { "id": "rcp_3", "name": "older server", "kind": null },
            { "id": "rcp_4", "name": "something new" },
            { "id": "rcp_5", "name": "a word from the future", "kind": "lesson" }
        ]))
        .expect("an unknown kind must not fail the listing");
        assert_eq!(
            listing
                .iter()
                .map(|row| row.kind.label())
                .collect::<Vec<_>>(),
            vec!["Recipe", "Workflow", "Recipe", "Recipe", "Recipe"]
        );
        assert!(listing[1].is_workflow());
        assert!(!listing[4].is_workflow());
    }

    fn put_cookie(client: &OpenGrokClient, set_cookie: &str) {
        let header = HeaderValue::from_str(set_cookie).unwrap();
        CookieStore::set_cookies(client.jar.as_ref(), &mut [header].iter(), &client.base);
    }

    /// A `Set-Cookie` for an access token with an `exp` far enough out that nothing refreshes
    /// before using it.
    ///
    /// Tests that watch what goes over the wire need the client to send exactly one request, and
    /// a token whose expiry cannot be read is a token the client replaces first — rightly, since
    /// that is the whole fix. Assembled rather than written down as a literal, so the file
    /// carries no string shaped like a credential.
    fn live_session() -> String {
        use base64::Engine as _;
        let part = |json: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
        // 2100-01-01, which outlives any test run and any machine running one.
        let token = format!(
            "{}.{}.not-a-signature",
            part(r#"{"alg":"HS256","typ":"JWT"}"#),
            part(r#"{"exp":4102444800}"#)
        );
        format!("og_access={token}; Path=/")
    }

    /// How the token went missing, pinned.
    ///
    /// `Jar::cookies` goes through `cookie_store`'s `matches`, which filters on `is_expired`, so
    /// the moment an access cookie's `Max-Age` passes it stops existing as far as the app is
    /// concerned — indistinguishable from one that was never set. An app left idle past the
    /// token's lifetime therefore had nothing to put in the header, and nothing in the old code
    /// noticed: the freshness check only fired on a token that was still *there*.
    #[test]
    fn an_expired_access_cookie_is_gone_rather_than_old() {
        let client = OpenGrokClient::new(NOWHERE).unwrap();
        put_cookie(&client, "og_access=tok-a; Path=/; Max-Age=300");
        assert_eq!(client.access_token().as_deref(), Some("tok-a"));
        assert!(client.has_session());

        // The server's own expiry arriving, or simply time passing: same thing to the jar.
        put_cookie(&client, "og_access=tok-a; Path=/; Max-Age=0");
        assert_eq!(
            client.access_token(),
            None,
            "the jar hands back nothing, not something stale"
        );
        assert_eq!(
            client.token_seconds_left(),
            None,
            "and there is no `exp` to read, because there is no token to read it from"
        );
        assert!(
            !client.has_session(),
            "nothing left to send and nothing left to trade: this is the signed-out case"
        );
    }

    /// The refresh cookie outliving the access token is a session the app can still rescue.
    #[test]
    fn a_refresh_cookie_alone_is_still_a_session() {
        let client = OpenGrokClient::new(NOWHERE).unwrap();
        put_cookie(&client, "og_refresh=tok-r; Path=/; Max-Age=3600");
        assert_eq!(client.access_token(), None);
        assert!(
            client.has_session(),
            "there is something to trade for a token, so the turn is worth attempting"
        );
    }

    /// The one line of the bug, as a truth table.
    #[test]
    fn a_token_that_cannot_be_read_is_a_token_that_needs_replacing() {
        assert!(
            needs_refresh(None),
            "no readable token is not the same as plenty of time"
        );
        assert!(needs_refresh(Some(0)), "expired to the second");
        assert!(needs_refresh(Some(-600)), "expired ten minutes ago");
        assert!(needs_refresh(Some(29)), "inside the slack");
        assert!(!needs_refresh(Some(31)));
        assert!(!needs_refresh(Some(900)));
    }

    /// The turn does not leave, and the app does not learn that from the server.
    ///
    /// Nothing goes out at all here: there is neither a token to send nor a refresh cookie to
    /// trade for one, so both the turn and the rescue attempt are round trips whose answer the
    /// jar already holds. What used to happen was the `/ag-ui` request going out bare — a round
    /// trip, and a red line in somebody's transcript, for a question already answered.
    #[tokio::test]
    async fn a_turn_is_not_sent_when_there_is_nothing_to_sign_it_with() {
        let server = MockServer::start().await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let error = client
            .run_turn("cw", "t", "run_1", &[], None, |_| {})
            .await
            .unwrap_err();
        assert!(error.is_signed_out());

        let paths: Vec<String> = server
            .received_requests()
            .await
            .expect("the recorder is on")
            .iter()
            .map(|request| request.url.path().to_string())
            .collect();
        assert!(paths.is_empty(), "nothing went out at all: {paths:?}");
    }

    /// An access token that has expired out of the jar, with the refresh cookie still there.
    ///
    /// This is today's bug end to end. The app looks signed in, has nothing to put in the
    /// header, and used to send the turn regardless. It now trades the refresh cookie for a
    /// token first and the turn goes out carrying it — no banner, no red line, nobody told.
    #[tokio::test]
    async fn an_expired_token_is_refreshed_before_the_turn_rather_than_sent_without_one() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/refresh"))
            .respond_with(
                ResponseTemplate::new(200).append_header("set-cookie", live_session().as_str()),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/ag-ui"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string("data: {\"type\":\"RUN_FINISHED\",\"runId\":\"r\"}\n\n"),
            )
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        // What the jar looks like after an access cookie's `Max-Age` passes: the refresh cookie
        // is all that is left, and the app is still showing a roster.
        put_cookie(&client, "og_refresh=tok-r; Path=/; Max-Age=3600");
        client
            .run_turn("cw", "t", "run_1", &[], None, |_| {})
            .await
            .expect("the turn goes out, on a token the app fetched for itself");

        let turn = server
            .received_requests()
            .await
            .expect("the recorder is on")
            .into_iter()
            .find(|request| request.url.path() == "/ag-ui")
            .expect("the turn was sent");
        assert!(
            turn.headers.contains_key("authorization"),
            "and it carried a bearer, which is the whole of the bug: auth_len was 0"
        );
    }

    /// A `401` on a turn is the session, not a verdict about the turn.
    ///
    /// The server's sentence is kept and is not what decides this: the status and the route are.
    /// Matching on the words would tie the app to copy the server is free to rewrite, and this
    /// one has already been rewritten once.
    #[tokio::test]
    async fn a_401_on_a_turn_reads_as_the_session_being_gone() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "error": "this turn names a coworker but says whose it is nowhere — sign in again"
            })))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        put_cookie(&client, &live_session());
        let error = client
            .run_turn("cw", "t", "run_1", &[], None, |_| {})
            .await
            .unwrap_err();
        assert!(error.is_signed_out());
        assert_eq!(
            error.unreachable(),
            None,
            "the server answered, so nothing is out of reach and nothing is reconnecting"
        );
        assert!(
            error.message.contains("sign in again"),
            "the server's own words survive: {}",
            error.message
        );
    }

    /// A refusal on a turn with a reason stays a verdict, and still reaches the transcript.
    #[tokio::test]
    async fn a_refusal_on_a_turn_is_still_a_verdict() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui"))
            .respond_with(ResponseTemplate::new(402).set_body_json(json!({
                "error": "spend cap reached for this org"
            })))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        put_cookie(&client, &live_session());
        let error = client
            .run_turn("cw", "t", "run_1", &[], None, |_| {})
            .await
            .unwrap_err();
        assert!(!error.is_signed_out(), "nobody should be asked to sign in");
        assert_eq!(error.unreachable(), None);
        assert_eq!(error.message, "spend cap reached for this org");
    }

    #[tokio::test]
    async fn login_sets_cookies_and_me_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/login"))
            .and(body_json(json!({"email":"a@b.c","password":"secret"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header(
                        "set-cookie",
                        "og_access=tok-a; HttpOnly; Path=/; SameSite=Lax",
                    )
                    .append_header(
                        "set-cookie",
                        "og_refresh=tok-r; HttpOnly; Path=/; SameSite=Lax",
                    )
                    .set_body_json(json!({"email":"a@b.c"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "acc_1",
                "email": "a@b.c",
                "firstName": "Ada",
                "lastName": "Lovelace",
                "avatarUrl": null,
                "orgId": "org_1",
                "verified": true,
                "enabled": true,
                "isAdmin": false
            })))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        client.login("a@b.c", "secret").await.unwrap();
        let me = client.me().await.unwrap();
        assert_eq!(me.email, "a@b.c");
        assert_eq!(me.display_name(), "Ada Lovelace");
    }

    #[tokio::test]
    async fn session_file_survives_a_new_client() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .append_header(
                        "set-cookie",
                        "og_access=tok-a; HttpOnly; Path=/; SameSite=Lax",
                    )
                    .append_header(
                        "set-cookie",
                        "og_refresh=tok-r; HttpOnly; Path=/; SameSite=Lax",
                    ),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "acc_1",
                "email": "a@b.c",
                "firstName": "Ada",
                "lastName": "Lovelace",
                "avatarUrl": null,
                "orgId": "org_1",
                "verified": true,
                "enabled": true,
                "isAdmin": false
            })))
            .mount(&server)
            .await;

        let dir = std::env::temp_dir().join(format!("nativechat-session-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("opengrok-session.json");
        let client = OpenGrokClient::new(&server.uri())
            .unwrap()
            .with_session_file(path.clone());
        client.login("a@b.c", "secret").await.unwrap();
        assert!(path.exists());

        let restored = OpenGrokClient::new(&server.uri())
            .unwrap()
            .with_session_file(path.clone());
        assert!(restored.load_session());
        let me = restored.me().await.unwrap();
        assert_eq!(me.email, "a@b.c");
        restored.clear_session();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn login_401_is_distinguishable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/login"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error":"bad password"})))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let err = client.login("a@b.c", "nope").await.unwrap_err();
        assert!(err.is_unauthorized());
        assert_eq!(err.message, "bad password");
    }

    #[tokio::test]
    async fn refresh_without_cookie_is_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/refresh"))
            .respond_with(ResponseTemplate::new(401).set_body_string("no session"))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let err = client.refresh().await.unwrap_err();
        assert!(err.is_unauthorized());
    }

    #[tokio::test]
    async fn empty_roster_is_empty_vec() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/coworkers"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let list = client.list_coworkers().await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn hire_posts_name() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/coworkers"))
            .and(body_json(json!({"name":"NativeChat"})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "id": "cw_1",
                "name": "NativeChat",
                "model": "xai/grok-4.6"
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let hired = client.hire("NativeChat", None).await.unwrap();
        assert_eq!(hired.id, "cw_1");
        assert_eq!(hired.model, "xai/grok-4.6");
    }

    #[test]
    fn sse_collects_text_deltas() {
        let body = concat!(
            "data: {\"type\":\"RUN_STARTED\",\"threadId\":\"t\",\"runId\":\"r\"}\n\n",
            "data: {\"type\":\"TEXT_MESSAGE_START\",\"messageId\":\"m1\",\"role\":\"assistant\"}\n\n",
            "data: {\"type\":\"TEXT_MESSAGE_CONTENT\",\"messageId\":\"m1\",\"delta\":\"Hello\"}\n\n",
            "data: {\"type\":\"TEXT_MESSAGE_CONTENT\",\"messageId\":\"m1\",\"delta\":\" world\"}\n\n",
            "data: {\"type\":\"TEXT_MESSAGE_END\",\"messageId\":\"m1\"}\n\n",
            "data: {\"type\":\"RUN_FINISHED\",\"runId\":\"r\"}\n\n",
        );
        assert_eq!(assistant_text_from_sse(body).unwrap(), "Hello world");
    }

    #[tokio::test]
    async fn run_turn_forwards_sse_frames_as_they_arrive() {
        use std::time::{Duration, Instant};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            socket.set_nodelay(true).unwrap();
            let mut socket = socket;
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await;
            let frames = [
                "data: {\"type\":\"RUN_STARTED\",\"threadId\":\"t\",\"runId\":\"r\"}\n\n",
                "data: {\"type\":\"TEXT_MESSAGE_START\",\"messageId\":\"m1\",\"role\":\"assistant\"}\n\n",
                "data: {\"type\":\"TEXT_MESSAGE_CONTENT\",\"messageId\":\"m1\",\"delta\":\"Hello\"}\n\n",
                "data: {\"type\":\"TEXT_MESSAGE_CONTENT\",\"messageId\":\"m1\",\"delta\":\" world\"}\n\n",
                "data: {\"type\":\"TEXT_MESSAGE_END\",\"messageId\":\"m1\"}\n\n",
                "data: {\"type\":\"RUN_FINISHED\",\"runId\":\"r\"}\n\n",
            ];
            let body: String = frames.concat();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.flush().await.unwrap();
            for frame in frames {
                socket.write_all(frame.as_bytes()).await.unwrap();
                socket.flush().await.unwrap();
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });

        let client = OpenGrokClient::new(&format!("http://{addr}")).unwrap();
        put_cookie(&client, &live_session());
        let first_at = std::sync::Arc::new(std::sync::Mutex::new(None::<Instant>));
        let start = Instant::now();
        let text = client
            .run_turn(
                "cw",
                "t",
                "run_1",
                &[AguiMessage {
                    id: "u1".into(),
                    role: "user".into(),
                    content: "hi".into(),
                    tool_call_id: None,
                    reply_to: None,
                }],
                None,
                {
                    let first_at = first_at.clone();
                    move |event| {
                        let kind = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        if kind == "TEXT_MESSAGE_CONTENT" {
                            let mut slot = first_at.lock().unwrap();
                            if slot.is_none() {
                                *slot = Some(Instant::now());
                            }
                        }
                    }
                },
            )
            .await
            .unwrap();
        assert_eq!(text, "Hello world");
        let first = first_at.lock().unwrap().expect("saw text");
        let until_first = first.duration_since(start);
        let total = start.elapsed();
        assert!(
            until_first + Duration::from_millis(50) < total,
            "first text at {until_first:?}, stream ended at {total:?} — frames were buffered"
        );
    }

    /// The turn's body is a contract with the server, which reads `forwardedProps` for the
    /// recipe and validates the values against the same declaration the composer read. Both
    /// sides are written apart, so the shape is asserted here rather than agreed in passing.
    #[tokio::test]
    async fn a_turn_carries_the_active_recipe_and_its_values_in_forwarded_props() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "data: {\"type\":\"RUN_FINISHED\",\"runId\":\"r\"}\n\n".to_string(),
                    ),
            )
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        put_cookie(&client, &live_session());
        let message = AguiMessage {
            id: "u1".into(),
            role: "user".into(),
            content: "find me something".into(),
            tool_call_id: None,
            reply_to: None,
        };
        let recipe = TurnRecipe {
            id: "rcp_1".to_string(),
            values: serde_json::Map::from_iter([
                ("search_term".to_string(), json!("mundo")),
                ("count".to_string(), json!(5)),
                ("shorts".to_string(), json!(true)),
            ]),
        };
        client
            .run_turn(
                "cw_1",
                "thread_1",
                "run_1",
                &[message],
                Some(&recipe),
                |_| {},
            )
            .await
            .unwrap();

        let requests = server.received_requests().await.expect("the turn was sent");
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body["forwardedProps"],
            json!({
                "coworkerId": "cw_1",
                "recipe": "rcp_1",
                "recipeValues": { "search_term": "mundo", "count": 5, "shorts": true }
            })
        );
        assert_eq!(
            body["messages"][0]["content"], "find me something",
            "a parameter value is not prose: the message stays exactly what was written"
        );
        assert_eq!(
            body["runId"], "run_1",
            "the run is filed under the id the caller minted, which is the only id it still has \
             to ask `GET /ag-ui/runs/{{id}}` with once the stream is gone"
        );

        // No recipe on the turn leaves the props as they were, so an ordinary chat is unchanged.
        client
            .run_turn(
                "cw_1",
                "thread_1",
                "run_2",
                &[AguiMessage {
                    id: "u2".into(),
                    role: "user".into(),
                    content: "hello".into(),
                    tool_call_id: None,
                    reply_to: None,
                }],
                None,
                |_| {},
            )
            .await
            .unwrap();
        let requests = server
            .received_requests()
            .await
            .expect("both turns were sent");
        let plain: Value = serde_json::from_slice(&requests[1].body).unwrap();
        assert_eq!(plain["forwardedProps"], json!({ "coworkerId": "cw_1" }));
    }

    #[tokio::test]
    async fn list_models_reads_ids() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [{"id":"xai/grok-4.6@sub"},{"id":"oag/auto"}],
                "note": null
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let cat = client.list_models().await.unwrap();
        assert_eq!(cat.models.len(), 2);
        assert_eq!(cat.models[0].id, "xai/grok-4.6@sub");
    }

    #[tokio::test]
    async fn patch_coworker_sends_model_and_role() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/coworkers/cw_1"))
            .and(body_json(json!({
                "model": "xai/grok-4.6@sub",
                "role": "Research, marketing, admin"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "cw_1",
                "model": "xai/grok-4.6@sub",
                "role": "Research, marketing, admin",
                "visibility": "private"
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let updated = client
            .patch_coworker(
                "cw_1",
                &CoworkerPatch {
                    model: Some("xai/grok-4.6@sub".into()),
                    role: Some("Research, marketing, admin".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.model, "xai/grok-4.6@sub");
        assert_eq!(updated.role.as_deref(), Some("Research, marketing, admin"));
    }

    #[tokio::test]
    async fn answer_run_posts_call_id_and_approved() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/runs/run-1/answer"))
            .and(body_json(json!({"call_id":"call-9","approved":true})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "runId": "run-1",
                "callId": "call-9",
                "approved": true,
                "alreadyAnswered": false,
                "continuing": true
            })))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let reply = client.answer_run("run-1", "call-9", true).await.unwrap();
        assert!(!reply.already_answered);
        assert!(reply.continuing);
    }

    #[tokio::test]
    async fn submit_user_form_posts_gateway_entry_id_and_values() {
        use super::super::user_form::{
            FormResolution, UserFormActionReply, UserFormValues, user_form_action_from_http,
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/user-form/submit"))
            .and(body_json(json!({
                "entryId": "e_form",
                "agentId": "cw_1",
                "values": { "email": "ada@example.com", "password": "s3cret-pass" }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": {
                        "title": "Google account",
                        "fields": [
                            {"id": "email", "label": "Email", "type": "email"},
                            {"id": "password", "label": "Password", "type": "password"}
                        ]
                    }
                },
                "formResolution": "submitted",
                "sharedValues": { "email": "ada@example.com" }
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let mut values = UserFormValues::default();
        values
            .by_id
            .insert("email".into(), "ada@example.com".into());
        values.by_id.insert("password".into(), "s3cret-pass".into());
        let reply = client
            .submit_user_form("e_form", "cw_1", &values)
            .await
            .unwrap();
        match reply {
            UserFormActionReply::Settled(spec) => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Submitted));
            }
            other => panic!("expected Settled, got {other:?}"),
        }
        let dump = format!("{values:?}");
        assert!(!dump.contains("s3cret-pass"), "{dump}");
        assert_eq!(
            user_form_action_from_http(404, &Value::Null),
            UserFormActionReply::MissingRoute
        );
    }

    #[tokio::test]
    async fn submit_user_form_404_is_missing_route_not_submitted() {
        use super::super::user_form::UserFormActionReply;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/user-form/submit"))
            .respond_with(
                ResponseTemplate::new(404).set_body_json(json!({ "error": "no such route" })),
            )
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let reply = client
            .submit_user_form("e_form", "cw_1", &Default::default())
            .await
            .unwrap();
        assert_eq!(reply, UserFormActionReply::MissingRoute);
        assert!(!matches!(reply, UserFormActionReply::Settled(_)));
    }

    #[tokio::test]
    async fn submit_user_form_200_null_is_not_a_fill() {
        use super::super::user_form::UserFormActionReply;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/user-form/submit"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Value::Null))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let reply = client
            .submit_user_form("e_form", "cw_1", &Default::default())
            .await
            .unwrap();
        assert_eq!(reply, UserFormActionReply::Empty);
    }

    #[tokio::test]
    async fn empty_entry_id_is_not_posted_as_call_id() {
        use super::super::user_form::UserFormActionReply;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/user-form/submit"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "formResolution": "submitted"
            })))
            .expect(0)
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let reply = client
            .submit_user_form("", "cw_1", &Default::default())
            .await
            .unwrap();
        assert_eq!(reply, UserFormActionReply::MissingEntryId);
    }

    #[tokio::test]
    async fn dismiss_user_form_posts_escalated_mode() {
        use super::super::user_form::{UserFormActionReply, UserFormDismissMode};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/user-form/dismiss"))
            .and(body_json(json!({
                "entryId": "e_form",
                "agentId": "cw_1",
                "mode": "escalated"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "kind": "send-message",
                "id": "e_form",
                "message": { "type": "user-form", "formRequest": { "title": "Sign in", "fields": [] } },
                "formResolution": "escalated",
                "widgetDismissed": true,
                "handoffEntryId": "e_hand"
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let reply = client
            .dismiss_user_form("e_form", "cw_1", UserFormDismissMode::Escalated)
            .await
            .unwrap();
        match reply {
            UserFormActionReply::Settled(spec) => {
                assert_eq!(
                    spec.effective_resolution(),
                    Some(super::super::user_form::FormResolution::Escalated)
                );
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.handoff_entry_id.as_deref(), Some("e_hand"));
            }
            other => panic!("expected Settled, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn resolve_box_handoff_posts_handoff_id_not_form_id() {
        use super::super::user_form::{BoxHandoffReply, BoxHandoffResolution};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/box-handoff/resolve"))
            .and(body_json(json!({
                "entryId": "e_hand",
                "agentId": "cw_1",
                "resolution": "handed_back"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "e_hand",
                "boxResolution": "handed_back"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/box-handoff/resolve"))
            .and(body_json(json!({
                "entryId": "e_form",
                "agentId": "cw_1",
                "resolution": "handed_back"
            })))
            .expect(0)
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let reply = client
            .resolve_box_handoff("e_hand", "cw_1", BoxHandoffResolution::HandedBack)
            .await
            .unwrap();
        assert_eq!(reply, BoxHandoffReply::Settled);
        let skipped = client
            .resolve_box_handoff("", "cw_1", BoxHandoffResolution::HandedBack)
            .await
            .unwrap();
        assert_eq!(skipped, BoxHandoffReply::MissingEntryId);
    }

    /// The route is idempotent, so the status is what is true of the run now rather than an
    /// account of what this call did: a turn that ended a moment before the press answers the
    /// same way one that was still going does.
    #[tokio::test]
    async fn stopping_a_run_posts_to_its_own_route_and_reads_the_status_back() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/runs/run-1/stop"))
            .respond_with(
                ResponseTemplate::new(202)
                    .set_body_json(json!({ "runId": "run-1", "status": "stopped" })),
            )
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let reply = client.stop_run("run-1").await.unwrap();
        assert_eq!(reply.run_id, "run-1");
        assert_eq!(reply.status, "stopped");
    }

    /// The run is not there, or is not ours. Both are `404`, and from the app's side they mean
    /// the same thing - nothing of ours is running under that id - so the caller is the one that
    /// decides whether that is worth saying anything about.
    #[tokio::test]
    async fn stopping_a_run_that_is_not_there_comes_back_as_a_404() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ag-ui/runs/run-gone/stop"))
            .respond_with(
                ResponseTemplate::new(404).set_body_json(json!({ "error": "no such run" })),
            )
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let error = client.stop_run("run-gone").await.unwrap_err();
        assert_eq!(error.status, Some(404));
    }

    #[tokio::test]
    async fn list_approvals_returns_the_waiting_call() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ag-ui/approvals"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
                "runId": "run-1",
                "threadId": "t1",
                "callId": "call-9",
                "tool": "user_machine_shell",
                "arguments": {"command": "ls"}
            }])))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let queue = client.list_approvals().await.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].call_id, "call-9");
        assert_eq!(queue[0].tool, "user_machine_shell");
    }

    #[tokio::test]
    async fn coworker_computer_reads_vnc_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/coworkers/cw_1/computer"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "agentId": "cw_1",
                "state": "running",
                "vncUrl": "http://127.0.0.1:6080/vnc.html"
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let status = client.coworker_computer("cw_1").await.unwrap();
        assert_eq!(status.state, "running");
        assert_eq!(status.vnc_url(), Some("http://127.0.0.1:6080/vnc.html"));
    }

    #[tokio::test]
    async fn ensure_coworker_computer_posts_and_reads_the_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/coworkers/cw_1/computer"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "agentId": "cw_1",
                "state": "running",
                "vncUrl": "http://127.0.0.1:6080/vnc.html"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let status = client.ensure_coworker_computer("cw_1").await.unwrap();
        assert_eq!(status.vnc_url(), Some("http://127.0.0.1:6080/vnc.html"));
    }

    /// A server without the endpoint is a state the app must recognise, not a crash.
    #[tokio::test]
    async fn coworker_computer_reports_a_missing_endpoint_as_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/coworkers/cw_1/computer"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let error = client.coworker_computer("cw_1").await.unwrap_err();
        assert_eq!(error.status, Some(404));
    }

    #[tokio::test]
    async fn a_box_without_a_screen_has_no_vnc_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/coworkers/cw_1/computer"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "agentId": "cw_1",
                "state": "running",
                "vncUrl": null
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let status = client.coworker_computer("cw_1").await.unwrap();
        assert_eq!(status.vnc_url(), None);
    }

    #[test]
    fn latest_queued_approval_is_the_last_for_that_thread() {
        let queue = vec![
            QueuedApproval {
                run_id: "r1".into(),
                thread_id: "t1".into(),
                call_id: "old".into(),
                tool: "user_machine_shell".into(),
                arguments: json!({"command": "ls"}),
            },
            QueuedApproval {
                run_id: "r2".into(),
                thread_id: "t2".into(),
                call_id: "other".into(),
                tool: "user_machine_shell".into(),
                arguments: json!({"command": "pwd"}),
            },
            QueuedApproval {
                run_id: "r3".into(),
                thread_id: "t1".into(),
                call_id: "new".into(),
                tool: "user_machine_shell".into(),
                arguments: json!({"command": "uname"}),
            },
        ];
        let latest = QueuedApproval::latest_for_thread(&queue, "t1").unwrap();
        assert_eq!(latest.call_id, "new");
        assert!(QueuedApproval::latest_for_thread(&queue, "missing").is_none());
    }

    #[test]
    fn local_exec_mode_round_trips_grok_settings_words() {
        assert_eq!(LocalExecMode::from_stored("ask").as_stored(), "ask");
        assert_eq!(LocalExecMode::from_stored("bypass"), LocalExecMode::Always);
        assert_eq!(LocalExecMode::Always.as_stored(), "bypass");
        assert_eq!(LocalExecMode::from_stored("never"), LocalExecMode::Never);
        assert_eq!(LocalExecMode::from_stored(""), LocalExecMode::Never);
        assert_eq!(LocalExecMode::Always.label(), "Always allow");
        assert_eq!(LocalExecMode::Ask.label(), "Ask every time");
        assert_eq!(LocalExecMode::Never.label(), "Never allow");
    }

    #[tokio::test]
    async fn list_computers_skips_revoked_and_reads_mode() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "machines": [
                    {"machineId": "mac_live", "label": "NativeChat on this Mac", "revoked": false, "connected": true},
                    {"machineId": "mac_dead", "label": "old", "revoked": true}
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/local-exec/policy"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"mode": "bypass"})))
            .mount(&server)
            .await;

        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let computers = client.list_computers().await.unwrap();
        assert_eq!(computers.len(), 1);
        assert_eq!(computers[0].machine_id, "mac_live");
        assert_eq!(computers[0].mode, LocalExecMode::Always);
        assert!(computers[0].online);
    }

    #[test]
    fn recipe_step_round_trips_its_op_tag() {
        let steps = vec![
            RecipeStep::Click {
                x: 412,
                y: 88,
                button: Some(json!("left")),
            },
            RecipeStep::DoubleClick { x: 1, y: 2 },
            RecipeStep::Drag {
                x1: 1,
                y1: 2,
                x2: 3,
                y2: 4,
            },
            RecipeStep::Type {
                text: "example.com".into(),
            },
            RecipeStep::Key {
                key: "Return".into(),
            },
            RecipeStep::Scroll {
                x: 10,
                y: 20,
                dx: 0,
                dy: -120,
            },
            RecipeStep::Wait { ms: 500 },
        ];
        let json = serde_json::to_value(&steps).unwrap();
        assert_eq!(json[0]["op"], "click");
        assert_eq!(json[1]["op"], "double_click");
        assert_eq!(json[6], json!({"op": "wait", "ms": 500}));
        let back: Vec<RecipeStep> = serde_json::from_value(json).unwrap();
        assert_eq!(back, steps);
        assert_eq!(back[0].describe(), "click (412, 88)");
        assert_eq!(back[1].describe(), "double-click (1, 2)");
        assert_eq!(back[3].describe(), "type \"example.com\"");
        assert_eq!(back[4].describe(), "key Return");
        assert_eq!(back[6].describe(), "wait 500 ms");
        let right: RecipeStep =
            serde_json::from_value(json!({"op": "click", "x": 1, "y": 2, "button": "right"}))
                .unwrap();
        assert_eq!(right.describe(), "click (1, 2) button right");
    }

    #[test]
    fn recipe_summary_reads_camel_case_and_the_share_state() {
        let summary: RecipeSummary = serde_json::from_value(json!({
            "id": "rcp_1",
            "ownerId": "acc_2",
            "orgId": "org_1",
            "name": "Open the mail",
            "description": null,
            "screen": {"width": 1280, "height": 800},
            "createdAtMs": 1,
            "updatedAtMs": 2,
            "deletedAtMs": null,
            "latestVersion": 2,
            "relation": "invited",
            "shareState": "pending"
        }))
        .unwrap();
        assert_eq!(summary.relation, RecipeRelation::Invited);
        assert_eq!(summary.share_state, Some(RecipeShareState::Pending));
        assert!(summary.is_pending_invite());
        assert!(!summary.is_mine());
        assert_eq!(summary.description, "");
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["ownerId"], "acc_2");
        assert_eq!(json["latestVersion"], 2);
        assert_eq!(json["relation"], "invited");
        assert_eq!(json["shareState"], "pending");
        let back: RecipeSummary = serde_json::from_value(json).unwrap();
        assert_eq!(back, summary);

        // A relation this client has no word for is "none", not a failed list.
        let odd: RecipeSummary =
            serde_json::from_value(json!({"id": "rcp_2", "relation": "custodian"})).unwrap();
        assert_eq!(odd.relation, RecipeRelation::None);
        assert_eq!(odd.screen, RecipeScreen::default());
        assert!(
            odd.parameters.is_empty(),
            "a server that names no parameters asks for nothing, rather than failing to list"
        );
    }

    /// The declaration exactly as `GET /recipes` sends it. A fixture written from memory is how
    /// a shape drifts apart between two repos without either noticing, so this one is the
    /// server's own keys and nothing else.
    #[test]
    fn a_summary_carries_the_parameters_the_server_declares() {
        let summary: RecipeSummary = serde_json::from_value(json!({
            "id": "rcp_1",
            "name": "youtube",
            "parameters": [
                { "name": "search_term", "description": "What to search YouTube for",
                  "required": true, "kind": "text", "default": null, "values": null }
            ]
        }))
        .unwrap();
        let [search_term] = &summary.parameters[..] else {
            panic!("the one declared parameter is read");
        };
        assert_eq!(search_term.name, "search_term");
        assert_eq!(search_term.description, "What to search YouTube for");
        assert!(search_term.required);
        assert_eq!(search_term.kind, RecipeParameterKind::Text);
        assert_eq!(search_term.default, None);
        assert_eq!(search_term.allowed(), None);

        // Round-trip, so what this client would send back reads as what it was sent.
        let json = serde_json::to_value(&summary.parameters).unwrap();
        assert_eq!(json[0]["name"], "search_term");
        assert_eq!(json[0]["kind"], "text");
        assert_eq!(json[0]["required"], true);
        assert!(json[0]["default"].is_null());
        assert!(json[0]["values"].is_null());
        let back: Vec<RecipeParameter> = serde_json::from_value(json).unwrap();
        assert_eq!(back, summary.parameters);

        // The other two kinds, a default sent as the thing itself, and a narrowed set.
        let rest: Vec<RecipeParameter> = serde_json::from_value(json!([
            { "name": "count", "description": "", "required": false, "kind": "number",
              "default": 5, "values": null },
            { "name": "shorts", "description": "", "required": false, "kind": "boolean",
              "default": false, "values": null },
            { "name": "lang", "description": "", "required": true, "kind": "text",
              "default": null, "values": ["en", "es"] },
            { "name": "mystery", "description": "", "required": false, "kind": "colour",
              "default": null, "values": null }
        ]))
        .unwrap();
        assert_eq!(rest[0].kind, RecipeParameterKind::Number);
        assert_eq!(rest[0].default.as_deref(), Some("5"));
        assert_eq!(rest[1].kind, RecipeParameterKind::Boolean);
        assert_eq!(rest[1].default.as_deref(), Some("false"));
        assert_eq!(
            rest[2].allowed(),
            Some(&["en".to_string(), "es".to_string()][..])
        );
        assert_eq!(
            rest[3].kind,
            RecipeParameterKind::Text,
            "a kind this client has no word for leaves a field that can still be typed into"
        );
    }

    #[test]
    fn a_version_declares_what_it_needs_told_whichever_half_of_the_row_it_is_on() {
        let beside: RecipeVersion = serde_json::from_value(json!({
            "version": 2, "kind": "filtered", "createdAtMs": 2,
            "parameters": [{"name": "search_term", "required": true, "kind": "text"}],
            "body": {"steps": []}
        }))
        .unwrap();
        assert_eq!(beside.declared().len(), 1);
        assert_eq!(beside.declared()[0].name, "search_term");

        let inside: RecipeVersion = serde_json::from_value(json!({
            "version": 2, "kind": "filtered", "createdAtMs": 2,
            "body": {"steps": [], "parameters": [
                {"name": "search_term", "required": true, "kind": "text"}
            ]}
        }))
        .unwrap();
        assert_eq!(inside.declared(), beside.declared());

        // A version taught before parameters existed declares nothing, and reads as it always
        // did rather than failing.
        let older: RecipeVersion =
            serde_json::from_value(json!({"version": 1, "kind": "raw", "body": {"events": 3}}))
                .unwrap();
        assert!(older.declared().is_empty());
        assert_eq!(older.event_count(), 3);
    }

    #[test]
    fn a_value_is_checked_against_the_kind_and_sent_as_that_kind() {
        let number: RecipeParameter =
            serde_json::from_value(json!({"name": "count", "required": true, "kind": "number"}))
                .unwrap();
        assert_eq!(number.reject("12"), None);
        assert_eq!(number.reject("-1.5"), None);
        assert_eq!(
            number.reject("ten").as_deref(),
            Some("count takes a number, and \"ten\" is not one.")
        );
        assert_eq!(number.encode("12"), json!(12));
        assert_eq!(number.encode("1.5"), json!(1.5));

        let flag: RecipeParameter =
            serde_json::from_value(json!({"name": "shorts", "kind": "boolean"})).unwrap();
        assert_eq!(flag.reject("yes"), None);
        assert_eq!(
            flag.reject("maybe").as_deref(),
            Some("shorts is a yes or a no, so pick one.")
        );
        assert_eq!(flag.encode("yes"), json!(true));
        assert_eq!(flag.encode("false"), json!(false));

        let lang: RecipeParameter =
            serde_json::from_value(json!({"name": "lang", "kind": "text", "values": ["en", "es"]}))
                .unwrap();
        assert_eq!(lang.reject("es"), None);
        assert_eq!(
            lang.reject("fr").as_deref(),
            Some("lang takes one of: en, es."),
            "a refusal that does not say what is allowed leaves the person guessing"
        );
        assert_eq!(
            lang.encode("ES"),
            json!("es"),
            "case is not what they are choosing between, so the declared spelling goes"
        );

        let free: RecipeParameter =
            serde_json::from_value(json!({"name": "search_term", "kind": "text"})).unwrap();
        assert_eq!(free.reject("anything at all"), None);
        assert_eq!(free.encode("mundo"), json!("mundo"));
    }

    #[test]
    fn a_raw_version_reads_the_tape_the_server_sends() {
        // The server sends the count under `events` and the tape under `tape`, cut at a cap.
        let sent: RecipeVersion = serde_json::from_value(json!({
            "version": 1, "kind": "raw", "createdAtMs": 1,
            "body": {"events": 3100, "truncated": true, "tape": [
                {"kind": "down", "x": 640, "y": 60, "button": 0, "at": 1000},
                {"kind": "keydown", "key": "e", "code": "KeyE", "at": 1900}
            ]}
        }))
        .unwrap();
        assert_eq!(sent.event_count(), 3100, "the count is the whole tape's");
        assert_eq!(sent.tape_events().map(<[_]>::len), Some(2));
        assert!(sent.tape_truncated());

        // A body that puts the events under `events` instead reads the same way.
        let inline: RecipeVersion = serde_json::from_value(json!({
            "version": 1, "kind": "raw", "createdAtMs": 1,
            "body": {"events": [{"kind": "up", "x": 1, "y": 2, "at": 5}]}
        }))
        .unwrap();
        assert_eq!(inline.event_count(), 1);
        assert_eq!(inline.tape_events().map(<[_]>::len), Some(1));
        assert!(!inline.tape_truncated());

        // And a count alone still says how much was taped, with no tape to show.
        let counted: RecipeVersion = serde_json::from_value(json!({
            "version": 1, "kind": "raw", "createdAtMs": 1, "body": {"events": 14}
        }))
        .unwrap();
        assert_eq!(counted.event_count(), 14);
        assert!(counted.tape_events().is_none());
    }

    #[test]
    fn recipe_detail_reads_versions_shares_grants_runs_and_bots() {
        let detail: RecipeDetail = serde_json::from_value(json!({
            "recipe": {"id": "rcp_1", "ownerId": "acc_1", "name": "Mail", "relation": "mine", "latestVersion": 2},
            "versions": [
                {"version": 1, "kind": "raw", "note": null, "createdBy": "acc_1", "createdAtMs": 1, "body": {"events": 312}},
                {"version": 2, "kind": "filtered", "note": null, "createdBy": null, "createdAtMs": 2,
                 "body": {"steps": [{"op": "click", "x": 1, "y": 2, "button": "left"}, {"op": "wait", "ms": 200}], "stop_on_error": true, "screenshot": null}}
            ],
            // Exactly the shape the server sends: its row structs are camelCase too, and a
            // snake_case fixture here let every grant, share and run bind to nothing at all.
            "shares": [{"recipeId": "rcp_1", "scope": "org", "scopeId": "org_1", "grantedBy": "acc_1", "grantedAtMs": 3, "acceptedAtMs": null, "declinedAtMs": null}],
            "grants": [{"recipeId": "rcp_1", "coworkerId": "cw_1", "grantedBy": "acc_1", "grantedAtMs": 4}],
            "runs": [{"id": "rrun_1", "recipeId": "rcp_1", "version": 2, "coworkerId": "cw_1", "runId": "run_1", "ok": false, "stoppedAt": 1,
                      "receipt": {"ok": false, "ran": 1, "stopped_at": 1, "steps": [{"ok": true}, {"ok": false, "error": "nothing at (5, 5)"}]}, "atMs": 5}],
            "myBots": [{"id": "cw_1", "name": "Bob"}]
        }))
        .unwrap();
        assert_eq!(detail.versions[0].event_count(), 312);
        assert!(!detail.versions[0].is_runnable());
        assert_eq!(detail.runnable_version().map(|v| v.version), Some(2));
        assert_eq!(detail.versions[1].body.steps.len(), 2);
        assert_eq!(detail.shares[0].state_label(), "pending");
        assert!(detail.is_granted("cw_1"));
        assert!(!detail.is_granted("cw_2"));
        assert_eq!(detail.bot_name("cw_1"), "Bob");
        assert_eq!(detail.bot_name("cw_2"), "cw_2");
        assert_eq!(detail.runs[0].stopped_at, Some(1));
        assert_eq!(detail.runs[0].coworker_id, "cw_1");
        assert_eq!(detail.runs[0].at_ms, 5);
        assert_eq!(
            detail.runs[0].id, "rrun_1",
            "the history names a row by this"
        );
        assert_eq!(
            detail.runs[0].receipt_steps(),
            vec![(true, None), (false, Some("nothing at (5, 5)".to_string()))]
        );
        assert_eq!(
            detail.runs[0].error().as_deref(),
            Some("nothing at (5, 5)"),
            "a run's reason is the first step that gave one"
        );
        assert_eq!(detail.shares[0].scope_id, "org_1");
        assert!(
            detail.versions[0].tape_events().is_none(),
            "a count is not a tape"
        );
    }

    #[test]
    fn a_raw_version_takes_the_tape_or_its_count() {
        let with_tape: RecipeVersion = serde_json::from_value(json!({
            "version": 1,
            "kind": "raw",
            "body": {"events": [
                {"kind": "down", "x": 640, "y": 60, "button": 1, "at": 1000},
                {"kind": "keydown", "key": "e", "code": "KeyE", "at": 2200},
            ]},
        }))
        .unwrap();
        assert_eq!(with_tape.event_count(), 2);
        let events = with_tape.tape_events().expect("the tape itself");
        assert_eq!(events[0].button, 1);
        assert_eq!(events[1].key, "e");
        assert_eq!(events[1].at, 2200);

        let counted: RecipeVersion =
            serde_json::from_value(json!({"version": 1, "kind": "raw", "body": {"events": 14}}))
                .unwrap();
        assert_eq!(counted.event_count(), 14);
        assert!(counted.tape_events().is_none());

        // A shape this client has no word for leaves the version readable, not unparsable.
        let odd: RecipeVersion = serde_json::from_value(
            json!({"version": 1, "kind": "raw", "body": {"events": {"total": 14}}}),
        )
        .unwrap();
        assert_eq!(odd.event_count(), 0);
        assert!(odd.tape_events().is_none());
    }

    #[test]
    fn share_target_serialises_its_scope() {
        assert_eq!(
            serde_json::to_value(RecipeShareTarget::Org).unwrap(),
            json!({"scope": "org"})
        );
        assert_eq!(
            serde_json::to_value(RecipeShareTarget::Account {
                email: "a@b.c".into()
            })
            .unwrap(),
            json!({"scope": "account", "email": "a@b.c"})
        );
    }

    #[test]
    fn thin_tape_keeps_one_move_per_forty_ms_and_every_other_event() {
        let event = |kind: &str, at: i64| json!({"kind": kind, "x": 1, "y": 1, "at": at});
        let tape = vec![
            event("down", 0),
            event("move", 5),
            event("move", 20),
            event("move", 44),
            event("move", 45),
            event("up", 46),
            event("move", 90),
            event("keydown", 91),
        ];
        let thin = thin_tape(&tape);
        let kept: Vec<(&str, i64)> = thin
            .iter()
            .map(|e| (e["kind"].as_str().unwrap(), e["at"].as_i64().unwrap()))
            .collect();
        assert_eq!(
            kept,
            vec![
                ("down", 0),
                ("move", 5),
                ("move", 45),
                ("up", 46),
                ("move", 90),
                ("keydown", 91),
            ]
        );
        assert!(thin_tape(&[]).is_empty());
    }

    #[tokio::test]
    async fn create_recipe_refuses_an_upload_over_the_limit_before_sending() {
        // Nothing listens here; the refusal must come from the size check, not the socket.
        let client = OpenGrokClient::new("http://127.0.0.1:9").unwrap();
        let big = "x".repeat(RECIPE_UPLOAD_LIMIT + 1);
        let error = client
            .create_recipe(
                "Big",
                "",
                &[json!({"kind": "keydown", "key": big, "at": 0})],
            )
            .await
            .unwrap_err();
        assert!(error.message.contains("5 MB"), "{}", error.message);
    }

    #[tokio::test]
    async fn list_recipes_sends_the_filter_and_reads_the_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/recipes"))
            .and(wiremock::matchers::query_param("filter", "shared"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "recipes": [{"id": "rcp_1", "ownerId": "acc_2", "name": "Mail", "relation": "shared", "shareState": "accepted", "latestVersion": 3}]
            })))
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let list = client.list_recipes(Some("shared")).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Mail");
        assert_eq!(list[0].relation, RecipeRelation::Shared);
        assert_eq!(list[0].latest_version, 3);
    }

    #[tokio::test]
    async fn recipe_errors_carry_the_servers_sentence() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/recipes/rcp_1/share"))
            .respond_with(
                ResponseTemplate::new(403).set_body_string("Only the owner can share a recipe."),
            )
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let error = client
            .share_recipe("rcp_1", &RecipeShareTarget::Org)
            .await
            .unwrap_err();
        assert_eq!(error.status, Some(403));
        assert_eq!(error.message, "Only the owner can share a recipe.");
    }
}
