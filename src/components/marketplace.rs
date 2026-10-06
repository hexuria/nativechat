//! The plugin marketplace (hexuria/nativechat#184), in the Grok Bot layout: search, the
//! marketplace's own categories in two columns, Add and Added, an installed count that opens what
//! is installed, and a detail with the plugin's accounts, skills, tools, apps and facts.
//!
//! Every row and every fact is the server's: the catalog opengrok-server reads from
//! xai-org/plugin-marketplace, the person's installs, the services this server signs in to, and the
//! sign-ins still waiting. Nothing here is ranked, invented or counted where the server says
//! nothing; an absent fact is not shown. The window and the gpui-agent driver read the same rows.

use crate::components::connections;
use crate::components::monitor_modal::{MarketPage, MonitorModal, PluginSelection};
use crate::opengrok::{ConnectionKind, PluginDetail};
use crate::state::{AppState, ConnectorList, Loaded, ToolList};

/// How many rows a category shows on the browse page before View all, as two lines of two.
pub(crate) const PER_SECTION: usize = 4;

pub(crate) const SEARCH: &str = "market-search";
pub(crate) const INSTALLED: &str = "market-installed";
pub(crate) const BACK: &str = "market-back";
pub(crate) const STATUS: &str = "market-status";
pub(crate) const EMPTY: &str = "market-empty";
pub(crate) const DETAIL: &str = "market-detail";
pub(crate) const DETAIL_ACTION: &str = "market-detail-action";
pub(crate) const DETAIL_REFUSAL: &str = "market-detail-refusal";
pub(crate) const UNINSTALL_YES: &str = "market-uninstall-confirm";
pub(crate) const UNINSTALL_NO: &str = "market-uninstall-cancel";
pub(crate) const SOURCE: &str = "market-source";
pub(crate) const TOOLS: &str = "market-tools";
pub(crate) const TOKEN_FIELD: &str = "market-token-field";
pub(crate) const TOKEN_SAVE: &str = "market-token-save";
pub(crate) const TOKEN_CANCEL: &str = "market-token-cancel";
pub(crate) const TOKEN_REFUSAL: &str = "market-token-refusal";
pub(crate) const REMOVE_YES: &str = "market-account-remove-confirm";
pub(crate) const REMOVE_NO: &str = "market-account-remove-cancel";

/// A row's Add, by what it opens.
pub(crate) fn add_id(selection: &PluginSelection) -> String {
    format!("{}-add", selection.detail_id())
}

/// A category's View all.
pub(crate) fn view_all_id(category: &str) -> String {
    format!("market-view-all-{category}")
}

/// Add Another Account, under a service's accounts.
pub(crate) fn add_account_id(connector: &str) -> String {
    format!("market-add-account-{connector}")
}

/// A waiting sign-in's Reopen, and its Remove.
pub(crate) fn reopen_id(attempt: &str) -> String {
    format!("market-reopen-{attempt}")
}
pub(crate) fn dismiss_id(attempt: &str) -> String {
    format!("market-dismiss-{attempt}")
}

/// An account's Remove, and a pasted account's new token.
pub(crate) fn remove_account_id(id: &str) -> String {
    format!("market-account-remove-{id}")
}
pub(crate) fn replace_token_id(id: &str) -> String {
    format!("market-replace-token-{id}")
}

/// An account's Bots button, the list's filter, and one Bot's row in it.
pub(crate) fn bots_id(id: &str) -> String {
    format!("market-account-bots-{id}")
}
pub(crate) const BOTS_SEARCH: &str = "market-bots-search";
pub(crate) const BOTS_BACK: &str = "market-bots-back";
pub(crate) fn bot_row_id(bot: &str) -> String {
    format!("market-bot-{bot}")
}

/// One Bot in an account's Bots list: whether it may use the account, and whether that can change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BotRow {
    pub id: String,
    pub name: String,
    pub on: bool,
    pub live: bool,
    pub note: Option<String>,
}

/// The person's Bots for an account's Bots list, filtered by what is typed, by name.
pub(crate) fn bot_rows(state: &AppState, connection_id: &str, query: &str) -> Vec<BotRow> {
    let connections = &state.connections;
    let own = connections.own_rows();
    let Some(row) = own.iter().find(|r| r.id == connection_id) else {
        return Vec::new();
    };
    let changing = connections.is_changing(&row.id);
    let query = query.trim().to_lowercase();
    state
        .coworkers
        .iter()
        .filter(|bot| query.is_empty() || bot.name.to_lowercase().contains(&query))
        .map(|bot| BotRow {
            id: bot.id.clone(),
            name: bot.name.clone(),
            on: connections.shows_lent(row, &bot.id),
            live: !changing,
            note: connections
                .lend_refusal(&row.id, &bot.id)
                .map(str::to_string),
        })
        .collect()
}

/// What a row's button says and does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RowAction {
    /// Add: installs a plugin, or starts a service's first sign-in. Dead while another is with the
    /// server.
    Add { live: bool },
    /// The install, or the sign-in page, is with the server.
    Adding,
    /// Installed, or a service the person has an account of.
    Added,
    /// The server will not install it, and says why in the detail.
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarketRow {
    pub selection: PluginSelection,
    pub title: String,
    pub description: String,
    pub category: Option<String>,
    pub action: RowAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub id: String,
    pub title: String,
    pub rows: Vec<MarketRow>,
    /// The category its View all opens, when it has more than it shows.
    pub view_all: Option<String>,
}

/// The marketplace's word for a category, as a heading: its own spelling, capitalised.
pub(crate) fn category_title(category: &str) -> String {
    let mut chars = category.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars)
            .collect::<String>()
            .replace('-', " "),
        None => String::new(),
    }
}

/// The services this server signs in to, then the marketplace's plugins in its order, then any
/// install the marketplace no longer lists.
pub(crate) fn all_rows(state: &AppState) -> Vec<MarketRow> {
    let market = &state.plugin_market;
    let connections = &state.connections;
    let mut rows = Vec::new();
    if let Some(ConnectorList::Listed(services)) = &connections.connectors {
        let own = connections.own_rows();
        for service in services.iter().filter(|s| s.plugin.is_none()) {
            let label = connections.connector_label(&service.name);
            let action = if own.iter().any(|row| row.connector == service.name) {
                RowAction::Added
            } else if connections.opening.as_deref() == Some(service.name.as_str()) {
                RowAction::Adding
            } else {
                RowAction::Add {
                    live: connections.opening.is_none(),
                }
            };
            rows.push(MarketRow {
                selection: PluginSelection::Service(service.name.clone()),
                description: format!("Sign in with your {label} accounts"),
                title: label,
                category: None,
                action,
            });
        }
    }
    let installs = market.installations.as_ref().and_then(Loaded::ready);
    let installed = |name: &str| installs.is_some_and(|rows| rows.iter().any(|i| i.name == name));
    let catalog = market.catalog.as_ref().and_then(Loaded::ready);
    if let Some(catalog) = catalog {
        for entry in &catalog.plugins {
            let action = if installed(&entry.name) {
                RowAction::Added
            } else if market.changing.as_ref() == Some(&(entry.name.clone(), true)) {
                RowAction::Adding
            } else if entry.unavailable_reason.is_some() {
                RowAction::Unavailable
            } else {
                RowAction::Add {
                    live: market.changing.is_none(),
                }
            };
            rows.push(MarketRow {
                selection: PluginSelection::Plugin(entry.name.clone()),
                title: entry.name.clone(),
                description: entry.description.clone(),
                category: entry.category.clone(),
                action,
            });
        }
    }
    for install in installs.into_iter().flatten() {
        let listed = catalog.is_some_and(|c| c.plugins.iter().any(|e| e.name == install.name));
        if !listed {
            rows.push(MarketRow {
                selection: PluginSelection::Plugin(install.name.clone()),
                title: install.name.clone(),
                description: install
                    .bundle
                    .manifest
                    .description
                    .clone()
                    .unwrap_or_default(),
                category: None,
                action: RowAction::Added,
            });
        }
    }
    rows
}

fn matches(row: &MarketRow, query: &str) -> bool {
    let haystack = format!(
        "{} {} {}",
        row.title,
        row.description,
        row.category.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|term| haystack.contains(&term.to_lowercase()))
}

/// The sections the open page shows, in order. A search shows every match, from every page.
pub(crate) fn sections(state: &AppState) -> Vec<Section> {
    let Some(modal) = state.monitor_modal.as_ref() else {
        return Vec::new();
    };
    sections_for(state, modal)
}

pub(crate) fn sections_for(state: &AppState, modal: &MonitorModal) -> Vec<Section> {
    let rows = all_rows(state);
    let query = modal.query.trim();
    if !query.is_empty() {
        let found: Vec<_> = rows.into_iter().filter(|row| matches(row, query)).collect();
        return if found.is_empty() {
            Vec::new()
        } else {
            vec![Section {
                id: "market-section-results".into(),
                title: "Results".into(),
                rows: found,
                view_all: None,
            }]
        };
    }
    let is_plugin_in_catalog = |row: &MarketRow| {
        matches!(row.selection, PluginSelection::Plugin(_))
            && state
                .plugin_market
                .catalog
                .as_ref()
                .and_then(Loaded::ready)
                .is_some_and(|c| {
                    c.plugins
                        .iter()
                        .any(|e| PluginSelection::Plugin(e.name.clone()) == row.selection)
                })
    };
    match &modal.page {
        MarketPage::Installed => {
            let installed: Vec<_> = rows
                .into_iter()
                .filter(|row| row.action == RowAction::Added)
                .collect();
            if installed.is_empty() {
                return Vec::new();
            }
            vec![Section {
                id: "market-section-installed".into(),
                title: "Installed".into(),
                rows: installed,
                view_all: None,
            }]
        }
        MarketPage::Category(category) => {
            let rows: Vec<_> = rows
                .into_iter()
                .filter(|row| row.category.as_deref() == Some(category.as_str()))
                .collect();
            vec![Section {
                id: format!("market-section-{category}"),
                title: category_title(category),
                rows,
                view_all: None,
            }]
        }
        MarketPage::Browse => {
            let mut sections = Vec::new();
            let services: Vec<_> = rows
                .iter()
                .filter(|row| matches!(row.selection, PluginSelection::Service(_)))
                .cloned()
                .collect();
            if !services.is_empty() {
                sections.push(Section {
                    id: "market-section-apps".into(),
                    title: "Apps".into(),
                    rows: services,
                    view_all: None,
                });
            }
            let mut order: Vec<String> = Vec::new();
            for row in rows.iter().filter(|row| is_plugin_in_catalog(row)) {
                if let Some(category) = &row.category
                    && !order.contains(category)
                {
                    order.push(category.clone());
                }
            }
            for category in order {
                let all: Vec<_> = rows
                    .iter()
                    .filter(|row| is_plugin_in_catalog(row))
                    .filter(|row| row.category.as_deref() == Some(category.as_str()))
                    .cloned()
                    .collect();
                let more = all.len() > PER_SECTION;
                sections.push(Section {
                    id: format!("market-section-{category}"),
                    title: category_title(&category),
                    rows: all.into_iter().take(PER_SECTION).collect(),
                    view_all: more.then(|| category.clone()),
                });
            }
            let unfiled: Vec<_> = rows
                .iter()
                .filter(|row| is_plugin_in_catalog(row) && row.category.is_none())
                .cloned()
                .collect();
            if !unfiled.is_empty() {
                sections.push(Section {
                    id: "market-section-more".into(),
                    title: "More plugins".into(),
                    rows: unfiled,
                    view_all: None,
                });
            }
            sections
        }
    }
}

/// Every row the open page shows, in the order Up and Down move through them.
pub(crate) fn visible_rows(state: &AppState) -> Vec<MarketRow> {
    sections(state)
        .into_iter()
        .flat_map(|section| section.rows)
        .collect()
}

pub(crate) fn row_exists(state: &AppState, selection: &PluginSelection) -> bool {
    all_rows(state)
        .iter()
        .any(|row| &row.selection == selection)
}

/// How many things are installed: plugins, and services the person has an account of.
pub(crate) fn installed_count(state: &AppState) -> usize {
    all_rows(state)
        .iter()
        .filter(|row| row.action == RowAction::Added)
        .count()
}

/// Why the marketplace cannot show everything it would, in the server's words.
pub(crate) fn status_lines(state: &AppState) -> Vec<String> {
    let market = &state.plugin_market;
    let mut lines = Vec::new();
    match &market.catalog {
        None | Some(Loaded::Loading) => lines.push("Reading the plugin marketplace…".into()),
        Some(Loaded::Failed(why)) => lines.push(why.clone()),
        Some(Loaded::Ready(_)) => {}
    }
    if let Some(Loaded::Failed(why)) = &market.installations {
        lines.push(why.clone());
    }
    if let Some(ConnectorList::Unavailable(why)) = &state.connections.connectors {
        lines.push(why.clone());
    }
    lines
}

/// One account in a detail's Accounts card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccountLine {
    /// The connection's id, or a waiting sign-in's.
    pub id: String,
    pub label: String,
    pub status: AccountStatus,
    pub renaming: Option<String>,
    pub can_rename: bool,
    /// Reconnect (a sign-in page), Reopen (a waiting sign-in) or a new token (a pasted account).
    pub action: Option<(String, AccountAction, bool)>,
    /// Remove can be pressed.
    pub removable: bool,
    /// How many of the person's Bots may use it (an OAuth account's loans), and whether its Bots
    /// list can change them now.
    pub lent: Option<(usize, bool)>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AccountStatus {
    Connected,
    /// A sign-in the service has not connected: waiting, or refused with why.
    NeedsAuth(Option<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AccountAction {
    Reconnect,
    Reopen,
    ReplaceToken,
    /// Sign in again at an installed plugin's own provider (#364).
    PluginReconnect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccountsCard {
    pub connector: String,
    pub title: String,
    pub lines: Vec<AccountLine>,
    /// Add Another Account can be pressed.
    pub can_add: bool,
    /// Its sign-in page is being asked for.
    pub adding: bool,
    pub add_refusal: Option<String>,
    /// Add opens a token field rather than a sign-in page.
    pub pasted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DetailAction {
    Add { live: bool },
    Adding,
    Uninstall { live: bool },
    Uninstalling,
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarketDetail {
    pub title: String,
    pub description: String,
    /// "owner/repo" and the address of the pinned source.
    pub source: Option<(String, String)>,
    pub action: Option<DetailAction>,
    pub refusal: Option<String>,
    pub accounts: Vec<AccountsCard>,
    /// Why the bundle's parts are not shown, while they are read or when they cannot be.
    pub parts_status: Option<String>,
    pub skills: Vec<String>,
    pub servers: Vec<String>,
    /// The open Bot's switch for the plugin, and the tools it has from it.
    pub bot_switch: Option<(bool, bool, Option<String>)>,
    /// Whether to ask "Use it in <Bot>?": the plugin has an account, the open Bot's switch is off
    /// and can be moved, and the person has not said Not now this session (#359). Setup is
    /// install, then an account, then the Bots; the third step is one tap here.
    pub use_prompt: bool,
    pub tools: Vec<String>,
    pub tools_status: Option<String>,
    pub apps: Vec<String>,
    pub unsupported: Vec<(String, String)>,
    pub info: Vec<(&'static str, String)>,
    pub question: Option<String>,
}

fn short(revision: &str) -> String {
    revision.chars().take(7).collect()
}

fn service_card(state: &AppState, connector: &str) -> AccountsCard {
    let connections = &state.connections;
    let market = &state.plugin_market;
    let modal = state.monitor_modal.as_ref();
    let removing = modal.is_some_and(|modal| modal.removing);
    let renaming = modal.and_then(|modal| modal.renaming.as_ref());
    let bot = state.active_coworker_id.as_deref().unwrap_or_default();
    let mut lines: Vec<AccountLine> = connections
        .own_rows()
        .into_iter()
        .filter(|row| row.connector == connector)
        .map(|row| {
            let changing = connections.is_changing(&row.id);
            let oauth = row.kind == ConnectionKind::Oauth;
            AccountLine {
                id: row.id.clone(),
                label: connections.shown_label(row),
                status: AccountStatus::Connected,
                renaming: renaming
                    .filter(|(about, _)| *about == row.id)
                    .map(|(_, typed)| typed.clone()),
                can_rename: !changing && !removing,
                action: oauth.then(|| {
                    let live = !changing && !removing && connections.opening.is_none();
                    let label = if connections.reconnecting.as_deref() == Some(row.id.as_str()) {
                        "Opening…"
                    } else {
                        "Reconnect"
                    };
                    (label.to_string(), AccountAction::Reconnect, live)
                }),
                removable: !changing && !removing,
                lent: oauth.then(|| {
                    let count = state
                        .coworkers
                        .iter()
                        .filter(|b| connections.shows_lent(row, &b.id))
                        .count();
                    (count, !changing && !removing)
                }),
                note: connections
                    .not_renamed
                    .get(&row.id)
                    .cloned()
                    .or_else(|| connections.lend_refusal(&row.id, bot).map(str::to_string))
                    .or_else(|| connections.disconnect_refusal(&row.id).map(str::to_string))
                    .or_else(|| {
                        connections
                            .reconnect_refused
                            .as_ref()
                            .filter(|(about, _)| *about == row.id)
                            .map(|(_, why)| why.clone())
                    }),
            }
        })
        .collect();
    for attempt in market.attempts_for(connector) {
        let changing = market.attempt_changing.as_deref() == Some(attempt.id.as_str());
        let failed = attempt.status == "failed";
        lines.push(AccountLine {
            id: attempt.id.clone(),
            label: attempt.label.clone(),
            status: AccountStatus::NeedsAuth(if failed { attempt.error.clone() } else { None }),
            renaming: renaming
                .filter(|(about, _)| *about == attempt.id)
                .map(|(_, typed)| typed.clone()),
            can_rename: !changing && !removing,
            action: Some((
                if changing { "Opening…" } else { "Reopen" }.to_string(),
                AccountAction::Reopen,
                !changing && market.attempt_changing.is_none() && connections.opening.is_none(),
            )),
            removable: !changing && market.attempt_changing.is_none(),
            lent: None,
            note: market.attempt_refusals.get(&attempt.id).cloned(),
        });
    }
    AccountsCard {
        connector: connector.to_string(),
        title: "Accounts".into(),
        lines,
        can_add: connections.opening.is_none() && !removing,
        adding: connections.reconnecting.is_none()
            && connections.opening.as_deref() == Some(connector),
        add_refusal: connections
            .connect_refused
            .as_ref()
            .filter(|(service, _)| *service == connector)
            .map(|(_, why)| why.clone()),
        pasted: false,
    }
}

fn token_card(state: &AppState, plugin: &str, connector: &str, several: bool) -> AccountsCard {
    let connections = &state.connections;
    let market = &state.plugin_market;
    let modal = state.monitor_modal.as_ref();
    let removing = modal.is_some_and(|modal| modal.removing);
    let renaming = modal.and_then(|modal| modal.renaming.as_ref());
    let key = crate::state::sign_in_key(plugin, connector);
    // How the service adds an account (#364): at its own provider, or a pasted token. Until the
    // server says, Add waits rather than guess.
    let method = market.sign_in.get(&key);
    let oauth = matches!(method, Some(Loaded::Ready(m)) if m == "oauth");
    let known = matches!(method, Some(Loaded::Ready(_) | Loaded::Failed(_)));
    let bound: Vec<&str> = market
        .installation(plugin)
        .map(|install| {
            install
                .accounts
                .iter()
                .filter(|a| a.connector == connector)
                .map(|a| a.connection_id.as_str())
                .collect()
        })
        .unwrap_or_default();
    let saving = market.token.as_ref().is_some_and(|form| form.saving);
    let busy = market.authorizing.is_some();
    let mut lines: Vec<AccountLine> = connections
        .own_rows()
        .into_iter()
        .filter(|row| bound.contains(&row.id.as_str()))
        .map(|row| {
            let changing = connections.is_changing(&row.id);
            let action = if row.kind == ConnectionKind::Mcp {
                let opening = market.authorizing.as_deref() == Some(row.id.as_str());
                (
                    if opening { "Opening…" } else { "Reconnect" }.to_string(),
                    AccountAction::PluginReconnect,
                    !changing && !busy && !removing,
                )
            } else {
                (
                    "New token".into(),
                    AccountAction::ReplaceToken,
                    !changing && !saving,
                )
            };
            AccountLine {
                id: row.id.clone(),
                label: connections.shown_label(row),
                status: AccountStatus::Connected,
                renaming: renaming
                    .filter(|(about, _)| *about == row.id)
                    .map(|(_, typed)| typed.clone()),
                can_rename: !changing && !removing,
                action: Some(action),
                removable: !changing && !removing,
                lent: None,
                note: connections
                    .not_renamed
                    .get(&row.id)
                    .cloned()
                    .or_else(|| market.authorize_refusals.get(&row.id).cloned())
                    .or_else(|| connections.disconnect_refusal(&row.id).map(str::to_string)),
            }
        })
        .collect();
    for attempt in market.plugin_attempts(plugin, connector) {
        let changing = market.attempt_changing.as_deref() == Some(attempt.id.as_str());
        let failed = attempt.status == "failed";
        lines.push(AccountLine {
            id: attempt.id.clone(),
            label: attempt.label.clone(),
            status: AccountStatus::NeedsAuth(if failed { attempt.error.clone() } else { None }),
            renaming: renaming
                .filter(|(about, _)| *about == attempt.id)
                .map(|(_, typed)| typed.clone()),
            can_rename: !changing && !removing,
            action: Some((
                if changing { "Opening…" } else { "Reopen" }.to_string(),
                AccountAction::Reopen,
                !changing && market.attempt_changing.is_none() && !busy,
            )),
            removable: !changing && market.attempt_changing.is_none(),
            lent: None,
            note: market.attempt_refusals.get(&attempt.id).cloned(),
        });
    }
    AccountsCard {
        connector: connector.to_string(),
        title: if several {
            format!("Accounts · {connector}")
        } else {
            "Accounts".into()
        },
        lines,
        can_add: known && !saving && !removing && !busy,
        adding: market.authorizing.as_deref() == Some(key.as_str()),
        add_refusal: market
            .authorize_refusals
            .get(&key)
            .cloned()
            .or_else(|| match method {
                Some(Loaded::Failed(why)) => Some(why.clone()),
                _ => None,
            }),
        pasted: !oauth,
    }
}

fn removal_question(state: &AppState, id: &str) -> Option<String> {
    let connections = &state.connections;
    let own = connections.own_rows();
    let row = own.iter().find(|row| row.id == id)?;
    let name_of = |id: &str| {
        state
            .coworkers
            .iter()
            .find(|bot| bot.id == id)
            .map(|bot| bot.name.clone())
    };
    Some(connections::confirm_question(connections, row, name_of))
}

pub(crate) fn detail(state: &AppState, selection: &PluginSelection) -> Option<MarketDetail> {
    let modal = state.monitor_modal.as_ref()?;
    let question = |plugin_question: Option<String>| {
        if !modal.confirming {
            return None;
        }
        match &modal.removal {
            Some(id) => removal_question(state, id),
            None => plugin_question,
        }
    };
    match selection {
        PluginSelection::Service(name) => {
            let connections = &state.connections;
            let label = connections.connector_label(name);
            let listed = matches!(&connections.connectors, Some(ConnectorList::Listed(rows)) if rows.iter().any(|r| &r.name == name && r.plugin.is_none()));
            if !listed {
                return None;
            }
            Some(MarketDetail {
                title: label.clone(),
                description: format!(
                    "Sign in with your {label} accounts through this server. Each Bot uses the \
                     account you pick for it, or asks each time."
                ),
                source: None,
                action: None,
                refusal: None,
                accounts: vec![service_card(state, name)],
                parts_status: None,
                skills: Vec::new(),
                servers: Vec::new(),
                bot_switch: None,
                use_prompt: false,
                tools: Vec::new(),
                tools_status: None,
                apps: vec![name.clone()],
                unsupported: Vec::new(),
                info: vec![(
                    "Sign-in",
                    "Through this server; the app never sees a token".into(),
                )],
                question: question(None),
            })
        }
        PluginSelection::Plugin(name) => {
            let market = &state.plugin_market;
            let catalog = market.catalog.as_ref().and_then(Loaded::ready);
            let entry = catalog.and_then(|c| c.plugins.iter().find(|e| &e.name == name));
            let install = market.installation(name);
            if entry.is_none() && install.is_none() {
                return None;
            }
            let read = market.details.get(name);
            let parts_detail: Option<&PluginDetail> = read.and_then(Loaded::ready);
            // The parts installed when installed (what runs), else the pinned bundle's.
            let parts = install
                .map(|i| i.bundle.parts.clone())
                .or_else(|| parts_detail.map(|d| d.parts.clone()))
                .unwrap_or_default();
            let connectors = install
                .map(|i| i.connectors.clone())
                .or_else(|| parts_detail.map(|d| d.connectors.clone()))
                .unwrap_or_default();
            let parts_status = if install.is_some() || parts_detail.is_some() {
                None
            } else {
                match read {
                    Some(Loaded::Failed(why)) => Some(why.clone()),
                    _ => Some("Reading what this plugin brings…".into()),
                }
            };
            let changing = market.changing.as_ref().filter(|(n, _)| n == name);
            let action = match (install, changing) {
                (_, Some((_, true))) => DetailAction::Adding,
                (_, Some((_, false))) => DetailAction::Uninstalling,
                (Some(_), None) => DetailAction::Uninstall {
                    live: market.changing.is_none() && !modal.removing,
                },
                (None, None) => match entry.and_then(|e| e.unavailable_reason.clone()) {
                    Some(why) => DetailAction::Unavailable(why),
                    None => DetailAction::Add {
                        live: market.changing.is_none(),
                    },
                },
            };
            let (repository, revision) = match (install, entry) {
                (Some(i), _) => (i.repository.clone(), i.revision.clone()),
                (None, Some(e)) => (e.repository.clone(), e.revision.clone()),
                (None, None) => (String::new(), String::new()),
            };
            let path = entry.map(|e| e.path.clone()).unwrap_or_default();
            // The registry adapter reads GitHub sources only, so `repository` is `owner/repo` there.
            let source = (!repository.is_empty() && repository.contains('/')).then(|| {
                let mut url = format!("https://github.com/{repository}/tree/{revision}");
                if !path.is_empty() {
                    url.push('/');
                    url.push_str(&path);
                }
                (repository.clone(), url)
            });
            let accounts = if install.is_some() {
                connectors
                    .iter()
                    .map(|c| token_card(state, name, c, connectors.len() > 1))
                    .collect()
            } else {
                Vec::new()
            };
            let bot = state.active_bot_name();
            let switch = crate::components::monitor_modal::plugin_switches(state)
                .into_iter()
                .find(|s| &s.name == name);
            let prefix = format!("{name}.");
            let (tools, tools_status) = match state.coworker_tools.as_ref() {
                Some((_, ToolList::Listed(list))) => (
                    list.iter()
                        .filter(|t| t.name.starts_with(&prefix))
                        .map(|t| t.name[prefix.len()..].to_string())
                        .collect::<Vec<_>>(),
                    None,
                ),
                Some((_, ToolList::Unavailable(why))) => (Vec::new(), Some(why.clone())),
                _ => (Vec::new(), Some(connections::ASKING.to_string())),
            };
            let tools_status = tools_status.or_else(|| {
                (install.is_some() && tools.is_empty())
                    .then(|| format!("{bot} has no tools from it right now."))
            });
            let manifest = install
                .map(|i| &i.bundle.manifest)
                .or_else(|| parts_detail.and_then(|d| d.manifest.as_ref()));
            let mut info = Vec::new();
            // What it brings, counted from its parts as the server read them.
            let count = |n: usize, one: &str| match n {
                0 => None,
                1 => Some(format!("1 {one}")),
                n => Some(format!("{n} {one}s")),
            };
            let skills_n = parts
                .iter()
                .filter(|p| p.kind == "skill" && p.supported)
                .count();
            let servers_n = parts
                .iter()
                .filter(|p| p.kind == "mcp" && p.supported)
                .count();
            let capabilities: Vec<String> = [
                count(connectors.len(), "app"),
                count(servers_n, "server"),
                count(skills_n, "skill"),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !capabilities.is_empty() {
                info.push(("Capabilities", capabilities.join(", ")));
            }
            if let Some(author) = manifest.and_then(|m| m.author.as_ref()) {
                info.push(("Developer", author.name.clone()));
            }
            if let Some(category) = entry.and_then(|e| e.category.as_deref()) {
                info.push(("Category", category_title(category)));
            }
            let website = entry
                .and_then(|e| e.homepage.clone())
                .or_else(|| manifest.and_then(|m| m.homepage.clone()));
            if let Some(site) = website {
                let shown = site
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_string();
                info.push(("Website", shown));
            }
            info.push((
                "Availability",
                match entry {
                    None => "No longer listed in the marketplace".into(),
                    Some(e) if e.unavailable_reason.is_some() => {
                        "Not installable on this server".into()
                    }
                    Some(_) => "Public".into(),
                },
            ));
            if let Some(version) = manifest.and_then(|m| m.version.clone()) {
                info.push(("Version", version));
            }
            if !revision.is_empty() {
                info.push(("Pinned commit", short(&revision)));
            }
            let accounts_bound = install.map(|i| i.accounts.len()).unwrap_or(0);
            Some(MarketDetail {
                title: name.clone(),
                description: entry
                    .map(|e| e.description.clone())
                    .filter(|d| !d.is_empty())
                    .or_else(|| install.and_then(|i| i.bundle.manifest.description.clone()))
                    .unwrap_or_default(),
                source,
                action: Some(action),
                refusal: market.refusals.get(name).cloned(),
                accounts,
                parts_status,
                skills: parts
                    .iter()
                    .filter(|p| p.kind == "skill" && p.supported)
                    .map(|p| p.name.clone())
                    .collect(),
                servers: parts
                    .iter()
                    .filter(|p| p.kind == "mcp" && p.supported)
                    .map(|p| p.name.clone())
                    .collect(),
                use_prompt: install.is_some_and(|install| !install.accounts.is_empty())
                    && switch.as_ref().is_some_and(|s| !s.on && s.live)
                    && state.active_coworker_id.as_ref().is_some_and(|bot| {
                        !market.use_declined.contains(&(name.clone(), bot.clone()))
                    }),
                bot_switch: install.and(switch).map(|s| (s.on, s.live, s.note)),
                tools,
                tools_status,
                apps: connectors.clone(),
                unsupported: parts
                    .iter()
                    .filter(|p| !p.supported)
                    .map(|p| {
                        (
                            format!("{} · {}", p.name, p.kind),
                            p.reason
                                .clone()
                                .unwrap_or_else(|| "Not run by this server".into()),
                        )
                    })
                    .collect(),
                info,
                question: question(Some(if accounts_bound > 0 {
                    format!(
                        "Uninstall {name}? Its {accounts_bound} pasted account{} go with it, and \
                         every Bot stops using it.",
                        if accounts_bound == 1 { "" } else { "s" }
                    )
                } else {
                    format!("Uninstall {name}? Every Bot stops using it.")
                })),
            })
        }
        PluginSelection::Connection(_) | PluginSelection::Skill(_) => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------

use crate::components::fields::field_input;
use crate::components::switch::Switch;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Icon, IconName, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

gpui_kit::actions!(
    nativechat,
    [MoveMarketPrevious, MoveMarketNext, OpenMarketHighlight]
);

/// The search field's context, nested in gpui's Input context, so these bindings take the arrows
/// and Enter before the field's own caret keys (the model picker's pattern, #190).
const SEARCH_CONTEXT: &str = "MarketSearch";
const SEARCH_INPUT_CONTEXT: &str = "MarketSearch > Input";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveMarketPrevious, Some(SEARCH_INPUT_CONTEXT)),
        KeyBinding::new("down", MoveMarketNext, Some(SEARCH_INPUT_CONTEXT)),
        KeyBinding::new("ctrl-p", MoveMarketPrevious, Some(SEARCH_INPUT_CONTEXT)),
        KeyBinding::new("ctrl-n", MoveMarketNext, Some(SEARCH_INPUT_CONTEXT)),
        KeyBinding::new("enter", OpenMarketHighlight, Some(SEARCH_INPUT_CONTEXT)),
    ]);
}

/// The inputs the marketplace draws, owned by the modal view.
pub(crate) struct MarketInputs<'a> {
    pub search: &'a Entity<InputState>,
    pub bots: Option<&'a Entity<InputState>>,
    pub token: Option<&'a Entity<InputState>>,
    pub rename: Option<&'a Entity<InputState>>,
}

/// Where each visible row sits among the scroll list's children, so Up and Down can bring the
/// highlighted one into view.
pub(crate) struct Laid {
    /// The scroll list's children.
    pub blocks: Vec<AnyElement>,
    /// For each visible row, in order, the index of the scroll list's child that holds it.
    pub row_blocks: Vec<usize>,
}

fn pill(
    id: impl Into<String>,
    text: impl Into<SharedString>,
    live: bool,
    theme: &Theme,
    on_click: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .flex_shrink_0()
        .px(px(14.))
        .py(px(6.))
        .rounded_full()
        .bg(theme.secondary)
        .text_color(theme.secondary_foreground)
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .when(live, |this| {
            this.cursor_pointer().hover(|style| style.bg(theme.muted))
        })
        .when(!live, |this| this.opacity(0.5))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            if live {
                on_click(cx);
            }
        })
        .child(text.into())
}

fn link(
    id: impl Into<String>,
    text: impl Into<SharedString>,
    live: bool,
    color: Hsla,
    on_click: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let selector = id.clone();
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .flex_shrink_0()
        .text_sm()
        .text_color(color)
        .when(live, |this| {
            this.cursor_pointer().hover(|s| s.opacity(0.75))
        })
        .when(!live, |this| this.opacity(0.45))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            if live {
                on_click(cx);
            }
        })
        .child(text.into())
}

/// A plugin's tile: its initial on a neutral square, as Grok Bot draws a plugin with no icon of
/// its own. No plugin's mark is guessed at.
fn tile(title: &str, size: f32, theme: &Theme) -> Div {
    let initial = title
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    div()
        .flex_shrink_0()
        .size(px(size))
        .rounded(px(size * 0.24))
        .bg(theme.secondary)
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme.muted_foreground)
        .text_size(px(size * 0.4))
        .font_weight(FontWeight::SEMIBOLD)
        .child(initial)
}

fn row_element(
    app: &Entity<AppState>,
    row: &MarketRow,
    highlighted: bool,
    theme: &Theme,
) -> AnyElement {
    let open_app = app.clone();
    let add_app = app.clone();
    let selection = row.selection.clone();
    let add_selection = row.selection.clone();
    let id = row.selection.detail_id();
    let selector = id.clone();
    let action: AnyElement = match &row.action {
        RowAction::Add { live } => pill(add_id(&row.selection), "Add", *live, theme, move |cx| {
            add_app.update(cx, |state, cx| match &add_selection {
                PluginSelection::Plugin(name) => state.install_market_plugin(name.clone(), cx),
                PluginSelection::Service(name) => state.connect_service(name.clone(), cx),
                _ => {}
            })
        })
        .into_any_element(),
        RowAction::Adding => div()
            .id(SharedString::from(add_id(&row.selection)))
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("Adding…")
            .into_any_element(),
        RowAction::Added => h_flex()
            .id(SharedString::from(add_id(&row.selection)))
            .gap(px(4.))
            .items_center()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(
                Icon::new(IconName::Check)
                    .size(px(14.))
                    .text_color(theme.success),
            )
            .child("Added")
            .into_any_element(),
        RowAction::Unavailable => div()
            .id(SharedString::from(add_id(&row.selection)))
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("Unavailable")
            .into_any_element(),
    };
    h_flex()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .flex_1()
        .min_w(px(0.))
        .gap(px(14.))
        .items_center()
        .px(px(12.))
        .py(px(10.))
        .rounded(px(14.))
        .cursor_pointer()
        .when(highlighted, |this| this.bg(theme.list_hover))
        .hover(|style| style.bg(theme.list_hover))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            open_app.update(cx, |state, cx| {
                state.open_market_detail(selection.clone(), cx)
            });
        })
        .child(tile(&row.title, 44., theme))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(row.title.clone()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(row.description.clone()),
                ),
        )
        .child(action)
        .into_any_element()
}

/// The marketplace pages: header, search, then the sections as the scroll list's children.
pub(crate) fn browse(
    app: &Entity<AppState>,
    modal: &MonitorModal,
    inputs: &MarketInputs,
    columns: usize,
    theme: &Theme,
    cx: &App,
) -> (AnyElement, Laid) {
    let state = app.read(cx);
    let back_app = app.clone();
    let installed_app = app.clone();
    let paged = modal.page != MarketPage::Browse;
    let title = match &modal.page {
        MarketPage::Browse => "Marketplace".to_string(),
        MarketPage::Installed => "Installed".to_string(),
        MarketPage::Category(category) => category_title(category),
    };
    let count = installed_count(state);
    let header = h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .child(
            h_flex()
                .items_center()
                .gap(px(8.))
                .when(paged, |this| {
                    this.child(
                        div()
                            .id(BACK)
                            .debug_selector(|| BACK.into())
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                back_app.update(cx, |state, cx| {
                                    state.set_market_page(MarketPage::Browse, cx)
                                })
                            })
                            .child(
                                Icon::new(IconName::ChevronLeft)
                                    .size(px(18.))
                                    .text_color(theme.muted_foreground),
                            ),
                    )
                })
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                ),
        )
        .when(modal.page != MarketPage::Installed, |this| {
            this.child(
                h_flex()
                    .id(INSTALLED)
                    .debug_selector(|| INSTALLED.into())
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .hover(|s| s.text_color(theme.foreground))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        installed_app.update(cx, |state, cx| {
                            state.set_market_page(MarketPage::Installed, cx)
                        })
                    })
                    .child(format!("{count} installed"))
                    .child(Icon::new(IconName::ChevronRight).size(px(14.))),
            )
        });
    let move_up = app.clone();
    let move_down = app.clone();
    let open = app.clone();
    let search = div()
        .id(SEARCH)
        .debug_selector(|| SEARCH.into())
        .w_full()
        .key_context(SEARCH_CONTEXT)
        .on_action(move |_: &MoveMarketPrevious, _, cx| {
            move_up.update(cx, |state, cx| state.move_market_highlight(-1, cx))
        })
        .on_action(move |_: &MoveMarketNext, _, cx| {
            move_down.update(cx, |state, cx| state.move_market_highlight(1, cx))
        })
        .on_action(move |_: &OpenMarketHighlight, _, cx| {
            open.update(cx, |state, cx| state.open_market_highlight(cx))
        })
        .child(
            field_input(inputs.search)
                .rounded_full()
                .prefix(
                    Icon::new(IconName::Search)
                        .size(px(16.))
                        .text_color(theme.muted_foreground),
                )
                .cleanable(true),
        );
    let top = v_flex()
        .w_full()
        .gap(px(16.))
        .child(header)
        .child(search)
        .into_any_element();

    let mut blocks: Vec<AnyElement> = Vec::new();
    let mut row_blocks = Vec::new();
    let status = status_lines(state);
    if !status.is_empty() {
        blocks.push(
            div()
                .id(STATUS)
                .debug_selector(|| STATUS.into())
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(status.join("\n"))
                .into_any_element(),
        );
    }
    let sections = sections_for(state, modal);
    let mut index = 0usize;
    for section in &sections {
        let view_all = section.view_all.clone();
        let all_app = app.clone();
        let section_id = section.id.clone();
        blocks.push(
            h_flex()
                .id(SharedString::from(section_id.clone()))
                .debug_selector(move || section_id)
                .w_full()
                .pt(px(14.))
                .px(px(12.))
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_base()
                        .font_weight(FontWeight::MEDIUM)
                        .child(section.title.clone()),
                )
                .when_some(view_all, |this, category| {
                    let id = view_all_id(&category);
                    this.child(link(
                        id,
                        "View all",
                        true,
                        theme.muted_foreground,
                        move |cx| {
                            all_app.update(cx, |state, cx| {
                                state.set_market_page(MarketPage::Category(category.clone()), cx)
                            })
                        },
                    ))
                })
                .into_any_element(),
        );
        for line in section.rows.chunks(columns.max(1)) {
            let mut line_el = h_flex().w_full().gap(px(16.));
            for row in line {
                line_el = line_el.child(row_element(app, row, index == modal.highlight, theme));
                row_blocks.push(blocks.len());
                index += 1;
            }
            // A last line of one keeps the column width of a full one.
            for _ in line.len()..columns.max(1) {
                line_el = line_el.child(div().flex_1());
            }
            blocks.push(line_el.into_any_element());
        }
    }
    // The person's private skills stay where the Installed list always had them: each opens its
    // own detail and is switched for the open Bot here.
    if modal.page == MarketPage::Installed && modal.query.trim().is_empty() {
        let skills = state
            .skills_card()
            .map(|card| crate::components::monitor_modal::private_skill_rows(&card))
            .unwrap_or_default();
        if !skills.is_empty() {
            blocks.push(
                div()
                    .id("market-section-skills")
                    .debug_selector(|| "market-section-skills".into())
                    .pt(px(14.))
                    .px(px(12.))
                    .text_base()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Your skills")
                    .into_any_element(),
            );
        }
        for row in skills {
            let selection = PluginSelection::Skill(row.id.clone());
            let id = selection.detail_id();
            let selector = id.clone();
            let open = app.clone();
            let switch = app.clone();
            let skill = row.id.clone();
            let subtitle = if row.switched_off {
                "Switched off in Settings → Skills".to_string()
            } else {
                row.first_line.clone()
            };
            blocks.push(
                h_flex()
                    .id(SharedString::from(id))
                    .debug_selector(move || selector)
                    .w_full()
                    .gap(px(14.))
                    .items_center()
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(14.))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.list_hover))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        open.update(cx, |state, cx| {
                            state.select_monitor_plugin(Some(selection.clone()), cx)
                        })
                    })
                    .child(tile(&row.title, 44., theme))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(row.title.clone()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .truncate()
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        Switch::new(ElementId::Name(
                            PluginSelection::Skill(row.id.clone()).switch_id().into(),
                        ))
                        .checked(row.on)
                        .small()
                        .disabled(!row.live)
                        .accessibility_label(format!(
                            "Attach {} to {}",
                            row.title,
                            state.active_bot_name()
                        ))
                        .on_click(move |on, _, cx| {
                            switch.update(cx, |state, cx| {
                                state.switch_bot_skill(skill.clone(), *on, cx)
                            })
                        }),
                    )
                    .into_any_element(),
            );
        }
    }
    if sections.is_empty() && status.is_empty() && blocks.is_empty() {
        let words = if !modal.query.trim().is_empty() {
            format!("No plugins match “{}”.", modal.query.trim())
        } else if modal.page == MarketPage::Installed {
            "Nothing is installed yet. Add a plugin from the marketplace.".into()
        } else {
            "The marketplace lists no plugins.".into()
        };
        blocks.push(
            div()
                .id(EMPTY)
                .debug_selector(|| EMPTY.into())
                .py(px(24.))
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(words)
                .into_any_element(),
        );
    }
    (top, Laid { blocks, row_blocks })
}

fn section_label(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .px(px(4.))
        .pt(px(10.))
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(text.into())
}

fn card(theme: &Theme) -> Div {
    v_flex()
        .w_full()
        .rounded(px(14.))
        .bg(theme.secondary)
        .px(px(16.))
}

fn card_row(theme: &Theme, first: bool) -> Div {
    h_flex()
        .w_full()
        .min_h(px(52.))
        .py(px(10.))
        .gap(px(10.))
        .items_center()
        .when(!first, |this| this.border_t_1().border_color(theme.border))
}

#[allow(clippy::too_many_arguments)]
fn accounts_card(
    app: &Entity<AppState>,
    plugin: Option<&str>,
    card_model: &AccountsCard,
    inputs: &MarketInputs,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    let state = app.read(cx);
    let token = state.plugin_market.token.as_ref().filter(|form| {
        form.connector == card_model.connector && Some(form.plugin.as_str()) == plugin
    });
    let mut body = card(theme);
    let mut first = true;
    for line in &card_model.lines {
        let rename_app = app.clone();
        let rename_target = line.id.clone();
        let mut row = card_row(theme, first);
        first = false;
        let label: AnyElement = match (&line.renaming, inputs.rename) {
            (Some(_), Some(input)) => {
                let save = app.clone();
                let cancel = app.clone();
                h_flex()
                    .flex_1()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .id(SharedString::from(
                                crate::components::monitor_modal::rename_field_id(&line.id),
                            ))
                            .debug_selector({
                                let id =
                                    crate::components::monitor_modal::rename_field_id(&line.id);
                                move || id
                            })
                            .flex_1()
                            .child(field_input(input)),
                    )
                    .child(pill(
                        "monitor-account-save",
                        "Save",
                        true,
                        theme,
                        move |cx| save.update(cx, |state, cx| state.save_account_rename(cx)),
                    ))
                    .child(link(
                        "monitor-account-cancel",
                        "Cancel",
                        true,
                        theme.muted_foreground,
                        move |cx| cancel.update(cx, |state, cx| state.cancel_account_rename(cx)),
                    ))
                    .into_any_element()
            }
            _ => h_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(6.))
                .items_center()
                .child(
                    div()
                        .id(SharedString::from(
                            crate::components::monitor_modal::account_id(&line.id),
                        ))
                        .debug_selector({
                            let id = crate::components::monitor_modal::account_id(&line.id);
                            move || id
                        })
                        .text_base()
                        .truncate()
                        .child(line.label.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(
                            crate::components::monitor_modal::rename_id(&line.id),
                        ))
                        .debug_selector({
                            let id = crate::components::monitor_modal::rename_id(&line.id);
                            move || id
                        })
                        .when(line.can_rename, |this| this.cursor_pointer())
                        .when(!line.can_rename, |this| this.opacity(0.4))
                        .on_mouse_down(MouseButton::Left, {
                            let live = line.can_rename;
                            move |_, _, cx| {
                                cx.stop_propagation();
                                if live {
                                    rename_app.update(cx, |state, cx| {
                                        state.start_account_rename(rename_target.clone(), cx)
                                    });
                                }
                            }
                        })
                        .child(
                            svg()
                                .path("icons/pencil.svg")
                                .size(px(14.))
                                .text_color(theme.muted_foreground),
                        ),
                )
                .into_any_element(),
        };
        row = row.child(label);
        if line.renaming.is_none() {
            if let Some((count, live)) = line.lent {
                let open = app.clone();
                let id = line.id.clone();
                row = row.child(
                    h_flex()
                        .id(SharedString::from(bots_id(&line.id)))
                        .debug_selector({
                            let sel = bots_id(&line.id);
                            move || sel
                        })
                        .gap(px(6.))
                        .items_center()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded_full()
                        .border_1()
                        .border_color(theme.border)
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .when(live, |this| {
                            this.cursor_pointer().hover(|s| s.bg(theme.muted))
                        })
                        .when(!live, |this| this.opacity(0.5))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            if live {
                                open.update(cx, |state, cx| {
                                    state.open_account_bots(Some(id.clone()), cx)
                                });
                            }
                        })
                        .child(
                            svg()
                                .path("icons/groups.svg")
                                .size(px(14.))
                                .text_color(theme.muted_foreground),
                        )
                        .child(format!("{count} Bot{}", if count == 1 { "" } else { "s" })),
                );
            }
            row = row.child(match &line.status {
                AccountStatus::Connected => {
                    div().text_sm().text_color(theme.success).child("Connected")
                }
                AccountStatus::NeedsAuth(_) => div()
                    .text_sm()
                    .text_color(theme.warning)
                    .child("Needs Auth"),
            });
            if let Some((text, action, live)) = &line.action {
                let act_app = app.clone();
                let id = line.id.clone();
                let action = *action;
                let plugin = plugin.map(str::to_string);
                let connector = card_model.connector.clone();
                let element_id = match action {
                    AccountAction::Reconnect => {
                        crate::components::monitor_modal::reconnect_id(&line.id)
                    }
                    AccountAction::Reopen => reopen_id(&line.id),
                    AccountAction::ReplaceToken => replace_token_id(&line.id),
                    AccountAction::PluginReconnect => {
                        crate::components::monitor_modal::reconnect_id(&line.id)
                    }
                };
                row = row.child(link(
                    element_id,
                    text.clone(),
                    *live,
                    theme.foreground,
                    move |cx| {
                        act_app.update(cx, |state, cx| match action {
                            AccountAction::Reconnect => state.reconnect_connection(id.clone(), cx),
                            AccountAction::Reopen => state.reopen_attempt(id.clone(), cx),
                            AccountAction::PluginReconnect => {
                                if let Some(plugin) = &plugin {
                                    state.authorize_plugin(
                                        plugin.clone(),
                                        connector.clone(),
                                        Some(id.clone()),
                                        cx,
                                    )
                                }
                            }
                            AccountAction::ReplaceToken => {
                                if let Some(plugin) = &plugin {
                                    state.start_market_token(
                                        plugin.clone(),
                                        connector.clone(),
                                        Some(id.clone()),
                                        cx,
                                    )
                                }
                            }
                        })
                    },
                ));
            }
            let remove_app = app.clone();
            let id = line.id.clone();
            let waiting = matches!(line.status, AccountStatus::NeedsAuth(_));
            let remove_id = if waiting {
                dismiss_id(&line.id)
            } else {
                remove_account_id(&line.id)
            };
            row = row.child(link(
                remove_id,
                "Remove",
                line.removable,
                theme.muted_foreground,
                move |cx| {
                    remove_app.update(cx, |state, cx| {
                        if waiting {
                            state.dismiss_attempt(id.clone(), cx)
                        } else {
                            state.ask_account_remove(Some(id.clone()), cx)
                        }
                    })
                },
            ));
        }
        body = body.child(row);
        let why = match &line.status {
            AccountStatus::NeedsAuth(Some(why)) => Some(why.clone()),
            _ => None,
        };
        if let Some(note) = line.note.clone().or(why) {
            body = body.child(
                div()
                    .id(SharedString::from(
                        crate::components::monitor_modal::account_note_id(&line.id),
                    ))
                    .pb(px(8.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(note),
            );
        }
    }
    if let (Some(form), Some(input)) = (token, inputs.token) {
        let save = app.clone();
        let cancel = app.clone();
        let saving = form.saving;
        body = body.child(
            card_row(theme, first)
                .child(
                    div()
                        .id(TOKEN_FIELD)
                        .debug_selector(|| TOKEN_FIELD.into())
                        .flex_1()
                        .child(field_input(input)),
                )
                .child(pill(
                    TOKEN_SAVE,
                    if saving { "Saving…" } else { "Save" },
                    !saving,
                    theme,
                    move |cx| save.update(cx, |state, cx| state.save_market_token(cx)),
                ))
                .child(link(
                    TOKEN_CANCEL,
                    "Cancel",
                    !saving,
                    theme.muted_foreground,
                    move |cx| cancel.update(cx, |state, cx| state.cancel_market_token(cx)),
                )),
        );
        first = false;
        if let Some(why) = form.refusal.clone() {
            body = body.child(
                div()
                    .id(TOKEN_REFUSAL)
                    .debug_selector(|| TOKEN_REFUSAL.into())
                    .pb(px(8.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(why),
            );
        }
    }
    let add_app = app.clone();
    let connector = card_model.connector.clone();
    let pasted = card_model.pasted;
    let plugin_name = plugin.map(str::to_string);
    let add_id = add_account_id(&card_model.connector);
    let add_selector = add_id.clone();
    let live = card_model.can_add && token.is_none();
    body = body.child(
        card_row(theme, first)
            .id(SharedString::from(add_id))
            .debug_selector(move || add_selector)
            .when(live, |this| this.cursor_pointer())
            .when(!live, |this| this.opacity(0.5))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                if !live {
                    return;
                }
                add_app.update(cx, |state, cx| match (&plugin_name, pasted) {
                    (Some(plugin), true) => {
                        state.start_market_token(plugin.clone(), connector.clone(), None, cx)
                    }
                    // An installed plugin whose server signs people in itself (#364).
                    (Some(plugin), false) => {
                        state.authorize_plugin(plugin.clone(), connector.clone(), None, cx)
                    }
                    (None, _) => state.connect_service(connector.clone(), cx),
                })
            })
            .child(
                Icon::new(IconName::Plus)
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
            )
            .child(div().text_base().text_color(theme.muted_foreground).child(
                if card_model.adding {
                    "Opening sign-in…"
                } else if card_model.lines.is_empty() {
                    "Add Account"
                } else {
                    "Add Another Account"
                },
            )),
    );
    let mut out = v_flex()
        .id(SharedString::from(format!(
            "market-accounts-{}",
            card_model.connector
        )))
        .gap(px(6.))
        .child(section_label(card_model.title.clone(), theme))
        .child(body);
    if let Some(why) = card_model.add_refusal.clone() {
        out = out.child(
            div()
                .id(crate::components::monitor_modal::ADD_ACCOUNT_ERROR)
                .text_xs()
                .text_color(theme.danger)
                .child(why),
        );
    }
    // Which account the open Bot uses, once there is one to pick (#359).
    if card_model
        .lines
        .iter()
        .any(|l| l.status == AccountStatus::Connected)
    {
        out = out.child(picker(app, &card_model.connector, card_model, theme, cx));
    }
    out.into_any_element()
}

fn picker(
    app: &Entity<AppState>,
    connector: &str,
    card_model: &AccountsCard,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    let state = app.read(cx);
    let connections = &state.connections;
    let bot = state.active_coworker_id.as_deref().unwrap_or_default();
    let picked = connections.shown_pick(bot, connector);
    let live = picked.is_some() && !connections.is_picking(bot, connector);
    let note = match (connections.pick_refusal(bot, connector), &connections.pins) {
        (Some(why), _) => Some(why.to_string()),
        (None, None | Some(crate::state::PinList::Loading)) => {
            Some(connections::ASKING.to_string())
        }
        (None, Some(crate::state::PinList::Unavailable(why))) => Some(why.clone()),
        (None, Some(crate::state::PinList::Listed(_))) => None,
    };
    let mut row = h_flex()
        .id(crate::components::monitor_modal::PICKER)
        .debug_selector(|| crate::components::monitor_modal::PICKER.into())
        .flex_wrap()
        .gap(px(8.))
        .items_center()
        .pt(px(4.))
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(format!("{} uses", state.active_bot_name())),
        );
    let choice = |id: String, text: String, on: bool, target: Option<String>| {
        let app = app.clone();
        let service = connector.to_string();
        let selector = id.clone();
        div()
            .id(SharedString::from(id))
            .debug_selector(move || selector)
            .px(px(12.))
            .py(px(4.))
            .rounded_full()
            .border_1()
            .border_color(if on { theme.foreground } else { theme.border })
            .text_sm()
            .when(on, |this| this.bg(theme.secondary))
            .when(live, |this| this.cursor_pointer())
            .when(!live, |this| this.opacity(0.5))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                if live {
                    app.update(cx, |state, cx| {
                        state.pick_account(service.clone(), target.clone(), cx)
                    });
                }
            })
            .child(text)
    };
    for line in card_model
        .lines
        .iter()
        .filter(|l| l.status == AccountStatus::Connected)
    {
        let on = picked.as_ref() == Some(&Some(line.id.clone()));
        row = row.child(choice(
            crate::components::monitor_modal::pick_id(&line.id),
            line.label.clone(),
            on,
            Some(line.id.clone()),
        ));
    }
    row = row.child(choice(
        crate::components::monitor_modal::PICK_ASK.into(),
        crate::components::monitor_modal::ASK_EACH_TIME.into(),
        picked.as_ref() == Some(&None),
        None,
    ));
    let mut out = v_flex().gap(px(4.)).child(row);
    if let Some(note) = note {
        out = out.child(
            div()
                .id(crate::components::monitor_modal::PICKER_NOTE)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(note),
        );
    }
    out.into_any_element()
}

/// A plugin's or a service's detail, in the Grok Bot layout.
pub(crate) fn detail_view(
    app: &Entity<AppState>,
    modal: &MonitorModal,
    selection: &PluginSelection,
    inputs: &MarketInputs,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    let state = app.read(cx);
    let back_app = app.clone();
    let back = div()
        .id(crate::components::monitor_modal::BACK)
        .debug_selector(|| crate::components::monitor_modal::BACK.into())
        .cursor_pointer()
        .when(!modal.removing, |this| {
            this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                back_app.update(cx, |state, cx| state.close_market_detail(cx))
            })
        })
        .child(
            Icon::new(IconName::ChevronLeft)
                .size(px(18.))
                .text_color(theme.muted_foreground),
        );
    if let Some(account) = modal.bots_for.clone() {
        return bots_view(app, modal, &account, inputs, theme, cx);
    }
    let Some(model) = detail(state, selection) else {
        return v_flex()
            .id(DETAIL)
            .gap(px(12.))
            .child(back)
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("This plugin is no longer listed or installed."),
            )
            .into_any_element();
    };
    let plugin = match selection {
        PluginSelection::Plugin(name) => Some(name.clone()),
        _ => None,
    };
    let action: Option<AnyElement> = model.action.as_ref().map(|action| {
        let act = app.clone();
        let name = plugin.clone().unwrap_or_default();
        match action {
            DetailAction::Add { live } => pill(DETAIL_ACTION, "Add", *live, theme, move |cx| {
                act.update(cx, |state, cx| {
                    state.install_market_plugin(name.clone(), cx)
                })
            })
            .into_any_element(),
            DetailAction::Uninstall { live } => pill(
                DETAIL_ACTION,
                "Uninstall",
                *live && !modal.confirming,
                theme,
                move |cx| act.update(cx, |state, cx| state.ask_market_uninstall(true, cx)),
            )
            .into_any_element(),
            DetailAction::Adding => {
                pill(DETAIL_ACTION, "Adding…", false, theme, |_| {}).into_any_element()
            }
            DetailAction::Uninstalling => {
                pill(DETAIL_ACTION, "Uninstalling…", false, theme, |_| {}).into_any_element()
            }
            DetailAction::Unavailable(_) => {
                pill(DETAIL_ACTION, "Unavailable", false, theme, |_| {}).into_any_element()
            }
        }
    });
    let source = model.source.clone();
    let header = h_flex()
        .w_full()
        .gap(px(16.))
        .items_center()
        .child(tile(&model.title, 64., theme))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(2.))
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(model.title.clone()),
                )
                .when_some(source, |this, (repository, url)| {
                    this.child(link(
                        SOURCE,
                        format!("View Source ↗  {repository}"),
                        true,
                        theme.muted_foreground,
                        move |cx| cx.open_url(&url),
                    ))
                }),
        )
        .when_some(action, |this, action| this.child(action));
    let mut body = v_flex()
        .id(DETAIL)
        .debug_selector(|| DETAIL.into())
        .w_full()
        .gap(px(12.))
        .child(back)
        .child(header);
    if !model.description.is_empty() {
        body = body.child(
            div()
                .text_base()
                .text_color(theme.muted_foreground)
                .child(model.description.clone()),
        );
    }
    if let Some(DetailAction::Unavailable(why)) = &model.action {
        body = body.child(
            div()
                .id(DETAIL_REFUSAL)
                .text_sm()
                .text_color(theme.warning)
                .child(why.clone()),
        );
    }
    if let Some(why) = model.refusal.clone() {
        body = body.child(
            div()
                .id(DETAIL_REFUSAL)
                .debug_selector(|| DETAIL_REFUSAL.into())
                .text_sm()
                .text_color(theme.danger)
                .child(why),
        );
    }
    if let Some(question) = model.question.clone() {
        let yes = app.clone();
        let no = app.clone();
        let account = modal.removal.is_some();
        body = body.child(
            v_flex()
                .id("monitor-plugin-remove-question")
                .debug_selector(|| "monitor-plugin-remove-question".into())
                .gap(px(10.))
                .p(px(14.))
                .rounded(px(14.))
                .border_1()
                .border_color(theme.border)
                .child(div().text_sm().child(question))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .child(pill(
                            if account { REMOVE_YES } else { UNINSTALL_YES },
                            if account { "Remove" } else { "Uninstall" },
                            !modal.removing,
                            theme,
                            move |cx| {
                                yes.update(cx, |state, cx| {
                                    if account {
                                        state.remove_monitor_plugin(cx)
                                    } else {
                                        state.uninstall_market_plugin(cx)
                                    }
                                })
                            },
                        ))
                        .child(link(
                            if account { REMOVE_NO } else { UNINSTALL_NO },
                            "Cancel",
                            true,
                            theme.muted_foreground,
                            move |cx| {
                                no.update(cx, |state, cx| {
                                    if account {
                                        state.ask_account_remove(None, cx)
                                    } else {
                                        state.ask_market_uninstall(false, cx)
                                    }
                                })
                            },
                        )),
                ),
        );
    }
    if model.use_prompt
        && let Some(name) = plugin.clone()
    {
        let bot = state.active_bot_name();
        let (on, later) = (app.clone(), app.clone());
        let (turn_on, not_now) = (name.clone(), name.clone());
        body = body.child(
            card(theme).child(
                card_row(theme, false)
                    .id(SharedString::from(format!("market-use-prompt-{name}")))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("Use {} in {bot}?", model.title)),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!(
                                    "It's off for {bot}, which uses it only when you tag @{name}."
                                ),
                            )),
                    )
                    .child(
                        h_flex()
                            .gap(px(12.))
                            .items_center()
                            .child(link(
                                format!("market-use-prompt-not-now-{name}"),
                                "Not now",
                                true,
                                theme.muted_foreground,
                                move |cx| {
                                    later.update(cx, |state, cx| {
                                        state.decline_use_prompt(not_now.clone(), cx)
                                    })
                                },
                            ))
                            .child(pill(
                                format!("market-use-prompt-on-{name}"),
                                "Turn on",
                                true,
                                theme,
                                move |cx| {
                                    on.update(cx, |state, cx| {
                                        state.switch_ceiling_tool(turn_on.clone(), true, cx)
                                    })
                                },
                            )),
                    ),
            ),
        );
    }
    for accounts in &model.accounts {
        body = body.child(accounts_card(
            app,
            plugin.as_deref(),
            accounts,
            inputs,
            theme,
            cx,
        ));
    }
    if let Some(status) = model.parts_status.clone() {
        body = body.child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(status),
        );
    }
    if plugin.is_some() && model.parts_status.is_none() {
        // Tools: the open Bot's switch for the plugin, and the tools it has from it right now.
        let toggle = app.clone();
        let open = modal.tools_open;
        let summary = if model.tools.is_empty() {
            format!(
                "{} server{}",
                model.servers.len(),
                if model.servers.len() == 1 { "" } else { "s" }
            )
        } else {
            format!(
                "{} tool{} for {}",
                model.tools.len(),
                if model.tools.len() == 1 { "" } else { "s" },
                state.active_bot_name()
            )
        };
        let mut tools = card(theme).child(
            card_row(theme, true)
                .id(TOOLS)
                .debug_selector(|| TOOLS.into())
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    toggle.update(cx, |state, cx| state.toggle_market_tools(cx))
                })
                .child(div().flex_1().text_base().child(summary))
                .when_some(model.bot_switch.clone(), |this, (on, live, _)| {
                    let name = plugin.clone().unwrap_or_default();
                    let switch = app.clone();
                    this.child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("Use in {}", state.active_bot_name()))
                            .child(
                                Switch::new(ElementId::Name(
                                    crate::components::monitor_modal::plugin_switch_id(&name)
                                        .into(),
                                ))
                                .checked(on)
                                .small()
                                .disabled(!live)
                                .accessibility_label(format!(
                                    "Use {name} in {}",
                                    state.active_bot_name()
                                ))
                                .on_click(move |on, _, cx| {
                                    switch.update(cx, |state, cx| {
                                        state.switch_ceiling_tool(name.clone(), *on, cx)
                                    })
                                }),
                            ),
                    )
                })
                .child(
                    Icon::new(if open {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
                ),
        );
        if open {
            for server in &model.servers {
                tools = tools.child(
                    card_row(theme, false)
                        .child(div().text_sm().child(format!("{server} · MCP server"))),
                );
            }
            for tool in &model.tools {
                tools =
                    tools.child(card_row(theme, false).child(div().text_sm().child(tool.clone())));
            }
            if let Some(status) = model.tools_status.clone() {
                tools = tools.child(
                    card_row(theme, false).child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(status),
                    ),
                );
            }
        }
        body = body.child(section_label("Tools", theme)).child(tools);
        if let Some((_, _, Some(note))) = model.bot_switch.clone() {
            let id = crate::components::monitor_modal::plugin_switch_note_id(
                plugin.as_deref().unwrap_or_default(),
            );
            body = body.child(
                div()
                    .id(SharedString::from(id))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(note),
            );
        }
    }
    let list_card =
        |label: String, rows: Vec<(String, Option<String>)>, icon: Option<&'static str>| {
            let mut c = card(theme);
            for (i, (title, sub)) in rows.into_iter().enumerate() {
                c = c.child(
                    card_row(theme, i == 0)
                        .when_some(icon, |this, icon| {
                            this.child(
                                svg()
                                    .path(icon)
                                    .size(px(16.))
                                    .text_color(theme.muted_foreground),
                            )
                        })
                        .child(
                            v_flex()
                                .flex_1()
                                .child(div().text_base().child(title))
                                .when_some(sub, |this, sub| {
                                    this.child(
                                        div()
                                            .text_sm()
                                            .text_color(theme.muted_foreground)
                                            .child(sub),
                                    )
                                }),
                        ),
                );
            }
            v_flex()
                .gap(px(6.))
                .child(section_label(label, theme))
                .child(c)
        };
    if !model.skills.is_empty() {
        body = body.child(list_card(
            format!("Skills  {}", model.skills.len()),
            model.skills.iter().map(|s| (s.clone(), None)).collect(),
            None,
        ));
    }
    if !model.apps.is_empty() {
        body = body.child(list_card(
            format!("Apps  {}", model.apps.len()),
            model
                .apps
                .iter()
                .map(|a| (a.clone(), Some("Connector".to_string())))
                .collect(),
            Some("icons/plugins.svg"),
        ));
    }
    if !model.unsupported.is_empty() {
        body = body.child(list_card(
            "Not run by this server".into(),
            model
                .unsupported
                .iter()
                .map(|(n, why)| (n.clone(), Some(why.clone())))
                .collect(),
            None,
        ));
    }
    if !model.info.is_empty() {
        let mut c = card(theme);
        for (i, (label, value)) in model.info.iter().enumerate() {
            c = c.child(
                card_row(theme, i == 0)
                    .child(div().flex_1().text_base().child(*label))
                    .child(
                        div()
                            .text_base()
                            .text_color(theme.muted_foreground)
                            .child(value.clone()),
                    ),
            );
        }
        body = body.child(section_label("Information", theme)).child(c);
    }
    body.into_any_element()
}

/// An account's Bots: every Bot of the person's, filtered by name, each allowed or not to use the
/// account with one press. A list scales to many Bots where a switch per Bot in the account's row
/// did not.
fn bots_view(
    app: &Entity<AppState>,
    modal: &MonitorModal,
    account: &str,
    inputs: &MarketInputs,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    let state = app.read(cx);
    let label = state
        .connections
        .own_rows()
        .into_iter()
        .find(|r| r.id == account)
        .map(|r| state.connections.shown_label(r))
        .unwrap_or_default();
    let rows = bot_rows(state, account, &modal.bots_query);
    let all = bot_rows(state, account, "");
    let allowed = all.iter().filter(|r| r.on).count();
    let back = app.clone();
    let mut body = v_flex()
        .id("market-bots")
        .debug_selector(|| "market-bots".into())
        .w_full()
        .gap(px(12.))
        .child(
            div()
                .id(BOTS_BACK)
                .debug_selector(|| BOTS_BACK.into())
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    back.update(cx, |state, cx| state.open_account_bots(None, cx))
                })
                .child(
                    Icon::new(IconName::ChevronLeft)
                        .size(px(18.))
                        .text_color(theme.muted_foreground),
                ),
        )
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!("Bots using {label}")),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(format!(
                    "{allowed} of {} Bots can use this account",
                    all.len()
                )),
        );
    if let Some(input) = inputs.bots {
        body = body.child(
            div()
                .id(BOTS_SEARCH)
                .debug_selector(|| BOTS_SEARCH.into())
                .child(
                    field_input(input)
                        .rounded_full()
                        .prefix(
                            Icon::new(IconName::Search)
                                .size(px(16.))
                                .text_color(theme.muted_foreground),
                        )
                        .cleanable(true),
                ),
        );
    }
    let mut list = card(theme);
    for (i, row) in rows.iter().enumerate() {
        let toggle = app.clone();
        let account = account.to_string();
        let bot = row.id.clone();
        let (on, live) = (row.on, row.live);
        let id = bot_row_id(&row.id);
        let sel = id.clone();
        list = list.child(
            card_row(theme, i == 0)
                .id(SharedString::from(id))
                .debug_selector(move || sel)
                .when(live, |this| this.cursor_pointer())
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    if live {
                        toggle.update(cx, |state, cx| {
                            state.set_connection_lent_to(account.clone(), bot.clone(), !on, cx)
                        });
                    }
                })
                .child(tile(&row.name, 32., theme))
                .child(
                    div()
                        .flex_1()
                        .text_base()
                        .truncate()
                        .child(row.name.clone()),
                )
                .child(if on {
                    h_flex()
                        .gap(px(4.))
                        .items_center()
                        .text_sm()
                        .text_color(theme.success)
                        .child(
                            Icon::new(IconName::Check)
                                .size(px(14.))
                                .text_color(theme.success),
                        )
                        .child("Allowed")
                } else {
                    h_flex()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Allow")
                }),
        );
        if let Some(note) = row.note.clone() {
            list = list.child(
                div()
                    .pb(px(8.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(note),
            );
        }
    }
    if rows.is_empty() {
        list = list.child(
            card_row(theme, true).child(div().text_sm().text_color(theme.muted_foreground).child(
                if all.is_empty() {
                    "You have no Bots yet."
                } else {
                    "No Bots match."
                },
            )),
        );
    }
    body.child(list).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{PluginSelection, all_rows};
    use crate::state::ConnectorList;

    /// An installed plugin's token service is listed by `GET /connectors` beside the sign-in
    /// apps; it is the plugin's, so it is not offered a second time as an app of its own.
    #[test]
    fn a_plugins_own_service_is_not_listed_again_as_an_app() {
        let mut state = crate::components::monitor_modal::tests::catalog();
        state.connections.connectors = Some(ConnectorList::Listed(
            serde_json::from_value(serde_json::json!([
                {"name":"gmail", "label":"Gmail"},
                {"name":"cloudflare", "label":"Cloudflare", "plugin":"cloudflare", "authentication":"token"}
            ]))
            .unwrap(),
        ));
        let rows = all_rows(&state);
        assert!(
            rows.iter()
                .any(|r| r.selection == PluginSelection::Service("gmail".into()))
        );
        assert!(
            rows.iter()
                .all(|r| r.selection != PluginSelection::Service("cloudflare".into()))
        );
    }
}
