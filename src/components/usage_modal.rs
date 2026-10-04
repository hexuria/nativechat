//! The Usage modal: what a Bot used, per model, in a window of the person's choosing, opened by Show
//! on the Usage card in the Bot's settings (hexuria/nativechat#174).
//!
//! It is centred over the whole window, as the routine Delete question is (`computer`), because
//! it is a look at the Bot's use and not part of the settings pane: the window dimmed under it, ✕,
//! Escape or a press beside it shut it. Over it three chips, 24h, 7d and Month, each ask the server
//! for that window (`GET /coworkers/{id}/usage?window=`); then only the models that answered a
//! request, each with its requests and what the paid keys charged for them; a "Total (paid keys)"
//! row under them that adds up the paid keys' charges, a model on the person's own subscription
//! being priced at nothing and adding nothing; and the note that replies on that subscription are
//! not counted here at all.
//!
//! What it says is [`usage_body`]'s, which the card's summary line and the gpui-agent tree read
//! too, so the three never disagree.
//!
//! A view of its own, mounted by the root as the Delete question is, so that it can hold focus while
//! it is open: Escape is bound in its own key context, which is only in the dispatch path while it
//! has focus, so it takes focus the moment it opens.

use crate::components::agent_settings::{UsageBody, usage_body};
use crate::components::reply_source::PLAN_USAGE_NOTE;
use crate::opengrok::UsageWindow;
use crate::state::AppState;
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// The modal, and its parts, as the gpui-agent tree names them.
pub(crate) const MODAL: &str = "usage-modal";
pub(crate) const CLOSE: &str = "usage-close";
/// The line that stands in the rows' place while the server is asked, would not say, or has
/// nothing for the window.
pub(crate) const STATUS: &str = "usage-status";
pub(crate) const TOTAL: &str = "usage-total";
pub(crate) const NOTE: &str = "usage-note";

/// What the total row is called: the paid keys' charges only.
pub(crate) const TOTAL_LABEL: &str = "Total (paid keys)";
/// In the rows' place for a window in which no model answered a request.
pub(crate) const NO_REQUESTS: &str = "No requests in this window.";
/// The modal's title.
pub(crate) const TITLE: &str = "Usage";

/// A window's chip, by the server's word for it: `usage-window-24h`, `usage-window-7d`,
/// `usage-window-month`.
pub(crate) fn window_id(window: UsageWindow) -> String {
    format!("usage-window-{}", window.word())
}

/// A model's row, by its place in the list: `usage-row-0`.
pub(crate) fn row_id(at: usize) -> String {
    format!("usage-row-{at}")
}

/// What the status line says for a body that has no rows, or nothing at all for one that has.
pub(crate) fn status_words(body: &UsageBody) -> Option<String> {
    match body {
        UsageBody::Asking => Some("Asking the server…".to_string()),
        UsageBody::Said(words) => Some(words.clone()),
        UsageBody::Empty => Some(NO_REQUESTS.to_string()),
        UsageBody::Rows { .. } => None,
    }
}

/// The widest the rows grow before they scroll in their box: a Bot can use a great many models.
const ROWS_MAX_HEIGHT: f32 = 320.;

pub struct UsageModalView {
    state: Entity<AppState>,
    focus_handle: FocusHandle,
    was_open: bool,
    pending_focus: bool,
}

impl UsageModalView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let open = state.read(cx).usage_modal.is_some();
        cx.observe(&state, |this, state, cx| {
            let open = state.read(cx).usage_modal.is_some();
            if open && !this.was_open {
                this.pending_focus = true;
            }
            this.was_open = open;
            cx.notify();
        })
        .detach();
        Self {
            state,
            focus_handle: cx.focus_handle(),
            was_open: open,
            pending_focus: open,
        }
    }
}

impl Render for UsageModalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_focus {
            self.pending_focus = false;
            self.focus_handle.focus(window, cx);
        }
        let Some(modal) = self.state.read(cx).usage_modal.clone() else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let body = usage_body(&modal.report);
        let close = {
            let app = self.state.clone();
            move |cx: &mut App| app.update(cx, |state, cx| state.close_usage_modal(cx))
        };
        let chips = h_flex()
            .gap(px(6.))
            .children(UsageWindow::ALL.into_iter().map(|window| {
                let id = window_id(window);
                let app = self.state.clone();
                div()
                    .id(SharedString::from(id.clone()))
                    .debug_selector(move || id)
                    .px(px(11.))
                    .py(px(5.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(0x808080).opacity(0.3))
                    .when(modal.window == window, |this| {
                        this.bg(rgb(0x808080).opacity(0.18))
                    })
                    .text_xs()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        app.update(cx, |state, cx| state.set_usage_window(window, cx));
                    })
                    .child(window.label())
            }));
        let list = match &body {
            UsageBody::Rows { rows, total } => v_flex()
                .gap(px(8.))
                .child(
                    v_flex()
                        .id("usage-rows")
                        .max_h(px(ROWS_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .gap(px(6.))
                        .children(rows.iter().enumerate().map(|(at, row)| {
                            let id = row_id(at);
                            h_flex()
                                .id(SharedString::from(id.clone()))
                                .debug_selector(move || id)
                                .justify_between()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .text_sm()
                                        .truncate()
                                        .child(row.model.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(row.detail()),
                                )
                        })),
                )
                .child(
                    h_flex()
                        .id(TOTAL)
                        .debug_selector(|| TOTAL.into())
                        .justify_between()
                        .gap(px(10.))
                        .pt(px(8.))
                        .border_t_1()
                        .border_color(theme.border)
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(TOTAL_LABEL)
                        .child(total.clone()),
                )
                .into_any_element(),
            other => div()
                .id(STATUS)
                .debug_selector(|| STATUS.into())
                .text_sm()
                .text_color(muted)
                .child(status_words(other).unwrap_or_default())
                .into_any_element(),
        };
        div()
            .id("usage-modal-overlay")
            .track_focus(&self.focus_handle)
            .key_context("UsageModal")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.32))
            .on_mouse_down(MouseButton::Left, {
                let close = close.clone();
                move |_, _, cx| close(cx)
            })
            .on_action({
                let close = close.clone();
                move |_: &crate::actions::CloseUsageModal, _: &mut Window, cx: &mut App| close(cx)
            })
            .child(
                v_flex()
                    .id(MODAL)
                    .debug_selector(|| MODAL.into())
                    .w(px(440.))
                    .bg(theme.popover)
                    .text_color(theme.foreground)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(14.))
                    .shadow_lg()
                    .px(px(20.))
                    .py(px(18.))
                    .gap(px(12.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(TITLE),
                            )
                            .child(
                                div()
                                    .id(CLOSE)
                                    .debug_selector(|| CLOSE.into())
                                    .size(px(28.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(0x777777).opacity(0.2)))
                                    .on_mouse_down(MouseButton::Left, {
                                        let close = close.clone();
                                        move |_, _, cx| {
                                            cx.stop_propagation();
                                            close(cx);
                                        }
                                    })
                                    .child(
                                        Icon::new(IconName::Close).size(px(14.)).text_color(muted),
                                    ),
                            ),
                    )
                    .child(chips)
                    .child(list)
                    .child(
                        div()
                            .id(NOTE)
                            .debug_selector(|| NOTE.into())
                            .text_xs()
                            .text_color(muted)
                            .child(PLAN_USAGE_NOTE),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::UsageModalView;
    use crate::opengrok::{CoworkerUsage, ModelUsage, OpenGrokClient, UsageWindow};
    use crate::state::{AppState, UsageModal, UsageReport};
    use gpui_kit::{
        Entity, KeyBinding, Modifiers, MouseButton, VisualTestContext, point, px, size,
    };

    /// A week of a bot's use: three models answered, one on the person's own subscription, which
    /// the server prices at nothing, and a fourth was asked and answered none.
    fn week() -> CoworkerUsage {
        let model = |id: &str, requests: i64, cost: &str| ModelUsage {
            model_id: id.into(),
            requests,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd: cost.into(),
        };
        CoworkerUsage {
            metered: true,
            note: None,
            window: "7d".into(),
            models: vec![
                model("oag/cheap", 12, "0.400000"),
                model("gpt-6-luna", 5, "0.000000"),
                model("xai/grok-4.7", 1, "0.020000"),
                model("oag/lost", 0, "9.990000"),
            ],
            totals: Default::default(),
        }
    }

    /// A runtime that nothing drives: a request begun while it is entered never answers.
    fn undriven_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
    }

    /// The modal in a window of its own, open on Ada's week, over a state whose server never
    /// answers.
    fn open_modal(cx: &mut gpui_kit::TestAppContext) -> (Entity<AppState>, &mut VisualTestContext) {
        use gpui_kit::AppContext as _;
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new(
                "escape",
                crate::actions::CloseUsageModal,
                Some("UsageModal"),
            )])
        });
        let mut app = AppState::new();
        app.opengrok = Some(OpenGrokClient::new("http://127.0.0.1:9").expect("a URL"));
        app.coworkers = vec![
            serde_json::from_value(serde_json::json!({"id": "cw_1", "name": "Ada"}))
                .expect("a row"),
        ];
        app.active_coworker_id = Some("cw_1".into());
        app.usage_modal = Some(UsageModal {
            coworker_id: "cw_1".into(),
            window: UsageWindow::Week,
            report: UsageReport::Read(week()),
        });
        let state = cx.new(|_| app);
        let (_, cx) = cx.add_window_view({
            let state = state.clone();
            move |_, cx| UsageModalView::new(state, cx)
        });
        cx.simulate_resize(size(px(1000.), px(800.)));
        draw(cx);
        (state, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn at(cx: &mut VisualTestContext, id: &'static str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
        cx.debug_bounds(id)
            .unwrap_or_else(|| panic!("`{id}` is drawn"))
    }

    fn click(cx: &mut VisualTestContext, at: gpui_kit::Point<gpui_kit::Pixels>) {
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        draw(cx);
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        draw(cx);
    }

    /// The modal is centred over the window and holds, top to bottom, ✕, the three chips, a row for
    /// each model that answered, the total of the paid keys, and the note.
    #[gpui_kit::test]
    fn the_modal_draws_its_chips_rows_total_and_note_in_order(cx: &mut gpui_kit::TestAppContext) {
        let runtime = undriven_runtime();
        let _enter = runtime.enter();
        let (_, cx) = open_modal(cx);
        let modal = cx.debug_bounds("usage-modal").expect("the modal is drawn");
        assert!(
            (modal.center().x - px(500.)).abs() < px(1.)
                && (modal.center().y - px(400.)).abs() < px(1.),
            "centred on the window: {modal:?}"
        );
        let close = at(cx, "usage-close");
        let chips: Vec<_> = ["usage-window-24h", "usage-window-7d", "usage-window-month"]
            .into_iter()
            .map(|id| at(cx, id))
            .collect();
        assert!(
            chips
                .windows(2)
                .all(|pair| pair[0].right() <= pair[1].left())
        );
        let rows: Vec<_> = ["usage-row-0", "usage-row-1", "usage-row-2"]
            .into_iter()
            .map(|id| at(cx, id))
            .collect();
        assert!(
            cx.debug_bounds("usage-row-3").is_none(),
            "the model that answered nothing is not listed"
        );
        let (total, note) = (at(cx, "usage-total"), at(cx, "usage-note"));
        assert!(
            close.bottom() <= chips[0].top() + px(1.),
            "✕ over the chips"
        );
        assert!(chips[0].bottom() <= rows[0].top());
        assert!(
            rows.windows(2)
                .all(|pair| pair[0].bottom() <= pair[1].top())
        );
        assert!(rows[2].bottom() <= total.top());
        assert!(total.bottom() <= note.top());
        for part in [close, chips[0], rows[0], total, note] {
            assert!(
                part.left() >= modal.left() && part.right() <= modal.right(),
                "{part:?} is inside {modal:?}"
            );
        }
    }

    /// A chip asks for its window, and the modal says it is asking until the answer comes; ✕ and a
    /// press beside the modal shut it, and a press in it does not.
    #[gpui_kit::test]
    fn a_chip_asks_for_its_window_and_close_and_a_press_beside_shut_the_modal(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let runtime = undriven_runtime();
        let _enter = runtime.enter();
        let (state, cx) = open_modal(cx);
        let center = |cx: &mut VisualTestContext, id: &'static str| at(cx, id).center();
        let shown = |cx: &mut VisualTestContext| {
            state.read_with(cx, |state, _| {
                state
                    .usage_modal
                    .as_ref()
                    .map(|modal| (modal.window, modal.report.clone()))
            })
        };
        let at = center(cx, "usage-window-24h");
        click(cx, at);
        assert_eq!(
            shown(cx),
            Some((UsageWindow::Day, UsageReport::Loading)),
            "the day is being asked for"
        );
        assert!(
            cx.debug_bounds("usage-status").is_some() && cx.debug_bounds("usage-total").is_none(),
            "asking, with no rows of another window under its chip"
        );
        let in_the_modal = center(cx, "usage-note");
        click(cx, in_the_modal);
        assert!(shown(cx).is_some(), "a press in the modal leaves it");
        let at = center(cx, "usage-close");
        click(cx, at);
        assert_eq!(shown(cx), None, "✕ shuts it");
        // Run again, and shut it with a press beside it.
        state.update(cx, |state, cx| state.open_usage_modal(cx));
        draw(cx);
        assert!(cx.debug_bounds("usage-modal").is_some());
        click(cx, point(px(8.), px(8.)));
        assert_eq!(shown(cx), None, "a press beside it shuts it");
        cx.run_until_parked();
    }

    /// Escape shuts the modal, which holds focus from the moment it opens.
    #[gpui_kit::test]
    fn escape_shuts_the_modal(cx: &mut gpui_kit::TestAppContext) {
        let runtime = undriven_runtime();
        let _enter = runtime.enter();
        let (state, cx) = open_modal(cx);
        assert!(state.read_with(cx, |state, _| state.usage_modal.is_some()));
        cx.simulate_keystrokes("escape");
        assert!(state.read_with(cx, |state, _| state.usage_modal.is_none()));
    }
}
