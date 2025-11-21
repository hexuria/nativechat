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

impl Message {
    /// Format the message timestamp into a human-readable string
    pub fn formatted_time(&self) -> String {
        let now = SystemTime::now();
        let duration = now.duration_since(self.sent_at).unwrap_or_default();

        let secs = duration.as_secs();

        if secs < 60 {
            "Just now".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
        } else if secs < 86400 {
            // Today - show time only
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            format!("{:02}:{:02}", hours, mins)
        } else if secs < 172800 {
            // Yesterday
            let hours = (secs % 86400) / 3600;
            let mins = (secs % 3600) / 60;
            format!("Yesterday {:02}:{:02}", hours, mins)
        } else {
            // Older messages - show date
            let days = secs / 86400;
            if days < 365 {
                format!("{} days ago", days)
            } else {
                let years = days / 365;
                format!("{} year{} ago", years, if years == 1 { "" } else { "s" })
            }
        }
    }
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
    pub theme_mode: String,
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

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        use gpui_component::{Theme, ThemeRegistry};

        println!("[THEME] Toggle called, current: {}", self.theme_mode);

        // Cycle: light → dark → system → light
        self.theme_mode = match self.theme_mode.as_str() {
            "light" => "dark",
            "dark" => "system",
            _ => "light",
        }
        .to_string();

        println!("[THEME] New mode: {}", self.theme_mode);

        // Determine which theme to apply
        let theme_name = match self.theme_mode.as_str() {
            "light" => "macOS Classic Light",
            "dark" => "macOS Classic Dark",
            "system" => {
                // TODO: Detect actual system appearance
                // For now, default to dark
                println!("[THEME] System mode - defaulting to dark");
                "macOS Classic Dark"
            }
            _ => "macOS Classic Light",
        };

        println!("[THEME] Loading theme: {}", theme_name);
        println!(
            "[THEME] Available themes: {:?}",
            ThemeRegistry::global(cx)
                .themes()
                .keys()
                .collect::<Vec<_>>()
        );

        if let Some(theme) = ThemeRegistry::global(cx)
            .themes()
            .get(&SharedString::from(theme_name))
            .cloned()
        {
            println!("[THEME] Found theme, applying...");
            Theme::global_mut(cx).apply_config(&theme);
            println!("[THEME] Applied!");
        } else {
            println!("[THEME] ERROR: Theme not found!");
        }

        cx.notify();
    }
}
