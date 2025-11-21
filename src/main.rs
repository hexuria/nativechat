use gpui::*;
use gpui_component::Root;
use gpui_component_assets::Assets;
use nativechat::root::RootView;
use nativechat::theme;

fn main() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        // Initialize GPUI Components
        gpui_component::init(cx);

        // Initialize Theme
        theme::init(cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(800.0), px(600.0)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("NativeChat".into()),
                ..TitlebarOptions::default()
            }),
            ..WindowOptions::default()
        };

        cx.open_window(options, |window, cx| {
            let view = cx.new(|cx| RootView::new(window, cx));
            // Root must be the first-level child
            cx.new(|cx| Root::new(view, window, cx))
        })
        .unwrap();
    });
}
