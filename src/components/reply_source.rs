//! Where a Bot's replies are paid from: the server's paid keys, or the person's own subscription
//! through opencodex on their Mac. Three surfaces: Settings → Reply source, where the account's
//! setting is changed; the composer's chip, which picks a door for the next turns; and the badge
//! on each reply, which says which door it came through.
//!
//! Nothing here calls a model or keeps a key. The server keeps the setting, and when a turn goes
//! through the person's plan it is the server that talks to opencodex. The words and element ids
//! live here so the gpui-agent tree (`agent/host.rs`) says what the window says and names what
//! the window names.

use crate::components::fields::field_input;
use crate::opengrok::{DEFAULT_PROXY_URL, InferenceKind, InferenceSource, ReplySource};
use crate::state::{
    AppState, REPLY_SOURCE_NOT_ON_SERVER, ReplySourceRead, ReplySourceSettings, TurnSourceChip,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Sizable as _, Theme, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// Settings → Reply source, in the settings' own list of pages.
pub(crate) const SETTINGS_TAB: &str = "settings-tab-reply-source";
/// The section: the whole form, once the setting has been read.
pub(crate) const SECTION: &str = "settings-reply-source";
/// The radio: Server (paid keys) or My subscription.
pub(crate) const KIND: &str = "settings-reply-source-kind";
pub(crate) const URL: &str = "settings-reply-source-url";
/// The model picker, with one choice per model of the person's plan.
pub(crate) const MODEL: &str = "settings-reply-source-model";
pub(crate) const KEY: &str = "settings-reply-source-key";
/// Whether opencodex answered the server.
pub(crate) const HEALTH: &str = "settings-reply-source-health";
/// Why Claude and Gemini are not offered.
pub(crate) const PROVIDERS: &str = "settings-reply-source-providers";
pub(crate) const SAVE: &str = "settings-reply-source-save";
/// Under Save: a refusal in the server's words, a Save nobody knows the fate of, a read that
/// failed.
pub(crate) const ERROR: &str = "settings-reply-source-error";
/// In place of the form: a server without reply sources, or a setting that could not be read.
pub(crate) const UNAVAILABLE: &str = "settings-reply-source-unavailable";
/// The composer's chip.
pub(crate) const COMPOSER_CHIP: &str = "composer-reply-source";

/// One of the radio's two choices.
pub(crate) fn kind_id(kind: InferenceKind) -> String {
    format!("{KIND}-{}", kind.word())
}

/// One model in the picker, by the id the server lists it under.
pub(crate) fn model_id(model: &str) -> String {
    format!("{MODEL}-{model}")
}

/// A reply's badge, by the reply's message id.
pub(crate) fn badge_id(message_id: &str) -> String {
    format!("reply-source-{message_id}")
}

pub(crate) const ASKING: &str = "Asking the server…";
pub(crate) const INTRO: &str = "Where your Bots' replies are paid from. Either way the server \
     runs the turn, its tools and its record; this app never calls a model or keeps a key.";
pub(crate) const RUNNING: &str = "opencodex is running";
/// The health line while opencodex is not answering, up to the command that starts it, which the
/// page sets as code.
const NOT_RUNNING_LEAD: &str = "Not running — start it with";
pub(crate) const START_COMMAND: &str = "ocx start";
/// Why the picker offers only OpenAI's and xAI's models.
pub(crate) const PROVIDERS_NOTE: &str = "Only Codex/OpenAI and Grok/xAI models are offered: \
     Claude's and Gemini's terms forbid routing a consumer subscription through a third-party \
     app.";
pub(crate) const PICK_MODEL: &str = "Pick a model";
pub(crate) const NO_MODELS: &str = "The server lists no models from your plan yet.";
pub(crate) const KEY_PLACEHOLDER: &str = "Proxy key, if opencodex asks for one";
pub(crate) const KEY_SET: &str = "The server holds a key. Type one to replace it.";

/// A choice of the radio, as it reads.
pub(crate) fn kind_label(kind: InferenceKind) -> &'static str {
    match kind {
        InferenceKind::Gateway => "Server (paid keys)",
        InferenceKind::LocalProxy => "My subscription",
    }
}

fn kind_detail(kind: InferenceKind) -> &'static str {
    match kind {
        InferenceKind::Gateway => "The server's own keys, through its gateway.",
        InferenceKind::LocalProxy => {
            "Your ChatGPT or Grok plan, through opencodex on your Mac. The server talks to it."
        }
    }
}

/// What a reply's badge reads.
pub(crate) fn badge_label(kind: InferenceKind) -> &'static str {
    match kind {
        InferenceKind::Gateway => "paid key",
        InferenceKind::LocalProxy => "your plan",
    }
}

/// What the composer's chip reads.
pub(crate) fn chip_label(kind: InferenceKind) -> &'static str {
    match kind {
        InferenceKind::Gateway => "Server",
        InferenceKind::LocalProxy => "My plan",
    }
}

/// What the chip says on hover: where the next turn goes, and that it is the person's pick for
/// their turns rather than the account's own door, which it stays until it is clicked back.
pub(crate) fn chip_tooltip(chip: &TurnSourceChip) -> String {
    let door = match (chip.kind, chip.local_model.as_deref()) {
        (InferenceKind::Gateway, _) => "Next replies: the server's paid keys".to_string(),
        (InferenceKind::LocalProxy, Some(model)) => format!("Next replies: your plan, {model}"),
        (InferenceKind::LocalProxy, None) => "Next replies: your plan".to_string(),
    };
    if chip.picked {
        format!("{door}, until you switch back. Click to switch.")
    } else {
        format!("{door}, as in Settings. Click to switch.")
    }
}

/// The health line, the server's word on whether opencodex answered it: its words, and when it
/// did not, the command that starts it, which the page sets as code after them.
pub(crate) fn health_line(kept: &InferenceSource) -> (&'static str, Option<&'static str>) {
    if kept.healthy {
        (RUNNING, None)
    } else {
        (NOT_RUNNING_LEAD, Some(START_COMMAND))
    }
}

/// What Save reads while it is with the server, and otherwise.
pub(crate) fn save_label(settings: &ReplySourceSettings) -> &'static str {
    if settings.saving.is_some() {
        "Saving…"
    } else {
        "Save"
    }
}

/// The line in place of the form, while there is no form to show: `None` once it has been read.
pub(crate) fn unavailable_line(settings: &ReplySourceSettings) -> Option<&str> {
    match &settings.kept {
        None | Some(ReplySourceRead::Loading) => Some(ASKING),
        Some(ReplySourceRead::NotOnServer) => Some(REPLY_SOURCE_NOT_ON_SERVER),
        Some(ReplySourceRead::Unavailable(why)) => Some(why),
        Some(ReplySourceRead::Read(_)) => None,
    }
}

/// Settings → Reply source. Its fields need a window, so it is made on the first render of the
/// tab, as Settings → Logins is.
pub struct ReplySourcePage {
    state: Entity<AppState>,
    url: Entity<InputState>,
    key: Entity<InputState>,
}

impl ReplySourcePage {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_PROXY_URL));
        let key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(KEY_PLACEHOLDER)
                .masked(true)
        });
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        // What is typed is the form: the state keeps the copy Save sends and a driver writes.
        cx.subscribe(&url, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let url = input.read(cx).value().to_string();
                this.state
                    .update(cx, |state, cx| state.set_reply_source_url(url, cx));
            }
        })
        .detach();
        cx.subscribe(&key, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let typed = input.read(cx).value().to_string();
                this.state
                    .update(cx, |state, cx| state.set_reply_source_key(&typed, cx));
            }
        })
        .detach();
        Self { state, url, key }
    }

    /// The fields follow the state: a URL a driver wrote, the server's own after a Save, and an
    /// empty key field once the key has gone to the server.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (url, key_sent) = {
            let settings = &self.state.read(cx).reply_source;
            (settings.shown_url(), settings.key_draft.is_none())
        };
        if self.url.read(cx).value().as_ref() != url.as_str() {
            self.url
                .update(cx, |input, cx| input.set_value(url, window, cx));
        }
        if key_sent && !self.key.read(cx).value().trim().is_empty() {
            self.key
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
    }
}

fn card() -> Div {
    div()
        .w_full()
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(0x777777).opacity(0.24))
        .overflow_hidden()
}

fn radio_dot(on: bool) -> impl IntoElement {
    div()
        .size(px(16.))
        .rounded_full()
        .border_1()
        .border_color(rgb(0x888888))
        .flex()
        .items_center()
        .justify_center()
        .when(on, |this| {
            this.child(div().size(px(8.)).rounded_full().bg(rgb(0x1084FE)))
        })
}

fn kind_row(
    kind: InferenceKind,
    picked: bool,
    live: bool,
    muted: Hsla,
    app: Entity<AppState>,
) -> impl IntoElement {
    h_flex()
        .id(SharedString::from(kind_id(kind)))
        .w_full()
        .px(px(16.))
        .py(px(14.))
        .gap(px(12.))
        .when(live, |this| {
            this.cursor_pointer()
                .hover(|s| s.bg(rgb(0x777777).opacity(0.08)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    app.update(cx, |state, cx| state.pick_reply_source_kind(kind, cx));
                })
        })
        .when(!live, |this| this.opacity(0.6))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(div().text_sm().child(kind_label(kind)))
                .child(div().text_xs().text_color(muted).child(kind_detail(kind))),
        )
        .child(radio_dot(picked))
}

fn labelled(label: &'static str, muted: Hsla, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap(px(4.))
        .child(div().text_xs().text_color(muted).child(label))
        .child(control)
}

fn model_picker(
    shown: Option<String>,
    models: Vec<String>,
    live: bool,
    app: Entity<AppState>,
) -> impl IntoElement {
    let label = shown.clone().unwrap_or_else(|| PICK_MODEL.to_string());
    Button::new(MODEL)
        .label(label)
        .ghost()
        .compact()
        .icon(IconName::ChevronDown)
        .disabled(!live || models.is_empty())
        .dropdown_menu(move |menu, _, _| {
            models.iter().fold(menu, |menu, model| {
                let app = app.clone();
                let picked = model.clone();
                // Its row carries the id a driver clicks, `settings-reply-source-model-{id}`.
                let id = SharedString::from(model_id(model));
                let label = model.clone();
                menu.item(
                    PopupMenuItem::element(move |_, _| div().id(id.clone()).child(label.clone()))
                        .checked(shown.as_deref() == Some(model.as_str()))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            app.update(cx, |state, cx| {
                                state.pick_reply_source_model(picked.clone(), cx);
                            });
                        }),
                )
            })
        })
}

impl Render for ReplySourcePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let app = self.state.clone();
        let (settings, models) = {
            let state = self.state.read(cx);
            (state.reply_source.clone(), state.subscription_models())
        };
        let intro = div().text_xs().text_color(muted).child(INTRO);
        let Some(kept) = settings.kept_source().cloned() else {
            let line = unavailable_line(&settings).unwrap_or(ASKING).to_string();
            let failed = matches!(settings.kept, Some(ReplySourceRead::Unavailable(_)));
            return v_flex().id(SECTION).gap(px(12.)).child(intro).child(
                div()
                    .id(UNAVAILABLE)
                    .text_sm()
                    .text_color(if failed { theme.danger } else { muted })
                    .child(line),
            );
        };
        let live = settings.can_edit();
        let shown = settings.shown_kind().unwrap_or(kept.kind);
        let healthy = kept.healthy;
        v_flex()
            .id(SECTION)
            .gap(px(12.))
            .child(intro)
            .child(
                card()
                    .flex()
                    .flex_col()
                    .id(KIND)
                    .child(kind_row(
                        InferenceKind::Gateway,
                        shown == InferenceKind::Gateway,
                        live,
                        muted,
                        app.clone(),
                    ))
                    .child(div().h(px(1.)).bg(rgb(0x777777).opacity(0.16)))
                    .child(kind_row(
                        InferenceKind::LocalProxy,
                        shown == InferenceKind::LocalProxy,
                        live,
                        muted,
                        app.clone(),
                    )),
            )
            .child(div().text_xs().text_color(muted).child("Your subscription"))
            .child(
                card()
                    .flex()
                    .flex_col()
                    .px(px(16.))
                    .py(px(14.))
                    .gap(px(12.))
                    .child(labelled(
                        "Proxy URL",
                        muted,
                        div().id(URL).child(field_input(&self.url).disabled(!live)),
                    ))
                    .child(labelled(
                        "Model",
                        muted,
                        v_flex()
                            .items_start()
                            .gap(px(4.))
                            .child(model_picker(
                                settings.shown_model(),
                                models.clone(),
                                live,
                                app.clone(),
                            ))
                            .when(models.is_empty(), |this| {
                                this.child(div().text_xs().text_color(muted).child(NO_MODELS))
                            }),
                    ))
                    .child(labelled(
                        "Proxy key",
                        muted,
                        v_flex()
                            .gap(px(4.))
                            .child(div().id(KEY).child(field_input(&self.key).disabled(!live)))
                            .when(kept.has_api_key, |this| {
                                this.child(div().text_xs().text_color(muted).child(KEY_SET))
                            }),
                    ))
                    .child(
                        h_flex()
                            .id(HEALTH)
                            .gap(px(8.))
                            .items_center()
                            .child(div().size(px(8.)).rounded_full().bg(if healthy {
                                theme.green
                            } else {
                                muted
                            }))
                            .child({
                                let (words, command) = health_line(&kept);
                                h_flex().gap(px(4.)).text_sm().child(words).when_some(
                                    command,
                                    |this, command| {
                                        this.child(div().font_family("monospace").child(command))
                                    },
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(PROVIDERS)
                            .text_xs()
                            .text_color(muted)
                            .child(PROVIDERS_NOTE),
                    ),
            )
            .when_some(settings.note.clone(), |this, note| {
                this.child(
                    div()
                        .id(ERROR)
                        .text_sm()
                        .text_color(theme.danger)
                        .child(note.line().to_string()),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap(px(10.))
                    .items_center()
                    .when(settings.is_unsaved() && settings.saving.is_none(), |this| {
                        this.child(div().text_xs().text_color(muted).child("Not saved yet"))
                    })
                    .child(
                        Button::new(SAVE)
                            .label(save_label(&settings))
                            .small()
                            .disabled(!settings.can_save())
                            .on_click(move |_, _, cx| {
                                app.update(cx, |state, cx| state.save_reply_source(cx));
                            }),
                    ),
            )
    }
}

/// The composer's chip: which door the next turns go through, "Server" or "My plan". A click
/// switches it; it stays as picked until it is clicked back, and is gone with a relaunch.
pub(crate) fn composer_chip(
    chip: &TurnSourceChip,
    app: Entity<AppState>,
    theme: &Theme,
) -> impl IntoElement {
    let tooltip = chip_tooltip(chip);
    let secondary = theme.secondary;
    div()
        .id(COMPOSER_CHIP)
        .flex_none()
        .h(px(26.))
        .px(px(10.))
        .rounded_full()
        .flex()
        .items_center()
        .border_1()
        .border_color(theme.border)
        .when(chip.picked, |this| this.bg(secondary))
        .text_xs()
        .text_color(theme.secondary_foreground)
        .cursor_pointer()
        .hover(move |style| style.bg(secondary))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            app.update(cx, |state, cx| state.toggle_turn_source(cx));
        })
        .child(chip_label(chip.kind))
}

/// A reply's badge: "paid key" or "your plan", and the model on hover when the server named one.
/// One to a reply, on the last row of its words, so it is named by the reply's message id.
pub(crate) fn reply_badge(
    source: &ReplySource,
    message_id: &str,
    muted: Hsla,
    border: Hsla,
) -> impl IntoElement {
    let model = source.model.clone();
    div()
        .id(ElementId::Name(badge_id(message_id).into()))
        .flex_none()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .text_xs()
        .text_color(muted)
        .when_some(model, |this, model| {
            this.tooltip(move |window, cx| Tooltip::new(model.clone()).build(window, cx))
        })
        .child(badge_label(source.kind))
}

#[cfg(test)]
mod tests {
    // Item by item rather than a glob: `use super::*` would drag in gpui_kit's own `test`.
    use super::{badge_label, chip_label, chip_tooltip, health_line, kind_id, kind_label};
    use crate::opengrok::{DEFAULT_PROXY_URL, InferenceKind, InferenceSource};
    use crate::state::TurnSourceChip;

    /// The words the three surfaces say, as the contract's two doors.
    #[test]
    fn each_door_reads_as_itself_on_every_surface() {
        assert_eq!(
            InferenceKind::ALL.map(badge_label),
            ["paid key", "your plan"]
        );
        assert_eq!(InferenceKind::ALL.map(chip_label), ["Server", "My plan"]);
        assert_eq!(
            InferenceKind::ALL.map(kind_label),
            ["Server (paid keys)", "My subscription"]
        );
        assert_eq!(
            InferenceKind::ALL.map(kind_id),
            [
                "settings-reply-source-kind-gateway",
                "settings-reply-source-kind-local_proxy"
            ]
        );
    }

    /// The chip says where the next replies go, and whether that is the person's pick or the
    /// account's own door.
    #[test]
    fn the_chip_says_where_the_next_replies_go() {
        let plan = TurnSourceChip {
            kind: InferenceKind::LocalProxy,
            picked: true,
            local_model: Some("gpt-5-codex".into()),
        };
        assert_eq!(
            chip_tooltip(&plan),
            "Next replies: your plan, gpt-5-codex, until you switch back. Click to switch."
        );
        let server = TurnSourceChip {
            kind: InferenceKind::Gateway,
            picked: false,
            local_model: Some("gpt-5-codex".into()),
        };
        assert_eq!(
            chip_tooltip(&server),
            "Next replies: the server's paid keys, as in Settings. Click to switch."
        );
    }

    /// The health line is the server's word on opencodex, both ways.
    #[test]
    fn the_health_line_is_the_servers_word() {
        let mut kept = InferenceSource {
            kind: InferenceKind::LocalProxy,
            base_url: Some(DEFAULT_PROXY_URL.into()),
            local_model: None,
            healthy: true,
            has_api_key: false,
        };
        assert_eq!(health_line(&kept), ("opencodex is running", None));
        kept.healthy = false;
        assert_eq!(
            health_line(&kept),
            ("Not running — start it with", Some("ocx start"))
        );
    }
}
