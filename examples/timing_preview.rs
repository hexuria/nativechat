//! Offline visual fixture using the real NativeChat feed, not a separately drawn mockup.
//! Run with `cargo run --example timing_preview --features agent`.
//! No account, database, or network service is opened; all durations below are illustrative.
use gpui_kit::component::Root;
use gpui_kit::*;
use nativechat::assets::CombinedAssets;
use nativechat::opengrok::{Account, ChatPart, Coworker, StepSpec, ThoughtSpec, TurnTiming};
use nativechat::root::RootView;
use nativechat::state::{AppState, AuthStatus, Conversation, Message};

fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("preview runtime");
    let _guard = runtime.enter();
    #[cfg(feature = "agent")]
    let mailbox = nativechat::agent::maybe_start();
    gpui_kit::application()
        .with_assets(CombinedAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            nativechat::components::composer_editor::init(cx);
            nativechat::theme::init(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1000.), px(760.)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("NativeChat — offline timing preview".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| {
                let state = cx.new(|_| fixture());
                let view = cx.new(|cx| {
                    let view = RootView::new(window, state, cx);
                    #[cfg(feature = "agent")]
                    let view = view.attach_agent(mailbox.clone(), cx);
                    view
                });
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("preview window");
        });
}

fn fixture() -> AppState {
    let mut state = AppState::new();
    state.auth_status = AuthStatus::SignedIn;
    state.account = Some(Account {
        id: "offline-preview".into(),
        email: "preview@example.invalid".into(),
        first_name: "Offline".into(),
        last_name: "Preview".into(),
        avatar_url: None,
        org_id: None,
        verified: true,
        enabled: true,
        is_admin: None,
    });
    state.show_turn_timing = true;
    state.sidebar_hidden = true;
    state.active_coworker_id = Some("offline-bot".into());
    state.coworkers.push(
        serde_json::from_value::<Coworker>(serde_json::json!({
            "id": "offline-bot", "name": "Timing preview", "model": "offline"
        }))
        .expect("preview bot"),
    );
    state.active_conversation_id = Some("timing-preview".into());
    let mut parts = vec![ChatPart::Reasoning(ThoughtSpec {
        text: "Check the implementation, inspect its configuration, search for context, then update the file and run the tests.".into(),
        took_ms: Some(5000),
    })];
    for (id, tool, arguments, result, took_ms) in [
        (
            "read-1",
            "read_file",
            r#"{"path":"src/app.rs"}"#,
            "fn main() {\n    render_chat();\n}",
            2000,
        ),
        (
            "read-2",
            "read_file",
            r#"{"path":"src/config.rs"}"#,
            "show_turn_timing = true",
            3000,
        ),
        (
            "search",
            "web_search",
            r#"{"query":"inline action timing"}"#,
            "Found three relevant references.",
            8000,
        ),
        (
            "write",
            "write_file",
            r#"{"path":"src/app.rs"}"#,
            "Updated src/app.rs.",
            4000,
        ),
        (
            "command",
            "shell",
            r#"{"command":"cargo test"}"#,
            "running 6 tests\ntest result: ok. 6 passed; 0 failed",
            8000,
        ),
    ] {
        parts.push(ChatPart::Step(StepSpec {
            call_id: id.into(),
            tool: tool.into(),
            arguments: arguments.into(),
            result: Some(result.into()),
            ok: Some(true),
            took_ms: Some(took_ms),
        }));
    }
    parts.push(ChatPart::Text("Updated the implementation. All tests passed.\n\nOffline visual fixture — illustrative durations.".into()));
    state.conversations.push(Conversation {
        id: "timing-preview".into(), title: "Action timing preview".into(), created_at: String::new(), updated_at: String::new(), unread_count: 0, origin: None,
        messages: vec![Message {
            id: "timing-preview-reply".into(), sender: "AI".into(), content: "Updated the implementation. All tests passed.\n\nOffline visual fixture — illustrative durations.".into(),
            sent_at: std::time::SystemTime::now(), finished_at: None,
            reply_source: None,
            run_timing: TurnTiming::from_value(&serde_json::json!({"total_ms":30000,"model_ms":[5000],"tool_wait_ms":25000})),
            is_me: false, reply_preview: None, reply_to_id: None, reply_is_me: false, parts, run_id: None, hidden: false,
        }],
    });
    state
}
