use gpui_kit::component::Root;
use gpui_kit::*;
use nativechat::actions::{
    About, BranchInNewChat, CopyMessage, Hide, HideOthers, Minimize, NewChat, OpenSettings, Quit,
    ReadAloud, ReportMessage, ShowAll, ToggleDebugMarkdown, ToggleFps, ToggleSidebar, ToggleTheme,
    Zoom,
};
use nativechat::assets::CombinedAssets;
use nativechat::components::chat_input::SubmitMessage;
use nativechat::config::Config;
use nativechat::db::{create_pool, run_migrations};
use nativechat::root::RootView;
use nativechat::services::database::DatabaseService;
use nativechat::state::AppState;
use nativechat::theme;

fn main() {
    #[cfg(feature = "agent")]
    let mailbox = nativechat::agent::maybe_start();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");
    let _guard = runtime.enter();

    dotenv::from_filename(".env.local").ok();
    dotenv::dotenv().ok();

    let (config, db_service) = runtime.block_on(async {
        let config = Config::load().expect("Failed to load config");
        println!("Debug: Database URL: {}", config.database_url);

        let pool = create_pool(&config.database_url)
            .await
            .expect("Failed to create database pool");

        run_migrations(&pool)
            .await
            .expect("Failed to run migrations");

        let db_service = DatabaseService::new(pool);

        (config, db_service)
    });

    gpui_kit::application()
        .with_assets(CombinedAssets)
        .run(move |cx: &mut App| {
            #[cfg(feature = "agent")]
            let mailbox = mailbox.clone();
            cx.bind_keys([
                KeyBinding::new("enter", SubmitMessage, Some("MessageInput")),
                KeyBinding::new("cmd-enter", SubmitMessage, Some("Editor")),
                KeyBinding::new("ctrl-enter", SubmitMessage, Some("Editor")),
                KeyBinding::new("cmd-b", ToggleSidebar, None),
                KeyBinding::new("cmd-t", ToggleTheme, None),
                KeyBinding::new("cmd-n", NewChat, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-f12", ToggleDebugMarkdown, None),
                KeyBinding::new("cmd-shift-f", ToggleFps, None),
                KeyBinding::new("cmd-shift-c", CopyMessage, None),
            ]);

            cx.on_action(quit);

            gpui_kit::init(cx);
            theme::init(cx);

            set_menus(cx);

            let displays = cx.displays();
            let display = displays.first().expect("No display found");
            let bounds = display.bounds();

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(520.), px(400.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("NativeChat".into()),
                    ..TitlebarOptions::default()
                }),
                ..WindowOptions::default()
            };

            cx.spawn(async move |cx| {
                cx.open_window(options, |window, cx| {
                    let state = cx.new(|_| AppState::new());

                    state.update(cx, |state, cx| {
                        state.set_config(config.clone(), cx);
                        state.set_database_service(db_service.clone(), cx);
                    });

                    let state_clone = state.clone();
                    let db_service_clone = db_service.clone();
                    cx.on_action(|_: &CopyMessage, _cx: &mut App| {});
                    cx.on_action(|_: &BranchInNewChat, _cx: &mut App| {
                        println!("Branch in new chat action triggered");
                    });
                    let state_read_aloud = state.clone();
                    cx.on_action(move |action: &ReadAloud, cx: &mut App| {
                        state_read_aloud.update(cx, |state, cx| {
                            state.read_aloud(
                                action.text.clone(),
                                action.message_id.clone(),
                                nativechat::actions::TtsSource::Native,
                                cx,
                            );
                        });
                    });
                    cx.on_action(|_: &ReportMessage, _cx: &mut App| {
                        println!("Report message action triggered");
                    });
                    cx.spawn(async move |cx| {
                        if let Err(e) =
                            nativechat::services::model_seeder::seed_models(&db_service_clone).await
                        {
                            eprintln!("Failed to seed models: {}", e);
                        }

                        match AppState::load_profiles_and_credentials(&db_service_clone).await {
                            Ok((profiles, credentials)) => {
                                let _ = state_clone.update(cx, |state, cx| {
                                    state.set_profiles_and_credentials(profiles, credentials, cx);
                                });
                            }
                            Err(e) => {
                                eprintln!("Failed to load profiles and credentials: {}", e);
                            }
                        }

                        println!("App data refreshed from DB successfully!");
                    })
                    .detach();

                    let view = cx.new(|cx| {
                        #[cfg(feature = "agent")]
                        {
                            RootView::new(window, state, cx).attach_agent(mailbox.clone(), cx)
                        }
                        #[cfg(not(feature = "agent"))]
                        {
                            RootView::new(window, state, cx)
                        }
                    });
                    view.update(cx, |view, cx| {
                        view.focus_handle.focus(window, cx);
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("Failed to open window");
            })
            .detach();
        });
}

fn set_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            name: "NativeChat".into(),
            disabled: false,
            items: vec![
                MenuItem::action("About NativeChat", About),
                MenuItem::separator(),
                MenuItem::action("Settings...", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide NativeChat", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit NativeChat", Quit),
            ],
        },
        Menu {
            name: "Edit".into(),
            disabled: false,
            items: vec![MenuItem::separator()],
        },
        Menu {
            name: "Window".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
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
