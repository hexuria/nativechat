//! The right pane: the picked row, or the empty state when nothing is picked.

use std::sync::Arc;

use super::{LoginsPage, site_icon};
use crate::chrome::TITLE_BAR_H;
use crate::site_login::{SiteLoginRecord, added_date, login_title, relative_time};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{Icon, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    row: Option<&SiteLoginRecord>,
    on_this_mac: bool,
    code: Option<(String, u64)>,
    icon: Option<&Arc<Image>>,
    notes: &Entity<TextareaState>,
    notes_dirty: bool,
    view: Entity<LoginsPage>,
    app: Entity<AppState>,
    theme: &Theme,
) -> AnyElement {
    let Some(row) = row else {
        return empty(theme);
    };
    let id = row.id.clone();
    let muted = theme.muted_foreground;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let last_used = row
        .last_used_at_ms
        .map(|at| relative_time(at, now_ms))
        .unwrap_or_else(|| "Never".to_string());
    let where_line = crate::site_login::where_the_secret_is(&row.kind, on_this_mac);
    let (secret_label, secret_value) = crate::site_login::secret_placeholder(&row.kind);
    v_flex()
        .id(SharedString::from(format!("settings-login-detail-{id}")))
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .overflow_y_scroll()
        .pt(px(TITLE_BAR_H))
        .px(px(28.))
        .pb(px(28.))
        .gap(px(18.))
        .child(
            h_flex()
                .gap(px(14.))
                .items_center()
                .child(site_icon(&row.origin, icon, 56., theme))
                .child(
                    v_flex()
                        .min_w(px(0.))
                        .gap(px(2.))
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(login_title(row).to_string()),
                        )
                        .child(div().text_xs().text_color(muted).child(row.origin.clone())),
                ),
        )
        .child(
            card(theme)
                .child(kv_row("User Name", row.username.clone(), theme))
                .child(divider(theme))
                // Dots, and nothing to copy: the secret is never read to paint this pane.
                .child(kv_row(secret_label, secret_value, theme))
                .child(divider(theme))
                .child(kv_row("Website", row.origin.clone(), theme)),
        )
        .child(
            v_flex()
                .gap(px(6.))
                .child(div().text_xs().text_color(muted).child("Notes"))
                .child(
                    div()
                        .id(SharedString::from(format!("settings-login-notes-{id}")))
                        .w_full()
                        .child(
                            Textarea::new(notes)
                                .appearance(false)
                                .w_full()
                                .rounded(px(8.))
                                .border_1()
                                .border_color(theme.input)
                                .bg(theme.input_background()),
                        ),
                )
                .when(notes_dirty, |this| {
                    this.child(
                        h_flex().w_full().justify_end().child(
                            Button::new(SharedString::from(format!(
                                "settings-login-notes-save-{id}"
                            )))
                            .label("Save")
                            .primary()
                            .small()
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    view.update(cx, |page, cx| page.save_notes(cx));
                                }
                            }),
                        ),
                    )
                }),
        )
        .child(
            card(theme)
                .child(
                    kv_row("Where", where_line, theme)
                        .id(SharedString::from(format!("settings-login-where-{id}"))),
                )
                .when_some(code, |this, (digits, ttl)| {
                    let shown = format!(
                        "{} {} · {ttl} s",
                        &digits[..digits.len() / 2],
                        &digits[digits.len() / 2..]
                    );
                    this.child(divider(theme)).child(
                        kv_row("Code", shown, theme)
                            .id(SharedString::from(format!("settings-login-code-{id}"))),
                    )
                })
                .child(divider(theme))
                .child(kv_row("Last used", last_used, theme))
                .child(divider(theme))
                .child(kv_row("Added", added_date(&row.created_at), theme)),
        )
        .child(
            h_flex().child(
                Button::new(SharedString::from(format!("settings-login-delete-{id}")))
                    .label("Delete")
                    .danger()
                    .small()
                    .on_click(move |_, _, cx| {
                        app.update(cx, |state, cx| {
                            state.delete_site_login(id.clone(), cx);
                        });
                    }),
            ),
        )
        .into_any_element()
}

/// Nothing picked: the key and the words, in the middle of the pane.
fn empty(theme: &Theme) -> AnyElement {
    v_flex()
        .id("settings-login-detail-none")
        .flex_1()
        .h_full()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .child(
            Icon::default()
                .path("icons/key.svg")
                .size(px(28.))
                .text_color(theme.muted_foreground),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("No Item Selected"),
        )
        .into_any_element()
}

fn card(theme: &Theme) -> Div {
    v_flex()
        .w_full()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
}

fn divider(theme: &Theme) -> Div {
    div().h(px(1.)).bg(theme.border)
}

/// One line of the sheet: the key in the margin, the value beside it.
fn kv_row(key: &'static str, value: impl Into<SharedString>, theme: &Theme) -> Div {
    h_flex()
        .w_full()
        .px(px(14.))
        .py(px(9.))
        .gap(px(12.))
        .items_start()
        .child(
            div()
                .w(px(96.))
                .flex_shrink_0()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(key),
        )
        .child(div().flex_1().min_w(px(0.)).text_sm().child(value.into()))
}
