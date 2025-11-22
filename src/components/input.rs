use crate::actions::{SelectAppCanva, SelectAppCanvas, SelectAppCoursera, SelectAppDeepResearch, SelectAppFigma, SelectAppImageGeneration, SelectAppLinear, SelectAppNotion, SelectAppPhotos, SelectAppSpotify, SelectAppStudy, SelectAppThinking, SelectAppWebSearch, ToggleMoreMenu};
use crate::audio::AudioInput;
use crate::components::voice_wave::VoiceWave;
use crate::state::AppState;
use gpui::InteractiveElement;
use gpui::prelude::*;
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, IconName,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    popover::Popover,
    tooltip::Tooltip,
    v_flex,
};

actions!(chat, [SubmitMessage]);

type SubmitCallback = Box<dyn Fn(String, &mut Context<MessageInput>)>;

pub struct MessageInput {
    input_state: Entity<InputState>,
    on_submit: Option<SubmitCallback>,
    voice_mode: bool,
    voice_wave: Option<Entity<VoiceWave>>,
    audio_input: Option<AudioInput>,
    state: Entity<AppState>,
}

impl MessageInput {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Type a message...")
                .multi_line()
                .auto_grow(1, 11) // Max 11 lines as per screenshot
                .clean_on_escape()
        });

        // Subscribe to input events to handle Enter key
        cx.subscribe_in(&input_state, window, |this, _state, event, window, cx| {
            if let InputEvent::PressEnter { secondary } = event
                && !secondary
            {
                // Enter without Shift - submit the message
                this.trigger_submit(window, cx);
            }
            // Shift+Enter is handled by the editor (newline)
        })
        .detach();

        Self {
            input_state,
            on_submit: None,
            voice_mode: false,
            voice_wave: None,
            audio_input: None,
            state,
        }
    }

    pub fn on_submit(mut self, handler: impl Fn(String, &mut Context<Self>) + 'static) -> Self {
        self.on_submit = Some(Box::new(handler));
        self
    }

    fn trigger_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        println!("Triggering submit...");
        let text = self.input_state.read(cx).value();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            println!("Submitting message: {}", trimmed);
            if let Some(handler) = &self.on_submit {
                (handler)(trimmed.to_string(), cx);
            }
            self.input_state.update(cx, |state, cx| {
                state.set_value("".to_string(), window, cx);
            });
            // Focus is handled by the input state usually, or we might need to re-focus
        } else {
            println!("Message is empty, ignoring.");
        }
    }

    fn toggle_voice_mode(&mut self, cx: &mut Context<Self>) {
        if self.voice_mode {
            self.voice_mode = false;
            self.audio_input = None;
            self.voice_wave = None;
        } else {
            let amplitude = self.state.read(cx).amplitude.clone();
            match AudioInput::new(amplitude.clone(), None) {
                Ok(input) => {
                    self.voice_mode = true;
                    self.audio_input = Some(input);
                    self.voice_wave = Some(VoiceWave::new(amplitude, self.state.clone(), cx));
                }
                Err(e) => {
                    eprintln!("Failed to start audio input: {}", e);
                }
            }
        }
        cx.notify();
    }

    fn confirm_voice_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.voice_mode = false;
        self.audio_input = None;
        self.voice_wave = None;
        cx.notify();

        // Mock transcription
        let mock_text = "This is a simulated transcription of your voice.";
        self.input_state.update(cx, |state, cx| {
            state.set_value(mock_text.to_string(), window, cx);
        });
    }
}

impl Render for MessageInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;
        let border = theme.border;
        let state_model = self.state.clone();
        let app_state = state_model.read(cx);
        let selected_apps = app_state.selected_apps.clone();

        // ChatGPT-style: centered container with max-width
        h_flex().w_full().justify_center().p_4().child(
            // Input container - rounded pill shape with shadow
            v_flex()
                .max_w(px(800.0)) // Max width like ChatGPT
                .w_full()
                .gap_2()
                .px_4()
                .py_3()
                .bg(theme.background) // Match chat background (white in light mode)
                .border_1()
                .border_color(border)
                .rounded(px(26.0)) // Rounded pill shape
                .shadow_sm()
                .child(
                    // Top: Input field (grows to fill space)
                    div().flex_grow().child(if self.voice_mode {
                        if let Some(voice_wave) = &self.voice_wave {
                            voice_wave.clone().into_any_element()
                        } else {
                            div().into_any_element()
                        }
                    } else {
                        Input::new(&self.input_state)
                            .appearance(false)
                            .into_any_element()
                    }),
                )
                .child(
                    // Bottom: Toolbar
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            // Left: App Picker Popover & Tags
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    // App Picker Popover
                                    Popover::new("app-picker-popover")
                                        .anchor(gpui::Corner::BottomLeft) // Anchor BottomLeft of popover to trigger (appears above)
                                        .trigger(
                                            Button::new("add-app").icon(IconName::Plus).ghost().rounded_full(),
                                        )
                                        .content(move |_, _, cx| {
                                            let state_model = state_model.clone();
                                            let inner_theme = cx.theme();
                                            let inner_secondary = inner_theme.secondary;
                                            let inner_secondary_foreground = inner_theme.secondary_foreground;
                                            
                                            v_flex()
                                                .w(px(300.0)) // Wider popover
                                                .py_1() // Vertical padding only
                                                .gap_0() // No gap between items for full hover effect
                                                // Removed styling (bg, border, shadow) to avoid double container issue
                                                // The Popover component likely provides the container styling
                                                .child(
                                                    // Helper for menu items
                                                    v_flex().gap_0() // No gap
                                                        .child(
                                                            div()
                                                                .id("add-photos")
                                                                .w_full()
                                                                .flex()
                                                                .items_center()
                                                                .gap_2()
                                                                .px_3() // Padding inside item
                                                                .py_2()
                                                                .hover(move |s| s.bg(inner_secondary))
                                                                .cursor_pointer()
                                                                .on_click({
                                                                    let state_model = state_model.clone();
                                                                    cx.listener(move |_, _, _, cx| {
                                                                        state_model.update(cx, |state, cx| state.select_app("Photos".to_string(), cx));
                                                                        cx.dispatch_action(&SelectAppPhotos);
                                                                    })
                                                                })
                                                                .child(svg().path("icons/clip.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                .child("Add photos & files"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id("create-image")
                                                                .w_full()
                                                                .flex()
                                                                .items_center()
                                                                .gap_2()
                                                                .px_3()
                                                                .py_2()
                                                                .hover(move |s| s.bg(inner_secondary))
                                                                .cursor_pointer()
                                                                .on_click({
                                                                    let state_model = state_model.clone();
                                                                    cx.listener(move |_, _, _, cx| {
                                                                        state_model.update(cx, |state, cx| state.select_app("Image Generation".to_string(), cx));
                                                                        cx.dispatch_action(&SelectAppImageGeneration);
                                                                    })
                                                                })
                                                                .child(svg().path("icons/create_image.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                .child("Create image"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id("thinking")
                                                                .w_full()
                                                                .flex()
                                                                .items_center()
                                                                .gap_2()
                                                                .px_3()
                                                                .py_2()
                                                                .hover(move |s| s.bg(inner_secondary))
                                                                .cursor_pointer()
                                                                .on_click({
                                                                    let state_model = state_model.clone();
                                                                    cx.listener(move |_, _, _, cx| {
                                                                        state_model.update(cx, |state, cx| state.select_app("Thinking".to_string(), cx));
                                                                        cx.dispatch_action(&SelectAppThinking);
                                                                    })
                                                                })
                                                                .child(svg().path("icons/thinking.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                .child("Thinking"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id("deep-search")
                                                                .w_full()
                                                                .flex()
                                                                .items_center()
                                                                .gap_2()
                                                                .px_3()
                                                                .py_2()
                                                                .hover(move |s| s.bg(inner_secondary))
                                                                .cursor_pointer()
                                                                .on_click({
                                                                    let state_model = state_model.clone();
                                                                    cx.listener(move |_, _, _, cx| {
                                                                        state_model.update(cx, |state, cx| state.select_app("Deep Research".to_string(), cx));
                                                                        cx.dispatch_action(&SelectAppDeepResearch);
                                                                    })
                                                                })
                                                                .child(svg().path("icons/deep_search.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                .child("Deep research"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id("study")
                                                                .w_full()
                                                                .flex()
                                                                .items_center()
                                                                .gap_2()
                                                                .px_3()
                                                                .py_2()
                                                                .hover(move |s| s.bg(inner_secondary))
                                                                .cursor_pointer()
                                                                .on_click({
                                                                    let state_model = state_model.clone();
                                                                    cx.listener(move |_, _, _, cx| {
                                                                        state_model.update(cx, |state, cx| state.select_app("Study".to_string(), cx));
                                                                        cx.dispatch_action(&SelectAppStudy);
                                                                    })
                                                                })
                                                                .child(svg().path("icons/study.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                .child("Study and learn"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id("more-trigger")
                                                                .w_full()
                                                                .flex()
                                                                .justify_between()
                                                                .items_center()
                                                                .px_3()
                                                                .py_2()
                                                                .hover(move |s| s.bg(inner_secondary))
                                                                .cursor_pointer()
                                                                .on_click({
                                                                    let state_model = state_model.clone();
                                                                    cx.listener(move |_, _, _, cx| {
                                                                        state_model.update(cx, |state, cx| state.toggle_more_menu(cx));
                                                                        cx.dispatch_action(&ToggleMoreMenu);
                                                                    })
                                                                })
                                                                .child(
                                                                    h_flex().gap_2().items_center()
                                                                        .child(Icon::new(IconName::Ellipsis).size(px(16.0)).text_color(inner_secondary_foreground))
                                                                        .child("More")
                                                                )
                                                                .child(
                                                                    Icon::new(if state_model.read(cx).more_menu_open { IconName::ChevronDown } else { IconName::ChevronRight })
                                                                        .size(px(14.0))
                                                                        .text_color(inner_secondary_foreground)
                                                                )
                                                        )
                                                        .children(if state_model.read(cx).more_menu_open {
                                                            Some(
                                                                v_flex().gap_0()
                                                                    .child(
                                                                         div().id("web-search").px_3().py_2().hover(move |s| s.bg(inner_secondary)).cursor_pointer()
                                                                             .on_click({
                                                                                 let state_model = state_model.clone();
                                                                                 cx.listener(move |_, _, _, cx| {
                                                                                     state_model.update(cx, |state, cx| state.select_app("Web search".to_string(), cx));
                                                                                     cx.dispatch_action(&SelectAppWebSearch);
                                                                                 })
                                                                             })
                                                                             .child(h_flex().gap_2().items_center()
                                                                                 .child(svg().path("icons/web_search.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                                 .child("Web search")
                                                                             )
                                                                     )
                                                                     .child(
                                                                         div().id("canvas").px_3().py_2().hover(move |s| s.bg(inner_secondary)).cursor_pointer()
                                                                             .on_click({
                                                                                 let state_model = state_model.clone();
                                                                                 cx.listener(move |_, _, _, cx| {
                                                                                     state_model.update(cx, |state, cx| state.select_app("Canvas".to_string(), cx));
                                                                                     cx.dispatch_action(&SelectAppCanvas);
                                                                                 })
                                                                             })
                                                                             .child(h_flex().gap_2().items_center()
                                                                                 .child(svg().path("icons/canvas.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                                 .child("Canvas")
                                                                             )
                                                                     )
                                                                     .child(
                                                                         div().id("canva").px_3().py_2().hover(move |s| s.bg(inner_secondary)).cursor_pointer()
                                                                             .on_click({
                                                                                 let state_model = state_model.clone();
                                                                                 cx.listener(move |_, _, _, cx| {
                                                                                     state_model.update(cx, |state, cx| state.select_app("Canva".to_string(), cx));
                                                                                     cx.dispatch_action(&SelectAppCanva);
                                                                                 })
                                                                             })
                                                                             .child(h_flex().gap_2().items_center()
                                                                                 .child(svg().path("icons/canva.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                                 .child("Canva")
                                                                             )
                                                                     )
                                                                     .child(
                                                                         div().id("coursera").px_3().py_2().hover(move |s| s.bg(inner_secondary)).cursor_pointer()
                                                                             .on_click({
                                                                                 let state_model = state_model.clone();
                                                                                 cx.listener(move |_, _, _, cx| {
                                                                                     state_model.update(cx, |state, cx| state.select_app("Coursera".to_string(), cx));
                                                                                     cx.dispatch_action(&SelectAppCoursera);
                                                                                 })
                                                                             })
                                                                             .child(h_flex().gap_2().items_center()
                                                                                 .child(svg().path("icons/coursera.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                                 .child("Coursera")
                                                                             )
                                                                     )
                                                                     .child(
                                                                         div().id("figma-more").px_3().py_2().hover(move |s| s.bg(inner_secondary)).cursor_pointer()
                                                                             .on_click({
                                                                                 let state_model = state_model.clone();
                                                                                 cx.listener(move |_, _, _, cx| {
                                                                                     state_model.update(cx, |state, cx| state.select_app("Figma".to_string(), cx));
                                                                                     cx.dispatch_action(&SelectAppFigma);
                                                                                 })
                                                                             })
                                                                             .child(h_flex().gap_2().items_center()
                                                                                 .child(svg().path("icons/figma.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                                 .child("Figma")
                                                                             )
                                                                     )
                                                                     .child(
                                                                         div().id("spotify").px_3().py_2().hover(move |s| s.bg(inner_secondary)).cursor_pointer()
                                                                             .on_click({
                                                                                 let state_model = state_model.clone();
                                                                                 cx.listener(move |_, _, _, cx| {
                                                                                     state_model.update(cx, |state, cx| state.select_app("Spotify".to_string(), cx));
                                                                                     cx.dispatch_action(&SelectAppSpotify);
                                                                                 })
                                                                             })
                                                                             .child(h_flex().gap_2().items_center()
                                                                                 .child(svg().path("icons/spotify.svg").size(px(16.0)).text_color(inner_secondary_foreground))
                                                                                 .child("Spotify")
                                                                             )
                                                                     )
                                                            )
                                                        } else {
                                                            None
                                                        })
                                                )
                                        }),
                                )
                                .child(
                                    // Tags Area (Middle)
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap_2()
                                        .children(
                                        selected_apps.iter().enumerate().map(|(i, app)| {
                                            let app_name = app.clone();
                                            let icon_path = match app_name.as_str() {
                                                "Photos" => "icons/clip.svg",
                                                "Image Generation" => "icons/create_image.svg",
                                                "Thinking" => "icons/thinking.svg",
                                                "Deep Research" => "icons/deep_search.svg",
                                                "Study" => "icons/study.svg",
                                                "Web search" => "icons/web_search.svg",
                                                "Canvas" => "icons/canvas.svg",
                                                "Canva" => "icons/canva.svg",
                                                "Coursera" => "icons/coursera.svg",
                                                "Figma" => "icons/figma.svg",
                                                "Spotify" => "icons/spotify.svg",
                                                _ => "icons/clip.svg",
                                            };

                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .bg(theme.secondary)
                                                .rounded_md()
                                                .px_2()
                                                .py_1()
                                                .child(
                                                    svg()
                                                        .path(icon_path)
                                                        .size(px(12.0))
                                                        .text_color(secondary_foreground)
                                                )
                                                .child(
                                                    div()
                                                        .child(app_name.clone())
                                                        .text_size(px(12.0)),
                                                )
                                                .child(
                                                    div()
                                                        .id(("remove-app", i))
                                                        .cursor_pointer()
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                this.state.update(
                                                                    cx,
                                                                    |state, cx| {
                                                                        state.remove_app(
                                                                            app_name.clone(),
                                                                            cx,
                                                                        );
                                                                    },
                                                                );
                                                            },
                                                        ))
                                                        .child(
                                                            Icon::new(IconName::Close)
                                                                .size(px(14.0)),
                                                        ),
                                                )
                                        }),
                                    ),
                                ),
                        )
                        .child(
                            // Right: Action Icons
                            h_flex()
                                .gap_1()
                                .items_center()
                                .when(self.voice_mode, |this| {
                                    // Voice Mode: Cancel (X) and Confirm (Check)
                                    this.child(
                                        div()
                                            .id("cancel-voice-btn")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_voice_mode(cx);
                                            }))
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(gpui::transparent_black())
                                            .text_color(secondary_foreground)
                                            .hover(move |style| style.bg(secondary))
                                            .cursor_pointer()
                                            .tooltip(|w, cx| Tooltip::new("Cancel").build(w, cx))
                                            .child(
                                                Icon::new(IconName::Close)
                                                    .text_color(secondary_foreground),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-voice-btn")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.confirm_voice_input(window, cx);
                                            }))
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(theme.foreground) // Black/White
                                            .text_color(theme.background) // White/Black
                                            .hover(move |style| {
                                                style.bg(theme.foreground.opacity(0.8))
                                            })
                                            .cursor_pointer()
                                            .tooltip(|w, cx| Tooltip::new("Done").build(w, cx))
                                            .child(
                                                Icon::new(IconName::Check)
                                                    .text_color(theme.background),
                                            ),
                                    )
                                })
                                .when(!self.voice_mode, |this| {
                                    // Text Mode: Mic and Send/Headphone
                                    this.child(
                                        div()
                                            .id("dictate")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_voice_mode(cx);
                                            }))
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(gpui::transparent_black()) // Transparent/White by default
                                            .text_color(secondary_foreground)
                                            .hover(move |style| style.bg(secondary)) // Gray on hover
                                            .cursor_pointer()
                                            .tooltip(|w, cx| Tooltip::new("Dictate").build(w, cx))
                                            .child(
                                                svg()
                                                    .path("icons/mic.svg")
                                                    .size(px(18.0))
                                                    .text_color(secondary_foreground),
                                            ),
                                    )
                                    .child(
                                        if self.input_state.read(cx).text().len() == 0 {
                                            // Empty state: Sparkles icon - opens voice mode modal
                                            div()
                                                .id("voice-mode")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.state.update(cx, |state, cx| {
                                                        state.start_voice_mode(cx);
                                                    });
                                                }))
                                                .w(px(36.0))
                                                .h(px(36.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .bg(gpui::transparent_black())
                                                .text_color(secondary_foreground)
                                                .hover(move |style| style.bg(secondary))
                                                .cursor_pointer()
                                                .tooltip(|w, cx| {
                                                    Tooltip::new("Voice Mode").build(w, cx)
                                                })
                                                .child(
                                                    svg()
                                                        .path("icons/sparkles.svg")
                                                        .size(px(18.0))
                                                        .text_color(secondary_foreground),
                                                )
                                        } else {
                                            // Typing state: Send button (Black bg, White arrow)
                                            div()
                                                .id("send-btn")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.trigger_submit(window, cx);
                                                }))
                                                .w(px(36.0))
                                                .h(px(36.0))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .bg(theme.foreground) // Theme-aware foreground (Black in light, White in dark)
                                                .text_color(theme.background) // Theme-aware background (White in light, Black in dark)
                                                .hover(move |style| {
                                                    style.bg(theme.foreground.opacity(0.8))
                                                })
                                                .cursor_pointer()
                                                .tooltip(|w, cx| {
                                                    Tooltip::new("Send message").build(w, cx)
                                                })
                                                .child(
                                                    Icon::new(IconName::ArrowUp)
                                                        .text_color(theme.background),
                                                )
                                        },
                                    )
                                }),
                        ),
                )
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppCanva, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Canva".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppFigma, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Figma".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppNotion, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Notion".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppLinear, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Linear".to_string(), cx));
                    }
                }),
        )
    }
}
