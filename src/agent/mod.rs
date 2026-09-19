//! Opt-in gpui-agent control plane (feature `agent`).
//!
//! Start NativeChat with `GPUI_AGENT=1` (and a token, or
//! `GPUI_AGENT_INSECURE_NO_TOKEN=1`). The TCP thread posts onto a mailbox;
//! [`RootView`](crate::root::RootView) drains it on the UI thread.
//!
//! Stable ids: `app-window`, `sidebar`, `sidebar-chat-list`, `nav-new-chat`,
//! `nav-toggle-sidebar`, `session-{id}`, `footer-theme`, `footer-account`,
//! `composer`, `composer-panel`, `composer-panel-search`, `composer-recipe-bar`,
//! `image-thumb-{n}`, `lightbox`, `user-form-{key}`, `user-form-field-{key}-{id}`,
//! `user-form-continue-{key}`, `user-form-dismiss-{key}`, `user-form-screen-{key}`,
//! `user-form-pill-{key}`, `computer-handoff-{key}`,
//! `computer-handoff-takeover-{key}`, `computer-handoff-done-{key}`,
//! `computer-handoff-skip-{key}`,
//! `save-login-{entry}`, `save-login-save-{entry}`, `save-login-skip-{entry}`,
//! `credential-request-{id}`, `credential-request-allow-{id}`,
//! `credential-request-deny-{id}`, `credential-request-pill-{id}`,
//! `settings-tab-logins`,
//! `settings-login-row-{id}`, `settings-login-delete-{id}`.
//!
//! Named invokes (parity / gpui-agent): `UserFormContinue`, `UserFormDismiss`,
//! `UserFormOpenScreen`, `AnswerCredentialRequest` (also kebab
//! `user-form.continue` / `user-form.dismiss` / `user-form.screen` /
//! `credential.answer`). Click ids above still work.
//!
//! Typing goes in as GPUI keystrokes. `type`, `key` and `set_value` on the composer are
//! planned here ([`ComposePlan`]) and pressed by [`RootView`](crate::root::RootView), because
//! `/` and `@` are keys the composer takes before the text field ever sees them.

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
