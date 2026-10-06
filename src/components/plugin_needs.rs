//! The card a message's turn answers with when a plugin it tagged needs something first (#360):
//! which of the person's accounts to use, an account at all, or an install. No model was asked,
//! so nothing here is the Bot's words: it is the server saying what the tag still needs, and each
//! answer sends the same message again.
//!
//! THE BOT NEVER GUESSES WHICH ACCOUNT. Two Cloudflare accounts and no choice on record is a
//! question for the person, asked here with one tap per account; "Remember for <Bot>" keeps the
//! choice as the Bot's pin, the one Plugins shows, so the next tag does not ask again.

use crate::components::switch::Switch;
use crate::opengrok::{PluginNeed, PluginNeedKind, PluginNeedsSpec};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

/// The card's own id, and its controls', by the reply row it is on and what each acts on: the
/// agent catalogue names them (`agent/mod.rs`).
pub fn card_id(message_id: &str) -> String {
    format!("plugin-needs-{message_id}")
}
pub fn use_id(message_id: &str, account_id: &str) -> String {
    format!("plugin-needs-use-{message_id}-{account_id}")
}
pub fn remember_id(message_id: &str) -> String {
    format!("plugin-needs-remember-{message_id}")
}
pub fn open_id(message_id: &str, plugin: &str) -> String {
    format!("plugin-needs-open-{message_id}-{plugin}")
}
pub fn again_id(message_id: &str) -> String {
    format!("plugin-needs-again-{message_id}")
}

/// A plugin's name as a person reads it: `cloudflare` is Cloudflare.
fn titled(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// How an account was added, in the words Plugins uses for it.
fn how(kind: &str) -> &'static str {
    match kind {
        "mcp" => "Signed in",
        "token" => "Key",
        _ => "Account",
    }
}

pub fn render_plugin_needs(
    spec: &PluginNeedsSpec,
    message_id: &str,
    app: Entity<AppState>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let state = app.read(cx);
    let bot = state.active_bot_name();
    let remember = state.plugin_needs_remember.contains(message_id);
    // Only the thread's last turn can be sent again: an older one has messages after it.
    let answerable = state.plugin_needs_answerable(message_id);
    let needs_again = spec.send_again
        && spec
            .needs
            .iter()
            .any(|need| !matches!(need.kind, PluginNeedKind::Choose(_)));
    let mut card = v_flex()
        .id(ElementId::Name(card_id(message_id).into()))
        .w_full()
        .gap(px(12.))
        .p(px(14.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background);
    for need in &spec.needs {
        card = card.child(render_need(
            need,
            message_id,
            &bot,
            answerable,
            app.clone(),
            cx,
        ));
    }
    let chooses = spec
        .needs
        .iter()
        .any(|need| matches!(need.kind, PluginNeedKind::Choose(_)));
    if chooses {
        let toggled = message_id.to_string();
        let target = app.clone();
        card = card.child(
            Switch::new(SharedString::from(remember_id(message_id)))
                .small()
                .checked(remember)
                .disabled(!answerable)
                .label(format!("Remember for {bot}"))
                .on_click(move |_, _, cx| {
                    target.update(cx, |state, cx| {
                        state.toggle_needs_remember(toggled.clone(), cx)
                    });
                }),
        );
    }
    if needs_again {
        let again = message_id.to_string();
        let target = app.clone();
        card = card.child(
            h_flex().child(
                Button::new(SharedString::from(again_id(message_id)))
                    .label("Send again")
                    .ghost()
                    .small()
                    .disabled(!answerable)
                    .on_click(move |_, _, cx| {
                        target.update(cx, |state, cx| state.resend_after_needs(again.clone(), cx));
                    }),
            ),
        );
    }
    card.into_any_element()
}

fn render_need(
    need: &PluginNeed,
    message_id: &str,
    bot: &str,
    answerable: bool,
    app: Entity<AppState>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let name = titled(&need.plugin);
    let title = |text: String| {
        div()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.foreground)
            .child(text)
    };
    let note = |text: String| {
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text)
    };
    let open = |label: &'static str| {
        let plugin = need.plugin.clone();
        let target = app.clone();
        Button::new(SharedString::from(open_id(message_id, &need.plugin)))
            .label(label)
            .primary()
            .small()
            .on_click(move |_, _, cx| {
                target.update(cx, |state, cx| state.open_plugin_detail(plugin.clone(), cx));
            })
    };
    match &need.kind {
        PluginNeedKind::Install => v_flex()
            .gap(px(6.))
            .child(title(format!("{name} isn't installed")))
            .child(note(format!(
                "Install it in Plugins, then send again. {bot} can use it as soon as it has an account."
            )))
            .child(h_flex().child(open("Open in Plugins")))
            .into_any_element(),
        PluginNeedKind::Account => v_flex()
            .gap(px(6.))
            .child(title(format!("{name} needs an account")))
            .child(note(
                "Sign in or add a key in Plugins, then send again. It never goes through chat.".to_string(),
            ))
            .child(h_flex().child(open("Add account")))
            .into_any_element(),
        PluginNeedKind::Choose(accounts) => {
            let connector = need.connector.clone().unwrap_or_else(|| need.plugin.clone());
            let rows = accounts.iter().map(|(id, label, kind)| {
                let (plugin, connector, id_owned) =
                    (need.plugin.clone(), connector.clone(), id.clone());
                let message = message_id.to_string();
                let target = app.clone();
                h_flex()
                    .w_full()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().text_color(theme.foreground).child(label.clone()))
                            .child(note(how(kind).to_string())),
                    )
                    .child(
                        Button::new(SharedString::from(use_id(message_id, id)))
                            .label("Use")
                            .small()
                            .primary()
                            .disabled(!answerable)
                            .on_click(move |_, _, cx| {
                                target.update(cx, |state, cx| {
                                    state.choose_plugin_account(
                                        message.clone(),
                                        (plugin.clone(), connector.clone(), id_owned.clone()),
                                        cx,
                                    )
                                });
                            }),
                    )
            });
            v_flex()
                .gap(px(8.))
                .child(title(format!("Which {name} account should {bot} use?")))
                .children(rows)
                .into_any_element()
        }
    }
}
