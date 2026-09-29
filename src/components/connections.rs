//! Connections (#2): a person signs in to a service once, on Settings → Connections, and lends
//! it to a Bot from that Bot's settings.
//!
//! Both surfaces draw the one list the account has ([`AccountConnections`]), and the words and
//! element ids they draw it with live here, so the gpui-agent tree (`agent/host.rs`) says what
//! the window says and names what the window names.
//!
//! Nothing here holds a connection. The server keeps it, a sign-in happens in the person's
//! browser and comes back to the server, and the app only ever reads the list again.

use crate::opengrok::{ConnectionView, Connector};
use crate::state::{AccountConnections, AppState, ConnectionChange, ConnectionList, ConnectorList};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// Settings → Connections, in the settings' own list of pages.
pub(crate) const SETTINGS_TAB: &str = "settings-tab-connections";
/// Read both lists again: the way back from a sign-in the window did not see come back.
pub(crate) const REFRESH: &str = "settings-connections-refresh";
/// The connected services, while there are any.
pub(crate) const LIST: &str = "settings-connections";
pub(crate) const LIST_EMPTY: &str = "settings-connections-empty";
pub(crate) const LIST_ERROR: &str = "settings-connections-error";
/// The services on offer that are not connected, while there are any.
pub(crate) const OFFERED: &str = "settings-connectors";
pub(crate) const OFFERED_EMPTY: &str = "settings-connectors-empty";
pub(crate) const OFFERED_ERROR: &str = "settings-connectors-error";
/// A Bot's Connections card, and what it says about lending.
pub(crate) const AGENT_CARD: &str = "agent-connections";
pub(crate) const AGENT_NOTE: &str = "agent-connections-note";

/// One connected service's row on Settings → Connections.
pub(crate) fn row_id(connection_id: &str) -> String {
    format!("settings-connection-{connection_id}")
}

/// That row's Disconnect.
pub(crate) fn disconnect_id(connection_id: &str) -> String {
    format!("settings-connection-disconnect-{connection_id}")
}

/// Why that row's last Disconnect did not go through.
/// The row's "Disconnect …?" line, and its Yes and No.
pub(crate) fn confirm_question_id(connection_id: &str) -> String {
    format!("settings-connection-ask-{connection_id}")
}

pub(crate) fn confirm_yes_id(connection_id: &str) -> String {
    format!("settings-connection-confirm-{connection_id}")
}

pub(crate) fn confirm_no_id(connection_id: &str) -> String {
    format!("settings-connection-keep-{connection_id}")
}

/// What the row asks before a Disconnect: the service, and the Bots that would lose it, by name.
pub(crate) fn confirm_question(
    connections: &AccountConnections,
    row: &ConnectionView,
    name_of: impl Fn(&str) -> Option<String>,
) -> String {
    let service = connections.connector_label(&row.connector);
    if row.loans.is_empty() {
        return format!("Disconnect {service}? No Bot is using it.");
    }
    let lent = lent_line(&row.loans, name_of);
    let who = lent.strip_prefix("Lent to ").unwrap_or(&lent);
    format!("Disconnect {service}? {who} will lose it.")
}

pub(crate) fn row_error_id(connection_id: &str) -> String {
    format!("settings-connection-error-{connection_id}")
}

/// Connect, for a service on offer, by the name the server lists it under.
pub(crate) fn connect_id(connector: &str) -> String {
    format!("settings-connect-{connector}")
}

/// Why Connect did not open the browser for that service.
pub(crate) fn connect_error_id(connector: &str) -> String {
    format!("settings-connect-error-{connector}")
}

/// A connection's switch on a Bot's Connections card: lent to this Bot, or not.
pub(crate) fn lend_id(connection_id: &str) -> String {
    format!("agent-connection-lend-{connection_id}")
}

/// Why that switch's last lend or revoke did not go through, on the card of the Bot it was about.
pub(crate) fn lend_error_id(connection_id: &str) -> String {
    format!("agent-connection-error-{connection_id}")
}

pub(crate) const ASKING: &str = "Asking the server…";
pub(crate) const INTRO: &str = "Sign in to a service once, here, then lend it to a Bot from that \
     Bot's settings. Signing in happens in your browser, and the service's keys stay on the \
     server: this app never holds them.";
pub(crate) const NOTHING_CONNECTED: &str = "Nothing connected yet.";
pub(crate) const NO_CONNECTORS: &str = "This server offers no services to connect.";
pub(crate) const ALL_CONNECTED: &str = "Every service this server offers is connected.";
pub(crate) const NOTHING_TO_LEND: &str =
    "Nothing connected yet. Connect a service in Settings → Connections.";
/// What a Bot's card says about what lending does, and what it does not do yet. A connection
/// reaches a Bot's turn through a plugin, and only a plugin the Bot is allowed; nothing can allow
/// one yet (opengrok-server#268). Saying less would be a switch that looks like it changes the
/// next turn and does not.
pub(crate) const LEND_NOTE: &str = "A lent connection reaches a plugin only once this Bot is \
     allowed that plugin. Allowing plugins is not on the server yet, so lending alone does not \
     change this Bot's next turn.";

/// While the person is signing in to a service in their browser.
pub(crate) fn waiting_line(label: &str) -> String {
    format!("Finish signing in to {label} in your browser, then come back here.")
}

/// Who a connection is lent to, by Bot name: "Lent to Ada and Bo", "Not lent to any Bot". A Bot
/// that is not on the person's list (another person's, or one since deleted) is counted rather
/// than named by an id nobody reads.
pub(crate) fn lent_line(loans: &[String], name_of: impl Fn(&str) -> Option<String>) -> String {
    if loans.is_empty() {
        return "Not lent to any Bot".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut strangers = 0;
    for coworker in loans {
        match name_of(coworker).filter(|name| !name.trim().is_empty()) {
            Some(name) => parts.push(name),
            None => strangers += 1,
        }
    }
    match strangers {
        0 => {}
        1 => parts.push("1 Bot not on your list".to_string()),
        n => parts.push(format!("{n} Bots not on your list")),
    }
    let last = parts.pop().unwrap_or_default();
    if parts.is_empty() {
        format!("Lent to {last}")
    } else {
        format!("Lent to {} and {last}", parts.join(", "))
    }
}

/// A Bot's Connections card's second line: how many of the person's own connections are lent
/// to it, or why there is nothing to count.
pub(crate) fn agent_summary(connections: &AccountConnections, coworker_id: &str) -> String {
    match &connections.list {
        None | Some(ConnectionList::Loading) => ASKING.to_string(),
        Some(ConnectionList::Unavailable(why)) => why.clone(),
        Some(ConnectionList::Listed(_)) => {
            let own = connections.own_rows();
            if own.is_empty() {
                return NOTHING_TO_LEND.to_string();
            }
            let lent = own
                .iter()
                .filter(|row| connections.shows_lent(row, coworker_id))
                .count();
            format!("{lent} of {} lent to this Bot", own.len())
        }
    }
}

/// What Settings → Connections offers to connect.
#[derive(Debug, PartialEq)]
pub(crate) enum ConnectOffer<'a> {
    /// A list is still on its way.
    Asking,
    /// The services on offer could not be read, and why.
    Unavailable(&'a str),
    /// A line instead of buttons: [`NO_CONNECTORS`] or [`ALL_CONNECTED`].
    Nothing(&'static str),
    /// The services on offer that are not connected.
    Offered(Vec<&'a Connector>),
    /// The person's connections could not be read, so nothing can be offered: what is already
    /// connected is not known. The page says why above.
    Unknown,
}

pub(crate) fn connect_offer(connections: &AccountConnections) -> ConnectOffer<'_> {
    match (&connections.connectors, &connections.list) {
        (None | Some(ConnectorList::Loading), _) => ConnectOffer::Asking,
        (Some(ConnectorList::Unavailable(why)), _) => ConnectOffer::Unavailable(why),
        (Some(ConnectorList::Listed(all)), _) if all.is_empty() => {
            ConnectOffer::Nothing(NO_CONNECTORS)
        }
        (Some(ConnectorList::Listed(_)), Some(ConnectionList::Listed(_))) => {
            let open = connections.connectable();
            if open.is_empty() {
                ConnectOffer::Nothing(ALL_CONNECTED)
            } else {
                ConnectOffer::Offered(open)
            }
        }
        (Some(ConnectorList::Listed(_)), Some(ConnectionList::Unavailable(_))) => {
            ConnectOffer::Unknown
        }
        (Some(ConnectorList::Listed(_)), None | Some(ConnectionList::Loading)) => {
            ConnectOffer::Asking
        }
    }
}

/// What a service's Connect says: "Connect Gmail", or that it is asking for the sign-in page.
pub(crate) fn connect_label(connections: &AccountConnections, connector: &Connector) -> String {
    if connections.opening.as_deref() == Some(connector.name.as_str()) {
        "Opening…".to_string()
    } else {
        format!("Connect {}", connections.connector_label(&connector.name))
    }
}

/// What a connected service's Disconnect says, and says while the server has it.
pub(crate) fn disconnect_label(
    connections: &AccountConnections,
    connection_id: &str,
) -> &'static str {
    match connections.changing.get(connection_id) {
        Some(ConnectionChange::Disconnect) => "Disconnecting…",
        _ => "Disconnect",
    }
}

/// The line under a connected service's name: the service, and who it is lent to, with a lend
/// or a revoke that is with the server shown as asked, the way the Bot's switch shows it.
pub(crate) fn row_detail(
    connections: &AccountConnections,
    row: &ConnectionView,
    name_of: impl Fn(&str) -> Option<String>,
) -> String {
    format!(
        "{} · {}",
        connections.connector_label(&row.connector),
        lent_line(&connections.shown_loans(row), name_of)
    )
}

fn card() -> Div {
    div()
        .w_full()
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(0x777777).opacity(0.24))
        .overflow_hidden()
}

fn divider() -> Div {
    div().h(px(1.)).bg(rgb(0x777777).opacity(0.16))
}

/// Settings → Connections: what the person has connected, with who each is lent to and a
/// Disconnect, and a Connect for each service this server offers that is not connected yet.
pub(crate) fn connections_page(app: Entity<AppState>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let danger = theme.danger;
    let state = app.read(cx);
    let connections = &state.connections;
    let name_of = |id: &str| {
        state
            .coworkers
            .iter()
            .find(|coworker| coworker.id == id)
            .map(|coworker| coworker.name.clone())
    };

    // Only the person's own: nothing of another scope is theirs to disconnect.
    let own = connections.own_rows();
    let connected: AnyElement = match &connections.list {
        None | Some(ConnectionList::Loading) => div()
            .text_sm()
            .text_color(muted)
            .child(ASKING)
            .into_any_element(),
        Some(ConnectionList::Unavailable(why)) => div()
            .id(LIST_ERROR)
            .text_sm()
            .text_color(danger)
            .child(why.clone())
            .into_any_element(),
        Some(ConnectionList::Listed(_)) if own.is_empty() => div()
            .id(LIST_EMPTY)
            .text_sm()
            .text_color(muted)
            .child(NOTHING_CONNECTED)
            .into_any_element(),
        Some(ConnectionList::Listed(_)) => {
            let mut list = card().id(LIST).flex().flex_col();
            for (at, row) in own.iter().enumerate() {
                if at > 0 {
                    list = list.child(divider());
                }
                let changing = connections.is_changing(&row.id);
                let id = row.id.clone();
                let app = app.clone();
                list = list.child(
                    h_flex()
                        .id(SharedString::from(row_id(&row.id)))
                        .w_full()
                        .px(px(16.))
                        .py(px(14.))
                        .gap(px(12.))
                        .items_center()
                        // A row whose change is with the server is still the server's row until
                        // it answers, so it stays, dimmed.
                        .when(changing, |this| this.opacity(0.6))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(0.))
                                .gap(px(2.))
                                .child(div().text_sm().child(row.label.clone()))
                                .child(div().text_xs().text_color(muted).child(row_detail(
                                    connections,
                                    row,
                                    name_of,
                                )))
                                .when_some(connections.disconnect_refusal(&row.id), |this, why| {
                                    this.child(
                                        div()
                                            .id(SharedString::from(row_error_id(&row.id)))
                                            .text_xs()
                                            .text_color(danger)
                                            .child(why.to_string()),
                                    )
                                }),
                        )
                        .child(if connections.is_confirming_disconnect(&row.id) {
                            // Asked first: the row says which Bots would lose the service, and
                            // nothing is sent until Yes.
                            let yes_app = app.clone();
                            let yes_id = id.clone();
                            v_flex()
                                .flex_shrink_0()
                                .items_end()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .id(SharedString::from(confirm_question_id(&row.id)))
                                        .text_xs()
                                        .child(confirm_question(connections, row, name_of)),
                                )
                                .child(
                                    h_flex()
                                        .gap(px(6.))
                                        .child(
                                            Button::new(ElementId::Name(
                                                confirm_no_id(&row.id).into(),
                                            ))
                                            .label("No")
                                            .ghost()
                                            .small()
                                            .on_click(move |_, _, cx| {
                                                app.update(cx, |state, cx| {
                                                    state.keep_connection(cx);
                                                });
                                            }),
                                        )
                                        .child(
                                            Button::new(ElementId::Name(
                                                confirm_yes_id(&row.id).into(),
                                            ))
                                            .label("Yes, disconnect")
                                            .danger()
                                            .small()
                                            .on_click(move |_, _, cx| {
                                                yes_app.update(cx, |state, cx| {
                                                    state.disconnect_connection(yes_id.clone(), cx);
                                                });
                                            }),
                                        ),
                                )
                                .into_any_element()
                        } else {
                            div()
                                .flex_shrink_0()
                                .child(
                                    Button::new(ElementId::Name(disconnect_id(&row.id).into()))
                                        .label(disconnect_label(connections, &row.id))
                                        .ghost()
                                        .small()
                                        .disabled(changing)
                                        .on_click(move |_, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.ask_to_disconnect(id.clone(), cx);
                                            });
                                        }),
                                )
                                .into_any_element()
                        }),
                );
            }
            list.into_any_element()
        }
    };

    let offer: Option<AnyElement> = match connect_offer(connections) {
        ConnectOffer::Asking => Some(
            div()
                .text_sm()
                .text_color(muted)
                .child(ASKING)
                .into_any_element(),
        ),
        ConnectOffer::Unavailable(why) => Some(
            div()
                .id(OFFERED_ERROR)
                .text_sm()
                .text_color(danger)
                .child(why.to_string())
                .into_any_element(),
        ),
        ConnectOffer::Nothing(line) => Some(
            div()
                .id(OFFERED_EMPTY)
                .text_sm()
                .text_color(muted)
                .child(line)
                .into_any_element(),
        ),
        ConnectOffer::Unknown => None,
        ConnectOffer::Offered(open) => {
            let mut list = card().id(OFFERED).flex().flex_col();
            for (at, connector) in open.into_iter().enumerate() {
                if at > 0 {
                    list = list.child(divider());
                }
                let name = connector.name.clone();
                let waiting = connections.is_waiting(&name, std::time::Instant::now());
                let refused = connections
                    .connect_refused
                    .as_ref()
                    .filter(|(refused, _)| *refused == name)
                    .map(|(_, why)| why.clone());
                let label = connections.connector_label(&name);
                let app = app.clone();
                list = list.child(
                    h_flex()
                        .w_full()
                        .px(px(16.))
                        .py(px(14.))
                        .gap(px(12.))
                        .items_center()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(0.))
                                .gap(px(2.))
                                .child(div().text_sm().child(label.clone()))
                                .when(waiting, |this| {
                                    this.child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(waiting_line(&label)),
                                    )
                                })
                                .when_some(refused, |this, why| {
                                    this.child(
                                        div()
                                            .id(SharedString::from(connect_error_id(&name)))
                                            .text_xs()
                                            .text_color(danger)
                                            .child(why),
                                    )
                                }),
                        )
                        .child(
                            div().flex_shrink_0().child(
                                Button::new(ElementId::Name(connect_id(&name).into()))
                                    .label(connect_label(connections, connector))
                                    .small()
                                    // One sign-in page at a time.
                                    .disabled(connections.opening.is_some())
                                    .on_click(move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.connect_service(name.clone(), cx);
                                        });
                                    }),
                            ),
                        ),
                );
            }
            Some(list.into_any_element())
        }
    };

    v_flex()
        .gap(px(12.))
        .child(
            h_flex()
                .w_full()
                .items_start()
                .justify_between()
                .gap(px(16.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_xs()
                        .text_color(muted)
                        .child(INTRO),
                )
                .child(
                    div().flex_shrink_0().child(
                        Button::new(REFRESH)
                            .label("Refresh")
                            .ghost()
                            .small()
                            .on_click({
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| state.refresh_connections(cx));
                                }
                            }),
                    ),
                ),
        )
        .child(div().text_xs().text_color(muted).child("Connected"))
        .child(connected)
        .when_some(offer, |this, offer| {
            this.child(div().text_xs().text_color(muted).child("Connect a service"))
                .child(offer)
        })
}

/// A Bot's Connections card: each of the person's connections with a switch, lent to this Bot
/// or not, and what lending does and does not do yet.
pub(crate) fn agent_card(app: Entity<AppState>, coworker_id: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let danger = theme.danger;
    let connections = &app.read(cx).connections;
    let bot_name = app.read(cx).active_bot_name();
    // Only the person's own: nothing of another scope is theirs to lend.
    let rows: Vec<AnyElement> = connections
        .own_rows()
        .into_iter()
        .map(|row| {
            let lent = connections.shows_lent(row, coworker_id);
            let changing = connections.is_changing(&row.id);
            let service = connections.connector_label(&row.connector);
            let id = row.id.clone();
            let app = app.clone();
            v_flex()
                .gap(px(2.))
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(10.))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(0.))
                                .gap(px(1.))
                                .child(div().text_sm().child(row.label.clone()))
                                .child(div().text_xs().text_color(muted).child(service.clone())),
                        )
                        .child(
                            Switch::new(ElementId::Name(lend_id(&row.id).into()))
                                .checked(lent)
                                .small()
                                .accessibility_label(format!("Lend {service} to {bot_name}"))
                                // A switch pressed twice would send two answers about one loan.
                                .disabled(changing)
                                .on_click(move |next, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.set_connection_lent(id.clone(), *next, cx);
                                    });
                                }),
                        ),
                )
                // A refusal about lending it to another Bot is that Bot's card's to say.
                .when_some(
                    connections.lend_refusal(&row.id, coworker_id),
                    |this, why| {
                        this.child(
                            div()
                                .id(SharedString::from(lend_error_id(&row.id)))
                                .text_xs()
                                .text_color(danger)
                                .child(why.to_string()),
                        )
                    },
                )
                .into_any_element()
        })
        .collect();
    div()
        .id(AGENT_CARD)
        .mb(px(16.))
        .px(px(14.))
        .py(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .child(
            v_flex()
                .gap(px(2.))
                .child(div().text_sm().child("Connections"))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(agent_summary(connections, coworker_id)),
                ),
        )
        .when(!rows.is_empty(), |this| {
            this.child(v_flex().pt(px(10.)).gap(px(10.)).children(rows))
        })
        .child(
            div()
                .id(AGENT_NOTE)
                .pt(px(10.))
                .text_xs()
                .text_color(muted)
                .child(LEND_NOTE),
        )
}

#[cfg(test)]
mod tests {
    /// The row's question names the service and the Bots that would lose it, the way the row's
    /// own line names them.
    #[test]
    fn a_disconnect_asks_about_the_bots_it_would_cut_off() {
        let connections = AccountConnections::default();
        let row = |loans: &[&str]| ConnectionView {
            id: "conn_1".into(),
            connector: "gmail".into(),
            owner: crate::opengrok::ConnectionOwner::User("acct_1".into()),
            label: "you@work.com".into(),
            loans: loans.iter().map(|lent| lent.to_string()).collect(),
            updated_at_ms: 1,
            expires_at_ms: None,
        };
        let name_of = |id: &str| match id {
            "cw_1" => Some("Ada".to_string()),
            "cw_2" => Some("Bo".to_string()),
            _ => None,
        };
        assert_eq!(
            super::confirm_question(&connections, &row(&["cw_1", "cw_2"]), name_of),
            "Disconnect gmail? Ada and Bo will lose it."
        );
        assert_eq!(
            super::confirm_question(&connections, &row(&[]), name_of),
            "Disconnect gmail? No Bot is using it."
        );
    }

    // Named rather than globbed: the module above takes all of `gpui_kit`, whose own `test`
    // attribute would stand in for the standard one.
    use super::{
        ALL_CONNECTED, ASKING, ConnectOffer, NO_CONNECTORS, NOTHING_TO_LEND, agent_summary,
        connect_label, connect_offer, lent_line, row_detail,
    };
    use crate::opengrok::{ConnectionOwner, ConnectionView, Connector};
    use crate::state::{AccountConnections, ConnectionChange, ConnectionList, ConnectorList};

    fn row(id: &str, connector: &str, loans: &[&str]) -> ConnectionView {
        ConnectionView {
            id: id.into(),
            connector: connector.into(),
            owner: ConnectionOwner::User("acct_1".into()),
            label: format!("{connector} account"),
            loans: loans.iter().map(|lent| lent.to_string()).collect(),
            updated_at_ms: 1,
            expires_at_ms: None,
        }
    }

    fn names(id: &str) -> Option<String> {
        match id {
            "cw_1" => Some("Ada".into()),
            "cw_2" => Some("Bo".into()),
            "cw_3" => Some("Cy".into()),
            _ => None,
        }
    }

    /// A connection says who it is lent to by the names on the person's list, and counts the
    /// Bots it cannot name rather than showing their ids.
    #[test]
    fn a_connection_says_who_it_is_lent_to_by_name() {
        let lent = |loans: &[&str]| {
            let loans: Vec<String> = loans.iter().map(|id| id.to_string()).collect();
            lent_line(&loans, names)
        };
        assert_eq!(lent(&[]), "Not lent to any Bot");
        assert_eq!(lent(&["cw_1"]), "Lent to Ada");
        assert_eq!(lent(&["cw_1", "cw_2"]), "Lent to Ada and Bo");
        assert_eq!(lent(&["cw_1", "cw_2", "cw_3"]), "Lent to Ada, Bo and Cy");
        assert_eq!(
            lent(&["cw_1", "cw_x"]),
            "Lent to Ada and 1 Bot not on your list"
        );
        assert_eq!(lent(&["cw_x", "cw_y"]), "Lent to 2 Bots not on your list");
    }

    /// A Bot's card counts what of the person's own is lent to it, the switch that is with the
    /// server counted as it was asked, and says why there is nothing to count.
    #[test]
    fn a_bots_card_counts_what_is_lent_to_it() {
        let mut connections = AccountConnections::default();
        assert_eq!(agent_summary(&connections, "cw_1"), ASKING);
        let bots_own = ConnectionView {
            owner: ConnectionOwner::Bot("cw_1".into()),
            ..row("conn_3", "drive", &["cw_1"])
        };
        connections.list = Some(ConnectionList::Listed(vec![bots_own.clone()]));
        assert_eq!(
            agent_summary(&connections, "cw_1"),
            NOTHING_TO_LEND,
            "a bot's own sign-in is not the person's to lend"
        );
        connections.list = Some(ConnectionList::Listed(vec![
            row("conn_1", "gmail", &["cw_1"]),
            row("conn_2", "github", &[]),
            bots_own,
        ]));
        assert_eq!(
            agent_summary(&connections, "cw_1"),
            "1 of 2 lent to this Bot"
        );
        assert_eq!(
            agent_summary(&connections, "cw_2"),
            "0 of 2 lent to this Bot"
        );
        connections
            .changing
            .insert("conn_2".into(), ConnectionChange::Lend("cw_1".into()));
        assert_eq!(
            agent_summary(&connections, "cw_1"),
            "2 of 2 lent to this Bot"
        );
        connections.list = Some(ConnectionList::Unavailable(
            "Your connections could not be read.".into(),
        ));
        assert_eq!(
            agent_summary(&connections, "cw_1"),
            "Your connections could not be read."
        );
    }

    /// Settings → Connections says who a connection is lent to as the Bot's switch does: a lend
    /// or a revoke that is with the server is shown as asked, and only for the Bot it is about.
    #[test]
    fn a_settings_row_shows_a_change_in_flight_as_the_bots_switch_does() {
        let mut connections = AccountConnections::default();
        let gmail = row("conn_1", "gmail", &["cw_1"]);
        connections.list = Some(ConnectionList::Listed(vec![gmail.clone()]));
        assert_eq!(
            row_detail(&connections, &gmail, names),
            "gmail · Lent to Ada"
        );
        connections
            .changing
            .insert("conn_1".into(), ConnectionChange::Lend("cw_2".into()));
        assert_eq!(
            row_detail(&connections, &gmail, names),
            "gmail · Lent to Ada and Bo"
        );
        connections
            .changing
            .insert("conn_1".into(), ConnectionChange::Revoke("cw_1".into()));
        assert_eq!(
            row_detail(&connections, &gmail, names),
            "gmail · Not lent to any Bot"
        );
        connections
            .changing
            .insert("conn_1".into(), ConnectionChange::Disconnect);
        assert_eq!(
            row_detail(&connections, &gmail, names),
            "gmail · Lent to Ada",
            "the loans go with a disconnect once the server says so"
        );
    }

    /// Only a service that is not connected is offered, only once both lists are in, and each
    /// empty state says which it is.
    #[test]
    fn a_service_is_offered_only_while_it_is_not_connected() {
        let gmail = Connector {
            name: "gmail".into(),
            label: "Gmail".into(),
        };
        let github = Connector {
            name: "github".into(),
            label: String::new(),
        };
        let mut connections = AccountConnections::default();
        assert_eq!(connect_offer(&connections), ConnectOffer::Asking);
        connections.connectors = Some(ConnectorList::Listed(vec![gmail.clone(), github.clone()]));
        assert_eq!(
            connect_offer(&connections),
            ConnectOffer::Asking,
            "what is connected is not known yet"
        );
        connections.list = Some(ConnectionList::Listed(vec![row("conn_1", "gmail", &[])]));
        assert_eq!(
            connect_offer(&connections),
            ConnectOffer::Offered(vec![&github])
        );
        assert_eq!(connect_label(&connections, &github), "Connect github");
        assert_eq!(connect_label(&connections, &gmail), "Connect Gmail");
        connections.opening = Some("github".into());
        assert_eq!(connect_label(&connections, &github), "Opening…");

        connections.list = Some(ConnectionList::Listed(vec![
            row("conn_1", "gmail", &[]),
            row("conn_2", "github", &[]),
        ]));
        assert_eq!(
            connect_offer(&connections),
            ConnectOffer::Nothing(ALL_CONNECTED)
        );
        connections.connectors = Some(ConnectorList::Listed(Vec::new()));
        assert_eq!(
            connect_offer(&connections),
            ConnectOffer::Nothing(NO_CONNECTORS)
        );
        connections.connectors = Some(ConnectorList::Listed(vec![gmail]));
        connections.list = Some(ConnectionList::Unavailable("no".into()));
        assert_eq!(connect_offer(&connections), ConnectOffer::Unknown);
        connections.connectors = Some(ConnectorList::Unavailable("why".into()));
        assert_eq!(
            connect_offer(&connections),
            ConnectOffer::Unavailable("why")
        );
    }
}
