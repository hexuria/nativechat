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
    // No `notifyOnUpdates`: opengrok-server reads it nowhere, on purpose (`agui/routes.rs`,
    // the coworker PATCH), and refuses a patch carrying only that with a 400. The desktop
    // client keeps that setting on the machine, and NativeChat has nowhere to keep it yet.
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

#[derive(Debug, Clone)]
pub struct AguiMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub reply_to: Option<ReplyQuote>,
    /// Files the person attached to this message, already uploaded (#90). With none, `content`
    /// goes as the plain string it always was.
    pub attachments: Vec<Attachment>,
}

/// A message goes as `{id, role, content, toolCallId?, replyTo?}`. `content` is the words, or,
/// when files ride along, an array of AG-UI 1.0 parts: a `text` part for the words and one part
/// per file (see [`Attachment::part`]). Transcribed from opengrok-server#259
/// (`crates/opengrok-wire/src/agui.rs` `Content`), which reads either.
impl Serialize for AguiMessage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("role", &self.role)?;
        if self.attachments.is_empty() {
            map.serialize_entry("content", &self.content)?;
        } else {
            let mut parts = Vec::with_capacity(self.attachments.len() + 1);
            if !self.content.is_empty() {
                parts.push(serde_json::json!({"type": "text", "text": self.content}));
            }
            parts.extend(self.attachments.iter().map(Attachment::part));
            map.serialize_entry("content", &parts)?;
        }
        if let Some(id) = &self.tool_call_id {
            map.serialize_entry("toolCallId", id)?;
        }
        if let Some(quote) = &self.reply_to {
            map.serialize_entry("replyTo", quote)?;
        }
        map.end()
    }
}

/// A file uploaded to `POST /artifacts` for a message, as the upload answers it (the server's
/// `ArtifactRow`, camelCase, opengrok-store `postgres.rs`). Only what the app uses is read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub mime: String,
    pub filename: String,
    pub size_bytes: u64,
}

impl Attachment {
    /// The part that names this file in a message: AG-UI 1.0 `ImagePartSchema` for a picture and
    /// `DocumentPartSchema` for anything else, each with a `FileSourceSchema` source whose
    /// provider is the server that issued the `art_` id (`ag-ui-protocol/ag-ui`
    /// `sdks/typescript/packages/core/src/generated/schemas.ts` at `b8ebd02c84`; the shape agreed
    /// on hexuria/nativechat#90).
    pub fn part(&self) -> serde_json::Value {
        let kind = if self.mime.starts_with("image/") {
            "image"
        } else {
            "document"
        };
        serde_json::json!({
            "type": kind,
            "source": {
                "type": "file",
                "value": self.id,
                "provider": "opengrok",
                "mimeType": self.mime,
            },
            "metadata": {"filename": self.filename, "sizeBytes": self.size_bytes},
        })
    }
}

/// A file sent on a thread, as `GET /artifacts?threadId=` lists it (opengrok-server#259
/// `artifacts.rs` `list_for_thread`): the row, with `meta.messageId` saying which message it
/// rode on. A row without one was uploaded and never sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentAttachment {
    pub file: Attachment,
    pub message_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArtifactListing {
    #[serde(flatten)]
    pub file: Attachment,
    #[serde(default)]
    pub meta: serde_json::Value,
}

impl ArtifactListing {
    pub(crate) fn sent(self) -> Option<SentAttachment> {
        let message_id = self.meta.get("messageId")?.as_str()?.to_string();
        Some(SentAttachment {
            file: self.file,
            message_id,
        })
    }
}

/// Pull assistant `delta` fields out of an AG-UI SSE body (desktop Seam A
/// paints from `/events`; we consume the same TEXT_MESSAGE_CONTENT frames
/// on the `POST /ag-ui` stream).
pub fn assistant_text_from_sse(body: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut persons = super::gen_ui::PersonsText::default();
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
            // The person's own words (a replay opens each run with them) are not the reply.
            let persons = kind.starts_with("TEXT_MESSAGE") && persons.is_persons(&value);
            if !persons
                && (kind == "TEXT_MESSAGE_CONTENT" || kind == "TEXT_MESSAGE_CHUNK")
                && let Some(delta) = value.get("delta").and_then(|v| v.as_str())
            {
                out.push_str(delta);
            }
        }
    }
    Ok(out)
}

pub fn error_message_from_body(body: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<ErrorBody>(body)
        && let Some(error) = parsed.error
        && !error.is_empty()
    {
        return error;
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

    /// Every key a coworker patch can carry is one the server's patch route reads
    /// (opengrok-server `agui/routes.rs`: name, model, role, visibility, hiddenFromSidebar, and
    /// title/avatarShape/avatarColor). A key it reads nowhere is a setting that looks saved and
    /// is not, and a patch of only that is refused.
    #[test]
    fn a_coworker_patch_names_only_what_the_server_keeps() {
        let full = CoworkerPatch {
            name: Some("n".into()),
            model: Some("m".into()),
            role: Some("r".into()),
            title: Some("t".into()),
            avatar_shape: Some("s".into()),
            avatar_color: Some("c".into()),
            hidden_from_sidebar: Some(true),
        };
        let wire = serde_json::to_value(&full).unwrap();
        let mut keys: Vec<&str> = wire
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let read = [
            "avatarColor",
            "avatarShape",
            "hiddenFromSidebar",
            "model",
            "name",
            "role",
            "title",
            "visibility",
        ];
        for key in &keys {
            assert!(read.contains(key), "the server reads no {key:?}");
        }
        assert_eq!(keys.len(), 7, "every field is on the wire: {keys:?}");
    }

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
            attachments: Vec::new(),
        };
        let json = serde_json::to_value(&message).expect("serialises");
        assert_eq!(json["content"], "hi");
        assert!(json.get("replyTo").is_none(), "{json}");
        assert!(json.get("toolCallId").is_none(), "{json}");
    }

    /// With files, `content` is the AG-UI parts: the words first, then one part per file, a
    /// picture as `image` and anything else as `document`, each naming its `art_` id as a file
    /// the opengrok server issued (hexuria/nativechat#90, opengrok-server#259).
    #[test]
    fn files_ride_as_parts_after_the_words() {
        let file = |id: &str, mime: &str, name: &str| Attachment {
            id: id.into(),
            mime: mime.into(),
            filename: name.into(),
            size_bytes: 42,
        };
        let message = AguiMessage {
            id: "m1".into(),
            role: "user".into(),
            content: "What changed in Q3?".into(),
            tool_call_id: None,
            reply_to: None,
            attachments: vec![
                file("art_1", "application/pdf", "q3.pdf"),
                file("art_2", "image/png", "screen.png"),
            ],
        };
        let json = serde_json::to_value(&message).expect("serialises");
        assert_eq!(
            json["content"],
            serde_json::json!([
                {"type": "text", "text": "What changed in Q3?"},
                {"type": "document",
                 "source": {"type": "file", "value": "art_1", "provider": "opengrok", "mimeType": "application/pdf"},
                 "metadata": {"filename": "q3.pdf", "sizeBytes": 42}},
                {"type": "image",
                 "source": {"type": "file", "value": "art_2", "provider": "opengrok", "mimeType": "image/png"},
                 "metadata": {"filename": "screen.png", "sizeBytes": 42}}
            ])
        );
        // Files alone: no empty text part.
        let alone = AguiMessage {
            content: String::new(),
            ..message
        };
        let json = serde_json::to_value(&alone).expect("serialises");
        assert_eq!(json["content"].as_array().map(Vec::len), Some(2));
        assert_eq!(json["content"][0]["type"], "document");
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
            attachments: Vec::new(),
        };
        let json = serde_json::to_value(&message).expect("serialises");
        assert_eq!(json["replyTo"]["messageId"], "m1");
        assert_eq!(json["replyTo"]["preview"], "The build is green.");
        assert_eq!(json["replyTo"]["isMe"], false);
    }
}
