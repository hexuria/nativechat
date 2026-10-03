//! Settings → General's first section, Default models: where a newly hired Bot starts, and what a
//! Bot on the person's plan answers with while the relay is off. Each is the account's, kept on
//! the server beside the reply source (`/account/inference-source`), and picked in the same card
//! and popover a Bot's model is picked in (`components::model_picker`), its model, effort and ⚡,
//! with None over the list.
//!
//! A server that keeps a default for new Bots says so by the key on its read (opengrok-server
//! #322, on main c0bb6ae), and every pick is kept on the account at once; on one that keeps none
//! the section says it is coming and its card takes no click (`state::DefaultForNewBots`).
//!
//! The Relay-off fallback, "When Relay is off, Subscription Bots use", is a Gateway model alone:
//! the server's paid keys answer instead of the person's plan. It is `planFallback` on the same
//! setting (opengrok-server #332 (PR #338 at 66b9f7b), which sends it on every read), live only
//! where the read carries that key, and otherwise coming, as the default for new Bots was
//! (`state::RelayOffFallback`).
//!
//! Default for new Bots was on Settings → Relay, and came here with its ids,
//! `settings-new-bots-*`: where a new Bot starts is not the relay's to say, and it is the first
//! thing a person setting up their Bots looks for.
//!
//! The words and element ids live here so the gpui-agent tree (`agent/host.rs`) says what the
//! window says and names what the window names.

use crate::components::model_picker::{self, ModelPicker};
use crate::state::{AccountChange, AppState, DefaultForNewBots, PickerFor, RelayOffFallback};
use gpui_kit::component::{ActiveTheme, Theme, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The section, first on Settings → General, over Chat.
pub(crate) const SECTION: &str = "settings-default-models";
pub(crate) const TITLE: &str = "Default models";
/// Default for new Bots: the section, the line saying why it takes no change, and the picker's
/// card in it, whose popover's parts are `model_picker::NEW_BOTS_IDS`.
pub(crate) const NEW_BOTS: &str = "settings-new-bots";
pub(crate) const NEW_BOTS_UNAVAILABLE: &str = "settings-new-bots-unavailable";
pub(crate) const NEW_BOTS_CARD: &str = "settings-new-bots-card";
pub(crate) const NEW_BOTS_TITLE: &str = "Default for new Bots";
/// The line under Default for new Bots' title while the server keeps one.
pub(crate) const NEW_BOTS_LINE: &str = "A newly hired Bot starts on this model, effort and ⚡. \
     None leaves it to the server's default.";
/// Why Default for new Bots takes no change: the server keeps no default for new Bots yet
/// (`state::DefaultForNewBots`).
pub(crate) const NEW_BOTS_COMING_SOON: &str =
    "Coming soon: the server can't keep a default for new Bots yet.";
/// The Relay-off fallback: the section, the line saying why it takes no change, and the picker's
/// card in it, whose popover's parts are `model_picker::PLAN_FALLBACK_IDS`.
pub(crate) const PLAN_FALLBACK: &str = "settings-plan-fallback";
pub(crate) const PLAN_FALLBACK_UNAVAILABLE: &str = "settings-plan-fallback-unavailable";
pub(crate) const PLAN_FALLBACK_CARD: &str = "settings-plan-fallback-card";
pub(crate) const PLAN_FALLBACK_TITLE: &str = "When Relay is off, Subscription Bots use";
/// Why the Relay-off fallback takes no change: the server keeps no such fallback yet
/// (`state::RelayOffFallback`).
pub(crate) const PLAN_FALLBACK_COMING_SOON: &str =
    "Coming soon: the server can't keep a Relay-off fallback yet.";

/// Default models, on Settings → General. Its pickers need a window, so it is made on the first
/// render of the tab, as Settings → Relay is.
pub struct DefaultModels {
    state: Entity<AppState>,
    /// Default for new Bots' picker: the Bot's card and popover, for the account's default.
    new_bots: Entity<ModelPicker>,
    /// The Relay-off fallback's picker: the same, over the Gateway group alone.
    plan_fallback: Entity<ModelPicker>,
}

impl DefaultModels {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let new_bots = cx.new(|cx| ModelPicker::new(window, state.clone(), PickerFor::NewBots, cx));
        let plan_fallback =
            cx.new(|cx| ModelPicker::new(window, state.clone(), PickerFor::PlanFallback, cx));
        Self {
            state,
            new_bots,
            plan_fallback,
        }
    }

    /// Default for new Bots: where a newly hired Bot starts, in the picker's card. While the
    /// server keeps a default for new Bots the card is the Bot's, opening the same popover, and
    /// what a refusal said is under it while the popover is shut; while it keeps none the section
    /// says it is coming and the card is dimmed and takes no click, since a pick here would change
    /// nothing on the server.
    fn new_bots_section(&self, state: &AppState, theme: &Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let section = v_flex()
            .id(NEW_BOTS)
            .debug_selector(|| NEW_BOTS.into())
            .gap(px(8.))
            .child(div().text_sm().child(NEW_BOTS_TITLE));
        match state.default_for_new_bots() {
            DefaultForNewBots::NotOnServer => section
                .child(
                    div()
                        .id(NEW_BOTS_UNAVAILABLE)
                        .text_xs()
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
                    .child(div().text_xs().text_color(muted).child(NEW_BOTS_LINE))
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

impl DefaultModels {
    /// When Relay is off, Subscription Bots use: the Relay-off fallback, in the picker's card,
    /// drawn as Default for new Bots is. While the server keeps one the card is the Bot's, over
    /// the Gateway group alone, and what a refusal said is under it while the popover is shut;
    /// while it keeps none the section says it is coming and the card is dimmed and takes no
    /// click.
    fn plan_fallback_section(&self, state: &AppState, theme: &Theme) -> AnyElement {
        let muted = theme.muted_foreground;
        let section = v_flex()
            .id(PLAN_FALLBACK)
            .debug_selector(|| PLAN_FALLBACK.into())
            .gap(px(8.))
            .child(div().text_sm().child(PLAN_FALLBACK_TITLE));
        match state.relay_off_fallback() {
            RelayOffFallback::NotOnServer => section
                .child(
                    div()
                        .id(PLAN_FALLBACK_UNAVAILABLE)
                        .text_xs()
                        .text_color(muted)
                        .child(PLAN_FALLBACK_COMING_SOON),
                )
                .child(model_picker::dead_card(PLAN_FALLBACK_CARD, theme))
                .into_any_element(),
            RelayOffFallback::Kept(_) => {
                let note = state
                    .reply_source
                    .change_note(AccountChange::PlanFallback)
                    .filter(|_| !state.plan_fallback_picker.open)
                    .map(str::to_string);
                section
                    .child(self.plan_fallback.clone())
                    .when_some(note, |this, note| {
                        this.child(
                            div()
                                .id(model_picker::PLAN_FALLBACK_IDS.error)
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

impl Render for DefaultModels {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = self.state.read(cx);
        v_flex()
            .id(SECTION)
            .debug_selector(|| SECTION.into())
            .gap(px(12.))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(TITLE),
            )
            .child(self.new_bots_section(state, &theme))
            .child(self.plan_fallback_section(state, &theme))
    }
}

#[cfg(test)]
mod tests {
    // Item by item rather than a glob: `use super::*` would drag in gpui_kit's own `test`.
    use super::{
        NEW_BOTS_COMING_SOON, NEW_BOTS_LINE, NEW_BOTS_TITLE, PLAN_FALLBACK_COMING_SOON,
        PLAN_FALLBACK_TITLE, TITLE,
    };

    /// Default for new Bots says it is coming, in the owner's words, while the server keeps no
    /// such default, which is also before the setting is read: the picker's card in it names no
    /// model and the effort a Bot with none of its own reads as, and takes no click. Where the
    /// server keeps one, the section says what the card is, and that None leaves it to the
    /// server. It is under Default models.
    #[test]
    fn the_default_for_new_bots_is_coming_and_its_card_is_dead() {
        use crate::components::model_picker::dead_card_words;
        use crate::state::DefaultForNewBots;
        assert_eq!(TITLE, "Default models");
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

    /// The Relay-off fallback says, in the owner's words, what it is for and that it is coming
    /// while the server keeps none, which is also before the setting is read; its card is the
    /// dead one Default for new Bots has.
    #[test]
    fn the_relay_off_fallback_is_coming_until_the_server_keeps_one() {
        use crate::state::RelayOffFallback;
        assert_eq!(
            PLAN_FALLBACK_TITLE,
            "When Relay is off, Subscription Bots use"
        );
        assert_eq!(
            PLAN_FALLBACK_COMING_SOON,
            "Coming soon: the server can't keep a Relay-off fallback yet."
        );
        assert_eq!(RelayOffFallback::default(), RelayOffFallback::NotOnServer);
        assert_eq!(
            crate::state::AppState::new().relay_off_fallback(),
            RelayOffFallback::NotOnServer
        );
        assert_eq!(
            super::PLAN_FALLBACK_CARD,
            crate::components::model_picker::PLAN_FALLBACK_IDS.card
        );
    }
}
