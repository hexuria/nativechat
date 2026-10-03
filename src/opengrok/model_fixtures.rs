//! The list of models the tests that draw a picker read, as `GET /models` gives it: every row with
//! the levels of effort it takes and the one it runs at (`efforts` and `ownEffort`). In the shape
//! of opengrok-server #342 (main 2136ffc): `Model::entry` in
//! `crates/opengrok-core/src/catalogue.rs` writes both on every row, and `list_models` in
//! `crates/opengrok-server/src/agui/routes.rs` lists them, as `fixtures/wire/rest/GET__models/`
//! records. Nothing outside the tests builds a list of models but by reading what the server
//! sends.

use super::ModelCatalogue;

/// What `GET /models` lists in the tests that draw a picker: two gateway routes, `oag/cheap`,
/// which lists five levels of effort, low to max, and `medium` as its own, and `xai/grok-4.7`,
/// whose source publishes none; and the plan's GPT-6 Luna, with its fast twin, listing the same
/// five and `medium`, and Sol, six, up to ultra, and `high`.
pub(crate) fn levelled_catalogue() -> ModelCatalogue {
    let levels = |words: &[&str]| -> Vec<serde_json::Value> {
        words
            .iter()
            .map(|word| {
                let name = format!("{}{}", word[..1].to_uppercase(), &word[1..]);
                serde_json::json!({"value": word, "label": format!("{name} Effort")})
            })
            .collect()
    };
    let five = levels(&["low", "medium", "high", "xhigh", "max"]);
    let six = levels(&["low", "medium", "high", "xhigh", "max", "ultra"]);
    serde_json::from_value(serde_json::json!({
        "models": [
            {"id": "oag/cheap", "source": "gateway", "efforts": five, "ownEffort": "medium"},
            {"id": "xai/grok-4.7", "source": "gateway", "efforts": null, "ownEffort": null},
            {"id": "gpt-6-luna", "source": "local_proxy", "efforts": five, "ownEffort": "medium"},
            {"id": "gpt-6-luna--fast", "source": "local_proxy", "efforts": five,
             "ownEffort": "medium"},
            {"id": "gpt-5.6-sol", "source": "local_proxy", "efforts": six, "ownEffort": "high"}
        ],
        "note": null
    }))
    .expect("a list")
}

/// [`levelled_catalogue`] with the level `id` runs at taken away: it lists its levels and names
/// none as its own, so a Bot that chose no effort has no level to sit on.
pub(crate) fn levelled_catalogue_without_own(id: &str) -> ModelCatalogue {
    let mut catalogue = levelled_catalogue();
    for entry in catalogue.models.iter_mut().filter(|entry| entry.id == id) {
        entry.own_effort = None;
    }
    catalogue
}
