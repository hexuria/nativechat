use gpui_agent::prelude::*;
use gpui_agent::{DispatchResult, virtual_unavailable};

use crate::state::AppState;

pub mod ids {
    pub const WINDOW: &str = "app-window";
    pub const PAGE: &str = "page-chat";
    pub const SIDEBAR: &str = "sidebar";
    pub const SIDEBAR_LIST: &str = "sidebar-chat-list";
    pub const NAV_NEW_CHAT: &str = "nav-new-chat";
    pub const NAV_SEARCH: &str = "nav-search";
    pub const NAV_LIBRARY: &str = "nav-library";
    pub const NAV_PROJECTS: &str = "nav-projects";
    pub const NAV_TOGGLE: &str = "nav-toggle-sidebar";
    pub const FOOTER_THEME: &str = "footer-theme";
    pub const FOOTER_ACCOUNT: &str = "footer-account";
    pub const FOOTER_CREDENTIALS: &str = "footer-credentials";
    pub const FOOTER_PROFILE: &str = "footer-profile";
    pub const FOOTER_SIGN_OUT: &str = "footer-sign-out";
    pub const COMPOSER: &str = "composer";
    pub const PROFILE_SELECT: &str = "profile-select";
    pub const DIALOG_ACCOUNT: &str = "dialog-account";
    pub const DIALOG_PROFILE: &str = "dialog-profile";
    pub const DIALOG_CREDENTIALS: &str = "dialog-credentials";
    pub const DIALOG_VOICE: &str = "dialog-voice";

    pub fn session(id: &str) -> String {
        format!("session-{id}")
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    NewChat,
    ToggleSidebar,
    ToggleTheme,
    ToggleAccount,
    ToggleCredentials,
    ToggleProfile,
    SelectSession(String),
    Shutdown,
}

impl Command {
    pub fn apply(self, state: &mut AppState, cx: &mut gpui_kit::Context<AppState>) {
        match self {
            Self::NewChat => state.create_new_session(cx),
            Self::ToggleSidebar => state.toggle_sidebar(cx),
            Self::ToggleTheme => state.toggle_theme(cx),
            Self::ToggleAccount => state.toggle_account_settings(cx),
            Self::ToggleCredentials => state.toggle_credentials_modal(cx),
            Self::ToggleProfile => state.toggle_profile_settings(cx),
            Self::SelectSession(id) => state.select_conversation(id, cx),
            Self::Shutdown => {}
        }
    }
}

#[derive(Clone)]
struct SessionSnap {
    id: String,
    title: String,
    active: bool,
}

pub struct NativeChatHost {
    ready: bool,
    sidebar_collapsed: bool,
    theme_mode: String,
    sessions: Vec<SessionSnap>,
    profile_name: Option<String>,
    account_open: bool,
    profile_open: bool,
    credentials_open: bool,
    voice_open: bool,
    pending: Option<Command>,
}

impl NativeChatHost {
    pub fn from_app(state: &AppState) -> Self {
        let active = state.active_conversation_id.clone();
        let sessions = state
            .conversations
            .iter()
            .map(|c| SessionSnap {
                active: active.as_ref() == Some(&c.id),
                id: c.id.clone(),
                title: c.title.clone(),
            })
            .collect();
        let profile_name = state
            .active_profile_id
            .and_then(|id| state.db_profiles.iter().find(|p| p.id == id))
            .map(|p| p.name.clone());
        Self {
            ready: true,
            sidebar_collapsed: state.sidebar_collapsed,
            theme_mode: state.theme_mode.clone(),
            sessions,
            profile_name,
            account_open: state.is_account_settings_open,
            profile_open: state.is_profile_settings_open,
            credentials_open: state.is_credentials_modal_open,
            voice_open: state.is_voice_mode_open,
            pending: None,
        }
    }

    pub fn take_command(&mut self) -> Option<Command> {
        self.pending.take()
    }

    fn tree(&self) -> UiTree {
        let sessions: Vec<UiNode> = self
            .sessions
            .iter()
            .map(|s| {
                let mut item = UiNode::listitem(ids::session(&s.id), s.title.clone());
                if s.active {
                    item.states.push("selected".into());
                }
                item
            })
            .collect();

        let sidebar = UiNode::navigation(ids::SIDEBAR, "Sidebar")
            .with_child(UiNode::button(ids::NAV_TOGGLE, "Toggle sidebar"))
            .with_child(UiNode::button(ids::NAV_NEW_CHAT, "New Chat"))
            .with_child(UiNode::button(ids::NAV_SEARCH, "Search"))
            .with_child(UiNode::button(ids::NAV_LIBRARY, "Library"))
            .with_child(UiNode::button(ids::NAV_PROJECTS, "Projects"))
            .with_child(
                UiNode::scroll(ids::SIDEBAR_LIST, "Chats").with_child(
                    UiNode::list("sidebar-sessions", "Sessions").with_children(sessions),
                ),
            )
            .with_child(UiNode::button(
                ids::FOOTER_THEME,
                format!("Theme: {}", self.theme_mode),
            ))
            .with_child(UiNode::button(ids::FOOTER_ACCOUNT, "Account Settings"))
            .with_child(UiNode::button(ids::FOOTER_CREDENTIALS, "Credentials"))
            .with_child(UiNode::button(ids::FOOTER_PROFILE, "Profile Settings"))
            .with_child(UiNode::button(ids::FOOTER_SIGN_OUT, "Sign Out"));

        let page = UiNode::page(ids::PAGE, "Chat")
            .with_child(
                UiNode::new(ids::PROFILE_SELECT, "combobox", "Select Profile").with_value(
                    self.profile_name
                        .clone()
                        .unwrap_or_else(|| "Select Profile".into()),
                ),
            )
            .with_child(UiNode::textbox(ids::COMPOSER, "Type a message..."));

        UiTree {
            app: "nativechat".into(),
            platform: PlatformKind::Desktop,
            ready: self.ready,
            nodes: vec![
                UiNode::window(ids::WINDOW, "NativeChat")
                    .with_child(sidebar)
                    .with_child(page)
                    .with_child(
                        UiNode::dialog(ids::DIALOG_ACCOUNT, "Account Settings")
                            .with_visible(self.account_open),
                    )
                    .with_child(
                        UiNode::dialog(ids::DIALOG_PROFILE, "Profile Settings")
                            .with_visible(self.profile_open),
                    )
                    .with_child(
                        UiNode::dialog(ids::DIALOG_CREDENTIALS, "Credentials")
                            .with_visible(self.credentials_open),
                    )
                    .with_child(
                        UiNode::dialog(ids::DIALOG_VOICE, "Voice Mode").with_visible(self.voice_open),
                    ),
            ],
        }
    }

    fn click(&mut self, target: &str) -> Result<DispatchResult, String> {
        let cmd = if target == ids::NAV_NEW_CHAT {
            Command::NewChat
        } else if target == ids::NAV_TOGGLE {
            Command::ToggleSidebar
        } else if target == ids::FOOTER_THEME {
            Command::ToggleTheme
        } else if target == ids::FOOTER_ACCOUNT {
            Command::ToggleAccount
        } else if target == ids::FOOTER_CREDENTIALS {
            Command::ToggleCredentials
        } else if target == ids::FOOTER_PROFILE {
            Command::ToggleProfile
        } else if target == ids::NAV_SEARCH
            || target == ids::NAV_LIBRARY
            || target == ids::NAV_PROJECTS
            || target == ids::FOOTER_SIGN_OUT
        {
            return Ok(DispatchResult::empty());
        } else if let Some(id) = target.strip_prefix("session-") {
            Command::SelectSession(id.to_string())
        } else {
            return Err(format!("unknown click target `{target}`"));
        };
        self.pending = Some(cmd);
        Ok(DispatchResult::empty())
    }

    fn invoke(&mut self, name: &str, args: &serde_json::Value) -> Result<DispatchResult, String> {
        let cmd = match name {
            "chat.new" => Command::NewChat,
            "sidebar.toggle" => Command::ToggleSidebar,
            "theme.toggle" => Command::ToggleTheme,
            "settings.account" => Command::ToggleAccount,
            "settings.credentials" => Command::ToggleCredentials,
            "settings.profile" => Command::ToggleProfile,
            "session.select" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "session.select requires arg id".to_string())?;
                Command::SelectSession(id.to_string())
            }
            other => return Err(format!("unknown invoke `{other}`")),
        };
        self.pending = Some(cmd);
        Ok(DispatchResult::empty())
    }
}

impl AgentHost for NativeChatHost {
    fn hello(&self) -> HelloInfo {
        HelloInfo {
            protocol: PROTOCOL_VERSION,
            app: "nativechat".into(),
            platform: PlatformKind::Desktop,
            os: gpui_agent::protocol::host_os(),
            ready: self.ready,
            deliveries: vec![DeliveryMode::Semantic],
            auth: HelloAuth::None,
        }
    }

    fn snapshot(&self) -> UiTree {
        self.tree()
    }

    fn dispatch(&mut self, op: &Op) -> Result<DispatchResult, String> {
        if op.is_virtual_input() {
            return Err(virtual_unavailable(
                "NativeChat agent host is semantic-only (no virtual GPUI events yet)",
            ));
        }
        match op {
            Op::Click { target, .. } => self.click(target),
            Op::Invoke { name, args } => self.invoke(name, args),
            Op::Shutdown => {
                self.pending = Some(Command::Shutdown);
                Ok(DispatchResult::empty())
            }
            Op::SetValue { .. } | Op::Type { .. } | Op::Key { .. } => {
                Err("composer typing is not wired yet".into())
            }
            _ => Ok(DispatchResult::empty()),
        }
    }

    fn screenshot(
        &self,
        _spec: gpui_agent::scroll_capture::ScreenshotSpec<'_>,
    ) -> Result<DispatchResult, String> {
        Err(gpui_agent::screenshot_unavailable(
            "screenshot is intercepted on the UI thread",
        ))
    }
}
