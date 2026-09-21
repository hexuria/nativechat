//! The left pane: the four tiles, and Import… / Add under them.

use crate::chrome::TITLE_BAR_H;
use crate::site_login::SiteLoginFilter;
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub(super) fn render(
    counts: &[(SiteLoginFilter, usize); 4],
    current: SiteLoginFilter,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    v_flex()
        .id("settings-logins-sidebar")
        .w(px(180.))
        .h_full()
        .flex_shrink_0()
        .border_r_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .pt(px(TITLE_BAR_H))
        .px(px(10.))
        .pb(px(14.))
        .gap(px(2.))
        .children(
            counts.iter().map(|(filter, count)| {
                tile(*filter, *count, *filter == current, theme, app.clone())
            }),
        )
        .child(div().flex_1())
        .child(
            v_flex()
                .w_full()
                .gap(px(6.))
                .child(
                    Button::new("settings-login-import")
                        .label("Import…")
                        .outline()
                        .small()
                        .w_full()
                        .tooltip("A CSV from the Passwords app, Safari, Chrome or 1Password.")
                        .on_click({
                            let app = app.clone();
                            move |_, _, cx| {
                                app.update(cx, |state, cx| state.pick_site_logins_import(cx));
                            }
                        }),
                )
                .child(
                    Button::new("settings-login-add")
                        .label("Add")
                        .primary()
                        .small()
                        .w_full()
                        .on_click(move |_, _, cx| {
                            app.update(cx, |state, cx| state.open_site_login_add(cx));
                        }),
                ),
        )
}

/// One tile: a coloured square with a glyph, the name, and how many rows it holds.
fn tile(
    filter: SiteLoginFilter,
    count: usize,
    selected: bool,
    theme: &Theme,
    app: Entity<AppState>,
) -> impl IntoElement {
    let (color, glyph) = match filter {
        SiteLoginFilter::All => (theme.blue, "icons/key.svg"),
        SiteLoginFilter::Passkeys => (theme.magenta, "icons/check.svg"),
        SiteLoginFilter::Codes => (theme.green, "icons/clock.svg"),
        SiteLoginFilter::Security => (theme.red, "icons/report.svg"),
    };
    h_flex()
        .id(SharedString::from(format!(
            "settings-logins-tile-{}",
            filter.id()
        )))
        .w_full()
        .px(px(8.))
        .py(px(6.))
        .gap(px(8.))
        .rounded(px(8.))
        .cursor_pointer()
        .when(selected, |this| this.bg(theme.list_active))
        .when(!selected, |this| this.hover(|s| s.bg(theme.list_hover)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            app.update(cx, |state, cx| state.set_site_login_filter(filter, cx));
        })
        .child(
            div()
                .size(px(24.))
                .flex_shrink_0()
                .rounded(px(6.))
                .bg(color)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::default()
                        .path(glyph)
                        .size(px(13.))
                        .text_color(gpui::white()),
                ),
        )
        .child(div().flex_1().min_w(px(0.)).text_sm().child(filter.title()))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(count.to_string()),
        )
}
