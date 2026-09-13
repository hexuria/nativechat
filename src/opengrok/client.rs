use std::sync::Arc;

use reqwest::cookie::{CookieStore, Jar};
use reqwest::{Client, StatusCode, Url};
use serde::Serialize;
use serde_json::json;

use super::error::OpenGrokError;
use super::types::{
    assistant_text_from_sse, error_message_from_body, Account, AguiMessage, Coworker,
    ProfileUpdate,
};

#[derive(Clone)]
pub struct OpenGrokClient {
    base: Url,
    http: Client,
    jar: Arc<Jar>,
}

impl OpenGrokClient {
    pub fn new(base_url: &str) -> Result<Self, OpenGrokError> {
        let base = Url::parse(base_url).map_err(|e| {
            OpenGrokError::message(format!("invalid OpenGrok URL {base_url}: {e}"))
        })?;
        let jar = Arc::new(Jar::default());
        let http = Client::builder()
            .cookie_provider(jar.clone())
            .build()
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
        Ok(Self { base, http, jar })
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
        let mut req = self.http.request(method, url);
        if let Some(token) = self.access_token() {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        req.send()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))
    }

    async fn read_error(response: reqwest::Response) -> OpenGrokError {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        OpenGrokError::status(status, error_message_from_body(&body))
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
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    pub async fn refresh(&self) -> Result<(), OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::POST, "/auth/refresh", None)
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Self::read_error(response).await)
        }
    }

    pub async fn logout(&self) -> Result<(), OpenGrokError> {
        let response = self
            .send_json::<()>(reqwest::Method::POST, "/auth/logout", None)
            .await?;
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
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))
    }

    pub async fn update_profile(&self, update: &ProfileUpdate) -> Result<Account, OpenGrokError> {
        let response = self
            .send_json(reqwest::Method::POST, "/account/profile", Some(update))
            .await?;
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))
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
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))
    }

    pub async fn hire(
        &self,
        name: &str,
        model: Option<&str>,
    ) -> Result<Coworker, OpenGrokError> {
        let mut body = json!({ "name": name });
        if let Some(model) = model.filter(|m| !m.is_empty()) {
            body["model"] = json!(model);
        }
        let response = self
            .send_json(reqwest::Method::POST, "/coworkers", Some(&body))
            .await?;
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        response
            .json()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))
    }

    /// One turn. Desktop Grok Bot POSTs `/api/sendPrompt` then paints from `GET /events`.
    /// NativeChat is a new client: same coworker + transcript, `POST /ag-ui` SSE instead.
    pub async fn run_turn(
        &self,
        coworker_id: &str,
        thread_id: &str,
        messages: &[AguiMessage],
    ) -> Result<String, OpenGrokError> {
        let body = json!({
            "threadId": thread_id,
            "runId": uuid::Uuid::now_v7().to_string(),
            "messages": messages,
            "forwardedProps": { "coworkerId": coworker_id },
        });
        let response = self
            .send_json(reqwest::Method::POST, "/ag-ui", Some(&body))
            .await?;
        if !response.status().is_success() {
            return Err(Self::read_error(response).await);
        }
        let text = response
            .text()
            .await
            .map_err(|e| OpenGrokError::message(e.to_string()))?;
        assistant_text_from_sse(&text).map_err(OpenGrokError::message)
    }
}

#[cfg(test)]
mod tests {
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
    async fn login_401_is_distinguishable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/auth/login"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({"error":"bad password"})),
            )
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
}
