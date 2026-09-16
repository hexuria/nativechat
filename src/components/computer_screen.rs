//! The coworker's screen (noVNC) in its own window. macOS only: the page is a
//! `wry` WebView laid over the GPUI window; other platforms never build this.
//!
//! The window has a strip of its own above the screen — the WebView is a
//! native child view, so nothing GPUI draws can sit on top of it — with the
//! "Teach a task" control, and it answers the window keys itself: ⌘W and ⌘Q
//! close this window only, ⌘M minimizes it, ⌘H hides the app.
//!
//! A stopped tape is written to disk and offered on a sheet under the title bar: named, it
//! goes up as a recipe (`POST /recipes`), and the Recipes page lists it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use raw_window_handle::HasWindowHandle;
use wry::{
    Rect, WebViewBuilder,
    dpi::{LogicalPosition, LogicalSize, Position, Size},
};

use crate::actions::{CloseWindow, Hide, Minimize, Quit};
use crate::components::fields::field_input;
use crate::opengrok::thin_tape;
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

  // The page's own background — the letterbox around the 1280×800 screen — in the colour the
  // app asks for (see `paint_page`), so it follows the app's theme rather than noVNC's dark.
  window.__ncPaint = (bg) => {
    const css = document.getElementById('nc-theme')
      || Object.assign(document.createElement('style'), {id: 'nc-theme'});
    css.textContent = `html, body, #noVNC_container, #noVNC_screen, .noVNC_canvas, #noVNC_fallback_error { background: ${bg} !important; }`;
    if (!css.parentNode) (document.head || document.documentElement).appendChild(css);
    if (document.body) document.body.style.background = bg;
  };
})();
"#;

/// A task being taught: when it started and what the page has reported so far.
struct Teaching {
    started_at_ms: i64,
    events: Rc<RefCell<Vec<serde_json::Value>>>,
}

/// A stopped tape waiting on Save or Discard: the events, and where the local copy went.
struct PendingTape {
    started_at_ms: i64,
    events: Vec<serde_json::Value>,
    /// "kept at <path>", or why there is no local copy.
    backup: String,
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
    /// What became of the last tape, in the title bar: the recipe it was saved as, or that
    /// it was let go.
    last_saved: Option<String>,
    /// The app: its client uploads a tape, and its Recipes list is refreshed after.
    app: Entity<AppState>,
    /// A stopped tape waiting on Save or Discard, with the sheet under the title bar.
    pending: Option<PendingTape>,
    name_input: Entity<InputState>,
    description_input: Entity<InputState>,
    /// An upload is on its way.
    saving: bool,
    /// Why the last upload was refused; the sheet stays open with it.
    save_error: Option<String>,
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
        // Every render paints the page, but the page may not have loaded by the first one;
        // paint it once more when it has had time to.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |this, cx| this.paint_page(cx));
        })
        .detach();
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
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name this task"));
        let description_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("What it does (optional)"));
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
            app,
            pending: None,
            name_input,
            description_input,
            saving: false,
            save_error: None,
        }
    }

    /// Let go of every modifier the page holds in the box before this window stops receiving
    /// keys — the keyup for the ⌘ that closed or hid the window would otherwise never arrive.
    fn release_keys(&self) {
        if let Ok(webview) = &self.webview {
            let _ = webview.evaluate_script("window.__ncRelease && window.__ncRelease();");
        }
    }

    /// The page's own background — the letterbox around the 1280×800 screen — in the app's
    /// background colour, so it is light in the light theme and dark in the dark one rather
    /// than noVNC's dark whatever the theme. A no-op until the page has loaded.
    fn paint_page(&self, cx: &App) {
        if let Ok(webview) = &self.webview {
            let gpui::Rgba { r, g, b, .. } = cx.theme().background.into();
            let css = format!(
                "rgb({}, {}, {})",
                (r * 255.).round() as u8,
                (g * 255.).round() as u8,
                (b * 255.).round() as u8
            );
            let _ =
                webview.evaluate_script(&format!("window.__ncPaint && window.__ncPaint('{css}');"));
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

    /// Start a tape, or stop the running one: write the local copy and open the save sheet.
    fn toggle_teaching(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.teaching.take() {
            None => {
                // A tape still on the sheet is not thrown away by a new one.
                if self.pending.is_some() || self.saving {
                    return;
                }
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
                // The file is the backup; the upload is what the Recipes page shows.
                let backup = match save_tape(&self.coworker_id, session.started_at_ms, &events) {
                    Ok(path) => format!("kept at {path}"),
                    Err(error) => format!("no local copy: {error}"),
                };
                let name = format!(
                    "Task taught on {}",
                    chrono::Local::now().format("%b %-d, %Y")
                );
                self.name_input.update(cx, |input, cx| {
                    input.set_value(name, window, cx);
                });
                self.description_input.update(cx, |input, cx| {
                    input.set_value(String::new(), window, cx);
                });
                self.last_saved = None;
                self.save_error = None;
                self.pending = Some(PendingTape {
                    started_at_ms: session.started_at_ms,
                    events,
                    backup,
                });
            }
        }
        cx.notify();
    }

    /// Upload the tape on the sheet as a recipe. The sheet stays, with the reason, when the
    /// server refuses it; on success the title bar says what it became.
    fn save_recipe(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        let name = self.name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.save_error = Some("Give the task a name.".to_string());
            cx.notify();
            return;
        }
        let description = self.description_input.read(cx).value().trim().to_string();
        let Some(client) = self.app.read(cx).opengrok.clone() else {
            self.save_error = Some("Not connected to OpenGrok.".to_string());
            cx.notify();
            return;
        };
        let raw = upload_tape(&pending.events, pending.started_at_ms);
        self.saving = true;
        self.save_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.create_recipe(&name, &description, &raw).await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                match result {
                    Ok(detail) => {
                        // The server's filtered steps are what a bot will play.
                        let filtered = detail
                            .versions
                            .iter()
                            .find(|version| version.kind == "filtered")
                            .or_else(|| detail.runnable_version());
                        let (version, steps) = filtered
                            .map(|version| (version.version, version.body.steps.len()))
                            .unwrap_or((2, 0));
                        this.pending = None;
                        this.last_saved =
                            Some(format!("Saved as {name} · v{version} has {steps} steps"));
                        this.app.update(cx, |state, cx| state.refresh_recipes(cx));
                    }
                    Err(error) => this.save_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Let the tape go; the local copy stays where it is.
    fn discard_tape(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.pending.take() {
            self.last_saved = Some(format!("Not saved · {}", pending.backup));
        }
        self.save_error = None;
        cx.notify();
    }

    /// The strip under the title bar after Stop: a name, a description, Save and Discard, and
    /// what the tape holds or why the upload was refused.
    fn save_sheet(
        &self,
        pending: &PendingTape,
        theme: &gpui_kit::component::Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let saving = self.saving;
        let (note, note_color) = match &self.save_error {
            Some(error) => (error.clone(), theme.danger),
            None => (
                format!("{} events · {}", pending.events.len(), pending.backup),
                theme.muted_foreground,
            ),
        };
        h_flex()
            .id("teach-save-sheet")
            .w_full()
            .flex_shrink_0()
            .flex_wrap()
            .px(px(12.))
            .py(px(8.))
            .gap(px(8.))
            .items_center()
            .bg(theme.sidebar)
            .text_color(theme.foreground)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div().w(px(240.)).child(
                    field_input(&self.name_input)
                        .id("teach-save-name")
                        .disabled(saving),
                ),
            )
            .child(
                div().flex_1().min_w(px(180.)).child(
                    field_input(&self.description_input)
                        .id("teach-save-description")
                        .disabled(saving),
                ),
            )
            .child(
                Button::new("teach-save")
                    .small()
                    .primary()
                    .label(if saving { "Saving…" } else { "Save" })
                    .disabled(saving)
                    .on_click(cx.listener(|this, _, _, cx| this.save_recipe(cx))),
            )
            .child(
                Button::new("teach-discard")
                    .small()
                    .label("Discard")
                    .disabled(saving)
                    .on_click(cx.listener(|this, _, _, cx| this.discard_tape(cx))),
            )
            .child(div().text_xs().text_color(note_color).child(note))
            .into_any_element()
    }
}

/// The tape as it goes up: `at` counted from when teaching started rather than the page's
/// clock, and the moves thinned.
fn upload_tape(events: &[serde_json::Value], started_at_ms: i64) -> Vec<serde_json::Value> {
    let rebased: Vec<serde_json::Value> = events
        .iter()
        .cloned()
        .map(|mut event| {
            if let Some(object) = event.as_object_mut()
                && let Some(at) = object.get("at").and_then(serde_json::Value::as_i64)
            {
                object.insert(
                    "at".to_string(),
                    serde_json::json!((at - started_at_ms).max(0)),
                );
            }
            event
        })
        .collect();
    thin_tape(&rebased)
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
        self.paint_page(cx);
        let theme = cx.theme().clone();
        let teaching = self.teaching.is_some();
        // The sheet holds a tape until it is saved or let go; no new tape until then.
        let waiting = self.pending.is_some() || self.saving;
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
                    .disabled(waiting)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_teaching(window, cx)));
                if teaching { button.primary() } else { button }
            });
        let sheet = self
            .pending
            .as_ref()
            .map(|pending| self.save_sheet(pending, &theme, cx));
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
            .when_some(sheet, |this, sheet| this.child(sheet))
            .child(body)
    }
}
