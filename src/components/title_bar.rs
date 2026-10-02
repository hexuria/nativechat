//! Floating chat chrome. The transcript continues behind the theme-coloured fade;
//! only the pill and icon buttons have a surface. Other pages retain their own header.

use crate::chrome::{
    CONTROL_ICON_PX, HEADER_PX, INFO_PANE_WIDTH, TITLE_BAR_H, TITLE_BAR_LEFT_PAD, chrome_floats,
    header_sidebar_toggle_visible, sidebar_width,
};
use crate::components::agent_settings::settings_header;
use crate::components::chat::ChatView;
use crate::components::computer::ComputerPane;
use crate::components::persona::PersonaMark;
use crate::components::recipes::{RecipesView, recipes_header};
use crate::components::sidebar::sidebar_toggle_button;
use crate::state::{AppState, MainPage, RightPane};
use gpui_kit::component::{ActiveTheme, Icon, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// A run of the bar with no control in it: a handle to drag the window by.
pub fn window_drag(el: Div) -> Div {
    el.on_mouse_down(MouseButton::Left, |_, window, _| move_window(window))
}

/// Hands the press being handled to the system, which moves the window with the pointer until
/// the button comes up.
///
/// The test platform has no window to move, and its `start_window_move` is `unimplemented!`.
/// Under test the press is counted instead, which is how a test tells the parts of the chrome
/// that drag the window from the parts that leave a press to whatever is under them.
fn move_window(window: &Window) {
    #[cfg(test)]
    let _ = window;
    #[cfg(test)]
    WINDOW_MOVES.set(WINDOW_MOVES.get() + 1);
    #[cfg(not(test))]
    window.start_window_move();
}

#[cfg(test)]
thread_local! {
    /// The presses on this thread that would have moved the window (`move_window`).
    static WINDOW_MOVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub struct TitleBar {
    state: Entity<AppState>,
    chat: Entity<ChatView>,
    computer: Entity<ComputerPane>,
    /// The Recipes page, for the header it draws in this bar: the share icon there opens the
    /// page's own modal, which is the page's to hold and not the app's.
    recipes: Entity<RecipesView>,
}

impl TitleBar {
    pub fn new(
        state: Entity<AppState>,
        chat: Entity<ChatView>,
        computer: Entity<ComputerPane>,
        recipes: Entity<RecipesView>,
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
            recipes,
        }
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        if state.is_signed_in() && state.page == MainPage::Chat && !state.is_app_settings_open {
            return self.floating_header(window, cx);
        }
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
            .filter(|_| !state.is_app_settings_open)
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
        // A routine's thread says so beside the bot's name, with the way back to the bot's own
        // chat: it is not in the sidebar, so the header is where a person finds where they are.
        let routine_thread = state
            .active_thread_origin()
            .filter(|_| !state.is_app_settings_open)
            .map(|origin| origin.routine_name.clone());
        // The spans line up with the columns below. The chat's starts where the sidebar
        // ends, or past the traffic lights when the sidebar is its rail or gone. A floating
        // sidebar or pane (a narrow window) keeps its header to itself, over the chat.
        let docked = signed_in && !floats;
        // Whether there is a sidebar under that span at all. `docked` only says the chrome is
        // not floating; the sidebar can still be hidden, and then the span over it is just the
        // traffic lights' corner with the chat behind it.
        let sidebar_shown = docked && left > 0.0;
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

        // The Recipes page owns the header row while it is open: its title and back chevron
        // stand where the coworker's name stands in a chat.
        let page = state.page;
        // The full settings page covers the window, chrome and all. The bot it happened to be
        // opened from is not what the person is looking at, so the bar does not name it there.
        let on_a_page = page != MainPage::Chat || state.is_app_settings_open;
        let app = self.state.clone();
        let on_recipes = signed_in && page == MainPage::Recipes;
        let recipes_span =
            on_recipes.then(|| recipes_header(self.state.clone(), &self.recipes, &theme, cx));
        let chat_span = h_flex()
            .id("chat-header")
            .flex_1()
            .min_w_0()
            .h_full()
            .px(px(HEADER_PX))
            .items_center()
            .gap_2()
            // THE RULE UNDER THE BAR BELONGS TO THE CHAT COLUMN ALONE. It marks where the
            // header ends and the transcript begins, and neither neighbour has that boundary:
            // the sidebar runs unbroken from the window's top edge to the account row, and the
            // right pane's header flows straight into its own fields. Drawn across the whole
            // row it cut both of them in half for no reason a reader could name.
            .border_b_1()
            .border_color(theme.border)
            .map(|this| match (recipes_span, &coworker, signed_in) {
                // The Recipes page's own title and back chevron, in place of a bot's name.
                (Some(header), _, _) => this.child(header),
                (None, Some((id, name, shape, color)), _) => this
                    .child(
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
                    )
                    .when_some(routine_thread.clone(), |this, routine| {
                        this.child(
                            div()
                                .id("header-routine-thread")
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(6.))
                                .bg(rgb(0x777777).opacity(0.17))
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("Routine · {routine}")),
                        )
                        .child(
                            div()
                                .id("header-routine-back")
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme.foreground))
                                .on_mouse_down(MouseButton::Left, {
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| state.back_to_bot_chat(cx));
                                    }
                                })
                                .child(format!("Back to {name}")),
                        )
                    }),
                // No bot: the page's title, in the same place and style.
                (None, None, true) => this.child(window_drag(
                    div()
                        .h_full()
                        .flex()
                        .items_center()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Bots"),
                )),
                (None, None, false) => this,
            })
            // The Recipes page's header keeps the whole span: its share icon belongs at the
            // far right of the bar, and a filler beside it would leave the icon mid-window.
            // The page's own header carries the handle to drag the window by instead.
            .when(!on_recipes, |this| {
                this.child(window_drag(div().flex_1().h_full()))
            })
            // The find bar and the screen button act on the chat; another page has no chat, and
            // drawing them there pushed them out of the span and over the right pane's header.
            .when(coworker.is_some() && !on_a_page, |this| {
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
                            .child(
                                Icon::default()
                                    .path("icons/monitor.svg")
                                    .size(px(CONTROL_ICON_PX)),
                            ),
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
            // EACH SPAN WEARS THE COLUMN'S OWN FILL. A bar painted one colour end to end put a
            // white band above a grey sidebar and a white band above a grey pane, so both read
            // as starting below the chrome instead of at the window's top edge. Only the chat
            // is the same colour as the bar, because the bar IS the chat's header.
            .child(
                window_drag(div().w(px(chat_x)).h_full().flex_shrink_0()).map(|this| {
                    if sidebar_shown {
                        // The sidebar's own edge, carried up through the bar so the column
                        // reads as one piece from the window's top to the account row.
                        this.bg(theme.sidebar)
                            .border_r_1()
                            .border_color(theme.border)
                    } else {
                        // No sidebar under it, so no edge to continue and nothing to tint.
                        // The chat's rule runs on under the traffic lights instead, out to
                        // the window's edge, rather than stopping short of it.
                        this.border_b_1().border_color(theme.border)
                    }
                }),
            )
            .child(chat_span)
            .when_some(right_header, |this, header| {
                this.child(
                    div()
                        .w(px(INFO_PANE_WIDTH))
                        .h_full()
                        .flex_shrink_0()
                        .bg(theme.sidebar)
                        .border_l_1()
                        .border_color(theme.border)
                        .child(header),
                )
            })
            .into_any_element()
    }
}

impl TitleBar {
    fn floating_header(&self, window: &mut Window, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let state = self.state.read(cx);
        let width = f32::from(window.viewport_size().width);
        let left = sidebar_width(
            state.sidebar_hidden,
            state.sidebar_collapsed,
            state.sidebar_expanded_width,
        );
        let (chat_left, chat_right, pill_width) =
            floating_header_span(width, left, state.right_pane != RightPane::Closed);
        let bot = state
            .active_coworker_id
            .as_ref()
            .and_then(|id| state.coworkers.iter().find(|bot| &bot.id == id));
        let app = self.state.clone();
        let find_bar = self.chat.read(cx).find_bar(self.chat.clone(), cx);

        div()
            .id("main-window-header")
            .w_full()
            .h(px(TITLE_BAR_H))
            .relative()
            .text_color(theme.foreground)
            // THE FADE IS PAINT AND NOTHING ELSE. GPUI gives a press to an occluding element
            // wherever it is, painted or clear, and the fade's last 16px hang below the bar over
            // the transcript, where the gradient has run out. A fade that occluded and dragged
            // the window took the presses meant for the images, links and text under it, so
            // only the pill and the icon buttons take a press here.
            .child(
                div()
                    .debug_selector(|| "header-fade".into())
                    .absolute()
                    .left(px(chat_left))
                    .right(px(chat_right))
                    .top_0()
                    .h(px(TITLE_BAR_H + 16.))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(theme.background.opacity(0.92), 0.),
                        linear_color_stop(theme.background.opacity(0.), 1.),
                    )),
            )
            .child(
                div()
                    .id("chat-header")
                    .absolute()
                    .left(px(chat_left))
                    .right(px(chat_right))
                    .top_0()
                    .h(px(TITLE_BAR_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("header-coworker")
                            .occlude()
                            .max_w(px(pill_width))
                            .min_w_0()
                            .h(px(36.))
                            .px(px(12.))
                            .rounded_full()
                            .bg(theme.secondary.opacity(0.94))
                            .border_1()
                            .border_color(theme.border.opacity(0.5))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .when_some(bot, |this, bot| {
                                let name = if bot.name.trim().is_empty() {
                                    "Bot"
                                } else {
                                    bot.name.trim()
                                };
                                this.cursor_pointer()
                                    .hover(|s| s.bg(theme.secondary))
                                    .on_mouse_down(MouseButton::Left, {
                                        let app = app.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            app.update(cx, |state, cx| {
                                                state.toggle_agent_settings(cx)
                                            });
                                        }
                                    })
                                    .child(
                                        div().flex_shrink_0().child(
                                            PersonaMark::new(bot.id.clone())
                                                .shape(bot.avatar_shape.clone())
                                                .color(bot.avatar_color.clone())
                                                .size(px(24.))
                                                .dark(theme.is_dark()),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(name.to_owned()),
                                    )
                            })
                            .when(bot.is_none(), |this| this.child("Bots")),
                    ),
            )
            .when(
                header_sidebar_toggle_visible(state.sidebar_hidden),
                |this| {
                    this.child(
                        sidebar_toggle_button(
                            app.clone(),
                            "header-left-sidebar",
                            28.,
                            cx.theme().foreground,
                            true,
                        )
                        .absolute()
                        .left(px(TITLE_BAR_LEFT_PAD))
                        .top(px(12.)),
                    )
                },
            )
            .child(
                h_flex()
                    .absolute()
                    .right(px(HEADER_PX))
                    .top(px(12.))
                    .gap(px(8.))
                    .when(bot.is_some(), |this| {
                        this.child(
                            header_icon(
                                "header-monitor",
                                "icons/monitor.svg",
                                state.right_pane == RightPane::Computer,
                            )
                            .on_mouse_down(MouseButton::Left, {
                                let app = app.clone();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    app.update(cx, |state, cx| state.toggle_computer_pane(cx));
                                }
                            }),
                        )
                    })
                    .child(
                        header_icon(
                            "header-right-sidebar",
                            "icons/panel-right.svg",
                            state.right_pane != RightPane::Closed,
                        )
                        .on_mouse_down(MouseButton::Left, {
                            let app = app.clone();
                            move |_, _, cx| {
                                cx.stop_propagation();
                                app.update(cx, |state, cx| {
                                    if state.right_pane == RightPane::Closed {
                                        state.toggle_agent_settings(cx);
                                    } else {
                                        state.close_right_pane(cx);
                                    }
                                });
                            }
                        }),
                    ),
            )
            .when_some(state.active_thread_origin(), |this, origin| {
                let name = bot
                    .map(|bot| bot.name.clone())
                    .unwrap_or_else(|| "Bot".into());
                this.child(
                    h_flex()
                        .absolute()
                        .left(px(chat_left + HEADER_PX))
                        .top(px(TITLE_BAR_H))
                        .occlude()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded(px(8.))
                        .bg(theme.background.opacity(0.95))
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(
                            div()
                                .id("header-routine-thread")
                                .child(format!("Routine · {}", origin.routine_name)),
                        )
                        .child(
                            div()
                                .id("header-routine-back")
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme.foreground))
                                .on_mouse_down(MouseButton::Left, {
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        cx.stop_propagation();
                                        app.update(cx, |state, cx| state.back_to_bot_chat(cx));
                                    }
                                })
                                .child(format!("Back to {name}")),
                        ),
                )
            })
            .when_some(find_bar, |this, find| {
                this.child(
                    div()
                        .absolute()
                        .right(px(chat_right + HEADER_PX))
                        .top(px(TITLE_BAR_H))
                        .occlude()
                        .rounded(px(8.))
                        .bg(theme.background)
                        .child(find),
                )
            })
            .into_any_element()
    }
}

fn header_icon(id: &'static str, path: &'static str, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .occlude()
        .size(px(28.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(selected, |this| this.bg(rgb(0x777777).opacity(0.12)))
        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
        .child(Icon::default().path(path).size(px(CONTROL_ICON_PX)))
}

/// Floating panes don't move the chat's centre. Docked panes do. Limit long names
/// without moving the pill off-centre or covering the traffic lights/icon clusters.
fn floating_header_span(width: f32, left: f32, right_open: bool) -> (f32, f32, f32) {
    let floats = chrome_floats(width);
    let left = if floats { 0. } else { left };
    let right = if !floats && right_open {
        INFO_PANE_WIDTH
    } else {
        0.
    };
    let centre = (left + width - right) / 2.;
    let half = (centre - (TITLE_BAR_LEFT_PAD + 44.))
        .min(width - 88. - centre)
        .min((width - left - right) / 2. - HEADER_PX)
        .max(0.);
    (left, right, half * 2.)
}

#[cfg(test)]
mod tests {
    use super::floating_header_span;

    #[test]
    fn pill_tracks_chat_centre_for_all_sidebar_states() {
        for left in [0., 88., 280., 400.] {
            for right in [false, true] {
                let (start, end, max) = floating_header_span(1200., left, right);
                assert_eq!(start, left);
                assert_eq!(end, if right { 320. } else { 0. });
                let centre = (start + 1200. - end) / 2.;
                assert!(centre - max / 2. >= 124.);
                assert!(centre + max / 2. <= 1112.);
                assert!(max > 36.);
            }
        }
    }

    #[test]
    fn floating_panes_do_not_shift_pill() {
        for left in [0., 88., 280.] {
            for right in [false, true] {
                assert_eq!(floating_header_span(700., left, right), (0., 0., 452.));
            }
        }
    }

    /// The chat page as `Layout` paints it, pressed the way a person presses it: the chat, the
    /// right pane beside it while one is open, and the title bar over both in a slot
    /// `TITLE_BAR_H` tall that does not clip. The chat counts the presses that reach it.
    mod chat_page_chrome {
        use super::super::{TitleBar, WINDOW_MOVES};
        use crate::chrome::{INFO_PANE_WIDTH, TITLE_BAR_H};
        use crate::components::agent_settings::AgentSettings;
        use crate::components::chat::ChatView;
        use crate::components::computer::ComputerPane;
        use crate::components::recipes::RecipesView;
        use crate::state::{AppState, AuthStatus, RightPane};
        use gpui_kit::prelude::FluentBuilder as _;
        use gpui_kit::{
            AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Modifiers,
            MouseButton, ParentElement as _, Pixels, Point, Render, Styled as _, TestAppContext,
            VisualTestContext, Window, div, point, px, size,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        struct ChatPage {
            app: Entity<AppState>,
            title_bar: Entity<TitleBar>,
            settings: Entity<AgentSettings>,
            computer: Entity<ComputerPane>,
            chat_presses: Rc<Cell<usize>>,
        }

        impl Render for ChatPage {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                let presses = self.chat_presses.clone();
                let pane = match self.app.read(cx).right_pane {
                    RightPane::Settings => Some(self.settings.clone().into_any_element()),
                    RightPane::Computer => Some(self.computer.clone().into_any_element()),
                    RightPane::Closed => None,
                };
                div()
                    .size_full()
                    .relative()
                    .flex()
                    .child(div().id("chat").flex_1().h_full().on_mouse_down(
                        MouseButton::Left,
                        move |_, _, _| {
                            presses.set(presses.get() + 1);
                        },
                    ))
                    .when_some(pane, |this, pane| {
                        this.child(
                            div()
                                .w(px(INFO_PANE_WIDTH))
                                .h_full()
                                .flex_shrink_0()
                                .child(pane),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(TITLE_BAR_H))
                            .child(self.title_bar.clone()),
                    )
            }
        }

        /// A window `width` wide, signed in and open on Ada's chat.
        fn chat_page(
            cx: &mut TestAppContext,
            width: f32,
        ) -> (Entity<ChatPage>, &mut VisualTestContext) {
            cx.update(gpui_kit::init);
            let (view, cx) = cx.add_window_view(|window, cx| {
                let mut state = AppState::new();
                state.auth_status = AuthStatus::SignedIn;
                state.account = Some(
                    serde_json::from_value(
                        serde_json::json!({ "id": "acc_1", "email": "ada@example.com" }),
                    )
                    .expect("an account"),
                );
                state.coworkers = vec![
                    serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "Ada" }))
                        .expect("a bot"),
                ];
                state.active_coworker_id = Some("cw_1".into());
                let app = cx.new(|_| state);
                cx.observe(&app, |_, _, cx| cx.notify()).detach();
                let chat = cx.new(|cx| ChatView::new(window, app.clone(), cx));
                let settings = cx.new(|cx| AgentSettings::new(window, app.clone(), cx));
                let computer = cx.new(|cx| ComputerPane::new(window, app.clone(), cx));
                let recipes = cx.new(|cx| RecipesView::new(window, app.clone(), cx));
                let title_bar =
                    cx.new(|cx| TitleBar::new(app.clone(), chat, computer.clone(), recipes, cx));
                ChatPage {
                    app,
                    title_bar,
                    settings,
                    computer,
                    chat_presses: Rc::default(),
                }
            });
            cx.simulate_resize(size(px(width), px(640.)));
            cx.run_until_parked();
            (view, cx)
        }

        /// A click at `at`, after the pointer has moved there as a person's would.
        fn press(cx: &mut VisualTestContext, at: Point<Pixels>) {
            cx.simulate_mouse_move(at, None, Modifiers::none());
            cx.simulate_click(at, Modifiers::none());
        }

        /// The fade under the chat's header takes no press. One in the clear tail it hangs over
        /// the transcript below the bar, or in the bar beside the pill, reaches the chat under
        /// it and moves no window. The pill keeps a surface of its own.
        #[gpui_kit::test]
        fn a_press_in_the_fade_reaches_the_chat(cx: &mut TestAppContext) {
            let (view, cx) = chat_page(cx, 1200.);
            let fade = cx.debug_bounds("header-fade").expect("the fade is drawn");
            let presses = view.update(cx, |page, _| page.chat_presses.clone());
            let moves = WINDOW_MOVES.get();
            for (y, place) in [
                (px(TITLE_BAR_H / 2.), "in the bar beside the pill"),
                (fade.bottom() - px(4.), "in the clear tail below the bar"),
            ] {
                let before = presses.get();
                press(cx, point(fade.left() + px(40.), y));
                assert_eq!(presses.get(), before + 1, "a press {place} missed the chat");
                assert_eq!(
                    WINDOW_MOVES.get(),
                    moves,
                    "a press {place} moved the window"
                );
            }

            // The pill is centred on the chat's span, which is the fade's.
            let before = presses.get();
            press(cx, point(fade.center().x, px(TITLE_BAR_H / 2.)));
            assert_eq!(presses.get(), before, "the pill let its press through");
            assert_eq!(
                view.update(cx, |page, cx| page.app.read(cx).right_pane),
                RightPane::Settings,
                "the pill opens the bot's settings"
            );
        }
    }
}
