//! The middle pane: the search field, the notice and error lines, and the rows.

use std::collections::HashMap;
use std::sync::Arc;

use super::site_icon;
use crate::chrome::TITLE_BAR_H;
use crate::components::fields::field_input;
use crate::site_login::{SiteLoginRecord, login_title};
use crate::state::AppState;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    search: &Entity<InputState>,
    shown: &[SiteLoginRecord],
    selected: Option<&str>,
    icons: &HashMap<String, Option<Arc<Image>>>,
    notice: Option<String>,
    error: Option<String>,
    any_rows: bool,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    v_flex()
        .id("settings-logins-list-pane")
        .w(px(300.))
        .h_full()
        .flex_shrink_0()
        .border_r_1()
        .border_color(theme.border)
        .pt(px(TITLE_BAR_H))
        .child(
            div()
                .id("settings-logins-search")
                .px(px(12.))
                .pb(px(8.))
                .child(field_input(search).cleanable(true)),
        )
        .when_some(notice, |this, notice| {
            this.child(
                div()
                    .id("settings-logins-notice")
                    .px(px(14.))
                    .pb(px(6.))
                    .text_xs()
                    .text_color(muted)
                    .child(notice),
            )
        })
        .when_some(error, |this, error| {
            this.child(
                div()
                    .id("settings-logins-error")
                    .px(px(14.))
                    .pb(px(6.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(error),
            )
        })
        .child(if shown.is_empty() {
            div()
                .id("settings-logins-empty")
                .px(px(14.))
                .py(px(12.))
                .text_sm()
                .text_color(muted)
                .child(if any_rows {
                    "No logins match."
                } else {
                    "No saved logins yet."
                })
                .into_any_element()
        } else {
            v_flex()
                .id("settings-logins-rows")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .px(px(8.))
                .pb(px(12.))
                .gap(px(1.))
                .children(shown.iter().map(|row| {
                    let icon = icons.get(&row.origin).and_then(|icon| icon.as_ref());
                    row_element(
                        row,
                        selected == Some(row.id.as_str()),
                        icon,
                        theme,
                        app.clone(),
                    )
                }))
                .into_any_element()
        })
}

/// One row: the site's icon, its title, and the name under it.
fn row_element(
    row: &SiteLoginRecord,
    selected: bool,
    icon: Option<&Arc<Image>>,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let id = row.id.clone();
    h_flex()
        .id(SharedString::from(format!("settings-login-row-{}", row.id)))
        .w_full()
        .px(px(8.))
        .py(px(6.))
        .gap(px(10.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(selected, |this| this.bg(theme.list_active))
        .when(!selected, |this| this.hover(|s| s.bg(theme.list_hover)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            app.update(cx, |state, cx| {
                state.select_site_login(Some(id.clone()), cx);
            });
        })
        .child(site_icon(&row.origin, icon, 28., theme))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .gap(px(1.))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .truncate()
                        .child(login_title(row).to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(row.username.clone()),
                ),
        )
}
