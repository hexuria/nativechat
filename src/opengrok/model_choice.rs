//! A Bot's model as its picker offers it: the models the server lists, folded and named for a
//! person in the two groups a Bot can be answered from; the effort slider's five stops; and what
//! each change the picker makes sends to the server.
//!
//! No GPUI. The composer's chip and the Bot's card in its settings both draw what is here
//! (`components::model_picker`), and the gpui-agent tree names it, so the three always agree.
//!
//! A Bot's setting is three things its row keeps: the door (`source`, opengrok-server main
//! d6f640e (#307, after #304), pin bf99845), the model it is pinned to
//! (`model`), and how hard it thinks (`effort`, opengrok-server#271). Fast is not a fourth.
//! opencodex lists a model's fast tier as a twin id, `gpt-6-luna--fast` beside `gpt-6-luna`, and
//! the server's allowlist takes the tier off before it reads the rest ([`is_subscription_model`]),
//! so ⚡ is the pin moved to the twin and back, offered only where the list holds both.
//!
//! What the chip says is what the server would run the Bot's next turn on, as far as this app can
//! tell, and never a pin the server would ignore. On the person's plan the server asks the Bot's
//! pin only where the Bot's own door is the plan (`source: "local_proxy"`) and its allowlist takes
//! the pin, and the account's plan model otherwise (`ahead_of_the_setting` in opengrok-server's
//! `crates/opengrok-harness/src/local_proxy.rs`, server main d6f640e (#307, after #304), pin
//! bf99845). A Bot that follows the account (`source: null`), or is on the
//! gateway, runs a turn on the plan with the account's plan model whatever it is pinned to: every
//! Bot hired by default is pinned `xai/grok-4.6`, which the allowlist takes, and that pin is a
//! gateway route, not the plan model the person chose. A server from before per-Bot doors takes
//! the account's plan model for every Bot on the plan.

use super::{
    Coworker, CoworkerPatch, CoworkerSource, EFFORT_INHERIT, InferenceKind, InferenceSource,
    ModelCatalogue, Via, is_subscription_model,
};

/// What opencodex puts after a model's id for its fast tier.
pub const FAST_SUFFIX: &str = "--fast";

/// What a gateway id ends with when it is a seat billed to a subscription (`xai/grok-4.6@sub`).
/// The Server group is the server's paid keys, and a seat is not one, so it is not listed there.
const SEAT_SUFFIX: &str = "@sub";

/// The Bot's list's two groups, as they read.
pub const PLAN_GROUP: &str = "Your plan · opencodex";
pub const SERVER_GROUP: &str = "Server · paid keys";

/// Why ⚡ is dead: the list holds no fast twin of the model, or does not hold the model at all.
pub const FAST_NO_TWIN: &str = "The server lists no fast version of this model.";
/// Why ⚡ is dead: the Bot's door is a word this app cannot name, or follows an account setting
/// that has not been read, so which group the model is in cannot be told.
pub const FAST_DOOR_UNKNOWN: &str = "Where this Bot's replies go isn't known yet.";
/// Why ⚡ is dead on a server without per-Bot doors while the account is on the person's plan:
/// the plan's model there is the account's, and a Bot's pin would change nothing.
pub const FAST_ACCOUNT_PLAN: &str =
    "On this server every Bot on your plan uses the model in Settings → Reply source.";
/// Why the slider is dead: a server from before opengrok-server#271 keeps no effort, and a pick
/// would look saved and change nothing.
pub const EFFORT_NOT_KEPT: &str = "This server has nowhere to keep an effort yet.";
/// What the chip names while there is no model to name: a Bot with no pin, or on a plan that
/// keeps no model.
pub const NO_MODEL: &str = "No model";
/// What the list says under its rows for a Bot whose own door is the person's plan
/// ([`ModelPick::routines`]).
pub const ROUTINES_ON_PLAN: &str = "This Bot's routines won't run while it answers on your own \
                                    plan: routines run on the server's keys. Pick a Server model \
                                    to run it on a schedule.";

/// Whether an id is a model's fast tier.
pub fn is_fast(id: &str) -> bool {
    id.ends_with(FAST_SUFFIX)
}

/// The id with its fast tier taken off.
pub fn without_fast(id: &str) -> &str {
    id.strip_suffix(FAST_SUFFIX).unwrap_or(id)
}

/// One row of the picker's list: a model, with its fast twin folded into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    /// The door it is served through, which is also the group it is listed in.
    pub source: InferenceKind,
    /// The id a pick pins the Bot to with ⚡ off. A fast tier listed without its plain one is a
    /// row of its own, by its own id, and has no twin to go back to.
    pub base_id: String,
    /// The list holds `{base_id}--fast` too, so ⚡ can move the pin to it and back.
    pub has_fast: bool,
    /// What the row reads as ([`model_label`]).
    pub label: String,
}

impl ModelChoice {
    /// The id a pick of this row pins with ⚡ on or off: the twin only where the list has one.
    pub fn pin(&self, fast: bool) -> String {
        if fast && self.has_fast {
            format!("{}{FAST_SUFFIX}", self.base_id)
        } else {
            self.base_id.clone()
        }
    }

    /// This row is the model `id`: its own id, or its fast twin's.
    pub fn takes(&self, id: &str) -> bool {
        id == self.base_id
            || (self.has_fast && id.strip_suffix(FAST_SUFFIX) == Some(self.base_id.as_str()))
    }
}

/// One group of the list: the models of one door, in the server's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceGroup {
    pub source: InferenceKind,
    pub rows: Vec<ModelChoice>,
}

impl ChoiceGroup {
    /// The group's heading.
    pub fn title(&self) -> &'static str {
        match self.source {
            InferenceKind::LocalProxy => PLAN_GROUP,
            InferenceKind::Gateway => SERVER_GROUP,
        }
    }
}

/// The rows a list of ids makes, in the list's order: a model and its fast twin are one row, at
/// the place of the first of the two, and an id listed twice is one row.
pub fn fold<'a>(source: InferenceKind, ids: impl IntoIterator<Item = &'a str>) -> Vec<ModelChoice> {
    let ids: Vec<&str> = ids.into_iter().collect();
    let listed = |id: &str| ids.contains(&id);
    let mut rows: Vec<ModelChoice> = Vec::new();
    for &id in &ids {
        let plain = id.strip_suffix(FAST_SUFFIX).filter(|plain| listed(plain));
        let base = plain.unwrap_or(id);
        if rows.iter().any(|row| row.base_id == base) {
            continue;
        }
        rows.push(ModelChoice {
            source,
            base_id: base.to_string(),
            has_fast: plain.is_some() || (!is_fast(id) && listed(&format!("{id}{FAST_SUFFIX}"))),
            label: model_label(base),
        });
    }
    rows
}

/// The Server group: the gateway's routes as `GET /models` lists them, in its order, less the
/// seats billed to a subscription.
pub fn server_choices(catalogue: &ModelCatalogue) -> Vec<ModelChoice> {
    fold(
        InferenceKind::Gateway,
        catalogue
            .models
            .iter()
            .filter(|entry| entry.source() == Some(InferenceKind::Gateway))
            .map(|entry| entry.id.as_str())
            .filter(|id| !without_fast(id).to_ascii_lowercase().ends_with(SEAT_SUFFIX)),
    )
}

/// The plan group: the person's plan's models as `AppState::plan_models` gives them for the
/// account's way to the plan, already held to the server's allowlist.
pub fn plan_choices(ids: &[String]) -> Vec<ModelChoice> {
    fold(InferenceKind::LocalProxy, ids.iter().map(String::as_str))
}

/// A model as a person reads it: `gpt-6-luna` is "GPT-6 Luna", `openai/gpt-6.1-sol` "GPT-6.1
/// Sol", `xai/grok-4.7` "Grok 4.7", and a route of the gateway's own, `oag/cheap`, "Cheap
/// (auto)", since the gateway picks the model behind it. A fast tier reads with ⚡ after it. Any
/// other id is shown as it is: a name made up for a model this app does not know would be a name
/// nobody could look up.
pub fn model_label(id: &str) -> String {
    match id.strip_suffix(FAST_SUFFIX) {
        Some(plain) => format!("{} ⚡", base_label(plain)),
        None => base_label(id),
    }
}

/// [`model_label`] without the fast tier's mark, for where ⚡ is said on its own.
pub fn base_label(id: &str) -> String {
    let id = without_fast(id);
    readable(id).unwrap_or_else(|| id.to_string())
}

fn readable(id: &str) -> Option<String> {
    let lower = id.to_ascii_lowercase();
    let (provider, name) = match lower.split_once('/') {
        Some((provider, name)) => (Some(provider), name),
        None => (None, lower.as_str()),
    };
    let words: Vec<&str> = name.split('-').collect();
    let plain = |word: &&str| {
        !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
    };
    if !words.iter().all(plain) {
        return None;
    }
    let spaced = |rest: &[&str]| -> String {
        rest.iter()
            .map(|word| format!(" {}", capitalised(word)))
            .collect()
    };
    match (provider, words.as_slice()) {
        (Some("oag"), [first, rest @ ..]) => {
            Some(format!("{}{} (auto)", capitalised(first), spaced(rest)))
        }
        (None | Some("openai"), ["gpt", version, rest @ ..]) if is_version(version) => {
            Some(format!("GPT-{version}{}", spaced(rest)))
        }
        (None | Some("xai"), ["grok", rest @ ..]) if !rest.is_empty() => {
            Some(format!("Grok{}", spaced(rest)))
        }
        _ => None,
    }
}

/// A model family's version, as `6`, `5.6` or `4o` are.
fn is_version(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_digit())
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + chars.as_str()
    })
}

/// The slider's five stops, left to right: the name each reads as, and the server's word it
/// keeps (opengrok-server#271's `EFFORT_WORDS`, in its order).
pub const EFFORT_STOPS: [(&str, &str); 5] = [
    ("Light", "low"),
    ("Medium", "medium"),
    ("High", "high"),
    ("Extra", "xhigh"),
    ("Ultra", "max"),
];

/// What a Bot with no effort of its own reads as: its turns send the gateway none, and the
/// model's route decides.
pub const DEFAULT_EFFORT_LABEL: &str = "Default";

/// Where the slider's thumb sits for a word that is none of the stops.
const MEDIUM_STOP: usize = 1;

/// The stop a server word is, if it is one.
pub fn effort_stop(word: &str) -> Option<usize> {
    EFFORT_STOPS.iter().position(|(_, stop)| *stop == word)
}

/// The server word a stop keeps.
pub fn stop_word(stop: usize) -> Option<&'static str> {
    EFFORT_STOPS.get(stop).map(|(_, word)| *word)
}

/// An effort as the picker names it: a stop by its name, `inherit` as "Default", and a word the
/// server keeps that is none of the stops (`none`, or one this app has not heard of) as it is,
/// so the picker never shows a value the server does not hold.
pub fn effort_label(word: &str) -> String {
    if word == EFFORT_INHERIT {
        return DEFAULT_EFFORT_LABEL.to_string();
    }
    effort_stop(word).map_or_else(|| word.to_string(), |stop| EFFORT_STOPS[stop].0.to_string())
}

/// Where the slider's thumb sits for a word: on its stop, and on Medium for a word with none
/// (Default, or a kept word such as `none`), where the slider is drawn muted and the label beside
/// it says which it is.
pub fn slider_stop(word: &str) -> usize {
    effort_stop(word).unwrap_or(MEDIUM_STOP)
}

/// On a server without per-Bot doors, while the account's replies are on the person's plan: the
/// plan model that answers for every Bot there, whatever each is pinned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountPlan {
    /// `None` where the account keeps no plan model, and the server turns every such turn away.
    pub model: Option<String>,
}

/// The open Bot's model, effort and fast tier as its picker shows them, what the list offers,
/// and what each change sends ([`bot_pick`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPick {
    pub bot_id: String,
    /// The server keeps a door per Bot: the Bot's row carries `source`. Only then does the list
    /// offer the plan's models and a pick send a door.
    pub per_bot: bool,
    /// The Bot's own door, when its row names one this app knows. `None` follows the account's.
    /// Only a Bot whose own door is the plan is answered there with its pin.
    pub bot_door: Option<InferenceKind>,
    /// The door the Bot's next turn goes through, when this app can tell.
    pub door: Option<InferenceKind>,
    /// The model the next turn runs on, when this app can tell: the Bot's pin, or on the
    /// person's plan the account's plan model, unless the Bot's own door is the plan and the
    /// allowlist takes its pin.
    pub model: Option<String>,
    /// The Bot's pin, as its row keeps it.
    pub pin: String,
    /// The row that is `model` through `door`: the one the list ticks.
    pub current: Option<ModelChoice>,
    /// Why ⚡ cannot be switched, when it cannot.
    pub fast_blocked: Option<&'static str>,
    /// The Bot's effort, in the server's word: `inherit` where it keeps none.
    pub effort: String,
    /// The server keeps an effort: the Bot's row carries one (opengrok-server#271).
    pub effort_kept: bool,
    /// The list, the plan's group first, each group only while it has rows.
    pub groups: Vec<ChoiceGroup>,
    /// The line that takes the plan group's place on a server without per-Bot doors, while the
    /// account is on the person's plan.
    pub account_plan: Option<AccountPlan>,
    /// The line that says this Bot's routines won't run ([`ROUTINES_ON_PLAN`]). A routine runs
    /// on the server's keys, and the person chose their own plan for a Bot whose own door is the
    /// plan, so the server refuses every routine of such a Bot, in words, before any model call
    /// and with nothing billed (the owner's decision: `Route::for_routine` in opengrok-server's
    /// `crates/opengrok-harness/src/local_proxy.rs`, server main d6f640e (#307, after #304), pin
    /// bf99845, whose recording holds the refusal). It is said for every
    /// such Bot, whatever it is pinned to and whatever the gateway lists: a pin the gateway has
    /// is refused as much as one it lacks. A Bot that follows the account (`source: null`), or
    /// is on the gateway, runs its routines through the gateway on its pin as before
    /// (opengrok-server #294, `autonomy/mod.rs`), and is told nothing.
    pub routines: Option<String>,
}

/// The way to the account's plan a turn that names none goes: the account's own where the server
/// knows the relay, which is `None` for one this app cannot name, and the server's own machine
/// where it does not know the relay, which has no other.
fn account_via(account: &InferenceSource) -> Option<Via> {
    if account.knows_relay() {
        account.default_via()
    } else {
        Some(Via::Loopback)
    }
}

/// The account's plan model for its way: the relay's for a Mac, the plan's own otherwise.
fn account_plan_model(account: &InferenceSource) -> Option<String> {
    match account_via(account)? {
        Via::Mac => account.relay_model().map(str::to_string),
        Via::Loopback => account.local_model.clone(),
    }
}

/// The open Bot's picker, from its row, the account's reply source as last read (`None` before it
/// is read, or on a server without reply sources), the server's list of models, and the plan's
/// models for a way to it (`AppState::plan_models`).
pub fn bot_pick(
    bot: &Coworker,
    account: Option<&InferenceSource>,
    catalogue: &ModelCatalogue,
    plan_models: impl Fn(Via) -> Vec<String>,
) -> ModelPick {
    let per_bot = bot.source != CoworkerSource::NotKept;
    let bot_door = match &bot.source {
        CoworkerSource::Kind(kind) => Some(*kind),
        _ => None,
    };
    let door = match &bot.source {
        CoworkerSource::Kind(kind) => Some(*kind),
        CoworkerSource::NotKept | CoworkerSource::AccountDefault => {
            account.map(|account| account.kind)
        }
        CoworkerSource::Unknown(_) => None,
    };
    let pin = Some(bot.model.clone()).filter(|pin| !pin.trim().is_empty());
    let account_model = account.and_then(account_plan_model);
    // On the plan the server asks the pin only of a Bot whose own door is the plan, and only a
    // pin its allowlist takes; any other Bot there runs on the account's plan model
    // (opengrok-server main d6f640e (#307, after #304), pin bf99845).
    // A Bot that follows the account may well hold a pin the allowlist takes, as every default
    // hire's `xai/grok-4.6` is, and that pin is not the plan model the person chose.
    let model = match door {
        Some(InferenceKind::LocalProxy)
            if bot_door == Some(InferenceKind::LocalProxy)
                && pin.as_deref().is_some_and(is_subscription_model) =>
        {
            pin.clone()
        }
        Some(InferenceKind::LocalProxy) => account_model.clone(),
        _ => pin.clone(),
    };
    // A Bot is offered its own plan model only where the server keeps a door per Bot: anywhere
    // else the server would run the account's plan model and ignore the pick.
    let plan = match (per_bot, account.and_then(account_via)) {
        (true, Some(via)) => plan_choices(&plan_models(via)),
        _ => Vec::new(),
    };
    let groups: Vec<ChoiceGroup> = [
        ChoiceGroup {
            source: InferenceKind::LocalProxy,
            rows: plan,
        },
        ChoiceGroup {
            source: InferenceKind::Gateway,
            rows: server_choices(catalogue),
        },
    ]
    .into_iter()
    .filter(|group| !group.rows.is_empty())
    .collect();
    let current = door.zip(model.as_deref()).and_then(|(door, model)| {
        groups
            .iter()
            .filter(|group| group.source == door)
            .flat_map(|group| &group.rows)
            .find(|row| row.takes(model))
            .cloned()
    });
    let fast_blocked = match (door, &current) {
        (None, _) => Some(FAST_DOOR_UNKNOWN),
        (Some(InferenceKind::LocalProxy), _) if !per_bot => Some(FAST_ACCOUNT_PLAN),
        (_, Some(row)) if row.has_fast => None,
        _ => Some(FAST_NO_TWIN),
    };
    let account_plan =
        (!per_bot && door == Some(InferenceKind::LocalProxy)).then_some(AccountPlan {
            model: account_model,
        });
    // The Bot's own door alone decides: the server refuses the routines of a Bot on its own plan
    // whatever its pin (opengrok-server #304).
    let routines =
        (bot_door == Some(InferenceKind::LocalProxy)).then(|| ROUTINES_ON_PLAN.to_string());
    ModelPick {
        bot_id: bot.id.clone(),
        per_bot,
        bot_door,
        door,
        model,
        pin: bot.model.clone(),
        current,
        fast_blocked,
        effort: bot.effort().to_string(),
        effort_kept: bot.effort.is_some(),
        groups,
        account_plan,
        routines,
    }
}

impl ModelPick {
    /// The next turn runs on a model's fast tier.
    pub fn is_fast(&self) -> bool {
        self.model.as_deref().is_some_and(is_fast)
    }

    /// The model's name as the chip and the list's opener say it, ⚡ apart.
    pub fn model_label(&self) -> String {
        self.model
            .as_deref()
            .map_or_else(|| NO_MODEL.to_string(), base_label)
    }

    /// What the chip reads: "GPT-6 Luna · Medium ⚡".
    pub fn chip_label(&self) -> String {
        let fast = if self.is_fast() { " ⚡" } else { "" };
        format!(
            "{} · {}{fast}",
            self.model_label(),
            effort_label(&self.effort)
        )
    }

    /// Every row the list offers, group by group.
    pub fn rows(&self) -> impl Iterator<Item = &ModelChoice> {
        self.groups.iter().flat_map(|group| group.rows.iter())
    }

    /// One row, by its group and its id.
    pub fn row(&self, source: InferenceKind, base_id: &str) -> Option<&ModelChoice> {
        self.rows()
            .find(|row| row.source == source && row.base_id == base_id)
    }

    /// Whether the list ticks this row.
    pub fn is_current(&self, row: &ModelChoice) -> bool {
        self.current.as_ref() == Some(row)
    }

    /// Something ↺ would change ([`Self::reset_patch`]).
    pub fn can_reset(&self) -> bool {
        self.reset_patch().is_some()
    }

    /// What pinning `row` at `pin` sends: the model where it is not the pin already, and the
    /// row's door where the server keeps one per Bot and it is not the Bot's already, so a model
    /// always goes with the door it is served through and the pick sticks to the Bot. Nothing
    /// when the Bot is there already.
    fn patch_to(&self, row: &ModelChoice, pin: String) -> Option<CoworkerPatch> {
        let source = (self.per_bot && self.bot_door != Some(row.source)).then_some(row.source);
        let model = (pin != self.pin).then_some(pin);
        (source.is_some() || model.is_some()).then(|| CoworkerPatch {
            model,
            source,
            ..Default::default()
        })
    }

    /// What a pick of a row sends: its model, fast where ⚡ is on and the row has a twin, and its
    /// door ([`Self::patch_to`]). A row the list does not offer is refused.
    pub fn pick_patch(
        &self,
        source: InferenceKind,
        base_id: &str,
    ) -> Result<Option<CoworkerPatch>, String> {
        let row = self.row(source, base_id).ok_or_else(|| {
            format!(
                "`{base_id}` is not among the {} models the list offers",
                source.word()
            )
        })?;
        Ok(self.patch_to(row, row.pin(self.is_fast())))
    }

    /// What ⚡ switched on or off sends: the pin moved to the model's twin or back. Refused, with
    /// why, where ⚡ is dead.
    pub fn fast_patch(&self, on: bool) -> Result<Option<CoworkerPatch>, &'static str> {
        if let Some(why) = self.fast_blocked {
            return Err(why);
        }
        let row = self.current.as_ref().ok_or(FAST_NO_TWIN)?;
        Ok(self.patch_to(row, row.pin(on)))
    }

    /// What a stop of the slider sends: its word, where it is not the Bot's already. Refused from
    /// a server that keeps no effort, and for a word that is not one of the five stops.
    pub fn effort_patch(&self, word: &str) -> Result<Option<CoworkerPatch>, String> {
        if !self.effort_kept {
            return Err(EFFORT_NOT_KEPT.to_string());
        }
        if effort_stop(word).is_none() {
            let stops: Vec<&str> = EFFORT_STOPS.iter().map(|(_, word)| *word).collect();
            return Err(format!(
                "`{word}` is not a stop of the slider, which keeps {}",
                stops.join(", ")
            ));
        }
        Ok((word != self.effort).then(|| CoworkerPatch {
            effort: Some(word.to_string()),
            ..Default::default()
        }))
    }

    /// What ↺ sends: the effort back to Default, where the server keeps one, and ⚡ off where it
    /// is on and can be switched. The model itself is left where it is. Nothing when there is
    /// nothing to put back.
    pub fn reset_patch(&self) -> Option<CoworkerPatch> {
        let effort =
            (self.effort_kept && self.effort != EFFORT_INHERIT).then(|| EFFORT_INHERIT.to_string());
        let fast_off = if self.is_fast() {
            self.fast_patch(false).ok().flatten()
        } else {
            None
        };
        let patch = CoworkerPatch {
            effort,
            ..fast_off.unwrap_or_default()
        };
        (!patch.is_empty()).then_some(patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opengrok::{ModelEntry, RelayRead};
    use serde_json::{Value, json};

    fn entry(id: &str, source: &str, via: Option<&str>) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            source: Some(source.into()),
            via: via.map(str::to_string),
        }
    }

    fn catalogue(gateway: &[&str]) -> ModelCatalogue {
        ModelCatalogue {
            models: gateway
                .iter()
                .map(|id| entry(id, "gateway", None))
                .collect(),
            note: None,
            local_proxy: None,
        }
    }

    /// A Bot's row, with `source` as given, or without the key at all.
    fn bot(source: Option<Value>, model: &str, effort: Option<&str>) -> Coworker {
        let mut row = json!({"id": "cw_1", "name": "Ada", "model": model});
        if let Some(source) = source {
            row["source"] = source;
        }
        if let Some(effort) = effort {
            row["effort"] = json!(effort);
        }
        serde_json::from_value(row).expect("a row")
    }

    fn account(kind: InferenceKind, local_model: Option<&str>) -> InferenceSource {
        InferenceSource {
            kind,
            base_url: Some("http://127.0.0.1:8080".into()),
            local_model: local_model.map(str::to_string),
            healthy: true,
            has_api_key: false,
            via: None,
            relay: None,
        }
    }

    fn plan(ids: &'static [&'static str]) -> impl Fn(Via) -> Vec<String> {
        move |via| match via {
            Via::Loopback => ids.iter().map(|id| id.to_string()).collect(),
            Via::Mac => Vec::new(),
        }
    }

    const PLAN: &[&str] = &["gpt-6-luna", "gpt-6-luna--fast", "gpt-5.6-sol"];
    const SERVER: &[&str] = &["oag/cheap", "xai/grok-4.7", "xai/grok-4.6@sub"];

    fn ids(rows: &[ModelChoice]) -> Vec<(&str, bool)> {
        rows.iter()
            .map(|row| (row.base_id.as_str(), row.has_fast))
            .collect()
    }

    /// A model and its fast twin are one row that offers ⚡, wherever the twin sits in the list;
    /// a fast tier listed alone is a row of its own, read with ⚡, with no twin to go back to;
    /// and an id listed twice is one row.
    #[test]
    fn a_fast_twin_folds_into_its_models_row() {
        let rows = fold(
            InferenceKind::LocalProxy,
            [
                "gpt-6-luna",
                "gpt-6-luna--fast",
                "gpt-5.6-sol--fast",
                "gpt-5.6-sol",
                "grok-4.7--fast",
                "gpt-5-codex",
                "gpt-5-codex",
            ],
        );
        assert_eq!(
            ids(&rows),
            [
                ("gpt-6-luna", true),
                ("gpt-5.6-sol", true),
                ("grok-4.7--fast", false),
                ("gpt-5-codex", false),
            ]
        );
        assert_eq!(rows[0].label, "GPT-6 Luna");
        assert_eq!(rows[2].label, "Grok 4.7 ⚡");
        assert_eq!(rows[0].pin(true), "gpt-6-luna--fast");
        assert_eq!(rows[0].pin(false), "gpt-6-luna");
        assert_eq!(
            rows[2].pin(false),
            "grok-4.7--fast",
            "no plain tier to go to"
        );
        assert!(rows[0].takes("gpt-6-luna--fast") && rows[0].takes("gpt-6-luna"));
        assert!(!rows[3].takes("gpt-5-codex--fast"), "no twin listed");
        assert!(
            rows.iter()
                .all(|row| row.source == InferenceKind::LocalProxy)
        );
    }

    /// Ids read as a person says the model's name, and any other id as it is.
    #[test]
    fn a_model_reads_as_a_person_says_it() {
        for (id, read) in [
            ("gpt-6-luna", "GPT-6 Luna"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("openai/gpt-6.1-sol", "GPT-6.1 Sol"),
            ("GPT-5.5", "GPT-5.5"),
            ("gpt-5-codex", "GPT-5 Codex"),
            ("gpt-4o", "GPT-4o"),
            ("xai/grok-4.7", "Grok 4.7"),
            ("grok-4", "Grok 4"),
            ("grok-code-fast-1", "Grok Code Fast 1"),
            ("oag/cheap", "Cheap (auto)"),
            ("gpt-6-luna--fast", "GPT-6 Luna ⚡"),
            // Nobody's family this app knows: as it is.
            ("o3-mini", "o3-mini"),
            ("anthropic/claude-opus-4", "anthropic/claude-opus-4"),
            ("xai/grok-4.6@sub", "xai/grok-4.6@sub"),
            ("xai/gpt-5", "xai/gpt-5"),
            ("gpt-oss-120b", "gpt-oss-120b"),
            ("grok", "grok"),
            ("", ""),
        ] {
            assert_eq!(model_label(id), read, "{id:?}");
        }
        assert_eq!(base_label("gpt-6-luna--fast"), "GPT-6 Luna");
    }

    /// The Server group is the gateway's routes, in the server's order, less the seats billed to
    /// a subscription, and never one of the plan's models.
    #[test]
    fn a_seat_billed_to_a_subscription_is_not_a_server_model() {
        let mut listed = catalogue(&[
            "oag/cheap",
            "xai/grok-4.6@sub",
            "xai/grok-4.7",
            "XAI/GROK-4.6@SUB",
            "oag/fast",
            "oag/fast--fast",
        ]);
        listed
            .models
            .push(entry("gpt-6-luna", "local_proxy", Some("loopback")));
        listed.models.push(ModelEntry {
            id: "odd".into(),
            source: Some("byok".into()),
            via: None,
        });
        assert_eq!(
            ids(&server_choices(&listed)),
            [
                ("oag/cheap", false),
                ("xai/grok-4.7", false),
                ("oag/fast", true)
            ]
        );
    }

    /// The slider's five stops are the server's words, both ways; Default is `inherit`, and a
    /// kept word that is no stop reads as itself, with the thumb on Medium.
    #[test]
    fn the_sliders_stops_are_the_servers_effort_words() {
        assert_eq!(
            EFFORT_STOPS.map(|(name, word)| (effort_label(word), effort_stop(word), name)),
            [
                ("Light".to_string(), Some(0), "Light"),
                ("Medium".to_string(), Some(1), "Medium"),
                ("High".to_string(), Some(2), "High"),
                ("Extra".to_string(), Some(3), "Extra"),
                ("Ultra".to_string(), Some(4), "Ultra"),
            ]
        );
        assert_eq!(
            (0..6).map(stop_word).collect::<Vec<_>>(),
            [
                Some("low"),
                Some("medium"),
                Some("high"),
                Some("xhigh"),
                Some("max"),
                None
            ]
        );
        assert_eq!(effort_label("inherit"), "Default");
        assert_eq!(effort_label("none"), "none");
        assert_eq!(effort_label("ultra"), "ultra");
        assert_eq!(
            ["low", "max", "inherit", "none"].map(slider_stop),
            [0, 4, 1, 1]
        );
    }

    /// A Bot on its own plan with a model the list has a twin of: the chip names the model, the
    /// effort and ⚡; ⚡ moves the pin to the twin and back, and never re-sends the door the Bot
    /// already has; a stop of the slider sends its word.
    #[test]
    fn a_bot_on_its_own_plan_moves_between_its_model_and_the_twin() {
        let on_plan = bot(Some(json!("local_proxy")), "gpt-6-luna", Some("medium"));
        let pick = bot_pick(
            &on_plan,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert!(pick.per_bot);
        assert_eq!(pick.door, Some(InferenceKind::LocalProxy));
        assert_eq!(pick.chip_label(), "GPT-6 Luna · Medium");
        assert_eq!(pick.fast_blocked, None);
        assert_eq!(
            pick.groups
                .iter()
                .map(ChoiceGroup::title)
                .collect::<Vec<_>>(),
            [PLAN_GROUP, SERVER_GROUP]
        );
        let ticked: Vec<&str> = pick
            .rows()
            .filter(|row| pick.is_current(row))
            .map(|row| row.base_id.as_str())
            .collect();
        assert_eq!(ticked, ["gpt-6-luna"]);
        assert_eq!(
            serde_json::to_value(pick.fast_patch(true).unwrap()).unwrap(),
            json!({"model": "gpt-6-luna--fast"})
        );
        assert_eq!(
            pick.fast_patch(false).unwrap().map(|_| ()),
            None,
            "off already"
        );
        assert_eq!(
            serde_json::to_value(pick.effort_patch("max").unwrap()).unwrap(),
            json!({"effort": "max"})
        );
        assert!(pick.effort_patch("medium").unwrap().is_none(), "no change");
        assert!(pick.effort_patch("inherit").is_err(), "not a stop: ↺ is");
        assert!(pick.effort_patch("none").is_err());

        let fast = bot(Some(json!("local_proxy")), "gpt-6-luna--fast", Some("max"));
        let pick = bot_pick(
            &fast,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(pick.chip_label(), "GPT-6 Luna · Ultra ⚡");
        assert!(pick.is_fast());
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("gpt-6-luna")
        );
        assert_eq!(
            serde_json::to_value(pick.fast_patch(false).unwrap()).unwrap(),
            json!({"model": "gpt-6-luna"})
        );
    }

    /// A pick of a row sends its model with its door, and only what changes: the door the Bot
    /// is on already is not sent again. ⚡ stays on where the new model has a twin and goes where
    /// it has none, and a Server model has no ⚡ to offer.
    #[test]
    fn a_pick_sends_the_rows_door_and_model_and_keeps_fast_where_it_can() {
        let fast = bot(Some(json!("local_proxy")), "gpt-6-luna--fast", Some("high"));
        let pick = bot_pick(
            &fast,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(SERVER),
            plan(&[
                "gpt-6-luna",
                "gpt-6-luna--fast",
                "gpt-5.6-sol",
                "gpt-5.6-sol--fast",
            ]),
        );
        let body = |patch: Result<Option<CoworkerPatch>, String>| {
            serde_json::to_value(patch.unwrap()).unwrap()
        };
        assert_eq!(
            body(pick.pick_patch(InferenceKind::LocalProxy, "gpt-5.6-sol")),
            json!({"model": "gpt-5.6-sol--fast"}),
            "⚡ stays on: the new model has a twin"
        );
        assert_eq!(
            body(pick.pick_patch(InferenceKind::Gateway, "xai/grok-4.7")),
            json!({"model": "xai/grok-4.7", "source": "gateway"})
        );
        assert!(
            pick.pick_patch(InferenceKind::LocalProxy, "gpt-6-luna")
                .unwrap()
                .is_none(),
            "the Bot is on it already, fast and all"
        );
        assert!(
            pick.pick_patch(InferenceKind::Gateway, "xai/grok-4.6@sub")
                .is_err(),
            "a seat is not offered"
        );
        assert!(
            pick.pick_patch(InferenceKind::LocalProxy, "oag/cheap")
                .is_err(),
            "not in that group"
        );

        // On a Server model: no twin in the list, so ⚡ is dead and says why.
        let keys = bot(Some(json!("gateway")), "xai/grok-4.7", Some("high"));
        let pick = bot_pick(
            &keys,
            Some(&account(InferenceKind::LocalProxy, Some("gpt-6-luna"))),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(pick.door, Some(InferenceKind::Gateway));
        assert_eq!(pick.chip_label(), "Grok 4.7 · High");
        assert_eq!(pick.fast_blocked, Some(FAST_NO_TWIN));
        assert_eq!(pick.fast_patch(true), Err(FAST_NO_TWIN));
        assert_eq!(
            body(pick.pick_patch(InferenceKind::LocalProxy, "gpt-6-luna")),
            json!({"model": "gpt-6-luna", "source": "local_proxy"})
        );
    }

    /// ↺ puts the effort back to Default and turns ⚡ off, and leaves the model where it is;
    /// with nothing to put back it has nothing to send.
    #[test]
    fn reset_puts_the_effort_back_to_default_and_turns_fast_off() {
        let fast = bot(Some(json!("local_proxy")), "gpt-6-luna--fast", Some("max"));
        let pick = bot_pick(
            &fast,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(
            serde_json::to_value(pick.reset_patch().unwrap()).unwrap(),
            json!({"model": "gpt-6-luna", "effort": "inherit"})
        );
        let settled = bot(Some(json!("local_proxy")), "gpt-6-luna", Some("inherit"));
        let pick = bot_pick(
            &settled,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(pick.chip_label(), "GPT-6 Luna · Default");
        assert!(!pick.can_reset());
        let effort_only = bot(Some(json!("gateway")), "oag/cheap", Some("low"));
        let pick = bot_pick(&effort_only, None, &catalogue(SERVER), plan(PLAN));
        assert_eq!(
            serde_json::to_value(pick.reset_patch().unwrap()).unwrap(),
            json!({"effort": "inherit"})
        );
    }

    /// A server whose rows carry no `source` keeps no door per Bot. The list offers the Server
    /// group alone and no pick sends a door; while the account is on the person's plan, the chip
    /// names the account's plan model, which answers for every Bot there, a read-only line says
    /// so, and ⚡ is dead, since a pin would change nothing.
    #[test]
    fn a_server_without_per_bot_doors_offers_no_plan_rows_and_says_whose_model_answers() {
        let old = bot(None, "oag/cheap", Some("medium"));
        let on_plan = account(InferenceKind::LocalProxy, Some("gpt-5-codex"));
        let pick = bot_pick(&old, Some(&on_plan), &catalogue(SERVER), plan(PLAN));
        assert!(!pick.per_bot);
        assert_eq!(
            pick.groups
                .iter()
                .map(|group| group.source)
                .collect::<Vec<_>>(),
            [InferenceKind::Gateway],
            "no plan rows for a Bot"
        );
        assert_eq!(pick.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(pick.chip_label(), "GPT-5 Codex · Medium");
        assert_eq!(
            pick.account_plan,
            Some(AccountPlan {
                model: Some("gpt-5-codex".into())
            })
        );
        assert_eq!(pick.fast_blocked, Some(FAST_ACCOUNT_PLAN));
        assert!(pick.current.is_none(), "no Server row answers");
        assert_eq!(
            serde_json::to_value(
                pick.pick_patch(InferenceKind::Gateway, "xai/grok-4.7")
                    .unwrap()
            )
            .unwrap(),
            json!({"model": "xai/grok-4.7"}),
            "no door to a server that keeps none"
        );
        assert!(
            pick.pick_patch(InferenceKind::LocalProxy, "gpt-6-luna")
                .is_err()
        );

        // The account on the server's keys: the Bot's pin answers, and no plan line.
        let pick = bot_pick(
            &old,
            Some(&account(InferenceKind::Gateway, Some("gpt-5-codex"))),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(pick.chip_label(), "Cheap (auto) · Medium");
        assert_eq!(pick.account_plan, None);
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("oag/cheap")
        );

        // The account's door not read yet: the pin, and ⚡ cannot tell its group.
        let pick = bot_pick(&old, None, &catalogue(SERVER), plan(PLAN));
        assert_eq!(pick.chip_label(), "Cheap (auto) · Medium");
        assert_eq!(pick.fast_blocked, Some(FAST_DOOR_UNKNOWN));
    }

    /// On the person's plan a Bot that follows the account runs on the account's plan model, a
    /// gateway pin as much as any, and the chip says so; ⚡ and a pick then pin that model with its
    /// door, so the pick sticks. On its own plan a Bot runs on a pin the allowlist takes.
    #[test]
    fn a_pin_the_plan_would_not_take_shows_the_accounts_plan_model() {
        let follows = bot(Some(Value::Null), "xai/grok-4.6@sub", Some("medium"));
        let on_plan = account(InferenceKind::LocalProxy, Some("gpt-6-luna"));
        let pick = bot_pick(&follows, Some(&on_plan), &catalogue(SERVER), plan(PLAN));
        assert!(pick.per_bot);
        assert_eq!(pick.bot_door, None);
        assert_eq!(pick.door, Some(InferenceKind::LocalProxy));
        assert_eq!(pick.model.as_deref(), Some("gpt-6-luna"));
        assert_eq!(pick.chip_label(), "GPT-6 Luna · Medium");
        assert_eq!(
            pick.account_plan, None,
            "the Bot's own plan rows are offered"
        );
        assert_eq!(
            serde_json::to_value(pick.fast_patch(true).unwrap()).unwrap(),
            json!({"model": "gpt-6-luna--fast", "source": "local_proxy"})
        );
        // On its own plan, a pin the allowlist takes is the Bot's, and the account's model is not.
        let pinned = bot(Some(json!("local_proxy")), "gpt-5.6-sol", Some("medium"));
        let pick = bot_pick(&pinned, Some(&on_plan), &catalogue(SERVER), plan(PLAN));
        assert_eq!(pick.chip_label(), "GPT-5.6 Sol · Medium");
        // An account with no plan model and a pin it will not take: no model to name.
        let pick = bot_pick(
            &follows,
            Some(&account(InferenceKind::LocalProxy, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(pick.chip_label(), "No model · Medium");
    }

    /// On the person's plan the server asks a Bot's pin only when the Bot's own door is the plan
    /// (opengrok-server main d6f640e (#307, after #304), pin bf99845).
    /// A Bot that follows the account there runs on the account's plan model even with a pin the
    /// allowlist takes: the chip names that model, the list ticks it, and ⚡ moves to its twin
    /// with the Bot's own door, so the pick sticks. A pick of the pin's own row sends the door
    /// alone. The same Bot on its own plan runs on its pin.
    #[test]
    fn a_bot_that_follows_the_account_shows_the_plan_model_and_not_its_pin() {
        const LISTED: &[&str] = &["gpt-6-luna", "gpt-6-luna--fast", "gpt-6-sol"];
        let on_plan = account(InferenceKind::LocalProxy, Some("gpt-6-luna"));
        let follows = bot(Some(Value::Null), "gpt-6-sol", Some("medium"));
        let pick = bot_pick(&follows, Some(&on_plan), &catalogue(SERVER), plan(LISTED));
        assert_eq!(pick.door, Some(InferenceKind::LocalProxy));
        assert_eq!(pick.model.as_deref(), Some("gpt-6-luna"));
        assert_eq!(pick.chip_label(), "GPT-6 Luna · Medium");
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("gpt-6-luna")
        );
        assert_eq!(pick.fast_blocked, None, "the account's model has a twin");
        assert_eq!(
            serde_json::to_value(pick.fast_patch(true).unwrap()).unwrap(),
            json!({"model": "gpt-6-luna--fast", "source": "local_proxy"})
        );
        assert_eq!(
            serde_json::to_value(
                pick.pick_patch(InferenceKind::LocalProxy, "gpt-6-sol")
                    .unwrap()
            )
            .unwrap(),
            json!({"source": "local_proxy"}),
            "pinned to it already: its own door is what puts the Bot on it"
        );

        let own = bot(Some(json!("local_proxy")), "gpt-6-sol", Some("medium"));
        let pick = bot_pick(&own, Some(&on_plan), &catalogue(SERVER), plan(LISTED));
        assert_eq!(pick.model.as_deref(), Some("gpt-6-sol"));
        assert_eq!(pick.chip_label(), "GPT-6 Sol · Medium");
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("gpt-6-sol")
        );
        assert_eq!(pick.fast_blocked, Some(FAST_NO_TWIN));
    }

    /// Where the account's way to the plan is the person's Mac, the plan group is the models a
    /// Mac lists and the account's plan model is the relay's; a way this app cannot name offers
    /// no plan group at all rather than guess one.
    #[test]
    fn through_the_mac_the_plan_is_the_macs_models_and_model() {
        let relayed = |via: &str| InferenceSource {
            via: Some(via.into()),
            relay: Some(RelayRead {
                connected: true,
                machine_id: Some("mac_1".into()),
                machine_label: None,
                local_model: Some("grok-4.7".into()),
            }),
            ..account(InferenceKind::LocalProxy, Some("gpt-5-codex"))
        };
        let lists = |via: Via| match via {
            Via::Mac => vec!["grok-4.7".to_string(), "grok-4.7--fast".to_string()],
            Via::Loopback => vec!["gpt-5-codex".to_string()],
        };
        let follows = bot(Some(Value::Null), "oag/cheap", Some("high"));
        let pick = bot_pick(&follows, Some(&relayed("mac")), &catalogue(SERVER), lists);
        assert_eq!(pick.chip_label(), "Grok 4.7 · High");
        assert_eq!(
            ids(&pick.groups[0].rows),
            [("grok-4.7", true)],
            "the Mac's models"
        );
        assert_eq!(pick.fast_blocked, None);
        // Following the account, a pin the allowlist takes is not asked through the Mac either.
        let pinned = bot(Some(Value::Null), "gpt-6-sol", Some("high"));
        let pick = bot_pick(&pinned, Some(&relayed("mac")), &catalogue(SERVER), lists);
        assert_eq!(pick.chip_label(), "Grok 4.7 · High");

        let pick = bot_pick(
            &follows,
            Some(&relayed("helper")),
            &catalogue(SERVER),
            lists,
        );
        assert_eq!(
            pick.groups
                .iter()
                .map(|group| group.source)
                .collect::<Vec<_>>(),
            [InferenceKind::Gateway]
        );
        assert_eq!(pick.chip_label(), "No model · High");
    }

    /// From a server that keeps no effort (before opengrok-server#271) the Bot reads Default and
    /// the slider takes no stop; ↺ has only ⚡ to put back.
    #[test]
    fn a_server_that_keeps_no_effort_has_a_dead_slider() {
        let old = bot(Some(json!("local_proxy")), "gpt-6-luna--fast", None);
        let pick = bot_pick(
            &old,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert!(!pick.effort_kept);
        assert_eq!(pick.chip_label(), "GPT-6 Luna · Default ⚡");
        assert_eq!(pick.effort_patch("high"), Err(EFFORT_NOT_KEPT.to_string()));
        assert_eq!(
            serde_json::to_value(pick.reset_patch().unwrap()).unwrap(),
            json!({"model": "gpt-6-luna"})
        );
    }

    /// The server refuses every routine of a Bot whose own door is `local_proxy`, before any
    /// model call (the owner's decision: opengrok-server main d6f640e (#307, after #304), pin
    /// bf99845, whose recording holds the refusal as the routine's failed last run, in words and
    /// with no code). Such a Bot is always told its
    /// routines won't run: pinned to a plan model the gateway lacks, to one the gateway lists,
    /// fast tier or not, or to nothing; before the gateway's models are listed, and whatever the
    /// account's door.
    #[test]
    fn a_bot_on_its_own_plan_is_always_told_its_routines_wont_run() {
        assert_eq!(
            ROUTINES_ON_PLAN,
            "This Bot's routines won't run while it answers on your own plan: routines run on \
             the server's keys. Pick a Server model to run it on a schedule."
        );
        let on_plan = account(InferenceKind::LocalProxy, Some("gpt-6-luna"));
        let on_keys = account(InferenceKind::Gateway, None);
        let gateway_has_luna: &[&str] = &["gpt-6-luna", "oag/cheap"];
        for (pin, gateway, kept) in [
            ("gpt-6-luna", SERVER, Some(&on_plan)),
            ("gpt-6-luna--fast", SERVER, Some(&on_plan)),
            ("gpt-6-luna", SERVER, Some(&on_keys)),
            ("gpt-6-luna", SERVER, None),
            ("xai/grok-4.7", SERVER, Some(&on_plan)),
            ("xai/grok-4.7--fast", SERVER, Some(&on_keys)),
            ("gpt-6-luna", gateway_has_luna, Some(&on_plan)),
            ("gpt-6-luna", &[], Some(&on_plan)),
            ("", SERVER, Some(&on_plan)),
        ] {
            let own = bot(Some(json!("local_proxy")), pin, Some("medium"));
            let pick = bot_pick(&own, kept, &catalogue(gateway), plan(PLAN));
            assert_eq!(
                pick.routines.as_deref(),
                Some(ROUTINES_ON_PLAN),
                "{pin:?} against {gateway:?} with {kept:?}"
            );
        }
    }

    /// A Bot whose own door is not the plan runs its routines as it did, and is never told they
    /// won't: one that follows the account (`source: null`), even onto the plan; one on the
    /// gateway; one from a server without per-Bot doors (no `source` key); and one whose door is
    /// a word this app cannot name. Pinned to a model the gateway lacks or to one it lists.
    #[test]
    fn a_bot_not_on_its_own_plan_is_never_told_of_its_routines() {
        let on_plan = account(InferenceKind::LocalProxy, Some("gpt-6-luna"));
        let on_keys = account(InferenceKind::Gateway, None);
        for source in [
            Some(Value::Null),
            Some(json!("gateway")),
            None,
            Some(json!("byok")),
        ] {
            for pin in ["gpt-6-luna", "xai/grok-4.7"] {
                for kept in [Some(&on_plan), Some(&on_keys), None] {
                    let row = bot(source.clone(), pin, Some("medium"));
                    let pick = bot_pick(&row, kept, &catalogue(SERVER), plan(PLAN));
                    assert_eq!(pick.routines, None, "{source:?} on {pin} with {kept:?}");
                }
            }
        }
    }
}
