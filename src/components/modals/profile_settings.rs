use crate::services::database::Credential;
use crate::services::model_registry::{ModelCapabilities, ModelProfile, Provider};
use crate::state::AppState;
use gpui::prelude::*;
use gpui::*;
use std::collections::HashMap;
use ui::IndexPath;
use ui::button::ButtonVariants;
use ui::list::ListItem;
use ui::scroll::ScrollbarAxis;
use ui::{
    Icon, IconName, Sizable, Size as UiSize, StyleSized, StyledExt, WindowExt,
    button::Button,
    input::{Input, InputState},
    label::Label,
    list::{List, ListDelegate, ListState},
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
    theme::ActiveTheme,
};

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
    profiles: Vec<String>,
    selected_index: Option<usize>,
}

impl ProfileListDelegate {
    pub fn new(profiles: Vec<String>) -> Self {
        Self {
            profiles,
            selected_index: None,
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

        Some(
            ListItem::new(ix)
                .p_2()
                .rounded_md()
                .when(is_selected, |s| s.bg(theme.secondary))
                .child(
                    div()
                        .child(profile.clone())
                        .font_weight(FontWeight::BOLD)
                        .text_sm(),
                )
                .child(
                    div()
                        .child("Google Gemini")
                        .text_xs()
                        .text_color(theme.muted_foreground),
                ),
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

pub struct ProfileSettingsModal {
    state: Entity<AppState>,
    profile_name_input: Entity<InputState>,
    provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    embedding_provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    embedding_model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    image_provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    image_model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    chat_credential_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
    embedding_credential_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
    image_credential_select: Entity<SelectState<SearchableVec<CredentialItem>>>,
    list_state: Entity<ListState<ProfileListDelegate>>,
    selected_index: Option<usize>,
    error_message: Option<String>,
    sidebar_open: bool,
    last_window_width: Option<Pixels>,
}

impl ProfileSettingsModal {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let profile_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Profile Name"));

        // Trigger model fetch with empty keys (will use env vars)
        state.update(cx, |state, cx| {
            state.fetch_models(HashMap::new(), cx);
        });

        let provider_items = SearchableVec::new(vec![
            Provider::Gemini,
            Provider::Anthropic,
            Provider::OpenAI,
        ]);

        let provider_select = cx.new(|cx| {
            let mut state = SelectState::new(provider_items.clone(), None, window, cx);
            state.set_selected_value(&Provider::Gemini, window, cx);
            state
        });

        let model_items = SearchableVec::new(vec![
            ModelProfile {
                provider: Provider::Gemini,
                id: "gemini-2.0-flash-exp".into(),
                display_name: "Gemini 2.0 Flash".into(),
                created_at: None,
                input_token_limit: None,
                output_token_limit: None,
                capabilities: ModelCapabilities::default(),
            },
            ModelProfile {
                provider: Provider::Gemini,
                id: "gemini-1.5-pro".into(),
                display_name: "Gemini 1.5 Pro".into(),
                created_at: None,
                input_token_limit: None,
                output_token_limit: None,
                capabilities: ModelCapabilities::default(),
            },
        ]);

        let model_select = cx.new(|cx| {
            let mut state = SelectState::new(model_items.clone(), None, window, cx);
            state.set_selected_value(&"gemini-2.0-flash-exp".to_string(), window, cx);
            state
        });

        let embedding_provider_select = cx.new(|cx| {
            let mut state = SelectState::new(provider_items.clone(), None, window, cx);
            state.set_selected_value(&Provider::Gemini, window, cx);
            state
        });

        let embedding_model_items = SearchableVec::new(vec![ModelProfile {
            provider: Provider::Gemini,
            id: "embedding-001".into(),
            display_name: "Embedding 001".into(),
            created_at: None,
            input_token_limit: None,
            output_token_limit: None,
            capabilities: ModelCapabilities::default(),
        }]);

        let embedding_model_select = cx.new(|cx| {
            let mut state = SelectState::new(embedding_model_items, None, window, cx);
            state.set_selected_value(&"embedding-001".to_string(), window, cx);
            state
        });

        let image_provider_select = cx.new(|cx| {
            let mut state = SelectState::new(provider_items.clone(), None, window, cx);
            state.set_selected_value(&Provider::Gemini, window, cx);
            state
        });

        let image_model_items = SearchableVec::new(vec![ModelProfile {
            provider: Provider::Gemini,
            id: "nano-banana-pro".into(),
            display_name: "Nano Banana Pro".into(),
            created_at: None,
            input_token_limit: None,
            output_token_limit: None,
            capabilities: ModelCapabilities::default(),
        }]);

        let image_model_select = cx.new(|cx| {
            let mut state = SelectState::new(image_model_items, None, window, cx);
            state.set_selected_value(&"nano-banana-pro".to_string(), window, cx);
            state
        });

        let chat_credential_select =
            cx.new(|cx| SelectState::new(SearchableVec::new(vec![]), None, window, cx));

        let embedding_credential_select =
            cx.new(|cx| SelectState::new(SearchableVec::new(vec![]), None, window, cx));

        let image_credential_select =
            cx.new(|cx| SelectState::new(SearchableVec::new(vec![]), None, window, cx));

        let list_delegate = ProfileListDelegate::new(vec![
            "Default".to_string(),
            "Gemini CLI".to_string(),
            "Untitled Profile".to_string(),
        ]);
        let list_state = cx.new(|cx| ListState::new(list_delegate, window, cx));

        let window_width = window.viewport_size().width;

        let mut this = Self {
            state,
            profile_name_input,
            provider_select,
            model_select,
            embedding_provider_select,
            embedding_model_select,
            image_provider_select,
            image_model_select,
            chat_credential_select,
            embedding_credential_select,
            image_credential_select,
            list_state,
            selected_index: None,
            error_message: None,
            sidebar_open: window_width >= px(650.0),
            last_window_width: Some(window_width),
        };

        this.fetch_credentials(cx);
        this.subscribe_to_selects(cx);
        this
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    fn subscribe_to_selects(&mut self, cx: &mut Context<Self>) {
        cx.subscribe(
            &self.provider_select,
            |this, _, event: &SelectEvent<SearchableVec<Provider>>, cx| {
                if let SelectEvent::Confirm(Some(provider)) = event {
                    let provider = provider.clone();
                    let model_select = this.model_select.clone();
                    this.update_model_list(
                        model_select,
                        &provider,
                        |m| m.capabilities.supports_text_generation,
                        cx,
                    );
                }
            },
        )
        .detach();

        cx.subscribe(
            &self.embedding_provider_select,
            |this, _, event: &SelectEvent<SearchableVec<Provider>>, cx| {
                if let SelectEvent::Confirm(Some(provider)) = event {
                    let provider = provider.clone();
                    let model_select = this.embedding_model_select.clone();
                    this.update_model_list(
                        model_select,
                        &provider,
                        |m| m.capabilities.supports_embedding,
                        cx,
                    );
                }
            },
        )
        .detach();

        cx.subscribe(
            &self.image_provider_select,
            |this, _, event: &SelectEvent<SearchableVec<Provider>>, cx| {
                if let SelectEvent::Confirm(Some(provider)) = event {
                    let provider = provider.clone();
                    let model_select = this.image_model_select.clone();
                    this.update_model_list(
                        model_select,
                        &provider,
                        |m| m.capabilities.supports_image_generation,
                        cx,
                    );
                }
            },
        )
        .detach();

        // Observe state changes to update lists if providers are already selected
        let state = self.state.clone();
        cx.observe(&state, |this: &mut Self, _, cx| {
            let provider = this.provider_select.read(cx).selected_value().cloned();
            if let Some(provider) = provider {
                let model_select = this.model_select.clone();
                this.update_model_list(
                    model_select,
                    &provider,
                    |m| m.capabilities.supports_text_generation,
                    cx,
                );
            }

            let embedding_provider = this
                .embedding_provider_select
                .read(cx)
                .selected_value()
                .cloned();
            if let Some(provider) = embedding_provider {
                let model_select = this.embedding_model_select.clone();
                this.update_model_list(
                    model_select,
                    &provider,
                    |m| m.capabilities.supports_embedding,
                    cx,
                );
            }

            let image_provider = this
                .image_provider_select
                .read(cx)
                .selected_value()
                .cloned();
            if let Some(provider) = image_provider {
                let model_select = this.image_model_select.clone();
                this.update_model_list(
                    model_select,
                    &provider,
                    |m| m.capabilities.supports_image_generation,
                    cx,
                );
            }
        })
        .detach();
    }

    fn update_model_list(
        &mut self,
        select: Entity<SelectState<SearchableVec<ModelProfile>>>,
        provider: &Provider,
        capability_filter: impl Fn(&ModelProfile) -> bool,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        let models: Vec<ModelProfile> = state
            .available_models
            .iter()
            .filter(|m| m.provider == *provider && capability_filter(m))
            .cloned()
            .collect();

        select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(models), cx);
        });
    }

    fn fetch_credentials(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);

        if let Some(db) = &state.database_service {
            let db = db.clone();
            cx.spawn(
                move |view: WeakEntity<ProfileSettingsModal>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        match db.get_credentials().await {
                            Ok(creds) => {
                                view.update(&mut cx, |this, cx| {
                                    let items: Vec<CredentialItem> =
                                        creds.into_iter().map(CredentialItem).collect();
                                    let creds_vec = SearchableVec::new(items);

                                    this.chat_credential_select.update(cx, |select, cx| {
                                        select.set_items(creds_vec.clone(), cx);
                                    });
                                    this.embedding_credential_select.update(cx, |select, cx| {
                                        select.set_items(creds_vec.clone(), cx);
                                    });
                                    this.image_credential_select.update(cx, |select, cx| {
                                        select.set_items(creds_vec, cx);
                                    });
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to fetch credentials: {}", e),
                        }
                    }
                },
            )
            .detach();
        }
    }
}

impl Render for ProfileSettingsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

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
                .w_64()
                .border_r_1()
                .border_color(theme.border)
                .bg(theme.background)
                .flex()
                .flex_col()
                .child(
                    div().p_4().border_b_1().border_color(theme.border).child(
                        Button::new("new_profile")
                            .label("New Profile")
                            .icon(IconName::Plus)
                            .w_full()
                            .on_click(|_, _, _| {}),
                    ),
                )
                .child(
                    List::new(&self.list_state)
                        .list_size(UiSize::Small)
                        .with_size(UiSize::Small)
                        .h_full(),
                );

            if is_small_screen {
                sidebar_div = sidebar_div
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .occlude() // Block clicks on elements below
                    .shadow_lg();
            }

            sidebar_div
        } else {
            div().hidden()
        };

        let main_content = div()
            .flex_1()
            .p_6()
            .flex()
            .flex_col()
            .gap_6()
            .scrollable(ScrollbarAxis::Vertical)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Input::new(&self.profile_name_input).with_size(UiSize::Large)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Label::new("Chat Model").font_weight(FontWeight::BOLD))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_4()
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.provider_select)
                                                .placeholder("Select Provider"),
                                        ),
                                    )
                                    .child(div().flex_1().min_w_64().child(
                                        Select::new(&self.model_select).placeholder("Select Model"),
                                    ))
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.chat_credential_select)
                                                .placeholder("Select Credential"),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Label::new("Embedding Model").font_weight(FontWeight::BOLD))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_4()
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.embedding_provider_select)
                                                .placeholder("Select Provider"),
                                        ),
                                    )
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.embedding_model_select)
                                                .placeholder("Select Model"),
                                        ),
                                    )
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.embedding_credential_select)
                                                .placeholder("Select Credential"),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Label::new("Image Model").font_weight(FontWeight::BOLD))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_4()
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.image_provider_select)
                                                .placeholder("Select Provider"),
                                        ),
                                    )
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.image_model_select)
                                                .placeholder("Select Model"),
                                        ),
                                    )
                                    .child(
                                        div().flex_1().min_w_64().child(
                                            Select::new(&self.image_credential_select)
                                                .placeholder("Select Credential"),
                                        ),
                                    ),
                            ),
                    ),
            )
            .child(
                div().flex().justify_end().gap_2().child(
                    Button::new("save")
                        .label("Save Changes")
                        .primary()
                        .on_click(|_, _, _| {}),
                ),
            );

        let container = div().flex().flex_1().relative();

        let content_area = if is_small_screen {
            container.child(main_content).child(sidebar)
        } else {
            container.child(sidebar).child(main_content)
        };

        div()
            .absolute()
            .inset_0()
            .bg(gpui::black().opacity(0.5))
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                cx.stop_propagation();
            })
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p_4()
                    .border_b_1()
                    .border_color(theme.border)
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
