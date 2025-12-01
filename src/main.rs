use gpui::*;
use nativechat::actions::{
    About, Hide, HideOthers, Minimize, OpenSettings, Quit, ShowAll, ToggleSidebar, ToggleTheme,
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
use ui::Root;

fn main() {
    // Create tokio runtime for async operations
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");
    let _guard = runtime.enter();

    // Load environment variables
    dotenv::from_filename(".env.local").ok();
    dotenv::dotenv().ok();

    Application::new()
        .with_assets(CombinedAssets)
        .run(|cx: &mut App| {
            cx.bind_keys([
                // Enter to submit in MessageInput context
                KeyBinding::new("enter", SubmitMessage, Some("MessageInput")),
                // Cmd+Enter to submit in Editor context
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

            // Initialize UI Components
            ui::init(cx);

            // Initialize Theme
            theme::init(cx);

            // Set up menus
            set_menus(cx);

            let displays = cx.displays();
            let display = displays.first().expect("No display found");
            let bounds = display.bounds();

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("NativeChat".into()),
                    ..TitlebarOptions::default()
                }),
                ..WindowOptions::default()
            };

            cx.open_window(options, |window, cx| {
                let state = cx.new(|_| AppState::new());

                // Initialize config and LLM provider
                let state_clone = state.clone();
                cx.spawn(|cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        // Load config
                        let config = match Config::load() {
                            Ok(config) => config,
                            Err(e) => {
                                eprintln!("Failed to load config: {}", e);
                                return;
                            }
                        };

                        println!("Debug: Database URL: {}", config.database_url);
                        println!("Debug: Default provider: {}", config.default_provider);

                        // Create connection pool
                        let pool = match create_pool(&config.database_url).await {
                            Ok(pool) => pool,
                            Err(e) => {
                                eprintln!("Failed to create database pool: {}", e);
                                return;
                            }
                        };

                        // Run migrations
                        if let Err(e) = run_migrations(&pool).await {
                            eprintln!("Failed to run migrations: {}", e);
                            return;
                        }

                        // Create DatabaseService
                        let db_service = DatabaseService::new(pool);

                        // Seed models
                        if let Err(e) =
                            nativechat::services::model_seeder::seed_models(&db_service).await
                        {
                            eprintln!("Failed to seed models: {}", e);
                        }

                        // Load profiles and credentials from database
                        let (profiles, credentials) =
                            match AppState::load_profiles_and_credentials(&db_service).await {
                                Ok(data) => data,
                                Err(e) => {
                                    eprintln!("Failed to load profiles and credentials: {}", e);
                                    (Vec::new(), Vec::new())
                                }
                            };

                        // Restore selected profile from settings
                        let restored_profile_id = match AppState::restore_selected_profile(
                            &db_service,
                            &profiles,
                        )
                        .await
                        {
                            Ok(id) => id,
                            Err(e) => {
                                eprintln!("Failed to restore selected profile: {}", e);
                                None
                            }
                        };

                        // Update state with config and services
                        let _ = state_clone.update(&mut cx, |state, cx| {
                            state.set_database_service(db_service, cx);
                            state.set_profiles_and_credentials(profiles, credentials, cx);

                            // Set the restored profile and update LLM provider
                            if let Some(profile_id) = restored_profile_id {
                                state.active_profile_id = Some(profile_id);
                                state.update_llm_provider(cx);
                            }

                            state.set_config(config, cx);
                        });

                        println!("App initialized successfully!");
                    }
                })
                .detach();

                let view = cx.new(|cx| RootView::new(window, state, cx));
                view.update(cx, |view, _cx| {
                    view.focus_handle.focus(window);
                });
                cx.new(|cx| Root::new(view, window, cx))
            })
            .unwrap();
        });
}

fn set_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            name: "NativeChat".into(),
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
            items: vec![MenuItem::separator()],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
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
