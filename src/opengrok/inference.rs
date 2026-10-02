//! Where a reply's model calls are paid from: the server's paid keys, or the person's own
//! subscription.
//!
//! The app never calls a model, whichever it is. The fork is in opengrok-server's model door:
//! the harness, the tools and the journal stay on the server either way, and when the source is
//! `local_proxy` it is the server that talks to opencodex, the proxy that holds the person's
//! sign-in: over the server's own loopback, or through the person's computer (the relay, below).
//! The account's loopback half (the proxy's address and key on the server's machine) is the
//! server's to keep, and this app no longer sets it up or sends it: Settings → Relay is the relay
//! alone. What lives here is the cockpit's half of the rest: the account's setting (`GET` and
//! `PUT /account/inference-source`), the door one turn names in
//! `forwardedProps.inferenceSource`, and the CUSTOM frame (`opengrok.inferenceSource`) that says
//! which door a reply came through.
//!
//! Every shape here is transcribed from the inference-source contract agreed with
//! open-ai-gateway and opengrok-server (2026-09-30), and checked against the server's half as
//! built in opengrok-server #294: the routes in `crates/opengrok-server/src/inference.rs`, what a
//! Save does and what a read answers (`apply`, `described`, `loopback_base`) in
//! `crates/opengrok-harness/src/local_proxy.rs`, and the door's words and the models a
//! subscription may answer in `crates/opengrok-core/src/inference.rs`. The conformance ledger
//! reads the two routes and the CUSTOM frame against the server's recording, vendored in
//! `fixtures/wire/` from opengrok-server #334, on main 8e7387f (recorded at its branch commit
//! 426fa0d). These shapes are as they were at main cad36fd (#303, after #298): #306 changed no
//! crate, #304 puts a Bot's own door between a turn's and the account's (`route` in
//! `crates/opengrok-harness/src/local_proxy.rs`), #308 lets a retry of a queued send's reply
//! name its own door over the one the send was queued with (`consume_for_turn` in
//! `crates/opengrok-server/src/agui/pending.rs`), and #322 adds the account's default for new
//! Bots (`newBotDefault`, in `described` and `apply`). #325 and #316 change where a Bot's message
//! and a routine's firing are answered (`for_message` there, and `routine_route` in the server's
//! `crates/opengrok-server/src/autonomy/mod.rs`), and no shape read here.
//!
//! The relay (opengrok-server #292, built in #298, whose recording holds its words) lifts the
//! one-machine limit: the server sends a turn's model calls down a stream to the person's
//! computer, whose background helper asks its own opencodex and streams the answer back
//! (`super::relay`). The window still never calls a model. On the wire it is a second word
//! beside the kind, the way the plan is reached ([`Via`]): the account keeps one, a turn and a
//! queued send may name one ([`TurnSource`]), the CUSTOM frame says which one answered, and the
//! account's setting says where the relay stands ([`RelayRead`]).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The CUSTOM `name` the server sends right after `RUN_STARTED`, with `value {"kind", "model"}`:
/// which door this run's model calls go through, and the model. It is journaled with the run, so
/// a thread's replay carries it as the live stream did.
pub const INFERENCE_SOURCE_CUSTOM: &str = "opengrok.inferenceSource";

/// Where opencodex listens unless the person says otherwise: on this computer, for the relay.
pub const DEFAULT_PROXY_URL: &str = "http://127.0.0.1:8080";

/// Whether an address is the machine it is dialled from, as the server reads one: `127.0.0.0/8`,
/// `[::1]` or the name `localhost`, with or without a port (`loopback_base` in
/// `crates/opengrok-harness/src/local_proxy.rs`). The relay asks it of opencodex's address: it
/// calls only an opencodex on this computer, as the server does on its own machine.
pub fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Which door a turn's model calls go through, in the server's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceKind {
    /// The server's paid keys, through open-ai-gateway.
    Gateway,
    /// The person's own subscription, through opencodex on the same machine as the server.
    LocalProxy,
}

impl InferenceKind {
    /// Both, in the order the settings offer them.
    pub const ALL: [Self; 2] = [Self::Gateway, Self::LocalProxy];

    /// The word on the wire: `kind` in the account's setting and in the CUSTOM frame, `source`
    /// on a `/models` entry, and `forwardedProps.inferenceSource` on a turn.
    pub fn word(self) -> &'static str {
        match self {
            Self::Gateway => "gateway",
            Self::LocalProxy => "local_proxy",
        }
    }

    /// The kind a wire word names, or `None` for a word this app has not heard of: a door it
    /// cannot name is not claimed to be either of the two it can.
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.word() == word)
    }
}

/// The way the person's own plan is reached, in the server's words (opengrok-server #292: `Via`
/// in `crates/opengrok-core/src/inference.rs`, server main cad36fd (#303, after #298), pin
/// 47a5d6b): `loopback`, the server calling opencodex on its own machine as it
/// always has; `mac`, the server sending each model call down the relay stream to the person's
/// computer, which asks its own opencodex. The contract's third word, `helper`, the server refuses
/// until its #293, so it is offered nowhere here and reads like any word this app has not heard
/// of: a way it cannot name, never mistaken for one of these two.
///
/// The relay is the app on the person's computer, which need not be a Mac, and the server is being
/// asked to take `computer` as a new name for `mac`. No server sends or takes it yet: this app
/// reads it as [`Via::Mac`] should it ever arrive, and sends `mac`, the word every server with the
/// relay reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    Loopback,
    #[serde(alias = "computer")]
    Mac,
}

impl Via {
    pub const ALL: [Self; 2] = [Self::Loopback, Self::Mac];

    /// The word on the wire: `via` in the account's setting, on a turn's door, in the CUSTOM
    /// frame and on a `/models` entry. The relay's is `mac`, whatever this app calls it.
    pub fn word(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Mac => "mac",
        }
    }

    /// The way a wire word names, or `None` for one this app has not heard of. `computer`, the
    /// name the server is being asked to take for `mac`, reads as it.
    pub fn from_word(word: &str) -> Option<Self> {
        match word {
            "computer" => Some(Self::Mac),
            _ => Self::ALL.into_iter().find(|via| via.word() == word),
        }
    }
}

/// The door one turn names in `forwardedProps.inferenceSource`, and that a queued send's row
/// keeps as `inferenceSource`: the kind, and for the person's plan the way to it when this app
/// names one (opengrok-server #292: `TurnSource` in `crates/opengrok-core/src/inference.rs`,
/// server main cad36fd (#303, after #298), pin 47a5d6b).
///
/// It goes as every turn went before the relay, the kind's bare word, when it names no way, and
/// as `{"kind", "via"}` when it does. A server from before the relay takes only the bare word and
/// refuses anything else, so a way is named only to a server that said it knows them (its
/// setting carries `relay`); the plan on its own machine is then the only way there is. The
/// server's own way stands when none is named, which is also how a way this app cannot name is
/// left to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TurnSource {
    pub kind: InferenceKind,
    /// `None` names the kind alone. Never set on the gateway's door, which has no way.
    pub via: Option<Via>,
}

impl TurnSource {
    /// The server's paid keys.
    pub const GATEWAY: Self = Self {
        kind: InferenceKind::Gateway,
        via: None,
    };

    /// The person's own plan, reached `via`, or the account's own way when it is `None`.
    pub fn plan(via: Option<Via>) -> Self {
        Self {
            kind: InferenceKind::LocalProxy,
            via,
        }
    }

    /// As a turn and a queued send's row carry it: the kind's bare word with no way named, and
    /// `{"kind", "via"}` with one.
    pub fn to_value(self) -> Value {
        match self.via.filter(|_| self.kind == InferenceKind::LocalProxy) {
            None => Value::String(self.kind.word().to_string()),
            Some(via) => json!({ "kind": self.kind.word(), "via": via.word() }),
        }
    }

    /// Either shape, as a row brings it back. A kind this app has not heard of, or a way it
    /// cannot name, is `None`: the turn that fires such a row names nothing either, and the server
    /// goes by the row's own word, rather than a door being guessed for it.
    pub fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::String(word) => InferenceKind::from_word(word).map(Self::from),
            Value::Object(door) => {
                let kind = InferenceKind::from_word(door.get("kind")?.as_str()?)?;
                if kind == InferenceKind::Gateway {
                    return Some(Self::GATEWAY);
                }
                let via = match door.get("via") {
                    None | Some(Value::Null) => None,
                    Some(word) => Some(Via::from_word(word.as_str()?)?),
                };
                Some(Self::plan(via))
            }
            _ => None,
        }
    }
}

impl From<InferenceKind> for TurnSource {
    fn from(kind: InferenceKind) -> Self {
        Self { kind, via: None }
    }
}

impl Serialize for TurnSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_value().serialize(serializer)
    }
}

/// The account's reply source, as `GET /account/inference-source` answers and as `PUT` answers
/// with once it has kept a change: `{"kind", "baseUrl", "localModel", "healthy", "hasApiKey"}`,
/// with `baseUrl` and `localModel` `null` while none is set; and from a server with the Mac relay,
/// `"via"` and `"relay"` beside them.
///
/// `kind` and the two flags are always there, so a body without one is refused rather than read
/// as `false`: "not running" and "no key" are things the person acts on, and a missing field is
/// neither. A `kind` this app has not heard of is refused too, and the page says it could not be
/// read rather than showing one of its two choices as the account's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceSource {
    pub kind: InferenceKind,
    /// Where the server reaches opencodex.
    #[serde(default)]
    pub base_url: Option<String>,
    /// The model a turn on the person's own subscription runs on.
    #[serde(default)]
    pub local_model: Option<String>,
    /// Whether opencodex answered at `baseUrl` when the server asked.
    pub healthy: bool,
    /// The server holds a key for the proxy. The key itself never comes back.
    pub has_api_key: bool,
    /// The account's own way to the person's plan, the word as sent: `loopback` until one is saved
    /// (opengrok-server #292: `described` in `crates/opengrok-harness/src/local_proxy.rs`, server
    /// main cad36fd (#303, after #298), pin 47a5d6b). Absent from a
    /// server before the relay, which has no other. Kept as the word and read through
    /// [`Self::default_via`], so a way this app cannot name never fails the read.
    #[serde(default)]
    pub via: Option<String>,
    /// Where the Mac relay stands, from a server that has one, which sends it on every read, nulls
    /// and all when no Mac holds it (the same PR); absent from a server before it, which is how
    /// this app knows not to offer it.
    #[serde(default)]
    pub relay: Option<RelayRead>,
    /// Default for new Bots, from a server that keeps one, which sends the key on every read,
    /// `null` until the person sets one (opengrok-server #322, on main c0bb6ae: `described` in
    /// `crates/opengrok-harness/src/local_proxy.rs`). `None` is the key left out, a server before
    /// it, which keeps no such default; `Some(None)` is `null`, none set.
    #[serde(default, deserialize_with = "keyed")]
    pub new_bot_default: Option<Option<NewBotDefault>>,
    /// Whether the relay is switched on for the account, from a server that keeps it, which sends
    /// it on every read, `true` until it is told otherwise (opengrok-server relay-off fallback
    /// contract, agreed 2026-10-03, not yet built). `None` is the key left out, a server before
    /// it, which is never sent one: the relay switch tells only a server that keeps it.
    #[serde(default)]
    pub relay_enabled: Option<bool>,
    /// The Relay-off fallback, from a server that keeps one, which sends the key on every read,
    /// `null` until the person sets one (opengrok-server relay-off fallback contract, agreed
    /// 2026-10-03, not yet built). `None` is the key left out, a server before it, which keeps no
    /// such fallback; `Some(None)` is `null`, none set.
    #[serde(default, deserialize_with = "keyed")]
    pub plan_fallback: Option<Option<PlanFallback>>,
}

/// A key that is there, `null` or not: `Some(None)` for `null` and `Some(Some(_))` for a value,
/// so the field's default, `None`, is the key left out.
pub(super) fn keyed<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// A person's default for new Bots, as opengrok-server #322 (on main c0bb6ae) writes it:
/// `NewBotDefault` in `crates/opengrok-core/src/inference.rs`,
/// `{"source", "model", "effort"}`. A Bot hired with no model of its own, and none from its
/// template, is born on it whole, its door, the model it is pinned to and how hard it thinks
/// written onto the Bot at its hire, so a changed default moves only the Bots hired after it
/// (`hire_model` in `crates/opengrok-server/src/inference.rs`). Fast is the model's `--fast` id,
/// not a field.
///
/// The server holds a `PUT` of it to what its parts are held to elsewhere, and refuses the whole
/// body with a 400 and `{"error"}` in their words (`NewBotDefault::named`): a `source` that is not
/// one of its two words (`newBotDefault.source must be "gateway" or "local_proxy"`), an effort
/// that is not a Bot's (`newBotDefault.effort must be one of inherit, none, low, medium, high,
/// xhigh, max`), and a model refused as a hire's is on the gateway (`newBotDefault.model: a
/// coworker needs a model to think with`) or as a plan's is on the person's plan
/// (`newBotDefault.model: ` and the allowlist's sentence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBotDefault {
    /// The door a new Bot is born on: `gateway` or `local_proxy`.
    pub source: InferenceKind,
    /// The id it is pinned to.
    pub model: String,
    /// One of a Bot's effort words (`types::EFFORT_WORDS`); left out or `null`, `inherit`.
    #[serde(default = "effort_inherit", deserialize_with = "effort_word")]
    pub effort: String,
}

/// What a Bot on the person's plan answers with while the relay is off, the account's Relay-off
/// fallback: `planFallback: {model, effort} | null` on `GET` and `PUT
/// /account/inference-source` (opengrok-server relay-off fallback contract, agreed 2026-10-03,
/// not yet built). Its model is on the server's paid keys, held to what a Bot's gateway pin is
/// held to, and its effort is one of a Bot's words. Fast is the model's `--fast` id, as
/// everywhere, not a field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanFallback {
    /// The gateway id a Bot on the plan answers on while the relay is off.
    pub model: String,
    /// One of a Bot's effort words (`types::EFFORT_WORDS`); left out or `null`, `inherit`.
    #[serde(default = "effort_inherit", deserialize_with = "effort_word")]
    pub effort: String,
}

fn effort_inherit() -> String {
    super::EFFORT_INHERIT.to_string()
}

/// An effort as a default for new Bots carries it: `null` is `inherit`, as the server reads it.
fn effort_word<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_else(effort_inherit))
}

impl InferenceSource {
    /// The server knows the Mac relay: it said where the relay stands, which a server from before
    /// #292 never does. Only then is a way to the plan named to it, the Mac offered as one, and
    /// this Mac's switch live.
    pub fn knows_relay(&self) -> bool {
        self.relay.is_some()
    }

    /// The account's own way to the person's plan: the word the server keeps, `loopback` from a
    /// server that keeps none, and `None` for a word this app has not heard of.
    pub fn default_via(&self) -> Option<Via> {
        match self.via.as_deref() {
            None => Some(Via::Loopback),
            Some(word) => Via::from_word(word),
        }
    }

    /// The account's own door as a turn names it: its kind, and for the plan its way, named only
    /// to a server that knows the relay and only when this app can name it.
    pub fn door(&self) -> TurnSource {
        match self.kind {
            InferenceKind::Gateway => TurnSource::GATEWAY,
            InferenceKind::LocalProxy => {
                TurnSource::plan(self.default_via().filter(|_| self.knows_relay()))
            }
        }
    }

    /// The model a turn through a Mac runs on, as the server keeps it.
    pub fn relay_model(&self) -> Option<&str> {
        self.relay.as_ref()?.local_model.as_deref()
    }
}

/// Where the Mac relay stands, as the account's setting says it: `{"connected", "machineId",
/// "machineLabel", "localModel"}` (opengrok-server #292: `described` in
/// `crates/opengrok-harness/src/local_proxy.rs`, server main cad36fd (#303, after #298), pin
/// 47a5d6b). The Mac answering is the account's enrolled machine that most
/// recently opened the relay stream; `connected` is whether one holds it now. `connected` is
/// always there, so a relay without it is refused rather than read as no Mac.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayRead {
    pub connected: bool,
    /// The Mac holding the stream, by its local-exec machine id.
    #[serde(default)]
    pub machine_id: Option<String>,
    /// That Mac's name, as it enrolled.
    #[serde(default)]
    pub machine_label: Option<String>,
    /// The model a turn through a Mac runs on.
    #[serde(default)]
    pub local_model: Option<String>,
}

/// A `PUT /account/inference-source` body, as this app sends one: `{"kind", "via"?,
/// "relayEnabled"?, "newBotDefault"?, "planFallback"?}`.
///
/// The server takes no `PUT` without a kind (`apply` in
/// `crates/opengrok-harness/src/local_proxy.rs`), and this app switches no kind, so `kind` is the
/// one the server keeps, sent back as it is. Every other field is there only when the app changes
/// it, and the server reads a field three ways: absent keeps what it has, `null` clears it, and a
/// value replaces it.
///
/// There is no field here for the plan on the server's own machine (`baseUrl`, `apiKey`,
/// `localModel`) nor for the relay's own model (`relay.localModel`): the app no longer sets up
/// either, and a field it cannot write is a field no `PUT` of it carries. Absent, the server keeps
/// them as they are, and the Bots that go that way keep going as it decides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceSourceUpdate {
    pub kind: InferenceKind,
    /// The account's way to the plan, sent only when the app changes it, and only to a server
    /// that knows the relay (opengrok-server #292: `apply` in
    /// `crates/opengrok-harness/src/local_proxy.rs`, server main cad36fd (#303, after #298), pin
    /// 47a5d6b), which keeps it as the account's way whatever the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Via>,
    /// Whether the relay is switched on, sent by the relay switch in the same body as what else it
    /// changes, and only to a server whose read carries the key (opengrok-server relay-off fallback
    /// contract, agreed 2026-10-03, not yet built): `true` with `via: "mac"` as it goes on, and
    /// `false` with no way as it goes off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_enabled: Option<bool>,
    /// Default for new Bots, sent whole when the app changes it, only to a server whose read
    /// carries the key, and `null` to take it away (opengrok-server #322, on main c0bb6ae:
    /// `apply` in `crates/opengrok-harness/src/local_proxy.rs`): absent keeps it, `null` clears
    /// it, and a value replaces it whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_bot_default: Option<Option<NewBotDefault>>,
    /// The Relay-off fallback, sent whole when the app changes it, only to a server whose read
    /// carries the key, and `null` to take it away (opengrok-server relay-off fallback contract,
    /// agreed 2026-10-03, not yet built): absent keeps it, `null` clears it, and a value replaces
    /// it whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_fallback: Option<Option<PlanFallback>>,
}

/// Why a reply came through the server's paid keys and not the door its Bot asks for, as the
/// run's `opengrok.inferenceSource` CUSTOM says in `fallbackFor` (opengrok-server relay-off
/// fallback contract, agreed 2026-10-03, not yet built). The one reason this app knows is
/// `relay_disabled`: a Bot on the person's plan answered by the account's Relay-off fallback,
/// because the relay is off. A word this app has not heard of is no reason it can name, and the
/// badge says only whose keys paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackFor {
    RelayDisabled,
}

impl FallbackFor {
    /// The reason a wire word names, or `None` for one this app has not heard of.
    pub fn from_word(word: &str) -> Option<Self> {
        (word == "relay_disabled").then_some(Self::RelayDisabled)
    }
}

/// Which door one reply came through, as the run's `opengrok.inferenceSource` CUSTOM says:
/// `value {"kind", "model"}`, and from a server with the Mac relay `{"kind", "via", "model"}`,
/// and `fallbackFor` beside them on a reply the Relay-off fallback answered. It is what the
/// reply's badge shows, live and when the thread is read back, and what the reply's row keeps on
/// disk (the same JSON column, so a `via` or a `fallbackFor` needs no migration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplySource {
    pub kind: InferenceKind,
    /// The way the person's plan was reached, when the frame named one this app knows: `mac` is
    /// a reply the person's Mac answered (opengrok-server #292, server main cad36fd (#303, after
    /// #298), pin 47a5d6b). Never set on the gateway's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<Via>,
    /// The model that answered, when the server named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Why the server's paid keys answered, when the frame named a reason this app knows
    /// ([`FallbackFor`]). Only ever on the gateway's.
    #[serde(
        default,
        rename = "fallbackFor",
        skip_serializing_if = "Option::is_none"
    )]
    pub fallback_for: Option<FallbackFor>,
}

impl ReplySource {
    /// The frame's source, if it is an `opengrok.inferenceSource` CUSTOM naming a door this app
    /// knows. A frame with a kind it does not know says nothing it could put on a badge.
    pub fn from_event(event: &Value) -> Option<Self> {
        if event.get("type").and_then(Value::as_str) != Some("CUSTOM")
            || event.get("name").and_then(Value::as_str) != Some(INFERENCE_SOURCE_CUSTOM)
        {
            return None;
        }
        Self::from_value(event.get("value")?)
    }

    /// `{"kind", "via"?, "model", "fallbackFor"?}`, as the frame's `value` and a saved row both
    /// hold it. A way this app cannot name, or a reason, leaves the badge saying only whose it
    /// was: each is read only where it is a word this app knows, on the door it can be on.
    pub fn from_value(value: &Value) -> Option<Self> {
        let kind = InferenceKind::from_word(value.get("kind")?.as_str()?)?;
        let via = value
            .get("via")
            .and_then(Value::as_str)
            .and_then(Via::from_word)
            .filter(|_| kind == InferenceKind::LocalProxy);
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string);
        let fallback_for = value
            .get("fallbackFor")
            .and_then(Value::as_str)
            .and_then(FallbackFor::from_word)
            .filter(|_| kind == InferenceKind::Gateway);
        Some(Self {
            kind,
            via,
            model,
            fallback_for,
        })
    }

    /// The server's paid keys answered a Bot on the person's plan because the relay is off.
    pub fn relay_off(&self) -> bool {
        self.fallback_for == Some(FallbackFor::RelayDisabled)
    }

    /// As a reply's row keeps it in sqlite.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    /// A row's saved source. One this build cannot read is no source, not a failed thread.
    pub fn from_json(raw: &str) -> Option<Self> {
        serde_json::from_str::<Value>(raw)
            .ok()
            .as_ref()
            .and_then(Self::from_value)
    }
}

/// Why the person's plan could not answer a turn, as its `RUN_ERROR` says beside its sentence in
/// `code`: the Mac relay's three (opengrok-server #292: `ModelError::Relay` in
/// `crates/opengrok-harness/src/relay.rs`, server main cad36fd (#303, after #298), pin 47a5d6b),
/// and `plan_unavailable` (opengrok-server main d6f640e (#307, after #304), pin bf99845:
/// `ModelError::PlanUnavailable` in `crates/opengrok-harness/src/model.rs`). The sentence is what
/// the person reads; the code is what offers the turn again on the server's keys, which could
/// answer it. A refusal with no code is none of these, and its sentence stands alone: a Mac
/// already carrying all the calls one Mac may at once, a reply source that could not be read, or
/// a proxy key that could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunErrorCode {
    /// No Mac held the relay.
    RelayOffline,
    /// The Mac said nothing for the server's sixty seconds, before the first byte or between two.
    RelayTimeout,
    /// The Mac answered with a failure.
    RelayFailed,
    /// The person's own setting left their plan nothing to answer with: a teammate with no proxy
    /// on a shared Bot on the plan, or a proxy turn with no address or no model stored.
    PlanUnavailable,
}

impl RunErrorCode {
    pub const ALL: [Self; 4] = [
        Self::RelayOffline,
        Self::RelayTimeout,
        Self::RelayFailed,
        Self::PlanUnavailable,
    ];

    /// The code word on the frame.
    pub fn word(self) -> &'static str {
        match self {
            Self::RelayOffline => "relay_offline",
            Self::RelayTimeout => "relay_timeout",
            Self::RelayFailed => "relay_failed",
            Self::PlanUnavailable => "plan_unavailable",
        }
    }

    /// The code a run ended with, or `None` for any other code, or none.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.word() == code)
    }
}

/// `heldFor` on a queued send's row while the server holds it for the person's Mac: the turn it
/// names goes through the Mac and no Mac holds the relay (opengrok-server #292: `HELD_FOR` in
/// `crates/opengrok-server/src/agui/pending.rs`, server main cad36fd (#303, after #298), pin
/// 47a5d6b). Absent otherwise. The server sends such a send itself, oldest
/// first, when a Mac opens the relay, so this app never fires one.
pub const HELD_FOR_RELAY_OFFLINE: &str = "relay_offline";

/// Whether "My subscription" may be pointed at a model, by the server's own rule, so the picker
/// never offers what a Save would be refused for, even should a list ever carry one (a list held
/// from before, or a server that tags a row `local_proxy` without asking): `subscription_model`
/// in opengrok-server's `crates/opengrok-core/src/inference.rs`, anchored since #296 and unchanged
/// in the vendored recording (#334, on main 8e7387f), where a Bot's own plan
/// model is held to it too, as it has been since #304, and so is a default for new Bots on the
/// plan, since #322.
///
/// An allowlist, not a denylist: an id it does not recognise is refused, so a provider nobody
/// has looked at is not offered by being new. With an `openai/` or `xai/` prefix and the `--fast`
/// tier taken off, the id STARTS with `gpt-`, `o1`, `o3`, `o4` or `codex` (OpenAI's) or `grok-`
/// (xAI's), and a prefix names the provider its model is from; an id that only contains one of
/// those words (`my-codex-thing`, `notgrok-1`) is nobody's the server knows. Claude and Gemini
/// are refused wherever their names, or their makers', appear in an id: Anthropic's and Google's
/// terms forbid routing a consumer subscription through a third-party app.
pub fn is_subscription_model(id: &str) -> bool {
    let id = id.trim().to_ascii_lowercase();
    if id.is_empty()
        || ["anthropic", "claude", "google", "gemini"]
            .iter()
            .any(|provider| id.contains(provider))
    {
        return false;
    }
    let (provider, core) = id
        .split_once('/')
        .map_or((None, id.as_str()), |(provider, core)| {
            (Some(provider), core)
        });
    let core = core.strip_suffix("--fast").unwrap_or(core);
    let openai = ["gpt-", "o1", "o3", "o4", "codex"]
        .iter()
        .any(|start| core.starts_with(start));
    let xai = core.starts_with("grok-");
    let recognised = match provider {
        None => openai || xai,
        Some("openai") => openai,
        Some("xai") => xai,
        Some(_) => false,
    };
    recognised
        && id.len() <= 128
        && !core.contains('/')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._:/".contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The words are the contract's, both ways, and a word this app has not heard of names
    /// neither door.
    #[test]
    fn a_kind_is_the_servers_word() {
        for kind in InferenceKind::ALL {
            assert_eq!(InferenceKind::from_word(kind.word()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(kind.word()));
        }
        assert_eq!(InferenceKind::Gateway.word(), "gateway");
        assert_eq!(InferenceKind::LocalProxy.word(), "local_proxy");
        assert_eq!(InferenceKind::from_word("local-proxy"), None);
        assert_eq!(InferenceKind::from_word(""), None);
    }

    /// The account's setting reads as the contract writes it, the two nullable fields either
    /// way, and a body without a flag is not read as that flag being false.
    #[test]
    fn the_accounts_setting_reads_as_sent() {
        let read: InferenceSource = serde_json::from_value(json!({
            "kind": "local_proxy",
            "baseUrl": "http://127.0.0.1:8080",
            "localModel": "gpt-5-codex",
            "healthy": true,
            "hasApiKey": false
        }))
        .unwrap();
        assert_eq!(
            read,
            InferenceSource {
                kind: InferenceKind::LocalProxy,
                base_url: Some("http://127.0.0.1:8080".into()),
                local_model: Some("gpt-5-codex".into()),
                healthy: true,
                has_api_key: false,
                via: None,
                relay: None,
                new_bot_default: None,
                relay_enabled: None,
                plan_fallback: None,
            }
        );
        let unset: InferenceSource = serde_json::from_value(json!({
            "kind": "gateway", "baseUrl": null, "localModel": null,
            "healthy": false, "hasApiKey": true
        }))
        .unwrap();
        assert_eq!((unset.base_url, unset.local_model), (None, None));
        for broken in [
            json!({"kind": "gateway", "baseUrl": null, "localModel": null, "hasApiKey": false}),
            json!({"kind": "gateway", "baseUrl": null, "localModel": null, "healthy": false}),
            json!({"baseUrl": null, "localModel": null, "healthy": false, "hasApiKey": false}),
            json!({"kind": "byok", "healthy": false, "hasApiKey": false}),
        ] {
            assert!(
                serde_json::from_value::<InferenceSource>(broken.clone()).is_err(),
                "{broken}"
            );
        }
    }

    /// A `PUT` body names the kind the server keeps and only what the app changed, and never
    /// carries the plan on the server's own machine or a model of the relay's: the app no longer
    /// sets up either, so the server keeps whatever it has of them.
    #[test]
    fn a_put_body_carries_no_loopback_field_and_no_relay_model() {
        for kind in InferenceKind::ALL {
            let bare = InferenceSourceUpdate {
                kind,
                via: None,
                new_bot_default: None,
                relay_enabled: None,
                plan_fallback: None,
            };
            assert_eq!(
                serde_json::to_value(&bare).unwrap(),
                json!({"kind": kind.word()}),
                "a field left alone is left out"
            );
        }
        let moved = InferenceSourceUpdate {
            kind: InferenceKind::Gateway,
            via: Some(Via::Mac),
            new_bot_default: None,
            relay_enabled: None,
            plan_fallback: None,
        };
        let body = serde_json::to_value(&moved).unwrap();
        assert_eq!(body, json!({"kind": "gateway", "via": "mac"}));
        for field in ["baseUrl", "apiKey", "localModel", "relay"] {
            assert!(body.get(field).is_none(), "{field} in {body}");
        }
    }

    /// An address is this computer when it is loopback as the server reads one: any of
    /// 127.0.0.0/8, `[::1]`, or `localhost`, with or without a port. A name that only starts with
    /// `localhost`, a LAN address or `0.0.0.0` is somewhere else.
    #[test]
    fn an_address_is_this_computer_only_at_a_loopback_address() {
        for here in [
            "http://127.0.0.1:1447",
            "http://127.5.6.7",
            "http://localhost:1447/",
            "https://LOCALHOST",
            "http://[::1]:1447",
        ] {
            assert!(is_loopback(&url::Url::parse(here).unwrap()), "{here}");
        }
        for elsewhere in [
            "http://192.168.1.5:1447",
            "https://opengrok.example.com",
            "http://localhost.example.com:1447",
            "http://0.0.0.0:1447",
            "http://[::2]:1447",
        ] {
            assert!(
                !is_loopback(&url::Url::parse(elsewhere).unwrap()),
                "{elsewhere}"
            );
        }
    }

    /// The CUSTOM frame reads as its kind and its model; a frame of another name, a kind this
    /// app has not heard of, or no value at all, is no source.
    #[test]
    fn the_custom_frame_names_the_door_and_the_model() {
        let frame = |value: Value| json!({"type": "CUSTOM", "name": INFERENCE_SOURCE_CUSTOM, "value": value});
        assert_eq!(
            ReplySource::from_event(&frame(
                json!({"kind": "local_proxy", "model": "gpt-5-codex"})
            )),
            Some(ReplySource {
                kind: InferenceKind::LocalProxy,
                model: Some("gpt-5-codex".into()),
                via: None,
                fallback_for: None,
            })
        );
        assert_eq!(
            ReplySource::from_event(&frame(json!({"kind": "gateway", "model": null}))),
            Some(ReplySource {
                kind: InferenceKind::Gateway,
                model: None,
                via: None,
                fallback_for: None,
            })
        );
        assert_eq!(
            ReplySource::from_event(&frame(json!({"kind": "gateway", "model": "  "}))),
            Some(ReplySource {
                kind: InferenceKind::Gateway,
                model: None,
                via: None,
                fallback_for: None,
            })
        );
        assert_eq!(
            ReplySource::from_event(&frame(json!({"kind": "byok", "model": "m"}))),
            None
        );
        assert_eq!(
            ReplySource::from_event(&json!({"type": "CUSTOM", "name": INFERENCE_SOURCE_CUSTOM})),
            None
        );
        assert_eq!(
            ReplySource::from_event(&json!({
                "type": "CUSTOM", "name": "run-timing", "value": {"kind": "gateway"}
            })),
            None
        );
    }

    /// What a row keeps reads back as what it was, and a row this build cannot read is no source.
    #[test]
    fn a_saved_source_reads_back() {
        let source = ReplySource {
            kind: InferenceKind::LocalProxy,
            model: Some("grok-4".into()),
            via: None,
            fallback_for: None,
        };
        assert_eq!(ReplySource::from_json(&source.to_json()), Some(source));
        let bare = ReplySource {
            kind: InferenceKind::Gateway,
            model: None,
            via: None,
            fallback_for: None,
        };
        assert_eq!(bare.to_json(), r#"{"kind":"gateway"}"#);
        assert_eq!(ReplySource::from_json(&bare.to_json()), Some(bare));
        assert_eq!(ReplySource::from_json("not json"), None);
        assert_eq!(ReplySource::from_json(r#"{"kind":"elsewhere"}"#), None);
        // A reply the person's Mac answered keeps its way on disk, and reads back with it.
        let mac = ReplySource {
            kind: InferenceKind::LocalProxy,
            via: Some(Via::Mac),
            model: Some("gpt-5-codex".into()),
            fallback_for: None,
        };
        assert_eq!(
            mac.to_json(),
            r#"{"kind":"local_proxy","via":"mac","model":"gpt-5-codex"}"#
        );
        assert_eq!(ReplySource::from_json(&mac.to_json()), Some(mac));
    }

    /// The ways are the contract's words, both ways; `helper`, which the server refuses until its
    /// #293, and any other word are no way this app names.
    #[test]
    fn a_way_to_the_plan_is_the_servers_word() {
        for via in Via::ALL {
            assert_eq!(Via::from_word(via.word()), Some(via));
            assert_eq!(serde_json::to_value(via).unwrap(), json!(via.word()));
        }
        assert_eq!((Via::Loopback.word(), Via::Mac.word()), ("loopback", "mac"));
        for unknown in ["helper", "Mac", "", "relay", "Computer"] {
            assert_eq!(Via::from_word(unknown), None, "{unknown:?}");
        }
    }

    /// The default for new Bots reads as opengrok-server #322 (on main c0bb6ae) writes it on the
    /// account's setting: left out by a server that keeps none, `null`
    /// while none is set, and whole once set, an effort left out or `null` read as `inherit`. A
    /// `PUT` names it only when the app changes it: whole, or `null` to take it away, with the
    /// kind the server keeps and nothing else.
    #[test]
    fn the_default_for_new_bots_reads_and_puts_as_the_server_writes_it() {
        let read = |default: Option<Value>| -> InferenceSource {
            let mut body = json!({
                "kind": "gateway", "via": "loopback", "baseUrl": null, "localModel": null,
                "healthy": false, "hasApiKey": false,
                "relay": {"connected": false, "machineId": null, "machineLabel": null,
                          "localModel": null}
            });
            if let Some(default) = default {
                body["newBotDefault"] = default;
            }
            serde_json::from_value(body).expect("a setting")
        };
        assert_eq!(read(None).new_bot_default, None, "a server that keeps none");
        assert_eq!(read(Some(Value::Null)).new_bot_default, Some(None));
        // As the branch's own recording of a pick words it
        // (`PUT__account_inference-source/200-a_person_names_the_model_new_bots_are_born_on`).
        let luna = NewBotDefault {
            source: InferenceKind::LocalProxy,
            model: "gpt-6-sol--fast".into(),
            effort: "high".into(),
        };
        assert_eq!(
            read(Some(
                json!({"source": "local_proxy", "model": "gpt-6-sol--fast", "effort": "high"})
            ))
            .new_bot_default,
            Some(Some(luna.clone()))
        );
        for left in [
            json!({"source": "gateway", "model": "xai/grok-4.7"}),
            json!({"source": "gateway", "model": "xai/grok-4.7", "effort": null}),
        ] {
            assert_eq!(
                read(Some(left.clone()))
                    .new_bot_default
                    .flatten()
                    .map(|default| default.effort),
                Some("inherit".to_string()),
                "{left}"
            );
        }

        let put = |new_bot_default| {
            serde_json::to_value(InferenceSourceUpdate {
                kind: InferenceKind::Gateway,
                via: None,
                relay_enabled: None,
                plan_fallback: None,
                new_bot_default,
            })
            .unwrap()
        };
        assert_eq!(put(None), json!({"kind": "gateway"}), "left alone, kept");
        assert_eq!(
            put(Some(None)),
            json!({"kind": "gateway", "newBotDefault": null}),
            "taken away"
        );
        assert_eq!(
            put(Some(Some(luna))),
            json!({
                "kind": "gateway",
                "newBotDefault": {"source": "local_proxy", "model": "gpt-6-sol--fast", "effort": "high"}
            }),
            "whole"
        );
    }

    /// Whether the relay is switched on reads as the agreed contract writes it (opengrok-server
    /// relay-off fallback contract, agreed 2026-10-03, not yet built), a bool on every read of a
    /// server that keeps it, and as no key from one before it. A `PUT` carries it only when the
    /// relay switch sends it: with the relay's way as the switch goes on, and with none as it goes
    /// off.
    #[test]
    fn whether_the_relay_is_on_reads_and_puts_as_the_contract_writes_it() {
        let read = |relay_enabled: Option<Value>| {
            let mut body = json!({
                "kind": "local_proxy", "via": "loopback", "baseUrl": null, "localModel": null,
                "healthy": false, "hasApiKey": false,
                "relay": {"connected": false, "machineId": null, "machineLabel": null,
                          "localModel": null}
            });
            if let Some(enabled) = relay_enabled {
                body["relayEnabled"] = enabled;
            }
            serde_json::from_value::<InferenceSource>(body).unwrap()
        };
        assert_eq!(read(None).relay_enabled, None, "a server before it");
        assert_eq!(read(Some(json!(true))).relay_enabled, Some(true));
        assert_eq!(read(Some(json!(false))).relay_enabled, Some(false));

        let put = |via, relay_enabled| {
            serde_json::to_value(InferenceSourceUpdate {
                kind: InferenceKind::LocalProxy,
                via,
                relay_enabled,
                new_bot_default: None,
                plan_fallback: None,
            })
            .unwrap()
        };
        assert_eq!(
            put(Some(Via::Mac), Some(true)),
            json!({"kind": "local_proxy", "relayEnabled": true, "via": "mac"})
        );
        assert_eq!(
            put(None, Some(false)),
            json!({"kind": "local_proxy", "relayEnabled": false})
        );
        assert_eq!(
            put(Some(Via::Mac), None),
            json!({"kind": "local_proxy", "via": "mac"}),
            "left alone, kept"
        );
    }

    /// The Relay-off fallback reads as the agreed contract writes it (opengrok-server relay-off
    /// fallback contract, agreed 2026-10-03, not yet built): left out by a server that keeps
    /// none, `null` while none is set, and whole once set, an effort left out or `null` read as
    /// `inherit`. A `PUT` names it only when the app changes it: whole, or `null` to take it away.
    #[test]
    fn the_relay_off_fallback_reads_and_puts_as_the_contract_writes_it() {
        let read = |fallback: Option<Value>| {
            let mut body = json!({
                "kind": "local_proxy", "baseUrl": null, "localModel": null,
                "healthy": false, "hasApiKey": false
            });
            if let Some(fallback) = fallback {
                body["planFallback"] = fallback;
            }
            serde_json::from_value::<InferenceSource>(body).unwrap()
        };
        let cheap = PlanFallback {
            model: "oag/cheap".into(),
            effort: "low".into(),
        };
        assert_eq!(read(None).plan_fallback, None, "a server before it");
        assert_eq!(read(Some(Value::Null)).plan_fallback, Some(None));
        assert_eq!(
            read(Some(json!({"model": "oag/cheap", "effort": "low"}))).plan_fallback,
            Some(Some(cheap.clone()))
        );
        for left in [
            json!({"model": "oag/cheap"}),
            json!({"model": "oag/cheap", "effort": null}),
        ] {
            assert_eq!(
                read(Some(left.clone()))
                    .plan_fallback
                    .flatten()
                    .map(|fallback| fallback.effort),
                Some("inherit".to_string()),
                "{left}"
            );
        }

        let put = |plan_fallback| {
            serde_json::to_value(InferenceSourceUpdate {
                kind: InferenceKind::LocalProxy,
                via: None,
                relay_enabled: None,
                new_bot_default: None,
                plan_fallback,
            })
            .unwrap()
        };
        assert_eq!(
            put(None),
            json!({"kind": "local_proxy"}),
            "left alone, kept"
        );
        assert_eq!(
            put(Some(None)),
            json!({"kind": "local_proxy", "planFallback": null}),
            "taken away"
        );
        assert_eq!(
            put(Some(Some(cheap))),
            json!({"kind": "local_proxy", "planFallback": {"model": "oag/cheap", "effort": "low"}}),
            "whole"
        );
    }

    /// `computer`, the name the server is being asked to take for `mac`, reads as the relay
    /// wherever a way is read: the account's own, a queued send's door, the CUSTOM frame, a model
    /// the relay lists, and a row written down. The app still sends `mac`, which every server with
    /// the relay reads.
    #[test]
    fn computer_reads_as_the_relay_and_mac_is_still_what_is_sent() {
        assert_eq!(Via::from_word("computer"), Some(Via::Mac));
        assert_eq!(
            serde_json::from_value::<Via>(json!("computer")).unwrap(),
            Via::Mac
        );
        assert_eq!(serde_json::to_value(Via::Mac).unwrap(), json!("mac"));
        let account: InferenceSource = serde_json::from_value(json!({
            "kind": "local_proxy", "healthy": false, "hasApiKey": false, "via": "computer",
            "relay": {"connected": true, "machineId": "mac_1"}
        }))
        .unwrap();
        assert_eq!(account.default_via(), Some(Via::Mac));
        assert_eq!(
            account.door().to_value(),
            json!({"kind": "local_proxy", "via": "mac"}),
            "named on a turn by the word every server reads"
        );
        assert_eq!(
            TurnSource::from_value(&json!({"kind": "local_proxy", "via": "computer"})),
            Some(TurnSource::plan(Some(Via::Mac)))
        );
        let frame = json!({
            "type": "CUSTOM", "name": INFERENCE_SOURCE_CUSTOM,
            "value": {"kind": "local_proxy", "via": "computer", "model": "gpt-6-luna"}
        });
        assert_eq!(
            ReplySource::from_event(&frame).and_then(|source| source.via),
            Some(Via::Mac)
        );
        let listed = crate::opengrok::ModelEntry {
            id: "gpt-6-luna".into(),
            source: Some("local_proxy".into()),
            via: Some("computer".into()),
        };
        assert_eq!(listed.plan_via(), Some(Via::Mac));
    }

    /// A turn names its door as every turn did before the relay, the bare word, when it names no
    /// way, and as `{"kind", "via"}` when it does; the gateway's door never carries one. A row
    /// brings either shape back, and a kind or a way this app cannot name is no door at all, so
    /// the server goes by the row's own word.
    #[test]
    fn a_turns_door_is_the_bare_word_or_the_kind_and_its_way() {
        assert_eq!(TurnSource::GATEWAY.to_value(), json!("gateway"));
        assert_eq!(TurnSource::plan(None).to_value(), json!("local_proxy"));
        assert_eq!(
            TurnSource::plan(Some(Via::Mac)).to_value(),
            json!({"kind": "local_proxy", "via": "mac"})
        );
        assert_eq!(
            TurnSource::plan(Some(Via::Loopback)).to_value(),
            json!({"kind": "local_proxy", "via": "loopback"})
        );
        let odd = TurnSource {
            kind: InferenceKind::Gateway,
            via: Some(Via::Mac),
        };
        assert_eq!(odd.to_value(), json!("gateway"), "the gateway has no way");
        assert_eq!(
            serde_json::to_value(TurnSource::plan(Some(Via::Mac))).unwrap(),
            json!({"kind": "local_proxy", "via": "mac"}),
            "serialized as it is sent"
        );
        for (value, read) in [
            (json!("gateway"), Some(TurnSource::GATEWAY)),
            (json!("local_proxy"), Some(TurnSource::plan(None))),
            (
                json!({"kind": "local_proxy", "via": "mac"}),
                Some(TurnSource::plan(Some(Via::Mac))),
            ),
            (
                json!({"kind": "local_proxy", "via": "loopback"}),
                Some(TurnSource::plan(Some(Via::Loopback))),
            ),
            (json!({"kind": "local_proxy"}), Some(TurnSource::plan(None))),
            (
                json!({"kind": "local_proxy", "via": null}),
                Some(TurnSource::plan(None)),
            ),
            (
                json!({"kind": "gateway", "via": "mac"}),
                Some(TurnSource::GATEWAY),
            ),
            (json!({"kind": "local_proxy", "via": "helper"}), None),
            (json!({"kind": "local_proxy", "via": 3}), None),
            (json!({"kind": "byok"}), None),
            (json!({"via": "mac"}), None),
            (json!("byok"), None),
            (json!(7), None),
            (Value::Null, None),
        ] {
            assert_eq!(TurnSource::from_value(&value), read, "{value}");
        }
        assert_eq!(
            TurnSource::from(InferenceKind::LocalProxy),
            TurnSource::plan(None)
        );
    }

    /// A server with the Mac relay says the account's way and where the relay stands, and the
    /// account's door is named with its way; one before the relay says neither, reads as the
    /// plan on the server's own machine, and is named the old way. A way this app cannot name
    /// leaves the door to the server. A relay without `connected` is refused rather than read
    /// as no Mac.
    #[test]
    fn the_accounts_setting_says_its_way_and_where_the_relay_stands() {
        let read: InferenceSource = serde_json::from_value(json!({
            "kind": "local_proxy", "baseUrl": null, "localModel": null,
            "healthy": false, "hasApiKey": false,
            "via": "mac",
            "relay": {
                "connected": true, "machineId": "mac_2",
                "machineLabel": "NativeChat on studio", "localModel": "gpt-5-codex"
            }
        }))
        .unwrap();
        assert!(read.knows_relay());
        assert_eq!(read.default_via(), Some(Via::Mac));
        assert_eq!(read.door(), TurnSource::plan(Some(Via::Mac)));
        assert_eq!(read.relay_model(), Some("gpt-5-codex"));
        assert_eq!(
            read.relay,
            Some(RelayRead {
                connected: true,
                machine_id: Some("mac_2".into()),
                machine_label: Some("NativeChat on studio".into()),
                local_model: Some("gpt-5-codex".into()),
            })
        );
        let nobody: InferenceSource = serde_json::from_value(json!({
            "kind": "gateway", "healthy": false, "hasApiKey": false, "via": "loopback",
            "relay": {"connected": false, "machineId": null, "machineLabel": null, "localModel": null}
        }))
        .unwrap();
        assert_eq!(nobody.door(), TurnSource::GATEWAY);
        assert_eq!(nobody.relay_model(), None);
        assert!(!nobody.relay.unwrap().connected);

        let before: InferenceSource = serde_json::from_value(json!({
            "kind": "local_proxy", "baseUrl": "http://127.0.0.1:8080",
            "localModel": "gpt-5-codex", "healthy": true, "hasApiKey": false
        }))
        .unwrap();
        assert!(!before.knows_relay());
        assert_eq!(before.default_via(), Some(Via::Loopback));
        assert_eq!(
            before.door(),
            TurnSource::plan(None),
            "named the old way, the only one such a server reads"
        );

        let helper: InferenceSource = serde_json::from_value(json!({
            "kind": "local_proxy", "healthy": true, "hasApiKey": false, "via": "helper",
            "relay": {"connected": false}
        }))
        .unwrap();
        assert_eq!(helper.default_via(), None);
        assert_eq!(
            helper.door(),
            TurnSource::plan(None),
            "the server's own way"
        );

        let broken = json!({
            "kind": "gateway", "healthy": false, "hasApiKey": false,
            "relay": {"machineId": "mac_2"}
        });
        assert!(serde_json::from_value::<InferenceSource>(broken).is_err());
    }

    /// The CUSTOM frame's way goes on the badge when it names one this app knows, and only on the
    /// plan's: a reply the gateway answered was reached no way, whatever a frame says.
    #[test]
    fn the_custom_frame_says_which_way_the_plan_answered() {
        let frame = |value: Value| json!({"type": "CUSTOM", "name": INFERENCE_SOURCE_CUSTOM, "value": value});
        assert_eq!(
            ReplySource::from_event(&frame(
                json!({"kind": "local_proxy", "via": "mac", "model": "gpt-5-codex"})
            )),
            Some(ReplySource {
                kind: InferenceKind::LocalProxy,
                via: Some(Via::Mac),
                model: Some("gpt-5-codex".into()),
                fallback_for: None,
            })
        );
        for (value, via) in [
            (
                json!({"kind": "local_proxy", "via": "loopback"}),
                Some(Via::Loopback),
            ),
            (json!({"kind": "local_proxy", "via": "helper"}), None),
            (json!({"kind": "local_proxy"}), None),
            (json!({"kind": "gateway", "via": "mac"}), None),
        ] {
            let read = ReplySource::from_event(&frame(value.clone())).expect("a badge");
            assert_eq!(read.via, via, "{value}");
        }
    }

    /// The relay's codes and `plan_unavailable` are the contracts' words; any other code, a
    /// gateway's say, is not one.
    #[test]
    fn a_plan_that_could_not_answer_is_read_off_its_code() {
        for code in RunErrorCode::ALL {
            assert_eq!(RunErrorCode::from_code(code.word()), Some(code));
        }
        assert_eq!(
            RunErrorCode::ALL.map(RunErrorCode::word),
            [
                "relay_offline",
                "relay_timeout",
                "relay_failed",
                "plan_unavailable"
            ]
        );
        for other in [
            "already-consumed",
            "relay",
            "",
            "RELAY_OFFLINE",
            "PLAN_UNAVAILABLE",
            "plan-unavailable",
        ] {
            assert_eq!(RunErrorCode::from_code(other), None, "{other:?}");
        }
    }

    /// Only OpenAI's and xAI's models are offered for the person's subscription, as the server's
    /// anchored allowlist reads them: bare or with their own provider's prefix, in any case, with
    /// or without the `--fast` tier. An id that only contains an allowed word is nobody's, and
    /// neither is one from a provider nobody has looked at. Anthropic's and Google's are refused
    /// wherever their names appear, an allowed start included.
    #[test]
    fn only_openai_and_xai_models_are_a_subscription_model() {
        for allowed in [
            "gpt-5-codex",
            "openai/gpt-5",
            "GPT-5.5",
            "o3-mini",
            "o4",
            "codex-mini-latest",
            "gpt-6-sol--fast",
            "grok-4",
            "xai/grok-code-fast",
            "xai/grok-4.7--fast",
        ] {
            assert!(is_subscription_model(allowed), "{allowed}");
        }
        for merely_contains in [
            "my-codex-thing",
            "my-codex-thing--fast",
            "openai/my-codex-thing",
            "notgrok-1",
            "xai/notgrok-1",
            "xgpt-5.5",
            "turbo-o3",
            "xai/codex-mini-latest",
            "openai/grok-4",
        ] {
            assert!(!is_subscription_model(merely_contains), "{merely_contains}");
        }
        for forbidden in [
            "claude-sonnet-4.5",
            "anthropic/claude-opus",
            "claude-codex",
            "Gemini-2.5-Pro",
            "google/gemini-flash",
            "gemini-gpt-4",
            "gpt-5-google",
        ] {
            assert!(!is_subscription_model(forbidden), "{forbidden}");
        }
        for unknown in [
            "",
            "  ",
            "llama-3.1-70b",
            "oag/cheap",
            "meta/gpt-5",
            "openai/gpt-5/extra",
            "gpt-5 codex",
        ] {
            assert!(!is_subscription_model(unknown), "{unknown:?}");
        }
    }
}
