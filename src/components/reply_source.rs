//! Where a Bot's replies are paid from: the server's paid keys, or the person's own subscription
//! through opencodex, running on the same machine as the server. Three surfaces: Settings → Reply
//! source, where the account's setting is changed; the composer's chip, which picks a door for the
//! next turns; and the badge on each reply, which says which door it came through.
//!
//! Nothing here calls a model or keeps a key. The server keeps the setting, and when a turn goes
//! through the person's plan it is the server that talks to opencodex. The words and element ids
//! live here so the gpui-agent tree (`agent/host.rs`) says what the window says and names what
//! the window names.

use crate::components::fields::field_input;
use crate::opengrok::{DEFAULT_PROXY_URL, InferenceKind, ReplySource};
use crate::state::{
    AppState, ProxyHealth, REPLY_SOURCE_NOT_ON_SERVER, REPLY_SOURCE_RETYPE_KEY, ReplySourceNote,
    ReplySourceRead, ReplySourceSettings, TurnSourceChip,
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
/// The picker's first choice while a model is shown: none, which takes the kept one away.
pub(crate) const NO_MODEL: &str = "settings-reply-source-no-model";
/// Under the picker: why it offers what it offers, or nothing.
pub(crate) const MODELS_NOTE: &str = "settings-reply-source-models-note";
pub(crate) const KEY: &str = "settings-reply-source-key";
/// Beside the key field while the server holds a key: Remove key, and Keep key to take it back.
pub(crate) const REMOVE_KEY: &str = "settings-reply-source-remove-key";
/// Whether opencodex answered the server.
pub(crate) const HEALTH: &str = "settings-reply-source-health";
/// Why Claude and Gemini are not offered.
pub(crate) const PROVIDERS: &str = "settings-reply-source-providers";
/// Where the app's server is not on this Mac: why the plan's half of the page takes no change.
pub(crate) const ELSEWHERE: &str = "settings-reply-source-elsewhere";
pub(crate) const SAVE: &str = "settings-reply-source-save";
/// Beside Save: what it will not send as the page stands, or what it keeps.
pub(crate) const HINT: &str = "settings-reply-source-hint";
/// Under Save: a refusal in the server's words, a Save nobody knows the fate of, a read that
/// failed, and a key to type again.
pub(crate) const ERROR: &str = "settings-reply-source-error";
/// In place of the form: asking the server, a server without reply sources, or a setting that
/// could not be read.
pub(crate) const UNAVAILABLE: &str = "settings-reply-source-unavailable";
/// The composer's chip.
pub(crate) const COMPOSER_CHIP: &str = "composer-reply-source";
/// Under a Bot's Model field, while replies go through the person's own plan.
pub(crate) const BOT_MODEL_PLAN: &str = "agent-model-plan";
/// In a Bot's Usage card, while replies go through the person's own plan.
pub(crate) const BOT_USAGE_PLAN: &str = "agent-usage-plan";

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

/// The model a reply's badge names on hover, as a driver's tree holds it: under the badge, by the
/// reply's message id. The window draws it in a tooltip, which has no id of its own.
#[cfg(feature = "agent")]
pub(crate) fn badge_model_id(message_id: &str) -> String {
    format!("reply-source-model-{message_id}")
}

pub(crate) const ASKING: &str = "Asking the server…";
pub(crate) const INTRO: &str = "Where your Bots' replies are paid from. Either way the server \
     runs the turn, its tools and its record; this app never calls a model or keeps a key.";
pub(crate) const RUNNING: &str = "opencodex is running";
/// The health line while opencodex is not answering, up to the command that starts it, which the
/// page sets as code.
const NOT_RUNNING_LEAD: &str = "Not running — start it with";
pub(crate) const START_COMMAND: &str = "ocx start";
/// The health line while opencodex is not answering a server on another machine, where a
/// command typed on this Mac would start nothing the server can reach.
pub(crate) const NOT_RUNNING_THERE: &str = "Not running on the server's machine";
/// The health line while the server keeps no proxy address: nothing to ask, which is not the
/// same as opencodex being down.
pub(crate) const NO_ADDRESS: &str = "No address saved";
/// Why the picker offers only OpenAI's and xAI's models.
pub(crate) const PROVIDERS_NOTE: &str = "Only Codex/OpenAI and Grok/xAI models are offered: \
     Claude's and Gemini's terms forbid routing a consumer subscription through a third-party \
     app.";
/// Where the app's server is not on this Mac. The server calls opencodex on its own machine, so
/// the person's plan is set up from the Mac the server runs on.
pub(crate) const ELSEWHERE_LINE: &str =
    "Your own subscription works only when this app's server runs on this Mac.";
pub(crate) const PICK_MODEL: &str = "Pick a model";
pub(crate) const NO_MODEL_LABEL: &str = "No model";
pub(crate) const NO_MODELS: &str = "The server lists no models from your plan yet.";
pub(crate) const NO_MODELS_WITHOUT_ADDRESS: &str =
    "The server lists your plan's models once it has the proxy's address.";
pub(crate) const NO_MODELS_NOT_RUNNING: &str =
    "opencodex isn't answering, so the server can't list your plan's models.";
pub(crate) const MODELS_MAY_BE_OLD: &str =
    "opencodex isn't answering; these are the models it listed last.";
pub(crate) const KEY_PLACEHOLDER: &str = "Proxy key, if opencodex asks for one";
pub(crate) const KEY_SET: &str = "The server holds a key. Type one to replace it.";
pub(crate) const KEY_GOES: &str = "The server's key goes when you save.";
pub(crate) const REMOVE_KEY_LABEL: &str = "Remove key";
pub(crate) const KEEP_KEY_LABEL: &str = "Keep key";
/// Under a Bot's Model field while the account or the composer's chip is on the person's plan:
/// the server asks the plan's model then, never the Bot's gateway pin.
pub(crate) const PLAN_MODEL_NOTE: &str =
    "On your own subscription, replies use the model chosen in Settings → Reply source.";
/// In a Bot's Usage card at the same times: a turn on the person's own plan is not metered and
/// carries no gateway key, so the server's usage report never counts it.
pub(crate) const PLAN_USAGE_NOTE: &str = "Replies on your own subscription aren't counted here.";

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
            "Your ChatGPT or Grok plan, through opencodex running on the same machine as the \
             server, which talks to it."
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

/// The health line ([`AppState::proxy_health`]): its words, and where opencodex is down beside
/// this Mac, the command that starts it, which the page sets as code after them. `here` is the
/// server running on this Mac, where that command would reach it.
pub(crate) fn health_line(health: ProxyHealth, here: bool) -> (&'static str, Option<&'static str>) {
    match health {
        ProxyHealth::NoAddress => (NO_ADDRESS, None),
        ProxyHealth::Running => (RUNNING, None),
        ProxyHealth::NotRunning if here => (NOT_RUNNING_LEAD, Some(START_COMMAND)),
        ProxyHealth::NotRunning => (NOT_RUNNING_THERE, None),
    }
}

/// The health line's word for a driver: `running`, `not-running` or `no-address`.
#[cfg(any(feature = "agent", test))]
pub(crate) fn health_word(health: ProxyHealth) -> &'static str {
    match health {
        ProxyHealth::NoAddress => "no-address",
        ProxyHealth::Running => "running",
        ProxyHealth::NotRunning => "not-running",
    }
}

/// The line under the picker, from whether the server lists any of the plan's models and what it
/// said of opencodex: why there are none, or that the ones there may be old.
pub(crate) fn models_note(health: ProxyHealth, has_models: bool) -> Option<&'static str> {
    match (health, has_models) {
        (ProxyHealth::NoAddress, false) => Some(NO_MODELS_WITHOUT_ADDRESS),
        (ProxyHealth::NotRunning, false) => Some(NO_MODELS_NOT_RUNNING),
        (ProxyHealth::NotRunning, true) => Some(MODELS_MAY_BE_OLD),
        (ProxyHealth::Running, false) => Some(NO_MODELS),
        (_, true) => None,
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

/// The line under Save: the note about the last thing that did not go as asked, and a key to
/// type again after it, or alone once the note has gone.
pub(crate) fn error_line(settings: &ReplySourceSettings) -> Option<String> {
    let note = settings.note.as_ref().map(ReplySourceNote::line);
    let retype = settings.retype_key.then_some(REPLY_SOURCE_RETYPE_KEY);
    match (note, retype) {
        (Some(note), Some(retype)) => Some(format!("{note} {retype}")),
        (Some(line), None) | (None, Some(line)) => Some(line.to_string()),
        (None, None) => None,
    }
}

/// The line under Save is trouble, drawn in the danger colour: a refusal or a failed read. A Save
/// whose fate is being checked, and a key to type again, are not, and are drawn muted.
pub(crate) fn error_is_trouble(settings: &ReplySourceSettings) -> bool {
    matches!(
        settings.note,
        Some(ReplySourceNote::Refused(_) | ReplySourceNote::ReadFailed(_))
    )
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
        // The fields follow the state whether the page is drawn or not: a key the state drops
        // when the page is left, or at a sign-out, leaves the field too, then and not at the
        // next time the page happens to be drawn.
        cx.observe_in(&state, window, |this, _, window, cx| {
            this.sync_inputs(window, cx);
            cx.notify();
        })
        .detach();
        // What is typed is the form: the state keeps the copy Save sends and a driver writes. A
        // field takes typing only while the page draws it editable, as the radio and the picker
        // take a click only then. Keys that reach one the page has just made read-only, before
        // it is drawn so, change no draft, and the field is put back to what the page holds
        // rather than left showing what nothing took.
        cx.subscribe_in(
            &url,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                if this.state.read(cx).reply_source.plan_editable() {
                    let url = input.read(cx).value().to_string();
                    this.state
                        .update(cx, |state, cx| state.set_reply_source_url(url, cx));
                }
                this.sync_inputs(window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &key,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                if this.state.read(cx).reply_source.key_editable() {
                    let typed = input.read(cx).value().to_string();
                    this.state
                        .update(cx, |state, cx| state.set_reply_source_key(&typed, cx));
                }
                this.sync_inputs(window, cx);
            },
        )
        .detach();
        let mut page = Self { state, url, key };
        page.sync_inputs(window, cx);
        page
    }

    /// The fields follow the state: a URL a driver wrote and the server's own after a Save; and
    /// the key field holds the key waiting for Save, drawn masked, one a driver wrote included,
    /// and nothing once it has gone to the server, been dropped, or never was.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (url, key) = {
            let settings = &self.state.read(cx).reply_source;
            let field = self.key.read(cx).value();
            let key = match settings.key_draft.as_ref() {
                None => (!field.trim().is_empty()).then(String::new),
                Some(key) => (field.trim() != key.as_str()).then(|| key.as_str().to_string()),
            };
            (settings.shown_url(), key)
        };
        if self.url.read(cx).value().as_ref() != url.as_str() {
            self.url
                .update(cx, |input, cx| input.set_value(url, window, cx));
        }
        if let Some(key) = key {
            self.key
                .update(cx, |input, cx| input.set_value(key, window, cx));
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
    // While a model is shown, the first choice takes it away.
    let clearable = shown.is_some();
    Button::new(MODEL)
        .label(label)
        .ghost()
        .compact()
        .icon(IconName::ChevronDown)
        .disabled(!live || (models.is_empty() && !clearable))
        .dropdown_menu(move |menu, _, _| {
            let menu = if clearable {
                let app = app.clone();
                menu.item(
                    PopupMenuItem::element(|_, _| div().id(NO_MODEL).child(NO_MODEL_LABEL))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            app.update(cx, |state, cx| state.clear_reply_source_model(cx));
                        }),
                )
            } else {
                menu
            };
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
        // Read in place: the settings can hold a typed key, and nothing here copies it.
        let state = self.state.read(cx);
        let settings = &state.reply_source;
        let intro = div().text_xs().text_color(muted).child(INTRO);
        let Some(kept) = settings.kept_source() else {
            let line = unavailable_line(settings).unwrap_or(ASKING).to_string();
            let failed = matches!(settings.kept, Some(ReplySourceRead::Unavailable(_)));
            return v_flex().id(SECTION).gap(px(12.)).child(intro).child(
                div()
                    .id(UNAVAILABLE)
                    .text_sm()
                    .text_color(if failed { theme.danger } else { muted })
                    .child(line),
            );
        };
        let models = state.subscription_models();
        let health = state.proxy_health().unwrap_or(ProxyHealth::NoAddress);
        let hint = state.reply_source_hint();
        let can_save = state.reply_source_can_save();
        let live = settings.can_edit();
        let here = settings.server_on_this_mac;
        let plan_live = settings.plan_editable();
        let shown = settings.shown_kind().unwrap_or(kept.kind);
        let shown_model = settings.shown_model().map(str::to_string);
        let models_line = models_note(health, !models.is_empty());
        let has_key = kept.has_api_key;
        let removing = settings.remove_key;
        let key_live = settings.key_editable();
        let unsaved = settings.is_unsaved() && settings.saving.is_none();
        let error = error_line(settings);
        let trouble = error_is_trouble(settings);
        let save_words = save_label(settings);
        let (health_words, command) = health_line(health, here);
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
                        plan_live,
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
                    .when(!here, |this| {
                        this.child(
                            div()
                                .id(ELSEWHERE)
                                .text_sm()
                                .text_color(muted)
                                .child(ELSEWHERE_LINE),
                        )
                    })
                    .child(labelled(
                        "Proxy URL",
                        muted,
                        div()
                            .id(URL)
                            .child(field_input(&self.url).disabled(!plan_live)),
                    ))
                    .child(labelled(
                        "Model",
                        muted,
                        v_flex()
                            .items_start()
                            .gap(px(4.))
                            .child(model_picker(shown_model, models, plan_live, app.clone()))
                            .when_some(models_line, |this, line| {
                                this.child(
                                    div()
                                        .id(MODELS_NOTE)
                                        .text_xs()
                                        .text_color(muted)
                                        .child(line),
                                )
                            }),
                    ))
                    .child(labelled(
                        "Proxy key",
                        muted,
                        v_flex()
                            .gap(px(4.))
                            .child(
                                div()
                                    .id(KEY)
                                    .child(field_input(&self.key).disabled(!key_live)),
                            )
                            .when(has_key, |this| {
                                let app = app.clone();
                                this.child(
                                    h_flex()
                                        .gap(px(8.))
                                        .items_center()
                                        .child(
                                            div().text_xs().text_color(muted).child(if removing {
                                                KEY_GOES
                                            } else {
                                                KEY_SET
                                            }),
                                        )
                                        .child(
                                            Button::new(REMOVE_KEY)
                                                .label(if removing {
                                                    KEEP_KEY_LABEL
                                                } else {
                                                    REMOVE_KEY_LABEL
                                                })
                                                .ghost()
                                                .small()
                                                .disabled(!plan_live)
                                                .on_click(move |_, _, cx| {
                                                    app.update(cx, |state, cx| {
                                                        state.toggle_remove_reply_source_key(cx)
                                                    });
                                                }),
                                        ),
                                )
                            }),
                    ))
                    .child(
                        h_flex()
                            .id(HEALTH)
                            .gap(px(8.))
                            .items_center()
                            .child(div().size(px(8.)).rounded_full().bg(
                                if health == ProxyHealth::Running {
                                    theme.green
                                } else {
                                    muted
                                },
                            ))
                            .child(
                                h_flex()
                                    .gap(px(4.))
                                    .text_sm()
                                    .child(health_words)
                                    .when_some(command, |this, command| {
                                        this.child(div().font_family("monospace").child(command))
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id(PROVIDERS)
                            .text_xs()
                            .text_color(muted)
                            .child(PROVIDERS_NOTE),
                    ),
            )
            .when_some(error, |this, line| {
                this.child(
                    div()
                        .id(ERROR)
                        .text_sm()
                        .text_color(if trouble { theme.danger } else { muted })
                        .child(line),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap(px(10.))
                    .items_center()
                    .when_some(hint, |this, hint| {
                        this.child(
                            div()
                                .id(HINT)
                                .flex_1()
                                .min_w(px(0.))
                                .text_xs()
                                .text_color(muted)
                                .child(hint),
                        )
                    })
                    .when(unsaved, |this| {
                        this.child(div().text_xs().text_color(muted).child("Not saved yet"))
                    })
                    .child(
                        Button::new(SAVE)
                            .label(save_words)
                            .small()
                            .disabled(!can_save)
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
    use super::{
        MODELS_MAY_BE_OLD, NO_MODELS, NO_MODELS_NOT_RUNNING, NO_MODELS_WITHOUT_ADDRESS,
        badge_label, chip_label, chip_tooltip, health_line, health_word, kind_id, kind_label,
        models_note,
    };
    use crate::opengrok::InferenceKind;
    use crate::state::{ProxyHealth, TurnSourceChip};

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

    /// The health line is the server's word on opencodex, and no address is its own answer, not
    /// opencodex being down. The command that starts opencodex is offered only where it would
    /// reach the server: on the server's own Mac.
    #[test]
    fn the_health_line_is_the_servers_word() {
        assert_eq!(
            health_line(ProxyHealth::Running, true),
            ("opencodex is running", None)
        );
        assert_eq!(
            health_line(ProxyHealth::NotRunning, true),
            ("Not running — start it with", Some("ocx start"))
        );
        assert_eq!(
            health_line(ProxyHealth::NotRunning, false),
            ("Not running on the server's machine", None)
        );
        for here in [true, false] {
            assert_eq!(
                health_line(ProxyHealth::NoAddress, here),
                ("No address saved", None)
            );
        }
        assert_eq!(
            [
                ProxyHealth::Running,
                ProxyHealth::NotRunning,
                ProxyHealth::NoAddress
            ]
            .map(health_word),
            ["running", "not-running", "no-address"]
        );
    }

    /// The line under the picker says why it offers nothing, or that what it offers may be old.
    #[test]
    fn the_picker_says_why_it_offers_what_it_does() {
        assert_eq!(
            models_note(ProxyHealth::NoAddress, false),
            Some(NO_MODELS_WITHOUT_ADDRESS)
        );
        assert_eq!(
            models_note(ProxyHealth::NotRunning, false),
            Some(NO_MODELS_NOT_RUNNING)
        );
        assert_eq!(
            models_note(ProxyHealth::NotRunning, true),
            Some(MODELS_MAY_BE_OLD)
        );
        assert_eq!(models_note(ProxyHealth::Running, false), Some(NO_MODELS));
        assert_eq!(models_note(ProxyHealth::Running, true), None);
    }
}
