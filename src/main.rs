use gpui::*;
use gpui_component::Root;
use nativechat::actions::{Minimize, OpenSettings, Quit, ToggleSidebar, ToggleTheme, Zoom};
use nativechat::assets::CombinedAssets;
use nativechat::components::input::SubmitMessage;
use nativechat::root::RootView;
use nativechat::theme;

use nativechat::state::{AppState, VoiceStatus};

fn main() {
    Application::new()
        .with_assets(CombinedAssets)
        .run(|cx: &mut App| {
            cx.bind_keys([
                // Enter to submit in MessageInput context
                KeyBinding::new("enter", SubmitMessage, Some("MessageInput")),
                // Cmd+Enter to submit in Editor context (to override default behavior or ensure it works)
                KeyBinding::new("cmd-enter", SubmitMessage, Some("Editor")),
                KeyBinding::new("ctrl-enter", SubmitMessage, Some("Editor")),
                // Global shortcuts
                KeyBinding::new("cmd-b", ToggleSidebar, None),
                KeyBinding::new("cmd-t", ToggleTheme, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("cmd-q", Quit, None),
            ]);

            // Register actions
            cx.on_action(quit);

            // Initialize GPUI Components
            gpui_component::init(cx);

            // Initialize Theme
            theme::init(cx);

            // Set up menus
            set_menus(cx);

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1200.0), px(800.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("NativeChat".into()),
                    ..TitlebarOptions::default()
                }),
                ..WindowOptions::default()
            };

            cx.open_window(options, |window, cx| {
                let state = cx.new(|_| AppState {
                    conversations: vec![
                        nativechat::state::Conversation {
                            id: 1,
                            title: "John Doe".to_string(),
                            messages: vec![
                                nativechat::state::Message {
                                    id: 1,
                                    sender: "John Doe".to_string(),
                                    content: "Hello there!".to_string(),
                                    sent_at: std::time::SystemTime::now(),
                                    is_me: false,
                                },
                                nativechat::state::Message {
                                    id: 2,
                                    sender: "Me".to_string(),
                                    content: "Hi John!".to_string(),
                                    sent_at: std::time::SystemTime::now(),
                                    is_me: true,
                                },
                            ],
                            unread_count: 0,
                        },
                        nativechat::state::Conversation {
                            id: 2,
                            title: "Jane Smith".to_string(),
                            messages: vec![nativechat::state::Message {
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
                    audio_input: None,
                    gemini_client: None,
                });

                let view = cx.new(|cx| RootView::new(window, state, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .unwrap();
        });
}

fn set_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            name: "NativeChat".into(),
            items: vec![MenuItem::action("Quit", Quit)],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Toggle Theme", ToggleTheme),
            ],
        },
    ]);
}

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}
