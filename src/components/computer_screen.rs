//! The coworker's screen (noVNC) in its own window. macOS only: the page is a
//! `wry` WebView laid over the GPUI window; other platforms never build this.

use std::rc::Rc;

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;
use raw_window_handle::HasWindowHandle;
use wry::{
    Rect, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize, Position, Size},
};

pub struct ComputerScreen {
    /// The page, or why there is none. A WebView that would not build must not
    /// take the app down with it: the window says what went wrong instead.
    webview: Result<Rc<wry::WebView>, String>,
}

impl ComputerScreen {
    pub fn new(url: &str, window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let webview = window
            .window_handle()
            .map_err(|error| format!("the window has no native handle: {error}"))
            .and_then(|handle| {
                WebViewBuilder::new()
                    .with_url(url)
                    .build_as_child(&handle)
                    .map_err(|error| format!("the screen could not be loaded: {error}"))
            })
            .map(Rc::new);
        if let Err(error) = &webview {
            eprintln!("NativeChat computer: {error}");
        }
        Self { webview }
    }
}

impl Render for ComputerScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.webview {
            Ok(webview) => {
                let webview = webview.clone();
                div().size_full().child(
                    canvas(
                        move |bounds, _, _| {
                            let _ = webview.set_bounds(Rect {
                                position: Position::Logical(LogicalPosition::new(
                                    f64::from(bounds.origin.x.as_f32()),
                                    f64::from(bounds.origin.y.as_f32()),
                                )),
                                size: Size::Logical(LogicalSize::new(
                                    f64::from(bounds.size.width.as_f32()),
                                    f64::from(bounds.size.height.as_f32()),
                                )),
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .size_full(),
                )
            }
            Err(error) => {
                let theme = cx.theme();
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.background)
                    .text_color(theme.muted_foreground)
                    .text_sm()
                    .child(error.clone())
            }
        }
    }
}
