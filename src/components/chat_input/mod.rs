mod items;

pub use items::{render_flyout_item, render_popover_item};

use crate::actions::{SelectAppCanva, SelectAppCanvas, SelectAppCoursera, SelectAppDeepResearch, SelectAppFigma, SelectAppImageGeneration, SelectAppLinear, SelectAppNotion, SelectAppPhotos, SelectAppSpotify, SelectAppStudy, SelectAppThinking, SelectAppWebSearch};
use crate::audio::AudioInput;
use crate::components::voice_wave::VoiceWave;
use crate::state::AppState;
use gpui::InteractiveElement;
use gpui::prelude::*;
use gpui::*;
use ui::{
    ActiveTheme, Icon, IconName,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::PopupMenuItem,
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
        println!("MessageInput::render called");
        let theme = cx.theme();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;
        let border = theme.border;
        let state_model = self.state.clone();
        let app_state = state_model.read(cx);
        let selected_apps = app_state.selected_apps.clone();
        
        // Check if any modal is open
        let any_modal_open = app_state.is_voice_mode_open
            || app_state.is_account_settings_open
            || app_state.is_profile_settings_open;

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
                                    // App Picker Popover
                                    Button::new("add-app")
                                        .icon(IconName::Plus)
                                        .ghost()
                                        .rounded_full()
                                        .when(!any_modal_open, |this| this.cursor_pointer())
                                        .dropdown_menu_with_anchor(Corner::BottomLeft, {
                                            let state_model = state_model.clone();
                                            move |menu, window, cx| {
                                                let make_item = |label: &str, app_name: &str, icon: &str, action: Box<dyn Action>, state_model: Entity<AppState>| {
                                                    let state_model = state_model.clone();
                                                    let label_string = label.to_string();
                                                    let app_name_string = app_name.to_string();
                                                    let icon_string = icon.to_string();
                                                    PopupMenuItem::new(label_string)
                                                        .icon(Icon::default().path(icon_string))
                                                        .on_click(move |_, window, cx| {
                                                            state_model.update(cx, |state, cx| state.select_app(app_name_string.clone(), cx));
                                                            window.dispatch_action(action.boxed_clone(), cx);
                                                        })
                                                };

                                                let state_model_submenu = state_model.clone();

                                                menu
                                                    .item(make_item("Add photos & files", "Photos", "icons/clip.svg", Box::new(SelectAppPhotos), state_model.clone()))
                                                    .item(make_item("Image Generation", "Image Generation", "icons/create_image.svg", Box::new(SelectAppImageGeneration), state_model.clone()))
                                                    .item(make_item("Thinking", "Thinking", "icons/thinking.svg", Box::new(SelectAppThinking), state_model.clone()))
                                                    .item(make_item("Deep Research", "Deep Research", "icons/deep_search.svg", Box::new(SelectAppDeepResearch), state_model.clone()))
                                                    .item(make_item("Study", "Study", "icons/study.svg", Box::new(SelectAppStudy), state_model.clone()))
                                                    .separator()
                                                    .submenu("More", window, cx, move |menu, _, _| {
                                                        let make_item = |label: &str, app_name: &str, icon: &str, action: Box<dyn Action>, state_model: Entity<AppState>| {
                                                            let state_model = state_model.clone();
                                                            let label_string = label.to_string();
                                                            let app_name_string = app_name.to_string();
                                                            let icon_string = icon.to_string();
                                                            PopupMenuItem::new(label_string)
                                                                .icon(Icon::default().path(icon_string))
                                                                .on_click(move |_, window, cx| {
                                                                    state_model.update(cx, |state, cx| state.select_app(app_name_string.clone(), cx));
                                                                    window.dispatch_action(action.boxed_clone(), cx);
                                                                })
                                                        };
                                                        
                                                        menu
                                                            .item(make_item("Web search", "Web search", "icons/web_search.svg", Box::new(SelectAppWebSearch), state_model_submenu.clone()))
                                                            .item(make_item("Canvas", "Canvas", "icons/canvas.svg", Box::new(SelectAppCanvas), state_model_submenu.clone()))
                                                            .item(make_item("Canva", "Canva", "icons/canva.svg", Box::new(SelectAppCanva), state_model_submenu.clone()))
                                                            .item(make_item("Coursera", "Coursera", "icons/coursera.svg", Box::new(SelectAppCoursera), state_model_submenu.clone()))
                                                            .item(make_item("Figma", "Figma", "icons/figma.svg", Box::new(SelectAppFigma), state_model_submenu.clone()))
                                                            .item(make_item("Spotify", "Spotify", "icons/spotify.svg", Box::new(SelectAppSpotify), state_model_submenu.clone()))
                                                    })
                                            }
                                        })
                                )
                                .child(
                                        // Tags Area (Middle)
                                        div()
                                        .flex()
                                        .flex_wrap()
                                        .gap_2()
                                        .children({
                                            let (tool_calls, rest): (Vec<_>, Vec<_>) = selected_apps.iter()
                                                .cloned()
                                                .partition(|app| matches!(app.as_str(), "Web search" | "Deep Research" | "Image Generation" | "Photos" | "Thinking"));

                                            let (skills, mini_apps): (Vec<_>, Vec<_>) = rest.into_iter()
                                                .partition(|app| matches!(app.as_str(), "Study" | "Canvas"));

                                            let groups = vec![
                                                ("Tools", tool_calls, "icons/wrench.svg"),
                                                ("Skills", skills, "icons/wizard_hat.svg"),
                                                ("Apps", mini_apps, "icons/plugins.svg"),
                                            ];

                                            let state_model = state_model.clone();
                                            groups.into_iter().flat_map(move |(group_name, apps, icon_path)| -> Box<dyn Iterator<Item = AnyElement>> {
                                                if apps.len() >= 2 {
                                                    let apps_clone = apps.clone();
                                                    let state_model = state_model.clone();
                                                    let group_name = group_name.to_string();
                                                    let icon_path = icon_path.to_string();
                                                    
                                                    Box::new(std::iter::once(
                                                        Popover::new(SharedString::from(format!("aggregated-{}-popover", group_name.to_lowercase())))
                                                            .anchor(gpui::Corner::BottomLeft)
                                                            .trigger(
                                                                Button::new(SharedString::from(format!("aggregated-{}-btn", group_name.to_lowercase())))
                                                                    .ghost()
                                                                    .bg(secondary)
                                                                    .rounded_md()
                                                                    .px_2()
                                                                    .py_1()
                                                                    .child(
                                                                        h_flex()
                                                                            .gap_1()
                                                                            .items_center()
                                                                            .child(
                                                                                svg()
                                                                                    .path(icon_path.clone())
                                                                                    .size(px(12.0))
                                                                                    .text_color(secondary_foreground)
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .child(format!("{} {}", apps.len(), group_name.to_lowercase()))
                                                                                    .text_size(px(12.0)),
                                                                            )
                                                                            .child(
                                                                                Icon::new(IconName::ChevronDown)
                                                                                    .size(px(12.0))
                                                                                    .text_color(secondary_foreground)
                                                                            )
                                                                    )
                                                            )
                                                            .content(move |_, _, cx| {
                                                                let theme = cx.theme();
                                                                v_flex()
                                                                    .w(px(200.0))
                                                                    .p_1()
                                                                    .gap_1()
                                                                    .children(
                                                                        apps_clone.iter().enumerate().map(|(i, app)| {
                                                                            let app_name = app.clone();
                                                                            let icon_path = match app_name.as_str() {
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

                                                                            h_flex()
                                                                                .gap_2()
                                                                                .items_center()
                                                                                .px_2()
                                                                                .py_1()
                                                                                .rounded_sm()
                                                                                .hover(move |s| s.bg(theme.secondary))
                                                                                .cursor_pointer()
                                                                                .id(SharedString::from(format!("remove-{}-aggregated-{}", group_name.to_lowercase(), i)))
                                                                                .on_click({
                                                                                    let state_model = state_model.clone();
                                                                                    move |_event, _window, cx| {
                                                                                        state_model.update(cx, |state, cx| {
                                                                                            state.remove_app(app_name.clone(), cx);
                                                                                        });
                                                                                    }
                                                                                })
                                                                                .child(
                                                                                    svg()
                                                                                        .path(icon_path)
                                                                                        .size(px(12.0))
                                                                                        .text_color(theme.secondary_foreground)
                                                                                )
                                                                                .child(
                                                                                    div()
                                                                                        .child(app.clone())
                                                                                        .text_size(px(12.0))
                                                                                )
                                                                                .child(
                                                                                    div().flex_grow() // Spacer
                                                                                )
                                                                                .child(
                                                                                    Icon::new(IconName::Close)
                                                                                        .size(px(12.0))
                                                                                        .text_color(theme.secondary_foreground)
                                                                                )
                                                                        })
                                                                    )
                                                            })
                                                            .into_any_element()
                                                    ))
                                                } else {
                                                    // Render individual tags
                                                    let state_model = state_model.clone();
                                                    Box::new(apps.into_iter().enumerate().map(move |(i, app)| {
                                                            let app_name = app.clone();
                                                            let icon_path = match app_name.as_str() {
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
                                                                .bg(secondary)
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
                                                                        .id(SharedString::from(format!("remove-{}-{}", app_name.to_lowercase(), i)))
                                                                        .cursor_pointer()
                                                                        .on_click({
                                                                            let state_model = state_model.clone();
                                                                            move |_event, _window, cx| {
                                                                                state_model.update(cx, |state, cx| {
                                                                                    state.remove_app(app_name.clone(), cx);
                                                                                });
                                                                            }
                                                                        })
                                                                        .child(
                                                                            Icon::new(IconName::Close)
                                                                                .size(px(14.0)),
                                                                        ),
                                                                )
                                                                .into_any_element()
                                                        }))
                                                }
                                            })
                                        }),
                                )
                                )
                        .child(
                            // Right: Action Icons
                            h_flex()
                                .gap_1()
                                .items_center()
                                .when(self.voice_mode, |this| {
                                    // Voice Mode: Cancel (X) and Confirm (Check)
                                    this.child({
                                        let mut cancel_btn = div()
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
                                            .tooltip(|w, cx| Tooltip::new("Cancel").build(w, cx))
                                            .child(
                                                Icon::new(IconName::Close)
                                                    .text_color(secondary_foreground),
                                            );
                                        
                                        if !any_modal_open {
                                            cancel_btn = cancel_btn.cursor_pointer();
                                        }
                                        
                                        cancel_btn
                                    })
                                    .child({
                                        let mut confirm_btn = div()
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
                                            .tooltip(|w, cx| Tooltip::new("Done").build(w, cx))
                                            .child(
                                                Icon::new(IconName::Check)
                                                    .text_color(theme.background),
                                            );
                                        
                                        if !any_modal_open {
                                            confirm_btn = confirm_btn.cursor_pointer();
                                        }
                                        
                                        confirm_btn
                                    })
                                })
                                .when(!self.voice_mode, |this| {
                                    // Text Mode: Mic and Send/Headphone
                                    this.child({
                                        let mut mic_btn = div()
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
                                            .tooltip(|w, cx| Tooltip::new("Dictate").build(w, cx))
                                            .child(
                                                svg()
                                                    .path("icons/mic.svg")
                                                    .size(px(18.0))
                                                    .text_color(secondary_foreground),
                                            );
                                        
                                        if !any_modal_open {
                                            mic_btn = mic_btn.cursor_pointer();
                                        }
                                        
                                        mic_btn
                                    })
                                    .child(
                                        if self.input_state.read(cx).text().len() == 0 {
                                            // Empty state: Sparkles icon - opens voice mode modal
                                            let mut sparkles_btn = div()
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
                                                .tooltip(|w, cx| {
                                                    Tooltip::new("Voice Mode").build(w, cx)
                                                })
                                                .child(
                                                    svg()
                                                        .path("icons/sparkles.svg")
                                                        .size(px(18.0))
                                                        .text_color(secondary_foreground),
                                                );
                                            
                                            if !any_modal_open {
                                                sparkles_btn = sparkles_btn.cursor_pointer();
                                            }
                                            
                                            sparkles_btn
                                        } else {
                                            // Typing state: Send button (Black bg, White arrow)
                                            let mut send_btn = div()
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
                                                .tooltip(|w, cx| {
                                                    Tooltip::new("Send message").build(w, cx)
                                                })
                                                .child(
                                                    Icon::new(IconName::ArrowUp)
                                                        .text_color(theme.background),
                                                );
                                            
                                            if !any_modal_open {
                                                send_btn = send_btn.cursor_pointer();
                                            }
                                            
                                            send_btn
                                        }
                                    )
                                })
                        )
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
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppWebSearch, _, cx| {
                        println!("Action SelectAppWebSearch received");
                        state.update(cx, |state, cx| state.select_app("Web search".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppCanvas, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Canvas".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppCoursera, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Coursera".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppSpotify, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Spotify".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppPhotos, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Photos".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppImageGeneration, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Image Generation".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppThinking, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Thinking".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppDeepResearch, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Deep Research".to_string(), cx));
                    }
                })
                .on_action({
                    let state = self.state.clone();
                    move |_: &SelectAppStudy, _, cx| {
                        state.update(cx, |state, cx| state.select_app("Study".to_string(), cx));
                    }
                }))
        
    }
}
