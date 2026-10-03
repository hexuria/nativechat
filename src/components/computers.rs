//! Settings → Computer's "Your computers": one card for each computer the person has enrolled,
//! from `GET /local-exec/daemon`, each with its own Relay your plan switch. The relay is the app on a
//! computer passing the person's ChatGPT or Grok plan's replies to the server (opengrok-server
//! #292); since each computer has its own switch (opengrok-server branch per-computer-relay at
//! d0a9855) the person says which computers do, from any of them, and there is no page of its own
//! for it any more.
//!
//! A computer the server lists as revoked has no card, as the roster has always left it out: the
//! server refuses to switch one (409 `revoked`), so a dimmed card would have nothing to offer.
//!
//! A card shows the computer's label with a dot for whether it is online, a "This computer" pill on
//! the one the app is running on (by the machine id it stored when it enrolled,
//! `opengrok::stored_machine_id`), its "Relay your plan" switch, and where the relay stands: it
//! relays, it does not, or it is on and the computer is asleep. The switch acts at once: it sends
//! `PATCH /local-exec/daemon/{machine_id} {relayEnabled}` for that computer, drawn where the click
//! asked to take it while the server is asked, and a refusal goes back to where it was with the
//! server's words under it (`AppState::set_computer_relay`). There is no ordering of computers: the
//! server asks the one that opened its relay stream last of those that are on.
//!
//! This computer's card alone also holds what its relay needs of this computer: opencodex's address
//! and its key, which Save keeps here, the key in its secure storage (the Keychain). The window still
//! calls no model, and the key never goes to the server.
//!
//! The words and element ids live here so the gpui-agent tree (`agent/host.rs`) says what the
//! window says and names what the window names.

use crate::components::fields::field_input;
use crate::components::switch::Switch;
use crate::opengrok::{DEFAULT_PROXY_URL, LocalExecMode, RelayReport, RelayStatus};
use crate::state::{
    AppState, REPLY_SOURCE_NOT_ON_SERVER, ReplySourceNote, ReplySourceRead, ReplySourceSettings,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The section's title, over its cards.
pub(crate) const SECTION_TITLE: &str = "Your computers";
/// Under the title: what the switch on each card is for.
pub(crate) const SECTION_INTRO: &str = "Relay your plan lets a computer answer your Subscription \
     Bots with your ChatGPT or Grok plan while NativeChat is open on it.";
/// On the page while the person has no enrolled computer.
pub(crate) const NO_COMPUTERS: &str =
    "No computers yet. Stay signed in here and NativeChat enrols this computer.";
/// On the pill of the computer the app is running on.
pub(crate) const THIS_COMPUTER: &str = "This computer";
/// Beside each card's switch.
pub(crate) const RELAY_LABEL: &str = "Relay your plan";
/// The status line of a computer whose relay is on and whose stream the server holds.
pub(crate) const RELAYING: &str = "Relaying";
/// The status line of a computer that does not relay: its switch is off, or it is on and the app is
/// open on it but its stream is not up (yet).
pub(crate) const NOT_RELAYING: &str = "Not relaying";
/// The status line of a computer whose relay is on, which is not relaying, and which the server
/// cannot reach: the app is not open on it, or the computer is asleep.
pub(crate) const ON_BUT_ASLEEP: &str = "On, but asleep: it can't answer right now";
/// Under this computer's status while its relay is opening its stream.
pub(crate) const CONNECTING: &str = "Connecting…";
/// Under this computer's status after another stream of this computer took the relay from it.
pub(crate) const TAKE_BACK: &str = "Another NativeChat on this computer took over the relay. \
     Turn this off and on to take it back.";

/// A card, by the server's machine id: the whole card, and each of its controls and lines.
pub(crate) fn card_id(machine_id: &str) -> String {
    format!("settings-computer-{machine_id}")
}

/// The card's Relay your plan switch.
pub(crate) fn relay_switch_id(machine_id: &str) -> String {
    format!("settings-computer-{machine_id}-relay")
}

/// Where the card's relay stands: relaying, not relaying, or on but asleep.
pub(crate) fn status_id(machine_id: &str) -> String {
    format!("settings-computer-{machine_id}-status")
}

/// Under the switch: what became of a switch that did not go as asked, in the server's words, or
/// that nobody knows whether it was kept.
pub(crate) fn error_id(machine_id: &str) -> String {
    format!("settings-computer-{machine_id}-error")
}

/// The "This computer" pill, which only a window test addresses: the gpui-agent tree says it as the
/// card's state `this-computer`.
fn pill_id(machine_id: &str) -> String {
    format!("settings-computer-{machine_id}-pill")
}

/// Under this computer's status line: why it is not relaying, in the relay's own words.
pub(crate) const RELAY_DETAIL: &str = "settings-relay-detail";
/// Beside Save: what it will not keep as the card stands.
pub(crate) const HINT: &str = "settings-reply-source-hint";
pub(crate) const SAVE: &str = "settings-reply-source-save";
/// Under Save: a read that failed, and what the Keychain said when it did not keep a key.
pub(crate) const ERROR: &str = "settings-reply-source-error";
/// In place of this computer's opencodex fields: asking the server, a server without reply sources,
/// or a setting that could not be read.
pub(crate) const UNAVAILABLE: &str = "settings-reply-source-unavailable";
pub(crate) const RELAY_ADDR: &str = "settings-relay-addr";
pub(crate) const RELAY_KEY: &str = "settings-relay-key";
/// Beside the key field while the Keychain holds a key: Remove key, and Keep key to take it back.
pub(crate) const RELAY_KEY_REMOVE: &str = "settings-relay-key-remove";
/// Why this computer's relay takes no change: a server without the relay.
pub(crate) const RELAY_UNAVAILABLE: &str = "settings-relay-unavailable";

pub(crate) const ASKING: &str = "Asking the server…";
/// Why the fields are dead: the server has no relay. The relay's own sentence for the same.
pub(crate) const RELAY_NOT_ON_SERVER: &str = crate::opengrok::SERVER_WITHOUT_RELAY;
pub(crate) const RELAY_ADDR_LABEL: &str = "opencodex address";
pub(crate) const RELAY_KEY_LABEL: &str = "opencodex key (kept in this computer's secure storage)";
pub(crate) const RELAY_KEY_PLACEHOLDER: &str = "opencodex key, if it asks for one";
pub(crate) const RELAY_KEY_KEPT: &str =
    "This computer's secure storage holds a key. Type one to replace it.";
pub(crate) const RELAY_KEY_GOES: &str =
    "The key leaves this computer's secure storage when you save.";
/// Where the relay's key was typed and the page left before a Save.
pub(crate) const RELAY_RETYPE_KEY: &str = "Type the key again to keep it on this computer.";
pub(crate) const REMOVE_KEY_LABEL: &str = "Remove key";
pub(crate) const KEEP_KEY_LABEL: &str = "Keep key";
pub(crate) const SAVE_LABEL: &str = "Save";
pub(crate) const NOT_SAVED: &str = "Not saved yet";

/// Where a computer's relay stands, as its status line says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelayState {
    /// Its relay is on and the server holds its stream: it answers the plan's replies.
    Relaying,
    /// It does not relay: its switch is off, or it is on, the computer is awake and the stream is
    /// not up.
    NotRelaying,
    /// Its switch is on, it is not relaying, and the server cannot reach it: asleep, or the app is
    /// not open on it.
    OnButAsleep,
}

impl RelayState {
    /// The status line's words.
    pub(crate) fn words(self) -> &'static str {
        match self {
            Self::Relaying => RELAYING,
            Self::NotRelaying => NOT_RELAYING,
            Self::OnButAsleep => ON_BUT_ASLEEP,
        }
    }

    /// The status line's word for a driver: `relaying`, `not-relaying` or `asleep`.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Relaying => "relaying",
            Self::NotRelaying => "not-relaying",
            Self::OnButAsleep => "asleep",
        }
    }
}

/// Where a computer's relay stands from its switch (`on`), the server holding its relay stream
/// (`relaying`) and whether the server can reach it (`online`, its reverse-exec link): relaying
/// only while it is on and the stream is up; off is not relaying whatever else is so; and on but
/// not relaying is asleep exactly when the server cannot reach it.
pub(crate) fn relay_state(on: bool, relaying: bool, online: bool) -> RelayState {
    if !on {
        RelayState::NotRelaying
    } else if relaying {
        RelayState::Relaying
    } else if !online {
        RelayState::OnButAsleep
    } else {
        RelayState::NotRelaying
    }
}

/// Under this computer's status line: why it is not relaying, in the relay's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RelayDetail {
    pub words: String,
    /// Drawn in the danger colour: the relay says something is wrong.
    pub trouble: bool,
}

/// What this computer's own relay says under its status line, if anything: that it is opening its
/// stream, why it could not, or that another NativeChat on this computer has it. Nothing while it
/// answers, or is not running.
pub(crate) fn relay_detail(report: Option<&RelayReport>) -> Option<RelayDetail> {
    match &report?.status {
        RelayStatus::Connecting => Some(RelayDetail {
            words: CONNECTING.to_string(),
            trouble: false,
        }),
        RelayStatus::Error(why) => Some(RelayDetail {
            words: why.clone(),
            trouble: true,
        }),
        RelayStatus::Replaced => Some(RelayDetail {
            words: TAKE_BACK.to_string(),
            trouble: false,
        }),
        RelayStatus::Answering | RelayStatus::Off => None,
    }
}

/// One computer's card as the window draws it, and as the gpui-agent tree says it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComputerCard {
    pub machine_id: String,
    pub label: String,
    /// The computer the app is running on.
    pub this_computer: bool,
    /// The dot: the server can reach it, and the app is open on it.
    pub online: bool,
    /// Its local-exec mode, for the Execution row.
    pub mode: LocalExecMode,
    /// The switch as drawn: the server's row, or where a click is taking it while the server is
    /// asked.
    pub relay_on: bool,
    /// The switch is with the server, and takes no click until it answers.
    pub switching: bool,
    pub state: RelayState,
    /// This computer's own relay's word on itself; never on another computer's card, which this
    /// app has no relay of.
    pub detail: Option<RelayDetail>,
    /// Under the switch: the server's words for a refusal, or that nobody knows whether a switch
    /// was kept.
    pub note: Option<String>,
}

/// The cards, in the roster's order, which has this computer first.
pub(crate) fn cards(state: &AppState) -> Vec<ComputerCard> {
    state
        .computers
        .iter()
        .map(|computer| {
            let switch = state.computer_relay_switch(&computer.machine_id);
            let relay_on = switch.map_or(computer.relay_enabled, |switch| switch.on);
            let this = computer.this_machine;
            let report = this.then_some(state.relay_mac.report.as_ref()).flatten();
            // This computer's relay knows at once what the server's row will say in a moment; and
            // the app is open on this computer, so it is not asleep.
            let relaying = computer.relaying
                || report.is_some_and(|report| report.status == RelayStatus::Answering);
            let online = computer.online || this;
            ComputerCard {
                machine_id: computer.machine_id.clone(),
                label: computer.label.clone(),
                this_computer: this,
                online,
                mode: computer.mode,
                relay_on,
                switching: switch.is_some(),
                state: relay_state(relay_on, relaying, online),
                detail: relay_on.then(|| relay_detail(report)).flatten(),
                note: state
                    .computer_relay_note(&computer.machine_id)
                    .map(str::to_string),
            }
        })
        .collect()
}

/// In place of this computer's opencodex fields, while there are none to draw: `None` once the
/// setting has been read.
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

/// A card's top section: the computer's label with its dot, the "This computer" pill, the Relay
/// your plan switch, where the relay stands, what became of a switch that did not go as asked, and
/// on this computer's card alone, why its relay is not relaying and what its relay needs of this
/// computer (`fields`). The switch acts at once (`AppState::set_computer_relay`).
pub(crate) fn relay_section(
    card: &ComputerCard,
    fields: Option<AnyElement>,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    let switch_id = relay_switch_id(&card.machine_id);
    let status = status_id(&card.machine_id);
    let says = format!("{status}-says-{}", card.state.word());
    let machine_id = card.machine_id.clone();
    let note = card.note.clone().map(|note| {
        let id = error_id(&card.machine_id);
        (id, note)
    });
    let detail = card.detail.clone();
    v_flex()
        .w_full()
        .px(px(16.))
        .py(px(14.))
        .gap(px(12.))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    h_flex()
                        .flex_1()
                        .min_w(px(0.))
                        .gap(px(8.))
                        .items_center()
                        .child(
                            div()
                                .flex_none()
                                .size(px(8.))
                                .rounded_full()
                                .bg(if card.online { theme.green } else { muted }),
                        )
                        .child(
                            div()
                                .min_w(px(0.))
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(card.label.clone()),
                        )
                        .when(card.this_computer, |this| {
                            this.child(
                                div()
                                    .debug_selector({
                                        let id = pill_id(&card.machine_id);
                                        move || id
                                    })
                                    .flex_none()
                                    .px(px(6.))
                                    .py(px(1.))
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(rgb(0x777777).opacity(0.3))
                                    .text_xs()
                                    .text_color(muted)
                                    .child(THIS_COMPUTER),
                            )
                        }),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .gap(px(10.))
                        .items_center()
                        .child(div().text_xs().text_color(muted).child(RELAY_LABEL))
                        .child(
                            div()
                                .debug_selector({
                                    let id = switch_id.clone();
                                    move || id
                                })
                                .child(
                                    Switch::new(ElementId::Name(switch_id.into()))
                                        .checked(card.relay_on)
                                        .accessibility_label(RELAY_LABEL)
                                        .disabled(card.switching)
                                        .on_click(move |next, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.set_computer_relay(
                                                    machine_id.clone(),
                                                    *next,
                                                    cx,
                                                )
                                            });
                                        }),
                                ),
                        ),
                ),
        )
        .child(
            div()
                .id(ElementId::Name(status.clone().into()))
                .debug_selector(move || says)
                .text_sm()
                .when(card.state != RelayState::Relaying, |this| {
                    this.text_color(muted)
                })
                .child(card.state.words()),
        )
        .when_some(detail, |this, detail| {
            this.child(
                div()
                    .id(RELAY_DETAIL)
                    .text_xs()
                    .text_color(if detail.trouble { theme.danger } else { muted })
                    .child(detail.words),
            )
        })
        .when_some(note, |this, (id, note)| {
            this.child(
                div()
                    .id(ElementId::Name(id.into()))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(note),
            )
        })
        .when_some(fields, |this, fields| this.child(fields))
}

/// This computer's opencodex address and key, and Save, on its card. The fields need a window, so
/// the view is made on the first render of the tab, as Settings → Logins is.
pub struct OpencodexFields {
    state: Entity<AppState>,
    /// The relay's two fields: opencodex's address on this computer, and its key.
    relay_address: Entity<InputState>,
    relay_key: Entity<InputState>,
}

impl OpencodexFields {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let relay_address = cx.new(|cx| InputState::new(window, cx).placeholder(DEFAULT_PROXY_URL));
        let relay_key = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(RELAY_KEY_PLACEHOLDER)
                .masked(true)
        });
        // The fields follow the state whether the card is drawn or not: a key the state drops
        // when the page is left, or at a sign-out, leaves the field too, then and not at the
        // next time the card happens to be drawn.
        cx.observe_in(&state, window, |this, _, window, cx| {
            this.sync_inputs(window, cx);
            cx.notify();
        })
        .detach();
        // What is typed is the form: the state keeps the copy Save keeps and a driver writes. A
        // field takes typing only while the card draws it editable, as Remove key takes a click
        // only then. Keys that reach one the card has just made read-only, before it is drawn
        // so, change no draft, and the field is put back to what the card holds rather than left
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
        let mut page = Self {
            state,
            relay_address,
            relay_key,
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

fn labelled(label: &'static str, muted: Hsla, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap(px(4.))
        .child(div().text_xs().text_color(muted).child(label))
        .child(control)
}

impl Render for OpencodexFields {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let app = self.state.clone();
        // Read in place: the settings can hold a typed key, and nothing here copies it.
        let state = self.state.read(cx);
        let settings = &state.reply_source;
        let error = error_line(settings).map(str::to_string);
        let trouble = error_is_trouble(settings);
        let block = v_flex().w_full().gap(px(12.));
        if settings.kept_source().is_none() {
            let line = unavailable_line(settings).unwrap_or(ASKING).to_string();
            let failed = matches!(settings.kept, Some(ReplySourceRead::Unavailable(_)));
            return block.child(
                div()
                    .id(UNAVAILABLE)
                    .text_sm()
                    .text_color(if failed { theme.danger } else { muted })
                    .child(line),
            );
        }
        // A server from before the relay has nothing for this computer to relay to, and the
        // fields would change nothing: the card says so in their place, and there is nothing to
        // save.
        if !settings.knows_relay() {
            return block
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
                });
        }
        let live = settings.relay_editable();
        let has_key = state.relay_mac.has_key;
        let removing = settings.relay_remove_key;
        let key_live = settings.relay_key_editable();
        let retype = settings.relay_retype_key;
        let hint = state.reply_source_hint();
        let can_save = state.reply_source_can_save();
        let unsaved = settings.is_unsaved();
        // opencodex's address and key, and Save for them: there is no radio and nothing of the plan
        // on the server's own machine. Where a Bot's replies go is picked with its model on its
        // card, and the account's setting stays as the server keeps it.
        block
            .child(labelled(
                RELAY_ADDR_LABEL,
                muted,
                div()
                    .id(RELAY_ADDR)
                    .debug_selector(|| RELAY_ADDR.into())
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
                            .debug_selector(|| RELAY_KEY.into())
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
                                    div().debug_selector(|| RELAY_KEY_REMOVE.into()).child(
                                        Button::new(RELAY_KEY_REMOVE)
                                            .label(if removing {
                                                KEEP_KEY_LABEL
                                            } else {
                                                REMOVE_KEY_LABEL
                                            })
                                            .ghost()
                                            .small()
                                            .disabled(!live)
                                            .on_click({
                                                let app = app.clone();
                                                move |_, _, cx| {
                                                    app.update(cx, |state, cx| {
                                                        state.toggle_remove_relay_key(cx)
                                                    });
                                                }
                                            }),
                                    ),
                                ),
                        )
                    }),
            ))
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
                        div().debug_selector(|| SAVE.into()).child(
                            Button::new(SAVE)
                                .label(SAVE_LABEL)
                                .small()
                                .disabled(!can_save)
                                .on_click(move |_, _, cx| {
                                    app.update(cx, |state, cx| state.save_reply_source(cx));
                                }),
                        ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    // Item by item rather than a glob: `use super::*` would drag in gpui_kit's own `test`.
    use super::{
        ASKING, CONNECTING, ERROR, HINT, KEEP_KEY_LABEL, NO_COMPUTERS, NOT_RELAYING, NOT_SAVED,
        ON_BUT_ASLEEP, RELAY_ADDR, RELAY_ADDR_LABEL, RELAY_DETAIL, RELAY_KEY, RELAY_KEY_GOES,
        RELAY_KEY_KEPT, RELAY_KEY_LABEL, RELAY_KEY_PLACEHOLDER, RELAY_KEY_REMOVE, RELAY_LABEL,
        RELAY_NOT_ON_SERVER, RELAY_RETYPE_KEY, RELAY_UNAVAILABLE, RELAYING, REMOVE_KEY_LABEL,
        RelayState, SAVE, SAVE_LABEL, SECTION_INTRO, SECTION_TITLE, TAKE_BACK, THIS_COMPUTER,
        UNAVAILABLE, card_id, error_id, relay_switch_id, status_id,
    };

    /// The words the owner approved for a card, exactly: the section, the pill, the switch, and the
    /// three lines that say where a computer's relay stands, with the state a driver reads each as.
    #[test]
    fn the_cards_say_the_approved_words() {
        assert_eq!(SECTION_TITLE, "Your computers");
        assert_eq!(THIS_COMPUTER, "This computer");
        assert_eq!(RELAY_LABEL, "Relay your plan");
        let said: Vec<(&str, &str)> = [
            RelayState::Relaying,
            RelayState::NotRelaying,
            RelayState::OnButAsleep,
        ]
        .map(|state| (state.words(), state.word()))
        .into();
        assert_eq!(
            said,
            [
                ("Relaying", "relaying"),
                ("Not relaying", "not-relaying"),
                ("On, but asleep: it can't answer right now", "asleep"),
            ]
        );
    }

    /// The ids a driver finds a card's parts by: the card, its switch, its status and its error by
    /// the server's machine id, and what Settings → Relay's page held for this computer under the
    /// ids it always had.
    #[test]
    fn the_cards_ids_are_the_stable_ones() {
        assert_eq!(card_id("mac_1"), "settings-computer-mac_1");
        assert_eq!(relay_switch_id("mac_1"), "settings-computer-mac_1-relay");
        assert_eq!(status_id("mac_1"), "settings-computer-mac_1-status");
        assert_eq!(error_id("mac_1"), "settings-computer-mac_1-error");
        assert_eq!(
            [
                RELAY_ADDR,
                RELAY_KEY,
                RELAY_KEY_REMOVE,
                RELAY_DETAIL,
                RELAY_UNAVAILABLE,
                SAVE,
                HINT,
                ERROR,
                UNAVAILABLE
            ],
            [
                "settings-relay-addr",
                "settings-relay-key",
                "settings-relay-key-remove",
                "settings-relay-detail",
                "settings-relay-unavailable",
                "settings-reply-source-save",
                "settings-reply-source-hint",
                "settings-reply-source-error",
                "settings-reply-source-unavailable",
            ]
        );
    }

    /// Every word a card says, and every sentence of the relay's it shows, says "computer" where it
    /// once said "Mac": the relay is the app on the person's computer, which need not be a Mac.
    /// The relay's own sentences are among them, as its errors are drawn on this computer's card
    /// under its status.
    #[test]
    fn the_cards_and_the_relays_sentences_say_computer_and_never_mac() {
        use crate::opengrok::{
            SERVER_QUIET, SERVER_UNREACHED, SERVER_WITHOUT_RELAY, TOKEN_REFUSED,
        };
        let words = [
            SECTION_TITLE,
            SECTION_INTRO,
            NO_COMPUTERS,
            THIS_COMPUTER,
            RELAY_LABEL,
            RELAYING,
            NOT_RELAYING,
            ON_BUT_ASLEEP,
            CONNECTING,
            TAKE_BACK,
            ASKING,
            RELAY_NOT_ON_SERVER,
            RELAY_ADDR_LABEL,
            RELAY_KEY_LABEL,
            RELAY_KEY_PLACEHOLDER,
            RELAY_KEY_KEPT,
            RELAY_KEY_GOES,
            RELAY_RETYPE_KEY,
            REMOVE_KEY_LABEL,
            KEEP_KEY_LABEL,
            SAVE_LABEL,
            NOT_SAVED,
            crate::state::RELAY_ADDRESS_NOT_HERE,
            crate::state::COMPUTER_RELAY_NOT_ON_SERVER,
            TOKEN_REFUSED,
            SERVER_QUIET,
            SERVER_UNREACHED,
            SERVER_WITHOUT_RELAY,
        ];
        for said in words {
            assert!(!said.contains("Mac"), "{said:?}");
        }
        for said in [
            NO_COMPUTERS,
            RELAY_KEY_LABEL,
            RELAY_KEY_KEPT,
            RELAY_RETYPE_KEY,
            TAKE_BACK,
            RELAY_NOT_ON_SERVER,
            TOKEN_REFUSED,
        ] {
            assert!(said.contains("computer"), "{said:?}");
        }
    }
}
