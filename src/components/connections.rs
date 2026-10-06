//! Connections (#2): a person signs in to a service once, on Settings → Connections, and lends
//! it to a Bot from that Bot's settings.
//!
//! Both surfaces draw the one list the account has ([`AccountConnections`]), and the words and
//! element ids they draw it with live here, so the gpui-agent tree (`agent/host.rs`) says what
//! the window says and names what the window names.
//!
//! Nothing here holds a connection. The server keeps it, a sign-in happens in the person's
//! browser and comes back to the server, and the app only ever reads the list again.

use crate::components::switch::Switch;
use crate::opengrok::ConnectionView;
use crate::state::{AccountConnections, AppState, ConnectionList};
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// A Bot's Connections card, and what it says about lending.
pub(crate) const AGENT_CARD: &str = "agent-connections";
pub(crate) const AGENT_NOTE: &str = "agent-connections-note";

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

/// A connection's switch on a Bot's Connections card: lent to this Bot, or not.
pub(crate) fn lend_id(connection_id: &str) -> String {
    format!("agent-connection-lend-{connection_id}")
}

/// Why that switch's last lend or revoke did not go through, on the card of the Bot it was about.
pub(crate) fn lend_error_id(connection_id: &str) -> String {
    format!("agent-connection-error-{connection_id}")
}

pub(crate) const ASKING: &str = "Asking the server…";
pub(crate) const NOTHING_TO_LEND: &str =
    "Nothing connected yet. Connect a service in Settings → Connections.";
/// What a Bot's card says about what lending does. A connection reaches a Bot's turn through a
/// plugin, and only a plugin the Bot is allowed, which its Tools card switches (opengrok-server
/// #268, `connect_plugins` gating on both). Saying less would be a switch that looks like it
/// changes the next turn when the plugin is not allowed.
pub(crate) const LEND_NOTE: &str = "A lent connection reaches a plugin only once this Bot is \
     allowed that plugin in its Tools.";

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

/// A Bot's Connections card: each of the person's connections with a switch, lent to this Bot
/// or not, and what lending does and does not do yet. The Bot's settings drew it under Skills
/// until Connections moved to the agent monitor's Plugins modal (hexuria/nativechat#174, #175),
/// which mounts it from here.
pub fn agent_card(app: Entity<AppState>, coworker_id: &str, cx: &App) -> impl IntoElement {
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
        .debug_selector(|| AGENT_CARD.into())
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
            kind: crate::opengrok::ConnectionKind::Oauth,
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
    use super::{ASKING, NOTHING_TO_LEND, agent_summary, lent_line};
    use crate::opengrok::{ConnectionOwner, ConnectionView};
    use crate::state::{AccountConnections, ConnectionChange, ConnectionList};

    fn row(id: &str, connector: &str, loans: &[&str]) -> ConnectionView {
        ConnectionView {
            id: id.into(),
            connector: connector.into(),
            owner: ConnectionOwner::User("acct_1".into()),
            label: format!("{connector} account"),
            loans: loans.iter().map(|lent| lent.to_string()).collect(),
            updated_at_ms: 1,
            expires_at_ms: None,
            kind: crate::opengrok::ConnectionKind::Oauth,
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
}
