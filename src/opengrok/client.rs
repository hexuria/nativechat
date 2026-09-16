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

    /// AG-UI `principal_from_bearer` only reads `Authorization`, not cookies.
    /// The console login stores the same JWT as `og_access`; send it as Bearer
    /// the way the desktop sends it as `x-opengrok-account` on Seam A.
    fn access_token(&self) -> Option<String> {
        let header = CookieStore::cookies(self.jar.as_ref(), &self.base)?;
        let raw = header.to_str().ok()?;
        for pair in raw.split(';') {
            let pair = pair.trim();
            if let Some((name, value)) = pair.split_once('=')
                && name.trim() == "og_access"
            {
                return Some(value.trim().to_string());
            }
        }
        None
    }

    /// Seconds until the access token expires, read from the JWT's `exp` without verifying
    /// it — the server verifies; this only decides whether to refresh first.
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
    /// runs it as nobody and the turn is held with no word to the person. Thirty seconds of
    /// slack covers a request that is slow to leave. Best effort; the request goes out either
    /// way and a 401 gets one more chance below.
    async fn ensure_fresh_token(&self, path: &str) {
        if path.starts_with("/auth/") {
            return;
        }
        if matches!(self.token_seconds_left(), Some(left) if left < 30) {
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
        let response = build(self.access_token())
            .send()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
        // A 401 on a signed-in session is a token that died between checks: refresh once and
        // send again. Auth routes are exempt, or a bad password would loop here.
        if response.status() == StatusCode::UNAUTHORIZED
            && !path.starts_with("/auth/")
            && self.refresh().await.is_ok()
        {
            return build(self.access_token())
                .send()
                .await
                .map_err(|e| OpenGrokError::message(e.to_string()));
        }
        Ok(response)
    }

    async fn read_error(response: reqwest::Response) -> OpenGrokError {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        OpenGrokError::status(status, error_message_from_body(&body))
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
            .map_err(|e| OpenGrokError::message(e.to_string()))
    }

    pub async fn health(&self) -> Result<(), OpenGrokError> {
        let url = self.url("/health")?;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
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
        let response = req
            .send()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
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
    pub async fn run_turn<F>(
        &self,
        coworker_id: &str,
        thread_id: &str,
        messages: &[AguiMessage],
        mut on_event: F,
    ) -> Result<String, OpenGrokError>
    where
        F: FnMut(&serde_json::Value),
    {
        let body = json!({
            "threadId": thread_id,
            "runId": uuid::Uuid::now_v7().to_string(),
            "messages": messages,
            "tools": super::gen_ui::agui_tools(),
            "forwardedProps": { "coworkerId": coworker_id },
        });
        let url = self.url("/ag-ui")?;
        self.ensure_fresh_token("/ag-ui").await;
        let mut req = self
            .http
            .post(url)
            .header(ACCEPT, "text/event-stream")
            .header(CACHE_CONTROL, "no-cache")
            .json(&body);
        if let Some(token) = self.access_token() {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        let mut assistant = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| OpenGrokError::message(e.to_string()))?;
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
                        return Err(OpenGrokError::message(message));
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

    pub async fn replay_run(&self, run_id: &str) -> Result<RunReplay, OpenGrokError> {
        let path = format!("/ag-ui/runs/{run_id}");
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
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
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
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        let mut stream = response.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| OpenGrokError::message(e.to_string()))?;
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

#[derive(Debug, Clone, Deserialize)]
pub struct RunReplay {
    #[serde(rename = "runId", default)]
    pub run_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub pending: Option<serde_json::Value>,
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
}

impl CoworkerComputer {
    pub fn vnc_url(&self) -> Option<&str> {
        self.vnc_url.as_deref().filter(|url| !url.is_empty())
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
        let first_at = std::sync::Arc::new(std::sync::Mutex::new(None::<Instant>));
        let start = Instant::now();
        let text = client
            .run_turn(
                "cw",
                "t",
                &[AguiMessage {
                    id: "u1".into(),
                    role: "user".into(),
                    content: "hi".into(),
                    tool_call_id: None,
                }],
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
}
