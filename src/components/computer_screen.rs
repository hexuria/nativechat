//! The coworker's screen (noVNC) in its own window. macOS only: the page is a
//! `wry` WebView laid over the GPUI window; other platforms never build this.
//!
//! The window has a strip of its own above the screen — the WebView is a
//! native child view, so nothing GPUI draws can sit on top of it — with the
//! "Teach a task" control, and it answers the window keys itself: ⌘W and ⌘Q
//! close this window only, ⌘M minimizes it, ⌘H hides the app.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use raw_window_handle::HasWindowHandle;
use wry::{
    Rect, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize, Position, Size},
};

use crate::actions::{CloseWindow, Hide, Minimize, Quit};
use crate::state::AppState;

/// The title bar the window paints for itself: tall enough for the traffic lights and a
/// button, and the part the person drags the window by.
const TITLE_BAR_HEIGHT: f32 = 44.;
/// Past the traffic lights.
const TITLE_BAR_LEFT_PAD: f32 = 80.;

/// What the page reports while a task is being taught: every pointer and key
/// event on the noVNC canvas, in the screen's own 1280×800 coordinates. This is
/// the RAW tape (v1); filtering it into steps and editing them come later.
const TEACH_SCRIPT: &str = r#"
(() => {
  if (window.__ncTeachInstalled) return;
  window.__ncTeachInstalled = true;
  window.__ncTeach = false;
  const post = (payload) => {
    try { window.ipc.postMessage(JSON.stringify(payload)); } catch (_) {}
  };
  const canvas = () => document.querySelector('canvas');
  const scale = (e) => {
    const c = canvas();
    if (!c) return null;
    const r = c.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return null;
    return {
      x: Math.round((e.clientX - r.left) * 1280 / r.width),
      y: Math.round((e.clientY - r.top) * 800 / r.height),
    };
  };
  const on = (type, handler) => document.addEventListener(type, (e) => {
    if (!window.__ncTeach) return;
    const at = Date.now();
    handler(e, at);
  }, true);
  on('mousedown', (e, at) => { const p = scale(e); if (p) post({kind: 'down', button: e.button, at, ...p}); });
  on('mouseup',   (e, at) => { const p = scale(e); if (p) post({kind: 'up', button: e.button, at, ...p}); });
  on('mousemove', (e, at) => { const p = scale(e); if (p) post({kind: 'move', at, ...p}); });
  on('wheel',     (e, at) => { const p = scale(e); if (p) post({kind: 'wheel', dx: Math.round(e.deltaX), dy: Math.round(e.deltaY), at, ...p}); });
  on('keydown',   (e, at) => post({kind: 'keydown', key: e.key, code: e.code, at}));
  on('keyup',     (e, at) => post({kind: 'keyup', key: e.key, code: e.code, at}));

  // Modifiers the page has sent DOWN to the box must go UP before the page loses the keyboard:
  // a ⌘W or ⌘Q closes this window with Meta still held in the box's X server, and every
  // letter after that is a shortcut there (Alt+F opens Chromium's menu, Super+E the files).
  // noVNC sends a key-up for each key it holds when it sees the matching keyup event.
  const releaseModifiers = () => {
    const c = canvas();
    if (!c) return;
    for (const [key, code] of [
      ['Meta', 'MetaLeft'], ['Meta', 'MetaRight'],
      ['Alt', 'AltLeft'], ['Alt', 'AltRight'],
      ['Control', 'ControlLeft'], ['Control', 'ControlRight'],
      ['Shift', 'ShiftLeft'], ['Shift', 'ShiftRight'],
    ]) {
      c.dispatchEvent(new KeyboardEvent('keyup', { key, code, bubbles: true, cancelable: true }));
    }
  };
  window.__ncRelease = releaseModifiers;
  window.addEventListener('blur', releaseModifiers);
  document.addEventListener('visibilitychange', () => { if (document.hidden) releaseModifiers(); });
})();
"#;

/// A task being taught: when it started and what the page has reported so far.
struct Teaching {
    started_at_ms: i64,
    events: Rc<RefCell<Vec<serde_json::Value>>>,
}

pub struct ComputerScreen {
    /// The page, or why there is none. A WebView that would not build must not
    /// take the app down with it: the window says what went wrong instead.
    webview: Result<Rc<wry::WebView>, String>,
    coworker_id: String,
    /// "<name>'s Computer", painted in the title bar.
    title: String,
    focus: FocusHandle,
    /// The page's reports land here whether or not a task is being taught; the
    /// script only posts while `__ncTeach` is on.
    tape: Rc<RefCell<Vec<serde_json::Value>>>,
    teaching: Option<Teaching>,
    /// Where the last tape went, shown for a moment after Stop.
    last_saved: Option<String>,
}

impl ComputerScreen {
    pub fn new(
        url: &str,
        coworker_id: &str,
        title: &str,
        app: Entity<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The theme is app-wide; a toggle in the main window must repaint this one too.
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        let tape: Rc<RefCell<Vec<serde_json::Value>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = tape.clone();
        let webview = window
            .window_handle()
            .map_err(|error| format!("the window has no native handle: {error}"))
            .and_then(|handle| {
                WebViewBuilder::new()
                    .with_url(url)
                    .with_initialization_script(TEACH_SCRIPT)
                    .with_ipc_handler(move |request: wry::http::Request<String>| {
                        if let Ok(event) = serde_json::from_str::<serde_json::Value>(request.body())
                        {
                            sink.borrow_mut().push(event);
                        }
                    })
                    .build_as_child(&handle)
                    .map_err(|error| format!("the screen could not be loaded: {error}"))
            })
            .map(Rc::new);
        if let Err(error) = &webview {
            eprintln!("NativeChat computer: {error}");
        }
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            webview,
            coworker_id: coworker_id.to_string(),
            title: title.to_string(),
            focus,
            tape,
            teaching: None,
            last_saved: None,
        }
    }

    /// Let go of every modifier the page holds in the box before this window stops receiving
    /// keys — the keyup for the ⌘ that closed or hid the window would otherwise never arrive.
    fn release_keys(&self) {
        if let Ok(webview) = &self.webview {
            let _ = webview.evaluate_script("window.__ncRelease && window.__ncRelease();");
        }
    }

    fn set_teaching(&mut self, on: bool) {
        if let Ok(webview) = &self.webview {
            let _ = webview.evaluate_script(if on {
                "window.__ncTeach = true;"
            } else {
                "window.__ncTeach = false;"
            });
        }
    }

    /// Start a tape, or stop the running one and write it out.
    fn toggle_teaching(&mut self, cx: &mut Context<Self>) {
        match self.teaching.take() {
            None => {
                self.tape.borrow_mut().clear();
                self.last_saved = None;
                self.set_teaching(true);
                self.teaching = Some(Teaching {
                    started_at_ms: chrono::Utc::now().timestamp_millis(),
                    events: self.tape.clone(),
                });
            }
            Some(session) => {
                self.set_teaching(false);
                let events = session.events.borrow().clone();
                self.last_saved = Some(
                    match save_tape(&self.coworker_id, session.started_at_ms, &events) {
                        Ok(path) => format!("{} events saved to {path}", events.len()),
                        Err(error) => format!("could not save the tape: {error}"),
                    },
                );
            }
        }
        cx.notify();
    }
}

/// The raw tape (v1) as a JSON file under the app's data directory:
/// `teach/<coworker>/<started>.raw.json`.
fn save_tape(
    coworker_id: &str,
    started_at_ms: i64,
    events: &[serde_json::Value],
) -> Result<String, String> {
    let dir = crate::config::Config::data_dir()
        .join("teach")
        .join(coworker_id);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{started_at_ms}.raw.json"));
    let tape = serde_json::json!({
        "version": 1,
        "coworkerId": coworker_id,
        "startedAtMs": started_at_ms,
        "screen": { "width": 1280, "height": 800 },
        "events": events,
    });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&tape).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(path.display().to_string())
}

impl Render for ComputerScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let teaching = self.teaching.is_some();
        let count = self.tape.borrow().len();
        let header = h_flex()
            .id("computer-window-header")
            .w_full()
            .h(px(TITLE_BAR_HEIGHT))
            .flex_shrink_0()
            .pl(px(TITLE_BAR_LEFT_PAD))
            .pr(px(12.))
            .items_center()
            .gap(px(10.))
            .bg(theme.background)
            .text_color(theme.foreground)
            .border_b_1()
            .border_color(theme.border)
            // The name and the empty run of the bar: the part that drags the window.
            .child(
                div()
                    .id("computer-window-drag")
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone())
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
            )
            .when_some(self.last_saved.clone(), |this, note| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(note),
                )
            })
            .when(teaching, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("Recording · {count} events")),
                )
            })
            .child({
                let button = Button::new("teach-task")
                    .small()
                    .label(if teaching {
                        "Stop teaching"
                    } else {
                        "Teach a task"
                    })
                    .icon(
                        Icon::default()
                            .path("icons/record.svg")
                            .size(px(14.))
                            .text_color(if teaching {
                                gpui::red()
                            } else {
                                theme.foreground
                            }),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_teaching(cx)));
                if teaching { button.primary() } else { button }
            });
        let body = match &self.webview {
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
            Err(error) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.background)
                .text_color(theme.muted_foreground)
                .text_sm()
                .child(error.clone()),
        };
        v_flex()
            .id("computer-window")
            .track_focus(&self.focus)
            .key_context("ComputerScreen")
            .size_full()
            // This window's keys: close or quit close only this window; the app stays. Each
            // first lets go of the modifiers the page holds in the box (see `release_keys`).
            .on_action(cx.listener(|this, _: &Quit, window, _cx| {
                this.release_keys();
                window.remove_window();
            }))
            .on_action(cx.listener(|this, _: &CloseWindow, window, _cx| {
                this.release_keys();
                window.remove_window();
            }))
            .on_action(cx.listener(|this, _: &Minimize, window, _cx| {
                this.release_keys();
                window.minimize_window();
            }))
            .on_action(cx.listener(|this, _: &Hide, _window, cx| {
                this.release_keys();
                cx.hide();
            }))
            .child(header)
            .child(body)
    }
}
