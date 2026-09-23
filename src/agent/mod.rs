//! Opt-in gpui-agent control plane (feature `agent`).
//!
//! Start NativeChat with `GPUI_AGENT=1` (and a token, or
//! `GPUI_AGENT_INSECURE_NO_TOKEN=1`). The TCP thread posts onto a mailbox;
//! [`RootView`](crate::root::RootView) drains it on the UI thread.
//!
//! Stable ids: `app-window`, `sidebar`, `sidebar-chat-list`, `nav-new-chat`,
//! `nav-toggle-sidebar`, `session-{id}`, `footer-theme`, `footer-account`,
//! `composer`, `composer-panel`, `composer-panel-search`, `composer-recipe-bar`,
//! `composer-skill` (the skill the next message is sent with, value = the id the turn names;
//! in the tree only while one is on the draft),
//! `image-thumb-{n}`, `lightbox`, `user-form-{key}`, `user-form-field-{key}-{id}`,
//! `user-form-continue-{key}`, `user-form-dismiss-{key}`, `user-form-screen-{key}`,
//! `user-form-pill-{key}`, `computer-handoff-{key}`,
//! `computer-handoff-takeover-{key}`, `computer-handoff-done-{key}`,
//! `computer-handoff-skip-{key}`,
//! `save-login-{entry}`, `save-login-save-{entry}`, `save-login-skip-{entry}`,
//! `user-form-use-saved-{key}-{login}` (one per saved account for the card's site),
//! `user-form-saved-note-{key}`, `user-form-saved-clear-{key}`,
//! `settings-tab-logins`, `settings-logins-search` (value = the query), `settings-login-add`,
//! `settings-login-import`, `settings-logins-notice`, `settings-logins-error`,
//! `settings-logins-empty`, `settings-logins-group-passwords|passkeys|codes|security` (a
//! section of the list, value = its count; the three kinds are always there until a search
//! leaves one empty, Security only while a row has a `Security:` note) with its rows
//! `settings-login-row-{id}` under it (a row with a `Security:` note is under its kind and
//! under Security, one id twice; the picked one has state `selected`),
//! `settings-login-code-{id}` (a row's live code and the seconds left, on the pane and in
//! the list), `settings-login-add-error` while the Add sheet shows one,
//! `settings-login-notes-save-{id}` (the button that files an edited note),
//! `settings-login-detail-{id}` (the picked row's pane: `settings-login-username-{id}`,
//! `settings-login-website-{id}`, `settings-login-where-{id}`, `settings-login-notes-{id}`
//! (value = the notes), `settings-login-last-used-{id}`, `settings-login-delete-{id}`),
//! `settings-login-add-sheet` while the Add sheet is up (`settings-login-add-title`,
//! `settings-login-add-username`, `settings-login-add-password`, `settings-login-add-website`,
//! `settings-login-add-notes`, `settings-login-add-save`, `settings-login-add-cancel`),
//! `routine-new`, `routine-{id}`, `routine-{id}-trigger-schedule`,
//! `routine-{id}-trigger-webhook`, `routine-{id}-webhook-url`,
//! `routine-{id}-webhook-key`, `routine-{id}-rotate`, `routine-{id}-delete`.
//!
//! A routine's `{id}` is the server's schedule id. The two trigger ids are in the tree only
//! while the routine has no trigger, and the webhook's three only while it has one, so
//! `assert --exists false` answers "this one already fires" and "this one is not a webhook".
//!
//! Named invokes (parity / gpui-agent): `UserFormContinue`, `UserFormDismiss`,
//! `UserFormOpenScreen`, `UserFormUseSaved`, `UserFormClearSaved` (also kebab
//! `user-form.continue` / `user-form.dismiss` / `user-form.screen` /
//! `user-form.use-saved --arg login_id=…` / `user-form.clear-saved`), `AddSiteLogin`
//! (`logins.add --arg origin= --arg username= --arg password= [--arg label= --arg notes=]`)
//! and `ImportSiteLogins` (`logins.import --arg path=`). Click ids above still work.
//!
//! Settings → Logins: `logins.list` (answers the rows — `id, kind, label, origin, username,
//! on_this_mac, last_used_at_ms`; never a password), `logins.search --arg q=…` (no `q`
//! clears; `set_value` / `type` / `key` on `settings-logins-search` do the same),
//! `logins.select --arg id=…` (no `id` clears the pick; a click on a row does the same),
//! `logins.notes --arg id=… --arg notes=…` (or `set_value` on `settings-login-notes-{id}`).
//! The Add sheet's fields are the window's own: `logins.add` carries the values instead.
//!
//! Routines: `routine.list` (answers with the open bot's rows —
//! `id, name, kind, cron, active, webhook_url, webhook_key`), `routine.create --arg
//! kind=cron|webhook --arg prompt=... [--arg cron=...]`, `routine.rotate --arg id=...`,
//! `routine.delete --arg id=...`.
//!
//! Typing goes in as GPUI keystrokes. `type`, `key` and `set_value` on the composer are
//! planned here ([`ComposePlan`]) and pressed by [`RootView`](crate::root::RootView), because
//! `/` and `@` are keys the composer takes before the text field ever sees them.
//!
//! `/` has no verb of its own: a row is taken the way a person takes it, by typing into
//! `composer-panel-search` and pressing Enter, which is the only path that puts the chip in the
//! message and the thing on the draft together. Two skills may share a name, and Enter takes
//! the first selectable row the search leaves: tell them apart by their description, which the
//! search reads too, or by counting rows under `composer-panel` and arrowing down to the one
//! wanted. `composer-skill`'s value says which of them was actually taken.

mod host;
#[cfg(target_os = "macos")]
mod macos_window;

pub use gpui_agent::mailbox::AgentMailbox;
pub use host::{Command, ComposePlan, NativeChatHost, ids};

use std::time::Duration;

use gpui_agent::security::from_env;
use gpui_agent::server::spawn_mailbox;

fn auth_banner(token_set: bool) -> &'static str {
    if token_set {
        "auth: required (GPUI_AGENT_TOKEN set; clients must send the same token)"
    } else {
        "auth: none (GPUI_AGENT_INSECURE_NO_TOKEN=1 — any local process can drive this host)"
    }
}

/// Start the localhost control plane when `GPUI_AGENT=1`.
pub fn maybe_start() -> Option<AgentMailbox> {
    match from_env() {
        Ok(None) => {
            eprintln!("agent control plane off (set GPUI_AGENT=1 to opt in)");
            None
        }
        Err(err) => {
            eprintln!("agent control plane refused: {err}");
            None
        }
        Ok(Some(config)) => {
            let mailbox = AgentMailbox::new();
            let auth = auth_banner(config.token.is_some());
            match spawn_mailbox(
                config.addr,
                config.token,
                mailbox.clone(),
                Duration::from_secs(30),
            ) {
                Ok((addr, _)) => {
                    eprintln!("gpui-agent listening on {addr} (platform=desktop, app=nativechat)");
                    eprintln!("opt-in: GPUI_AGENT=1 · bind via from_env · protocol v2");
                    eprintln!("{auth}");
                    eprintln!(
                        "screenshot: macOS writes this window via screencapture -l (Screen Recording)"
                    );
                    Some(mailbox)
                }
                Err(err) => {
                    eprintln!("gpui-agent failed to bind: {err}");
                    None
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub fn screenshot_this_window(
    window: &gpui_kit::Window,
    path: Option<&str>,
) -> Result<gpui_agent::DispatchResult, String> {
    let path = gpui_agent::require_screenshot_path(path)?;
    let _dest = gpui_agent::confine_screenshot_path(path)?;
    let id = macos_window::cgwindow_id(window)?;
    gpui_agent::capture_window_via_screencapture(id, Some(path))
}

#[cfg(not(target_os = "macos"))]
pub fn screenshot_this_window(
    _window: &gpui_kit::Window,
    _path: Option<&str>,
) -> Result<gpui_agent::DispatchResult, String> {
    Err(gpui_agent::screenshot_unavailable(
        "desktop PNG of the app window is macOS-only (`screencapture -l`)",
    ))
}
