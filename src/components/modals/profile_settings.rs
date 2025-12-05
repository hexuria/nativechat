use crate::services::database::{Credential, Profile};
use crate::services::model_registry::{ModelProfile, ModelType, Provider};
use crate::state::AppState;

use gpui::prelude::*;
use gpui::*;
use std::collections::HashMap;
use std::rc::Rc;
use ui::button::ButtonVariants;
use ui::list::ListItem;
use ui::notification::Notification;

use ui::root::root::WindowExt;
use ui::scroll::ScrollbarAxis;
use ui::{
    Disableable, Icon, IconName, Sizable, Size as UiSize, StyledExt,
    button::{Button, ButtonVariant},
    dialog::DialogButtonProps,
    h_flex,
    input::{Input, InputState},
    label::Label,
    list::{List, ListDelegate, ListState},
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
    theme::ActiveTheme,
};
use ui::{IndexPath, StyleSized};

#[derive(Clone)]
pub struct CredentialItem(pub Credential);

impl SelectItem for CredentialItem {
    type Value = Credential;

    fn title(&self) -> SharedString {
        self.0.name.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

pub struct ProfileListDelegate {
    pub profiles: Vec<Profile>,
    selected_index: Option<usize>,
    on_click: Rc<dyn Fn(usize, &mut Window, &mut App)>,
}

impl ProfileListDelegate {
    pub fn new(profiles: Vec<Profile>, on_click: Rc<dyn Fn(usize, &mut Window, &mut App)>) -> Self {
        Self {
            profiles,
            selected_index: None,
            on_click,
        }
    }
}

impl ListDelegate for ProfileListDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.profiles.len()
    }

    fn render_item(&self, ix: IndexPath, _window: &mut Window, cx: &mut App) -> Option<Self::Item> {
        let profile = self.profiles.get(ix.row)?;
        let theme = cx.theme();
        let is_selected = self.selected_index == Some(ix.row);
        let on_click = self.on_click.clone();

        Some(
            ListItem::new(ix)
                .p_1()
                .child(
                    div()
                        .w_full()
                        .p_2()
                        .rounded_md()
                        .hover(|s| s.bg(theme.secondary.opacity(0.5)))
                        .when(is_selected, |s| s.bg(theme.secondary))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .child(profile.name.clone())
                                .font_weight(FontWeight::MEDIUM)
                                .text_sm()
                                .text_color(if is_selected {
                                    theme.foreground
                                } else {
                                    theme.foreground
                                }),
                        )
                        .child(
                            div()
                                .child(if let Some(model_id) = &profile.text_model_id {
                                    model_id.clone()
                                } else {
                                    "No model selected".to_string()
                                })
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .text_ellipsis(),
                        ),
                )
                .on_click(move |_, window, cx| {
                    on_click(ix.row, window, cx);
                }),
        )
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix.map(|ix| ix.row);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileMode {
    Editing(i64),
    Creating,
}

pub struct ProfileSettingsModal {
    state: Entity<AppState>,
    profile_name_input: Entity<InputState>,
    provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    embedding_provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    embedding_model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    image_provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    image_model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    tts_provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    tts_model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    chat_credential_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
    embedding_credential_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
    image_credential_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
    list_state: Entity<ListState<ProfileListDelegate>>,
    selected_index: Option<usize>,
    error_message: Option<String>,
    sidebar_open: bool,
    last_window_width: Option<Pixels>,
    credentials: Vec<Credential>,
    mode: ProfileMode,
    needs_reset: bool,
    creating_chat_cred: bool,
    creating_embedding_cred: bool,
    creating_image_cred: bool,
    new_chat_cred_input: Entity<InputState>,
    new_embedding_cred_input: Entity<InputState>,
    new_image_cred_input: Entity<InputState>,
    pending_credential_id: Option<i64>,
    should_focus_chat: bool,
    should_focus_embedding: bool,
    should_focus_image: bool,
    is_saving: bool,
    show_form: bool,
    is_new_mode: bool,
    pending_success_message: Option<String>,
}

impl ProfileSettingsModal {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let profile_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Profile Name"));

        // Trigger model fetch with empty keys (will use env vars)
        state.update(cx, |state, cx| {
            state.fetch_models(HashMap::new(), cx);
        });

        let provider_items = SearchableVec::new(vec![]);
        let provider_select = cx.new(|cx| SelectState::new(provider_items, None, window, cx));

        let model_items = SearchableVec::new(vec![]);
        let model_select = cx.new(|cx| SelectState::new(model_items, None, window, cx));

        let embedding_provider_items = SearchableVec::new(vec![]);
        let embedding_provider_select =
            cx.new(|cx| SelectState::new(embedding_provider_items, None, window, cx));

        let embedding_model_items = SearchableVec::new(vec![]);
        let embedding_model_select =
            cx.new(|cx| SelectState::new(embedding_model_items, None, window, cx));

        let image_provider_items = SearchableVec::new(vec![]);
        let image_provider_select =
            cx.new(|cx| SelectState::new(image_provider_items, None, window, cx));

        let image_model_items = SearchableVec::new(vec![]);
        let image_model_select = cx.new(|cx| SelectState::new(image_model_items, None, window, cx));

        let tts_provider_items = SearchableVec::new(vec![]);
        let tts_provider_select =
            cx.new(|cx| SelectState::new(tts_provider_items, None, window, cx));

        let tts_model_items = SearchableVec::new(vec![]);
        let tts_model_select = cx.new(|cx| SelectState::new(tts_model_items, None, window, cx));

        let chat_credential_select =
            cx.new(|cx| SelectState::new(SearchableVec::new(vec![]), None, window, cx));

        let embedding_credential_select =
            cx.new(|cx| SelectState::new(SearchableVec::new(vec![]), None, window, cx));

        let image_credential_select =
            cx.new(|cx| SelectState::new(SearchableVec::new(vec![]), None, window, cx));

        let weak_self = cx.entity().downgrade();
        let on_click = Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
            weak_self
                .update(cx, |this, cx| {
                    // Update selection state FIRST to avoid input observer overwriting old profile
                    this.selected_index = Some(index);
                    this.list_state.update(cx, |list, cx| {
                        list.delegate_mut().selected_index = Some(index);
                        cx.notify();
                    });

                    // THEN load the profile (which updates inputs and triggers observers)
                    this.load_profile(index, window, cx);
                })
                .ok();
        });

        let list_delegate = ProfileListDelegate::new(vec![], on_click);
        let list_state = cx.new(|cx| ListState::new(list_delegate, window, cx));

        let new_chat_cred_input = cx.new(|cx| InputState::new(window, cx).placeholder("API Key"));
        let new_embedding_cred_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("API Key"));
        let new_image_cred_input = cx.new(|cx| InputState::new(window, cx).placeholder("API Key"));

        let _window_width = window.viewport_size().width;

        let state_clone = state.clone();

        let mut this = Self {
            state,
            profile_name_input,
            provider_select,
            model_select,
            embedding_provider_select,
            embedding_model_select,
            image_provider_select,
            image_model_select,
            tts_provider_select,
            tts_model_select,
            chat_credential_select,
            embedding_credential_select,
            image_credential_select,
            list_state,
            selected_index: None,
            error_message: None,
            sidebar_open: true,
            last_window_width: None,
            credentials: Vec::new(),
            mode: ProfileMode::Creating,
            needs_reset: false,
            creating_chat_cred: false,
            creating_embedding_cred: false,
            creating_image_cred: false,
            new_chat_cred_input,
            new_embedding_cred_input,
            new_image_cred_input,
            pending_credential_id: None,
            should_focus_chat: false,
            should_focus_embedding: false,
            should_focus_image: false,
            is_saving: false,
            show_form: false,
            is_new_mode: false,
            pending_success_message: None,
        };

        this.fetch_credentials(cx);
        this.fetch_profiles(None, cx);
        this.subscribe_to_selects(cx);

        // Observe list state for other things if needed, but not for dirty check on selection
        cx.observe(&this.list_state, |this, list, cx| {
            // Keep selected_index in sync if needed, but we handle it manually now
            let selected_index = list.read(cx).delegate().selected_index;
            if selected_index != this.selected_index {
                this.selected_index = selected_index;
                // We don't load profile here anymore, we do it in on_click
            }
        })
        .detach();

        // Update provider selects after models are fetched
        this.update_provider_selects(cx);

        // Subscribe to modal open state to refresh data
        cx.observe(&state_clone, |this: &mut Self, state, cx| {
            if state.read(cx).is_profile_settings_open {
                this.fetch_credentials(cx);
                this.fetch_profiles(None, cx);
            }
        })
        .detach();

        this
    }

    fn load_profile(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // If we were creating a profile and switched away, remove the ephemeral one
        if self.mode == ProfileMode::Creating {
            self.list_state.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                if let Some(pos) = delegate.profiles.iter().position(|p| p.id == -1) {
                    if pos != index {
                        delegate.profiles.remove(pos);
                        cx.notify();
                    }
                }
            });
        }

        let profile = {
            let list_state = self.list_state.read(cx);
            let delegate = list_state.delegate();
            // Adjust index if we removed an item
            let adjusted_index = if self.mode == ProfileMode::Creating {
                // If we removed the last item (ephemeral), and the requested index was before it, it's fine.
                // If the requested index was the ephemeral one (which shouldn't happen here as we're loading *another*),
                // we need to be careful.
                // Actually, since we remove by ID -1, and that's usually at the end,
                // indices of existing profiles shouldn't shift unless -1 was inserted in the middle (unlikely).
                // However, if we clicked the ephemeral profile itself, we shouldn't remove it.
                // The check `pos != index` above handles that.
                // But wait, if we remove an item, the `index` passed in might be invalid if it was > pos.
                // Since ephemeral is added to the end, existing items are safe.
                index
            } else {
                index
            };

            delegate.profiles.get(adjusted_index).cloned()
        };

        if let Some(profile) = profile {
            self.mode = ProfileMode::Editing(profile.id);
            self.show_form = true;
            self.is_new_mode = false;

            // Populate profile name
            self.profile_name_input.update(cx, |input, cx| {
                input.set_value(&profile.name, window, cx);
            });

            // Set providers based on model IDs
            if let Some(mid) = &profile.text_model_id {
                let state = self.state.read(cx);
                if let Some(model) = state.available_models.iter().find(|m| &m.id == mid) {
                    let provider = model.provider.clone();
                    self.provider_select
                        .update(cx, |s, cx| s.set_selected_value(&provider, window, cx));
                }
            }

            if let Some(mid) = &profile.embedding_model_id {
                let state = self.state.read(cx);
                if let Some(model) = state.available_models.iter().find(|m| &m.id == mid) {
                    let provider = model.provider.clone();
                    self.embedding_provider_select
                        .update(cx, |s, cx| s.set_selected_value(&provider, window, cx));
                }
            }

            if let Some(mid) = &profile.image_model_id {
                let state = self.state.read(cx);
                if let Some(model) = state.available_models.iter().find(|m| &m.id == mid) {
                    let provider = model.provider.clone();
                    self.image_provider_select
                        .update(cx, |s, cx| s.set_selected_value(&provider, window, cx));
                }
            }

            if let Some(mid) = &profile.tts_model_id {
                let state = self.state.read(cx);
                if let Some(model) = state.available_models.iter().find(|m| &m.id == mid) {
                    let provider = model.provider.clone();
                    self.tts_provider_select
                        .update(cx, |s, cx| s.set_selected_value(&provider, window, cx));
                }
            }

            // Update model and credential selects based on providers
            self.update_model_selects(cx, false);
            self.update_credential_selects(cx);

            // Set models
            if let Some(model_id) = &profile.text_model_id {
                self.model_select
                    .update(cx, |s, cx| s.set_selected_value(model_id, window, cx));
            }
            if let Some(model_id) = &profile.embedding_model_id {
                self.embedding_model_select
                    .update(cx, |s, cx| s.set_selected_value(model_id, window, cx));
            }
            if let Some(model_id) = &profile.image_model_id {
                self.image_model_select
                    .update(cx, |s, cx| s.set_selected_value(model_id, window, cx));
            }
            if let Some(model_id) = &profile.tts_model_id {
                self.tts_model_select
                    .update(cx, |s, cx| s.set_selected_value(model_id, window, cx));
            }

            // Update credential selects again after models are set
            self.update_credential_selects(cx);

            // Set chat credential
            if let Some(cred_id) = profile.text_credential_id {
                if let Some(cred) = self.credentials.iter().find(|c| c.id == cred_id) {
                    let item = CredentialItem(cred.clone());
                    self.chat_credential_select
                        .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                } else {
                    self.chat_credential_select
                        .update(cx, |s, cx| s.reset_selection(cx));
                }
            } else {
                // Try to use system credential as fallback
                let mut found = false;
                if let Some(model_id) = self.model_select.read(cx).selected_value() {
                    let state = self.state.read(cx);
                    if let Some(model) = state.available_models.iter().find(|m| &m.id == model_id) {
                        let provider_str = model.provider.to_string();
                        if let Some(sys_cred) = self
                            .credentials
                            .iter()
                            .find(|c| c.id < 0 && c.provider == provider_str)
                        {
                            let item = CredentialItem(sys_cred.clone());
                            self.chat_credential_select
                                .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                            found = true;
                        }
                    }
                }
                if !found {
                    self.chat_credential_select
                        .update(cx, |s, cx| s.reset_selection(cx));
                }
            }

            // Set embedding credential
            if let Some(cred_id) = profile.embedding_credential_id {
                if let Some(cred) = self.credentials.iter().find(|c| c.id == cred_id) {
                    let item = CredentialItem(cred.clone());
                    self.embedding_credential_select
                        .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                } else {
                    self.embedding_credential_select
                        .update(cx, |s, cx| s.reset_selection(cx));
                }
            } else {
                // Try to use system credential as fallback
                let mut found = false;
                if let Some(model_id) = self.embedding_model_select.read(cx).selected_value() {
                    let state = self.state.read(cx);
                    if let Some(model) = state.available_models.iter().find(|m| &m.id == model_id) {
                        let provider_str = model.provider.to_string();
                        if let Some(sys_cred) = self
                            .credentials
                            .iter()
                            .find(|c| c.id < 0 && c.provider == provider_str)
                        {
                            let item = CredentialItem(sys_cred.clone());
                            self.embedding_credential_select
                                .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                            found = true;
                        }
                    }
                }
                if !found {
                    self.embedding_credential_select
                        .update(cx, |s, cx| s.reset_selection(cx));
                }
            }

            // Set image credential
            if let Some(cred_id) = profile.image_credential_id {
                if let Some(cred) = self.credentials.iter().find(|c| c.id == cred_id) {
                    let item = CredentialItem(cred.clone());
                    self.image_credential_select
                        .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                } else {
                    self.image_credential_select
                        .update(cx, |s, cx| s.reset_selection(cx));
                }
            } else {
                // Try to use system credential as fallback
                let mut found = false;
                if let Some(model_id) = self.image_model_select.read(cx).selected_value() {
                    let state = self.state.read(cx);
                    if let Some(model) = state.available_models.iter().find(|m| &m.id == model_id) {
                        let provider_str = model.provider.to_string();
                        if let Some(sys_cred) = self
                            .credentials
                            .iter()
                            .find(|c| c.id < 0 && c.provider == provider_str)
                        {
                            let item = CredentialItem(sys_cred.clone());
                            self.image_credential_select
                                .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                            found = true;
                        }
                    }
                }
                if !found {
                    self.image_credential_select
                        .update(cx, |s, cx| s.reset_selection(cx));
                }
            }
            cx.notify();
        }
    }

    fn fetch_credentials(&mut self, cx: &mut Context<Self>) {
        let db = self.state.read(cx).database_service.clone();
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let mut credentials = if let Some(db) = db {
                    db.get_credentials().await.unwrap_or_default()
                } else {
                    Vec::new()
                };

                // Inject System Credentials from Env Vars
                if std::env::var("GEMINI_API_KEY").is_ok() {
                    credentials.push(Credential {
                        id: -1,
                        name: "Gemini (System)".to_string(),
                        provider: "Google Gemini".to_string(),
                        api_key: String::new(), // Not needed for display/logic here
                        created_at: String::new(),
                    });
                }
                if std::env::var("OPENAI_API_KEY").is_ok() {
                    credentials.push(Credential {
                        id: -2,
                        name: "OpenAI (System)".to_string(),
                        provider: "OpenAI".to_string(),
                        api_key: String::new(),
                        created_at: String::new(),
                    });
                }
                if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                    credentials.push(Credential {
                        id: -3,
                        name: "Anthropic (System)".to_string(),
                        provider: "Anthropic".to_string(),
                        api_key: String::new(),
                        created_at: String::new(),
                    });
                }

                this.update(&mut cx, |this, cx| {
                    this.credentials = credentials;
                    this.update_credential_selects(cx);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
    fn fetch_profiles(&mut self, select_id: Option<i64>, cx: &mut Context<Self>) {
        let db = self.state.read(cx).database_service.clone();
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let profiles = if let Some(db) = db {
                    db.get_profiles().await.ok()
                } else {
                    None
                };

                if let Some(profiles) = profiles {
                    this.update(&mut cx, |this, cx| {
                        this.list_state.update(cx, |list, cx| {
                            list.delegate_mut().profiles = profiles;

                            // Only select a profile if we're explicitly given an ID to select
                            // (e.g., after saving a profile). Don't auto-select first profile.
                            if let Some(id) = select_id {
                                if let Some(index) =
                                    list.delegate().profiles.iter().position(|p| p.id == id)
                                {
                                    list.delegate_mut().selected_index = Some(index);
                                    cx.notify();
                                }
                            }
                        });

                        // If we have a specific profile to select, load it
                        // Note: We can't call load_profile here because we don't have a Window reference
                        // The profile will be loaded when the user clicks it in the sidebar
                        // No auto-load on initial open - user must click a profile
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn delete_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = match self.mode {
            ProfileMode::Editing(id) => id,
            _ => return,
        };

        let db = self.state.read(cx).database_service.clone();
        let weak_self = cx.entity().downgrade();

        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Delete Profile")
                .child(div().child(
                    "Are you sure you want to delete this profile? This action cannot be undone.",
                ))
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Delete"),
                )
                .on_ok({
                    let db = db.clone();
                    let weak_self = weak_self.clone();
                    move |_, _, cx| {
                        let db = db.clone();
                        let weak_self = weak_self.clone();
                        cx.spawn(move |cx: &mut AsyncApp| {
                            let mut cx = cx.clone();
                            async move {
                                if let Some(db) = db {
                                    if let Err(e) = db.delete_profile(id).await {
                                        eprintln!("Failed to delete profile: {}", e);
                                    } else {
                                        weak_self
                                            .update(&mut cx, |this, cx| {
                                                this.fetch_profiles(None, cx);
                                                this.needs_reset = true;
                                                cx.notify();
                                            })
                                            .ok();
                                    }
                                }
                            }
                        })
                        .detach();

                        true
                    }
                })
        });
    }

    fn create_new_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = ProfileMode::Creating;
        self.show_form = true;
        self.is_new_mode = true;
        self.error_message = None;

        // Clear inputs
        self.profile_name_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });

        // Clear all provider selects
        self.provider_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.embedding_provider_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.image_provider_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));

        // Clear all model selects
        self.model_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.embedding_model_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.image_model_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.tts_provider_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.tts_model_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));

        // Clear all credential selects
        self.chat_credential_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.embedding_credential_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));
        self.image_credential_select
            .update(cx, |s, cx| s.set_selected_index(None, window, cx));

        // Add ephemeral "Untitled" profile
        self.list_state.update(cx, |list, cx| {
            let delegate = list.delegate_mut();

            // Remove any existing ephemeral profiles first
            delegate.profiles.retain(|p| p.id != -1);

            let new_profile = Profile {
                id: -1,
                name: "Untitled".to_string(),
                text_model_id: None,
                text_credential_id: None,
                embedding_model_id: None,
                embedding_credential_id: None,
                image_model_id: None,
                image_credential_id: None,
                tts_model_id: None,
                created_at: String::new(),
            };

            delegate.profiles.push(new_profile);
            let new_index = delegate.profiles.len() - 1;

            // Update ListState's selected_index via set_selected_index
            list.set_selected_index(Some(IndexPath::default().row(new_index)), window, cx);
            self.selected_index = Some(new_index);

            cx.notify();
        });

        cx.notify();
    }

    fn save_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(msg) = self.validate_profile(cx) {
            self.error_message = Some(msg.clone());
            window.push_notification(Notification::error(msg), cx);
            cx.notify();
            return;
        }
        self.error_message = None;
        self.is_saving = true;
        cx.notify();

        let name = self.profile_name_input.read(cx).value().to_string();

        let text_credential_id = self
            .chat_credential_select
            .read(cx)
            .selected_value()
            .map(|c| if c.id < 0 { None } else { Some(c.id) })
            .flatten();
        let embedding_credential_id = self
            .embedding_credential_select
            .read(cx)
            .selected_value()
            .map(|c| if c.id < 0 { None } else { Some(c.id) })
            .flatten();
        let image_credential_id = self
            .image_credential_select
            .read(cx)
            .selected_value()
            .map(|c| if c.id < 0 { None } else { Some(c.id) })
            .flatten();

        let text_model_id = self
            .model_select
            .read(cx)
            .selected_value()
            .map(|m| m.clone());
        let embedding_model_id = self
            .embedding_model_select
            .read(cx)
            .selected_value()
            .map(|m| m.clone());
        let image_model_id = self
            .image_model_select
            .read(cx)
            .selected_value()
            .map(|m| m.clone());
        let tts_model_id = self
            .tts_model_select
            .read(cx)
            .selected_value()
            .map(|m| m.clone());

        let mode = self.mode.clone();
        let db = self.state.read(cx).database_service.clone();

        println!("Saving profile. Mode: {:?}, Name: {}", mode, name);
        println!(
            "IDs - Chat: {:?}/{:?}, Embed: {:?}/{:?}, Image: {:?}/{:?}",
            text_model_id,
            text_credential_id,
            embedding_model_id,
            embedding_credential_id,
            image_model_id,
            image_credential_id
        );

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = if let Some(db) = db {
                    match mode {
                        ProfileMode::Creating => {
                            // Create profile first to get ID
                            match db.create_profile(&name).await {
                                Ok(id) => {
                                    // Now update with other fields
                                    let profile = Profile {
                                        id,
                                        name,
                                        text_model_id,
                                        text_credential_id,
                                        embedding_model_id,
                                        embedding_credential_id,
                                        image_model_id,
                                        image_credential_id,
                                        tts_model_id,
                                        created_at: String::new(),
                                    };

                                    if let Err(e) = db.update_profile(&profile).await {
                                        Err(format!("Failed to update new profile: {}", e))
                                    } else {
                                        Ok(Some(id))
                                    }
                                }
                                Err(e) => Err(format!("Failed to create profile: {}", e)),
                            }
                        }
                        ProfileMode::Editing(id) => {
                            let profile = Profile {
                                id,
                                name,
                                text_model_id,
                                text_credential_id,
                                embedding_model_id,
                                embedding_credential_id,
                                image_model_id,
                                image_credential_id,
                                tts_model_id: tts_model_id.clone(),
                                created_at: String::new(),
                            };

                            if let Err(e) = db.update_profile(&profile).await {
                                Err(format!("Failed to update profile: {}", e))
                            } else {
                                Ok(Some(id))
                            }
                        }
                    }
                } else {
                    Err("Database service not available".to_string())
                };

                this.update(&mut cx, |this, cx| {
                    match result {
                        Ok(Some(id)) => {
                            println!("Profile saved successfully. ID: {}", id);
                            this.mode = ProfileMode::Editing(id);
                            this.is_new_mode = false;
                            this.pending_success_message =
                                Some("Profile saved successfully".to_string());
                            this.fetch_profiles(Some(id), cx);

                            // Reload global state to ensure credentials and profile names are updated everywhere
                            this.state.update(cx, |state, cx| {
                                state.reload_from_db(cx);
                            });

                            // Reset UI to empty state
                            this.show_form = false;
                            this.selected_index = None;
                            this.list_state.update(cx, |list, cx| {
                                list.delegate_mut().selected_index = None;
                                cx.notify();
                            });
                        }
                        Ok(None) => {
                            // Should not happen with current logic but handle anyway
                            println!("Profile saved (no ID change).");
                            this.pending_success_message = Some("Profile saved".to_string());
                            this.fetch_profiles(None, cx);

                            // Reload global state
                            this.state.update(cx, |state, cx| {
                                state.reload_from_db(cx);
                            });

                            // Reset UI to empty state
                            this.show_form = false;
                            this.selected_index = None;
                            this.list_state.update(cx, |list, cx| {
                                list.delegate_mut().selected_index = None;
                                cx.notify();
                            });
                        }
                        Err(e) => {
                            eprintln!("Save failed: {}", e);
                            this.error_message = Some(e);
                        }
                    }
                    this.is_saving = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    fn subscribe_to_selects(&mut self, cx: &mut Context<Self>) {
        // Observe input changes for live name update
        cx.observe(&self.profile_name_input, |this, input, cx| {
            let name = input.read(cx).value();
            if let Some(selected_index) = this.selected_index {
                this.list_state.update(cx, |list, cx| {
                    if let Some(profile) = list.delegate_mut().profiles.get_mut(selected_index) {
                        profile.name = if name.is_empty() {
                            "Untitled".to_string()
                        } else {
                            name.to_string()
                        };
                        cx.notify();
                    }
                });
            }
        })
        .detach();

        // Subscribe to model selects to update credential lists
        let subscribe_model_select = |select: &Entity<SelectState<SearchableVec<ModelProfile>>>,
                                      cx: &mut Context<Self>| {
            cx.subscribe(
                select,
                |this, _, _event: &SelectEvent<SearchableVec<ModelProfile>>, cx| {
                    this.update_credential_selects(cx);
                },
            )
            .detach();
        };

        subscribe_model_select(&self.model_select, cx);
        subscribe_model_select(&self.embedding_model_select, cx);
        subscribe_model_select(&self.image_model_select, cx);

        // Subscribe to credential selects to handle "Create New"
        cx.subscribe(
            &self.chat_credential_select,
            |this, _, event: &SelectEvent<SearchableVec<CredentialItem>>, cx| {
                if let SelectEvent::Confirm(Some(cred)) = event {
                    if cred.id == -999 {
                        this.creating_chat_cred = true;
                        this.should_focus_chat = true;
                        cx.notify();
                    }
                }
            },
        )
        .detach();

        cx.subscribe(
            &self.embedding_credential_select,
            |this, _, event: &SelectEvent<SearchableVec<CredentialItem>>, cx| {
                if let SelectEvent::Confirm(Some(cred)) = event {
                    if cred.id == -999 {
                        this.creating_embedding_cred = true;
                        this.should_focus_embedding = true;
                        cx.notify();
                    }
                }
            },
        )
        .detach();

        cx.subscribe(
            &self.image_credential_select,
            |this, _, event: &SelectEvent<SearchableVec<CredentialItem>>, cx| {
                if let SelectEvent::Confirm(Some(cred)) = event {
                    if cred.id == -999 {
                        this.creating_image_cred = true;
                        this.should_focus_image = true;
                        cx.notify();
                    }
                }
            },
        )
        .detach();

        // Subscribe to chat provider select - reset model and credential on change
        cx.subscribe(
            &self.provider_select,
            |this, _, _event: &SelectEvent<SearchableVec<Provider>>, cx| {
                // Reset model and credential when provider changes
                this.model_select.update(cx, |s, cx| s.reset_selection(cx));
                this.chat_credential_select
                    .update(cx, |s, cx| s.reset_selection(cx));
                this.update_model_selects(cx, true);
            },
        )
        .detach();

        // Subscribe to embedding provider select - reset model and credential on change
        cx.subscribe(
            &self.embedding_provider_select,
            |this, _, _event: &SelectEvent<SearchableVec<Provider>>, cx| {
                this.embedding_model_select
                    .update(cx, |s, cx| s.reset_selection(cx));
                this.embedding_credential_select
                    .update(cx, |s, cx| s.reset_selection(cx));
                this.update_model_selects(cx, true);
            },
        )
        .detach();

        // Subscribe to image provider select - reset model and credential on change
        cx.subscribe(
            &self.image_provider_select,
            |this, _, _event: &SelectEvent<SearchableVec<Provider>>, cx| {
                this.image_model_select
                    .update(cx, |s, cx| s.reset_selection(cx));
                this.image_credential_select
                    .update(cx, |s, cx| s.reset_selection(cx));
                this.update_model_selects(cx, true);
            },
        )
        .detach();

        // Note: We don't observe state to update selects - they're updated via provider select subscriptions above
    }

    fn update_provider_selects(&mut self, cx: &mut Context<Self>) {
        let (chat_providers, embedding_providers, image_providers, tts_providers) = {
            let state = self.state.read(cx);
            let models = &state.available_models;

            let get_providers = |m_type: ModelType| {
                let mut providers: Vec<Provider> = models
                    .iter()
                    .filter(|m| m.model_type == m_type)
                    .map(|m| m.provider.clone())
                    .collect();
                providers.sort_by_key(|p| p.to_string());
                providers.dedup();
                providers
            };

            let get_tts_providers = || {
                let mut providers: Vec<Provider> = models
                    .iter()
                    .filter(|m| m.capabilities.supports_text_to_speech)
                    .map(|m| m.provider.clone())
                    .collect();
                providers.sort_by_key(|p| p.to_string());
                providers.dedup();
                providers
            };

            (
                get_providers(ModelType::TextGeneration),
                get_providers(ModelType::TextEmbedding),
                get_providers(ModelType::ImageGeneration),
                get_tts_providers(),
            )
        };

        println!(
            "Updating provider selects: Chat={}, Embedding={}, Image={}, TTS={}",
            chat_providers.len(),
            embedding_providers.len(),
            image_providers.len(),
            tts_providers.len()
        );

        self.provider_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(chat_providers), cx);
        });

        self.embedding_provider_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(embedding_providers), cx);
        });

        self.image_provider_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(image_providers), cx);
        });

        self.tts_provider_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(tts_providers), cx);
        });
    }

    fn update_model_selects(&mut self, cx: &mut Context<Self>, reset_selection: bool) {
        let chat_provider = self.provider_select.read(cx).selected_value().cloned();
        let embedding_provider = self
            .embedding_provider_select
            .read(cx)
            .selected_value()
            .cloned();
        let image_provider = self
            .image_provider_select
            .read(cx)
            .selected_value()
            .cloned();
        let tts_provider = self.tts_provider_select.read(cx).selected_value().cloned();

        let (chat_models, embedding_models, image_models, tts_models) = {
            let state = self.state.read(cx);
            let models = &state.available_models;

            let filter_models = |m_type: ModelType, provider: Option<&Provider>| {
                models
                    .iter()
                    .filter(|m| m.model_type == m_type)
                    .filter(|m| provider.map_or(true, |p| m.provider == *p))
                    .cloned()
                    .collect::<Vec<_>>()
            };

            let filter_tts_models = |provider: Option<&Provider>| {
                models
                    .iter()
                    .filter(|m| m.capabilities.supports_text_to_speech)
                    .filter(|m| provider.map_or(true, |p| m.provider == *p))
                    .cloned()
                    .collect::<Vec<_>>()
            };

            (
                filter_models(ModelType::TextGeneration, chat_provider.as_ref()),
                filter_models(ModelType::TextEmbedding, embedding_provider.as_ref()),
                filter_models(ModelType::ImageGeneration, image_provider.as_ref()),
                filter_tts_models(tts_provider.as_ref()),
            )
        };

        println!(
            "Updating model selects: Chat={}, Embedding={}, Image={}, TTS={}",
            chat_models.len(),
            embedding_models.len(),
            image_models.len(),
            tts_models.len()
        );

        self.model_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(chat_models), cx);
            if reset_selection {
                select.reset_selection(cx);
            }
        });

        self.embedding_model_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(embedding_models), cx);
            if reset_selection {
                select.reset_selection(cx);
            }
        });

        self.image_model_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(image_models), cx);
            if reset_selection {
                select.reset_selection(cx);
            }
        });

        self.tts_model_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(tts_models), cx);
            if reset_selection {
                select.reset_selection(cx);
            }
        });
    }
    fn validate_profile(&self, cx: &App) -> Result<(), String> {
        let name = self.profile_name_input.read(cx).value();
        if name.trim().is_empty() {
            return Err("Profile name cannot be empty".to_string());
        }
        if name.chars().count() > 30 {
            return Err("Profile name cannot exceed 30 characters".to_string());
        }

        // Check Chat Model
        if self.model_select.read(cx).selected_value().is_none() {
            return Err("Please select a Chat Model".to_string());
        }
        if self
            .chat_credential_select
            .read(cx)
            .selected_value()
            .is_none()
        {
            return Err("Please select a Chat Credential".to_string());
        }

        // Check Embedding Model
        if self
            .embedding_model_select
            .read(cx)
            .selected_value()
            .is_none()
        {
            return Err("Please select an Embedding Model".to_string());
        }
        if self
            .embedding_credential_select
            .read(cx)
            .selected_value()
            .is_none()
        {
            return Err("Please select an Embedding Credential".to_string());
        }

        // Check Image Model
        if self.image_model_select.read(cx).selected_value().is_none() {
            return Err("Please select an Image Model".to_string());
        }
        if self
            .image_credential_select
            .read(cx)
            .selected_value()
            .is_none()
        {
            return Err("Please select an Image Credential".to_string());
        }

        Ok(())
    }

    fn update_credential_selects(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let chat_provider = self.model_select.read(cx).selected_value().and_then(|id| {
            state
                .available_models
                .iter()
                .find(|m| &m.id == id)
                .map(|m| m.provider.clone())
        });

        let embedding_provider = self
            .embedding_model_select
            .read(cx)
            .selected_value()
            .and_then(|id| {
                state
                    .available_models
                    .iter()
                    .find(|m| &m.id == id)
                    .map(|m| m.provider.clone())
            });

        let image_provider = self
            .image_model_select
            .read(cx)
            .selected_value()
            .and_then(|id| {
                state
                    .available_models
                    .iter()
                    .find(|m| &m.id == id)
                    .map(|m| m.provider.clone())
            });

        println!(
            "Updating credential selects. Providers: Chat={:?}, Embedding={:?}, Image={:?}",
            chat_provider, embedding_provider, image_provider
        );

        let all_creds = self.credentials.clone();

        // Helper to filter and add "Create New"
        let filter_creds = |provider: Option<Provider>| {
            if provider.is_none() {
                return SearchableVec::new(vec![]);
            }

            let mut filtered: Vec<CredentialItem> = all_creds
                .iter()
                .filter(|c| {
                    if let Some(p) = &provider {
                        c.provider.eq_ignore_ascii_case(&p.to_string())
                    } else {
                        false
                    }
                })
                .map(|c| CredentialItem(c.clone()))
                .collect();

            if let Some(p) = provider {
                filtered.push(CredentialItem(Credential {
                    id: -999,
                    name: format!("Create new {:?} credential...", p),
                    provider: format!("{:?}", p),
                    api_key: String::new(),
                    created_at: String::new(),
                }));
            }

            SearchableVec::new(filtered)
        };

        let chat_creds = filter_creds(chat_provider);
        self.chat_credential_select.update(cx, |select, cx| {
            select.set_items(chat_creds, cx);
        });

        let embedding_creds = filter_creds(embedding_provider);
        self.embedding_credential_select.update(cx, |select, cx| {
            select.set_items(embedding_creds, cx);
        });

        let image_creds = filter_creds(image_provider);
        self.image_credential_select.update(cx, |select, cx| {
            select.set_items(image_creds, cx);
        });
    }

    fn save_new_credential(
        &mut self,
        input: Entity<InputState>,
        model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
        _cred_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
        cx: &mut Context<Self>,
    ) {
        let api_key = input.read(cx).value();
        if api_key.trim().is_empty() {
            return;
        }

        let state = self.state.read(cx);
        let provider = model_select.read(cx).selected_value().and_then(|id| {
            state
                .available_models
                .iter()
                .find(|m| &m.id == id)
                .map(|m| m.provider.clone())
        });

        if let Some(provider) = provider {
            if let Some(db) = &state.database_service {
                let db = db.clone();
                let provider_str = format!("{:?}", provider);
                let name = format!("{} Credential", provider_str);
                let api_key = api_key.to_string();

                cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        match db.create_credential(&name, &provider_str, &api_key).await {
                            Ok(id) => {
                                view.update(&mut cx, |this, cx| {
                                    this.creating_chat_cred = false;
                                    this.creating_embedding_cred = false;
                                    this.creating_image_cred = false;
                                    this.pending_credential_id = Some(id);
                                    this.fetch_credentials(cx);
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to create credential: {}", e),
                        }
                    }
                })
                .detach();
            }
        }
    }

    fn render_model_section(
        title: impl Into<SharedString>,
        provider_select: Entity<SelectState<SearchableVec<Provider>>>,
        model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
        credential_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
        creating_cred: bool,
        new_cred_input: Entity<InputState>,
        save_action: &'static str,
        cancel_action: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(Label::new(title).font_weight(FontWeight::MEDIUM))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(
                        div().flex_1().min_w_64().child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    Label::new("Provider")
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(
                                    Select::new(&provider_select).placeholder("Select Provider"),
                                ),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_64().child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    Label::new("Model")
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(Select::new(&model_select).placeholder("Select Model")),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_64().child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    Label::new("Credential")
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(if creating_cred {
                                    h_flex()
                                        .gap_2()
                                        .child(Input::new(&new_cred_input).flex_1())
                                        .child(Button::new(save_action).label("Save").on_click(
                                            cx.listener(
                                                move |this, _, window, cx| match save_action {
                                                    "save_chat_cred" => this.save_chat_cred(
                                                        &ClickEvent::default(),
                                                        window,
                                                        cx,
                                                    ),
                                                    "save_embedding_cred" => {
                                                        this.save_embedding_cred(
                                                            &ClickEvent::default(),
                                                            window,
                                                            cx,
                                                        )
                                                    }
                                                    "save_image_cred" => this.save_image_cred(
                                                        &ClickEvent::default(),
                                                        window,
                                                        cx,
                                                    ),
                                                    _ => {}
                                                },
                                            ),
                                        ))
                                        .child(
                                            Button::new(cancel_action)
                                                .label("Cancel")
                                                .ghost()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    match cancel_action {
                                                        "cancel_chat_cred" => {
                                                            this.creating_chat_cred = false;
                                                            cx.notify();
                                                        }
                                                        "cancel_embedding_cred" => {
                                                            this.creating_embedding_cred = false;
                                                            cx.notify();
                                                        }
                                                        "cancel_image_cred" => {
                                                            this.creating_image_cred = false;
                                                            cx.notify();
                                                        }
                                                        _ => {}
                                                    }
                                                })),
                                        )
                                        .into_any_element()
                                } else {
                                    Select::new(&credential_select)
                                        .placeholder("Select Credential")
                                        .into_any_element()
                                }),
                        ),
                    ),
            )
    }

    fn save_chat_cred(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let input = self.new_chat_cred_input.clone();
        let model_select = self.model_select.clone();
        let cred_select = self.chat_credential_select.clone();
        self.save_new_credential(input, model_select, cred_select, cx);
    }

    fn save_embedding_cred(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.new_embedding_cred_input.clone();
        let model_select = self.embedding_model_select.clone();
        let cred_select = self.embedding_credential_select.clone();
        self.save_new_credential(input, model_select, cred_select, cx);
    }

    fn save_image_cred(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let input = self.new_image_cred_input.clone();
        let model_select = self.image_model_select.clone();
        let cred_select = self.image_credential_select.clone();
        self.save_new_credential(input, model_select, cred_select, cx);
    }
}

impl Render for ProfileSettingsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Handle pending success notification
        if let Some(msg) = self.pending_success_message.take() {
            window.push_notification(Notification::success(msg), cx);
        }

        if self.needs_reset {
            self.create_new_profile(window, cx);
            self.needs_reset = false;
        }

        // Handle focus requests
        if self.should_focus_chat {
            self.new_chat_cred_input
                .read(cx)
                .focus_handle()
                .focus(window);
            self.should_focus_chat = false;
        }
        if self.should_focus_embedding {
            self.new_embedding_cred_input
                .read(cx)
                .focus_handle()
                .focus(window);
            self.should_focus_embedding = false;
        }
        if self.should_focus_image {
            self.new_image_cred_input
                .read(cx)
                .focus_handle()
                .focus(window);
            self.should_focus_image = false;
        }

        // Handle pending credential selection
        if let Some(id) = self.pending_credential_id {
            if let Some(cred) = self.credentials.iter().find(|c| c.id == id) {
                let item = CredentialItem(cred.clone());
                self.chat_credential_select
                    .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                self.embedding_credential_select
                    .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
                self.image_credential_select
                    .update(cx, |s, cx| s.set_selected_value(&item.0, window, cx));
            }
            self.pending_credential_id = None;
        }

        let window_width = window.viewport_size().width;
        let is_small_screen = window_width < px(650.0);

        // Auto-hide/show sidebar on resize
        if let Some(last_width) = self.last_window_width {
            if last_width >= px(650.0) && window_width < px(650.0) {
                self.sidebar_open = false;
            } else if last_width < px(650.0) && window_width >= px(650.0) {
                self.sidebar_open = true;
            }
        }
        self.last_window_width = Some(window_width);

        let sidebar = if self.sidebar_open {
            let mut sidebar_div = div()
                .w_72()
                .border_r_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .flex()
                .flex_col()
                .child(
                    div()
                        .p_4()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(Label::new("Profiles").font_weight(FontWeight::BOLD))
                        .child(
                            Button::new("new_profile")
                                .icon(IconName::Plus)
                                .ghost()
                                .tooltip("Create New Profile")
                                .disabled(self.is_new_mode)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.create_new_profile(window, cx);
                                })),
                        ),
                )
                .child(div().flex_1().child(
                    if self.list_state.read(cx).delegate().profiles.len() > 0 {
                        List::new(&self.list_state)
                            .list_size(UiSize::Small)
                            .with_size(UiSize::Small)
                            .h_full()
                            .w_full()
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .h_full()
                            .p_4()
                            .gap_2()
                            .child(
                                Label::new("No profiles yet")
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                Label::new("Create a new profile to get started")
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .text_align(TextAlign::Center),
                            )
                            .into_any_element()
                    },
                ));

            if is_small_screen {
                sidebar_div = sidebar_div
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .occlude()
                    .shadow_lg();
            }

            sidebar_div
        } else {
            div().hidden()
        };

        let main_content = if self.show_form {
            div()
                .flex_1()
                .p_6()
                .flex()
                .flex_col()
                .gap_8()
                .scrollable(ScrollbarAxis::Vertical)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            Label::new("General")
                                .font_weight(FontWeight::BOLD)
                                .text_lg(),
                        )
                        .child(
                            div().child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .when_some(self.error_message.clone(), |div, msg| {
                                        div.child(Label::new(msg).text_color(cx.theme().danger))
                                    })
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                Label::new("Profile Name")
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM),
                                            )
                                            .child(
                                                Label::new(format!(
                                                    "{}/30",
                                                    self.profile_name_input
                                                        .read(cx)
                                                        .value()
                                                        .chars()
                                                        .count()
                                                ))
                                                .text_xs()
                                                .text_color(
                                                    if self
                                                        .profile_name_input
                                                        .read(cx)
                                                        .value()
                                                        .chars()
                                                        .count()
                                                        > 30
                                                    {
                                                        cx.theme().danger
                                                    } else {
                                                        cx.theme().muted_foreground
                                                    },
                                                ),
                                            ),
                                    )
                                    .child(Input::new(&self.profile_name_input)),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            Label::new("Model Configuration")
                                .font_weight(FontWeight::BOLD)
                                .text_lg(),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_6()
                                .child(Self::render_model_section(
                                    "Chat Model",
                                    self.provider_select.clone(),
                                    self.model_select.clone(),
                                    self.chat_credential_select.clone(),
                                    self.creating_chat_cred,
                                    self.new_chat_cred_input.clone(),
                                    "save_chat_cred",
                                    "cancel_chat_cred",
                                    cx,
                                ))
                                .child(Self::render_model_section(
                                    "Embedding Model",
                                    self.embedding_provider_select.clone(),
                                    self.embedding_model_select.clone(),
                                    self.embedding_credential_select.clone(),
                                    self.creating_embedding_cred,
                                    self.new_embedding_cred_input.clone(),
                                    "save_embedding_cred",
                                    "cancel_embedding_cred",
                                    cx,
                                ))
                                .child(Self::render_model_section(
                                    "Image Model",
                                    self.image_provider_select.clone(),
                                    self.image_model_select.clone(),
                                    self.image_credential_select.clone(),
                                    self.creating_image_cred,
                                    self.new_image_cred_input.clone(),
                                    "save_image_cred",
                                    "cancel_image_cred",
                                    cx,
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_3()
                                        .child(
                                            Label::new("TTS Model").font_weight(FontWeight::MEDIUM),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .gap_4()
                                                .child(
                                                    div().flex_1().min_w_64().child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .gap_1()
                                                            .child(
                                                                Label::new("Provider")
                                                                    .text_xs()
                                                                    .text_color(
                                                                        cx.theme().muted_foreground,
                                                                    ),
                                                            )
                                                            .child(
                                                                Select::new(
                                                                    &self.tts_provider_select,
                                                                )
                                                                .placeholder("Select Provider"),
                                                            ),
                                                    ),
                                                )
                                                .child(
                                                    div().flex_1().min_w_64().child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .gap_1()
                                                            .child(
                                                                Label::new("Model")
                                                                    .text_xs()
                                                                    .text_color(
                                                                        cx.theme().muted_foreground,
                                                                    ),
                                                            )
                                                            .child(
                                                                Select::new(&self.tts_model_select)
                                                                    .placeholder("Select Model"),
                                                            ),
                                                    ),
                                                ),
                                        ),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .children(if let ProfileMode::Editing(_) = self.mode {
                            Some(
                                Button::new("delete_profile_btn")
                                    .label("Delete Profile")
                                    .danger()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.delete_profile(window, cx)
                                    })),
                            )
                        } else {
                            None
                        })
                        .child(
                            Button::new("save_profile_btn")
                                .label(if self.is_saving {
                                    "Saving..."
                                } else {
                                    "Save Changes"
                                })
                                .primary()
                                .disabled(self.is_saving)
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.save_profile(window, cx)
                                    }),
                                ),
                        ),
                )
                .into_any_element()
        } else {
            // Empty state - show when no profile is selected and not in new mode
            div()
                .flex_1()
                .bg(cx.theme().background)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_6()
                        .child(
                            div()
                                .p_6()
                                .rounded_full()
                                .bg(cx.theme().secondary)
                                .child(
                                    Icon::new(IconName::User)
                                        .text_color(cx.theme().muted_foreground)
                                        .with_size(UiSize::Large), // Use a larger size if possible or scale it
                                )
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_2()
                                .child(
                                    Label::new("Select a Profile")
                                        .text_xl()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(cx.theme().foreground),
                                )
                                .child(
                                    Label::new("Choose a profile from the sidebar to edit settings\nor create a new one to get started.")
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .text_align(TextAlign::Center),
                                ),
                        )
                        .child(
                            Button::new("empty_state_new_profile")
                                .label("Create New Profile")
                                .icon(IconName::Plus)
                                .primary()
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.create_new_profile(window, cx);
                                })),
                        ),
                )
                .into_any_element()
        };

        let container = div().flex().flex_1().h_full().overflow_hidden();

        let content_area = if is_small_screen {
            container.child(main_content).child(sidebar)
        } else {
            container.child(sidebar).child(main_content)
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("toggle_sidebar")
                                    .icon(IconName::Menu)
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_sidebar(cx);
                                    })),
                            )
                            .child(Icon::new(IconName::Settings).with_size(UiSize::Small))
                            .child(
                                Label::new("Profile Settings")
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD),
                            ),
                    )
                    .child(Button::new("close").icon(IconName::Close).ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |state, cx| {
                                state.toggle_profile_settings(cx);
                            });
                        }),
                    )),
            )
            .child(content_area)
    }
}
