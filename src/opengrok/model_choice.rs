//! A Bot's model as its picker offers it: the models the server lists, folded and named for a
//! person in the two groups a Bot can be answered from; the effort slider's five stops; and what
//! each change the picker makes sends to the server.
//!
//! No GPUI. The Model card in the Bot's settings draws what is here (`components::model_picker`),
//! and the gpui-agent tree names it, so the two always agree.
//!
//! A Bot's setting is three things its row keeps: the door (`source`, opengrok-server main
//! d6f640e (#307, after #304), pin bf99845), the model it is pinned to
//! (`model`), and how hard it thinks (`effort`, opengrok-server#271). Fast is not a fourth.
//! opencodex lists a model's fast tier as a twin id, `gpt-6-luna--fast` beside `gpt-6-luna`, and
//! the server's allowlist takes the tier off before it reads the rest ([`is_subscription_model`]),
//! so ⚡ is the pin moved to the twin and back, offered only where the list holds both.
//!
//! What the card says is what the server would run the Bot's next turn on, as far as this app can
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
    ModelCatalogue, NewBotDefault, PlanFallback, Via, is_subscription_model,
};

/// What opencodex puts after a model's id for its fast tier.
pub const FAST_SUFFIX: &str = "--fast";

/// What a gateway id ends with when it is a seat billed to a subscription (`xai/grok-4.6@sub`).
/// The Gateway group is the server's paid keys, and a seat is not one, so it is not listed there.
const SEAT_SUFFIX: &str = "@sub";

/// What a gateway id ends with when it pins that same model to an API-key credential
/// (`xai/grok-4.6@api`). The unqualified id is the one the gateway routes: the cheapest live
/// credential (open-ai-gateway `docs/02-cost-routing.md`). Listing both is the same model twice,
/// so the pin is not a row of its own where the unqualified id is listed, and a pick goes to
/// that id. A pin whose unqualified id is absent stays, as the only spelling the list offers.
const API_SUFFIX: &str = "@api";

/// The Bot's list's two groups, as they read: the person's own plan, which opencodex serves
/// (`local_proxy`), and the server's paid keys, through its gateway.
pub const SUBSCRIPTION_GROUP: &str = "Subscription";
pub const GATEWAY_GROUP: &str = "Gateway";

/// Why ⚡ is dead: the list holds no fast twin of the model, or does not hold the model at all.
pub const FAST_NO_TWIN: &str = "The server lists no fast version of this model.";
/// Why ⚡ is dead: the Bot's door is a word this app cannot name, or follows an account setting
/// that has not been read, so which group the model is in cannot be told.
pub const FAST_DOOR_UNKNOWN: &str = "Where this Bot's replies go isn't known yet.";
/// Why ⚡ is dead on a server without per-Bot doors while the account is on the person's plan:
/// the plan's model there is the account's, and a Bot's pin would change nothing.
pub const FAST_ACCOUNT_PLAN: &str =
    "On this server every Bot on your plan uses your account's plan model.";
/// Why the slider is dead: a server from before opengrok-server#271 keeps no effort, and a pick
/// would look saved and change nothing.
pub const EFFORT_NOT_KEPT: &str = "This server has nowhere to keep an effort yet.";
/// What the card names while there is no model to name: a Bot with no pin, or on a plan that
/// keeps no model.
pub const NO_MODEL: &str = "No model";
/// What Default for new Bots' card names while the person has set none, and the list's row that
/// sets none: a new Bot is then left to the server's own default, the deployment's model on the
/// account's door (opengrok-server #322, on main c0bb6ae: `hire_model` in
/// `crates/opengrok-server/src/inference.rs`).
pub const NEW_BOTS_NONE: &str = "None";
/// Why ⚡ and the slider are dead in Default for new Bots while it holds none: the server keeps a
/// default whole, its model with its door and effort, and there is no model yet to go with them.
pub const NEW_BOTS_PICK_FIRST: &str = "Pick a model first: a default for new Bots starts with one.";
/// The same for the Relay-off fallback: the server keeps it whole, its model with its effort.
pub const PLAN_FALLBACK_PICK_FIRST: &str =
    "Pick a model first: a Relay-off fallback starts with one.";
/// What the list says under its rows for a Bot whose own door is the person's plan
/// ([`ModelPick::routines`]).
pub const ROUTINES_ON_PLAN: &str = "This Bot's routines run on your own plan: one that's due \
                                    while your plan can't answer is skipped.";
/// The same while the person has switched the relay off with a Relay-off fallback set, and their
/// plan goes by their computer: its routines run on that fallback (opengrok-server #332 (PR #338
/// at 66b9f7b), the server's own note for such a routine).
pub const ROUTINES_ON_FALLBACK: &str =
    "This Bot's routines run on your Server fallback model while Relay is off.";
/// And with no fallback set: every one that is due is skipped (the same note).
pub const ROUTINES_SKIPPED_RELAY_OFF: &str =
    "This Bot's routines are skipped while Relay is off for your plan.";

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

    /// The row is one a search for `query` leaves: its name as the list reads it, or its id,
    /// holds what was typed, whatever the case. Nothing typed leaves every row.
    pub fn matches(&self, query: &str) -> bool {
        holds_query(&[&self.label, &self.base_id], query)
    }
}

/// Whether any of `texts` holds `query`, whatever the case and without the spaces around what was
/// typed. Nothing typed is held by everything.
fn holds_query(texts: &[&str], query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || texts
            .iter()
            .any(|text| text.to_lowercase().contains(&query))
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
        group_title(self.source)
    }
}

/// A group's heading, by the door its models are served through.
pub fn group_title(source: InferenceKind) -> &'static str {
    match source {
        InferenceKind::LocalProxy => SUBSCRIPTION_GROUP,
        InferenceKind::Gateway => GATEWAY_GROUP,
    }
}

/// How many models the groups hold between them.
pub fn row_count(groups: &[ChoiceGroup]) -> usize {
    groups.iter().map(|group| group.rows.len()).sum()
}

/// How many models the list shows at a time. A group's heading is not one of them: it is drawn
/// over the first of its group's models in view, and the window still shows this many models.
pub const LIST_ROWS: usize = 5;

/// One line of the list's window ([`list_window`]): a group's heading, or a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListLine<'a> {
    Heading(InferenceKind),
    Row(&'a ModelChoice),
}

/// The furthest the window can start among `rows` models: where it shows the last [`LIST_ROWS`].
pub fn last_window_start(rows: usize) -> usize {
    rows.saturating_sub(LIST_ROWS)
}

/// The list's window from its `start`th model, counting through the groups in order: at most
/// [`LIST_ROWS`] models, and over the first of each group's models in view the group's heading,
/// which is not counted, so the window shows as many models wherever a group begins. A start
/// past the last whole window is taken back to it, so a list that shrank under the window still
/// fills it.
pub fn list_window(groups: &[ChoiceGroup], start: usize) -> Vec<ListLine<'_>> {
    let start = start.min(last_window_start(row_count(groups)));
    let in_view = start..start + LIST_ROWS;
    let mut lines = Vec::new();
    let mut at = 0;
    for group in groups {
        let mut headed = false;
        for row in &group.rows {
            if in_view.contains(&at) {
                if !headed {
                    lines.push(ListLine::Heading(group.source));
                    headed = true;
                }
                lines.push(ListLine::Row(row));
            }
            at += 1;
        }
    }
    lines
}

/// Where the window starts as the list opens, among `rows` models, with the one at `selected`
/// ticked: with that model in view, as near the window's middle as the list allows, and at the
/// top where none is ticked.
pub fn window_opening_on(rows: usize, selected: Option<usize>) -> usize {
    selected
        .map_or(0, |at| at.saturating_sub(LIST_ROWS / 2))
        .min(last_window_start(rows))
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

/// The Gateway group: the gateway's routes as `GET /models` lists them, in its order, less the
/// seats billed to a subscription and less an `@api` pin of a model the list also names
/// without one.
pub fn server_choices(catalogue: &ModelCatalogue) -> Vec<ModelChoice> {
    let ids: Vec<&str> = catalogue
        .models
        .iter()
        .filter(|entry| entry.source() == Some(InferenceKind::Gateway))
        .map(|entry| entry.id.as_str())
        .filter(|id| !ends_with_pin(without_fast(id), SEAT_SUFFIX))
        .collect();
    fold(
        InferenceKind::Gateway,
        ids.iter()
            .copied()
            .filter(|id| !api_pin_of_a_listed_model(id, &ids)),
    )
}

/// Whether `id` ends with a credential pin (`@sub`, `@api`), ignoring the pin's case.
fn ends_with_pin(id: &str, suffix: &str) -> bool {
    id.len() >= suffix.len() && id[id.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// An `@api` id whose unqualified model is also listed: the same model, not a second row.
fn api_pin_of_a_listed_model(id: &str, listed: &[&str]) -> bool {
    let plain = without_fast(id);
    if !ends_with_pin(plain, API_SUFFIX) {
        return false;
    }
    let unqualified = &plain[..plain.len() - API_SUFFIX.len()];
    listed
        .iter()
        .any(|other| without_fast(other).eq_ignore_ascii_case(unqualified))
}

/// The Subscription group: the person's plan's models as `AppState::plan_models` gives them for
/// the account's way to the plan, already held to the server's allowlist.
pub fn plan_choices(ids: &[String]) -> Vec<ModelChoice> {
    fold(InferenceKind::LocalProxy, ids.iter().map(String::as_str))
}

/// A model as a person reads it: `gpt-6-luna` is "GPT-6 Luna", `openai/gpt-6.1-sol` "GPT-6.1
/// Sol", `xai/grok-4.7` "Grok 4.7", `zai/glm-4.6` "GLM 4.6", `claude-3-5-haiku` "Claude 3.5
/// Haiku", and a route of the gateway's own, `oag/cheap`, "Cheap (auto)", since the gateway
/// picks the model behind it. A reseller's id, `merge/xai/grok-4.6`, reads as the maker's model.
/// A fast tier reads with ⚡ after it. Any other id is shown as it is: a name made up for a model
/// this app does not know would be a name nobody could look up.
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
    // `merge/` is a reseller (open-ai-gateway `docs/02-cost-routing.md`): the id is
    // `merge/<upstream id>`, and the upstream is the model. An upstream this app does not know
    // is left as the whole id, reseller and all, so it can be looked up.
    if let Some(upstream) = lower.strip_prefix("merge/") {
        return readable(upstream);
    }
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
        (Some("zai"), ["glm", rest @ ..]) if !rest.is_empty() => {
            Some(format!("GLM{}", spaced(rest)))
        }
        (None | Some("anthropic"), ["claude", rest @ ..]) => claude_name(rest),
        _ => None,
    }
}

/// Claude as a person says it: `claude-3-5-haiku` is "Claude 3.5 Haiku", `claude-opus-4`
/// "Claude Opus 4", `claude-sonnet-5-5` "Claude Sonnet 5.5". A snapshot date (`20250929`) is
/// not a version, and the id is left as it is so the snapshot can be looked up.
fn claude_name(rest: &[&str]) -> Option<String> {
    if rest.is_empty() || rest.iter().copied().any(is_snapshot_date) {
        return None;
    }
    let mut parts = Vec::with_capacity(rest.len());
    let mut ix = 0;
    while ix < rest.len() {
        if is_number(rest[ix]) {
            let mut version = String::new();
            while ix < rest.len() && is_number(rest[ix]) {
                if !version.is_empty() {
                    version.push('.');
                }
                version.push_str(rest[ix]);
                ix += 1;
            }
            parts.push(version);
        } else {
            parts.push(capitalised(rest[ix]));
            ix += 1;
        }
    }
    Some(format!("Claude {}", parts.join(" ")))
}

/// A version token that is only digits, so `3` and `5` in `claude-3-5-haiku` join as `3.5`.
fn is_number(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit())
}

/// A dated snapshot, `claude-sonnet-4-5-20250929`, which is not a version of the model.
fn is_snapshot_date(word: &str) -> bool {
    word.len() == 8 && is_number(word)
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
/// and what each change sends ([`bot_pick`]); or the same of Default for new Bots
/// ([`new_bots_pick`]), which the same card and popover show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPick {
    /// The Bot's id, or [`NEW_BOTS_PICK_ID`] for Default for new Bots.
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
    /// Why the slider takes no stop, when it takes none: a server that keeps no effort
    /// (opengrok-server#271), and Default for new Bots while it holds no model.
    pub effort_dead: Option<&'static str>,
    /// The list, the Subscription group first, each group only while it has rows.
    pub groups: Vec<ChoiceGroup>,
    /// The line that takes the Subscription group's place on a server without per-Bot doors,
    /// while the account is on the person's plan.
    pub account_plan: Option<AccountPlan>,
    /// The line that says where this Bot's routines run ([`ROUTINES_ON_PLAN`]). A Bot whose own
    /// door is the person's plan runs its routines on that plan, as a live turn there would, and
    /// one due while the plan cannot answer (no computer holds the relay, or the proxy does not
    /// answer) is skipped, with nothing run and the history saying why (the owner's rule of 3 Oct
    /// 2026, #334 on main 8e7387f: `routine_route` and `unreachable` in opengrok-server's
    /// `crates/opengrok-server/src/autonomy/mod.rs`; before it the server refused every routine of
    /// such a Bot). While the person has switched the relay off and the plan goes by their
    /// computer, they run on the Relay-off fallback instead ([`ROUTINES_ON_FALLBACK`]), or with
    /// none set are skipped ([`ROUTINES_SKIPPED_RELAY_OFF`]), opengrok-server #332 (PR #338 at
    /// 66b9f7b). It is said for every such Bot, whatever it is pinned to. A Bot that follows
    /// the account (`source: null`), or is on the gateway, runs its routines through the gateway
    /// on its pin as before (opengrok-server #294), and is told nothing.
    pub routines: Option<String>,
    /// What the card names while there is no model to name: [`NO_MODEL`] for a Bot, and
    /// [`NEW_BOTS_NONE`] for Default for new Bots, where none is the server's own default.
    pub unset: &'static str,
}

/// [`ModelPick::bot_id`] for Default for new Bots, which is no Bot's.
pub const NEW_BOTS_PICK_ID: &str = "new-bots";
/// [`ModelPick::bot_id`] for the Relay-off fallback, which is no Bot's either.
pub const PLAN_FALLBACK_PICK_ID: &str = "plan-fallback";

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
    let groups = choice_groups(per_bot, account, catalogue, plan_models);
    let current = current_row(&groups, door, model.as_deref());
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
    // The Bot's own door alone decides whether it is told: the routines of a Bot on its own plan
    // run on that plan, whatever its pin (opengrok-server #334), unless the relay is off.
    let routines = (bot_door == Some(InferenceKind::LocalProxy))
        .then(|| routines_on_plan(account).to_string());
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
        effort_dead: bot.effort.is_none().then_some(EFFORT_NOT_KEPT),
        groups,
        account_plan,
        routines,
        unset: NO_MODEL,
    }
}

/// Where a Bot whose own door is the person's plan runs its routines, by the account's setting as
/// last read, as the server's own note for such a routine says it (opengrok-server #332 (PR #338
/// at 66b9f7b): `note` in `crates/opengrok-tools/src/routine.rs`, after `routine_route` in
/// `crates/opengrok-server/src/autonomy/mod.rs`): on the plan, unless the plan goes by the
/// person's computer and they switched the relay off, when they run on the Relay-off fallback, or
/// with none set are skipped, every one. The switch is the computer's way's alone: a plan the
/// server reaches on its own machine never reads it.
fn routines_on_plan(account: Option<&InferenceSource>) -> &'static str {
    let relay_off = account.filter(|account| {
        account.relay_enabled == Some(false) && account_via(account) == Some(Via::Mac)
    });
    match relay_off.map(|account| &account.plan_fallback) {
        None => ROUTINES_ON_PLAN,
        Some(Some(Some(_))) => ROUTINES_ON_FALLBACK,
        Some(_) => ROUTINES_SKIPPED_RELAY_OFF,
    }
}

/// The list a picker offers: the Subscription group first, the plan's models for the account's
/// way to it, and the Gateway group, the gateway's routes; each only while it has rows. The plan's
/// models are offered only where the server keeps a door per Bot (`per_bot`): anywhere else the
/// server would run the account's plan model and ignore the pick.
fn choice_groups(
    per_bot: bool,
    account: Option<&InferenceSource>,
    catalogue: &ModelCatalogue,
    plan_models: impl Fn(Via) -> Vec<String>,
) -> Vec<ChoiceGroup> {
    let plan = match (per_bot, account.and_then(account_via)) {
        (true, Some(via)) => plan_choices(&plan_models(via)),
        _ => Vec::new(),
    };
    [
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
    .collect()
}

/// The row that is `model` through `door`: the one the list ticks.
fn current_row(
    groups: &[ChoiceGroup],
    door: Option<InferenceKind>,
    model: Option<&str>,
) -> Option<ModelChoice> {
    let (door, model) = door.zip(model)?;
    groups
        .iter()
        .filter(|group| group.source == door)
        .flat_map(|group| &group.rows)
        .find(|row| row.takes(model))
        .cloned()
}

/// Default for new Bots as the same card and popover show it (opengrok-server #322, on main
/// c0bb6ae: `NewBotDefault` in `crates/opengrok-core/src/inference.rs`):
/// the default the server keeps, `None` while the person has set none, read as a Bot on its own
/// door would be, from the same list a Bot's picker offers. A default always names its door, so
/// the plan's models are offered as they are to a Bot on a server that keeps a door per Bot, which
/// every server that keeps a default does. It is no Bot, so there are no routines to speak of.
/// With none set there is no model to tick, nor one to move to its fast twin or give an effort:
/// the server keeps a default whole, and a pick in the list is what starts one.
pub fn new_bots_pick(
    default: Option<&NewBotDefault>,
    account: Option<&InferenceSource>,
    catalogue: &ModelCatalogue,
    plan_models: impl Fn(Via) -> Vec<String>,
) -> ModelPick {
    let groups = choice_groups(true, account, catalogue, plan_models);
    let door = default.map(|default| default.source);
    let model = default
        .map(|default| default.model.clone())
        .filter(|model| !model.trim().is_empty());
    let current = current_row(&groups, door, model.as_deref());
    let fast_blocked = match (default, &current) {
        (None, _) => Some(NEW_BOTS_PICK_FIRST),
        (Some(_), Some(row)) if row.has_fast => None,
        (Some(_), _) => Some(FAST_NO_TWIN),
    };
    ModelPick {
        bot_id: NEW_BOTS_PICK_ID.to_string(),
        per_bot: true,
        bot_door: door,
        door,
        model,
        pin: default.map_or_else(String::new, |default| default.model.clone()),
        current,
        fast_blocked,
        effort: default.map_or_else(
            || EFFORT_INHERIT.to_string(),
            |default| default.effort.clone(),
        ),
        effort_dead: default.is_none().then_some(NEW_BOTS_PICK_FIRST),
        groups,
        account_plan: None,
        routines: None,
        unset: NEW_BOTS_NONE,
    }
}

/// The Relay-off fallback as the same card and popover show it (opengrok-server #332 (PR #338 at
/// 66b9f7b): `planFallback`): what a Bot on the person's plan answers with while the relay is
/// off, `None` while the person has set none. It is on the server's paid keys, so the list is the
/// Gateway group alone, whatever the plan offers; and like a default for new Bots, with none set
/// there is no model to tick, nor one to move to its fast twin or give an effort: the server
/// keeps it whole, and a pick in the list is what starts one.
pub fn plan_fallback_pick(
    fallback: Option<&PlanFallback>,
    catalogue: &ModelCatalogue,
) -> ModelPick {
    let groups: Vec<ChoiceGroup> = [ChoiceGroup {
        source: InferenceKind::Gateway,
        rows: server_choices(catalogue),
    }]
    .into_iter()
    .filter(|group| !group.rows.is_empty())
    .collect();
    let door = fallback.map(|_| InferenceKind::Gateway);
    let model = fallback
        .map(|fallback| fallback.model.clone())
        .filter(|model| !model.trim().is_empty());
    let current = current_row(&groups, door, model.as_deref());
    let fast_blocked = match (fallback, &current) {
        (None, _) => Some(PLAN_FALLBACK_PICK_FIRST),
        (Some(_), Some(row)) if row.has_fast => None,
        (Some(_), _) => Some(FAST_NO_TWIN),
    };
    ModelPick {
        bot_id: PLAN_FALLBACK_PICK_ID.to_string(),
        per_bot: true,
        bot_door: door,
        door,
        model,
        pin: fallback.map_or_else(String::new, |fallback| fallback.model.clone()),
        current,
        fast_blocked,
        effort: fallback.map_or_else(
            || EFFORT_INHERIT.to_string(),
            |fallback| fallback.effort.clone(),
        ),
        effort_dead: fallback.is_none().then_some(PLAN_FALLBACK_PICK_FIRST),
        groups,
        account_plan: None,
        routines: None,
        unset: NEW_BOTS_NONE,
    }
}

impl ModelPick {
    /// The next turn runs on a model's fast tier.
    pub fn is_fast(&self) -> bool {
        self.model.as_deref().is_some_and(is_fast)
    }

    /// The model's name as the card and the list's opener say it, ⚡ apart.
    pub fn model_label(&self) -> String {
        self.model
            .as_deref()
            .map_or_else(|| self.unset.to_string(), base_label)
    }

    /// The picker in a line, as a driver's tree names the card: "GPT-6 Luna · Medium ⚡".
    pub fn summary(&self) -> String {
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

    /// The list as a search for `query` leaves it, the Subscription group first: each group with
    /// only the models the search leaves ([`ModelChoice::matches`]), and no group with none.
    /// Nothing typed leaves the whole list.
    pub fn search(&self, query: &str) -> Vec<ChoiceGroup> {
        self.groups
            .iter()
            .map(|group| ChoiceGroup {
                source: group.source,
                rows: group
                    .rows
                    .iter()
                    .filter(|row| row.matches(query))
                    .cloned()
                    .collect(),
            })
            .filter(|group| !group.rows.is_empty())
            .collect()
    }

    /// The account's plan line ([`Self::account_plan`]) while a search for `query` leaves it: the
    /// model's name as the line reads it, or its id, holds what was typed. It is no row to pick,
    /// but it stands where the Subscription group would, and goes as the group's rows would.
    pub fn plan_line(&self, query: &str) -> Option<&AccountPlan> {
        self.account_plan.as_ref().filter(|plan| {
            let id = plan.model.as_deref().unwrap_or_default();
            let name = plan
                .model
                .as_deref()
                .map_or_else(|| NO_MODEL.to_string(), base_label);
            holds_query(&[&name, id], query)
        })
    }

    /// Where the list's window starts as it opens, before anything is typed: with the model that
    /// answers in view ([`window_opening_on`]).
    pub fn opening_window_start(&self) -> usize {
        let selected = self.rows().position(|row| self.is_current(row));
        window_opening_on(self.rows().count(), selected)
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
        if let Some(why) = self.effort_dead {
            return Err(why.to_string());
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
        let effort = (self.effort_dead.is_none() && self.effort != EFFORT_INHERIT)
            .then(|| EFFORT_INHERIT.to_string());
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

    /// What a change the picker makes, as a Bot's patch would carry it, makes of Default for new
    /// Bots: the whole default, the patch's door, model and effort over the ones kept, since the
    /// server keeps the default whole and takes it whole (`apply` in opengrok-server #322's
    /// `crates/opengrok-harness/src/local_proxy.rs`, on main c0bb6ae). `None` while that names no
    /// door or no model, which a default cannot be without.
    pub fn new_bots_default(&self, patch: &CoworkerPatch) -> Option<NewBotDefault> {
        let source = patch.source.or(self.bot_door)?;
        let model = patch.model.clone().unwrap_or_else(|| self.pin.clone());
        if model.trim().is_empty() {
            return None;
        }
        let effort = patch.effort.clone().unwrap_or_else(|| self.effort.clone());
        Some(NewBotDefault {
            source,
            model,
            effort,
        })
    }

    /// What a change the Relay-off fallback's picker makes, as a Bot's patch would carry it,
    /// makes of the fallback: the whole of it, the patch's model and effort over the ones kept,
    /// since the server takes it whole. `None` while that names no model, or a door other than the
    /// Gateway, which a fallback on the server's paid keys cannot be.
    pub fn plan_fallback(&self, patch: &CoworkerPatch) -> Option<PlanFallback> {
        if patch
            .source
            .is_some_and(|source| source != InferenceKind::Gateway)
        {
            return None;
        }
        let model = patch.model.clone().unwrap_or_else(|| self.pin.clone());
        if model.trim().is_empty() {
            return None;
        }
        let effort = patch.effort.clone().unwrap_or_else(|| self.effort.clone());
        Some(PlanFallback { model, effort })
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
            ..Default::default()
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
            new_bot_default: None,
            relay_enabled: None,
            plan_fallback: None,
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

    /// Ids read as a person says the model's name, and any other id as it is. A model the gateway
    /// reaches through a reseller whose own ids carry the maker reads as the maker's model.
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
            ("zai/glm-4.6", "GLM 4.6"),
            ("anthropic/claude-opus-4", "Claude Opus 4"),
            ("claude-3-5-haiku", "Claude 3.5 Haiku"),
            // The gateway's routes through a reseller, as its list names them.
            ("merge/zai/glm-5.3-flash", "GLM 5.3 Flash"),
            ("merge/xai/grok-4.6", "Grok 4.6"),
            ("merge/anthropic/claude-sonnet-5-5", "Claude Sonnet 5.5"),
            ("merge/openai/gpt-6-luna", "GPT-6 Luna"),
            // Nobody's family this app knows: as it is.
            ("o3-mini", "o3-mini"),
            ("google/gemini-3-pro", "google/gemini-3-pro"),
            (
                "merge/mistral/mistral-large-3",
                "merge/mistral/mistral-large-3",
            ),
            ("xai/grok-4.6@sub", "xai/grok-4.6@sub"),
            ("xai/gpt-5", "xai/gpt-5"),
            ("merge/zai/grok-4.6", "merge/zai/grok-4.6"),
            // A snapshot's date is no version of the model's.
            (
                "anthropic/claude-sonnet-4-5-20250929",
                "anthropic/claude-sonnet-4-5-20250929",
            ),
            ("gpt-oss-120b", "gpt-oss-120b"),
            ("grok", "grok"),
            ("claude", "claude"),
            ("", ""),
        ] {
            assert_eq!(model_label(id), read, "{id:?}");
        }
        assert_eq!(base_label("gpt-6-luna--fast"), "GPT-6 Luna");
    }

    /// `@api` is the same model pinned to an API key. The unqualified id is the one the gateway
    /// routes, so the two are one row, that id, wherever the pin sits in the list. A pin with no
    /// unqualified id beside it stays, and a seat still does not.
    #[test]
    fn an_api_key_pin_of_a_listed_model_is_that_models_row() {
        let listed = catalogue(&[
            "xai/grok-4.6@api",
            "xai/grok-4.6",
            "xai/grok-4.6@sub",
            "anthropic/claude-sonnet-5-5@api",
            "XAI/GROK-4.7@API",
            "xai/grok-4.7",
        ]);
        assert_eq!(
            ids(&server_choices(&listed)),
            [
                ("xai/grok-4.6", false),
                ("anthropic/claude-sonnet-5-5@api", false),
                ("xai/grok-4.7", false),
            ]
        );
    }

    /// The Gateway group is the gateway's routes, in the server's order, less the seats billed
    /// to a subscription, and never one of the plan's models.
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
            ..Default::default()
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

    /// The list's two groups are named for whose they are, exactly: "Subscription", the person's
    /// own plan through opencodex (`local_proxy`), and "Gateway", the server's paid keys; the
    /// Subscription group first.
    #[test]
    fn the_groups_are_subscription_and_gateway() {
        assert_eq!(
            [
                group_title(InferenceKind::LocalProxy),
                group_title(InferenceKind::Gateway)
            ],
            ["Subscription", "Gateway"]
        );
        let own = bot(Some(json!("local_proxy")), "gpt-6-luna", Some("medium"));
        let pick = bot_pick(
            &own,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(
            pick.groups
                .iter()
                .map(ChoiceGroup::title)
                .collect::<Vec<_>>(),
            [SUBSCRIPTION_GROUP, GATEWAY_GROUP]
        );
    }

    /// A Bot on its own plan whose plan lists the same Grok as the gateway, for a search to find
    /// in both groups.
    fn searched_pick() -> ModelPick {
        let own = bot(Some(json!("local_proxy")), "gpt-6-luna", Some("medium"));
        bot_pick(
            &own,
            Some(&account(InferenceKind::Gateway, None)),
            &catalogue(&["oag/cheap", "xai/grok-4.7"]),
            plan(&["gpt-6-luna", "gpt-6-luna--fast", "gpt-5.6-sol", "grok-4.7"]),
        )
    }

    /// The search filters both groups at once, whatever the case, by the name a row reads as and
    /// by its raw id; a group the search leaves nothing of goes, heading and all; and nothing
    /// typed, or only spaces, leaves the whole list.
    #[test]
    fn a_search_filters_both_groups_by_name_and_id_whatever_the_case() {
        let pick = searched_pick();
        let found = |query: &str| -> Vec<(InferenceKind, Vec<String>)> {
            pick.search(query)
                .into_iter()
                .map(|group| {
                    (
                        group.source,
                        group.rows.into_iter().map(|row| row.base_id).collect(),
                    )
                })
                .collect()
        };
        let both = vec![
            (InferenceKind::LocalProxy, vec!["grok-4.7".to_string()]),
            (InferenceKind::Gateway, vec!["xai/grok-4.7".to_string()]),
        ];
        assert_eq!(found("grok"), both, "by name, in both groups at once");
        assert_eq!(found("GROK 4.7"), both, "whatever the case");
        assert_eq!(
            found("xai/"),
            [(InferenceKind::Gateway, vec!["xai/grok-4.7".to_string()])],
            "by the raw id, which the name does not hold"
        );
        assert_eq!(
            found("Luna"),
            [(InferenceKind::LocalProxy, vec!["gpt-6-luna".to_string()])]
        );
        assert_eq!(
            found("cheap (auto)"),
            [(InferenceKind::Gateway, vec!["oag/cheap".to_string()])]
        );
        assert_eq!(
            found("gpt"),
            [(
                InferenceKind::LocalProxy,
                vec!["gpt-6-luna".to_string(), "gpt-5.6-sol".to_string()]
            )],
            "a group with nothing left goes"
        );
        assert!(found("claude").is_empty());
        for nothing in ["", "   "] {
            assert_eq!(pick.search(nothing), pick.groups, "{nothing:?}");
        }
    }

    /// A list of `plan` Subscription models and `keys` Gateway ones, named by their places.
    fn long_list(plan: usize, keys: usize) -> Vec<ChoiceGroup> {
        let rows = |source, count: usize, word: &str| -> Vec<ModelChoice> {
            (0..count)
                .map(|at| ModelChoice {
                    source,
                    base_id: format!("{word}-{at}"),
                    has_fast: false,
                    label: format!("{word} {at}"),
                })
                .collect()
        };
        vec![
            ChoiceGroup {
                source: InferenceKind::LocalProxy,
                rows: rows(InferenceKind::LocalProxy, plan, "s"),
            },
            ChoiceGroup {
                source: InferenceKind::Gateway,
                rows: rows(InferenceKind::Gateway, keys, "g"),
            },
        ]
    }

    /// The window shows five models at a time, whatever the headings: each group's heading is
    /// drawn over its first model in view, the group's first or not, and is never one of the
    /// five. A start past the last whole window shows the last five.
    #[test]
    fn the_window_shows_five_models_and_its_headings_are_not_among_them() {
        let groups = long_list(4, 6);
        let shown = |start: usize| -> Vec<String> {
            list_window(&groups, start)
                .into_iter()
                .map(|line| match line {
                    ListLine::Heading(source) => group_title(source).to_string(),
                    ListLine::Row(row) => row.base_id.clone(),
                })
                .collect()
        };
        assert_eq!(LIST_ROWS, 5);
        assert_eq!(
            shown(0),
            ["Subscription", "s-0", "s-1", "s-2", "s-3", "Gateway", "g-0"]
        );
        assert_eq!(
            shown(3),
            ["Subscription", "s-3", "Gateway", "g-0", "g-1", "g-2", "g-3"]
        );
        assert_eq!(
            shown(5),
            ["Gateway", "g-1", "g-2", "g-3", "g-4", "g-5"],
            "headed by its group, though not at the group's first model"
        );
        assert_eq!(shown(9), shown(5), "the last whole window");
        for start in 0..=9 {
            let models = list_window(&groups, start)
                .into_iter()
                .filter(|line| matches!(line, ListLine::Row(_)))
                .count();
            assert_eq!(models, LIST_ROWS, "from {start}");
        }
        // A list of five or fewer shows them all, from the top, wherever the window was.
        let short = long_list(2, 1);
        assert_eq!(list_window(&short, 4), list_window(&short, 0));
        assert_eq!(
            list_window(&short, 0).len(),
            5,
            "three models, two headings"
        );
        assert_eq!(last_window_start(3), 0);
        assert!(list_window(&[], 0).is_empty());
    }

    /// The list opens with the model that answers in view, as near the window's middle as the
    /// list allows, and at the top where none answers.
    #[test]
    fn the_list_opens_with_the_ticked_model_in_view() {
        assert_eq!(window_opening_on(10, None), 0);
        assert_eq!(window_opening_on(10, Some(0)), 0);
        assert_eq!(window_opening_on(10, Some(1)), 0);
        assert_eq!(window_opening_on(10, Some(4)), 2, "in the middle");
        assert_eq!(
            window_opening_on(10, Some(9)),
            5,
            "at the bottom of the last"
        );
        assert_eq!(window_opening_on(4, Some(3)), 0, "all in view");

        // A Bot on the eighth of nine gateway models opens on a window that holds it.
        let ids: Vec<String> = (0..9).map(|at| format!("oag/route-{at}")).collect();
        let listed: Vec<&str> = ids.iter().map(String::as_str).collect();
        let pinned = bot(Some(json!("gateway")), "oag/route-7", Some("medium"));
        let pick = bot_pick(&pinned, None, &catalogue(&listed), plan(PLAN));
        let start = pick.opening_window_start();
        assert_eq!(start, 4);
        let groups = pick.search("");
        let in_view: Vec<&str> = list_window(&groups, start)
            .into_iter()
            .filter_map(|line| match line {
                ListLine::Row(row) => Some(row.base_id.as_str()),
                ListLine::Heading(_) => None,
            })
            .collect();
        assert!(in_view.contains(&"oag/route-7"), "{in_view:?}");
    }

    /// On a server without per-Bot doors the account's plan line stands where the Subscription
    /// group would, and a search leaves it or takes it away as it would the group's rows.
    #[test]
    fn a_search_takes_the_accounts_plan_line_as_it_would_a_row() {
        let old = bot(None, "oag/cheap", Some("medium"));
        let on_plan = account(InferenceKind::LocalProxy, Some("gpt-5-codex"));
        let pick = bot_pick(&old, Some(&on_plan), &catalogue(SERVER), plan(PLAN));
        assert!(pick.plan_line("").is_some());
        assert!(pick.plan_line("codex").is_some(), "by its name");
        assert!(pick.plan_line("GPT-5-CODEX").is_some(), "by its id");
        assert!(pick.plan_line("cheap").is_none());
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

    /// A Bot on its own plan with a model the list has a twin of: the card names the model, the
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
        assert_eq!(pick.summary(), "GPT-6 Luna · Medium");
        assert_eq!(pick.fast_blocked, None);
        assert_eq!(
            pick.groups
                .iter()
                .map(ChoiceGroup::title)
                .collect::<Vec<_>>(),
            [SUBSCRIPTION_GROUP, GATEWAY_GROUP]
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
        assert_eq!(pick.summary(), "GPT-6 Luna · Ultra ⚡");
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
    /// it has none, and a Gateway model has no ⚡ to offer.
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

        // On a Gateway model: no twin in the list, so ⚡ is dead and says why.
        let keys = bot(Some(json!("gateway")), "xai/grok-4.7", Some("high"));
        let pick = bot_pick(
            &keys,
            Some(&account(InferenceKind::LocalProxy, Some("gpt-6-luna"))),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(pick.door, Some(InferenceKind::Gateway));
        assert_eq!(pick.summary(), "Grok 4.7 · High");
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
        assert_eq!(pick.summary(), "GPT-6 Luna · Default");
        assert!(!pick.can_reset());
        let effort_only = bot(Some(json!("gateway")), "oag/cheap", Some("low"));
        let pick = bot_pick(&effort_only, None, &catalogue(SERVER), plan(PLAN));
        assert_eq!(
            serde_json::to_value(pick.reset_patch().unwrap()).unwrap(),
            json!({"effort": "inherit"})
        );
    }

    /// A server whose rows carry no `source` keeps no door per Bot. The list offers the Server
    /// group alone and no pick sends a door; while the account is on the person's plan, the card
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
        assert_eq!(pick.summary(), "GPT-5 Codex · Medium");
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
        assert_eq!(pick.summary(), "Cheap (auto) · Medium");
        assert_eq!(pick.account_plan, None);
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("oag/cheap")
        );

        // The account's door not read yet: the pin, and ⚡ cannot tell its group.
        let pick = bot_pick(&old, None, &catalogue(SERVER), plan(PLAN));
        assert_eq!(pick.summary(), "Cheap (auto) · Medium");
        assert_eq!(pick.fast_blocked, Some(FAST_DOOR_UNKNOWN));
    }

    /// On the person's plan a Bot that follows the account runs on the account's plan model, a
    /// gateway pin as much as any, and the card says so; ⚡ and a pick then pin that model with its
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
        assert_eq!(pick.summary(), "GPT-6 Luna · Medium");
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
        assert_eq!(pick.summary(), "GPT-5.6 Sol · Medium");
        // An account with no plan model and a pin it will not take: no model to name.
        let pick = bot_pick(
            &follows,
            Some(&account(InferenceKind::LocalProxy, None)),
            &catalogue(SERVER),
            plan(PLAN),
        );
        assert_eq!(pick.summary(), "No model · Medium");
    }

    /// On the person's plan the server asks a Bot's pin only when the Bot's own door is the plan
    /// (opengrok-server main d6f640e (#307, after #304), pin bf99845).
    /// A Bot that follows the account there runs on the account's plan model even with a pin the
    /// allowlist takes: the card names that model, the list ticks it, and ⚡ moves to its twin
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
        assert_eq!(pick.summary(), "GPT-6 Luna · Medium");
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
        assert_eq!(pick.summary(), "GPT-6 Sol · Medium");
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("gpt-6-sol")
        );
        assert_eq!(pick.fast_blocked, Some(FAST_NO_TWIN));
    }

    /// Where the account's way to the plan is the person's Mac, the Subscription group is the
    /// models a Mac lists and the account's plan model is the relay's; a way this app cannot name
    /// offers no Subscription group at all rather than guess one.
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
        assert_eq!(pick.summary(), "Grok 4.7 · High");
        assert_eq!(
            ids(&pick.groups[0].rows),
            [("grok-4.7", true)],
            "the Mac's models"
        );
        assert_eq!(pick.fast_blocked, None);
        // Following the account, a pin the allowlist takes is not asked through the Mac either.
        let pinned = bot(Some(Value::Null), "gpt-6-sol", Some("high"));
        let pick = bot_pick(&pinned, Some(&relayed("mac")), &catalogue(SERVER), lists);
        assert_eq!(pick.summary(), "Grok 4.7 · High");

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
        assert_eq!(pick.summary(), "No model · High");
    }

    /// Default for new Bots is the Bot's picker over the same list, read as a Bot on its own door
    /// would be (opengrok-server #322, on main c0bb6ae): set, the card names its model, door,
    /// effort and ⚡, the list ticks its row, and every change makes the whole default anew, the
    /// kept door, model and effort under the change, since the server keeps it whole. With none
    /// set the card says None, nothing is ticked, ⚡ and the slider are dead and say why, and a
    /// pick of a row starts a whole default, on its door and the Default effort.
    #[test]
    fn the_default_for_new_bots_is_picked_whole_in_the_bots_picker() {
        let kept = NewBotDefault {
            source: InferenceKind::LocalProxy,
            model: "gpt-6-luna--fast".into(),
            effort: "high".into(),
        };
        let default = |source, model: &str, effort: &str| {
            Some(NewBotDefault {
                source,
                model: model.into(),
                effort: effort.into(),
            })
        };
        let on_keys = account(InferenceKind::Gateway, None);
        let pick = new_bots_pick(Some(&kept), Some(&on_keys), &catalogue(SERVER), plan(PLAN));
        assert_eq!(pick.summary(), "GPT-6 Luna · High ⚡");
        assert_eq!(pick.door, Some(InferenceKind::LocalProxy));
        assert_eq!(
            pick.groups
                .iter()
                .map(ChoiceGroup::title)
                .collect::<Vec<_>>(),
            [SUBSCRIPTION_GROUP, GATEWAY_GROUP]
        );
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("gpt-6-luna")
        );
        assert_eq!((pick.routines.as_deref(), pick.effort_dead), (None, None));
        let whole = |patch: Option<CoworkerPatch>| pick.new_bots_default(&patch.expect("a change"));
        assert_eq!(
            whole(pick.fast_patch(false).unwrap()),
            default(InferenceKind::LocalProxy, "gpt-6-luna", "high"),
            "⚡ off: the plain twin, on the kept door and effort"
        );
        assert_eq!(
            whole(
                pick.pick_patch(InferenceKind::Gateway, "xai/grok-4.7")
                    .unwrap()
            ),
            default(InferenceKind::Gateway, "xai/grok-4.7", "high"),
            "another door's model, with the kept effort"
        );
        assert_eq!(
            whole(pick.effort_patch("low").unwrap()),
            default(InferenceKind::LocalProxy, "gpt-6-luna--fast", "low")
        );
        assert_eq!(
            whole(pick.reset_patch()),
            default(InferenceKind::LocalProxy, "gpt-6-luna", "inherit")
        );

        let none = new_bots_pick(None, Some(&on_keys), &catalogue(SERVER), plan(PLAN));
        assert_eq!(none.summary(), "None · Default");
        assert_eq!(none.model_label(), NEW_BOTS_NONE);
        assert!(none.current.is_none() && none.door.is_none());
        assert_eq!(none.fast_blocked, Some(NEW_BOTS_PICK_FIRST));
        assert_eq!(none.effort_dead, Some(NEW_BOTS_PICK_FIRST));
        assert!(none.effort_patch("high").is_err());
        assert!(!none.can_reset());
        let first = none
            .pick_patch(InferenceKind::LocalProxy, "gpt-5.6-sol")
            .unwrap()
            .expect("a change");
        assert_eq!(
            none.new_bots_default(&first),
            default(InferenceKind::LocalProxy, "gpt-5.6-sol", "inherit")
        );
        assert_eq!(
            none.new_bots_default(&CoworkerPatch {
                effort: Some("high".into()),
                ..Default::default()
            }),
            None,
            "an effort alone makes no default"
        );
    }

    /// The Relay-off fallback is the Bot's picker over the Gateway group alone (opengrok-server
    /// #332 (PR #338 at 66b9f7b)), whatever else the list of models holds: it is what a Bot on
    /// the plan answers with on the server's paid keys. Set, the card names its model and effort,
    /// the list ticks its row, and every change makes the whole fallback anew. With none set the card says None, ⚡ and the slider are dead and say why,
    /// and a pick starts a whole fallback on the Default effort. The plan's door makes none.
    #[test]
    fn the_relay_off_fallback_is_picked_whole_from_the_gateway_alone() {
        let fallback = |model: &str, effort: &str| {
            Some(PlanFallback {
                model: model.into(),
                effort: effort.into(),
            })
        };
        let mut listed = catalogue(SERVER);
        listed
            .models
            .push(entry("gpt-6-luna", "local_proxy", Some("mac")));
        let kept = fallback("xai/grok-4.7", "high");
        let pick = plan_fallback_pick(kept.as_ref(), &listed);
        assert_eq!(
            pick.groups
                .iter()
                .map(ChoiceGroup::title)
                .collect::<Vec<_>>(),
            [GATEWAY_GROUP]
        );
        assert!(pick.rows().all(|row| row.source == InferenceKind::Gateway));
        assert_eq!(pick.summary(), "Grok 4.7 · High");
        assert_eq!(pick.door, Some(InferenceKind::Gateway));
        assert_eq!(
            pick.current.as_ref().map(|row| row.base_id.as_str()),
            Some("xai/grok-4.7")
        );
        assert!(
            pick.pick_patch(InferenceKind::LocalProxy, "gpt-6-luna")
                .is_err(),
            "the plan's models are not offered"
        );
        let whole = |patch: Option<CoworkerPatch>| pick.plan_fallback(&patch.expect("a change"));
        assert_eq!(
            whole(
                pick.pick_patch(InferenceKind::Gateway, "oag/cheap")
                    .unwrap()
            ),
            fallback("oag/cheap", "high")
        );
        assert_eq!(
            whole(pick.effort_patch("low").unwrap()),
            fallback("xai/grok-4.7", "low")
        );
        assert_eq!(
            pick.plan_fallback(&CoworkerPatch {
                source: Some(InferenceKind::LocalProxy),
                model: Some("gpt-6-luna".into()),
                ..Default::default()
            }),
            None,
            "never the plan"
        );

        let none = plan_fallback_pick(None, &listed);
        assert_eq!(none.summary(), "None · Default");
        assert_eq!(
            (none.fast_blocked, none.effort_dead),
            (
                Some(PLAN_FALLBACK_PICK_FIRST),
                Some(PLAN_FALLBACK_PICK_FIRST)
            )
        );
        let first = none
            .pick_patch(InferenceKind::Gateway, "oag/cheap")
            .unwrap()
            .expect("a change");
        assert_eq!(none.plan_fallback(&first), fallback("oag/cheap", "inherit"));
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
        assert_eq!(pick.effort_dead, Some(EFFORT_NOT_KEPT));
        assert_eq!(pick.summary(), "GPT-6 Luna · Default ⚡");
        assert_eq!(pick.effort_patch("high"), Err(EFFORT_NOT_KEPT.to_string()));
        assert_eq!(
            serde_json::to_value(pick.reset_patch().unwrap()).unwrap(),
            json!({"model": "gpt-6-luna"})
        );
    }

    /// A Bot whose own door is `local_proxy` runs its routines on the person's plan, and one due
    /// while the plan cannot answer is skipped (#334 on main 8e7387f, whose recording holds a
    /// plan Bot's routine run through the Mac and one skipped with no Mac). Such a Bot is always
    /// told so: pinned to a plan model the gateway lacks, to one the gateway lists, fast tier or
    /// not, or to nothing; before the gateway's models are listed, and whatever the account's
    /// door. It is not told its routines won't run: the server no longer refuses them.
    #[test]
    fn a_bot_on_its_own_plan_is_told_its_routines_run_there_and_may_be_skipped() {
        assert_eq!(
            ROUTINES_ON_PLAN,
            "This Bot's routines run on your own plan: one that's due while your plan can't answer \
             is skipped."
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

    /// While the person has switched the relay off and their plan goes by their computer, a Bot
    /// on its own plan runs its routines on the Relay-off fallback, or with none set every one is
    /// skipped (opengrok-server #332 (PR #338 at 66b9f7b), whose recording holds a plan Bot's
    /// routine run on the fallback and one skipped with none), and its list says so, in the words
    /// of the server's own note for such a routine (`note` in
    /// `crates/opengrok-tools/src/routine.rs`). Read off the settings the server recorded for
    /// those two. With the relay on, or the plan on the server's own machine, which never reads
    /// the switch, they run on the plan as before; and a Bot not on its own plan is told nothing.
    #[test]
    fn a_bot_on_its_own_plan_is_told_where_its_routines_run_while_relay_is_off() {
        assert_eq!(
            (ROUTINES_ON_FALLBACK, ROUTINES_SKIPPED_RELAY_OFF),
            (
                "This Bot's routines run on your Server fallback model while Relay is off.",
                "This Bot's routines are skipped while Relay is off for your plan."
            )
        );
        let recorded = |fixture: &str| -> InferenceSource {
            let recorded: Value = serde_json::from_str(fixture).expect("the recording");
            serde_json::from_value(recorded["body"].clone()).expect("a setting")
        };
        let on_fallback = recorded(include_str!(
            "../../fixtures/wire/rest/PUT__account_inference-source/200-a_plan_bots_routine_runs_on_the_fallback_while_the_relay_is_off.json"
        ));
        let none_set = recorded(include_str!(
            "../../fixtures/wire/rest/PUT__account_inference-source/200-a_plan_bots_routine_is_skipped_while_the_relay_is_off_with_no_fallback.json"
        ));
        let own = bot(Some(json!("local_proxy")), "gpt-6-luna", Some("medium"));
        let routines = |kept: &InferenceSource, row: &Coworker| {
            bot_pick(row, Some(kept), &catalogue(SERVER), plan(PLAN)).routines
        };
        assert_eq!(
            routines(&on_fallback, &own).as_deref(),
            Some(ROUTINES_ON_FALLBACK)
        );
        assert_eq!(
            routines(&none_set, &own).as_deref(),
            Some(ROUTINES_SKIPPED_RELAY_OFF)
        );
        let on = InferenceSource {
            relay_enabled: Some(true),
            ..on_fallback.clone()
        };
        assert_eq!(routines(&on, &own).as_deref(), Some(ROUTINES_ON_PLAN));
        let loopback = InferenceSource {
            via: Some("loopback".into()),
            ..none_set.clone()
        };
        assert_eq!(
            routines(&loopback, &own).as_deref(),
            Some(ROUTINES_ON_PLAN),
            "the switch is the computer's way's alone"
        );
        for source in [Some(Value::Null), Some(json!("gateway"))] {
            let row = bot(source.clone(), "gpt-6-luna", Some("medium"));
            assert_eq!(routines(&on_fallback, &row), None, "{source:?}");
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