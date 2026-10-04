//! The badge on each reply, which says which door it came through: the server's paid keys, the
//! person's own plan, and for a reply the person's computer answered through the relay, that the
//! computer did (opengrok-server #292), or that the server's keys answered because the relay is
//! off (#332). A Bot's door is picked with its model, on its card in its settings
//! (`components::model_picker`), and nowhere else.
//!
//! The relay's switches are each computer's own, on Settings → Computer (`components::computers`);
//! this module was Settings → Relay, which is gone.
//!
//! The words and element ids live here so the gpui-agent tree (`agent/host.rs`) says what the
//! window says and names what the window names.

use crate::opengrok::{InferenceKind, ReplySource, Via};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// In a Bot's Usage card, while replies go through the person's own plan.
pub(crate) const BOT_USAGE_PLAN: &str = "agent-usage-plan";

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

/// In a Bot's Usage card while its replies go through the person's own plan, and always in the
/// Usage modal (`usage_modal`): a turn there is not metered and carries no gateway key, so the
/// server's usage report never counts it.
pub(crate) const PLAN_USAGE_NOTE: &str = "Replies on your own subscription aren't counted here.";

/// What a reply's badge reads when the server's paid keys answered a Bot on the person's plan
/// because the relay is off ([`ReplySource::relay_off`]).
pub(crate) const RELAY_OFF_BADGE: &str = "paid key · relay off";

/// What a reply's badge reads: whose keys paid for it, that the person's computer answered it where
/// it did ([`badge_label`]), or that the server's keys answered because the relay is off; and ⚡
/// where the model that answered is a fast twin, which is all fast ever is on the wire.
pub(crate) fn badge_words(source: &ReplySource) -> String {
    let fast = source
        .model
        .as_deref()
        .is_some_and(crate::opengrok::is_fast);
    let label = if source.relay_off() {
        RELAY_OFF_BADGE
    } else {
        badge_label(source.kind, source.via)
    };
    format!("{label}{}", if fast { " ⚡" } else { "" })
}

/// What a reply's badge reads of its door: whose keys paid for it, and for a reply the person's
/// computer answered through the relay, that the computer did. The wire still calls that door
/// `via: "mac"`, but the relay is the app on whatever computer the person has, so the badge says
/// "computer".
pub(crate) fn badge_label(kind: InferenceKind, via: Option<Via>) -> &'static str {
    match (kind, via) {
        (InferenceKind::Gateway, _) => "paid key",
        (InferenceKind::LocalProxy, Some(Via::Mac)) => "your plan · computer",
        (InferenceKind::LocalProxy, _) => "your plan",
    }
}

/// A reply's badge: "paid key", "paid key · relay off", "your plan" or "your plan · computer", with
/// ⚡ for a fast twin, and the model on hover when the server named one.
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
    /// server's own machine, and the plan the person's computer answers through the relay, which
    /// says "computer" and never "Mac". A way never named, as from a server before the relay, reads
    /// as the plan.
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
            ["paid key", "your plan", "your plan", "your plan · computer"]
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
                fallback_for: None,
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
            "your plan · computer ⚡"
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

    /// A reply the server's paid keys answered because the relay is off says so on its badge, from
    /// the frame as the server writes it (opengrok-server #332 (PR #338 at 66b9f7b), whose
    /// recording holds it live and in a replay): `fallbackFor: "relay_disabled"`, read
    /// tolerantly. A reason this app has not heard of, one that is no word, and one on a reply the
    /// plan answered add nothing to the badge. The reply's row keeps the reason, so the badge
    /// reads the same when the thread is read back.
    #[test]
    fn a_reply_answered_because_the_relay_is_off_says_so_on_its_badge() {
        use crate::opengrok::ReplySource;
        use serde_json::json;
        let frame = |value: serde_json::Value| {
            ReplySource::from_event(&json!({
                "type": "CUSTOM", "name": "opengrok.inferenceSource", "value": value
            }))
            .expect("a badge")
        };
        let off = frame(
            json!({"kind": "gateway", "model": "oag/cheap", "fallbackFor": "relay_disabled"}),
        );
        assert_eq!(badge_words(&off), "paid key · relay off");
        assert_eq!(
            badge_words(&frame(json!({
                "kind": "gateway", "model": "oag/cheap--fast", "fallbackFor": "relay_disabled"
            }))),
            "paid key · relay off ⚡"
        );
        for value in [
            json!({"kind": "gateway", "model": "oag/cheap", "fallbackFor": "relay_slow"}),
            json!({"kind": "gateway", "model": "oag/cheap", "fallbackFor": 3}),
            json!({"kind": "gateway", "model": "oag/cheap", "fallbackFor": null}),
            json!({"kind": "gateway", "model": "oag/cheap"}),
        ] {
            assert_eq!(badge_words(&frame(value.clone())), "paid key", "{value}");
        }
        assert_eq!(
            badge_words(&frame(json!({
                "kind": "local_proxy", "via": "mac", "model": "gpt-6-luna",
                "fallbackFor": "relay_disabled"
            }))),
            "your plan · computer"
        );
        let row = off.to_json();
        assert!(row.contains(r#""fallbackFor":"relay_disabled""#), "{row}");
        assert_eq!(ReplySource::from_json(&row), Some(off));
    }
}
