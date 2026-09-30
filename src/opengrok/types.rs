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
    /// How hard it thinks before it answers: one of [`EFFORT_WORDS`], or a word this app has not
    /// heard of, kept as the server sent it so the settings never show a value the server does
    /// not hold. Missing is a server from before opengrok-server#271, which keeps no effort and
    /// sends none on a turn: that reads as `inherit` ([`Self::effort`]), and nothing offers to
    /// change what that server has nowhere to keep.
    #[serde(default)]
    pub effort: Option<String>,
    /// Who may use this bot: `private`, its owner alone, or `org`, shared with the owner's
    /// organization. Transcribed from opengrok-server `agui/routes.rs` `coworker_row`, which
    /// writes it on every row, as `fixtures/wire/rest/GET__coworkers/` records both words. The
    /// app reads it through [`Self::is_shared`].
    #[serde(default)]
    pub visibility: Option<String>,
}

/// How hard a coworker thinks before it answers, in the server's words and in its order.
/// Transcribed from opengrok-server#271, the shape agreed with the server before it landed:
/// every roster row carries one of these under `effort`, and `PATCH /coworkers/{id}` takes one.
/// `inherit` is a coworker with none set, whose turns send the gateway no effort, so the model's
/// route decides; any other word goes to the gateway as the turn's `reasoning_effort`, taken when
/// the run starts. Some models ignore it, and the server cannot know which.
pub const EFFORT_WORDS: [&str; 7] = ["inherit", "none", "low", "medium", "high", "xhigh", "max"];

/// The effort of a coworker with none set, and of every coworker on a server that keeps none.
pub const EFFORT_INHERIT: &str = "inherit";

impl Coworker {
    /// The effort its turns run with, in the server's word: `inherit` where the server keeps none.
    pub fn effort(&self) -> &str {
        self.effort.as_deref().unwrap_or(EFFORT_INHERIT)
    }

    /// Shared with the owner's organization, so the people in it use this bot too, and a bot
    /// used by others lets them read the skills attached to it (opengrok-server#270). Only the
    /// server's `org` says so: a row that does not say, or says a word this app does not know,
    /// is not claimed as shared.
    pub fn is_shared(&self) -> bool {
        self.visibility.as_deref() == Some("org")
    }
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
    /// One of [`EFFORT_WORDS`], sent only when the person changed it: absent leaves the stored
    /// effort alone, and `inherit` clears it (opengrok-server#271, which reads `null` the same
    /// way). The server refuses a word it does not know with a 400 and changes nothing, and
    /// refuses it on a coworker shared with the caller with a 403, as it does every change there
    /// but the sidebar flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
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
            && self.effort.is_none()
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
    /// Which door serves it, `gateway` or `local_proxy` (the inference-source contract agreed
    /// with open-ai-gateway and opengrok-server, 2026-09-30, built in opengrok-server #294:
    /// `list_models` in `crates/opengrok-server/src/agui/routes.rs`, `listed` in
    /// `crates/opengrok-harness/src/local_proxy.rs`): a `local_proxy` entry is one of
    /// opencodex's models, as the server lists them for the person's own subscription. Kept as
    /// the word the server sent and read through [`Self::source`].
    #[serde(default)]
    pub source: Option<String>,
}

impl ModelEntry {
    /// The door this model is served through. A server from before reply sources sends no
    /// `source`, and every model it lists is the gateway's. A word this app has not heard of is
    /// `None`, and such a model is offered by neither picker rather than guessed into one.
    pub fn source(&self) -> Option<super::InferenceKind> {
        match self.source.as_deref() {
            None => Some(super::InferenceKind::Gateway),
            Some(word) => super::InferenceKind::from_word(word),
        }
    }

    /// One of the person's own plan's models, which Settings → Reply source offers and a Bot's
    /// Model field does not.
    pub fn is_local_proxy(&self) -> bool {
        self.source() == Some(super::InferenceKind::LocalProxy)
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModelCatalogue {
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    #[serde(default)]
    pub note: Option<String>,
    /// Whether opencodex answered the server as it listed these, `"localProxy": {"healthy"}`:
    /// there whenever the account keeps a proxy address, whatever its reply source, and absent
    /// while it keeps none (the inference-source contract agreed with opengrok-server
    /// 2026-09-30, built in its #294: `list_models` in
    /// `crates/opengrok-server/src/agui/routes.rs`, `listed` in
    /// `crates/opengrok-harness/src/local_proxy.rs`). A proxy that is down lists nothing, and
    /// this is how that reads apart from a plan with nothing to offer. One this app cannot read
    /// is none: the list the Bot's Model field is drawn from never fails for it.
    #[serde(
        default,
        rename = "localProxy",
        deserialize_with = "proxy_status_or_none"
    )]
    pub local_proxy: Option<LocalProxyStatus>,
}

/// `GET /models`' word on the person's own proxy (see [`ModelCatalogue::local_proxy`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct LocalProxyStatus {
    /// opencodex answered its `/healthz` when the server asked.
    pub healthy: bool,
}

fn proxy_status_or_none<'de, D>(deserializer: D) -> Result<Option<LocalProxyStatus>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw.and_then(|raw| serde_json::from_value(raw).ok()))
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

/// What a refusal's JSON body says: the sentence the person is shown, and the server's code word
/// for the refusal, which is what a caller branches on. Either may be missing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RefusalWords {
    pub(crate) sentence: Option<String>,
    pub(crate) code: Option<String>,
}

/// Reads both of a refusal's fields, by the rule opengrok-server writes them to (its
/// error-bodies change):
///
/// - with a `code`, the code is `code` and the sentence is `error`: `{"error": "this run id
///   already has a run; a new turn needs a new run id", "code": "run-exists"}`;
/// - without one, a bare code word under `error` with a `message` beside it is the code, and the
///   `message` is the sentence: the queue's 409s, which keep that shape (`agui/pending.rs`
///   `stale-pending-message`);
/// - otherwise `error` is the sentence, and there is no code.
///
/// One case the rule leaves open is read so the queue still works: a bare code word with nothing
/// beside it (the queue's `already-consumed` and `not-pending`, which carry no `message`) is kept
/// as the code, and is all there is to show. A body with nothing under `error` gives no sentence,
/// and is shown as its own text. Shown `run-exists`, a person is shown the server's name for the
/// problem rather than the problem, which is why the sentence is read wherever it sits.
pub(crate) fn refusal_words(body: &serde_json::Value) -> RefusalWords {
    let field = |key: &str| {
        body.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    let error = field("error");
    let message = field("message");
    if let Some(code) = field("code") {
        return RefusalWords {
            sentence: error.or(message).map(str::to_string),
            code: Some(code.to_string()),
        };
    }
    match error {
        Some(code) if is_error_code(code) => RefusalWords {
            sentence: Some(message.unwrap_or(code).to_string()),
            code: Some(code.to_string()),
        },
        error => RefusalWords {
            sentence: error.map(str::to_string),
            code: None,
        },
    }
}

/// What a refusal says to the person: the sentence of a JSON body ([`refusal_words`]), or a text
/// body's own text.
pub fn error_message_from_body(body: &str) -> String {
    if let Some(sentence) = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|body| refusal_words(&body).sentence)
    {
        return sentence;
    }
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "request failed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The server's code word for a refusal, when its body names one ([`refusal_words`]).
pub(crate) fn error_code_from_body(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|body| refusal_words(&body).code)
}

/// Whether OpenGrok wrote this refusal itself, which it says by its shape: a JSON object with its
/// sentence under `error`. Nothing standing in front of the server writes that (a proxy answers
/// with its own page, or with nothing), so it is how a `502` the server wrote about a box that is
/// down is told from a `502` that means the server itself could not be reached.
pub(crate) fn written_by_opengrok(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|body| {
            body.get("error")
                .and_then(serde_json::Value::as_str)
                .map(|error| !error.trim().is_empty())
        })
        .unwrap_or(false)
}

/// A code word rather than a sentence: one lowercase token of letters and digits joined by `-`
/// or `_`, with no spaces (`run-exists`, `stale-pending-message`, `shared-computer`).
fn is_error_code(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_lowercase())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key a coworker patch can carry is one the server's patch route reads
    /// (opengrok-server `agui/routes.rs`: name, model, role, visibility, hiddenFromSidebar, and
    /// title/avatarShape/avatarColor; `effort` from opengrok-server#271). A key it reads nowhere
    /// is a setting that looks saved and is not, and a patch of only that is refused.
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
            effort: Some("high".into()),
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
            "effort",
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
        assert_eq!(keys.len(), 8, "every field is on the wire: {keys:?}");
    }

    /// A roster from a server before opengrok-server#271 has no `effort` on its rows, and every
    /// row still parses: it is a server that keeps none, so its bots run on `inherit`. A word
    /// this app has not heard of is kept as the server sent it rather than failing the roster,
    /// or being read as another word the server does not hold.
    #[test]
    fn a_rows_effort_reads_as_sent_and_a_missing_one_as_inherit() {
        let roster: Vec<Coworker> = serde_json::from_value(serde_json::json!([
            {"id": "cw_old", "name": "Old", "model": "oag/cheap"},
            {"id": "cw_new", "name": "New", "model": "oag/cheap", "effort": "ultra"},
            {"id": "cw_set", "name": "Set", "model": "oag/cheap", "effort": "xhigh"},
            {"id": "cw_unset", "name": "Unset", "model": "oag/cheap", "effort": "inherit"}
        ]))
        .expect("one row's effort never fails the roster");
        let efforts: Vec<(Option<&str>, &str)> = roster
            .iter()
            .map(|coworker| (coworker.effort.as_deref(), coworker.effort()))
            .collect();
        assert_eq!(
            efforts,
            vec![
                (None, "inherit"),
                (Some("ultra"), "ultra"),
                (Some("xhigh"), "xhigh"),
                (Some("inherit"), "inherit"),
            ]
        );
    }

    /// The effort rides a patch only when it is set, so a Save that did not touch it leaves the
    /// body a server before opengrok-server#271 already reads.
    #[test]
    fn a_patch_without_an_effort_leaves_the_key_off() {
        let patch = CoworkerPatch {
            name: Some("Bob".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            serde_json::json!({"name": "Bob"})
        );
        let effort = CoworkerPatch {
            effort: Some("inherit".into()),
            ..Default::default()
        };
        assert!(!effort.is_empty(), "an effort alone is a change to send");
        assert_eq!(
            serde_json::to_value(&effort).unwrap(),
            serde_json::json!({"effort": "inherit"})
        );
    }

    /// A `/models` entry says which door serves it. A server from before reply sources sends
    /// none, and its models are the gateway's; a word this app has not heard of names neither
    /// door, and one entry's word never fails the list.
    #[test]
    fn a_models_source_reads_as_sent_and_a_missing_one_as_the_gateway() {
        use super::super::InferenceKind;
        let catalogue: ModelCatalogue = serde_json::from_value(serde_json::json!({
            "models": [
                {"id": "oag/cheap"},
                {"id": "oag/fast", "source": "gateway", "points": null},
                {"id": "gpt-5-codex", "source": "local_proxy"},
                {"id": "odd", "source": "byok"},
                {"id": "nulled", "source": null}
            ],
            "note": null
        }))
        .expect("one entry's source never fails the list");
        let sources: Vec<(&str, Option<InferenceKind>)> = catalogue
            .models
            .iter()
            .map(|entry| (entry.id.as_str(), entry.source()))
            .collect();
        assert_eq!(
            sources,
            vec![
                ("oag/cheap", Some(InferenceKind::Gateway)),
                ("oag/fast", Some(InferenceKind::Gateway)),
                ("gpt-5-codex", Some(InferenceKind::LocalProxy)),
                ("odd", None),
                ("nulled", Some(InferenceKind::Gateway)),
            ]
        );
    }

    /// `localProxy` says whether opencodex answered as the server listed its models: read when
    /// there, none when the account keeps no proxy address, and none, not a failed list, when it
    /// is in a shape this app cannot read.
    #[test]
    fn a_models_list_says_whether_the_proxy_answered() {
        let read = |body: serde_json::Value| -> ModelCatalogue {
            serde_json::from_value(body).expect("the list is read whatever localProxy says")
        };
        let up = read(serde_json::json!({
            "models": [{"id": "gpt-5-codex", "source": "local_proxy", "points": null}],
            "note": null,
            "localProxy": {"healthy": true}
        }));
        assert_eq!(up.local_proxy, Some(LocalProxyStatus { healthy: true }));
        assert!(up.models[0].is_local_proxy());
        let down = read(serde_json::json!({
            "models": [{"id": "oag/cheap", "source": "gateway", "points": null}],
            "note": null,
            "localProxy": {"healthy": false}
        }));
        assert_eq!(down.local_proxy, Some(LocalProxyStatus { healthy: false }));
        assert!(!down.models[0].is_local_proxy());
        let none = read(serde_json::json!({"models": [], "note": null}));
        assert_eq!(none.local_proxy, None);
        // `?source=local_proxy` as the server builds it: the plan's models, `note: null`.
        let plan_only = read(serde_json::json!({
            "models": [{"id": "grok-4", "points": null, "source": "local_proxy"}],
            "note": null,
            "localProxy": {"healthy": true}
        }));
        assert_eq!(plan_only.note, None);
        assert!(plan_only.models[0].is_local_proxy());
        for odd in [
            serde_json::json!(null),
            serde_json::json!({}),
            serde_json::json!({"healthy": "yes"}),
            serde_json::json!(true),
        ] {
            let odd = read(serde_json::json!({
                "models": [{"id": "oag/cheap"}], "note": null, "localProxy": odd
            }));
            assert_eq!(odd.local_proxy, None);
            assert_eq!(odd.models.len(), 1);
        }
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

    /// A refusal with a code and a sentence is shown as the sentence, with the code kept apart,
    /// in the shape the server sends for a run that already exists (the sentence under `error`,
    /// the code under `code`, as
    /// `fixtures/wire/rest/POST__ag-ui/409-another_account_cannot_take_a_run_by_its_id.json`
    /// records it since opengrok-server's error-bodies change) and in the one it sent before (the
    /// code under `error`, the sentence under `message`), which the queue's 409s keep. `run-exists` used to reach the person as it was.
    #[test]
    fn a_code_beside_a_sentence_is_shown_as_the_sentence() {
        let said = "this run id already has a run; a new turn needs a new run id";
        let taken = r#"{"error": "run-exists", "message": "this run id already has a run; a new turn needs a new run id"}"#;
        let moved = r#"{"error": "this run id already has a run; a new turn needs a new run id", "code": "run-exists"}"#;
        for body in [taken, moved] {
            assert_eq!(error_message_from_body(body), said, "{body}");
            assert_eq!(
                error_code_from_body(body).as_deref(),
                Some("run-exists"),
                "{body}"
            );
            assert!(written_by_opengrok(body), "{body}");
        }

        let stale = r#"{"v": 1, "error": "stale-pending-message", "id": "pum_1", "message": "This queued message changed. Refresh it before sending again."}"#;
        assert_eq!(
            error_message_from_body(stale),
            "This queued message changed. Refresh it before sending again."
        );
        assert_eq!(
            error_code_from_body(stale).as_deref(),
            Some("stale-pending-message")
        );

        // A code with no sentence beside it is all the body says.
        let consumed = r#"{"v": 1, "error": "already-consumed", "id": "pum_1"}"#;
        assert_eq!(error_message_from_body(consumed), "already-consumed");
        assert_eq!(
            error_code_from_body(consumed).as_deref(),
            Some("already-consumed")
        );
        for alone in [
            r#"{"error": "run-exists", "message": ""}"#,
            r#"{"error": "run-exists", "message": "   "}"#,
            r#"{"error": "run-exists", "message": 7}"#,
        ] {
            assert_eq!(error_message_from_body(alone), "run-exists", "{alone}");
        }

        // A sentence under `error` is the sentence, whatever else the body carries, and names no
        // code.
        let sentence = r#"{"error": "form entry missing", "message": "something else"}"#;
        assert_eq!(error_message_from_body(sentence), "form entry missing");
        assert_eq!(error_code_from_body(sentence), None);
        let capital = r#"{"error": "Wrong email or password."}"#;
        assert_eq!(error_message_from_body(capital), "Wrong email or password.");
        assert_eq!(error_code_from_body(capital), None);

        // Whatever sits under `message` beside a bare code is its sentence.
        assert_eq!(
            error_message_from_body(r#"{"error": "run-exists", "message": "retry"}"#),
            "retry"
        );
        // Nothing under `error` is nothing the rule reads: the body is its own text, and
        // nothing says OpenGrok wrote it, which is a proxy's shape as much as the server's.
        let bare = r#"{"message": "Internal server error"}"#;
        assert_eq!(error_message_from_body(bare), bare);
        assert_eq!(error_code_from_body(bare), None);
        assert!(!written_by_opengrok(bare));
        let coded = r#"{"code": "run-exists"}"#;
        assert_eq!(error_message_from_body(coded), coded);
        assert_eq!(error_code_from_body(coded).as_deref(), Some("run-exists"));

        // A body that is not JSON, or has no string to say, is its own text.
        assert_eq!(error_message_from_body("  no such run \n"), "no such run");
        assert_eq!(error_code_from_body("no-such-run"), None);
        assert!(!written_by_opengrok("the box is unreachable"));
        assert!(!written_by_opengrok(
            "<html><body>502 Bad Gateway</body></html>"
        ));
        assert!(!written_by_opengrok(""));
        assert_eq!(
            error_message_from_body(r#"{"error": {"message": "nested"}}"#),
            r#"{"error": {"message": "nested"}}"#
        );
        assert_eq!(error_message_from_body(""), "request failed");
    }
}
