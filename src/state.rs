use crate::audio::AudioInput;

use crate::services::gemini_client::GeminiLiveClient;
use gpui::*;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
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

#[derive(Clone, Debug, PartialEq)]
pub enum VoiceStatus {
    Ready,
    Connecting,
    Connected,
    Disconnected,
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: usize,
    pub name: String,
    pub avatar: Option<String>, // Path or IconName
}

pub struct AppState {
    pub conversations: Vec<Conversation>,
    pub active_conversation_id: Option<usize>,
    pub theme_mode: String,
    pub amplitude: Arc<AtomicU32>,
    pub ai_amplitude: Arc<AtomicU32>, // New field for AI voice viz
    pub is_voice_mode_open: bool,
    pub is_sidebar_open: bool,
    pub is_voice_muted: bool,
    pub voice_status: VoiceStatus,
    pub more_menu_open: bool,
    pub is_account_settings_open: bool,
    pub is_profile_settings_open: bool,
    pub audio_input: Option<AudioInput>,
    pub gemini_client: Option<GeminiLiveClient>,
    pub profiles: Vec<Profile>,
    pub selected_profile: Option<Profile>,
    pub available_apps: Vec<String>,
    pub selected_apps: Vec<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let profiles = vec![
            Profile {
                id: 1,
                name: "John Doe".to_string(),
                avatar: None,
            },
            Profile {
                id: 2,
                name: "Jane Smith".to_string(),
                avatar: None,
            },
        ];
        let selected_profile = profiles.first().cloned();

        let available_apps = vec![
            "Canva".to_string(),
            "Figma".to_string(),
            "Notion".to_string(),
            "Linear".to_string(),
        ];

        Self {
            conversations: vec![
                Conversation {
                    id: 1,
                    title: "John Doe".to_string(),
                    messages: vec![
                        Message {
                            id: 1,
                            sender: "John Doe".to_string(),
                            content: "Hello there!".to_string(),
                            sent_at: std::time::SystemTime::now(),
                            is_me: false,
                        },
                        Message {
                            id: 2,
                            sender: "Me".to_string(),
                            content: "Hi John!".to_string(),
                            sent_at: std::time::SystemTime::now(),
                            is_me: true,
                        },
                    ],
                    unread_count: 0,
                },
                Conversation {
                    id: 2,
                    title: "Jane Smith".to_string(),
                    messages: vec![Message {
                        id: 1,
                        sender: "Jane Smith".to_string(),
                        content: "Meeting at 3?".to_string(),
                        sent_at: std::time::SystemTime::now(),
                        is_me: false,
                    }],
                    unread_count: 1,
                },
            ],
            active_conversation_id: Some(1),
            theme_mode: "light".to_string(),
            amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            ai_amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            is_voice_mode_open: false,
            is_voice_muted: false,
            is_sidebar_open: true,
            voice_status: VoiceStatus::Ready,
            more_menu_open: false,
            is_account_settings_open: false,
            is_profile_settings_open: false,
            audio_input: None,
            gemini_client: None,
            profiles,
            selected_profile,
            available_apps,
            selected_apps: Vec::new(),
        }
    }

    pub fn select_conversation(&mut self, conversation_id: usize, cx: &mut Context<Self>) {
        self.active_conversation_id = Some(conversation_id);
        cx.notify();
    }

    pub fn select_profile(&mut self, profile_id: usize, cx: &mut Context<Self>) {
        if let Some(profile) = self.profiles.iter().find(|p| p.id == profile_id) {
            self.selected_profile = Some(profile.clone());
            cx.notify();
        }
    }

    pub fn select_app(&mut self, app_name: String, cx: &mut Context<Self>) {
        if !self.selected_apps.contains(&app_name) {
            self.selected_apps.push(app_name);
            cx.notify();
        }
    }

    pub fn remove_app(&mut self, app_name: String, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_apps.iter().position(|a| *a == app_name) {
            self.selected_apps.remove(index);
            cx.notify();
        }
    }

    pub fn send_message(&mut self, content: String, cx: &mut Context<Self>) {
        if let Some(conversation_id) = self.active_conversation_id
            && let Some(conversation) = self
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

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.is_sidebar_open = !self.is_sidebar_open;
        cx.notify();
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

    pub fn set_voice_mode(&mut self, open: bool, cx: &mut Context<Self>) {
        self.is_voice_mode_open = open;
        if !open {
            self.stop_voice_mode(cx);
        }
        cx.notify();
    }

    pub fn toggle_voice_mute(&mut self, cx: &mut Context<Self>) {
        self.is_voice_muted = !self.is_voice_muted;
        cx.notify();
    }

    pub fn toggle_account_settings(&mut self, cx: &mut Context<Self>) {
        self.is_account_settings_open = !self.is_account_settings_open;
        cx.notify();
    }

    pub fn toggle_profile_settings(&mut self, cx: &mut Context<Self>) {
        self.is_profile_settings_open = !self.is_profile_settings_open;
        cx.notify();
    }

    pub fn start_voice_mode(&mut self, cx: &mut Context<Self>) {
        self.is_voice_mode_open = true;
        self.voice_status = VoiceStatus::Connecting;
        cx.notify();

        if self.gemini_client.is_some() {
            println!("Gemini client already connected, ignoring start request");
            self.voice_status = VoiceStatus::Connected;
            return;
        }

        let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            eprintln!("GEMINI_API_KEY not set");
            self.voice_status = VoiceStatus::Error("API Key Missing".to_string());
            // Still start audio input for local viz, but without Gemini
            match AudioInput::new(self.amplitude.clone(), None) {
                Ok(input) => self.audio_input = Some(input),
                Err(e) => eprintln!("Failed to start local audio input: {}", e),
            }
            return;
        }

        let ai_amplitude = self.ai_amplitude.clone();
        let amplitude = self.amplitude.clone();

        cx.spawn(|this: WeakEntity<AppState>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                println!("Connecting to Gemini...");
                match GeminiLiveClient::connect(api_key, ai_amplitude) {
                    Ok(client) => {
                        println!("Connected to Gemini!");
                        let _ = this.update(&mut cx, |state, _cx| {
                            state.gemini_client = Some(client.clone());
                            state.voice_status = VoiceStatus::Connected;

                            // Restart audio input with the connected client
                            match AudioInput::new(amplitude, Some(client)) {
                                Ok(input) => {
                                    state.audio_input = Some(input);
                                    println!("Audio input started with Gemini client");
                                }
                                Err(e) => {
                                    eprintln!("Failed to start audio input: {}", e);
                                    state.voice_status =
                                        VoiceStatus::Error("Mic Error".to_string());
                                }
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Failed to connect to Gemini: {}", e);
                        let _ = this.update(&mut cx, |state, _cx| {
                            state.voice_status =
                                VoiceStatus::Error("Connection Failed".to_string());
                            // Fallback to local audio if connection fails
                            match AudioInput::new(amplitude, None) {
                                Ok(input) => state.audio_input = Some(input),
                                Err(e) => eprintln!("Failed to start local audio input: {}", e),
                            }
                        });
                    }
                }
            }
        })
        .detach();
    }

    pub fn stop_voice_mode(&mut self, cx: &mut Context<Self>) {
        self.is_voice_mode_open = false;
        self.voice_status = VoiceStatus::Disconnected;

        if let Some(client) = &self.gemini_client {
            client.disconnect();
        }
        self.gemini_client = None;
        self.audio_input = None; // Drops AudioInput, stops capture

        cx.notify();
    }
}
