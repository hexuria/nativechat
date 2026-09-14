use gpui_kit::component::Root;
use gpui_kit::*;
use nativechat::actions::{
    About, BranchInNewChat, ClearSearch, CloseBotFinder, CloseSettings, CopyMessage, Hide,
    HideOthers, Minimize, NewChat, OpenSettings, PickFinderItem, Quit, ReadAloud, ReportMessage,
    CloseCommandPalette, CloseFind, FindNext, FindPrev, FocusChatInput, NavBack, NavForward,
    OpenCommandPalette, PaletteNextTab, PalettePrevTab, PaletteSelectNext, PaletteSelectPrev,
    Search, ShowAll, ToggleAgentSettings,
    ToggleComputerPane, ToggleDebugMarkdown, ToggleFps,
    ToggleMiniSidebar,
    ToggleSidebar, ToggleTheme, Zoom,
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
                KeyBinding::new("cmd-b", ToggleSidebar, Some("Editor")),
                KeyBinding::new("cmd-b", ToggleSidebar, Some("AppSettings")),
                KeyBinding::new("cmd-shift-h", ToggleMiniSidebar, None),
                KeyBinding::new("cmd-shift-h", ToggleMiniSidebar, Some("Editor")),
                KeyBinding::new("cmd-shift-h", ToggleMiniSidebar, Some("AppSettings")),
                KeyBinding::new("cmd-shift-b", ToggleAgentSettings, None),
                KeyBinding::new("cmd-shift-b", ToggleAgentSettings, Some("Editor")),
                KeyBinding::new("cmd-shift-b", ToggleAgentSettings, Some("AppSettings")),
                KeyBinding::new("cmd-shift-m", ToggleComputerPane, None),
                KeyBinding::new("cmd-shift-m", ToggleComputerPane, Some("Editor")),
                KeyBinding::new("cmd-shift-m", ToggleComputerPane, Some("AppSettings")),
                KeyBinding::new("cmd-[", NavBack, None),
                KeyBinding::new("cmd-[", NavBack, Some("Editor")),
                KeyBinding::new("cmd-[", NavBack, Some("AppSettings")),
                KeyBinding::new("cmd-[", NavBack, Some("Root")),
                KeyBinding::new("cmd-[", NavBack, Some("CommandPalette")),
                KeyBinding::new("cmd-]", NavForward, None),
                KeyBinding::new("cmd-]", NavForward, Some("Editor")),
                KeyBinding::new("cmd-]", NavForward, Some("AppSettings")),
                KeyBinding::new("cmd-]", NavForward, Some("Root")),
                KeyBinding::new("cmd-]", NavForward, Some("CommandPalette")),
                KeyBinding::new("cmd-t", ToggleTheme, None),
                KeyBinding::new("cmd-t", ToggleTheme, Some("Editor")),
                KeyBinding::new("cmd-t", ToggleTheme, Some("AppSettings")),
                KeyBinding::new("cmd-n", NewChat, None),
                KeyBinding::new("cmd-n", NewChat, Some("Editor")),
                KeyBinding::new("cmd-n", NewChat, Some("MessageInput")),
                KeyBinding::new("cmd-n", NewChat, Some("BotFinder")),
                KeyBinding::new("cmd-n", NewChat, Some("CommandPalette")),
                KeyBinding::new("cmd-n", NewChat, Some("AppSettings")),
                KeyBinding::new("cmd-n", NewChat, Some("Root")),
                KeyBinding::new("cmd-k", OpenCommandPalette, None),
                KeyBinding::new("cmd-k", OpenCommandPalette, Some("Root")),
                KeyBinding::new("cmd-k", OpenCommandPalette, Some("Editor")),
                KeyBinding::new("cmd-k", OpenCommandPalette, Some("MessageInput")),
                KeyBinding::new("cmd-k", OpenCommandPalette, Some("BotFinder")),
                KeyBinding::new("cmd-k", OpenCommandPalette, Some("CommandPalette")),
                KeyBinding::new("cmd-k", OpenCommandPalette, Some("AppSettings")),
                KeyBinding::new("cmd-l", FocusChatInput, None),
                KeyBinding::new("cmd-l", FocusChatInput, Some("Root")),
                KeyBinding::new("cmd-l", FocusChatInput, Some("Editor")),
                KeyBinding::new("cmd-l", FocusChatInput, Some("MessageInput")),
                KeyBinding::new("cmd-l", FocusChatInput, Some("BotFinder")),
                KeyBinding::new("cmd-l", FocusChatInput, Some("CommandPalette")),
                KeyBinding::new("cmd-l", FocusChatInput, Some("AppSettings")),
                KeyBinding::new("cmd-f", Search, None),
                KeyBinding::new("cmd-f", Search, Some("Root")),
                KeyBinding::new("cmd-f", Search, Some("Editor")),
                KeyBinding::new("cmd-f", Search, Some("Input")),
                KeyBinding::new("cmd-f", Search, Some("MessageInput")),
                KeyBinding::new("cmd-f", Search, Some("BotFinder")),
                KeyBinding::new("cmd-f", Search, Some("CommandPalette")),
                KeyBinding::new("cmd-f", Search, Some("AppSettings")),
                KeyBinding::new("cmd-f", Search, Some("FindInChat")),
                KeyBinding::new("cmd-g", FindNext, None),
                KeyBinding::new("cmd-g", FindNext, Some("Root")),
                KeyBinding::new("cmd-g", FindNext, Some("Editor")),
                KeyBinding::new("cmd-g", FindNext, Some("Input")),
                KeyBinding::new("cmd-g", FindNext, Some("MessageInput")),
                KeyBinding::new("cmd-g", FindNext, Some("FindInChat")),
                KeyBinding::new("cmd-shift-g", FindPrev, None),
                KeyBinding::new("cmd-shift-g", FindPrev, Some("Root")),
                KeyBinding::new("cmd-shift-g", FindPrev, Some("Editor")),
                KeyBinding::new("cmd-shift-g", FindPrev, Some("Input")),
                KeyBinding::new("cmd-shift-g", FindPrev, Some("MessageInput")),
                KeyBinding::new("cmd-shift-g", FindPrev, Some("FindInChat")),
                KeyBinding::new("escape", CloseFind, Some("FindInChat")),
                KeyBinding::new("escape", CloseBotFinder, Some("BotFinder")),
                KeyBinding::new("escape", CloseCommandPalette, Some("CommandPalette")),
                KeyBinding::new("escape", CloseCommandPalette, Some("Root")),
                KeyBinding::new("tab", PaletteNextTab, Some("CommandPalette")),
                KeyBinding::new("shift-tab", PalettePrevTab, Some("CommandPalette")),
                KeyBinding::new("down", PaletteSelectNext, Some("CommandPalette")),
                KeyBinding::new("ctrl-n", PaletteSelectNext, Some("CommandPalette")),
                KeyBinding::new("up", PaletteSelectPrev, Some("CommandPalette")),
                KeyBinding::new("ctrl-p", PaletteSelectPrev, Some("CommandPalette")),
                KeyBinding::new("cmd-1", PickFinderItem { index: 0 }, Some("BotFinder")),
                KeyBinding::new("cmd-1", PickFinderItem { index: 0 }, Some("CommandPalette")),
                KeyBinding::new("cmd-1", PickFinderItem { index: 0 }, Some("Input")),
                KeyBinding::new("cmd-1", PickFinderItem { index: 0 }, Some("Editor")),
                KeyBinding::new("cmd-1", PickFinderItem { index: 0 }, Some("MessageInput")),
                KeyBinding::new("cmd-1", PickFinderItem { index: 0 }, Some("Root")),
                KeyBinding::new("cmd-2", PickFinderItem { index: 1 }, Some("BotFinder")),
                KeyBinding::new("cmd-2", PickFinderItem { index: 1 }, Some("CommandPalette")),
                KeyBinding::new("cmd-2", PickFinderItem { index: 1 }, Some("Input")),
                KeyBinding::new("cmd-2", PickFinderItem { index: 1 }, Some("Editor")),
                KeyBinding::new("cmd-2", PickFinderItem { index: 1 }, Some("MessageInput")),
                KeyBinding::new("cmd-2", PickFinderItem { index: 1 }, Some("Root")),
                KeyBinding::new("cmd-3", PickFinderItem { index: 2 }, Some("BotFinder")),
                KeyBinding::new("cmd-3", PickFinderItem { index: 2 }, Some("CommandPalette")),
                KeyBinding::new("cmd-3", PickFinderItem { index: 2 }, Some("Input")),
                KeyBinding::new("cmd-3", PickFinderItem { index: 2 }, Some("Editor")),
                KeyBinding::new("cmd-3", PickFinderItem { index: 2 }, Some("MessageInput")),
                KeyBinding::new("cmd-3", PickFinderItem { index: 2 }, Some("Root")),
                KeyBinding::new("cmd-4", PickFinderItem { index: 3 }, Some("BotFinder")),
                KeyBinding::new("cmd-4", PickFinderItem { index: 3 }, Some("CommandPalette")),
                KeyBinding::new("cmd-4", PickFinderItem { index: 3 }, Some("Input")),
                KeyBinding::new("cmd-4", PickFinderItem { index: 3 }, Some("Editor")),
                KeyBinding::new("cmd-4", PickFinderItem { index: 3 }, Some("MessageInput")),
                KeyBinding::new("cmd-4", PickFinderItem { index: 3 }, Some("Root")),
                KeyBinding::new("cmd-5", PickFinderItem { index: 4 }, Some("BotFinder")),
                KeyBinding::new("cmd-5", PickFinderItem { index: 4 }, Some("CommandPalette")),
                KeyBinding::new("cmd-5", PickFinderItem { index: 4 }, Some("Input")),
                KeyBinding::new("cmd-5", PickFinderItem { index: 4 }, Some("Editor")),
                KeyBinding::new("cmd-5", PickFinderItem { index: 4 }, Some("MessageInput")),
                KeyBinding::new("cmd-5", PickFinderItem { index: 4 }, Some("Root")),
                KeyBinding::new("cmd-6", PickFinderItem { index: 5 }, Some("BotFinder")),
                KeyBinding::new("cmd-6", PickFinderItem { index: 5 }, Some("CommandPalette")),
                KeyBinding::new("cmd-6", PickFinderItem { index: 5 }, Some("Input")),
                KeyBinding::new("cmd-6", PickFinderItem { index: 5 }, Some("Editor")),
                KeyBinding::new("cmd-6", PickFinderItem { index: 5 }, Some("MessageInput")),
                KeyBinding::new("cmd-6", PickFinderItem { index: 5 }, Some("Root")),
                KeyBinding::new("cmd-7", PickFinderItem { index: 6 }, Some("BotFinder")),
                KeyBinding::new("cmd-7", PickFinderItem { index: 6 }, Some("CommandPalette")),
                KeyBinding::new("cmd-7", PickFinderItem { index: 6 }, Some("Input")),
                KeyBinding::new("cmd-7", PickFinderItem { index: 6 }, Some("Editor")),
                KeyBinding::new("cmd-7", PickFinderItem { index: 6 }, Some("MessageInput")),
                KeyBinding::new("cmd-7", PickFinderItem { index: 6 }, Some("Root")),
                KeyBinding::new("cmd-8", PickFinderItem { index: 7 }, Some("BotFinder")),
                KeyBinding::new("cmd-8", PickFinderItem { index: 7 }, Some("CommandPalette")),
                KeyBinding::new("cmd-8", PickFinderItem { index: 7 }, Some("Input")),
                KeyBinding::new("cmd-8", PickFinderItem { index: 7 }, Some("Editor")),
                KeyBinding::new("cmd-8", PickFinderItem { index: 7 }, Some("MessageInput")),
                KeyBinding::new("cmd-8", PickFinderItem { index: 7 }, Some("Root")),
                KeyBinding::new("cmd-9", PickFinderItem { index: 8 }, Some("BotFinder")),
                KeyBinding::new("cmd-9", PickFinderItem { index: 8 }, Some("CommandPalette")),
                KeyBinding::new("cmd-9", PickFinderItem { index: 8 }, Some("Input")),
                KeyBinding::new("cmd-9", PickFinderItem { index: 8 }, Some("Editor")),
                KeyBinding::new("cmd-9", PickFinderItem { index: 8 }, Some("MessageInput")),
                KeyBinding::new("cmd-9", PickFinderItem { index: 8 }, Some("Root")),
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("cmd-,", OpenSettings, Some("Editor")),
                KeyBinding::new("cmd-,", OpenSettings, Some("AppSettings")),
                KeyBinding::new("escape", CloseSettings, Some("AppSettings")),
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
                        state.warm_tts(cx);
                    });

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
                MenuItem::action("New Bot", NewChat),
                MenuItem::action("Command Palette", OpenCommandPalette),
                MenuItem::action("Focus Chat", FocusChatInput),
                MenuItem::separator(),
                MenuItem::action("Back", NavBack),
                MenuItem::action("Forward", NavForward),
                MenuItem::separator(),
                MenuItem::action("Toggle Sidebar", ToggleSidebar),
                MenuItem::action("Toggle Mini Sidebar", ToggleMiniSidebar),
                MenuItem::action("Toggle Agent Settings", ToggleAgentSettings),
                MenuItem::action("Toggle Agent Screen", ToggleComputerPane),
                MenuItem::action("Toggle Theme", ToggleTheme),
            ],
        },
    ]);
}

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}
