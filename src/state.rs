use gpui::*;
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Message {
    pub id: usize,
    pub sender: String,
    pub content: String,
    pub sent_at: SystemTime,
    pub is_me: bool,
}

#[derive(Clone, Debug)]
pub struct Conversation {
    pub id: usize,
    pub title: String,
    pub messages: Vec<Message>,
    pub unread_count: usize,
}

pub struct AppState {
    pub conversations: Vec<Conversation>,
    pub active_conversation_id: Option<usize>,
}

impl AppState {
    pub fn select_conversation(&mut self, conversation_id: usize, cx: &mut Context<Self>) {
        self.active_conversation_id = Some(conversation_id);
        cx.notify();
    }

    pub fn send_message(&mut self, content: String, cx: &mut Context<Self>) {
        if let Some(conversation_id) = self.active_conversation_id {
            if let Some(conversation) = self
                .conversations
                .iter_mut()
                .find(|c| c.id == conversation_id)
            {
                let message = Message {
                    id: conversation.messages.len() + 1,
                    sender: "Me".to_string(),
                    content,
                    sent_at: SystemTime::now(),
                    is_me: true,
                };
                conversation.messages.push(message);
                cx.notify();
            }
        }
    }
}
