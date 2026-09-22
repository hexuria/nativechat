//! Durable pending user messages — OpenGrok's follow-up queue, per thread.
//!
//! Contract: hexuria/opengrok-server#171. Payload `v: 1`. NativeChat's process
//! queue (`queued_sends`) is still what drain posts; these routes are how that
//! queue survives another machine, and how cancel/edit reach it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Writes that name another number are refused unread. Missing `v` is v1.
pub const PAYLOAD_V: u32 = 1;

/// CUSTOM `name` on mutation and snapshot events. NativeChat hydrates from
/// `pendingUserMessages` rather than walking these, but the name is the contract.
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

/// POST/PATCH answer: the row plus a CUSTOM event NativeChat does not need to
/// decode when it already has `pendingUserMessage`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingMutation {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub pending_user_message: Option<PendingUserMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
