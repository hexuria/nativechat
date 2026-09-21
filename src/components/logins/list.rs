//! The left pane: the search field with "+" beside it, the notice and error lines, the
//! list grouped by kind, and Import… pinned at the bottom.

use std::collections::HashMap;
use std::sync::Arc;

use super::site_icon;
use crate::chrome::TITLE_BAR_H;
use crate::components::fields::field_input;
use crate::site_login::{SiteLoginGroup, SiteLoginRecord, login_title};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{IconName, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn render(
    search: &Entity<InputState>,
    groups: &[(SiteLoginGroup, Vec<&SiteLoginRecord>)],
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
        .w(px(320.))
        .h_full()
        .flex_shrink_0()
        .border_r_1()
        .border_color(theme.border)
        .pt(px(TITLE_BAR_H))
        .px(px(8.))
        .pb(px(8.))
        .child(
            h_flex()
                .id("settings-logins-search-row")
                .w_full()
                .gap(px(6.))
                .items_center()
                .child(
                    div()
                        .id("settings-logins-search")
                        .flex_1()
                        .min_w(px(0.))
                        .child(field_input(search).cleanable(true)),
                )
                .child(
                    Button::new("settings-login-add")
                        .icon(IconName::Plus)
                        .ghost()
                        .tooltip("New login")
                        .on_click({
                            let app = app.clone();
                            move |_, _, cx| {
                                app.update(cx, |state, cx| state.open_site_login_add(cx));
                            }
                        }),
                ),
        )
        .when_some(notice, |this, notice| {
            this.child(
                div()
                    .id("settings-logins-notice")
                    .pt(px(6.))
                    .px(px(8.))
                    .text_xs()
                    .text_color(muted)
                    .child(notice),
            )
        })
        .when_some(error, |this, error| {
            this.child(
                div()
                    .id("settings-logins-error")
                    .pt(px(6.))
                    .px(px(8.))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(error),
            )
        })
        .child(
            v_flex()
                .id("settings-logins-rows")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .mt(px(6.))
                .when(groups.iter().all(|(_, rows)| rows.is_empty()), |this| {
                    this.child(
                        div()
                            .id("settings-logins-empty")
                            .px(px(8.))
                            .py(px(8.))
                            .text_sm()
                            .text_color(muted)
                            .child(if any_rows {
                                "No logins match."
                            } else {
                                "No saved logins yet."
                            }),
                    )
                })
                .children(groups.iter().map(|(group, members)| {
                    section(*group, members, selected, icons, theme, app.clone())
                })),
        )
        .child(
            Button::new("settings-login-import")
                .label("Import…")
                .outline()
                .small()
                .w_full()
                .mt(px(8.))
                .tooltip("A CSV from the Passwords app, Safari, Chrome or 1Password.")
                .on_click(move |_, _, cx| {
                    app.update(cx, |state, cx| state.pick_site_logins_import(cx));
                }),
        )
}

/// One section: its name and count on a header row, then its rows. The section carries
/// the id, so a row listed under two sections is two elements.
fn section(
    group: SiteLoginGroup,
    members: &[&SiteLoginRecord],
    selected: Option<&str>,
    icons: &HashMap<String, Option<Arc<Image>>>,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let muted = theme.muted_foreground;
    v_flex()
        .id(SharedString::from(format!(
            "settings-logins-group-{}",
            group.id()
        )))
        .w_full()
        .child(
            h_flex()
                .w_full()
                .pt(px(14.))
                .pb(px(4.))
                .px(px(8.))
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(muted)
                        .child(group.title()),
                )
                .child(
                    div()
                        .px(px(7.))
                        .py(px(1.))
                        .rounded_full()
                        .bg(theme.muted)
                        .text_xs()
                        .text_color(muted)
                        .child(members.len().to_string()),
                ),
        )
        .children(members.iter().map(|row| {
            let icon = icons.get(&row.origin).and_then(|icon| icon.as_ref());
            row_element(
                row,
                selected == Some(row.id.as_str()),
                icon,
                theme,
                app.clone(),
            )
        }))
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
        .py(px(8.))
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
