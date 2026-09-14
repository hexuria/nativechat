use crate::components::persona::PersonaMark;
use crate::state::AppState;
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub fn hidden_bots_overlay(app: Entity<AppState>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let state = app.read(cx);
    let hidden: Vec<_> = state
        .coworkers
        .iter()
        .filter(|c| state.hidden_coworker_ids.contains(&c.id))
        .cloned()
        .collect();
    let dark = theme.is_dark();
    let fg = theme.foreground;
    let muted = theme.muted_foreground;
    let hover = theme.secondary;
    let border = theme.border;
    let bg = theme.popover;

    div()
        .id("hidden-bots-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::black().opacity(0.32))
        .on_mouse_down(MouseButton::Left, {
            let app = app.clone();
            move |_, _, cx| {
                app.update(cx, |state, cx| state.close_hidden_bots(cx));
            }
        })
        .child(
            v_flex()
                .id("hidden-bots-dialog")
                .w(px(420.))
                .max_h(px(520.))
                .bg(bg)
                .text_color(fg)
                .border_1()
                .border_color(border)
                .rounded(px(14.))
                .shadow_lg()
                .overflow_hidden()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    v_flex()
                        .px(px(20.))
                        .pt(px(18.))
                        .pb(px(12.))
                        .gap(px(4.))
                        .child(
                            h_flex()
                                .w_full()
                                .items_start()
                                .justify_between()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Hidden Bots"),
                                )
                                .child(
                                    div()
                                        .id("hidden-bots-close")
                                        .size(px(28.))
                                        .rounded(px(8.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(hover))
                                        .child(
                                            Icon::new(IconName::Close)
                                                .size(px(14.))
                                                .text_color(muted),
                                        )
                                        .on_click({
                                            let app = app.clone();
                                            move |_, _, cx| {
                                                cx.stop_propagation();
                                                app.update(cx, |state, cx| {
                                                    state.close_hidden_bots(cx);
                                                });
                                            }
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child("Hidden Bots stay active and keep their history, they just don't show in the sidebar."),
                        ),
                )
                .child(div().h(px(1.)).w_full().bg(border))
                .child(
                    div()
                        .id("hidden-bots-list")
                        .flex_1()
                        .min_h_0()
                        .px(px(8.))
                        .py(px(8.))
                        .overflow_y_scroll()
                        .when(hidden.is_empty(), |this| {
                            this.child(
                                div()
                                    .py(px(40.))
                                    .text_center()
                                    .text_sm()
                                    .text_color(muted)
                                    .child("No hidden bots"),
                            )
                        })
                        .children(hidden.into_iter().map(|coworker| {
                            let id = coworker.id.clone();
                            let name = coworker.name.clone();
                            let app = app.clone();
                            h_flex()
                                .id(SharedString::from(format!("hidden-bot-{id}")))
                                .w_full()
                                .items_center()
                                .justify_between()
                                .gap(px(12.))
                                .px(px(8.))
                                .py(px(6.))
                                .min_h(px(46.))
                                .rounded(px(8.))
                                .hover(move |s| s.bg(hover))
                                .child(
                                    h_flex()
                                        .min_w_0()
                                        .flex_1()
                                        .items_center()
                                        .gap(px(10.))
                                        .child(
                                            PersonaMark::new(id.clone())
                                                .shape(coworker.avatar_shape.clone())
                                                .color(coworker.avatar_color.clone())
                                                .size(px(28.))
                                                .dark(dark),
                                        )
                                        .child(
                                            div()
                                                .min_w_0()
                                                .text_sm()
                                                .truncate()
                                                .child(name),
                                        ),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!("unhide-{id}")))
                                        .px(px(8.))
                                        .py(px(6.))
                                        .rounded(px(6.))
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(muted)
                                        .hover(move |s| s.bg(hover))
                                        .child("Unhide")
                                        .on_click({
                                            let app = app.clone();
                                            let id = id.clone();
                                            move |_, _, cx| {
                                                cx.stop_propagation();
                                                app.update(cx, |state, cx| {
                                                    state.unhide_coworker(id.clone(), cx);
                                                });
                                            }
                                        }),
                                )
                        })),
                ),
        )
}
