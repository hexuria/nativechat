use crate::state::AppState;
use gpui::InteractiveElement;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, IntoElement, Render, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    scroll::ScrollbarAxis,
};

pub struct ProfileSettingsModal {
    state: Entity<AppState>,
    profile_name_input: Entity<InputState>,
    provider_input: Entity<InputState>,
    model_input: Entity<InputState>,
    embedding_provider_input: Entity<InputState>,
    embedding_model_input: Entity<InputState>,
    image_provider_input: Entity<InputState>,
    image_model_input: Entity<InputState>,
    api_key_input: Entity<InputState>,
}

impl ProfileSettingsModal {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let profile_name_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Profile Name");
            input.set_value("Default".to_string(), window, cx);
            input
        });
        let provider_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Provider");
            input.set_value("Google Gemini".to_string(), window, cx);
            input
        });
        let model_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Model");
            input.set_value("Gemini 2.5 Flash".to_string(), window, cx);
            input
        });
        let embedding_provider_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Provider");
            input.set_value("Google Gemini".to_string(), window, cx);
            input
        });
        let embedding_model_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Model");
            input.set_value("Text Embedding 004".to_string(), window, cx);
            input
        });
        let image_provider_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Provider");
            input.set_value("Google Gemini".to_string(), window, cx);
            input
        });
        let image_model_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("Model");
            input.set_value("Gemini 2.5 Flash Image Preview".to_string(), window, cx);
            input
        });
        let api_key_input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder("API Key");
            input.set_value("Configured".to_string(), window, cx);
            input.masked(true)
        });

        Self {
            state,
            profile_name_input,
            provider_input,
            model_input,
            embedding_provider_input,
            embedding_model_input,
            image_provider_input,
            image_model_input,
            api_key_input,
        }
    }
}

impl Render for ProfileSettingsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .absolute()
            .inset_0()
            .bg(gpui::black().opacity(0.5))
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
                                                            .child(Input::new(&self.provider_input))
                                                    )
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Default chat model").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Input::new(&self.model_input))
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
                                                            .child(Input::new(&self.embedding_provider_input))
                                                    )
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Embedding model").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Input::new(&self.embedding_model_input))
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
                                                            .child(Input::new(&self.image_provider_input))
                                                    )
                                                    .child(
                                                        div().flex().flex_col()
                                                            .flex_1()
                                                            .gap_1()
                                                            .child(div().child("Image model").font_weight(FontWeight::BOLD).text_sm())
                                                            .child(Input::new(&self.image_model_input))
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
                                                    )
                                            )
                                    )
                            )
                    )
            )
    }
}
