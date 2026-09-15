mod items;
#[macro_use]
mod sync_macros;

pub use items::{render_flyout_item, render_popover_item};

use crate::actions::{
    SelectAppCanva, SelectAppCanvas, SelectAppCoursera, SelectAppDeepResearch, SelectAppFigma,
    SelectAppImageGeneration, SelectAppLinear, SelectAppNotion, SelectAppPhotos, SelectAppSpotify,
    SelectAppStudy, SelectAppThinking, SelectAppWebSearch,
};
use crate::audio::AudioInput;
use crate::components::voice_wave::VoiceWave;
use crate::icons::NativeIcon;
use crate::state::{AppState, ReplyTo, SubmitChord};
use gpui_kit::InteractiveElement;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName,
    button::{Button, ButtonVariants},
    h_flex,
    input::{InputEvent, Textarea, TextareaState},
    menu::{DropdownMenu, PopupMenuItem},
    popover::Popover,
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

actions!(chat, [SubmitMessage]);

type SubmitCallback = Box<dyn Fn(String, &mut Context<MessageInput>)>;

pub struct MessageInput {
    input_state: Entity<TextareaState>,
    on_submit: Option<SubmitCallback>,
    voice_mode: bool,
    voice_wave: Option<Entity<VoiceWave>>,
    audio_input: Option<AudioInput>,
    state: Entity<AppState>,
    // Cached state to avoid re-rendering on every AppState change
    selected_apps: Vec<String>,
    is_voice_mode_open: bool,
    is_app_settings_open: bool,
    submit_chord: SubmitChord,
    reply_to: Option<ReplyTo>,
    coworker_name: String,
}

impl MessageInput {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(format!("Message {}", composer_bot_name(&state.read(cx))))
                .auto_grow(1, 20)
                .submit_on_enter(true)
        });

        // Cache initial values from AppState
        let app_state = state.read(cx);
        let selected_apps = app_state.selected_apps.clone();
        let is_voice_mode_open = app_state.is_voice_mode_open;
        let is_app_settings_open = app_state.is_app_settings_open;
        let submit_chord = app_state.submit_chord;
        let reply_to = app_state.reply_to.clone();
        let coworker_name = composer_bot_name(&app_state);

        let this = Self {
            state: state.clone(),
            input_state: input_state.clone(),
            selected_apps,
            is_voice_mode_open,
            is_app_settings_open,
            submit_chord,
            reply_to,
            coworker_name: coworker_name.clone(),
            on_submit: None,
            voice_mode: false,
            voice_wave: None,
            audio_input: None,
        };

        // Subscribe to state changes to update cached values and notify only when relevant fields change
        cx.observe(&state, |this: &mut Self, state, cx| {
            let mut changed = false;
            {
                let state = state.read(cx);
                sync_field_clone!(this, state, selected_apps, changed);
                sync_field_copy!(this, state, is_voice_mode_open, changed);
                sync_field_copy!(this, state, is_app_settings_open, changed);
                sync_field_clone!(this, state, reply_to, changed);
                if this.submit_chord != state.submit_chord {
                    this.submit_chord = state.submit_chord;
                    changed = true;
                }
                let name = composer_bot_name(&state);
                if this.coworker_name != name {
                    this.coworker_name = name;
                    changed = true;
                }
            }

            if changed {
                let send_on_enter = this.submit_chord == SubmitChord::Enter;
                this.input_state.update(cx, |input, cx| {
                    input.set_submit_on_enter(send_on_enter, cx);
                });
                cx.notify();
            }
        })
        .detach();

        cx.subscribe_in(
            &input_state,
            window,
            |this, _state, event, window, cx| match event {
                InputEvent::PressEnter { secondary, shift } => {
                    let send = match this.submit_chord {
                        SubmitChord::Enter => !shift && !secondary,
                        SubmitChord::CommandEnter => *secondary,
                    };
                    if send {
                        this.trigger_submit(window, cx);
                    }
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();

        this
    }

    pub fn on_submit(mut self, handler: impl Fn(String, &mut Context<Self>) + 'static) -> Self {
        self.on_submit = Some(Box::new(handler));
        self
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input_state.update(cx, |state, cx| {
            state.focus_handle(cx).focus(window, cx);
        });
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
            match AudioInput::new(amplitude.clone()) {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state_model = self.state.clone();
        let placeholder = format!("Message {}", self.coworker_name);
        self.input_state.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
        });

        let theme = cx.theme();
        let secondary = theme.secondary;
        let secondary_foreground = theme.secondary_foreground;
        let border = theme.border;
        // Removed direct state read to prevent excessive re-renders
        // let app_state = state_model.read(cx);
        let selected_apps = self.selected_apps.clone();

        // Check if any modal is open using cached state
        let any_modal_open = self.is_voice_mode_open || self.is_app_settings_open;
        let draft = self.input_state.read(cx).value();
        let compact = !self.voice_mode && self.selected_apps.is_empty() && !draft.contains('\n');

        // ChatGPT-style: centered container with max-width
        h_flex().w_full().justify_center().child(
            // Input container - rounded pill shape with shadow
            v_flex()
                .key_context("MessageInput")
                .max_w(px(crate::chrome::CHAT_CONTENT_MAX))
                .w_full()
                .gap_2()
                .when(compact, |this| this.px_3().py(px(6.)))
                .when(!compact, |this| this.px_4().py_3())
                .bg(theme.background) // Match chat background (white in light mode)
                .border_1()
                .border_color(border)
                .rounded(px(26.0)) // Rounded pill shape
                .shadow_sm()
                .when_some(self.reply_to.clone(), |this, reply| {
                    let preview = reply.preview.clone();
                    this.child(
                        h_flex()
                            .id("reply-bar")
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_1()
                            .pb_1()
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                                            .text_color(theme.muted_foreground)
                                            .child("Replying"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .truncate()
                                            .child(preview),
                                    ),
                            )
                            .child(
                                div()
                                    .id("reply-dismiss")
                                    .size(px(22.))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(secondary))
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(12.))
                                            .text_color(secondary_foreground),
                                    )
                                    .on_click({
                                        let state_model = state_model.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            state_model.update(cx, |state, cx| {
                                                state.clear_reply_to(cx);
                                            });
                                        }
                                    }),
                            ),
                    )
                })
                .when(!compact, |this| {
                    this.child(
                        // Top: Input field (grows to fill space)
                        div().flex_grow(1.).child(if self.voice_mode {
                            if let Some(voice_wave) = &self.voice_wave {
                                voice_wave.clone().into_any_element()
                            } else {
                                div().into_any_element()
                            }
                        } else {
                            Textarea::new(&self.input_state)
                                .appearance(false)
                                .into_any_element()
                        }),
                    )
                })
                .child(
                    // Bottom: Toolbar (and the field, when the composer is one line)
                    h_flex()
                        .when(compact, |this| this.items_center().gap_1())
                        .when(!compact, |this| this.justify_between().items_start().gap_2())
                        .child(
                             // App Picker Popover (Moved out of wrapping container)
                            Button::new("add-app")
                                .icon(IconName::Plus)
                                .ghost()
                                .rounded_full()
                                .when(!any_modal_open, |this| this.cursor_pointer())
                                .dropdown_menu_with_anchor(Anchor::BottomLeft, {
                                    let state_model = state_model.clone();
                                    move |menu, window, cx| {
                                        let state = state_model.read(cx);
                                        let capabilities = state.capabilities.clone();
                                        
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

                                        fn get_action(action_id: &str) -> Box<dyn Action> {
                                            match action_id {
                                                "SelectAppPhotos" => Box::new(SelectAppPhotos),
                                                "SelectAppImageGeneration" => Box::new(SelectAppImageGeneration),
                                                "SelectAppThinking" => Box::new(SelectAppThinking),
                                                "SelectAppDeepResearch" => Box::new(SelectAppDeepResearch),
                                                "SelectAppStudy" => Box::new(SelectAppStudy),
                                                "SelectAppWebSearch" => Box::new(SelectAppWebSearch),
                                                "SelectAppCanvas" => Box::new(SelectAppCanvas),
                                                "SelectAppCanva" => Box::new(SelectAppCanva),
                                                "SelectAppCoursera" => Box::new(SelectAppCoursera),
                                                "SelectAppFigma" => Box::new(SelectAppFigma),
                                                "SelectAppSpotify" => Box::new(SelectAppSpotify),
                                                _ => Box::new(SelectAppWebSearch), // Fallback
                                            }
                                        }

                                        let mut menu = menu;
                                        
                                        // Primary Items
                                        for cap in capabilities.iter().filter(|c| c.is_primary) {
                                            menu = menu.item(make_item(
                                                &cap.label,
                                                &cap.name,
                                                &cap.icon,
                                                get_action(&cap.action_id),
                                                state_model.clone()
                                            ));
                                        }

                                        menu = menu.separator();

                                        // Secondary Items ("More" submenu)
                                        let secondary_caps: Vec<_> = capabilities.iter().filter(|c| !c.is_primary).cloned().collect();
                                        if !secondary_caps.is_empty() {
                                            let state_model_submenu = state_model.clone();
                                            let secondary_caps_for_submenu = secondary_caps.clone();
                                            menu = menu.submenu("More", window, cx, move |menu, _, _| {
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

                                                let mut submenu = menu;
                                                for cap in secondary_caps_for_submenu.iter() {
                                                    submenu = submenu.item(make_item(
                                                        &cap.label,
                                                        &cap.name,
                                                        &cap.icon,
                                                        get_action(&cap.action_id),
                                                        state_model_submenu.clone()
                                                    ));
                                                }
                                                submenu
                                            });
                                        }
                                        menu
                                    }
                                })
                        )
                        .when(!compact, |this| {
                        this.child(
                            // Bottom Row
                            div()
                                .flex()
                                .flex_1() // Allow this section to shrink/grow
                                .min_w_0() // Allow shrinking below content size to force wrapping
                                .flex_wrap() // Allow wrapping
                                .items_center()
                                .gap_2()
                                .children(
                                    std::iter::once(
                                        div().into_any_element() // Placeholder or remove entirely if not needed
                                    )
                                    .chain({
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
                                                        .anchor(Anchor::BottomLeft)
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
                                                                        let icon = tool_icon(&app_name);

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
                                                                                Icon::new(icon)
                                                                                    .size(px(12.0))
                                                                                    .text_color(theme.secondary_foreground)
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .child(app.clone())
                                                                                    .text_size(px(12.0))
                                                                            )
                                                                            .child(
                                                                                div().flex_grow(1.) // Spacer
                                                                            )
                                                                            .child(
                                                                                Icon::new(NativeIcon::Close)
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
                                                        let icon = tool_icon(&app_name);

                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap_1()
                                                            .bg(secondary)
                                                            .rounded_md()
                                                            .px_2()
                                                            .py_1()
                                                            .child(
                                                                Icon::new(icon)
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
                                                                        Icon::new(NativeIcon::Close)
                                                                            .size(px(14.0)),
                                                                    ),
                                                            )
                                                            .into_any_element()
                                                    }))
                                            }
                                        })
                                    })
                                )
                                )
                        })
                        .when(compact, |this| {
                            this.child(
                                div()
                                    .id("composer-field")
                                    .flex_1()
                                    .min_w_0()
                                    .w_full()
                                    .child(
                                        Textarea::new(&self.input_state)
                                            .appearance(false)
                                            .w_full(),
                                    ),
                            )
                        })
                        .child(
                            // Right: Action Icons
                            h_flex()
                                .flex_none() // Prevent this section from shrinking
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
                                            .bg(gpui_kit::transparent_black())
                                            .text_color(secondary_foreground)
                                            .hover(move |style| style.bg(secondary))
                                            .tooltip(|w, cx| Tooltip::new("Cancel").build(w, cx))
                                            .child(
                                                Icon::new(NativeIcon::Close)
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
                                            .bg(gpui_kit::transparent_black()) // Transparent/White by default
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
                                                .bg(gpui_kit::transparent_black())
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

fn tool_icon(name: &str) -> Icon {
    match name {
        "Image Generation" => Icon::new(NativeIcon::CreateImage),
        "Thinking" => Icon::new(NativeIcon::Thinking),
        "Deep Research" => Icon::new(NativeIcon::DeepSearch),
        "Study" => Icon::new(NativeIcon::Study),
        "Web search" => Icon::new(NativeIcon::WebSearch),
        "Canvas" => Icon::new(NativeIcon::Canvas),
        "Canva" => Icon::new(NativeIcon::Canva),
        "Coursera" => Icon::new(NativeIcon::Coursera),
        "Figma" => Icon::new(NativeIcon::Figma),
        "Spotify" => Icon::new(NativeIcon::Spotify),
        _ => Icon::new(NativeIcon::Clip),
    }
}

fn composer_bot_name(state: &AppState) -> String {
    state
        .active_coworker_id
        .as_ref()
        .and_then(|id| state.coworkers.iter().find(|c| &c.id == id))
        .map(|c| c.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "bot".into())
}
