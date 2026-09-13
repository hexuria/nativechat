use crate::actions::TtsSource;
use crate::audio::AudioInput;
use crate::config::Config;
use crate::chrome::{
    collapse_for_width, remember_choice, sidebar_from_resize, ResponsiveCollapse, SidebarChrome,
    SIDEBAR_EXPANDED,
};
use crate::opengrok::{
    activity_from_agui, Account, ActivityTick, AguiMessage, Coworker, CoworkerPatch, ModelCatalogue,
    OpenGrokClient, ProfileUpdate,
};
use crate::llm::{
    ChatMessage, ChatRequest, LlmProvider, create_provider, create_provider_from_credential,
};
use crate::services::database::{
    Credential as DbCredential, DatabaseService, Profile as DbProfile,
};
use crate::services::gemini_client::GeminiLiveClient;
use crate::services::model_registry::{ModelProfile, ModelRegistry, Provider};
use crate::services::tts_service::TtsService;
use chrono::NaiveDateTime;
use futures::StreamExt;
use gpui_kit::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
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
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub messages: Vec<Message>,
    pub unread_count: usize,
}

impl Conversation {
    pub fn relative_time(&self) -> String {
        let now = SystemTime::now();

        // Parse the ISO 8601 string or fallback to now
        let created_at = NaiveDateTime::parse_from_str(&self.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| SystemTime::from(dt.and_utc()))
            .unwrap_or(SystemTime::now());

        let duration = now.duration_since(created_at).unwrap_or_default();
        let secs = duration.as_secs();

        if secs < 60 {
            "Just now".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("{}m ago", mins)
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!("{}h ago", hours)
        } else if secs < 604800 {
            let days = secs / 86400;
            format!("{}d ago", days)
        } else if secs < 2592000 {
            let weeks = secs / 604800;
            format!("{}w ago", weeks)
        } else if secs < 31536000 {
            let months = secs / 2592000;
            format!("{}mo ago", months)
        } else {
            let years = secs / 31536000;
            format!("{}y ago", years)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VoiceStatus {
    Ready,
    Connecting,
    Connected,
    Disconnected,
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AuthStatus {
    #[default]
    SignedOut,
    SigningIn,
    SignedIn,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub id: usize,
    pub name: String,
    pub avatar: Option<String>, // Path or IconName
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppCapability {
    pub name: String,      // The tag name (e.g., "Photos")
    pub label: String,     // The menu label (e.g., "Add photos & files")
    pub icon: String,      // Icon path
    pub action_id: String, // Action identifier
    pub is_primary: bool,  // Whether it belongs in the main menu or "More" submenu
}

pub struct AppState {
    pub conversations: Vec<Conversation>,
    pub active_conversation_id: Option<String>,
    pub theme_mode: String,
    pub amplitude: Arc<AtomicU32>,
    pub ai_amplitude: Arc<AtomicU32>, // New field for AI voice viz
    pub is_ai_speaking: Arc<AtomicBool>,
    pub is_voice_mode_open: bool,
    pub is_sidebar_open: bool,
    pub is_voice_muted: bool,
    pub voice_status: VoiceStatus,
    pub more_menu_open: bool,
    pub is_account_settings_open: bool,
    pub is_profile_settings_open: bool,
    pub is_credentials_modal_open: bool,
    pub audio_input: Option<AudioInput>,
    pub gemini_client: Option<GeminiLiveClient>,
    pub profiles: Vec<Profile>,
    pub selected_profile: Option<Profile>,
    pub available_apps: Vec<String>,
    pub selected_apps: Vec<String>,
    pub capabilities: Vec<AppCapability>,
    pub sidebar_collapsed: bool,
    pub sidebar_hidden: bool,
    pub sidebar_expanded_width: f32,
    pub sidebar_responsive: ResponsiveCollapse,
    pub auto_collapsed: bool,
    pub model_registry: Arc<ModelRegistry>,
    pub available_models: Vec<ModelProfile>,
    pub database_service: Option<DatabaseService>,
    pub llm_provider: Option<Arc<dyn LlmProvider>>,
    pub config: Option<Config>,
    pub is_ai_responding: bool,
    // Database profile and credential fields
    pub db_profiles: Vec<DbProfile>,
    pub db_credentials: Vec<DbCredential>,
    pub active_profile_id: Option<i64>,
    // Debug mode for markdown rendering
    pub debug_markdown_disabled: bool,
    pub tts_service: Option<TtsService>,
    pub native_tts: SourceTtsState,
    pub ai_tts: SourceTtsState,
    pub opengrok: Option<OpenGrokClient>,
    pub account: Option<Account>,
    pub auth_status: AuthStatus,
    pub auth_error: Option<String>,
    login_epoch: u64,
    pub login_email: String,
    pub login_password: String,
    pub coworkers: Vec<Coworker>,
    pub active_coworker_id: Option<String>,
    pub bot_status: Option<String>,
    pub model_catalogue: ModelCatalogue,
    pub is_agent_settings_open: bool,
    pub model_picker_open: bool,
    pub avatar_editor_open: bool,
    pub hiring: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SourceTtsState {
    pub message_id: Option<String>,
    pub is_paused: bool,
    pub is_loading: bool,
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

        let capabilities = vec![
            // Primary Items
            AppCapability {
                name: "Photos".to_string(),
                label: "Add photos & files".to_string(),
                icon: "icons/clip.svg".to_string(),
                action_id: "SelectAppPhotos".to_string(),
                is_primary: true,
            },
            AppCapability {
                name: "Image Generation".to_string(),
                label: "Image Generation".to_string(),
                icon: "icons/create_image.svg".to_string(),
                action_id: "SelectAppImageGeneration".to_string(),
                is_primary: true,
            },
            AppCapability {
                name: "Thinking".to_string(),
                label: "Thinking".to_string(),
                icon: "icons/thinking.svg".to_string(),
                action_id: "SelectAppThinking".to_string(),
                is_primary: true,
            },
            AppCapability {
                name: "Deep Research".to_string(),
                label: "Deep Research".to_string(),
                icon: "icons/deep_search.svg".to_string(),
                action_id: "SelectAppDeepResearch".to_string(),
                is_primary: true,
            },
            AppCapability {
                name: "Study".to_string(),
                label: "Study".to_string(),
                icon: "icons/study.svg".to_string(),
                action_id: "SelectAppStudy".to_string(),
                is_primary: true,
            },
            // Secondary Items ("More" submenu)
            AppCapability {
                name: "Web search".to_string(),
                label: "Web search".to_string(),
                icon: "icons/web_search.svg".to_string(),
                action_id: "SelectAppWebSearch".to_string(),
                is_primary: false,
            },
            AppCapability {
                name: "Canvas".to_string(),
                label: "Canvas".to_string(),
                icon: "icons/canvas.svg".to_string(),
                action_id: "SelectAppCanvas".to_string(),
                is_primary: false,
            },
            AppCapability {
                name: "Canva".to_string(),
                label: "Canva".to_string(),
                icon: "icons/canva.svg".to_string(),
                action_id: "SelectAppCanva".to_string(),
                is_primary: false,
            },
            AppCapability {
                name: "Coursera".to_string(),
                label: "Coursera".to_string(),
                icon: "icons/coursera.svg".to_string(),
                action_id: "SelectAppCoursera".to_string(),
                is_primary: false,
            },
            AppCapability {
                name: "Figma".to_string(),
                label: "Figma".to_string(),
                icon: "icons/figma.svg".to_string(),
                action_id: "SelectAppFigma".to_string(),
                is_primary: false,
            },
            AppCapability {
                name: "Spotify".to_string(),
                label: "Spotify".to_string(),
                icon: "icons/spotify.svg".to_string(),
                action_id: "SelectAppSpotify".to_string(),
                is_primary: false,
            },
        ];

        let mut state = Self {
            conversations: Vec::new(),
            active_conversation_id: None,
            theme_mode: "light".to_string(),
            amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            ai_amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            is_ai_speaking: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            is_voice_mode_open: false,
            is_voice_muted: false,
            is_sidebar_open: true,
            voice_status: VoiceStatus::Ready,
            more_menu_open: false,
            is_account_settings_open: false,
            is_profile_settings_open: false,
            is_credentials_modal_open: false,
            audio_input: None,
            gemini_client: None,
            profiles,
            selected_profile,
            available_apps,
            selected_apps: Vec::new(),
            capabilities,
            sidebar_collapsed: false,
            sidebar_hidden: false,
            sidebar_expanded_width: SIDEBAR_EXPANDED,
            sidebar_responsive: ResponsiveCollapse::default(),
            auto_collapsed: false,
            model_registry: Arc::new(ModelRegistry::new()),
            available_models: Vec::new(),
            database_service: None,
            llm_provider: None,
            config: None,
            is_ai_responding: false,
            db_profiles: Vec::new(),
            db_credentials: Vec::new(),
            active_profile_id: None,
            debug_markdown_disabled: false,
            tts_service: None,
            native_tts: SourceTtsState::default(),
            ai_tts: SourceTtsState::default(),
            opengrok: None,
            account: None,
            auth_status: AuthStatus::SignedOut,
            auth_error: None,
            login_epoch: 0,
            login_email: String::new(),
            login_password: String::new(),
            coworkers: Vec::new(),
            active_coworker_id: None,
            bot_status: None,
            model_catalogue: ModelCatalogue::default(),
            is_agent_settings_open: false,
            model_picker_open: false,
            avatar_editor_open: false,
            hiring: false,
        };
        // Synchronously load cached state to avoid startup delay
        if let Some((cached_id, cached_profiles)) = Self::load_cached_state() {
            println!(
                "Loaded cached state: ID {:?}, {} profiles",
                cached_id,
                cached_profiles.len()
            );
            state.active_profile_id = cached_id;
            state.db_profiles = cached_profiles;
        }

        state
    }

    pub fn set_config(&mut self, config: Config, cx: &mut Context<Self>) {
        // Initialize LLM provider based on config
        match create_provider(&config) {
            Ok(provider) => {
                println!("[LLM] Initialized {} provider", config.default_provider);
                self.llm_provider = Some(Arc::from(provider));
            }
            Err(e) => {
                eprintln!("[LLM] Failed to create provider: {}", e);
            }
        }
        match OpenGrokClient::new(&config.opengrok_base_url) {
            Ok(client) => self.opengrok = Some(client),
            Err(error) => {
                self.auth_error = Some(error.message);
                self.opengrok = None;
            }
        }
        self.config = Some(config);
        cx.notify();
    }

    pub fn is_signed_in(&self) -> bool {
        self.auth_status == AuthStatus::SignedIn && self.account.is_some()
    }

    pub fn login(&mut self, email: String, password: String, cx: &mut Context<Self>) {
        if self.auth_status == AuthStatus::SigningIn {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        self.login_epoch += 1;
        let epoch = self.login_epoch;
        self.auth_status = AuthStatus::SigningIn;
        self.auth_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match client.login(&email, &password).await {
                Ok(()) => client.me().await,
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                if state.login_epoch != epoch {
                    return;
                }
                match result {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_status = AuthStatus::SignedIn;
                        state.auth_error = None;
                        state.refresh_coworkers(cx);
                    }
                    Err(error) => {
                        state.account = None;
                        state.auth_status = AuthStatus::SignedOut;
                        state.auth_error = Some(error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn logout(&mut self, cx: &mut Context<Self>) {
        let client = self.opengrok.clone();
        self.account = None;
        self.auth_status = AuthStatus::SignedOut;
        self.auth_error = None;
        self.coworkers.clear();
        self.active_coworker_id = None;
        self.bot_status = None;
        self.is_account_settings_open = false;
        self.is_agent_settings_open = false;
        cx.notify();
        if let Some(client) = client {
            cx.spawn(async move |_, _| {
                let _ = client.logout().await;
            })
            .detach();
        }
    }

    pub fn refresh_coworkers(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.list_coworkers().await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(list) => {
                        state.coworkers = list;
                        if state
                            .active_coworker_id
                            .as_ref()
                            .is_none_or(|id| !state.coworkers.iter().any(|c| &c.id == id))
                        {
                            if let Some(first) = state.coworkers.first().cloned() {
                                state.select_coworker(first.id, cx);
                            }
                        }
                    }
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
        self.refresh_models(cx);
    }

    pub fn refresh_models(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.list_models().await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(catalogue) => state.model_catalogue = catalogue,
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn toggle_agent_settings(&mut self, cx: &mut Context<Self>) {
        self.is_agent_settings_open = !self.is_agent_settings_open;
        if !self.is_agent_settings_open {
            self.model_picker_open = false;
            self.avatar_editor_open = false;
        }
        cx.notify();
    }

    pub fn dismiss_popovers(&mut self, cx: &mut Context<Self>) {
        if !self.model_picker_open && !self.avatar_editor_open {
            return;
        }
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        cx.notify();
    }

    pub fn set_model_picker_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.model_picker_open == open && (!open || !self.avatar_editor_open) {
            return;
        }
        self.model_picker_open = open;
        if open {
            self.avatar_editor_open = false;
        }
        cx.notify();
    }

    pub fn set_avatar_editor_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.avatar_editor_open == open && (!open || !self.model_picker_open) {
            return;
        }
        self.avatar_editor_open = open;
        if open {
            self.model_picker_open = false;
        }
        cx.notify();
    }

    pub fn patch_active_coworker(
        &mut self,
        model: Option<String>,
        role: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.patch_active_agent(
            CoworkerPatch {
                model,
                role,
                ..Default::default()
            },
            cx,
        );
    }

    pub fn patch_active_agent(&mut self, patch: CoworkerPatch, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".into());
            cx.notify();
            return;
        };
        let Some(id) = self.active_coworker_id.clone() else {
            self.auth_error = Some("No agent selected".into());
            cx.notify();
            return;
        };
        if let Some(existing) = self.coworkers.iter_mut().find(|c| c.id == id) {
            if let Some(name) = patch.name.clone() {
                existing.name = name;
            }
            if let Some(model) = patch.model.clone() {
                existing.model = model;
            }
            if let Some(role) = patch.role.clone() {
                existing.role = if role.trim().is_empty() {
                    None
                } else {
                    Some(role)
                };
            }
            if let Some(title) = patch.title.clone() {
                existing.title = if title.trim().is_empty() {
                    None
                } else {
                    Some(title)
                };
            }
            if let Some(shape) = patch.avatar_shape.clone() {
                existing.avatar_shape = if shape.is_empty() { None } else { Some(shape) };
            }
            if let Some(color) = patch.avatar_color.clone() {
                existing.avatar_color = if color.is_empty() { None } else { Some(color) };
            }
            if let Some(notify) = patch.notify_on_updates {
                existing.notify_on_updates = Some(notify);
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.patch_coworker(&id, &patch).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(updated) => {
                        if let Some(existing) = state.coworkers.iter_mut().find(|c| c.id == id) {
                            if !updated.name.is_empty() {
                                existing.name = updated.name;
                            }
                            if !updated.model.is_empty() {
                                existing.model = updated.model;
                            }
                            if updated.role.is_some() {
                                existing.role = updated.role;
                            }
                            if updated.title.is_some() {
                                existing.title = updated.title;
                            }
                            if updated.avatar_shape.is_some() {
                                existing.avatar_shape = updated.avatar_shape;
                            }
                            if updated.avatar_color.is_some() {
                                existing.avatar_color = updated.avatar_color;
                            }
                            if updated.notify_on_updates.is_some() {
                                existing.notify_on_updates = updated.notify_on_updates;
                            }
                        }
                        state.auth_error = None;
                    }
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn update_opengrok_profile(
        &mut self,
        first_name: String,
        last_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .update_profile(&ProfileUpdate {
                    first_name: Some(first_name),
                    last_name: Some(last_name),
                    avatar_url: None,
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_error = None;
                    }
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn change_opengrok_password(
        &mut self,
        current: String,
        new_password: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.change_password(&current, &new_password).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(()) => state.auth_error = None,
                    Err(error) => state.auth_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn set_database_service(&mut self, service: DatabaseService, cx: &mut Context<Self>) {
        self.database_service = Some(service.clone());
        cx.notify();

        // Load sessions when DB service is set
        self.load_sessions(cx);
    }

    pub fn load_sessions(&mut self, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            cx.spawn(async move |this, cx| {
                    match db.get_sessions().await {
                        Ok(sessions) => {
                            this.update(cx, |state, cx| {
                                state.conversations = sessions
                                    .into_iter()
                                    .map(|s| Conversation {
                                        id: s.id,
                                        title: s.title,
                                        created_at: s.created_at,
                                        messages: Vec::new(), // Messages loaded on demand
                                        unread_count: 0,
                                    })
                                    .collect();

                                // If no active conversation, select the most recent one
                                if state.active_conversation_id.is_none() {
                                    if let Some(first) = state.conversations.first() {
                                        let id = first.id.clone();
                                        state.select_conversation(id, cx);
                                    }
                                }
                                cx.notify();
                            })
                            .ok();
                        }
                        Err(e) => eprintln!("Failed to load sessions: {}", e),
                    }
                })
            .detach();
        }
    }

    pub fn create_agent(&mut self, cx: &mut Context<Self>) {
        if self.hiring {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        if !self.is_signed_in() {
            self.auth_error = Some("Sign in first".to_string());
            cx.notify();
            return;
        }
        self.hiring = true;
        self.auth_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.hire("New Bot", None).await;
            let _ = this.update(cx, |state, cx| {
                state.hiring = false;
                match result {
                    Ok(hired) => {
                        let id = hired.id.clone();
                        state.coworkers.insert(0, hired);
                        state.select_coworker(id, cx);
                    }
                    Err(error) => {
                        state.auth_error = Some(format!(
                            "Could not create agent: {} (is OpenGrok running at {}?)",
                            error.message,
                            state
                                .config
                                .as_ref()
                                .map(|c| c.opengrok_base_url.as_str())
                                .unwrap_or("http://127.0.0.1:1447")
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn select_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(coworker) = self.coworkers.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        self.active_coworker_id = Some(id.clone());
        if !self.conversations.iter().any(|c| c.id == id) {
            self.conversations.insert(
                0,
                Conversation {
                    id: id.clone(),
                    title: coworker.name.clone(),
                    created_at: chrono::Local::now()
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string(),
                    messages: Vec::new(),
                    unread_count: 0,
                },
            );
        }
        self.select_conversation(id, cx);
    }

    pub fn create_new_session(&mut self, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            cx.spawn(async move |this, cx| {
                    match db.create_session("New Chat").await {
                        Ok(id) => {
                            this.update(cx, |state, cx| {
                                state.conversations.insert(
                                    0,
                                    Conversation {
                                        id: id.clone(),
                                        title: "New Chat".to_string(),
                                        created_at: chrono::Local::now()
                                            .format("%Y-%m-%d %H:%M:%S")
                                            .to_string(),
                                        messages: Vec::new(),
                                        unread_count: 0,
                                    },
                                );
                                state.select_conversation(id, cx);
                            })
                            .ok();
                        }
                        Err(e) => eprintln!("Failed to create session: {}", e),
                    }
                })
            .detach();
        }
    }

    pub fn rename_session(&mut self, id: String, new_title: String, cx: &mut Context<Self>) {
        if let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) {
            conversation.title = new_title.clone();
            cx.notify();

            if let Some(db) = self.database_service.clone() {
                cx.spawn(async move |_this, _cx| {
                        if let Err(e) = db.update_session_title(&id, &new_title).await {
                            eprintln!("Failed to rename session: {}", e);
                        }
                    },
                )
                .detach();
            }
        }
    }

    pub fn delete_session(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(index) = self.conversations.iter().position(|c| c.id == id) {
            self.conversations.remove(index);

            // If we deleted the active conversation, select another one
            if self.active_conversation_id.as_ref() == Some(&id) {
                self.active_conversation_id = self.conversations.first().map(|c| c.id.clone());
                if let Some(new_id) = self.active_conversation_id.clone() {
                    self.load_session_messages(new_id, cx);
                }
            }

            cx.notify();

            if let Some(db) = self.database_service.clone() {
                cx.spawn(async move |_this, _cx| {
                        if let Err(e) = db.delete_session(&id).await {
                            eprintln!("Failed to delete session: {}", e);
                        }
                    },
                )
                .detach();
            }
        }
    }

    pub fn load_session_messages(&mut self, session_id: String, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            let session_id_clone = session_id.clone();
            cx.spawn(async move |this, cx| {
                    match db.get_messages(&session_id_clone).await {
                        Ok(db_messages) => {
                            this.update(cx, |state, cx| {
                                if let Some(conversation) = state
                                    .conversations
                                    .iter_mut()
                                    .find(|c| c.id == session_id_clone)
                                {
                                    conversation.messages = db_messages
                                        .into_iter()
                                        .map(|m| {
                                            let sent_at = NaiveDateTime::parse_from_str(
                                                &m.created_at,
                                                "%Y-%m-%d %H:%M:%S",
                                            )
                                            .map(|dt| SystemTime::from(dt.and_utc()))
                                            .unwrap_or(SystemTime::now());

                                            Message {
                                                id: m.id,
                                                sender: if m.role == "user" {
                                                    "Me".to_string()
                                                } else {
                                                    "AI".to_string()
                                                },
                                                content: m.content,
                                                sent_at,
                                                is_me: m.role == "user",
                                            }
                                        })
                                        .collect();
                                    cx.notify();
                                }
                            })
                            .ok();
                        }
                        Err(e) => eprintln!("Failed to load messages: {}", e),
                    }
                })
            .detach();
        }
    }

    /// Load all profiles and credentials from the database into AppState.
    pub async fn load_profiles_and_credentials(
        db: &DatabaseService,
    ) -> anyhow::Result<(Vec<DbProfile>, Vec<DbCredential>)> {
        let profiles = db.get_profiles().await?;
        let credentials = db.get_credentials().await?;
        Ok((profiles, credentials))
    }

    /// Set the loaded profiles and credentials into AppState.
    pub fn set_profiles_and_credentials(
        &mut self,
        profiles: Vec<DbProfile>,
        credentials: Vec<DbCredential>,
        cx: &mut Context<Self>,
    ) {
        self.db_profiles = profiles.clone();
        self.db_credentials = credentials;

        // Update cache with fresh data from DB
        Self::save_cached_state(self.active_profile_id, self.db_profiles.clone());

        // Re-evaluate LLM provider with new credentials
        self.update_llm_provider(cx);

        cx.notify();
    }

    /// Reload profiles and credentials from the database and update the state.
    /// This ensures that any changes made (e.g., in settings) are reflected globally.
    pub fn reload_from_db(&mut self, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            cx.spawn(async move |this, cx| {
                    if let Ok((profiles, credentials)) =
                        Self::load_profiles_and_credentials(&db).await
                    {
                        this.update(cx, |state, cx| {
                            state.set_profiles_and_credentials(profiles, credentials, cx);
                        })
                        .ok();
                    }
                })
            .detach();
        }
    }

    /// Select a database profile by ID and update the active profile.
    /// This also updates the LLM provider to use the profile's credential.
    /// Persists the selection to the settings table.
    /// Requirements: 2.2, 5.1
    pub fn select_db_profile(&mut self, profile_id: i64, cx: &mut Context<Self>) {
        // Verify the profile exists before setting
        if self.db_profiles.iter().any(|p| p.id == profile_id) {
            // Stop any active TTS
            self.stop_read_aloud(cx);

            self.active_profile_id = Some(profile_id);
            self.update_llm_provider(cx);

            // Persist to local cache immediately
            Self::save_cached_state(Some(profile_id), self.db_profiles.clone());

            // Persist the selection asynchronously to DB
            if let Some(db) = self.database_service.clone() {
                cx.spawn(async move |_this, _cx| {
                        if let Err(e) = Self::persist_selected_profile(&db, Some(profile_id)).await
                        {
                            eprintln!("Failed to persist selected profile: {}", e);
                        }
                    },
                )
                .detach();
            }

            cx.notify();
        }
    }

    /// Get the currently active database profile.
    pub fn active_profile(&self) -> Option<&DbProfile> {
        self.active_profile_id
            .and_then(|id| self.db_profiles.iter().find(|p| p.id == id))
    }

    /// Get the text credential for the active profile.
    pub fn active_credential(&self) -> Option<&DbCredential> {
        self.active_profile()
            .and_then(|profile| profile.text_credential_id)
            .and_then(|cred_id| self.db_credentials.iter().find(|c| c.id == cred_id))
    }

    /// Persist the selected profile ID to the settings table.
    /// Requirements: 5.1
    pub async fn persist_selected_profile(
        db: &DatabaseService,
        profile_id: Option<i64>,
    ) -> anyhow::Result<()> {
        const SETTING_KEY: &str = "selected_profile_id";
        match profile_id {
            Some(id) => {
                db.set_setting(SETTING_KEY, &id.to_string()).await?;
            }
            None => {
                db.delete_setting(SETTING_KEY).await?;
            }
        }
        Ok(())
    }

    /// Save the selected profile ID and profile list to a local JSON file for instant startup.
    pub fn save_cached_state(profile_id: Option<i64>, profiles: Vec<DbProfile>) {
        use std::fs;
        use std::path::PathBuf;

        // Determine config directory
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nativechat");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let cache_file = config_dir.join("last_profile.json");

        #[derive(serde::Serialize)]
        struct CachedState {
            profile_id: Option<i64>,
            profiles: Vec<DbProfile>,
        }

        let data = CachedState {
            profile_id,
            profiles,
        };

        if let Ok(json) = serde_json::to_string(&data) {
            let _ = fs::write(cache_file, json);
        }
    }

    /// Load the cached profile ID and profile list from the local JSON file.
    pub fn load_cached_state() -> Option<(Option<i64>, Vec<DbProfile>)> {
        use std::fs;
        use std::path::PathBuf;

        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nativechat");
        let cache_file = config_dir.join("last_profile.json");

        #[derive(serde::Deserialize)]
        struct CachedState {
            profile_id: Option<i64>,
            profiles: Vec<DbProfile>,
        }

        if let Ok(content) = fs::read_to_string(cache_file) {
            if let Ok(data) = serde_json::from_str::<CachedState>(&content) {
                return Some((data.profile_id, data.profiles));
            }
        }

        None
    }

    /// Restore the selected profile from settings on startup.
    /// Validates that the profile still exists, clears if not.
    /// Requirements: 5.2, 5.3
    pub async fn restore_selected_profile(
        db: &DatabaseService,
        profiles: &[DbProfile],
    ) -> anyhow::Result<Option<i64>> {
        const SETTING_KEY: &str = "selected_profile_id";

        if let Some(value) = db.get_setting(SETTING_KEY).await? {
            if let Ok(profile_id) = value.parse::<i64>() {
                // Validate profile still exists
                if profiles.iter().any(|p| p.id == profile_id) {
                    return Ok(Some(profile_id));
                } else {
                    // Profile no longer exists, clear the setting
                    db.delete_setting(SETTING_KEY).await?;
                }
            }
        }
        Ok(None)
    }

    /// Update the LLM provider based on the active profile's credential.
    /// Falls back to Config-based provider if no profile/credential is selected.
    pub fn update_llm_provider(&mut self, cx: &mut Context<Self>) {
        // Try to create provider from active profile's credential
        if let Some(credential) = self.active_credential() {
            let model_id = self
                .active_profile()
                .and_then(|p| p.text_model_id.as_deref());

            match create_provider_from_credential(credential, model_id) {
                Ok(provider) => {
                    println!(
                        "[LLM] Initialized {} provider from profile credential",
                        credential.provider
                    );
                    self.llm_provider = Some(Arc::from(provider));
                    cx.notify();
                    return;
                }
                Err(e) => {
                    eprintln!("[LLM] Failed to create provider from credential: {}", e);
                }
            }
        }

        // Fall back to Config-based provider
        if let Some(config) = &self.config {
            match create_provider(config) {
                Ok(provider) => {
                    println!(
                        "[LLM] Initialized {} provider from config (fallback)",
                        config.default_provider
                    );
                    self.llm_provider = Some(Arc::from(provider));
                }
                Err(e) => {
                    eprintln!("[LLM] Failed to create provider from config: {}", e);
                }
            }
        }
        cx.notify();
    }

    pub fn fetch_models(&mut self, api_keys: HashMap<Provider, String>, cx: &mut Context<Self>) {
        let registry = self.model_registry.clone();
        cx.spawn(async move |this, cx| {
                let models = registry.get_all_models(&api_keys).await;
                this.update(cx, |state: &mut AppState, cx| {
                    state.available_models = models;
                    cx.notify();
                })
                .ok();
            })
        .detach();
    }

    pub fn select_conversation(&mut self, conversation_id: String, cx: &mut Context<Self>) {
        self.active_conversation_id = Some(conversation_id.clone());
        self.load_session_messages(conversation_id, cx);
        cx.notify();
    }

    pub fn select_profile(&mut self, profile_id: usize, cx: &mut Context<Self>) {
        if let Some(profile) = self.profiles.iter().find(|p| p.id == profile_id) {
            self.selected_profile = Some(profile.clone());
            cx.notify();
        }
    }

    pub fn select_app(&mut self, app_name: String, cx: &mut Context<Self>) {
        println!("State: select_app called for {}", app_name);
        if !self.selected_apps.contains(&app_name) {
            println!("State: Adding {} to selected_apps", app_name);
            self.selected_apps.push(app_name);
            cx.notify();
        } else {
            println!("State: {} already selected", app_name);
        }
    }

    pub fn remove_app(&mut self, app_name: String, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_apps.iter().position(|a| *a == app_name) {
            self.selected_apps.remove(index);
            cx.notify();
        }
    }

    fn send_opengrok_turn(
        &mut self,
        conversation_id: String,
        _content: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        let coworker_id = self.active_coworker_id.clone();
        let history: Vec<AguiMessage> = self
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .map(|c| {
                c.messages
                    .iter()
                    .map(|m| AguiMessage {
                        id: m.id.clone(),
                        role: if m.is_me {
                            "user".to_string()
                        } else {
                            "assistant".to_string()
                        },
                        content: m.content.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            conversation.messages.push(Message {
                id: "temp-ai".to_string(),
                sender: "AI".to_string(),
                content: String::new(),
                sent_at: SystemTime::now(),
                is_me: false,
            });
        }
        self.is_ai_responding = true;
        self.bot_status = Some("Thinking".into());
        cx.notify();

        cx.spawn(async move |this, cx| {
            let coworker = match coworker_id {
                Some(id) => Ok(id),
                None => match client.hire("NativeChat", None).await {
                    Ok(hired) => {
                        let id = hired.id.clone();
                        let _ = this.update(cx, |state, _| {
                            state.active_coworker_id = Some(id.clone());
                            state.coworkers = vec![hired];
                        });
                        Ok(id)
                    }
                    Err(error) => Err(error),
                },
            };
            let result = match coworker {
                Ok(id) => {
                    let mut args_by_call: std::collections::HashMap<String, String> =
                        std::collections::HashMap::new();
                    let mut names_by_call: std::collections::HashMap<String, String> =
                        std::collections::HashMap::new();
                    client
                        .run_turn(&id, &conversation_id, &history, |event| {
                            if let Some(call_id) =
                                event.get("toolCallId").and_then(|v| v.as_str())
                            {
                                if let Some(name) =
                                    event.get("toolCallName").and_then(|v| v.as_str())
                                {
                                    names_by_call.insert(call_id.to_string(), name.to_string());
                                }
                                if let Some(delta) = event.get("delta").and_then(|v| v.as_str()) {
                                    args_by_call
                                        .entry(call_id.to_string())
                                        .or_default()
                                        .push_str(delta);
                                }
                            }
                            let call_id = event.get("toolCallId").and_then(|v| v.as_str());
                            let args = call_id.and_then(|id| args_by_call.get(id)).map(String::as_str);
                            let mut event = event.clone();
                            if event.get("toolCallName").is_none() {
                                if let Some(name) = call_id.and_then(|id| names_by_call.get(id)) {
                                    event
                                        .as_object_mut()
                                        .map(|o| o.insert("toolCallName".into(), name.clone().into()));
                                }
                            }
                            match activity_from_agui(&event, args) {
                                ActivityTick::Keep => {}
                                ActivityTick::Clear => {
                                    let _ = this.update(cx, |state, cx| {
                                        if state.bot_status.is_some() {
                                            state.bot_status = None;
                                            cx.notify();
                                        }
                                    });
                                }
                                ActivityTick::Set(activity) => {
                                    let _ = this.update(cx, |state, cx| {
                                        if state.bot_status.as_deref() != Some(activity.label.as_str())
                                        {
                                            state.bot_status = Some(activity.label);
                                            cx.notify();
                                        }
                                    });
                                }
                            }
                        })
                        .await
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                if let Some(conversation) = state
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    if let Some(last) = conversation.messages.last_mut() {
                        if last.id == "temp-ai" {
                            match &result {
                                Ok(text) if !text.is_empty() => last.content = text.clone(),
                                Ok(_) => {
                                    last.content =
                                        "(OpenGrok returned no assistant text.)".to_string()
                                }
                                Err(error) => last.content = format!("OpenGrok: {}", error.message),
                            }
                            last.id = uuid::Uuid::now_v7().to_string();
                        }
                    }
                }
                state.is_ai_responding = false;
                state.bot_status = None;
                if let Err(error) = result {
                    state.auth_error = Some(error.message);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn send_message(&mut self, content: String, cx: &mut Context<Self>) {
        if self.is_signed_in() && self.active_coworker_id.is_none() {
            self.auth_error = Some("Create a bot first".to_string());
            cx.notify();
            return;
        }
        let conversation_id = match &self.active_conversation_id {
            Some(id) => id.clone(),
            None => return,
        };

        // Add user message to UI immediately
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            let message = Message {
                id: "temp".to_string(), // Temporary ID
                sender: "Me".to_string(),
                content: content.clone(),
                sent_at: SystemTime::now(),
                is_me: true,
            };
            conversation.messages.push(message);
        }
        cx.notify();

        // Save to DB
        if let Some(db) = self.database_service.clone() {
            let content_clone = content.clone();
            let conversation_id_clone = conversation_id.clone();
            cx.spawn(async move |this, cx| {
                    match db
                        .save_message(&conversation_id_clone, "user", &content_clone, None, None)
                        .await
                    {
                        Ok(id) => {
                            this.update(cx, |state, cx| {
                                if let Some(conversation) = state
                                    .conversations
                                    .iter_mut()
                                    .find(|c| c.id == conversation_id_clone)
                                {
                                    if let Some(msg) = conversation.messages.last_mut() {
                                        if msg.id == "temp" {
                                            msg.id = id;
                                        }
                                    }
                                }
                            })
                            .ok();
                        }
                        Err(e) => eprintln!("Failed to save user message: {}", e),
                    }
                })
            .detach();
        }

        if self.is_signed_in() {
            self.send_opengrok_turn(conversation_id, content, cx);
            return;
        }

        // Get AI response
        let provider = match &self.llm_provider {
            Some(p) => p.clone(),
            None => {
                eprintln!("[LLM] No provider configured");
                return;
            }
        };

        // Build chat history for context
        let chat_messages: Vec<ChatMessage> = self
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .map(|c| {
                c.messages
                    .iter()
                    .map(|m| ChatMessage {
                        role: if m.is_me {
                            "user".to_string()
                        } else {
                            "assistant".to_string()
                        },
                        content: m.content.clone(),
                        images: None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Use active profile's text_model_id if set, otherwise fall back to provider default
        // Requirements 4.1, 4.2
        let model = self
            .active_profile()
            .and_then(|p| p.text_model_id.clone())
            .unwrap_or_else(|| provider.default_model().to_string());

        let request = ChatRequest {
            model: model.clone(),
            messages: chat_messages,
            system_prompt: Some("You are a helpful AI assistant.".to_string()),
            temperature: 0.7,
            max_tokens: Some(2048),
            stream: true,
        };

        self.is_ai_responding = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
                println!("[LLM] Sending request to AI...");

                // Create the AI message placeholder first
                let _ = this.update(cx, |state, model_cx| {
                    if let Some(conversation) = state
                        .conversations
                        .iter_mut()
                        .find(|c| c.id == conversation_id)
                    {
                        let ai_message = Message {
                            id: "temp".to_string(), // Temporary ID
                            sender: "AI".to_string(),
                            content: String::new(), // Start empty
                            sent_at: SystemTime::now(),
                            is_me: false,
                        };
                        conversation.messages.push(ai_message);
                    }
                    model_cx.notify();
                });

                let mut full_response = String::new();

                match provider.chat_stream(request).await {
                    Ok(mut stream) => {
                        println!("[LLM] Stream started");

                        while let Some(chunk_result) = stream.next().await {
                            match chunk_result {
                                Ok(chunk) => {
                                    if !chunk.delta.is_empty() {
                                        full_response.push_str(&chunk.delta);
                                        let _ = this.update(cx, |state, cx| {
                                            if let Some(conversation) = state
                                                .conversations
                                                .iter_mut()
                                                .find(|c| c.id == conversation_id)
                                            {
                                                if let Some(last_msg) =
                                                    conversation.messages.last_mut()
                                                {
                                                    last_msg.content.push_str(&chunk.delta);
                                                    cx.notify();
                                                }
                                            }
                                        });
                                    }
                                }
                                Err(e) => {
                                    eprintln!("[LLM] Stream error: {}", e);
                                    // Append error to message or show error
                                }
                            }
                        }

                        let _ = this.update(cx, |state, cx| {
                            state.is_ai_responding = false;
                            // Save AI response to DB
                            if let Some(db) = state.database_service.clone() {
                                let response_clone = full_response.clone();
                                let model_clone = model.clone();
                                let conversation_id_clone = conversation_id.clone();
                                cx.spawn(async move |this, cx| {
                                        match db
                                            .save_message(
                                                &conversation_id_clone,
                                                "assistant",
                                                &response_clone,
                                                Some(model_clone),
                                                None,
                                            )
                                            .await
                                        {
                                            Ok(id) => {
                                                this.update(cx, |state, cx| {
                                                    if let Some(conversation) = state
                                                        .conversations
                                                        .iter_mut()
                                                        .find(|c| c.id == conversation_id_clone)
                                                    {
                                                        if let Some(msg) =
                                                            conversation.messages.last_mut()
                                                        {
                                                            if msg.id == "temp" {
                                                                msg.id = id;
                                                            }
                                                        }
                                                    }
                                                })
                                                .ok();
                                            }
                                            Err(e) => eprintln!("Failed to save AI message: {}", e),
                                        }
                                })
                                .detach();
                            }
                            cx.notify();
                        });
                        println!("[LLM] Stream finished");
                    }
                    Err(e) => {
                        eprintln!("[LLM] Error starting stream: {}", e);
                        let _ = this.update(cx, |state, cx| {
                            if let Some(conversation) = state
                                .conversations
                                .iter_mut()
                                .find(|c| c.id == conversation_id)
                            {
                                // If we failed to start stream, we might want to remove the empty message
                                // or update it with error
                                if let Some(last_msg) = conversation.messages.last_mut() {
                                    last_msg.content = format!("Error: {}", e);
                                }
                            }
                            state.is_ai_responding = false;
                            cx.notify();
                        });
                    }
                }
            })
        .detach();
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_hidden = !self.sidebar_hidden;
        cx.notify();
    }

    pub fn toggle_mini_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_hidden {
            self.sidebar_hidden = false;
            self.sidebar_collapsed = false;
        } else {
            self.sidebar_collapsed = !self.sidebar_collapsed;
        }
        self.auto_collapsed = false;
        self.sidebar_responsive = remember_choice(self.sidebar_responsive, self.sidebar_collapsed);
        cx.notify();
    }

    pub fn resize_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        let next = sidebar_from_resize(
            SidebarChrome {
                hidden: self.sidebar_hidden,
                collapsed: self.sidebar_collapsed,
                expanded_width: self.sidebar_expanded_width,
            },
            width,
        );
        self.sidebar_hidden = next.hidden;
        self.sidebar_collapsed = next.collapsed;
        self.sidebar_expanded_width = next.expanded_width;
        self.auto_collapsed = false;
        if !next.hidden {
            self.sidebar_responsive = remember_choice(self.sidebar_responsive, next.collapsed);
        }
        cx.notify();
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool, auto: bool, cx: &mut Context<Self>) {
        self.sidebar_collapsed = collapsed;
        self.auto_collapsed = auto;
        self.sidebar_responsive = remember_choice(self.sidebar_responsive, collapsed);
        cx.notify();
    }

    pub fn apply_responsive_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        let result = collapse_for_width(
            self.sidebar_responsive,
            width,
            self.sidebar_collapsed,
        );
        self.sidebar_responsive = result.next;
        if let Some(apply) = result.apply {
            if self.sidebar_collapsed != apply {
                self.sidebar_collapsed = apply;
                self.auto_collapsed = true;
                cx.notify();
            }
        }
    }

    pub fn toggle_debug_markdown(&mut self, cx: &mut Context<Self>) {
        self.debug_markdown_disabled = !self.debug_markdown_disabled;
        println!(
            "[DEBUG] Markdown rendering: {}",
            if self.debug_markdown_disabled {
                "DISABLED (plain text)"
            } else {
                "ENABLED"
            }
        );
        cx.notify();
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        use gpui_kit::component::{Theme, ThemeRegistry};

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
            Theme::sync_base(cx);
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

    pub fn toggle_credentials_modal(&mut self, cx: &mut Context<Self>) {
        self.is_credentials_modal_open = !self.is_credentials_modal_open;
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

        cx.spawn(async move |this, cx| {
                println!("Connecting to Gemini...");
                match GeminiLiveClient::connect(api_key, ai_amplitude) {
                    Ok(client) => {
                        println!("Connected to Gemini!");
                        let _ = this.update(cx, |state, _cx| {
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
                        let _ = this.update(cx, |state, _cx| {
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

    pub fn read_aloud(
        &mut self,
        text: String,
        message_id: String,
        source: TtsSource,
        cx: &mut Context<Self>,
    ) {
        if self.tts_service.is_none() {
            match TtsService::new(self.is_ai_speaking.clone(), self.ai_amplitude.clone()) {
                Ok(service) => self.tts_service = Some(service),
                Err(e) => {
                    eprintln!("Failed to initialize TTS service: {}", e);
                    return;
                }
            }
        }

        if let Some(service) = &self.tts_service {
            match source {
                TtsSource::Native => {
                    // Pause AI if running
                    if self.ai_tts.message_id.is_some() && !self.ai_tts.is_paused {
                        service.pause();
                        self.ai_tts.is_paused = true;
                    }

                    self.native_tts.message_id = Some(message_id.clone());
                    self.native_tts.is_paused = false;
                    self.native_tts.is_loading = false;
                    cx.notify();

                    let service = service.clone();
                    let message_id = message_id.clone();
                    let text = text.clone();

                    if service.start_speaking_native(&text, &message_id) {
                        cx.spawn(async move |this, cx| {
                            service.wait_until_finished_native().await;
                            if let Some(this) = this.upgrade() {
                                let _ = this.update(cx, |state, cx| {
                                    if state.native_tts.message_id.as_ref() == Some(&message_id) {
                                        state.native_tts.message_id = None;
                                        cx.notify();
                                    }
                                });
                            }
                        })
                        .detach();
                    }
                }
                TtsSource::AI => {
                    // Pause Native if running
                    if self.native_tts.message_id.is_some() && !self.native_tts.is_paused {
                        service.pause_native();
                        self.native_tts.is_paused = true;
                    }

                    self.ai_tts.message_id = Some(message_id.clone());
                    self.ai_tts.is_loading = true;
                    self.ai_tts.is_paused = false;
                    cx.notify();

                    // Get Config
                    let tts_model_id = self
                        .active_profile()
                        .and_then(|p| p.tts_model_id.clone())
                        .unwrap_or_else(|| "native".to_string());
                    let tts_voice = self.active_profile().and_then(|p| p.tts_voice.clone());
                    let api_key = self
                        .active_credential()
                        .map(|c| c.api_key.clone())
                        .or_else(|| self.config.as_ref().and_then(|c| c.gemini_api_key.clone()))
                        .unwrap_or_default();

                    let service = service.clone();
                    let message_id = message_id.clone();
                    let text = text.clone();

                    cx.spawn(async move |this, cx| {
                            let start_result = service
                                .start_speaking(
                                    &text,
                                    &message_id,
                                    &tts_model_id,
                                    &api_key,
                                    &tts_voice,
                                )
                                .await;
                            match start_result {
                                Ok(true) => {
                                    // Speaking started
                                    this.update(cx, |state, cx| {
                                        state.ai_tts.is_loading = false;
                                        cx.notify();
                                    })
                                    .ok();

                                    service.wait_until_finished_ai().await;

                                    this.update(cx, |state, cx| {
                                        if state.ai_tts.message_id.as_ref() == Some(&message_id) {
                                            state.ai_tts.message_id = None;
                                            cx.notify();
                                        }
                                    })
                                    .ok();
                                }
                                Ok(false) | Err(_) => {
                                    this.update(cx, |state, cx| {
                                        state.ai_tts.message_id = None;
                                        state.ai_tts.is_loading = false;
                                        cx.notify();
                                    })
                                    .ok();
                                }
                            }
                        })
                    .detach();
                }
            }
        }
    }

    pub fn stop_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            let _ = service.stop_native();
            let _ = service.stop();
            self.native_tts = SourceTtsState::default();
            self.ai_tts = SourceTtsState::default();
            cx.notify();
        }
    }

    pub fn pause_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            if self.native_tts.message_id.is_some() && !self.native_tts.is_paused {
                service.pause_native();
                self.native_tts.is_paused = true;
            }
            if self.ai_tts.message_id.is_some() && !self.ai_tts.is_paused {
                service.pause();
                self.ai_tts.is_paused = true;
            }
            cx.notify();
        }
    }

    pub fn resume_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            if self.native_tts.message_id.is_some() && self.native_tts.is_paused {
                service.resume_native();
                self.native_tts.is_paused = false;
            }
            if self.ai_tts.message_id.is_some() && self.ai_tts.is_paused {
                service.resume();
                self.ai_tts.is_paused = false;
            }
            cx.notify();
        }
    }

    pub fn active_highlight_range(&self) -> Option<std::ops::Range<usize>> {
        self.tts_service
            .as_ref()
            .and_then(|s| s.get_active_word_range())
    }

    pub fn toggle_read_aloud(
        &mut self,
        message_id: String,
        text: String,
        mode: TtsSource,
        cx: &mut Context<Self>,
    ) {
        if let Some(service) = &self.tts_service {
            match mode {
                TtsSource::Native => {
                    // Check if Native is active on this message
                    if self.native_tts.message_id.as_ref() == Some(&message_id) {
                        if self.native_tts.is_paused {
                            // Resume Native
                            // Ensure AI is paused first
                            if self.ai_tts.message_id.is_some() && !self.ai_tts.is_paused {
                                service.pause();
                                self.ai_tts.is_paused = true;
                            }
                            service.resume_native();
                            self.native_tts.is_paused = false;
                        } else {
                            // Pause Native
                            service.pause_native();
                            self.native_tts.is_paused = true;
                        }
                        cx.notify();
                    } else {
                        self.read_aloud(text, message_id, TtsSource::Native, cx);
                    }
                }
                TtsSource::AI => {
                    // Check if AI is active on this message
                    if self.ai_tts.message_id.as_ref() == Some(&message_id) {
                        if self.ai_tts.is_paused {
                            // Resume AI
                            // Ensure Native is paused
                            // Ensure Native is stopped completely so highlight color reverts to AI
                            if self.native_tts.message_id.is_some() {
                                service.stop_native();
                                self.native_tts = SourceTtsState::default();
                            }
                            service.resume();
                            self.ai_tts.is_paused = false;
                        } else {
                            // Pause AI
                            service.pause();
                            self.ai_tts.is_paused = true;
                        }
                        cx.notify();
                    } else {
                        self.read_aloud(text, message_id, TtsSource::AI, cx);
                    }
                }
            }
        } else {
            // Initialize if needed
            self.read_aloud(text, message_id, mode, cx);
        }
    }

    pub fn regenerate_audio(&mut self, message_id: String, text: String, cx: &mut Context<Self>) {
        // Clear cache helper
        if let Some(path) = TtsService::get_cache_path(&message_id) {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }

        // Force loading state immediately for reactivity
        self.ai_tts.message_id = Some(message_id.clone());
        self.ai_tts.is_loading = true;
        self.ai_tts.is_paused = false;
        cx.notify();

        self.read_aloud(text, message_id, TtsSource::AI, cx);
    }
}
