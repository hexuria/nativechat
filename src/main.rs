use gpui::*;
use gpui_component::Root;
use gpui_component_assets::Assets;
use nativechat::components::input::SubmitMessage;
use nativechat::root::RootView;
use nativechat::theme;

use nativechat::state::AppState;

fn main() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        cx.bind_keys([
            // Enter to submit in MessageInput context
            KeyBinding::new("enter", SubmitMessage, Some("MessageInput")),
            // Cmd+Enter to submit in Editor context (to override default behavior or ensure it works)
            KeyBinding::new("cmd-enter", SubmitMessage, Some("Editor")),
            KeyBinding::new("ctrl-enter", SubmitMessage, Some("Editor")),
        ]);

        // Initialize GPUI Components
        gpui_component::init(cx);

        // Initialize Theme
        theme::init(cx);

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
            });

            let view = cx.new(|cx| RootView::new(window, state, cx));
            cx.new(|cx| Root::new(view, window, cx))
        })
        .unwrap();
    });
}
