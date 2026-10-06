//! Tools and Plugins open from the Computer pane (hexuria/nativechat#175, R-A and P1).
//! The lists, switch state and errors are the existing server-backed cards' own. Plugins uses
//! connections, private skills and the open Bot's ceiling; catalog installation awaits
//! opengrok-server#356. Connector metadata is not invented where the server supplies none.
//!
//! A service can have several accounts (hexuria/nativechat#185, the client half of
//! opengrok-server #359): a connection's detail lists every account of its service, each with
//! Rename and Reconnect, adds another, and picks which one the open Bot uses, or none, so it
//! asks each time. Each installed plugin is switched for the open Bot through its ceiling row.

use crate::components::agent_settings::{
    CeilingCardLine, shown_skill_rows, skills_card_lines, skills_summary,
};
use crate::components::switch::Switch;
use crate::components::{agent_settings, connections};
use crate::opengrok::ConnectionKind;
use crate::state::{AppState, ConnectionList, ConnectorList, PinList, SkillsCard, ToolCeiling};
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub(crate) const TOOLS: &str = "computer-tools";
pub(crate) const PLUGINS: &str = "computer-plugins";
pub(crate) const RECIPES: &str = "computer-recipes";
pub(crate) const MODAL: &str = "monitor-modal";
pub(crate) const CLOSE: &str = "monitor-modal-close";
pub(crate) const BACK: &str = "monitor-plugin-back";
pub(crate) const REMOVE: &str = "monitor-plugin-remove";
pub(crate) const REMOVE_YES: &str = "monitor-plugin-remove-confirm";
pub(crate) const REMOVE_NO: &str = "monitor-plugin-remove-cancel";
pub(crate) const ADD_ACCOUNT: &str = "monitor-plugin-add-account";
pub(crate) const DETAIL: &str = "monitor-plugin-detail";
pub(crate) const STATUS: &str = "monitor-plugin-status";
pub(crate) const NOT_SUPPLIED: &str = "Not supplied by this server";
/// A connection's detail lists every account of its service: the list, and under it why the
/// last Add another account did not open the browser.
pub(crate) const ACCOUNTS: &str = "monitor-plugin-accounts";
pub(crate) const ADD_ACCOUNT_ERROR: &str = "monitor-plugin-add-account-error";
/// Which account the open Bot uses for the service: the picker, its Ask each time, and the line
/// under it.
pub(crate) const PICKER: &str = "monitor-account-picker";
pub(crate) const PICK_ASK: &str = "monitor-account-ask";
pub(crate) const PICKER_NOTE: &str = "monitor-account-picker-note";
/// The picker's choice of no account: the Bot has none picked, and asks each time.
pub(crate) const ASK_EACH_TIME: &str = "Ask each time";

/// One account's row in a connection's detail, by its connection id.
pub(crate) fn account_id(id: &str) -> String {
    format!("monitor-account-row-{id}")
}

/// That account's Rename, which opens its label as a field.
pub(crate) fn rename_id(id: &str) -> String {
    format!("monitor-account-rename-{id}")
}

/// The field its label is renamed in, while it is open: Enter saves, Escape puts it back.
pub(crate) fn rename_field_id(id: &str) -> String {
    format!("monitor-account-label-{id}")
}

/// That account's Reconnect, on an account signed in to through a sign-in page.
pub(crate) fn reconnect_id(id: &str) -> String {
    format!("monitor-account-reconnect-{id}")
}

/// Why that account's last rename or Reconnect did not go through.
pub(crate) fn account_note_id(id: &str) -> String {
    format!("monitor-account-note-{id}")
}

/// The picker's choice of that account for the open Bot.
pub(crate) fn pick_id(id: &str) -> String {
    format!("monitor-account-pick-{id}")
}

/// An installed plugin's switch for the open Bot, by the plugin's name in the Bot's ceiling.
pub(crate) fn plugin_switch_id(name: &str) -> String {
    format!("monitor-plugin-switch-{name}")
}

/// Why that switch's last change did not go through.
pub(crate) fn plugin_switch_note_id(name: &str) -> String {
    format!("monitor-plugin-switch-note-{name}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorKind {
    Tools,
    Plugins,
}

impl MonitorKind {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Tools => "Tools",
            Self::Plugins => "Plugins",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginSelection {
    Connection(String),
    Skill(String),
    /// A service this server signs in to (`GET /connectors`), by name, whether or not the person
    /// has an account of it yet.
    Service(String),
    /// A marketplace plugin, by its name in the catalog or in the person's installs.
    Plugin(String),
}

impl PluginSelection {
    pub(crate) fn detail_id(&self) -> String {
        match self {
            Self::Connection(id) => format!("monitor-connection-detail-{id}"),
            Self::Skill(id) => format!("monitor-skill-detail-{id}"),
            Self::Service(name) => format!("market-service-{name}"),
            Self::Plugin(name) => format!("market-plugin-{name}"),
        }
    }
    pub(crate) fn switch_id(&self) -> String {
        match self {
            Self::Connection(id) => connections::lend_id(id),
            Self::Skill(id) => format!("agent-skills-switch-{id}"),
            Self::Service(name) => format!("market-service-switch-{name}"),
            Self::Plugin(name) => plugin_switch_id(name),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MonitorModal {
    pub coworker_id: String,
    pub kind: MonitorKind,
    pub selected: Option<PluginSelection>,
    pub confirming: bool,
    pub removing: bool,
    pub error: Option<String>,
    /// The account whose rename field is open in the detail, by its connection id, and what the
    /// field holds as typed.
    pub renaming: Option<(String, String)>,
    /// Which marketplace page is open under any detail.
    pub page: MarketPage,
    /// What is typed in the marketplace's search.
    pub query: String,
    /// The result Up and Down move and Enter opens, by its place among the rows shown.
    pub highlight: usize,
    /// A plugin detail's Tools section is open.
    pub tools_open: bool,
    /// The account whose Remove was pressed on its row, which the confirmation is about.
    pub removal: Option<String>,
    /// The account whose Bots list is open in place of the detail, and what its filter holds.
    pub bots_for: Option<String>,
    pub bots_query: String,
}

/// The marketplace's pages: everything by category, one category whole, or what is installed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MarketPage {
    #[default]
    Browse,
    Category(String),
    Installed,
}

impl MonitorModal {
    pub(crate) fn new(coworker_id: String, kind: MonitorKind) -> Self {
        Self {
            coworker_id,
            kind,
            selected: None,
            confirming: false,
            removing: false,
            error: None,
            renaming: None,
            page: MarketPage::Browse,
            query: String::new(),
            highlight: 0,
            tools_open: false,
            removal: None,
            bots_for: None,
            bots_query: String::new(),
        }
    }
}

/// The window and the driver use the same rows, including pending changes and refusals.
#[derive(Clone, Debug)]
pub(crate) struct PluginRow {
    pub selection: PluginSelection,
    pub title: String,
    pub subtitle: String,
    pub on: bool,
    pub live: bool,
    pub note: Option<String>,
}

pub(crate) fn private_skill_rows(card: &SkillsCard) -> Vec<agent_settings::ShownSkillRow> {
    shown_skill_rows(card)
        .into_iter()
        .filter(|row| row.mine)
        .collect()
}

pub(crate) fn plugin_rows(state: &AppState) -> Vec<PluginRow> {
    let Some(bot) = state.active_coworker_id.as_deref() else {
        return Vec::new();
    };
    let mut rows: Vec<_> = state
        .connections
        .own_rows()
        .into_iter()
        .map(|row| PluginRow {
            selection: PluginSelection::Connection(row.id.clone()),
            title: state.connections.connector_label(&row.connector),
            subtitle: row.label.clone(),
            on: state.connections.shows_lent(row, bot),
            live: !state.connections.is_changing(&row.id),
            note: state
                .connections
                .lend_refusal(&row.id, bot)
                .map(str::to_string),
        })
        .collect();
    if let Some(card) = state.skills_card() {
        rows.extend(private_skill_rows(&card).into_iter().map(|row| PluginRow {
            selection: PluginSelection::Skill(row.id),
            title: row.title,
            subtitle: if row.switched_off {
                "Switched off in Settings → Skills".into()
            } else {
                row.first_line
            },
            on: row.on,
            live: row.live,
            note: row.note,
        }));
    }
    rows
}

/// An installed plugin's switch for the open Bot: its row in the Bot's ceiling
/// (opengrok-server #268), which the Tools card switches as well. The window and the driver use
/// the same rows, with a switch that is with the server shown where it was asked to go.
#[derive(Clone, Debug)]
pub(crate) struct PluginSwitch {
    pub name: String,
    pub title: String,
    pub subtitle: String,
    pub on: bool,
    pub live: bool,
    pub note: Option<String>,
}

/// Every plugin in the open Bot's ceiling, in the server's order, or none while it is not read.
pub(crate) fn plugin_switches(state: &AppState) -> Vec<PluginSwitch> {
    let Some(card) = state.ceiling_card() else {
        return Vec::new();
    };
    agent_settings::shown_ceiling_rows(&card)
        .into_iter()
        .filter(|row| !row.builtin)
        .map(|row| PluginSwitch {
            subtitle: row
                .unavailable
                .map(str::to_string)
                .or(row.connector)
                .unwrap_or(row.first_line),
            name: row.name,
            title: row.title,
            on: row.on,
            live: row.live,
            note: row.note,
        })
        .collect()
}

pub(crate) fn plugin_exists(state: &AppState, selected: &PluginSelection) -> bool {
    match selected {
        PluginSelection::Service(_) | PluginSelection::Plugin(_) => {
            crate::components::marketplace::row_exists(state, selected)
        }
        _ => plugin_rows(state)
            .iter()
            .any(|row| &row.selection == selected),
    }
}

/// Only facts the current server supplies, with an explicit absence for metadata catalog #356
/// will add. Connections routes #267/#269 expose no source, transport, URL or tool membership.
#[derive(Clone, Debug)]
pub(crate) struct PluginDetail {
    pub title: String,
    pub fields: Vec<(&'static str, String)>,
    pub question: String,
    pub can_remove: bool,
    /// A connection's service: its accounts, Add another account and the open Bot's pick.
    pub accounts: Option<ServiceAccounts>,
    pub error: Option<String>,
}

/// What a connection's detail shows of its service (opengrok-server #359): every account the
/// person has of it, in the server's order, Add another account, and which one the open Bot
/// uses.
#[derive(Clone, Debug)]
pub(crate) struct ServiceAccounts {
    pub connector: String,
    pub rows: Vec<AccountRow>,
    /// Add another account can be pressed: no sign-in page is being asked for.
    pub can_add: bool,
    /// Its sign-in page is being asked for, and the button says so.
    pub adding: bool,
    /// Why the last Connect of this service did not open the browser.
    pub add_refusal: Option<String>,
    pub picker: AccountPicker,
}

/// One account of the service, as the window and the driver show it.
#[derive(Clone, Debug)]
pub(crate) struct AccountRow {
    pub id: String,
    /// What it is called, with a rename that is with the server shown as asked.
    pub label: String,
    /// The account the detail was opened on, which Remove removes.
    pub selected: bool,
    /// What its rename field holds, while the field is open.
    pub renaming: Option<String>,
    /// Rename can be pressed: nothing is with the server for it.
    pub can_rename: bool,
    /// It was signed in to through the service's sign-in page, which Reconnect opens again. An
    /// account made from a key has no page to go back to, and no Reconnect.
    pub reconnects: bool,
    /// Reconnect can be pressed: nothing is with the server for it, and no sign-in page is being
    /// asked for.
    pub can_reconnect: bool,
    /// Its Reconnect is asking for the page.
    pub opening: bool,
    /// Why its last rename or Reconnect did not go through, in the server's words.
    pub note: Option<String>,
}

/// The open Bot's pick of an account for the service: one choice for each account, then
/// [`ASK_EACH_TIME`].
#[derive(Clone, Debug)]
pub(crate) struct AccountPicker {
    /// "Ada uses".
    pub title: String,
    /// What shows as picked, `None` inside for Ask each time: see
    /// [`crate::state::AccountConnections::shown_pick`]. `None` until the pins are read.
    pub picked: Option<Option<String>>,
    /// A choice can be made: the pins are read, and no pick for this Bot and service is with the
    /// server.
    pub live: bool,
    /// Why the last pick did not go through, or why nothing can be picked yet.
    pub note: Option<String>,
}

fn account_picker(state: &AppState, connector: &str) -> AccountPicker {
    let connections = &state.connections;
    let bot = state.active_coworker_id.as_deref().unwrap_or_default();
    let picked = connections.shown_pick(bot, connector);
    let note = match (connections.pick_refusal(bot, connector), &connections.pins) {
        (Some(why), _) => Some(why.to_string()),
        (None, None | Some(PinList::Loading)) => Some(connections::ASKING.to_string()),
        (None, Some(PinList::Unavailable(why))) => Some(why.clone()),
        (None, Some(PinList::Listed(_))) => None,
    };
    AccountPicker {
        title: format!("{} uses", state.active_bot_name()),
        live: picked.is_some() && !connections.is_picking(bot, connector),
        picked,
        note,
    }
}

pub(crate) fn plugin_detail(state: &AppState, selected: &PluginSelection) -> Option<PluginDetail> {
    match selected {
        PluginSelection::Connection(id) => {
            let connections = &state.connections;
            let own = connections.own_rows();
            let row = *own.iter().find(|row| &row.id == id)?;
            let name_of = |id: &str| {
                state
                    .coworkers
                    .iter()
                    .find(|bot| bot.id == id)
                    .map(|bot| bot.name.clone())
            };
            let modal = state.monitor_modal.as_ref();
            let removing = modal.is_some_and(|modal| modal.removing);
            let renaming = modal.and_then(|modal| modal.renaming.as_ref());
            let reconnecting = connections.reconnecting.as_deref();
            let rows = own
                .iter()
                .filter(|account| account.connector == row.connector)
                .map(|account| {
                    let changing = connections.is_changing(&account.id);
                    let reconnects = account.kind == ConnectionKind::Oauth;
                    let reconnect_refusal = connections
                        .reconnect_refused
                        .as_ref()
                        .filter(|(about, _)| *about == account.id)
                        .map(|(_, why)| why.clone());
                    AccountRow {
                        id: account.id.clone(),
                        label: connections.shown_label(account),
                        selected: account.id == *id,
                        renaming: renaming
                            .filter(|(about, _)| *about == account.id)
                            .map(|(_, typed)| typed.clone()),
                        can_rename: !changing && !removing,
                        reconnects,
                        can_reconnect: reconnects
                            && !changing
                            && !removing
                            && connections.opening.is_none(),
                        opening: reconnecting == Some(account.id.as_str()),
                        note: connections
                            .not_renamed
                            .get(&account.id)
                            .cloned()
                            .or(reconnect_refusal),
                    }
                })
                .collect();
            Some(PluginDetail {
                title: connections.connector_label(&row.connector),
                fields: vec![
                    ("Source", row.connector.clone()),
                    ("Transport", NOT_SUPPLIED.into()),
                    ("URL", NOT_SUPPLIED.into()),
                    ("Tools", NOT_SUPPLIED.into()),
                ],
                question: connections::confirm_question(connections, row, name_of),
                can_remove: !connections.is_changing(id),
                accounts: Some(ServiceAccounts {
                    connector: row.connector.clone(),
                    rows,
                    can_add: connections.opening.is_none() && !removing,
                    adding: reconnecting.is_none()
                        && connections.opening.as_deref() == Some(row.connector.as_str()),
                    add_refusal: connections
                        .connect_refused
                        .as_ref()
                        .filter(|(service, _)| *service == row.connector)
                        .map(|(_, why)| why.clone()),
                    picker: account_picker(state, &row.connector),
                }),
                error: connections.disconnect_refusal(id).map(str::to_string),
            })
        }
        PluginSelection::Service(_) | PluginSelection::Plugin(_) => None,
        PluginSelection::Skill(id) => {
            let card = state.skills_card()?;
            let row = private_skill_rows(&card)
                .into_iter()
                .find(|row| &row.id == id)?;
            Some(PluginDetail {
                title: row.title.clone(),
                fields: vec![
                    ("Source", "Your private skill".into()),
                    ("Transport", "Read by the Bot through use_skill".into()),
                    ("URL", "No external URL".into()),
                    ("Tools", "use_skill".into()),
                    ("Accounts", "Your account".into()),
                ],
                question: format!(
                    "Remove {} from your library? Every Bot using this skill will lose it.",
                    row.title
                ),
                can_remove: card.blocked.is_none(),
                accounts: None,
                error: None,
            })
        }
    }
}

pub(crate) fn plugin_status(state: &AppState) -> Vec<String> {
    let mut lines = Vec::new();
    match &state.connections.list {
        None | Some(ConnectionList::Loading) => lines.push(connections::ASKING.into()),
        Some(ConnectionList::Unavailable(why)) => lines.push(why.clone()),
        Some(ConnectionList::Listed(_)) => {}
    }
    if let Some(ConnectorList::Unavailable(why)) = &state.connections.connectors {
        lines.push(why.clone());
    }
    // The plugins' switches are the open Bot's ceiling rows, and the Tools card's lines say why
    // they cannot move when they cannot.
    if let Some(card) = state.ceiling_card() {
        if let ToolCeiling::Unavailable(why) = &card.ceiling {
            lines.push(why.clone());
        }
        for line in agent_settings::ceiling_card_lines(&card) {
            let (CeilingCardLine::ReadOnly(words)
            | CeilingCardLine::Wait(words)
            | CeilingCardLine::Note(words)) = line;
            lines.push(words);
        }
    }
    if let Some(card) = state.skills_card() {
        if matches!(card.skills, crate::state::BotSkills::Read(_)) {
            let rows = private_skill_rows(&card);
            lines.push(format!(
                "{} of {} private skills attached",
                rows.iter().filter(|row| row.on).count(),
                rows.len()
            ));
        } else {
            lines.push(skills_summary(&card.skills, card.pending.as_ref()));
        }
        for line in skills_card_lines(&card) {
            let (agent_settings::SkillsCardLine::ReadOnly(words)
            | agent_settings::SkillsCardLine::Wait(words)
            | agent_settings::SkillsCardLine::Note(words)) = line;
            lines.push(words);
        }
        if card.shared {
            lines.push(
                "This Bot is shared. Attached skills are available to everyone using it.".into(),
            );
        }
    }
    lines
}

pub struct MonitorModalView {
    state: Entity<AppState>,
    focus: FocusHandle,
    was_open: bool,
    pending_focus: bool,
    rename_input: Option<Entity<gpui_kit::component::input::InputState>>,
    rename_account: Option<String>,
    /// The marketplace's search field, made on first draw (it needs the window).
    search_input: Option<Entity<gpui_kit::component::input::InputState>>,
    /// The token field, for the token form it was made for.
    token_input: Option<Entity<gpui_kit::component::input::InputState>>,
    /// The Bots list's filter, while one is open.
    bots_input: Option<Entity<gpui_kit::component::input::InputState>>,
    token_for: Option<(String, String, Option<String>)>,
    /// The marketplace list, and the highlight it last scrolled into view.
    scroll: ScrollHandle,
    scrolled_to: Option<usize>,
}

impl MonitorModalView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let open = state.read(cx).monitor_modal.is_some();
        cx.observe(&state, |this, state, cx| {
            let open = state.read(cx).monitor_modal.is_some();
            if open && !this.was_open {
                this.pending_focus = true;
            }
            this.was_open = open;
            cx.notify();
        })
        .detach();
        Self {
            state,
            focus: cx.focus_handle(),
            was_open: open,
            pending_focus: open,
            rename_input: None,
            rename_account: None,
            search_input: None,
            token_input: None,
            bots_input: None,
            token_for: None,
            scroll: ScrollHandle::new(),
            scrolled_to: None,
        }
    }

    /// The search field, made once, typing into the state; and whatever the state holds (a
    /// driver's `set-value`, or a page change that cleared it) put back into the field.
    fn sync_market_inputs(
        &mut self,
        modal: &MonitorModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use gpui_kit::component::input::{InputEvent, InputState};
        let search = match &self.search_input {
            Some(search) => search.clone(),
            None => {
                let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search plugins"));
                cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let query = input.read(cx).value().to_string();
                        this.state
                            .update(cx, |state, cx| state.set_market_query(query, cx));
                    }
                })
                .detach();
                self.search_input = Some(search.clone());
                search
            }
        };
        if search.read(cx).value().as_str() != modal.query.as_str() {
            let query = modal.query.clone();
            search.update(cx, |input, cx| input.set_value(query, window, cx));
        }
        if modal.bots_for.is_some() {
            let bots = match &self.bots_input {
                Some(bots) => bots.clone(),
                None => {
                    let bots = cx.new(|cx| InputState::new(window, cx).placeholder("Filter Bots"));
                    cx.subscribe(&bots, |this, input, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            let query = input.read(cx).value().to_string();
                            this.state
                                .update(cx, |state, cx| state.set_bots_query(query, cx));
                        }
                    })
                    .detach();
                    self.bots_input = Some(bots.clone());
                    bots
                }
            };
            if bots.read(cx).value().as_str() != modal.bots_query.as_str() {
                let query = modal.bots_query.clone();
                bots.update(cx, |input, cx| input.set_value(query, window, cx));
            }
        } else {
            self.bots_input = None;
        }
        let form = self.state.read(cx).plugin_market.token.clone();
        let key = form.as_ref().map(|form| {
            (
                form.plugin.clone(),
                form.connector.clone(),
                form.replacing.clone(),
            )
        });
        if key != self.token_for {
            self.token_for = key.clone();
            self.token_input = key.map(|_| {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Paste a token")
                        .masked(true)
                });
                cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
                    let typed = input.read(cx).value().to_string();
                    this.state.update(cx, |state, cx| {
                        state.set_market_token(typed, cx);
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            state.save_market_token(cx);
                        }
                    });
                })
                .detach();
                input
            });
        }
    }
}

fn button(
    id: &str,
    text: &str,
    live: bool,
    on_click: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    let selector = id.to_string();
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector)
        .flex_shrink_0()
        .text_xs()
        .px(px(10.))
        .py(px(6.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgb(0x777777).opacity(0.3))
        .when(live, |this| {
            this.cursor_pointer()
                .hover(|style| style.bg(rgb(0x777777).opacity(0.2)))
        })
        .when(!live, |this| this.opacity(0.5))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            if live {
                on_click(cx);
            }
        })
        .child(text.to_string())
}

impl Render for MonitorModalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_focus {
            self.pending_focus = false;
            self.focus.focus(window, cx);
        }
        let Some(modal) = self.state.read(cx).monitor_modal.clone().filter(|modal| {
            self.state.read(cx).active_coworker_id.as_deref() == Some(&modal.coworker_id)
        }) else {
            return div().into_any_element();
        };
        if let Some((id, label)) = &modal.renaming {
            if self.rename_account.as_ref() != Some(id) {
                let label = label.clone();
                let input = cx.new(|cx| {
                    let mut input = gpui_kit::component::input::InputState::new(window, cx);
                    input.set_value(label, window, cx);
                    input
                });
                cx.subscribe(
                    &input,
                    |this, input, event: &gpui_kit::component::input::InputEvent, cx| {
                        let text = input.read(cx).value().to_string();
                        this.state.update(cx, |state, cx| {
                            state.set_account_rename(text, cx);
                            if matches!(
                                event,
                                gpui_kit::component::input::InputEvent::PressEnter { .. }
                            ) {
                                state.save_account_rename(cx);
                            }
                        });
                    },
                )
                .detach();
                self.rename_input = Some(input);
                self.rename_account = Some(id.clone());
            } else if let Some(input) = &self.rename_input
                && input.read(cx).value().as_str() != label.as_str()
            {
                let label = label.clone();
                input.update(cx, |input, cx| input.set_value(label, window, cx));
            }
        } else {
            self.rename_input = None;
            self.rename_account = None;
        }
        let theme = cx.theme().clone();
        let app = self.state.clone();
        if modal.kind == MonitorKind::Plugins
            && !matches!(
                modal.selected,
                Some(PluginSelection::Connection(_) | PluginSelection::Skill(_))
            )
        {
            self.sync_market_inputs(&modal, window, cx);
            return self.render_market(&modal, window, cx);
        }
        let body = match modal.kind {
            MonitorKind::Tools => agent_settings::tools_card(app.clone(), &theme, cx)
                .unwrap_or_else(|| div().child(connections::ASKING).into_any_element()),
            MonitorKind::Plugins => {
                plugins_body(app.clone(), &modal, &theme, self.rename_input.as_ref(), cx)
            }
        };
        div()
            .id("monitor-modal-overlay")
            .track_focus(&self.focus)
            .key_context("MonitorModal")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.32))
            .on_mouse_down(MouseButton::Left, {
                let app = app.clone();
                move |_, _, cx| app.update(cx, |state, cx| state.close_monitor_modal(cx))
            })
            .on_action({
                let app = app.clone();
                move |_: &crate::actions::CloseMonitorModal, _: &mut Window, cx: &mut App| {
                    app.update(cx, |state, cx| {
                        if state
                            .monitor_modal
                            .as_ref()
                            .is_some_and(|modal| modal.renaming.is_some())
                        {
                            state.cancel_account_rename(cx);
                        } else {
                            state.close_monitor_modal(cx);
                        }
                    })
                }
            })
            .child(
                v_flex()
                    .id(MODAL)
                    .debug_selector(|| MODAL.into())
                    .w(px(480.))
                    .max_h(window.viewport_size().height - px(60.))
                    .bg(theme.popover)
                    .text_color(theme.foreground)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(14.))
                    .shadow_lg()
                    .px(px(20.))
                    .py(px(18.))
                    .gap(px(12.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                                format!(
                                    "{} · {}",
                                    modal.kind.title(),
                                    app.read(cx).active_bot_name()
                                ),
                            ))
                            .child(
                                div()
                                    .id(CLOSE)
                                    .debug_selector(|| CLOSE.into())
                                    .size(px(28.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .on_mouse_down(MouseButton::Left, {
                                        let app = app.clone();
                                        move |_, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.close_monitor_modal(cx)
                                            })
                                        }
                                    })
                                    .child(
                                        Icon::new(IconName::Close)
                                            .size(px(14.))
                                            .text_color(theme.muted_foreground),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("monitor-modal-scroll")
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .child(body),
                    ),
            )
            .into_any_element()
    }
}

impl MonitorModalView {
    /// The marketplace, in the Grok Bot layout: a wide card over the window, two columns when it
    /// is wide enough and one when it is not, the highlighted result kept in view.
    fn render_market(
        &mut self,
        modal: &MonitorModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::components::marketplace;
        let theme = cx.theme().clone();
        let app = self.state.clone();
        let viewport = window.viewport_size();
        let width = (viewport.width - px(80.)).min(px(880.)).max(px(320.));
        let columns = if width >= px(640.) { 2 } else { 1 };
        let search = self
            .search_input
            .clone()
            .expect("made by sync_market_inputs");
        let inputs = marketplace::MarketInputs {
            search: &search,
            bots: self.bots_input.as_ref(),
            token: self.token_input.as_ref(),
            rename: self.rename_input.as_ref(),
        };
        let (top, blocks) = match &modal.selected {
            Some(selection) => (
                None,
                vec![marketplace::detail_view(
                    &app, modal, selection, &inputs, &theme, cx,
                )],
            ),
            None => {
                let (top, laid) = marketplace::browse(&app, modal, &inputs, columns, &theme, cx);
                if let Some(&block) = laid.row_blocks.get(modal.highlight)
                    && self.scrolled_to != Some(modal.highlight)
                {
                    self.scroll.scroll_to_item(block);
                    self.scrolled_to = Some(modal.highlight);
                }
                (Some(top), laid.blocks)
            }
        };
        if modal.selected.is_some() {
            self.scrolled_to = None;
        }
        div()
            .id("monitor-modal-overlay")
            .track_focus(&self.focus)
            .key_context("MonitorModal")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.45))
            .on_mouse_down(MouseButton::Left, {
                let app = app.clone();
                move |_, _, cx| app.update(cx, |state, cx| state.close_monitor_modal(cx))
            })
            .on_action({
                let app = app.clone();
                move |_: &crate::actions::CloseMonitorModal, _: &mut Window, cx: &mut App| {
                    app.update(cx, |state, cx| {
                        let modal = state.monitor_modal.as_ref();
                        if modal.is_some_and(|modal| modal.renaming.is_some()) {
                            state.cancel_account_rename(cx);
                        } else if state.plugin_market.token.is_some() {
                            state.cancel_market_token(cx);
                        } else if modal.is_some_and(|modal| modal.selected.is_some()) {
                            state.close_market_detail(cx);
                        } else {
                            state.close_monitor_modal(cx);
                        }
                    })
                }
            })
            .child(
                v_flex()
                    .id(MODAL)
                    .debug_selector(|| MODAL.into())
                    .relative()
                    .w(width)
                    .h(viewport.height - px(80.))
                    .max_h(px(900.))
                    .bg(theme.popover)
                    .text_color(theme.foreground)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(18.))
                    .shadow_lg()
                    .pt(px(28.))
                    .pb(px(12.))
                    .gap(px(16.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id(CLOSE)
                            .debug_selector(|| CLOSE.into())
                            .absolute()
                            .top(px(14.))
                            .right(px(14.))
                            .size(px(28.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, {
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| state.close_monitor_modal(cx))
                                }
                            })
                            .child(
                                Icon::new(IconName::Close)
                                    .size(px(16.))
                                    .text_color(theme.muted_foreground),
                            ),
                    )
                    .when_some(top, |this, top| this.child(div().px(px(40.)).child(top)))
                    .child(
                        div()
                            .id("market-scroll")
                            .flex_1()
                            .min_h(px(0.))
                            .px(px(28.))
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(blocks),
                    ),
            )
            .into_any_element()
    }
}

fn plugins_body(
    app: Entity<AppState>,
    modal: &MonitorModal,
    theme: &gpui_kit::component::Theme,
    rename_input: Option<&Entity<gpui_kit::component::input::InputState>>,
    cx: &App,
) -> AnyElement {
    let state = app.read(cx);
    let muted = theme.muted_foreground;
    if let Some(selected) = &modal.selected {
        let detail = plugin_detail(state, selected);
        let back_app = app.clone();
        let mut body = v_flex()
            .id(DETAIL)
            .debug_selector(|| DETAIL.into())
            .gap(px(12.))
            .child(button(BACK, "← Installed", !modal.removing, move |cx| {
                back_app.update(cx, |state, cx| state.select_monitor_plugin(None, cx))
            }));
        let Some(detail) = detail else {
            return body
                .child("This plugin is no longer installed.")
                .into_any_element();
        };
        body = body.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(detail.title),
        );
        for (label, value) in detail.fields {
            let selector = format!("monitor-plugin-{}", label.to_lowercase());
            body = body.child(
                v_flex()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector)
                    .gap(px(2.))
                    .child(div().text_xs().text_color(muted).child(label))
                    .child(div().text_sm().child(value)),
            );
        }
        if let Some(accounts) = detail.accounts {
            let add = app.clone();
            let connector = accounts.connector.clone();
            body = body.child(button(
                ADD_ACCOUNT,
                "Add another account",
                accounts.can_add,
                move |cx| {
                    add.update(cx, |state, cx| state.connect_service(connector.clone(), cx));
                },
            ));
            body = body.child(div().id(PICKER).child(accounts.picker.title.clone()));
            if let Some(note) = accounts.add_refusal {
                body = body.child(div().id(ADD_ACCOUNT_ERROR).child(note));
            }
            if let Some(note) = accounts.picker.note.clone() {
                body = body.child(div().id(PICKER_NOTE).child(note));
            }
            body = body.child(div().id(ACCOUNTS).debug_selector(|| ACCOUNTS.into()).child(
                if accounts.adding {
                    "Opening sign-in…"
                } else {
                    "Accounts"
                },
            ));
            for row in accounts.rows {
                let rename = app.clone();
                let rename_account = row.id.clone();
                body = body.child(button(
                    &rename_id(&row.id),
                    "Rename",
                    row.can_rename,
                    move |cx| {
                        rename.update(cx, |state, cx| {
                            state.start_account_rename(rename_account.clone(), cx)
                        });
                    },
                ));
                if row.renaming.is_some()
                    && let Some(input) = rename_input
                {
                    body = body.child(
                        div()
                            .id(SharedString::from(rename_field_id(&row.id)))
                            .child(gpui_kit::component::input::Input::new(input)),
                    );
                    let save = app.clone();
                    let cancel = app.clone();
                    body = body
                        .child(button("monitor-account-save", "Save", true, move |cx| {
                            save.update(cx, |state, cx| state.save_account_rename(cx))
                        }))
                        .child(button(
                            "monitor-account-cancel",
                            "Cancel",
                            true,
                            move |cx| {
                                cancel.update(cx, |state, cx| state.cancel_account_rename(cx))
                            },
                        ));
                }
                if let Some(note) = row.note.clone() {
                    body = body.child(
                        div()
                            .id(SharedString::from(account_note_id(&row.id)))
                            .child(note),
                    );
                }
                let reconnect = app.clone();
                let id = row.id.clone();
                let choose = app.clone();
                let picked_id = row.id.clone();
                let service = accounts.connector.clone();
                body = body.child(
                    v_flex()
                        .id(SharedString::from(account_id(&row.id)))
                        .gap(px(4.))
                        .child(div().text_sm().child(format!(
                            "{}{}{}",
                            row.label,
                            if row.selected { " (open account)" } else { "" },
                            if row.opening { " — Opening…" } else { "" }
                        )))
                        .child(button(
                            &pick_id(&row.id),
                            if accounts.picker.picked.as_ref() == Some(&Some(row.id.clone())) {
                                "Selected for this Bot"
                            } else {
                                "Use for this Bot"
                            },
                            accounts.picker.live,
                            move |cx| {
                                choose.update(cx, |state, cx| {
                                    state.pick_account(service.clone(), Some(picked_id.clone()), cx)
                                });
                            },
                        ))
                        .when(row.reconnects, |this| {
                            this.child(button(
                                &reconnect_id(&row.id),
                                "Reconnect",
                                row.can_reconnect,
                                move |cx| {
                                    reconnect.update(cx, |state, cx| {
                                        state.reconnect_connection(id.clone(), cx)
                                    });
                                },
                            ))
                        }),
                );
            }
            let ask = app.clone();
            let connector = accounts.connector;
            body = body.child(button(
                PICK_ASK,
                ASK_EACH_TIME,
                accounts.picker.live,
                move |cx| {
                    ask.update(cx, |state, cx| {
                        state.pick_account(connector.clone(), None, cx)
                    });
                },
            ));
        }
        if let Some(error) = modal.error.as_ref().or(detail.error.as_ref()) {
            body = body.child(
                div()
                    .id("monitor-plugin-error")
                    .debug_selector(|| "monitor-plugin-error".into())
                    .text_xs()
                    .text_color(theme.danger)
                    .child(error.clone()),
            );
        }
        if modal.confirming {
            let yes = app.clone();
            let no = app.clone();
            body = body
                .child(
                    div()
                        .id("monitor-plugin-remove-question")
                        .debug_selector(|| "monitor-plugin-remove-question".into())
                        .text_sm()
                        .child(detail.question),
                )
                .child(
                    h_flex()
                        .gap(px(8.))
                        .child(button(REMOVE_NO, "Cancel", !modal.removing, move |cx| {
                            no.update(cx, |state, cx| state.ask_monitor_remove(false, cx))
                        }))
                        .child(button(
                            REMOVE_YES,
                            if modal.removing {
                                "Removing…"
                            } else {
                                "Remove"
                            },
                            detail.can_remove && !modal.removing,
                            move |cx| yes.update(cx, |state, cx| state.remove_monitor_plugin(cx)),
                        )),
                );
        } else {
            let remove = app.clone();
            body = body.child(button(
                REMOVE,
                "Remove",
                detail.can_remove && !modal.removing,
                move |cx| remove.update(cx, |state, cx| state.ask_monitor_remove(true, cx)),
            ));
        }
        return body.into_any_element();
    }
    let rows = plugin_rows(state);
    let mut body = v_flex()
        .gap(px(12.))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Installed"),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format!("Choose what {} can use.", state.active_bot_name())),
        )
        .child(
            div()
                .id(STATUS)
                .debug_selector(|| STATUS.into())
                .text_xs()
                .text_color(muted)
                .child(plugin_status(state).join("\n")),
        );
    for plugin in plugin_switches(state) {
        let switch = app.clone();
        let name = plugin.name.clone();
        let on = plugin.on;
        body = body
            .child(div().child(format!("{} — {}", plugin.title, plugin.subtitle)))
            .child(button(
                &plugin_switch_id(&plugin.name),
                if on { "Switch off" } else { "Switch on" },
                plugin.live,
                move |cx| {
                    switch.update(cx, |state, cx| {
                        state.switch_ceiling_tool(name.clone(), !on, cx)
                    });
                },
            ));
        if let Some(note) = plugin.note {
            body = body.child(
                div()
                    .id(SharedString::from(plugin_switch_note_id(&plugin.name)))
                    .child(note),
            );
        }
    }
    for row in rows.iter() {
        let selection = row.selection.clone();
        let open = app.clone();
        let switch = app.clone();
        let target = row.selection.clone();
        let detail_id = row.selection.detail_id();
        let mut entry = v_flex().gap(px(4.)).child(
            h_flex()
                .gap(px(10.))
                .items_center()
                .child(
                    v_flex()
                        .id(SharedString::from(detail_id.clone()))
                        .debug_selector(move || detail_id)
                        .flex_1()
                        .min_w(px(0.))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            open.update(cx, |state, cx| {
                                state.select_monitor_plugin(Some(selection.clone()), cx)
                            });
                        })
                        .child(div().text_sm().child(row.title.clone()))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(row.subtitle.clone()),
                        ),
                )
                .child(
                    Switch::new(ElementId::Name(row.selection.switch_id().into()))
                        .checked(row.on)
                        .small()
                        .disabled(!row.live)
                        .accessibility_label(format!(
                            "Allow {} for {}",
                            row.title,
                            state.active_bot_name()
                        ))
                        .on_click(move |on, _, cx| {
                            switch.update(cx, |state, cx| match &target {
                                PluginSelection::Connection(id) => {
                                    state.set_connection_lent(id.clone(), *on, cx)
                                }
                                PluginSelection::Skill(id) => {
                                    state.switch_bot_skill(id.clone(), *on, cx)
                                }
                                PluginSelection::Service(_) | PluginSelection::Plugin(_) => {}
                            })
                        }),
                ),
        );
        if let Some(note) = &row.note {
            entry = entry.child(div().text_xs().text_color(theme.danger).child(note.clone()));
        }
        body = body.child(entry);
    }
    if rows.is_empty() {
        body = body.child(
            div()
                .text_sm()
                .text_color(muted)
                .child("No connections or private skills installed."),
        );
    }
    body.child(
        div()
            .text_xs()
            .text_color(muted)
            .child(connections::LEND_NOTE),
    )
    .into_any_element()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{
        MonitorKind, MonitorModal, MonitorModalView, PluginSelection, plugin_detail, plugin_rows,
    };
    use crate::opengrok::{ConnectionOwner, ConnectionView, CoworkerCeiling, CoworkerSkills};
    use crate::state::{
        AppState, BotSkills, ConnectionChange, ConnectionList, ConnectorList, ToolCeiling,
    };
    use gpui_kit::{
        AppContext as _, Entity, KeyBinding, Modifiers, MouseButton, VisualTestContext, point, px,
        size,
    };

    /// A person's connection and private skill, plus rows they do not own. The same catalog
    /// feeds the window and the driver so neither can lend another person's sign-in.
    pub(crate) fn catalog() -> AppState {
        let mut state = AppState::new();
        state.auth_status = crate::state::AuthStatus::SignedIn;
        state.account = Some(
            serde_json::from_value(serde_json::json!({"id":"acct_1", "email":"you@example.com"}))
                .unwrap(),
        );
        state.active_coworker_id = Some("cw_1".into());
        state.coworkers = serde_json::from_value(serde_json::json!([
            {"id":"cw_1", "name":"Ada"}, {"id":"cw_2", "name":"Bo"}
        ]))
        .unwrap();
        state.connections.list = Some(ConnectionList::Listed(vec![
            ConnectionView {
                id: "conn_1".into(),
                connector: "gmail".into(),
                label: "you@example.com".into(),
                owner: ConnectionOwner::User("acct_1".into()),
                loans: vec!["cw_1".into(), "cw_2".into()],
                updated_at_ms: 1,
                expires_at_ms: None,
                kind: crate::opengrok::ConnectionKind::Oauth,
            },
            ConnectionView {
                id: "conn_foreign".into(),
                connector: "github".into(),
                label: "other".into(),
                owner: ConnectionOwner::Bot("cw_1".into()),
                loans: vec![],
                updated_at_ms: 1,
                expires_at_ms: None,
                kind: crate::opengrok::ConnectionKind::Oauth,
            },
        ]));
        state.connections.connectors = Some(ConnectorList::Listed(
            serde_json::from_value(serde_json::json!([{ "name":"gmail", "label":"Gmail" }]))
                .unwrap(),
        ));
        let skills: CoworkerSkills = serde_json::from_value(serde_json::json!({"skills": [
            {"id":"sk_1", "name":"draft", "description":"Write a draft", "scope":"mine", "attached":false, "enabled":true},
            {"id":"sk_org", "name":"organization", "scope":"org", "attached":true, "enabled":true}
        ], "version":1})).unwrap();
        state.coworker_skills = Some(("cw_1".into(), BotSkills::Read(skills.into())));
        let ceiling: CoworkerCeiling = serde_json::from_value(serde_json::json!({"tools":[
            {"name":"shell", "kind":"builtin", "enabled":true, "description":"Run commands"}
        ], "version":1}))
        .unwrap();
        state.coworker_ceiling = Some(("cw_1".into(), ToolCeiling::Read(ceiling.into())));
        state
    }

    #[test]
    fn plugins_show_only_owned_connections_and_private_skills_and_pending_switches() {
        let mut state = catalog();
        let rows = plugin_rows(&state);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "Gmail");
        assert!(rows[0].on && rows[0].live);
        assert!(!rows[1].on);
        state
            .connections
            .changing
            .insert("conn_1".into(), ConnectionChange::Revoke("cw_1".into()));
        let rows = plugin_rows(&state);
        assert!(!rows[0].on && !rows[0].live);
        assert!(
            plugin_detail(&state, &PluginSelection::Connection("conn_foreign".into())).is_none()
        );
        assert!(plugin_detail(&state, &PluginSelection::Skill("sk_org".into())).is_none());
    }

    #[test]
    fn the_plugin_detail_reports_missing_metadata_and_names_every_bot_removal_affects() {
        let state = catalog();
        let detail = plugin_detail(&state, &PluginSelection::Connection("conn_1".into())).unwrap();
        assert!(detail.question.contains("Ada and Bo will lose it"));
        for label in ["Transport", "URL", "Tools"] {
            assert_eq!(
                detail
                    .fields
                    .iter()
                    .find(|(key, _)| key == &label)
                    .unwrap()
                    .1,
                super::NOT_SUPPLIED
            );
        }
        let detail = plugin_detail(&state, &PluginSelection::Skill("sk_1".into())).unwrap();
        assert!(detail.question.contains("Every Bot"));
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    fn click(cx: &mut VisualTestContext, id: &'static str) {
        draw(cx);
        let at = cx
            .debug_bounds(id)
            .unwrap_or_else(|| panic!("{id} is drawn"))
            .center();
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        draw(cx);
    }

    fn open(
        cx: &mut gpui_kit::TestAppContext,
        kind: MonitorKind,
    ) -> (Entity<AppState>, &mut VisualTestContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.bind_keys([KeyBinding::new(
                "escape",
                crate::actions::CloseMonitorModal,
                Some("MonitorModal"),
            )]);
        });
        let state = cx.new(|_| {
            let mut state = catalog();
            state.monitor_modal = Some(MonitorModal::new("cw_1".into(), kind));
            state.agent_tools_open = true;
            state
        });
        let (_, cx) = cx.add_window_view({
            let state = state.clone();
            move |_, cx| MonitorModalView::new(state, cx)
        });
        cx.simulate_resize(size(px(1000.), px(800.)));
        draw(cx);
        (state, cx)
    }

    #[gpui_kit::test]
    fn plugins_open_a_services_accounts_and_confirm_removal(cx: &mut gpui_kit::TestAppContext) {
        let (state, cx) = open(cx, MonitorKind::Plugins);
        // Plugins is the marketplace: its search, its installed count and the service's row.
        for id in ["market-search", "market-installed", "market-service-gmail"] {
            assert!(cx.debug_bounds(id).is_some(), "{id}");
        }
        click(cx, "market-service-gmail");
        assert!(
            cx.debug_bounds(crate::components::marketplace::DETAIL)
                .is_some()
        );
        for id in [
            "monitor-account-row-conn_1",
            "monitor-account-rename-conn_1",
            "monitor-account-reconnect-conn_1",
            "market-add-account-gmail",
            "monitor-account-picker",
        ] {
            assert!(cx.debug_bounds(id).is_some(), "{id}");
        }
        assert!(
            cx.debug_bounds("monitor-account-row-conn_foreign")
                .is_none(),
            "a Bot's own sign-in is not listed as the person's"
        );
        click(cx, "market-account-remove-conn_1");
        assert!(state.read_with(cx, |state, _| {
            let modal = state.monitor_modal.as_ref().unwrap();
            modal.confirming && modal.removal.as_deref() == Some("conn_1")
        }));
        assert!(
            state.read_with(cx, |state, _| state.connections.changing.is_empty()),
            "the question sends nothing"
        );
        click(cx, crate::components::marketplace::REMOVE_NO);
        assert!(!state.read_with(cx, |state, _| {
            state.monitor_modal.as_ref().unwrap().confirming
        }));
        click(cx, super::BACK);
        assert!(state.read_with(cx, |state, _| {
            state.monitor_modal.as_ref().unwrap().selected.is_none()
        }));
        cx.simulate_keystrokes("escape");
        assert!(state.read_with(cx, |state, _| state.monitor_modal.is_none()));
    }

    #[gpui_kit::test]
    fn tools_show_the_ceiling_and_close_with_escape_the_button_and_the_backdrop(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (state, cx) = open(cx, MonitorKind::Tools);
        assert!(cx.debug_bounds("agent-tools").is_some());
        click(cx, super::CLOSE);
        assert!(state.read_with(cx, |state, _| state.monitor_modal.is_none()));
        state.update(cx, |state, cx| {
            state.open_monitor_modal(MonitorKind::Tools, cx)
        });
        draw(cx);
        cx.simulate_mouse_down(point(px(8.), px(8.)), MouseButton::Left, Modifiers::none());
        assert!(state.read_with(cx, |state, _| state.monitor_modal.is_none()));
        state.update(cx, |state, cx| {
            state.open_monitor_modal(MonitorKind::Tools, cx)
        });
        draw(cx);
        cx.simulate_keystrokes("escape");
        assert!(state.read_with(cx, |state, _| state.monitor_modal.is_none()));
    }
}
