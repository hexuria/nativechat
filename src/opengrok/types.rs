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
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub avatar_shape: Option<String>,
    #[serde(default)]
    pub avatar_color: Option<String>,
    #[serde(default)]
    pub notify_on_updates: Option<bool>,
    /// Hire/rename time from the server. Idle bots (no messages) sort by this.
    #[serde(default, alias = "updated_at_ms", alias = "updatedAt")]
    pub updated_at_ms: i64,
    #[serde(default, alias = "hiddenFromSidebar")]
    pub hidden_from_sidebar: bool,
    #[serde(default, alias = "boxId", alias = "box_id")]
    pub box_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoworkerPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_shape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_on_updates: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_from_sidebar: Option<bool>,
}

impl CoworkerPatch {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.model.is_none()
            && self.role.is_none()
            && self.title.is_none()
            && self.avatar_shape.is_none()
            && self.avatar_color.is_none()
            && self.notify_on_updates.is_none()
            && self.hidden_from_sidebar.is_none()
    }
}

/// One of the account's conversations, as `GET /ag-ui/threads` lists it.
///
/// Transcribed from `ThreadListRow` in opengrok-server
/// `crates/opengrok-server/src/agui/routes.rs` (hexuria/opengrok-server#247, for #230). The list
/// is the caller's own threads, newest first, without hidden runs or the MCP door's audit thread.
/// The server sends `coworkerId` and `title` as `null` when there is none, never leaves them out.
/// Every other field is always there, so a row without one is refused rather than read as zero:
/// a time of zero would date the thread 1970 and hand the pager a cursor that ends the list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadListing {
    pub thread_id: String,
    #[serde(default)]
    pub coworker_id: Option<String>,
    /// `chat`, `schedule`, `webhook` or `monitor`: whether a person started the thread or one of
    /// their routines did.
    pub origin: String,
    /// A routine's name, or the first line of the first thing the person said.
    #[serde(default)]
    pub title: Option<String>,
    pub last_run_id: String,
    pub last_status: String,
    /// The thread's latest activity. The list is ordered by it, and a page's last row hands it
    /// back as the cursor for the next page.
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ModelEntry {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModelCatalogue {
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    #[serde(default)]
    pub note: Option<String>,
}

/// The message a reply points at, carried beside the message that answers it.
///
/// The quote is also spelled into the user message's `content`, because today's server reads
/// only `content`. This field is what a server that understands replies should read instead:
/// it can find the quoted message itself and word the context its own way.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReplyQuote {
    #[serde(rename = "messageId")]
    pub message_id: String,
    /// The quoted words, already clipped: a reply to a long answer names it, it does not replay it.
    pub preview: String,
    /// The person wrote the quoted message, rather than the coworker.
    #[serde(rename = "isMe")]
    pub is_me: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AguiMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(rename = "replyTo", skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<ReplyQuote>,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The field is new: a server that has never heard of it must still see the array it saw
    /// before, field for field.
    #[test]
    fn a_message_with_no_reply_leaves_reply_to_off_the_wire() {
        let message = AguiMessage {
            id: "m1".into(),
            role: "user".into(),
            content: "hi".into(),
            tool_call_id: None,
            reply_to: None,
        };
        let json = serde_json::to_value(&message).expect("serialises");
        assert_eq!(json["content"], "hi");
        assert!(json.get("replyTo").is_none(), "{json}");
        assert!(json.get("toolCallId").is_none(), "{json}");
    }

    #[test]
    fn a_reply_rides_along_under_reply_to() {
        let message = AguiMessage {
            id: "m2".into(),
            role: "user".into(),
            content: "what am I replying to?".into(),
            tool_call_id: None,
            reply_to: Some(ReplyQuote {
                message_id: "m1".into(),
                preview: "The build is green.".into(),
                is_me: false,
            }),
        };
        let json = serde_json::to_value(&message).expect("serialises");
        assert_eq!(json["replyTo"]["messageId"], "m1");
        assert_eq!(json["replyTo"]["preview"], "The build is green.");
        assert_eq!(json["replyTo"]["isMe"], false);
    }
}
