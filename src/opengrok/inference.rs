//! Where a reply's model calls are paid from: the server's paid keys, or the person's own
//! subscription.
//!
//! The app never calls a model, whichever it is. The fork is in opengrok-server's model door:
//! the harness, the tools and the journal stay on the server either way, and when the source is
//! `local_proxy` it is the server that talks to opencodex, the proxy that holds the person's
//! sign-in, over the server's own loopback. So opencodex runs on the same machine as the server,
//! and the person's plan can be set up only from a Mac that is that machine ([`is_loopback`]).
//! What lives here is the cockpit's half of that: the account's setting (`GET` and
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
//! `fixtures/wire/` from its main at e55a8c8 (pin f56bbde, after #296).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The CUSTOM `name` the server sends right after `RUN_STARTED`, with `value {"kind", "model"}`:
/// which door this run's model calls go through, and the model. It is journaled with the run, so
/// a thread's replay carries it as the live stream did.
pub const INFERENCE_SOURCE_CUSTOM: &str = "opengrok.inferenceSource";

/// Where opencodex listens unless the person says otherwise. The server follows no redirects and
/// refuses anything that is not literal loopback, so this is also the shape a URL has to have.
pub const DEFAULT_PROXY_URL: &str = "http://127.0.0.1:8080";

/// Whether an address is the machine it is dialled from, as the server reads one: `127.0.0.0/8`,
/// `[::1]` or the name `localhost`, with or without a port (`loopback_base` in
/// `crates/opengrok-harness/src/local_proxy.rs`). The app asks it of its own server's address:
/// the server calls opencodex on its own loopback, so the person's plan is set up from this Mac
/// only when the server runs on it.
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

    /// The other door: what the composer's chip turns to when it is clicked.
    pub fn other(self) -> Self {
        match self {
            Self::Gateway => Self::LocalProxy,
            Self::LocalProxy => Self::Gateway,
        }
    }
}

/// The account's reply source, as `GET /account/inference-source` answers and as `PUT` answers
/// with once it has kept a change: `{"kind", "baseUrl", "localModel", "healthy", "hasApiKey"}`,
/// with `baseUrl` and `localModel` `null` while none is set.
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
}

/// A `PUT /account/inference-source` body: `{"kind", "baseUrl"?, "localModel"?, "apiKey"?}`.
///
/// `kind` is always there. Each other field is there only when the page changes it, and the
/// server reads a field three ways (`apply` in `crates/opengrok-harness/src/local_proxy.rs`):
/// absent keeps what it has, `null` (or `""`) clears it, and a value replaces it. So each is
/// `None` to leave out, `Some(None)` for `null`, and `Some(Some(_))` for a value; `apiKey: null`
/// is Remove key.
///
/// The server keeps it or refuses the whole of it with a 400 and `{"error": sentence}`, and asks
/// every field that is there whatever the kind: a `baseUrl` that is not literal loopback (it
/// follows no redirects), or a `localModel` from a provider it will not route a subscription to.
/// It routes only OpenAI/Codex and xAI/Grok models that way, and fails closed on anything else.
///
/// Not `Clone`: it can hold the key.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceSourceUpdate {
    pub kind: InferenceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_model: Option<Option<String>>,
    /// Sent once, when the person typed one, and kept by the server: `hasApiKey` is all that
    /// ever comes back. `Some(None)` asks the server to forget the one it has; absent leaves it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<Option<ProxyKey>>,
}

/// A key for the proxy as the person typed it, on its way to the server. This app keeps it
/// nowhere but the page it was typed on, until it is sent in one `PUT` or the page is left: it is
/// never cloned (the type is not `Clone`), and its `Debug` is a mark, so no log or panic message
/// ever carries it.
#[derive(PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ProxyKey(String);

impl ProxyKey {
    /// A key, or `None` for a field left blank, which is not a key and must not replace one.
    pub fn new(typed: &str) -> Option<Self> {
        let typed = typed.trim();
        (!typed.is_empty()).then(|| Self(typed.to_string()))
    }

    /// The key, for the one field that shows it, masked: a key a driver wrote is drawn there as
    /// the dots a typed one is.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ProxyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProxyKey(«redacted»)")
    }
}

/// Which door one reply came through, as the run's `opengrok.inferenceSource` CUSTOM says:
/// `value {"kind", "model"}`. It is what the reply's badge shows, live and when the thread is
/// read back, and what the reply's row keeps on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplySource {
    pub kind: InferenceKind,
    /// The model that answered, when the server named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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

    /// `{"kind", "model"}`, as the frame's `value` and a saved row both hold it.
    pub fn from_value(value: &Value) -> Option<Self> {
        let kind = InferenceKind::from_word(value.get("kind")?.as_str()?)?;
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string);
        Some(Self { kind, model })
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

/// Whether "My subscription" may be pointed at a model, by the server's own rule, so the picker
/// never offers what a Save would be refused for, even should a list ever carry one (a list held
/// from before, or a server that tags a row `local_proxy` without asking): `subscription_model`
/// in opengrok-server's `crates/opengrok-core/src/inference.rs`, anchored since #296, at the
/// vendored pin f56bbde.
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
        assert_eq!(InferenceKind::Gateway.other(), InferenceKind::LocalProxy);
        assert_eq!(InferenceKind::LocalProxy.other(), InferenceKind::Gateway);
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

    /// A `PUT` body names the door and only what the page changed: a value to keep, `null` to
    /// clear, and nothing for a field left alone, which the server keeps. The key rides along
    /// only when one was typed, and `null` is Remove key. No `Debug` of the body, or of the key,
    /// ever prints the key.
    #[test]
    fn a_put_body_carries_what_changed_and_null_clears() {
        let full = InferenceSourceUpdate {
            kind: InferenceKind::LocalProxy,
            base_url: Some(Some("http://127.0.0.1:8080".into())),
            local_model: Some(Some("gpt-5-codex".into())),
            api_key: Some(ProxyKey::new("  sk-proxy-1  ")),
        };
        assert_eq!(
            serde_json::to_value(&full).unwrap(),
            json!({
                "kind": "local_proxy",
                "baseUrl": "http://127.0.0.1:8080",
                "localModel": "gpt-5-codex",
                "apiKey": "sk-proxy-1"
            })
        );
        let printed = format!("{full:?}");
        assert!(!printed.contains("sk-proxy-1"), "{printed}");
        assert!(printed.contains("redacted"), "{printed}");
        let bare = InferenceSourceUpdate {
            kind: InferenceKind::Gateway,
            base_url: None,
            local_model: None,
            api_key: None,
        };
        assert_eq!(
            serde_json::to_value(&bare).unwrap(),
            json!({"kind": "gateway"}),
            "a field left alone is left out"
        );
        let cleared = InferenceSourceUpdate {
            kind: InferenceKind::Gateway,
            base_url: Some(None),
            local_model: Some(None),
            api_key: Some(None),
        };
        assert_eq!(
            serde_json::to_value(&cleared).unwrap(),
            json!({"kind": "gateway", "baseUrl": null, "localModel": null, "apiKey": null})
        );
        assert_eq!(ProxyKey::new("   "), None, "a blank field is no key");
    }

    /// The app's own server is on this Mac when its address is loopback as the server reads one:
    /// any of 127.0.0.0/8, `[::1]`, or `localhost`, with or without a port. A name that only
    /// starts with `localhost`, a LAN address or `0.0.0.0` is somewhere else.
    #[test]
    fn this_mac_is_the_servers_machine_only_at_a_loopback_address() {
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
            })
        );
        assert_eq!(
            ReplySource::from_event(&frame(json!({"kind": "gateway", "model": null}))),
            Some(ReplySource {
                kind: InferenceKind::Gateway,
                model: None,
            })
        );
        assert_eq!(
            ReplySource::from_event(&frame(json!({"kind": "gateway", "model": "  "}))),
            Some(ReplySource {
                kind: InferenceKind::Gateway,
                model: None,
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
        };
        assert_eq!(ReplySource::from_json(&source.to_json()), Some(source));
        let bare = ReplySource {
            kind: InferenceKind::Gateway,
            model: None,
        };
        assert_eq!(bare.to_json(), r#"{"kind":"gateway"}"#);
        assert_eq!(ReplySource::from_json(&bare.to_json()), Some(bare));
        assert_eq!(ReplySource::from_json("not json"), None);
        assert_eq!(ReplySource::from_json(r#"{"kind":"elsewhere"}"#), None);
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
