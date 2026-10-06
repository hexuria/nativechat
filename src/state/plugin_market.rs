//! The plugin marketplace's half of the app state (hexuria/nativechat#184, and the account half of
//! #185 that installed plugins and unfinished sign-ins add): the catalog, the person's installs,
//! each plugin's detail, the sign-ins that still need auth, and what is with the server for each.
//!
//! Everything here is the server's word. The registry is read by opengrok-server and never by this
//! app; an install is the server's once it answers; a sign-in needs auth until the server says it
//! connected. A read answers only the opening it was asked for (`generation`), so a marketplace
//! closed and opened again, or a person who signed out meanwhile, never shows a stale answer.

use super::{AppState, rules_refusal};
use crate::components::monitor_modal::{MarketPage, MonitorKind, MonitorModal, PluginSelection};
use crate::opengrok::{
    ConnectionAttempt, OpenGrokError, PluginCatalog, PluginDetail, PluginInstallation,
};
use gpui_kit::Context;
use std::collections::BTreeMap;

/// One read's state: asked, answered, or refused with the sentence to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loaded<T> {
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loaded<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// A token being pasted for an installed plugin's service: a new account, or a new token for one
/// (`replacing`). The typed token lives here only until it is sent, and is never written anywhere.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenForm {
    pub plugin: String,
    pub connector: String,
    pub replacing: Option<String>,
    pub typed: String,
    pub saving: bool,
    pub refusal: Option<String>,
}

impl std::fmt::Debug for TokenForm {
    /// The typed token is a secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenForm")
            .field("plugin", &self.plugin)
            .field("connector", &self.connector)
            .field("replacing", &self.replacing)
            .field("saving", &self.saving)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default)]
pub struct PluginMarket {
    pub catalog: Option<Loaded<PluginCatalog>>,
    pub installations: Option<Loaded<Vec<PluginInstallation>>>,
    /// Each plugin's detail, by name, read at the catalog's commit.
    pub details: BTreeMap<String, Loaded<PluginDetail>>,
    pub attempts: Option<Loaded<Vec<ConnectionAttempt>>>,
    /// The plugin an install (`true`) or uninstall (`false`) is with the server for.
    pub changing: Option<(String, bool)>,
    /// Why a plugin's last install or uninstall did not go through, by plugin name.
    pub refusals: BTreeMap<String, String>,
    pub token: Option<TokenForm>,
    /// The unfinished sign-in a Reopen, rename or dismiss is with the server for.
    pub attempt_changing: Option<String>,
    /// Why one's last Reopen, rename or dismiss did not go through, by its id.
    pub attempt_refusals: BTreeMap<String, String>,
    pub generation: u64,
    /// Requests sent and not yet answered, whichever opening they were for.
    pub in_flight: u32,
    /// How each installed plugin's service adds an account (#364), by `sign_in_key`: `"oauth"`
    /// at the plugin's own provider, or `"token"` pasted.
    pub sign_in: BTreeMap<String, Loaded<String>>,
    /// The plugin service (`sign_in_key`) or MCP account whose sign-in page is being asked for.
    pub authorizing: Option<String>,
    /// Why the last Authorize or Reconnect did not open a page, by the same key.
    pub authorize_refusals: BTreeMap<String, String>,
    /// `(plugin, Bot)` pairs whose "Use it in this Bot?" prompt was answered Not now, for this
    /// session: asked again after a restart, never again while the person is busy saying no.
    pub use_declined: std::collections::BTreeSet<(String, String)>,
}

/// The key a plugin's service is known by in [`PluginMarket::sign_in`].
pub fn sign_in_key(plugin: &str, connector: &str) -> String {
    format!("{plugin}/{connector}")
}

impl PluginMarket {
    pub fn installation(&self, name: &str) -> Option<&PluginInstallation> {
        self.installations
            .as_ref()?
            .ready()?
            .iter()
            .find(|install| install.name == name)
    }

    /// A configured service's waiting sign-ins: none of an installed plugin's.
    pub fn attempts_for(&self, connector: &str) -> Vec<&ConnectionAttempt> {
        self.attempts
            .as_ref()
            .and_then(Loaded::ready)
            .map(|rows| {
                rows.iter()
                    .filter(|a| a.connector == connector && a.plugin.is_none())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// An installed plugin's waiting sign-ins for one of its services (#364).
    pub fn plugin_attempts(&self, plugin: &str, connector: &str) -> Vec<&ConnectionAttempt> {
        self.attempts
            .as_ref()
            .and_then(Loaded::ready)
            .map(|rows| {
                rows.iter()
                    .filter(|a| a.connector == connector && a.plugin.as_deref() == Some(plugin))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn refusal(prefix: &str, error: &OpenGrokError) -> String {
    rules_refusal(prefix, error)
}

impl AppState {
    /// Plugins, from the sidebar or the Computer pane: the marketplace for the open Bot, read
    /// afresh. It needs no Computer: the catalog, the installs and the accounts are the server's.
    pub fn open_plugin_market(&mut self, cx: &mut Context<Self>) {
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        self.monitor_generation += 1;
        self.monitor_modal = Some(MonitorModal::new(coworker_id, MonitorKind::Plugins));
        self.plugin_market.generation += 1;
        self.plugin_market.refusals.clear();
        self.plugin_market.attempt_refusals.clear();
        self.plugin_market.token = None;
        self.plugin_market.details.clear();
        self.agent_skills_open = true;
        self.refresh_connections(cx);
        self.refresh_coworker_skills(cx);
        self.refresh_coworker_ceiling(cx);
        self.refresh_coworker_tools(cx);
        self.read_catalog(cx);
        self.read_installations(cx);
        self.read_attempts(cx);
        cx.notify();
    }

    fn market_open(&self) -> bool {
        self.monitor_modal
            .as_ref()
            .is_some_and(|modal| modal.kind == MonitorKind::Plugins)
    }

    pub(crate) fn read_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if !matches!(self.plugin_market.catalog, Some(Loaded::Ready(_))) {
            self.plugin_market.catalog = Some(Loaded::Loading);
        }
        let generation = self.plugin_market.generation;
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let read = client.plugin_catalog().await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.catalog = Some(match read {
                    Ok(catalog) => Loaded::Ready(catalog),
                    Err(error) => {
                        Loaded::Failed(refusal("The plugin marketplace could not be read", &error))
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn read_installations(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if !matches!(self.plugin_market.installations, Some(Loaded::Ready(_))) {
            self.plugin_market.installations = Some(Loaded::Loading);
        }
        let generation = self.plugin_market.generation;
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let read = client.plugin_installations().await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.installations = Some(match read {
                    Ok(rows) => Loaded::Ready(rows),
                    Err(error) => {
                        Loaded::Failed(refusal("Your installed plugins could not be read", &error))
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn read_attempts(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if !matches!(self.plugin_market.attempts, Some(Loaded::Ready(_))) {
            self.plugin_market.attempts = Some(Loaded::Loading);
        }
        let generation = self.plugin_market.generation;
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let read = client.connection_attempts().await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.attempts = Some(match read {
                    Ok(rows) => Loaded::Ready(rows),
                    Err(error) => Loaded::Failed(refusal(
                        "Sign-ins waiting on a service could not be read",
                        &error,
                    )),
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// Coming back to the window, or after a sign-in page opened: a sign-in finishes in the
    /// browser, so what needs auth and what connected is asked again.
    pub(crate) fn reread_market_accounts(&mut self, cx: &mut Context<Self>) {
        if self.market_open() {
            self.read_attempts(cx);
            self.read_installations(cx);
        }
    }

    fn read_detail(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(revision) = self
            .plugin_market
            .catalog
            .as_ref()
            .and_then(Loaded::ready)
            .map(|catalog| catalog.revision.clone())
        else {
            return;
        };
        if matches!(
            self.plugin_market.details.get(&name),
            Some(Loaded::Ready(_) | Loaded::Loading)
        ) {
            return;
        }
        self.plugin_market
            .details
            .insert(name.clone(), Loaded::Loading);
        let generation = self.plugin_market.generation;
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let read = client.plugin_detail(&name, &revision).await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                let read = match read {
                    Ok(detail) => Loaded::Ready(detail),
                    Err(error) => {
                        Loaded::Failed(refusal("What this plugin brings could not be read", &error))
                    }
                };
                state.plugin_market.details.insert(name, read);
                cx.notify();
            });
        })
        .detach();
    }

    /// What is typed in the marketplace's search. The highlight goes back to the first result,
    /// since the one it was on may no longer be listed.
    pub fn set_market_query(&mut self, query: String, cx: &mut Context<Self>) {
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        if modal.query != query {
            modal.query = query;
            modal.highlight = 0;
            cx.notify();
        }
    }

    /// Up or Down in the search: the highlight moves through the results in the order they show,
    /// and stops at either end rather than wrapping out of sight.
    pub fn move_market_highlight(&mut self, by: isize, cx: &mut Context<Self>) {
        let count = crate::components::marketplace::visible_rows(self).len();
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        if count == 0 {
            modal.highlight = 0;
            return;
        }
        let next = (modal.highlight as isize + by).clamp(0, count as isize - 1) as usize;
        if next != modal.highlight {
            modal.highlight = next;
            cx.notify();
        }
    }

    /// Enter in the search: the highlighted plugin's detail opens. Never an install: Add is a
    /// separate, deliberate press.
    pub fn open_market_highlight(&mut self, cx: &mut Context<Self>) {
        let rows = crate::components::marketplace::visible_rows(self);
        let Some(modal) = self.monitor_modal.as_ref() else {
            return;
        };
        if let Some(row) = rows.get(modal.highlight) {
            let selection = row.selection.clone();
            self.open_market_detail(selection, cx);
        }
    }

    pub fn set_market_page(&mut self, page: MarketPage, cx: &mut Context<Self>) {
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        if modal.removing {
            return;
        }
        modal.page = page;
        modal.selected = None;
        modal.highlight = 0;
        modal.confirming = false;
        modal.removal = None;
        modal.error = None;
        modal.renaming = None;
        self.plugin_market.token = None;
        cx.notify();
    }

    /// Not now, on a plugin's "Use it in this Bot?" prompt.
    pub fn decline_use_prompt(&mut self, plugin: String, cx: &mut Context<Self>) {
        if let Some(bot) = self.active_coworker_id.clone() {
            self.plugin_market.use_declined.insert((plugin, bot));
            cx.notify();
        }
    }

    /// Plugins, opened on `plugin`'s detail: where a needs card sends the person to install it or
    /// add an account (#360).
    pub fn open_plugin_detail(&mut self, plugin: String, cx: &mut Context<Self>) {
        self.open_plugin_market(cx);
        self.open_market_detail(PluginSelection::Plugin(plugin), cx);
    }

    /// Whether a needs card on `message_id` can still be answered: it is the open thread's last
    /// row, and no turn is going. An older one has messages after it, and sending it again would
    /// answer the newest message instead, as Try again's rule has it.
    pub fn plugin_needs_answerable(&self, message_id: &str) -> bool {
        if self.is_turn_in_flight() || self.session.is_expired() {
            return false;
        }
        let open = self
            .conversations
            .iter()
            .find(|c| Some(&c.id) == self.active_conversation_id.as_ref());
        let last = open.and_then(|c| c.messages.iter().rev().find(|m| !m.hidden));
        last.is_some_and(|last| last.id == message_id)
    }

    pub fn toggle_needs_remember(&mut self, message_id: String, cx: &mut Context<Self>) {
        if !self.plugin_needs_remember.remove(&message_id) {
            self.plugin_needs_remember.insert(message_id);
        }
        cx.notify();
    }

    /// The account the person picked on a "Which account?" card: sent with the same message
    /// again, for that turn (`pluginAccounts`), and kept as the Bot's pin when Remember is on.
    pub fn choose_plugin_account(
        &mut self,
        message_id: String,
        (plugin, connector, account): (String, String, String),
        cx: &mut Context<Self>,
    ) {
        if !self.plugin_needs_answerable(&message_id) {
            return;
        }
        let remember = self.plugin_needs_remember.remove(&message_id);
        self.next_turn_accounts
            .entry(plugin)
            .or_default()
            .insert(connector.clone(), account.clone());
        if remember
            && let (Some(client), Some(bot)) =
                (self.opengrok.clone(), self.active_coworker_id.clone())
        {
            cx.spawn(async move |this, cx| {
                let pinned = client.pin_connection(&bot, &connector, &account).await;
                let _ = this.update(cx, |state, cx| {
                    if let Err(error) = pinned {
                        state.auth_error = Some(format!(
                            "The account was used but not remembered: {}",
                            error.message
                        ));
                    }
                    state.read_pins(cx);
                    cx.notify();
                });
            })
            .detach();
        }
        self.resend_after_needs(message_id, cx);
    }

    /// Send the message a needs card answered again: the card's row goes, and the turn runs from
    /// the thread as it stands, as Try again's does. The person's message is not sent twice.
    pub fn resend_after_needs(&mut self, message_id: String, cx: &mut Context<Self>) {
        if !self.plugin_needs_answerable(&message_id) {
            return;
        }
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            return;
        };
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            conversation.messages.retain(|m| m.id != message_id);
        }
        self.plugin_needs_remember.remove(&message_id);
        self.send_opengrok_turn(conversation_id, String::new(), None, cx);
    }

    /// A marketplace row: its detail opens, read from the plugin's pinned bundle when it is one.
    pub fn open_market_detail(&mut self, selection: PluginSelection, cx: &mut Context<Self>) {
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        if modal.removing {
            return;
        }
        if let Some(index) = crate::components::marketplace::visible_rows(self)
            .iter()
            .position(|row| row.selection == selection)
            && let Some(modal) = self.monitor_modal.as_mut()
        {
            modal.highlight = index;
        }
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        modal.selected = Some(selection.clone());
        modal.confirming = false;
        modal.removal = None;
        modal.error = None;
        modal.renaming = None;
        modal.tools_open = false;
        self.plugin_market.token = None;
        if let PluginSelection::Plugin(name) = selection {
            self.read_sign_in_methods(&name, cx);
            self.read_detail(name, cx);
        }
        cx.notify();
    }

    /// Back, in a detail: the page it was opened from, with the highlight where it was.
    pub fn close_market_detail(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        if modal.removing {
            return;
        }
        modal.selected = None;
        modal.confirming = false;
        modal.removal = None;
        modal.renaming = None;
        modal.bots_for = None;
        self.plugin_market.token = None;
        cx.notify();
    }

    pub fn toggle_market_tools(&mut self, cx: &mut Context<Self>) {
        if let Some(modal) = self.monitor_modal.as_mut() {
            modal.tools_open = !modal.tools_open;
            cx.notify();
        }
    }

    /// Add: install the plugin at the registry commit the marketplace was read at. Only once the
    /// server answers is it installed; until then the row says Adding, and a refusal is said in
    /// the server's words where Add was. One install or uninstall at a time.
    pub fn install_market_plugin(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if self.plugin_market.changing.is_some() || self.plugin_market.installation(&name).is_some()
        {
            return;
        }
        let Some(catalog) = self.plugin_market.catalog.as_ref().and_then(Loaded::ready) else {
            return;
        };
        let Some(entry) = catalog.plugins.iter().find(|entry| entry.name == name) else {
            return;
        };
        if entry.unavailable_reason.is_some() {
            return;
        }
        let revision = catalog.revision.clone();
        self.plugin_market.changing = Some((name.clone(), true));
        self.plugin_market.refusals.remove(&name);
        let generation = self.plugin_market.generation;
        cx.notify();
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let answer = client.install_plugin(&name, &revision).await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.changing = None;
                if let Err(error) = answer {
                    state
                        .plugin_market
                        .refusals
                        .insert(name, refusal("It was not added", &error));
                }
                // Read again either way: a reply that timed out may still have installed it, and
                // the list is the server's word on whether it did.
                state.read_installations(cx);
                state.refresh_coworker_ceiling(cx);
                state.refresh_coworker_tools(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Uninstall, in an installed plugin's detail: asks first, since its pasted accounts go with
    /// it and every Bot stops using it.
    pub fn ask_market_uninstall(&mut self, confirm: bool, cx: &mut Context<Self>) {
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        if !matches!(modal.selected, Some(PluginSelection::Plugin(_))) || modal.removing {
            return;
        }
        modal.confirming = confirm;
        modal.removal = None;
        cx.notify();
    }

    pub fn uninstall_market_plugin(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(modal) = self.monitor_modal.as_ref() else {
            return;
        };
        let Some(PluginSelection::Plugin(name)) = modal.selected.clone() else {
            return;
        };
        if !modal.confirming
            || self.plugin_market.changing.is_some()
            || self.plugin_market.installation(&name).is_none()
        {
            return;
        }
        if let Some(modal) = self.monitor_modal.as_mut() {
            modal.confirming = false;
        }
        self.plugin_market.changing = Some((name.clone(), false));
        self.plugin_market.refusals.remove(&name);
        let generation = self.plugin_market.generation;
        cx.notify();
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let answer = client.uninstall_plugin(&name).await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.changing = None;
                if let Err(error) = answer {
                    state
                        .plugin_market
                        .refusals
                        .insert(name, refusal("It was not uninstalled", &error));
                }
                state.read_installations(cx);
                state.refresh_connections(cx);
                state.refresh_coworker_ceiling(cx);
                state.refresh_coworker_tools(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Add Another Account, on an installed plugin's service: a field for the token, which the
    /// server keeps. `replacing` names the account a new token is for (its Reconnect).
    pub fn start_market_token(
        &mut self,
        plugin: String,
        connector: String,
        replacing: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.plugin_market.installation(&plugin).is_none() {
            return;
        }
        self.plugin_market.token = Some(TokenForm {
            plugin,
            connector,
            replacing,
            typed: String::new(),
            saving: false,
            refusal: None,
        });
        cx.notify();
    }

    pub fn set_market_token(&mut self, typed: String, cx: &mut Context<Self>) {
        if let Some(form) = self.plugin_market.token.as_mut()
            && !form.saving
            && form.typed != typed
        {
            form.typed = typed;
            cx.notify();
        }
    }

    pub fn cancel_market_token(&mut self, cx: &mut Context<Self>) {
        if self
            .plugin_market
            .token
            .as_ref()
            .is_some_and(|form| !form.saving)
        {
            self.plugin_market.token = None;
            cx.notify();
        }
    }

    /// Save, on the token field: to the server and nowhere else. The field closes once the server
    /// has it; a refusal stays under the field in the server's words, with what was typed kept so
    /// the person can correct it. Never sent again on its own.
    pub fn save_market_token(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(form) = self.plugin_market.token.as_mut() else {
            return;
        };
        if form.saving || form.typed.trim().is_empty() {
            return;
        }
        form.saving = true;
        form.refusal = None;
        let form = form.clone();
        let generation = self.plugin_market.generation;
        cx.notify();
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let token = form.typed.trim().to_string();
            let answer = match &form.replacing {
                Some(id) => client
                    .replace_plugin_token(&form.plugin, &form.connector, id, &token)
                    .await
                    .map(|()| id.clone()),
                None => {
                    client
                        .add_plugin_token(&form.plugin, &form.connector, &token)
                        .await
                }
            };
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                match answer {
                    Ok(_) => state.plugin_market.token = None,
                    Err(error) => {
                        if let Some(open) = state.plugin_market.token.as_mut() {
                            open.saving = false;
                            open.refusal = Some(refusal("The token was not saved", &error));
                        }
                    }
                }
                state.read_installations(cx);
                state.refresh_connections(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Reopen, on a sign-in that needs auth: the service's page for THAT sign-in, so finishing it
    /// adds the account under the label shown, and no second row appears.
    pub fn reopen_attempt(&mut self, attempt_id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if self.plugin_market.attempt_changing.is_some() || self.connections.opening.is_some() {
            return;
        }
        self.plugin_market.attempt_changing = Some(attempt_id.clone());
        self.plugin_market.attempt_refusals.remove(&attempt_id);
        let generation = self.plugin_market.generation;
        cx.notify();
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let link = client.reopen_link(&attempt_id).await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.attempt_changing = None;
                match link {
                    // Only while the marketplace that asked is still open: a browser opening for a
                    // page the person has left is a sign-in they did not ask for now.
                    Ok(url) if state.market_open() => cx.open_url(&url),
                    Ok(_) => {}
                    Err(error) => {
                        state.plugin_market.attempt_refusals.insert(
                            attempt_id,
                            refusal("The sign-in page could not be opened", &error),
                        );
                    }
                }
                state.read_attempts(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn dismiss_attempt(&mut self, attempt_id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if self.plugin_market.attempt_changing.is_some() {
            return;
        }
        self.plugin_market.attempt_changing = Some(attempt_id.clone());
        let generation = self.plugin_market.generation;
        cx.notify();
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let answer = client.dismiss_attempt(&attempt_id).await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.attempt_changing = None;
                if let Err(error) = answer {
                    state
                        .plugin_market
                        .attempt_refusals
                        .insert(attempt_id, refusal("It was not removed", &error));
                }
                state.read_attempts(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Enter in a waiting sign-in's rename field: the label goes to the server, which the account
    /// takes when it connects. A refusal is said under the row.
    pub(crate) fn save_attempt_rename(
        &mut self,
        attempt_id: String,
        label: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let unchanged = self
            .plugin_market
            .attempts
            .as_ref()
            .and_then(Loaded::ready)
            .and_then(|rows| rows.iter().find(|a| a.id == attempt_id))
            .is_some_and(|a| a.label == label.trim());
        if unchanged || self.plugin_market.attempt_changing.is_some() {
            return;
        }
        self.plugin_market.attempt_changing = Some(attempt_id.clone());
        self.plugin_market.attempt_refusals.remove(&attempt_id);
        let generation = self.plugin_market.generation;
        cx.notify();
        self.plugin_market.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let answer = client.rename_attempt(&attempt_id, &label).await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.attempt_changing = None;
                if let Err(error) = answer {
                    state
                        .plugin_market
                        .attempt_refusals
                        .insert(attempt_id, refusal("It was not renamed", &error));
                }
                state.read_attempts(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Remove, on one of a service's accounts in its detail: asks first, naming the Bots that will
    /// lose it, then disconnects it.
    pub fn ask_account_remove(&mut self, connection_id: Option<String>, cx: &mut Context<Self>) {
        let Some(modal) = self.monitor_modal.as_mut() else {
            return;
        };
        if modal.removing {
            return;
        }
        modal.confirming = connection_id.is_some();
        modal.removal = connection_id;
        cx.notify();
    }

    /// An account's Bots button: the detail gives way to the list of Bots that may use it.
    pub fn open_account_bots(&mut self, connection_id: Option<String>, cx: &mut Context<Self>) {
        if let Some(modal) = self.monitor_modal.as_mut() {
            modal.bots_for = connection_id;
            modal.bots_query.clear();
            cx.notify();
        }
    }

    pub fn set_bots_query(&mut self, query: String, cx: &mut Context<Self>) {
        if let Some(modal) = self.monitor_modal.as_mut()
            && modal.bots_query != query
        {
            modal.bots_query = query;
            cx.notify();
        }
    }

    /// Ask how each service of an installed plugin adds an account, once per opening.
    pub(crate) fn read_sign_in_methods(&mut self, plugin: &str, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(install) = self.plugin_market.installation(plugin) else {
            return;
        };
        let connectors = install.connectors.clone();
        let generation = self.plugin_market.generation;
        for connector in connectors {
            let key = sign_in_key(plugin, &connector);
            if matches!(
                self.plugin_market.sign_in.get(&key),
                Some(Loaded::Ready(_) | Loaded::Loading)
            ) {
                continue;
            }
            self.plugin_market
                .sign_in
                .insert(key.clone(), Loaded::Loading);
            let (client, plugin) = (client.clone(), plugin.to_string());
            self.plugin_market.in_flight += 1;
            cx.spawn(async move |this, cx| {
                let read = client.plugin_sign_in_method(&plugin, &connector).await;
                let _ = this.update(cx, |state, cx| {
                    state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                    if state.plugin_market.generation != generation {
                        return;
                    }
                    let read = match read {
                        Ok(method) => Loaded::Ready(method),
                        Err(error) => Loaded::Failed(refusal(
                            "How to add an account could not be read",
                            &error,
                        )),
                    };
                    state.plugin_market.sign_in.insert(key, read);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Authorize, on an installed plugin's service that signs people in itself (#364), or Reconnect
    /// on one of its MCP accounts (`connection_id`): the provider's own page, in the person's
    /// browser. The account is the server's word once the provider sends them back; meanwhile the
    /// sign-in waits as Needs Auth. One page at a time.
    pub fn authorize_plugin(
        &mut self,
        plugin: String,
        connector: String,
        connection_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if self.plugin_market.authorizing.is_some()
            || self.plugin_market.installation(&plugin).is_none()
        {
            return;
        }
        let key = connection_id
            .clone()
            .unwrap_or_else(|| sign_in_key(&plugin, &connector));
        self.plugin_market.authorizing = Some(key.clone());
        self.plugin_market.authorize_refusals.remove(&key);
        let generation = self.plugin_market.generation;
        self.plugin_market.in_flight += 1;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let link = client
                .plugin_authorize_link(&plugin, &connector, connection_id.as_deref())
                .await;
            let _ = this.update(cx, |state, cx| {
                state.plugin_market.in_flight = state.plugin_market.in_flight.saturating_sub(1);
                if state.plugin_market.generation != generation {
                    return;
                }
                state.plugin_market.authorizing = None;
                match link {
                    // Only while the marketplace that asked is open: a page the person has left
                    // opening a browser is a sign-in they did not ask for now.
                    Ok(url) if state.market_open() => cx.open_url(&url),
                    Ok(_) => {}
                    Err(error) => {
                        state
                            .plugin_market
                            .authorize_refusals
                            .insert(key, refusal("The sign-in page could not be opened", &error));
                    }
                }
                // The server lists the sign-in as waiting now.
                state.read_attempts(cx);
                cx.notify();
            });
        })
        .detach();
    }
}
