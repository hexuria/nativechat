use crate::services::model_registry::{ModelProfile, Provider};
use crate::state::AppState;
use gpui::InteractiveElement;
use gpui::prelude::*;
use gpui::{
    Context, Entity, FontWeight, IntoElement, MouseButton, Render, Styled, Window, div, px,
};
use std::collections::HashMap;
use ui::{
    ActiveTheme, Icon, IconName, IndexPath, Sizable, StyledExt, SearchableVec, Select, SelectEvent, SelectState,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    scroll::ScrollbarAxis,
};

pub struct ProfileSettingsModal {
    state: Entity<AppState>,
    profile_name_input: Entity<InputState>,
    provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    embedding_provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    embedding_model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    image_provider_select: Entity<SelectState<SearchableVec<Provider>>>,
    image_model_select: Entity<SelectState<SearchableVec<ModelProfile>>>,
    api_key_input: Entity<InputState>,
}

impl ProfileSettingsModal {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let profile_name_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Profile Name");
            input.set_value("Default".to_string(), window, cx);
            input
        });
        
        // Trigger model fetch with empty keys (will use env vars)
        state.update(cx, |state, cx| {
            state.fetch_models(HashMap::new(), cx);
        });

        let providers = SearchableVec::new(vec![Provider::Gemini, Provider::OpenAI, Provider::Anthropic]);
        
        let provider_select = cx.new(|cx| {
            SelectState::new(providers.clone(), Some(IndexPath::default()), window, cx).searchable(true)
        });
        let model_select = cx.new(|cx| {
            SelectState::new(SearchableVec::new(vec![]), None, window, cx).searchable(true)
        });
        let embedding_provider_select = cx.new(|cx| {
            SelectState::new(providers.clone(), Some(IndexPath::default()), window, cx).searchable(true)
        });
        let embedding_model_select = cx.new(|cx| {
            SelectState::new(SearchableVec::new(vec![]), None, window, cx).searchable(true)
        });
        let image_provider_select = cx.new(|cx| {
            SelectState::new(providers.clone(), Some(IndexPath::default()), window, cx).searchable(true)
        });
        let image_model_select = cx.new(|cx| {
            SelectState::new(SearchableVec::new(vec![]), None, window, cx).searchable(true)
        });

        let api_key_input = cx.new(|cx| {
            let input = InputState::new(window, cx).placeholder("API Key");
            input.masked(true)
        });

        cx.subscribe(&provider_select, |this, _, event: &SelectEvent<SearchableVec<Provider>>, cx| {
            if let SelectEvent::Confirm(Some(provider)) = event {
                let model_select = this.model_select.clone();
                this.update_model_list(model_select, provider, |m| m.capabilities.supports_text_generation, cx);
            }
        }).detach();

        cx.subscribe(&embedding_provider_select, |this, _, event: &SelectEvent<SearchableVec<Provider>>, cx| {
            if let SelectEvent::Confirm(Some(provider)) = event {
                let model_select = this.embedding_model_select.clone();
                this.update_model_list(model_select, provider, |m| m.capabilities.supports_embedding, cx);
            }
        }).detach();

        cx.subscribe(&image_provider_select, |this, _, event: &SelectEvent<SearchableVec<Provider>>, cx| {
            if let SelectEvent::Confirm(Some(provider)) = event {
                let model_select = this.image_model_select.clone();
                this.update_model_list(model_select, provider, |m| m.capabilities.supports_image_generation, cx);
            }
        }).detach();

        cx.observe(&state, |this: &mut Self, _, cx| {
            let provider = this.provider_select.read(cx).selected_value().cloned();
            if let Some(provider) = provider {
                let model_select = this.model_select.clone();
                this.update_model_list(model_select, &provider, |m| m.capabilities.supports_text_generation, cx);
            }
            
            let embedding_provider = this.embedding_provider_select.read(cx).selected_value().cloned();
            if let Some(provider) = embedding_provider {
                let model_select = this.embedding_model_select.clone();
                this.update_model_list(model_select, &provider, |m| m.capabilities.supports_embedding, cx);
            }

            let image_provider = this.image_provider_select.read(cx).selected_value().cloned();
            if let Some(provider) = image_provider {
                let model_select = this.image_model_select.clone();
                this.update_model_list(model_select, &provider, |m| m.capabilities.supports_image_generation, cx);
            }
        }).detach();

        Self {
            state,
            profile_name_input,
            provider_select,
            model_select,
            embedding_provider_select,
            embedding_model_select,
            image_provider_select,
            image_model_select,
            api_key_input,
        }
    }

    fn update_model_list(&mut self, select: Entity<SelectState<SearchableVec<ModelProfile>>>, provider: &Provider, capability_filter: impl Fn(&ModelProfile) -> bool, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let models: Vec<ModelProfile> = state.available_models.iter()
            .filter(|m| m.provider == *provider && capability_filter(m))
            .cloned()
            .collect();
        
        select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(models), cx);
        });
    }
}

impl Render for ProfileSettingsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .absolute()
            .inset_0()
            .bg(gpui::black().opacity(0.5))
            // Prevent clicks from passing through to elements behind the modal
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
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(800.0))
                    .h(px(700.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .rounded_xl()
                    .shadow_lg()
                    .child(
                        div().flex().flex_col().h_full()
                            .child(
                                // Header
                                div().flex().flex_row()
                                    .justify_between()
                                    .items_center()
                                    .p_4()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(div().child("Provider Profiles").font_weight(FontWeight::BOLD).text_lg())
                                    .child(
                                        gpui::div()
                                            .id("close-profile-settings")
                                            .cursor_pointer()
                                            .child(Icon::new(IconName::Close))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.state.update(cx, |state, cx| {
                                                    state.toggle_profile_settings(cx);
                                                });
                                            }))
                                    )
                            )
                            .child(
                                // Content
                                div().flex().flex_row()
                                    .flex_1()
                                    .child(
                                        // Sidebar List
                                        div().flex().flex_col()
                                            .w(px(250.0))
                                            .border_r_1()
                                            .border_color(theme.border)
                                            .p_2()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .flex()
                                                    .justify_between()
                                                    .items_center()
                                                    .p_2()
                                                    .child(div().child("Profiles").font_weight(FontWeight::BOLD).text_sm())
                                                    .child(
                                                        Button::new("new-profile-btn")
                                                            .label("+ New")
                                                            .small()
                                                            .ghost()
                                                    )
                                            )
                                            .child(
                                                div()
                                                    .p_2()
                                                    .bg(theme.secondary)
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(theme.accent)
                                                    .child(div().child("Default").font_weight(FontWeight::BOLD).text_sm())
                                                    .child(div().child("Google Gemini").text_xs().text_color(theme.muted_foreground))
                                            )
                                            .child(
                                                div()
                                                    .p_2()
                                                    .rounded_md()
                                                    .hover(move |s| s.bg(theme.secondary))
                                                    .child(div().child("Gemini CLI").font_weight(FontWeight::BOLD).text_sm())
                                                    .child(div().child("Gemini CLI").text_xs().text_color(theme.muted_foreground))
                                            )
                                            .child(
                                                div()
                                                    .p_2()
                                                    .rounded_md()
                                                    .hover(move |s| s.bg(theme.secondary))
                                                    .child(div().child("Untitled Profile").font_weight(FontWeight::BOLD).text_sm())
                                                    .child(div().child("Google Gemini").text_xs().text_color(theme.muted_foreground))
                                            )
                                            .child(
                                                div()
                                                    .mt_4()
                                                    .p_2()
                                                    .child(div().child("Create new profile").text_sm().text_color(theme.muted_foreground))
                                            )
                                    )
                                    .child(
                                        // Form
                                        div().flex().flex_col()
                                            .flex_1()
                                            .min_h(px(0.0))
                                            .p_6()
                                            .gap_6()
                                            .scrollable(ScrollbarAxis::Vertical)
                                            .child(
                                                div().flex().flex_col()
                                                    .gap_1()
                                                    .child(div().child("Profile name").font_weight(FontWeight::BOLD).text_sm())
                                                    .child(Input::new(&self.profile_name_input))
                                            )
                                            .child(
                                                div().flex().flex_row()
                                                    .gap_4()
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Provider").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Select::new(&self.provider_select).placeholder("Select Provider").w_full())
                                                    )
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Default chat model").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Select::new(&self.model_select).placeholder("Select Model").w_full().search_placeholder("Search models..."))
                                                    )
                                            )
                                            .child(
                                                div().flex().flex_row()
                                                    .gap_4()
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Embedding provider").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Select::new(&self.embedding_provider_select).placeholder("Select Provider").w_full())
                                                    )
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Embedding model").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Select::new(&self.embedding_model_select).placeholder("Select Model").w_full().search_placeholder("Search models..."))
                                                    )
                                            )
                                            .child(
                                                div().flex().flex_row()
                                                    .gap_4()
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Image provider").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Select::new(&self.image_provider_select).placeholder("Select Provider").w_full())
                                                    )
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Image model").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Select::new(&self.image_model_select).placeholder("Select Model").w_full().search_placeholder("Search models..."))
                                                    )
                                            )
                                            .child(
                                                div().flex().flex_row()
                                                    .justify_end()
                                                    .child(
                                                        div().child(format!("Updated 10/21/2025, 10:31:41 PM")).text_xs().text_color(theme.muted_foreground).mr_4()
                                                    )
                                                    .child(
                                                        Button::new("save-profile-btn")
                                                            .label("Save profile")
                                                            .bg(theme.foreground.clone())
                                                            .text_color(theme.background.clone())
                                                    )
                                            )
                                            .child(
                                                div()
                                                    .p_4()
                                                    .bg(theme.secondary.opacity(0.3))
                                                    .rounded_lg()
                                                    .border_1()
                                                    .border_color(theme.border)
                                                    .flex()
                                                    .flex_col()
                                                    .gap_4()
                                                    .child(
                                                        div().flex().flex_col()
                                                            .gap_1()
                                                            .child(div().child("Credentials").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(div().child("Enter credentials to authenticate with the provider.").text_xs().text_color(theme.muted_foreground))
                                                    )
                                                    .child(
                                                        div().flex().flex_col()
                                                            .gap_1()
                                                            .child(div().child("API KEY *").text_xs().font_weight(FontWeight::BOLD))
                                                            .child(Input::new(&self.api_key_input))
                                                    )
                                                    .child(
                                                        Button::new("save-credentials-btn")
                                                            .label("Save credentials")
                                                            .bg(theme.foreground.clone())
                                                            .text_color(theme.background.clone())
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                let api_key = this.api_key_input.read(cx).text().to_string();
                                                                
                                                                // Get selected provider from select state  
                                                                if let Some(provider) = this.provider_select.read(cx).selected_value() {
                                                                    let mut api_keys = HashMap::new();
                                                                    api_keys.insert(provider.clone(), api_key);
                                                                    
                                                                    this.state.update(cx, |state, cx| {
                                                                        state.fetch_models(api_keys, cx);
                                                                    });
                                                                }
                                                            }))
                                                    )
                                            )
                                    )
                            )
                    )
            )
    }
}
