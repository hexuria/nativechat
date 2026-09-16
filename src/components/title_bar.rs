//! The window's title bar. The system one is transparent (see main.rs) and this row is
//! painted in its place: the traffic lights' corner, then, as in Grok Bot, the chat's header
//! over the chat column and the right pane's header over its column, each as wide as the
//! column below it. Whatever in it is not a control drags the window.

use crate::chrome::{
    HEADER_PX, INFO_PANE_WIDTH, TITLE_BAR_H, TITLE_BAR_LEFT_PAD, chrome_floats, sidebar_width,
};
use crate::components::agent_settings::settings_header;
use crate::components::chat::ChatView;
use crate::components::computer::ComputerPane;
use crate::components::persona::PersonaMark;
use crate::state::{AppState, RightPane};
use gpui_kit::component::{ActiveTheme, Icon, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// A run of the bar with no control in it: a handle to drag the window by.
pub fn window_drag(el: Div) -> Div {
    el.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

pub struct TitleBar {
    state: Entity<AppState>,
    chat: Entity<ChatView>,
    computer: Entity<ComputerPane>,
}

impl TitleBar {
    pub fn new(
        state: Entity<AppState>,
        chat: Entity<ChatView>,
        computer: Entity<ComputerPane>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The bar shows the bot, the panes and the chat's find bar: it follows the app state
        // and the chat, whose find state is its own.
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        cx.observe(&chat, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            chat,
            computer,
        }
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let floats = chrome_floats(f32::from(window.viewport_size().width));
        let state = self.state.read(cx);
        let signed_in = state.is_signed_in();
        let left = sidebar_width(
            state.sidebar_hidden,
            state.sidebar_collapsed,
            state.sidebar_expanded_width,
        );
        let right_pane = state.right_pane;
        let coworker = state
            .active_coworker_id
            .as_ref()
            .and_then(|id| state.coworkers.iter().find(|c| &c.id == id))
            .map(|c| {
                let name = c.name.trim();
                (
                    c.id.clone(),
                    if name.is_empty() {
                        "Bot".to_string()
                    } else {
                        name.to_string()
                    },
                    c.avatar_shape.clone(),
                    c.avatar_color.clone(),
                )
            });
        // The spans line up with the columns below. The chat's starts where the sidebar
        // ends, or past the traffic lights when the sidebar is its rail or gone. A floating
        // sidebar or pane (a narrow window) keeps its header to itself, over the chat.
        let docked = signed_in && !floats;
        let chat_x = if docked {
            left.max(TITLE_BAR_LEFT_PAD)
        } else {
            TITLE_BAR_LEFT_PAD
        };
        let right_header = match right_pane {
            RightPane::Settings if docked => {
                Some(settings_header(self.state.clone(), true).into_any_element())
            }
            RightPane::Computer if docked => Some(self.computer.read(cx).header(cx, true)),
            _ => None,
        };
        let find_bar = if signed_in {
            self.chat.read(cx).find_bar(self.chat.clone(), cx)
        } else {
            None
        };

        let app = self.state.clone();
        let chat_span = h_flex()
            .id("chat-header")
            .flex_1()
            .min_w_0()
            .h_full()
            .px(px(HEADER_PX))
            .items_center()
            .gap_2()
            .map(|this| match (&coworker, signed_in) {
                (Some((id, name, shape, color)), _) => this.child(
                    div()
                        .id("header-coworker")
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, {
                            let app = app.clone();
                            move |_, _, cx| {
                                app.update(cx, |state, cx| state.toggle_agent_settings(cx));
                            }
                        })
                        .child(
                            PersonaMark::new(id.clone())
                                .shape(shape.clone())
                                .color(color.clone())
                                .size(px(24.))
                                .dark(theme.is_dark()),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(name.clone()),
                        ),
                ),
                // No bot: the page's title, in the same place and style.
                (None, true) => this.child(window_drag(
                    div()
                        .h_full()
                        .flex()
                        .items_center()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Bots"),
                )),
                (None, false) => this,
            })
            .child(window_drag(div().flex_1().h_full()))
            .when(coworker.is_some(), |this| {
                this.child(
                    h_flex().gap_2().items_center().children(find_bar).child(
                        div()
                            .id("header-monitor")
                            .size(px(28.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                            .on_mouse_down(MouseButton::Left, {
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| state.toggle_computer_pane(cx));
                                }
                            })
                            .child(Icon::default().path("icons/monitor.svg").size(px(16.))),
                    ),
                )
            });

        h_flex()
            .id("main-window-header")
            .w_full()
            .h(px(TITLE_BAR_H))
            .flex_shrink_0()
            .items_center()
            .bg(theme.background)
            .text_color(theme.foreground)
            .border_b_1()
            .border_color(theme.border)
            // The traffic lights' corner and the sidebar's span: nothing but a handle.
            .child(window_drag(div().w(px(chat_x)).h_full().flex_shrink_0()))
            .child(chat_span)
            .when_some(right_header, |this, header| {
                this.child(
                    div()
                        .w(px(INFO_PANE_WIDTH))
                        .h_full()
                        .flex_shrink_0()
                        .border_l_1()
                        .border_color(theme.border)
                        .child(header),
                )
            })
    }
}
