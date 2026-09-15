use std::rc::Rc;

use gpui_kit::*;
use raw_window_handle::HasWindowHandle;
use wry::{
    Rect, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize, Position, Size},
};

pub struct ComputerScreen {
    webview: Rc<wry::WebView>,
}

impl ComputerScreen {
    pub fn new(url: &str, window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let handle = window
            .window_handle()
            .expect("computer window has a native handle");
        let webview = WebViewBuilder::new()
            .with_url(url)
            .build_as_child(&handle)
            .expect("computer screen webview");
        Self {
            webview: Rc::new(webview),
        }
    }
}

impl Render for ComputerScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let webview = self.webview.clone();
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
}
