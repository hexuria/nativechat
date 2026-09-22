//! Durable pending user messages — OpenGrok's follow-up queue, per thread.
//!
//! Contract: hexuria/opengrok-server#171. Payload `v: 1`. NativeChat's process
//! queue (`queued_sends`) is still what drain posts; these routes are how that
//! queue survives another machine, and how cancel/edit reach it.
//!
//! Mutations answer with AG-UI CUSTOM `name: pending-user-message`. GET list
//! and GET thread carry the same events as `pendingEvents` (`op: snapshot`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Writes that name another number are refused unread. Missing `v` is v1.
pub const PAYLOAD_V: u32 = 1;

/// CUSTOM `name` on mutation and snapshot events.
pub const CUSTOM_NAME: &str = "pending-user-message";

/// One follow-up as `GET /ag-ui/threads/{id}` and `GET …/pending` return it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingUserMessage {
    #[serde(default)]
    pub v: u32,
    pub id: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reply_to: Option<Value>,
    #[serde(default)]
    pub recipe_id: Option<String>,
    #[serde(default)]
    pub recipe_values: Option<Value>,
    #[serde(default)]
    pub skill_id: Option<String>,
    #[serde(default)]
    pub client_message_id: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub drained_at_ms: Option<i64>,
    #[serde(default)]
    pub drained_run_id: Option<String>,
}

impl PendingUserMessage {
    /// The bubble this row is about. NativeChat's id is `clientMessageId`; a
    /// row minted elsewhere with none of that uses the server id so hydrate
    /// still has a stable name.
    pub fn bubble_id(&self) -> &str {
        self.client_message_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(self.id.as_str())
    }
}

/// What a `pending-user-message` CUSTOM did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOp {
    Created,
    Edited,
    Canceled,
    Drained,
    Snapshot,
}

impl PendingOp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Edited => "edited",
            Self::Canceled => "canceled",
            Self::Drained => "drained",
            Self::Snapshot => "snapshot",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "created" => Some(Self::Created),
            "edited" => Some(Self::Edited),
            "canceled" => Some(Self::Canceled),
            "drained" => Some(Self::Drained),
            "snapshot" => Some(Self::Snapshot),
            _ => None,
        }
    }
}

/// AG-UI CUSTOM `pending-user-message`, `v: 1`.
///
/// `canceled` omits `message` — the path (and the bubble id the client already
/// has) is enough to drop the hold, and the text is not sent back.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingCustom {
    pub timestamp: Option<i64>,
    pub v: u32,
    pub op: PendingOp,
    pub thread_id: String,
    pub message: Option<PendingUserMessage>,
}

impl PendingCustom {
    /// `None` when this is not our CUSTOM, or when `v` names another number.
    pub fn from_agui(event: &Value) -> Option<Self> {
        if event.get("type").and_then(Value::as_str) != Some("CUSTOM") {
            return None;
        }
        if event.get("name").and_then(Value::as_str) != Some(CUSTOM_NAME) {
            return None;
        }
        let value = event.get("value")?;
        let v = value
            .get("v")
            .and_then(Value::as_u64)
            .unwrap_or(PAYLOAD_V as u64);
        if v != PAYLOAD_V as u64 {
            return None;
        }
        let op = PendingOp::parse(value.get("op").and_then(Value::as_str)?)?;
        let thread_id = value
            .get("threadId")
            .or_else(|| value.get("thread_id"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let message = value.get("message").and_then(|message| {
            if message.is_null() {
                None
            } else {
                serde_json::from_value(message.clone()).ok()
            }
        });
        Some(Self {
            timestamp: event.get("timestamp").and_then(Value::as_i64),
            v: PAYLOAD_V,
            op,
            thread_id,
            message,
        })
    }

    /// Live rows from GET `pendingEvents` (`op: snapshot`, and `created` if a
    /// mutation event snuck onto the list).
    pub fn snapshot_messages(events: &[Value]) -> Vec<PendingUserMessage> {
        events
            .iter()
            .filter_map(Self::from_agui)
            .filter(|custom| matches!(custom.op, PendingOp::Snapshot | PendingOp::Created))
            .filter_map(|custom| custom.message)
            .collect()
    }
}

/// `GET /ag-ui/threads/{threadId}/pending`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PendingList {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub pending_user_messages: Vec<PendingUserMessage>,
    /// Snapshot CUSTOMs. `None` when the server has never heard of the field;
    /// `Some` (even empty) is the live queue.
    #[serde(default)]
    pub pending_events: Option<Vec<Value>>,
}

impl PendingList {
    /// Prefer `pendingEvents` snapshots when the field is present; otherwise
    /// the `pendingUserMessages` rows.
    pub fn live_messages(&self) -> Vec<PendingUserMessage> {
        match &self.pending_events {
            Some(events) => PendingCustom::snapshot_messages(events),
            None => self.pending_user_messages.clone(),
        }
    }
}

/// POST create / PATCH edit. Omitted fields stay on PATCH; JSON `null` is not
/// sent from this client (NativeChat edits content, not reply/recipe/skill).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingWrite {
    pub v: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe_values: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
}

impl PendingWrite {
    pub fn enqueue(
        content: String,
        client_message_id: String,
        reply_to: Option<Value>,
        recipe_id: Option<String>,
        recipe_values: Option<Value>,
        skill_id: Option<String>,
    ) -> Self {
        Self {
            v: PAYLOAD_V,
            content: Some(content),
            client_message_id: Some(client_message_id).filter(|id| !id.is_empty()),
            reply_to,
            recipe_id: recipe_id.filter(|id| !id.is_empty()),
            recipe_values,
            skill_id: skill_id.filter(|id| !id.is_empty()),
        }
    }

    pub fn content_patch(content: String) -> Self {
        Self {
            v: PAYLOAD_V,
            content: Some(content),
            client_message_id: None,
            reply_to: None,
            recipe_id: None,
            recipe_values: None,
            skill_id: None,
        }
    }
}

/// POST/PATCH/DELETE answer: the row (except on cancel) plus the CUSTOM event
/// NativeChat applies to `queued_sends`.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PendingMutation {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub pending_user_message: Option<PendingUserMessage>,
    #[serde(default)]
    pub event: Option<Value>,
}

impl PendingMutation {
    pub fn custom(&self) -> Option<PendingCustom> {
        self.event.as_ref().and_then(PendingCustom::from_agui)
    }

    /// The row, or the CUSTOM's `message` when the sibling field was omitted.
    pub fn row(&self) -> Option<PendingUserMessage> {
        self.pending_user_message
            .clone()
            .or_else(|| self.custom().and_then(|custom| custom.message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn custom_event(op: &str, message: Option<Value>) -> Value {
        let mut value = json!({
            "v": 1,
            "op": op,
            "threadId": "th_1",
        });
        if let Some(message) = message {
            value["message"] = message;
        }
        json!({
            "type": "CUSTOM",
            "name": CUSTOM_NAME,
            "timestamp": 1710000000000i64,
            "value": value,
        })
    }

    #[test]
    fn a_pending_row_reads_camel_case_and_names_its_bubble() {
        let row: PendingUserMessage = serde_json::from_value(json!({
            "v": 1,
            "id": "pum_1",
            "threadId": "th_1",
            "content": "send this after the turn",
            "replyTo": { "messageId": "m1", "preview": "hi" },
            "recipeId": "rec_1",
            "recipeValues": { "q": "later" },
            "skillId": "skl_1",
            "clientMessageId": "msg_bubble_1",
            "status": "pending",
            "createdAtMs": 10,
            "updatedAtMs": 20,
            "drainedAtMs": null,
            "drainedRunId": null
        }))
        .expect("row");
        assert_eq!(row.v, PAYLOAD_V);
        assert_eq!(row.thread_id, "th_1");
        assert_eq!(row.bubble_id(), "msg_bubble_1");
        assert_eq!(row.reply_to.unwrap()["messageId"], "m1");
        assert_eq!(row.recipe_id.as_deref(), Some("rec_1"));
        assert_eq!(row.skill_id.as_deref(), Some("skl_1"));
    }

    #[test]
    fn a_row_without_a_client_id_is_named_by_the_server_id() {
        let row: PendingUserMessage = serde_json::from_value(json!({
            "id": "pum_x",
            "content": "later",
        }))
        .expect("row");
        assert_eq!(row.bubble_id(), "pum_x");
    }

    #[test]
    fn enqueue_write_is_v1_and_omits_empty_optionals() {
        let body = serde_json::to_value(PendingWrite::enqueue(
            "later".into(),
            "msg_1".into(),
            None,
            None,
            None,
            None,
        ))
        .expect("json");
        assert_eq!(body["v"], PAYLOAD_V);
        assert_eq!(body["content"], "later");
        assert_eq!(body["clientMessageId"], "msg_1");
        assert!(body.get("replyTo").is_none(), "{body}");
        assert!(body.get("recipeId").is_none(), "{body}");
        assert!(body.get("skillId").is_none(), "{body}");
    }

    #[test]
    fn a_content_patch_does_not_clear_the_other_fields() {
        let body =
            serde_json::to_value(PendingWrite::content_patch("instead".into())).expect("json");
        assert_eq!(body, json!({ "v": 1, "content": "instead" }));
    }

    #[test]
    fn pending_user_message_customs_are_v1_and_name_every_op() {
        let row = json!({
            "v": 1,
            "id": "pum_1",
            "threadId": "th_1",
            "content": "later",
            "clientMessageId": "msg_1",
            "status": "pending",
        });
        for op in ["created", "edited", "drained", "snapshot"] {
            let custom = PendingCustom::from_agui(&custom_event(op, Some(row.clone()))).expect(op);
            assert_eq!(custom.v, PAYLOAD_V);
            assert_eq!(custom.op.as_str(), op);
            assert_eq!(custom.thread_id, "th_1");
            assert_eq!(custom.message.as_ref().unwrap().bubble_id(), "msg_1");
        }
        let canceled = PendingCustom::from_agui(&custom_event("canceled", None)).expect("canceled");
        assert_eq!(canceled.op, PendingOp::Canceled);
        assert!(canceled.message.is_none(), "canceled omits the words");
    }

    #[test]
    fn a_custom_for_another_name_or_version_is_not_ours() {
        let mut event = custom_event("created", None);
        event["name"] = json!("user-form");
        assert!(PendingCustom::from_agui(&event).is_none());

        let mut event = custom_event("created", None);
        event["value"]["v"] = json!(2);
        assert!(PendingCustom::from_agui(&event).is_none());

        assert!(PendingCustom::from_agui(&json!({ "type": "TEXT_MESSAGE_START" })).is_none());
    }

    #[test]
    fn a_list_prefers_snapshot_events_when_the_field_is_present() {
        let row = json!({
            "id": "pum_1",
            "content": "from the event",
            "clientMessageId": "msg_1",
        });
        let list: PendingList = serde_json::from_value(json!({
            "v": 1,
            "threadId": "th_1",
            "pendingUserMessages": [{
                "id": "pum_stale",
                "content": "from the row list",
                "clientMessageId": "msg_stale"
            }],
            "pendingEvents": [custom_event("snapshot", Some(row))]
        }))
        .expect("list");
        let live = list.live_messages();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].content, "from the event");
        assert_eq!(live[0].bubble_id(), "msg_1");
    }

    #[test]
    fn an_empty_pending_events_array_is_none_held() {
        let list: PendingList = serde_json::from_value(json!({
            "v": 1,
            "threadId": "th_1",
            "pendingUserMessages": [{
                "id": "pum_stale",
                "content": "should not be used",
                "clientMessageId": "msg_stale"
            }],
            "pendingEvents": []
        }))
        .expect("list");
        assert!(list.live_messages().is_empty());
    }

    #[test]
    fn a_list_without_events_uses_the_row_array() {
        let list: PendingList = serde_json::from_value(json!({
            "v": 1,
            "pendingUserMessages": [{
                "id": "pum_1",
                "content": "later",
                "clientMessageId": "msg_1"
            }]
        }))
        .expect("list");
        assert!(list.pending_events.is_none());
        assert_eq!(list.live_messages()[0].bubble_id(), "msg_1");
    }

    #[test]
    fn a_mutation_exposes_the_custom_and_the_row() {
        let mutation: PendingMutation = serde_json::from_value(json!({
            "v": 1,
            "threadId": "th_1",
            "pendingUserMessage": {
                "id": "pum_1",
                "content": "later",
                "clientMessageId": "msg_1"
            },
            "event": custom_event("created", Some(json!({
                "id": "pum_1",
                "content": "later",
                "clientMessageId": "msg_1"
            })))
        }))
        .expect("mutation");
        assert_eq!(mutation.custom().unwrap().op, PendingOp::Created);
        assert_eq!(mutation.row().unwrap().id, "pum_1");
    }
}
