//! Tools and Plugins open from the Computer pane (hexuria/nativechat#175, R-A and P1).
//! The lists, switch state and errors are the existing server-backed cards' own. Step one of
//! Plugins uses connections and private skills; catalog installation and extra accounts await
//! opengrok-server#356. Connector metadata is not invented where the server supplies none.

use crate::components::agent_settings::{shown_skill_rows, skills_card_lines, skills_summary};
use crate::components::switch::Switch;
use crate::components::{agent_settings, connections};
use crate::state::{AppState, ConnectionList, ConnectorList, SkillsCard};
use gpui_kit::component::tooltip::Tooltip;
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
}

impl PluginSelection {
    pub(crate) fn detail_id(&self) -> String {
        match self {
            Self::Connection(id) => format!("monitor-connection-detail-{id}"),
            Self::Skill(id) => format!("monitor-skill-detail-{id}"),
        }
    }
    pub(crate) fn switch_id(&self) -> String {
        match self {
            Self::Connection(id) => connections::lend_id(id),
            Self::Skill(id) => format!("agent-skills-switch-{id}"),
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

pub(crate) fn plugin_exists(state: &AppState, selected: &PluginSelection) -> bool {
    plugin_rows(state)
        .iter()
        .any(|row| &row.selection == selected)
}

/// Only facts the current server supplies, with an explicit absence for metadata catalog #356
/// will add. Connections routes #267/#269 expose no source, transport, URL or tool membership.
#[derive(Clone, Debug)]
pub(crate) struct PluginDetail {
    pub title: String,
    pub fields: Vec<(&'static str, String)>,
    pub question: String,
    pub can_remove: bool,
    pub connection: bool,
    pub error: Option<String>,
}

pub(crate) fn plugin_detail(state: &AppState, selected: &PluginSelection) -> Option<PluginDetail> {
    match selected {
        PluginSelection::Connection(id) => {
            let row = state
                .connections
                .own_rows()
                .into_iter()
                .find(|row| &row.id == id)?;
            let name_of = |id: &str| {
                state
                    .coworkers
                    .iter()
                    .find(|bot| bot.id == id)
                    .map(|bot| bot.name.clone())
            };
            Some(PluginDetail {
                title: state.connections.connector_label(&row.connector),
                fields: vec![
                    ("Source", row.connector.clone()),
                    ("Transport", NOT_SUPPLIED.into()),
                    ("URL", NOT_SUPPLIED.into()),
                    ("Tools", NOT_SUPPLIED.into()),
                    ("Accounts", row.label.clone()),
                ],
                question: connections::confirm_question(&state.connections, row, name_of),
                can_remove: !state.connections.is_changing(id),
                connection: true,
                error: state.connections.disconnect_refusal(id).map(str::to_string),
            })
        }
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
                connection: false,
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
        let theme = cx.theme().clone();
        let app = self.state.clone();
        let body = match modal.kind {
            MonitorKind::Tools => agent_settings::tools_card(app.clone(), &theme, cx)
                .unwrap_or_else(|| div().child(connections::ASKING).into_any_element()),
            MonitorKind::Plugins => plugins_body(app.clone(), &modal, &theme, cx),
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
                    app.update(cx, |state, cx| state.close_monitor_modal(cx))
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

fn plugins_body(
    app: Entity<AppState>,
    modal: &MonitorModal,
    theme: &gpui_kit::component::Theme,
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
        if detail.connection {
            body = body.child(
                button(ADD_ACCOUNT, "Add another account", false, |_| {})
                    .tooltip(|window, cx| Tooltip::new("Coming later").build(window, cx)),
            );
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
            },
            ConnectionView {
                id: "conn_foreign".into(),
                connector: "github".into(),
                label: "other".into(),
                owner: ConnectionOwner::Bot("cw_1".into()),
                loans: vec![],
                updated_at_ms: 1,
                expires_at_ms: None,
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
    fn plugins_open_a_detail_disable_extra_accounts_and_confirm_removal(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (state, cx) = open(cx, MonitorKind::Plugins);
        click(cx, "monitor-connection-detail-conn_1");
        assert!(cx.debug_bounds(super::DETAIL).is_some());
        for id in [
            "monitor-plugin-source",
            "monitor-plugin-transport",
            "monitor-plugin-url",
            "monitor-plugin-tools",
            "monitor-plugin-accounts",
        ] {
            assert!(cx.debug_bounds(id).is_some(), "{id}");
        }
        click(cx, super::ADD_ACCOUNT);
        assert!(!state.read_with(cx, |state, _| {
            state.monitor_modal.as_ref().unwrap().confirming
        }));
        click(cx, super::REMOVE);
        assert!(state.read_with(cx, |state, _| {
            state.monitor_modal.as_ref().unwrap().confirming
        }));
        assert!(
            state.read_with(cx, |state, _| state.connections.changing.is_empty()),
            "the question sends nothing"
        );
        click(cx, super::REMOVE_NO);
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
