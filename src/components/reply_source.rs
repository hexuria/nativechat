//! Settings → Relay: the person's ChatGPT or Grok plan, relayed to the server from this computer,
//! and where a newly hired Bot starts. And the badge on each reply, which says which door it came
//! through. A Bot's door is picked with its model, on its card in its settings
//! (`components::model_picker`), and nowhere else: the page switches no door, and the server keeps
//! the account's as it is for the Bots that have picked none of their own.
//!
//! The relay (opengrok-server #292): while it is on and the app is open, the server sends each
//! model call on the person's plan down a stream to this computer, whose background helper asks
//! its own opencodex and streams the answer back (`opengrok::relay`). The page has the switch that
//! makes this computer the relay, where the relay stands, and opencodex's address and key here,
//! which Save keeps on this computer, the key in its secure storage (the Keychain). The window
//! still calls no model, and the key never goes to the server. The relay answers only the turns
//! that ask through this computer, so the same press that turns it on points the account's way to
//! the plan here (`via: "mac"`); turning it off moves no way (`AppState::begin_relay_change`).
//!
//! The page no longer sets up the plan on the server's own machine, the proxy address and key the
//! server dials on its loopback, and sends neither: whatever the account keeps there stays as the
//! server keeps it, and the Bots that go that way keep going as the server decides. Nor does it
//! pick a model of the relay's own: each Bot picks its own, and new Bots start on the default
//! below.
//!
//! The words say "this computer". The relay is the app on the person's computer passing their
//! plan's replies to the server, which is not an idea only a Mac can have; the wire still calls
//! the way `via: "mac"`, and the code keeps its names.
//!
//! The page ends with Default for new Bots, where a newly hired Bot will start: the same card and
//! popover a Bot's model is picked in (`components::model_picker`), its model, effort and ⚡, with
//! None over the list. A server that keeps such a default says so by the key on its read
//! (opengrok-server PR #322 new-bot-default, not yet on main), and every pick is kept on the
//! account at once; on one that keeps none the section says it is coming and its card takes no
//! click (`state::DefaultForNewBots`).
//!
//! The words and element ids live here so the gpui-agent tree (`agent/host.rs`) says what the
//! window says and names what the window names.

use crate::components::fields::field_input;
use crate::components::model_picker::{self, ModelPicker};
use crate::components::switch::Switch;
use crate::opengrok::{DEFAULT_PROXY_URL, InferenceKind, RelayStatus, ReplySource, Via};
use crate::state::{
    AccountChange, AppState, DefaultForNewBots, PickerFor, REPLY_SOURCE_NOT_ON_SERVER, RelayLine,
    RelayMac, ReplySourceNote, ReplySourceRead, ReplySourceSettings,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// Settings → Relay, in the settings' own list of pages. The id is the one the tab has always
/// had, so a driver that opened it by name still does.
pub(crate) const SETTINGS_TAB: &str = "settings-tab-reply-source";
/// What the tab, and the page's title, are called.
pub(crate) const TAB_LABEL: &str = "Relay";
/// The section: the whole page, once the setting has been read.
pub(crate) const SECTION: &str = "settings-reply-source";
pub(crate) const SAVE: &str = "settings-reply-source-save";
/// Beside Save: what it will not keep as the page stands.
pub(crate) const HINT: &str = "settings-reply-source-hint";
/// Under Save: a read that failed, and what the Keychain said when it did not keep a key.
pub(crate) const ERROR: &str = "settings-reply-source-error";
/// In place of the page: asking the server, a server without reply sources, or a setting that
/// could not be read.
pub(crate) const UNAVAILABLE: &str = "settings-reply-source-unavailable";
/// In a Bot's Usage card, while replies go through the person's own plan.
pub(crate) const BOT_USAGE_PLAN: &str = "agent-usage-plan";
/// Relay your plan from this computer: the card, and each of its controls and lines.
pub(crate) const RELAY: &str = "settings-relay";
pub(crate) const RELAY_SWITCH: &str = "settings-relay-switch";
/// Under the switch: what became of what it sent the account when that did not go as asked, a
/// refusal in the server's words or that nobody knows whether it was kept.
pub(crate) const RELAY_SWITCH_ERROR: &str = "settings-relay-switch-error";
/// Where the relay stands: this computer relaying, another computer, connecting, or not
/// connected.
pub(crate) const RELAY_STATUS: &str = "settings-relay-status";
/// Under the status line: why this computer is not connected, or how it takes the relay back.
pub(crate) const RELAY_DETAIL: &str = "settings-relay-detail";
pub(crate) const RELAY_ADDR: &str = "settings-relay-addr";
pub(crate) const RELAY_KEY: &str = "settings-relay-key";
/// Beside the key field while the Keychain holds a key: Remove key, and Keep key to take it back.
pub(crate) const RELAY_KEY_REMOVE: &str = "settings-relay-key-remove";
/// Why the relay takes no change: a server without the relay, or a computer not enrolled.
pub(crate) const RELAY_UNAVAILABLE: &str = "settings-relay-unavailable";
/// Default for new Bots: the section, the line saying why it takes no change, and the picker's
/// card in it, whose popover's parts are `model_picker::NEW_BOTS_IDS`.
pub(crate) const NEW_BOTS: &str = "settings-new-bots";
pub(crate) const NEW_BOTS_UNAVAILABLE: &str = "settings-new-bots-unavailable";
pub(crate) const NEW_BOTS_CARD: &str = "settings-new-bots-card";

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
/// The page's opening line, as the approved design words it: what the relay is for.
pub(crate) const INTRO: &str = "Your ChatGPT or Grok plan answers through this computer.";
/// The same, from a server that keeps a door per Bot (opengrok-server main d6f640e (#307, after
/// #304), pin bf99845), where a Bot picks its plan's model, and with it its door, on its card.
pub(crate) const INTRO_PER_BOT: &str = "Your ChatGPT or Grok plan answers through this computer. \
     Bots use it when they pick a Subscription model.";
pub(crate) const RELAY_TITLE: &str = "Relay your plan from this computer";
pub(crate) const RELAY_INTRO: &str = "While this is on and the app is open, the server sends \
     your plan's replies here, and this computer asks its own opencodex.";
/// Why the switch is dead: the server has no relay. The relay's own sentence for the same.
pub(crate) const RELAY_NOT_ON_SERVER: &str = crate::opengrok::SERVER_WITHOUT_RELAY;
/// Why the switch is dead: this computer has no machine token to open the stream with.
pub(crate) const RELAY_NOT_ENROLLED: &str = "This computer isn't enrolled with the server yet, so \
     it can't relay. It enrols when you sign in.";
pub(crate) const RELAY_ADDR_LABEL: &str = "opencodex address";
pub(crate) const RELAY_KEY_LABEL: &str = "opencodex key (kept in this computer's secure storage)";
pub(crate) const RELAY_KEY_PLACEHOLDER: &str = "opencodex key, if it asks for one";
pub(crate) const RELAY_KEY_KEPT: &str =
    "This computer's secure storage holds a key. Type one to replace it.";
pub(crate) const RELAY_KEY_GOES: &str =
    "The key leaves this computer's secure storage when you save.";
/// Where the relay's key was typed and the page left before a Save.
pub(crate) const RELAY_RETYPE_KEY: &str = "Type the key again to keep it on this computer.";
/// Under the status line once another computer took the relay from this one.
pub(crate) const RELAY_TAKE_BACK: &str =
    "Turn this off and on to relay from this computer instead.";
pub(crate) const REMOVE_KEY_LABEL: &str = "Remove key";
pub(crate) const KEEP_KEY_LABEL: &str = "Keep key";
pub(crate) const SAVE_LABEL: &str = "Save";
pub(crate) const NOT_SAVED: &str = "Not saved yet";
/// In a Bot's Usage card while its replies go through the person's own plan: a turn there is not
/// metered and carries no gateway key, so the server's usage report never counts it.
pub(crate) const PLAN_USAGE_NOTE: &str = "Replies on your own subscription aren't counted here.";
pub(crate) const NEW_BOTS_TITLE: &str = "Default for new Bots";
/// The line under Default for new Bots' title while the server keeps one.
pub(crate) const NEW_BOTS_LINE: &str = "A newly hired Bot starts on this model, effort and ⚡. \
     None leaves it to the server's default.";
/// Why Default for new Bots takes no change: the server keeps no default for new Bots yet
/// (`state::DefaultForNewBots`).
pub(crate) const NEW_BOTS_COMING_SOON: &str =
    "Coming soon: the server can't keep a default for new Bots yet.";

/// The page's opening line, which says a Bot picks a Subscription model, and with it the plan,
/// only to a server that keeps a door per Bot.
pub(crate) fn intro(per_bot: bool) -> &'static str {
    if per_bot { INTRO_PER_BOT } else { INTRO }
}

/// The relay's status line, as it reads: who is relaying, and nothing of what the relay is doing.
pub(crate) fn relay_line_words(line: &RelayLine) -> String {
    match line {
        RelayLine::Answering => "This computer is relaying".to_string(),
        RelayLine::Another { label: Some(label) } => {
            format!("Another computer ({label}) is relaying")
        }
        RelayLine::Another { label: None } => "Another computer is relaying".to_string(),
        RelayLine::Connecting => "Connecting…".to_string(),
        RelayLine::NotConnected { .. } => "Not connected".to_string(),
    }
}

/// The status line's word for a driver: `answering`, `another-mac`, `connecting` or
/// `not-connected`. A driver's words, kept as they were when the page said "Mac", so a script
/// that asserts them still does.
#[cfg(any(feature = "agent", test))]
pub(crate) fn relay_line_word(line: &RelayLine) -> &'static str {
    match line {
        RelayLine::Answering => "answering",
        RelayLine::Another { .. } => "another-mac",
        RelayLine::Connecting => "connecting",
        RelayLine::NotConnected { .. } => "not-connected",
    }
}

/// Under the status line: why this computer is not connected, in the relay's words, or, once
/// another computer took the relay from this one, how to take it back.
pub(crate) fn relay_detail(line: &RelayLine, relay: &RelayMac) -> Option<String> {
    match line {
        RelayLine::NotConnected { why } => why.clone(),
        RelayLine::Another { .. }
            if relay
                .report
                .as_ref()
                .is_some_and(|report| report.status == RelayStatus::Replaced) =>
        {
            Some(RELAY_TAKE_BACK.to_string())
        }
        _ => None,
    }
}

/// Why the relay takes no change, if it takes none: a server without the relay, or a computer not
/// enrolled with it.
pub(crate) fn relay_unavailable_line(knows_relay: bool, enrolled: bool) -> Option<&'static str> {
    if !knows_relay {
        Some(RELAY_NOT_ON_SERVER)
    } else if !enrolled {
        Some(RELAY_NOT_ENROLLED)
    } else {
        None
    }
}

/// What a reply's badge reads: whose keys paid for it, that the person's Mac answered it where it
/// did ([`badge_label`]), and ⚡ where the model that answered is a fast twin, which is all fast
/// ever is on the wire.
pub(crate) fn badge_words(source: &ReplySource) -> String {
    let fast = source
        .model
        .as_deref()
        .is_some_and(crate::opengrok::is_fast);
    format!(
        "{}{}",
        badge_label(source.kind, source.via),
        if fast { " ⚡" } else { "" }
    )
}

/// What a reply's badge reads of its door: whose keys paid for it, and for a reply the person's
/// Mac answered through the relay, that the Mac did.
pub(crate) fn badge_label(kind: InferenceKind, via: Option<Via>) -> &'static str {
    match (kind, via) {
        (InferenceKind::Gateway, _) => "paid key",
        (InferenceKind::LocalProxy, Some(Via::Mac)) => "your plan · Mac",
        (InferenceKind::LocalProxy, _) => "your plan",
    }
}

/// The line in place of the page, while there is no page to show: `None` once it has been read.
pub(crate) fn unavailable_line(settings: &ReplySourceSettings) -> Option<&str> {
    match &settings.kept {
        None | Some(ReplySourceRead::Loading) => Some(ASKING),
        Some(ReplySourceRead::NotOnServer) => Some(REPLY_SOURCE_NOT_ON_SERVER),
        Some(ReplySourceRead::Unavailable(why)) => Some(why),
        Some(ReplySourceRead::Read(_)) => None,
    }
}

/// The line under Save: the note about the last thing that did not go as asked.
pub(crate) fn error_line(settings: &ReplySourceSettings) -> Option<&str> {
    settings.note.as_ref().map(ReplySourceNote::line)
}

/// The line under Save is trouble, drawn in the danger colour: a failed read. What the Keychain
/// said is drawn muted, as it always has been.
pub(crate) fn error_is_trouble(settings: &ReplySourceSettings) -> bool {
    matches!(settings.note, Some(ReplySourceNote::ReadFailed(_)))
}

/// Settings → Relay. Its fields need a window, so it is made on the first render of the tab, as
/// Settings → Logins is.
pub struct ReplySourcePage {
    state: Entity<AppState>,
    /// The relay's two fields: opencodex's address on this computer, and its key.
    relay_address: Entity<InputState>,
    relay_key: Entity<InputState>,
    /// Default for new Bots' picker: the Bot's card and popover, for the account's default.
    new_bots: Entity<ModelPicker>,
}

impl ReplySourcePage {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let relay_address = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_PROXY_URL));
        let relay_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(RELAY_KEY_PLACEHOLDER)
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
        // What is typed is the form: the state keeps the copy Save keeps and a driver writes. A
        // field takes typing only while the page draws it editable, as Remove key takes a click
        // only then. Keys that reach one the page has just made read-only, before it is drawn
        // so, change no draft, and the field is put back to what the page holds rather than left
        // showing what nothing took.
        cx.subscribe_in(
            &relay_address,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                if this.state.read(cx).reply_source.relay_editable() {
                    let typed = input.read(cx).value().to_string();
                    this.state
                        .update(cx, |state, cx| state.set_relay_address(typed, cx));
                }
                this.sync_inputs(window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &relay_key,
            window,
            |this, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                if this.state.read(cx).reply_source.relay_key_editable() {
                    let typed = input.read(cx).value().to_string();
                    this.state
                        .update(cx, |state, cx| state.set_relay_key(&typed, cx));
                }
                this.sync_inputs(window, cx);
            },
        )
        .detach();
        let new_bots = cx.new(|cx| ModelPicker::new(window, state.clone(), PickerFor::NewBots, cx));
        let mut page = Self {
            state,
            relay_address,
            relay_key,
            new_bots,
        };
        page.sync_inputs(window, cx);
        page
    }

    /// The fields follow the state: an address a driver wrote, and the one kept after a Save;
    /// and the key field holds the key waiting for Save, drawn masked, one a driver wrote
    /// included, and nothing once it is in the Keychain, dropped, or never was.
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (relay_address, relay_key) = {
            let state = self.state.read(cx);
            let settings = &state.reply_source;
            let field = self.relay_key.read(cx).value();
            let relay_key = match settings.relay_key_draft.as_ref() {
                None => (!field.trim().is_empty()).then(String::new),
                Some(key) => (field.trim() != key.expose()).then(|| key.expose().to_string()),
            };
            let relay_address = settings
                .relay_address_draft
                .clone()
                .unwrap_or_else(|| state.relay_mac.shown_address());
            (relay_address, relay_key)
        };
        if self.relay_address.read(cx).value().as_ref() != relay_address.as_str() {
            self.relay_address
                .update(cx, |input, cx| input.set_value(relay_address, window, cx));
        }
        if let Some(key) = relay_key {
            self.relay_key
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

fn labelled(label: &'static str, muted: Hsla, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap(px(4.))
        .child(div().text_xs().text_color(muted).child(label))
        .child(control)
}

impl ReplySourcePage {
    /// Relay your plan from this computer: the switch that makes this computer the relay, where
    /// the relay stands, and opencodex's address and key here. The switch acts at once, and as it
    /// goes on it also points the account at this computer (`AppState::set_relay_on`), with what
    /// became of that under it when it did not go as asked; the address and the key wait for
    /// Save.
    fn relay_card(&self, state: &AppState, theme: &Theme) -> impl IntoElement {
        let muted = theme.muted_foreground;
        let app = self.state.clone();
        let settings = &state.reply_source;
        let relay = &state.relay_mac;
        let live = settings.relay_editable();
        let unavailable = relay_unavailable_line(settings.knows_relay(), state.relay_enrolled());
        let switch_live = state.relay_switch_live();
        let line = state.relay_line();
        let words = relay_line_words(&line);
        let answering = matches!(line, RelayLine::Answering);
        let detail = relay_detail(&line, relay);
        let failed = matches!(line, RelayLine::NotConnected { why: Some(_) });
        let has_key = relay.has_key;
        let removing = settings.relay_remove_key;
        let key_live = settings.relay_key_editable();
        let retype = settings.relay_retype_key;
        let on = relay.on;
        let switch_note = settings
            .change_note(AccountChange::Relay)
            .map(str::to_string);
        card()
            .flex()
            .flex_col()
            .id(RELAY)
            .px(px(16.))
            .py(px(14.))
            .gap(px(12.))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .gap(px(2.))
                            .child(div().text_sm().child(RELAY_TITLE))
                            .child(div().text_xs().text_color(muted).child(RELAY_INTRO)),
                    )
                    .child({
                        let app = app.clone();
                        Switch::new(RELAY_SWITCH)
                            .checked(on)
                            .accessibility_label(RELAY_TITLE)
                            .disabled(!switch_live)
                            .on_click(move |next, _, cx| {
                                app.update(cx, |state, cx| state.set_relay_on(*next, cx));
                            })
                    }),
            )
            // The server's words for what the switch sent the account, when it did not keep it.
            // The switch stays as the person put it, and the relay runs as it says.
            .when_some(switch_note, |this, note| {
                this.child(
                    div()
                        .id(RELAY_SWITCH_ERROR)
                        .text_xs()
                        .text_color(theme.danger)
                        .child(note),
                )
            })
            .when_some(unavailable, |this, line| {
                this.child(
                    div()
                        .id(RELAY_UNAVAILABLE)
                        .text_sm()
                        .text_color(muted)
                        .child(line),
                )
            })
            .child(
                h_flex()
                    .id(RELAY_STATUS)
                    .gap(px(8.))
                    .items_center()
                    .child(div().size(px(8.)).rounded_full().bg(if answering {
                        theme.green
                    } else {
                        muted
                    }))
                    .child(div().text_sm().child(words)),
            )
            .when_some(detail, |this, detail| {
                this.child(
                    div()
                        .id(RELAY_DETAIL)
                        .text_xs()
                        .text_color(if failed { theme.danger } else { muted })
                        .child(detail),
                )
            })
            .child(labelled(
                RELAY_ADDR_LABEL,
                muted,
                div()
                    .id(RELAY_ADDR)
                    .child(field_input(&self.relay_address).disabled(!live)),
            ))
            .child(labelled(
                RELAY_KEY_LABEL,
                muted,
                v_flex()
                    .gap(px(4.))
                    .child(
                        div()
                            .id(RELAY_KEY)
                            .child(field_input(&self.relay_key).disabled(!key_live)),
                    )
                    .when(retype, |this| {
                        this.child(div().text_xs().text_color(muted).child(RELAY_RETYPE_KEY))
                    })
                    .when(has_key, |this| {
                        this.child(
                            h_flex()
                                .gap(px(8.))
                                .items_center()
                                .child(div().text_xs().text_color(muted).child(if removing {
                                    RELAY_KEY_GOES
                                } else {
                                    RELAY_KEY_KEPT
                                }))
                                .child(
                                    Button::new(RELAY_KEY_REMOVE)
                                        .label(if removing {
                                            KEEP_KEY_LABEL
                                        } else {
                                            REMOVE_KEY_LABEL
                                        })
                                        .ghost()
                                        .small()
                                        .disabled(!live)
                                        .on_click(move |_, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.toggle_remove_relay_key(cx)
                                            });
                                        }),
                                ),
                        )
                    }),
            ))
    }
}

impl ReplySourcePage {
    /// Default for new Bots: where a newly hired Bot starts, in the picker's card. While the
    /// server keeps a default for new Bots the card is the Bot's, opening the same popover, and
    /// what a refusal said is under it while the popover is shut; while it keeps none the section
    /// says it is coming and the card is dimmed and takes no click, since a pick here would change
    /// nothing on the server.
    fn new_bots_section(&self, state: &AppState, theme: &Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let section = v_flex()
            .id(NEW_BOTS)
            .gap(px(8.))
            .child(div().text_xs().text_color(muted).child(NEW_BOTS_TITLE));
        match state.default_for_new_bots() {
            DefaultForNewBots::NotOnServer => section
                .child(
                    div()
                        .id(NEW_BOTS_UNAVAILABLE)
                        .text_sm()
                        .text_color(muted)
                        .child(NEW_BOTS_COMING_SOON),
                )
                .child(model_picker::dead_card(NEW_BOTS_CARD, theme))
                .into_any_element(),
            DefaultForNewBots::Kept(_) => {
                let note = state
                    .reply_source
                    .change_note(AccountChange::NewBots)
                    .filter(|_| !state.new_bots_picker.open)
                    .map(str::to_string);
                section
                    .child(div().text_sm().text_color(muted).child(NEW_BOTS_LINE))
                    .child(self.new_bots.clone())
                    .when_some(note, |this, note| {
                        this.child(
                            div()
                                .id(model_picker::NEW_BOTS_IDS.error)
                                .text_xs()
                                .text_color(theme.danger)
                                .child(note),
                        )
                    })
                    .into_any_element()
            }
        }
    }
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
        let per_bot = state.server_keeps_bot_doors();
        let intro = div().text_xs().text_color(muted).child(intro(per_bot));
        // Its own section, whatever the reply source: the default for new Bots is a setting of
        // its own, and says so whether or not the reply source could be read.
        let new_bots = self.new_bots_section(state, &theme);
        let error = error_line(settings).map(str::to_string);
        let trouble = error_is_trouble(settings);
        let section = v_flex().id(SECTION).gap(px(12.)).child(intro);
        if settings.kept_source().is_none() {
            let line = unavailable_line(settings).unwrap_or(ASKING).to_string();
            let failed = matches!(settings.kept, Some(ReplySourceRead::Unavailable(_)));
            return section
                .child(
                    div()
                        .id(UNAVAILABLE)
                        .text_sm()
                        .text_color(if failed { theme.danger } else { muted })
                        .child(line),
                )
                .child(new_bots);
        }
        // A server from before the relay has nothing for this computer to relay to, and the card
        // would be a switch that changes nothing: the page says so in its place, and there is
        // nothing to save.
        if !settings.knows_relay() {
            return section
                .child(
                    div()
                        .id(RELAY_UNAVAILABLE)
                        .text_sm()
                        .text_color(muted)
                        .child(RELAY_NOT_ON_SERVER),
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
                .child(new_bots);
        }
        let hint = state.reply_source_hint();
        let can_save = state.reply_source_can_save();
        let unsaved = settings.is_unsaved();
        // The relay, Save for its address and key, and the default for new Bots. There is no
        // radio and nothing of the plan on the server's own machine: where a Bot's replies go is
        // picked with its model on its card, and the account's setting stays as the server keeps
        // it.
        section
            .child(self.relay_card(state, &theme))
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
                        this.child(div().text_xs().text_color(muted).child(NOT_SAVED))
                    })
                    .child(
                        Button::new(SAVE)
                            .label(SAVE_LABEL)
                            .small()
                            .disabled(!can_save)
                            .on_click(move |_, _, cx| {
                                app.update(cx, |state, cx| state.save_reply_source(cx));
                            }),
                    ),
            )
            // After Save, which does not keep it: a pick for new Bots is kept at once.
            .child(new_bots)
    }
}

/// A reply's badge: "paid key", "your plan" or "your plan · Mac", with ⚡ for a fast twin, and the
/// model on hover when the server named one.
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
        .child(badge_words(source))
}

#[cfg(test)]
mod tests {
    // Item by item rather than a glob: `use super::*` would drag in gpui_kit's own `test`.
    use super::{badge_label, badge_words};
    use crate::opengrok::{InferenceKind, Via};

    /// The words a reply's badge says, as the contract's doors: the server's keys, the plan on the
    /// server's own machine, and the plan the person's Mac answers through the relay. A way never
    /// named, as from a server before the relay, reads as the plan.
    #[test]
    fn each_door_reads_as_itself_on_its_badge() {
        let doors = [
            (InferenceKind::Gateway, None),
            (InferenceKind::LocalProxy, None),
            (InferenceKind::LocalProxy, Some(Via::Loopback)),
            (InferenceKind::LocalProxy, Some(Via::Mac)),
        ];
        assert_eq!(
            doors.map(|(kind, via)| badge_label(kind, via)),
            ["paid key", "your plan", "your plan", "your plan · Mac"]
        );
    }

    /// A reply's badge says ⚡ when the model that answered is a fast twin, on either door, and
    /// nothing of the kind for a plain model or one the server did not name.
    #[test]
    fn a_reply_on_a_fast_twin_says_so_on_its_badge() {
        use crate::opengrok::ReplySource;
        let badge = |kind, via, model: Option<&str>| {
            badge_words(&ReplySource {
                kind,
                via,
                model: model.map(str::to_string),
            })
        };
        assert_eq!(
            badge(InferenceKind::LocalProxy, None, Some("gpt-6-luna--fast")),
            "your plan ⚡"
        );
        assert_eq!(
            badge(
                InferenceKind::LocalProxy,
                Some(Via::Mac),
                Some("grok-4.7--fast")
            ),
            "your plan · Mac ⚡"
        );
        assert_eq!(
            badge(InferenceKind::Gateway, None, Some("oag/fast--fast")),
            "paid key ⚡"
        );
        assert_eq!(
            badge(InferenceKind::LocalProxy, None, Some("gpt-6-luna")),
            "your plan"
        );
        assert_eq!(badge(InferenceKind::Gateway, None, None), "paid key");
    }

    /// The tab is Relay, and the page opens with what the relay is for, in the approved words:
    /// that a Bot uses the plan when it picks a Subscription model only to a server that keeps a
    /// door per Bot. Neither offers a radio's words, nor a model of the plan's or the relay's
    /// own.
    #[test]
    fn the_page_is_relay_and_switches_no_door() {
        use super::{INTRO, INTRO_PER_BOT, RELAY_TITLE, TAB_LABEL, intro};
        assert_eq!(TAB_LABEL, "Relay");
        assert_eq!(RELAY_TITLE, "Relay your plan from this computer");
        assert_eq!(intro(true), INTRO_PER_BOT);
        assert_eq!(intro(false), INTRO);
        assert_eq!(
            INTRO_PER_BOT,
            "Your ChatGPT or Grok plan answers through this computer. Bots use it when they \
             pick a Subscription model."
        );
        assert!(!INTRO.contains("Bots use it"));
        for words in [INTRO, INTRO_PER_BOT] {
            for gone in [
                "Server (paid keys)",
                "My subscription",
                "haven't picked",
                "hasn't chosen",
            ] {
                assert!(!words.contains(gone), "{words:?} says {gone:?}");
            }
        }
    }

    /// Every word the page says, and every sentence of the relay's it shows, says "computer"
    /// where it said "Mac": the relay is the app on the person's computer, which need not be a
    /// Mac. The relay's own sentences are among them, as its status and its errors are drawn on
    /// the page under the status line.
    #[test]
    fn the_page_and_the_relays_sentences_say_computer_and_never_mac() {
        use super::{
            ASKING, INTRO, INTRO_PER_BOT, KEEP_KEY_LABEL, NEW_BOTS_COMING_SOON, NEW_BOTS_LINE,
            NEW_BOTS_TITLE, NOT_SAVED, RELAY_ADDR_LABEL, RELAY_INTRO, RELAY_KEY_GOES,
            RELAY_KEY_KEPT, RELAY_KEY_LABEL, RELAY_KEY_PLACEHOLDER, RELAY_NOT_ENROLLED,
            RELAY_NOT_ON_SERVER, RELAY_RETYPE_KEY, RELAY_TAKE_BACK, RELAY_TITLE, REMOVE_KEY_LABEL,
            SAVE_LABEL, TAB_LABEL, relay_line_words,
        };
        use crate::opengrok::{
            SERVER_QUIET, SERVER_UNREACHED, SERVER_WITHOUT_RELAY, TOKEN_REFUSED,
        };
        use crate::state::{RELAY_ADDRESS_NOT_HERE, RelayLine};
        let lines = [
            RelayLine::Answering,
            RelayLine::Another {
                label: Some("NativeChat on studio".into()),
            },
            RelayLine::Another { label: None },
            RelayLine::Connecting,
            RelayLine::NotConnected { why: None },
        ]
        .map(|line| relay_line_words(&line));
        let words: Vec<&str> = [
            TAB_LABEL,
            INTRO,
            INTRO_PER_BOT,
            RELAY_TITLE,
            RELAY_INTRO,
            RELAY_NOT_ON_SERVER,
            RELAY_NOT_ENROLLED,
            RELAY_ADDR_LABEL,
            RELAY_KEY_LABEL,
            RELAY_KEY_PLACEHOLDER,
            RELAY_KEY_KEPT,
            RELAY_KEY_GOES,
            RELAY_RETYPE_KEY,
            RELAY_TAKE_BACK,
            REMOVE_KEY_LABEL,
            KEEP_KEY_LABEL,
            SAVE_LABEL,
            NOT_SAVED,
            NEW_BOTS_TITLE,
            NEW_BOTS_COMING_SOON,
            NEW_BOTS_LINE,
            ASKING,
            RELAY_ADDRESS_NOT_HERE,
            TOKEN_REFUSED,
            SERVER_QUIET,
            SERVER_UNREACHED,
            SERVER_WITHOUT_RELAY,
        ]
        .into_iter()
        .chain(lines.iter().map(String::as_str))
        .collect();
        for said in &words {
            assert!(!said.contains("Mac"), "{said:?}");
        }
        assert_eq!(
            lines,
            [
                "This computer is relaying",
                "Another computer (NativeChat on studio) is relaying",
                "Another computer is relaying",
                "Connecting…",
                "Not connected",
            ]
        );
        for said in [
            RELAY_NOT_ENROLLED,
            RELAY_KEY_LABEL,
            RELAY_KEY_KEPT,
            RELAY_RETYPE_KEY,
            RELAY_TAKE_BACK,
            RELAY_NOT_ON_SERVER,
            TOKEN_REFUSED,
        ] {
            assert!(said.contains("computer"), "{said:?}");
        }
        assert_eq!(
            RELAY_KEY_LABEL,
            "opencodex key (kept in this computer's secure storage)"
        );
    }

    /// Default for new Bots says it is coming, in the owner's words, while the server keeps no
    /// such default, which is also before the setting is read: the picker's card in it names no
    /// model and the effort a Bot with none of its own reads as, and takes no click. Where the
    /// server keeps one, the section says what the card is, and that None leaves it to the
    /// server.
    #[test]
    fn the_default_for_new_bots_is_coming_and_its_card_is_dead() {
        use super::{NEW_BOTS_COMING_SOON, NEW_BOTS_LINE, NEW_BOTS_TITLE};
        use crate::components::model_picker::dead_card_words;
        use crate::state::DefaultForNewBots;
        assert_eq!(NEW_BOTS_TITLE, "Default for new Bots");
        assert_eq!(
            NEW_BOTS_COMING_SOON,
            "Coming soon: the server can't keep a default for new Bots yet."
        );
        assert_eq!(DefaultForNewBots::default(), DefaultForNewBots::NotOnServer);
        assert_eq!(
            crate::state::AppState::new().default_for_new_bots(),
            DefaultForNewBots::NotOnServer
        );
        assert_eq!(dead_card_words(), ("No model", "Default"));
        assert!(NEW_BOTS_LINE.contains("None leaves it to the server's default"));
    }

    /// The relay tab says who is relaying and nothing of what the relay is doing: no count of the
    /// replies in progress, nor any other word of live activity, while this computer is answering
    /// calls.
    #[test]
    fn the_relay_tab_says_no_replies_in_progress() {
        use super::relay_line_words;
        use crate::opengrok::{RelayReport, RelayStatus};
        let mut state = crate::state::AppState::new();
        state.relay_mac.report = Some(RelayReport {
            status: RelayStatus::Answering,
            halted: false,
        });
        let said = relay_line_words(&state.relay_line());
        assert_eq!(said, "This computer is relaying");
        for activity in ["in progress", "repl", "call"] {
            assert!(!said.contains(activity), "{said:?} says {activity:?}");
        }
    }

    /// The relay's status line says who is relaying, and its word for a driver; the line under
    /// it says why this computer is not connected, or, once another computer took the relay from
    /// it, how to take it back; and the card says why it takes no change when it takes none.
    #[test]
    fn the_relay_says_who_is_relaying() {
        use super::{
            RELAY_NOT_ENROLLED, RELAY_NOT_ON_SERVER, RELAY_TAKE_BACK, relay_detail,
            relay_line_word, relay_line_words, relay_unavailable_line,
        };
        use crate::opengrok::{RelayReport, RelayStatus};
        use crate::state::{RelayLine, RelayMac};
        let lines = [
            (RelayLine::Answering, "answering"),
            (
                RelayLine::Another {
                    label: Some("NativeChat on studio".into()),
                },
                "another-mac",
            ),
            (RelayLine::Connecting, "connecting"),
            (RelayLine::NotConnected { why: None }, "not-connected"),
        ];
        for (line, word) in lines {
            assert_eq!(relay_line_word(&line), word, "{}", relay_line_words(&line));
        }
        let idle = RelayMac::default();
        let replaced = RelayMac {
            report: Some(RelayReport {
                status: RelayStatus::Replaced,
                halted: true,
            }),
            ..RelayMac::default()
        };
        let another = RelayLine::Another { label: None };
        assert_eq!(relay_detail(&another, &idle), None, "switched off here");
        assert_eq!(
            relay_detail(&another, &replaced).as_deref(),
            Some(RELAY_TAKE_BACK)
        );
        assert_eq!(
            relay_detail(
                &RelayLine::NotConnected {
                    why: Some("Can't reach the server. Trying again…".into())
                },
                &idle
            )
            .as_deref(),
            Some("Can't reach the server. Trying again…")
        );
        assert_eq!(
            relay_unavailable_line(false, true),
            Some(RELAY_NOT_ON_SERVER)
        );
        assert_eq!(
            relay_unavailable_line(true, false),
            Some(RELAY_NOT_ENROLLED)
        );
        assert_eq!(relay_unavailable_line(true, true), None);
    }
}
