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
}

impl RecipeSummary {
    pub fn is_mine(&self) -> bool {
        self.relation == RecipeRelation::Mine
    }

    /// A share the person has not answered: Accept or Decline comes before anything else.
    pub fn is_pending_invite(&self) -> bool {
        self.relation == RecipeRelation::Invited
            && matches!(self.share_state, Some(RecipeShareState::Pending) | None)
    }
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
    #[serde(default)]
    pub body: RecipeVersionBody,
}

impl RecipeVersion {
    /// The tape is kept, not run; every other kind carries steps.
    pub fn is_runnable(&self) -> bool {
        self.kind != "raw"
    }

    /// How many tape events a raw version holds: the server sends the count, or the events.
    pub fn event_count(&self) -> u64 {
        match &self.body.events {
            Some(Value::Number(count)) => count.as_u64().unwrap_or(0),
            Some(Value::Array(events)) => events.len() as u64,
            _ => 0,
        }
    }
}

/// A version's body: the tape's size for v1, the steps for every version after it. The keys
/// are the server's snake_case.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RecipeVersionBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Value>,
    #[serde(default)]
    pub steps: Vec<RecipeStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_on_error: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<Value>,
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

/// Who a recipe has been shared with (snake_case from the server; only the owner sees these).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A bot that may run the recipe (snake_case from the server).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// One past run of the recipe (snake_case from the server; newest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeRun {
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
    #[serde(default)]
    pub at_ms: i64,
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
                    reply_to: None,
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
            "shares": [{"recipe_id": "rcp_1", "scope": "org", "scope_id": "org_1", "granted_by": "acc_1", "granted_at_ms": 3, "accepted_at_ms": null, "declined_at_ms": null}],
            "grants": [{"recipe_id": "rcp_1", "coworker_id": "cw_1", "granted_by": "acc_1", "granted_at_ms": 4}],
            "runs": [{"id": 9, "recipe_id": "rcp_1", "version": 2, "coworker_id": "cw_1", "run_id": "run_1", "ok": false, "stopped_at": 1, "receipt": {}, "at_ms": 5}],
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
