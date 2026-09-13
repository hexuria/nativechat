use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub is_admin: Option<bool>,
}

impl Account {
    pub fn display_name(&self) -> String {
        let name = format!("{} {}", self.first_name, self.last_name)
            .trim()
            .to_string();
        if name.is_empty() {
            self.email.clone()
        } else {
            name
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Coworker {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AguiMessage {
    pub id: String,
    pub role: String,
    pub content: String,
}

/// Pull assistant `delta` fields out of an AG-UI SSE body (desktop Seam A
/// paints from `/events`; we consume the same TEXT_MESSAGE_CONTENT frames
/// on the `POST /ag-ui` stream).
pub fn assistant_text_from_sse(body: &str) -> Result<String, String> {
    let mut out = String::new();
    for block in body.split("\n\n") {
        for line in block.lines() {
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
                return Err(message.to_string());
            }
            if kind == "TEXT_MESSAGE_CONTENT" || kind == "TEXT_MESSAGE_CHUNK" {
                if let Some(delta) = value.get("delta").and_then(|v| v.as_str()) {
                    out.push_str(delta);
                }
            }
        }
    }
    Ok(out)
}

pub fn error_message_from_body(body: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<ErrorBody>(body) {
        if let Some(error) = parsed.error {
            if !error.is_empty() {
                return error;
            }
        }
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "request failed".to_string()
    } else {
        trimmed.to_string()
    }
}
