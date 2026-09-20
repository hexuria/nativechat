use std::rc::Rc;
use std::sync::Arc;

use crate::chrome::{
    BOX_SCREEN_ASPECT, HEADER_PX, INFO_PANE_WIDTH, TITLE_BAR_H, box_screen_height_for_width,
    chrome_floats, computer_pane_screen_width,
};
use crate::components::alert_chrome::{
    attention_cta, attention_ctas, attention_glass, attention_shadow,
};
use crate::components::fields::field_input;
use crate::opengrok::{
    BoxHandoffResolution, computer_attention_done_id, computer_attention_id,
    computer_attention_skip_id,
};
use crate::state::{
    AgentRoutine, AppState, ComputerView, NewTrigger, RoutineTrigger, ScheduleDayKind,
    ScheduleSpec, ScheduleUiMode, ScheduleUnit,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon, Selectable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct ComputerPane {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    instruction_input: Entity<TextareaState>,
    custom_cron: Entity<InputState>,
    loaded_editor: Option<Option<String>>,
    webhook_popover: Option<String>,
}

impl ComputerPane {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name this routine"));
        let instruction_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What should this routine do each time it runs?")
                .auto_grow(3, 8)
        });
        let custom_cron = cx.new(|cx| InputState::new(window, cx).placeholder("@every 1h"));
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        Self {
            state,
            name_input,
            instruction_input,
            custom_cron,
            loaded_editor: None,
            webhook_popover: None,
        }
    }

    fn sync_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.state.read(cx).computer_view.clone();
        let ComputerView::Editor { id } = view else {
            self.loaded_editor = None;
            return;
        };
        if self.loaded_editor.as_ref() == Some(&id) {
            return;
        }
        self.loaded_editor = Some(id.clone());
        let coworker = self.state.read(cx).active_coworker_id.clone();
        let routine = coworker.as_ref().and_then(|cid| {
            id.as_ref().and_then(|rid| {
                self.state
                    .read(cx)
                    .coworker_routines(cid)
                    .iter()
                    .find(|row| &row.id == rid)
                    .cloned()
            })
        });
        let name = routine.as_ref().map(|r| r.name.clone()).unwrap_or_default();
        let instruction = routine
            .as_ref()
            .map(|r| r.instruction.clone())
            .unwrap_or_default();
        self.name_input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
        });
        self.instruction_input.update(cx, |input, cx| {
            input.set_value(instruction, window, cx);
        });
        let cron = routine
            .as_ref()
            .and_then(|r| {
                r.triggers.iter().rev().find_map(|t| match t {
                    RoutineTrigger::Schedule { spec, .. }
                        if spec.mode == ScheduleUiMode::Custom =>
                    {
                        Some(spec.expr.clone())
                    }
                    _ => None,
                })
            })
            .unwrap_or_default();
        self.custom_cron.update(cx, |input, cx| {
            input.set_value(cron, window, cx);
        });
    }
}

impl Render for ComputerPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_editor(window, cx);
        let theme = cx.theme().clone();
        let muted = theme.muted_foreground;
        let app = self.state.clone();
        let (
            view,
            agent_name,
            coworker_id,
            box_id,
            routines,
            has_screen,
            screen,
            controls,
            attention,
        ) = {
            let state = self.state.read(cx);
            let coworker = state
                .active_coworker_id
                .as_ref()
                .and_then(|id| state.coworkers.iter().find(|c| &c.id == id));
            let name = coworker
                .map(|c| c.name.clone())
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "Bot".into());
            // The live box the status reports wins over the id frozen on the coworker's row.
            let box_id = state
                .coworker_computer
                .as_ref()
                .and_then(|status| status.box_id.clone())
                .or_else(|| coworker.and_then(|c| c.box_id.clone()));
            let controls = ComputerControls::from_state(state);
            let coworker_id = state.active_coworker_id.clone().unwrap_or_default();
            let routines = state.coworker_routines(&coworker_id).to_vec();
            let has_screen = state
                .coworker_computer
                .as_ref()
                .and_then(|status| status.vnc_url())
                .is_some();
            let attention = state
                .active_computer_handoff()
                .map(|spec| (spec.card_key().to_string(), spec.handoff_prompt()));
            (
                state.computer_view.clone(),
                name,
                coworker_id,
                box_id,
                routines,
                has_screen,
                state.coworker_screen.clone(),
                controls,
                attention,
            )
        };

        // Floating over the chat (a narrow window), the pane carries its own header; docked,
        // the title bar shows it over the pane.
        let floats = chrome_floats(f32::from(window.viewport_size().width));
        let floating_header = if floats {
            Some(self.header(cx, false))
        } else {
            None
        };
        v_flex()
            .id("computer-pane")
            .h_full()
            .w(px(INFO_PANE_WIDTH))
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .text_color(theme.foreground)
            .children(floating_header)
            .child(match view {
                ComputerView::Overview => self
                    .overview(
                        &agent_name,
                        &coworker_id,
                        box_id.as_deref(),
                        &routines,
                        has_screen,
                        screen,
                        &controls,
                        attention,
                        muted,
                        app,
                        &theme,
                        cx,
                    )
                    .into_any_element(),
                ComputerView::Editor { id } => self
                    .editor(id, coworker_id, routines, muted, app, &theme, window, cx)
                    .into_any_element(),
            })
    }
}

impl ComputerPane {
    /// Writes the editor's fields back to the routine being edited (nothing for a new one):
    /// the back control and each field's change run it.
    fn persist_routine(
        &self,
        app: Entity<AppState>,
        coworker_id: String,
        id: Option<String>,
    ) -> Rc<dyn Fn(&mut App)> {
        let name_input = self.name_input.clone();
        let instruction_input = self.instruction_input.clone();
        let custom_cron = self.custom_cron.clone();
        Rc::new(move |cx: &mut App| {
            let Some(rid) = id.clone() else {
                return;
            };
            let name = name_input.read(cx).value().to_string();
            let instruction = instruction_input.read(cx).value().to_string();
            let cron = custom_cron.read(cx).value().to_string();
            app.update(cx, |state, cx| {
                state.save_routine_fields(&coworker_id, &rid, name, instruction, cx);
                if let Some(sid) = state.routine_mut(&coworker_id, &rid).and_then(|row| {
                    row.triggers.iter().rev().find_map(|t| match t {
                        RoutineTrigger::Schedule { id, .. } => Some(id.clone()),
                        _ => None,
                    })
                }) {
                    if let Some(row) = state.routine_mut(&coworker_id, &rid)
                        && let Some(RoutineTrigger::Schedule { spec, .. }) =
                            row.triggers.iter_mut().find(|t| t.id() == sid)
                        && spec.mode == ScheduleUiMode::Custom
                    {
                        spec.expr = cron;
                    }
                }
            });
        })
    }

    /// The pane's header row. In the title bar over the pane while the pane is docked (then
    /// its title and empty run drag the window), in the pane itself while it floats.
    /// Overview: close chevron (Update / Reset sit next to the screen). Routine: back, title,
    /// close.
    pub fn header(&self, cx: &App, drag: bool) -> AnyElement {
        let app = self.state.clone();
        let state = self.state.read(cx);
        match state.computer_view.clone() {
            ComputerView::Overview => pane_header(None, "", None, app, drag).into_any_element(),
            ComputerView::Editor { id } => {
                let coworker_id = state.active_coworker_id.clone().unwrap_or_default();
                let persist = self.persist_routine(app.clone(), coworker_id, id);
                let back = {
                    let app = app.clone();
                    Rc::new(move |cx: &mut App| {
                        persist(cx);
                        app.update(cx, |state, cx| state.back_to_computer(cx));
                    }) as Rc<dyn Fn(&mut App)>
                };
                pane_header(Some(back), "Routine", None, app, drag).into_any_element()
            }
        }
    }

    fn overview(
        &self,
        agent_name: &str,
        _coworker_id: &str,
        box_id: Option<&str>,
        routines: &[AgentRoutine],
        has_screen: bool,
        screen: Option<Arc<gpui_kit::Image>>,
        controls: &ComputerControls,
        attention: Option<(String, String)>,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
        cx: &App,
    ) -> impl IntoElement {
        v_flex().size_full().child(
            v_flex()
                .w_full()
                .flex_shrink_0()
                .px(px(16.))
                .pt(px(8.))
                .pb(px(16.))
                .gap(px(12.))
                .when_some(attention, |this, (key, instruction)| {
                    this.child(computer_attention_banner(
                        computer_attention_id(),
                        computer_attention_skip_id(&key),
                        computer_attention_done_id(&key),
                        key,
                        instruction,
                        true,
                        app.clone(),
                        cx,
                    ))
                })
                .child(box_chrome(controls, app.clone(), theme, cx))
                .child(screen_tile(
                    has_screen,
                    screen,
                    box_id.map(str::to_string),
                    app.clone(),
                    theme,
                ))
                .child(
                    div()
                        .w_full()
                        .text_center()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{agent_name}'s screen")),
                )
                .when(box_id.is_none(), |this| {
                    this.child(
                        div()
                            .w_full()
                            .text_center()
                            .text_xs()
                            .text_color(muted)
                            .child("No computer yet. The next turn may attach a local box."),
                    )
                })
                .when_some(controls.error.clone(), |this, error| {
                    this.child(
                        div()
                            .w_full()
                            .text_center()
                            .text_xs()
                            .text_color(theme.danger)
                            .child(error),
                    )
                })
                // A task is taught on this screen, so the page that keeps those tasks belongs
                // next to it: this is the way in from where a recipe is born and used.
                .child(recipes_entry(muted, app.clone(), theme))
                .child(if routines.is_empty() {
                    v_flex()
                        .w_full()
                        .gap(px(10.))
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child("Routines are recurring tasks this Bot runs on a schedule."),
                        )
                        .child(
                            div()
                                .id("create-routine")
                                .px(px(12.))
                                .py(px(8.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(theme.border)
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
                                .on_mouse_down(MouseButton::Left, {
                                    let app = app.clone();
                                    move |_, _, cx| {
                                        app.update(cx, |state, cx| {
                                            state.open_routine_editor(None, cx);
                                        });
                                    }
                                })
                                .child(div().text_sm().child("Create routine")),
                        )
                        .into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .gap(px(8.))
                        .child(
                            h_flex()
                                .w_full()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Routines"),
                                )
                                .child(
                                    div()
                                        .id("add-routine")
                                        .px(px(8.))
                                        .py(px(4.))
                                        .rounded(px(8.))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
                                        .on_mouse_down(MouseButton::Left, {
                                            let app = app.clone();
                                            move |_, _, cx| {
                                                app.update(cx, |state, cx| {
                                                    state.open_routine_editor(None, cx);
                                                });
                                            }
                                        })
                                        .child(div().text_sm().child("+")),
                                ),
                        )
                        .children(routines.iter().map(|row| {
                            let id = row.id.clone();
                            let name = if row.name.trim().is_empty() {
                                "Untitled routine".to_string()
                            } else {
                                row.name.clone()
                            };
                            let paused = !row.active;
                            let app = app.clone();
                            h_flex()
                                .id(SharedString::from(format!("routine-{id}")))
                                .w_full()
                                .gap(px(8.))
                                .px(px(10.))
                                .py(px(8.))
                                .rounded(px(10.))
                                .border_1()
                                .border_color(theme.border)
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x777777).opacity(0.1)))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.open_routine_editor(Some(id.clone()), cx);
                                    });
                                })
                                .child(
                                    Icon::default()
                                        .path("icons/sparkles.svg")
                                        .size(px(14.))
                                        .text_color(muted),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w(px(0.))
                                        .child(div().text_sm().truncate().child(name))
                                        .when(paused, |this| {
                                            this.child(
                                                div().text_xs().text_color(muted).child("Paused"),
                                            )
                                        }),
                                )
                        }))
                        .into_any_element()
                }),
        )
    }

    fn editor(
        &self,
        id: Option<String>,
        coworker_id: String,
        routines: Vec<AgentRoutine>,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let existing = id
            .as_ref()
            .and_then(|rid| routines.iter().find(|row| &row.id == rid).cloned());
        let active = existing.as_ref().map(|r| r.active).unwrap_or(true);
        let triggers = existing
            .as_ref()
            .map(|r| r.triggers.clone())
            .unwrap_or_default();
        let runs = existing
            .as_ref()
            .map(|r| r.runs.clone())
            .unwrap_or_default();
        // The same line the overview puts a refused Update on. A routine's calls go out from
        // this page, so their refusals have to be readable from it.
        let trouble = app.read(cx).computer_action_error.clone();
        let persist = self.persist_routine(app.clone(), coworker_id.clone(), id.clone());

        v_flex().size_full().child(
            v_flex()
                .id("routine-editor-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .px(px(16.))
                .py(px(12.))
                .gap(px(16.))
                .child(
                    h_flex()
                        .w_full()
                        .gap(px(8.))
                        .items_center()
                        .child(
                            Switch::new("routine-active")
                                .checked(active)
                                .label("Active")
                                .on_click({
                                    let app = app.clone();
                                    let coworker_id = coworker_id.clone();
                                    let id = id.clone();
                                    move |checked, _, cx| {
                                        if let Some(id) = id.clone() {
                                            app.update(cx, |state, cx| {
                                                state.set_routine_active(
                                                    &coworker_id,
                                                    &id,
                                                    *checked,
                                                    cx,
                                                );
                                            });
                                        }
                                    }
                                }),
                        )
                        .child(div().flex_1())
                        .when(id.is_some(), |this| {
                            this.child(
                                Button::new("routine-delete")
                                    .ghost()
                                    .label("Delete")
                                    .on_click({
                                        let app = app.clone();
                                        let coworker_id = coworker_id.clone();
                                        let id = id.clone();
                                        move |_, _, cx| {
                                            if let Some(id) = id.clone() {
                                                app.update(cx, |state, cx| {
                                                    state.delete_routine(&coworker_id, &id, cx);
                                                });
                                            }
                                        }
                                    }),
                            )
                        })
                        .child(
                            Button::new("routine-test")
                                .primary()
                                .label("Test run")
                                .on_click({
                                    let persist = persist.clone();
                                    let app = app.clone();
                                    let coworker_id = coworker_id.clone();
                                    let id = id.clone();
                                    move |_, _, cx| {
                                        persist(cx);
                                        if let Some(id) = id.clone() {
                                            app.update(cx, |state, cx| {
                                                state.record_routine_run(&coworker_id, &id, cx);
                                            });
                                        }
                                    }
                                }),
                        ),
                )
                .when_some(trouble, |this, trouble| {
                    this.child(
                        div()
                            .id("routine-error")
                            .w_full()
                            .text_xs()
                            .text_color(theme.danger)
                            .child(trouble),
                    )
                })
                .child(field_label("Name", muted))
                .child(field_input(&self.name_input))
                .child(field_label("Instruction", muted))
                .child(field_textarea(&self.instruction_input, theme))
                .child(field_label("When to run", muted))
                .child(self.triggers_box(
                    &triggers,
                    &coworker_id,
                    id.clone(),
                    muted,
                    app.clone(),
                    theme,
                    persist.clone(),
                    cx,
                ))
                .child(field_label("Run history", muted))
                .child(if runs.is_empty() {
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child("No runs yet")
                        .into_any_element()
                } else {
                    v_flex()
                        .w_full()
                        .gap(px(6.))
                        .children(runs.into_iter().enumerate().map(|(i, run)| {
                            h_flex()
                                .id(SharedString::from(format!("run-{i}")))
                                .w_full()
                                .justify_between()
                                .items_center()
                                .py(px(4.))
                                .child(div().text_sm().child(run.at))
                                .when(run.ok, |this| {
                                    this.child(
                                        Icon::default()
                                            .path("icons/check.svg")
                                            .size(px(14.))
                                            .text_color(rgb(0x34c759)),
                                    )
                                })
                        }))
                        .into_any_element()
                }),
        )
    }

    fn triggers_box(
        &self,
        triggers: &[RoutineTrigger],
        coworker_id: &str,
        routine_id: Option<String>,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
        persist: Rc<dyn Fn(&mut App) + 'static>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let last_schedule = triggers.iter().rev().find_map(|t| match t {
            RoutineTrigger::Schedule { id, spec } => Some((id.clone(), spec.clone())),
            _ => None,
        });
        // One routine is one schedule on the server, so the trigger is offered while there is
        // none and gone once there is: a second one would be a second routine.
        let add = triggers.is_empty().then(|| {
            add_trigger_button(
                "+ Add trigger",
                coworker_id.to_string(),
                routine_id.clone(),
                app.clone(),
                persist,
            )
        });
        v_flex()
            .w_full()
            .gap(px(10.))
            .child(
                v_flex()
                    .w_full()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(theme.border)
                    .p(px(10.))
                    .gap(px(10.))
                    .children({
                        let view = cx.entity();
                        let open_id = self.webhook_popover.clone();
                        let theme = theme.clone();
                        let app = app.clone();
                        let coworker_id = coworker_id.to_string();
                        triggers.iter().cloned().map(move |trigger| {
                            if let RoutineTrigger::Webhook {
                                id,
                                url,
                                key,
                                header,
                            } = &trigger
                            {
                                webhook_popover_row(
                                    id.clone(),
                                    coworker_id.clone(),
                                    url.clone(),
                                    key.clone(),
                                    header.clone(),
                                    muted,
                                    open_id.as_ref() == Some(id),
                                    view.clone(),
                                    app.clone(),
                                    theme.clone(),
                                )
                            } else {
                                trigger_row(&trigger, muted)
                            }
                        })
                    })
                    .children(add),
            )
            .when_some(last_schedule, |this, (sid, spec)| {
                this.child(schedule_editor(
                    coworker_id.to_string(),
                    routine_id.clone(),
                    sid,
                    spec,
                    app.clone(),
                    self.custom_cron.clone(),
                    muted,
                    theme,
                ))
            })
    }
}

/// What the Update and Reset controls need to know, read once per frame.
#[derive(Debug, Clone, Default)]
pub struct ComputerControls {
    /// There is a box to act on.
    pub present: bool,
    /// An update is in flight; the buttons wait.
    pub updating: bool,
    /// The box runs an older image than a new one would get.
    pub stale: bool,
    /// The provider compared images and they match: nothing to update to.
    pub current: bool,
    /// Why the last action was refused, or how the last update failed.
    pub error: Option<String>,
}

/// A computer button's label: its resting word, or "Updating…" while an update runs.
pub fn confirm_label(busy: bool, rest: &'static str) -> &'static str {
    if busy { "Updating…" } else { rest }
}

/// The Update button's resting word: what the image comparison says. Unknown (a provider that
/// cannot compare) stays "Update" so a person can still ask.
pub fn update_rest_label(stale: bool, current: bool) -> &'static str {
    if stale {
        "Update available"
    } else if current {
        "Up to date"
    } else {
        "Update"
    }
}

impl ComputerControls {
    /// What the app state says of the active coworker's box, read once per frame.
    pub fn from_state(state: &AppState) -> Self {
        Self {
            present: state
                .coworker_computer
                .as_ref()
                .is_some_and(|s| s.state != "absent"),
            updating: state
                .coworker_computer
                .as_ref()
                .is_some_and(|s| s.updating()),
            stale: state
                .coworker_computer
                .as_ref()
                .is_some_and(|s| s.image_stale()),
            current: state
                .coworker_computer
                .as_ref()
                .is_some_and(|s| s.image.as_ref().is_some() && !s.image_stale()),
            error: state.computer_action_error.clone().or_else(|| {
                state
                    .coworker_computer
                    .as_ref()
                    .and_then(|s| s.update.as_ref())
                    .filter(|u| !u.in_flight())
                    .map(|u| u.detail())
            }),
        }
    }

    /// Nothing to update, or nothing to update to: the Update button waits.
    pub fn update_disabled(&self) -> bool {
        !self.present || self.updating || (self.current && !self.stale)
    }
}

/// The way from this screen to the tasks taught on it: the Recipes page, in the main slot.
fn recipes_entry(
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    h_flex()
        .id("computer-recipes")
        .w_full()
        .gap(px(8.))
        .px(px(10.))
        .py(px(8.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.1)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            app.update(cx, |state, cx| state.open_recipes(cx));
        })
        .child(
            Icon::default()
                .path("icons/record.svg")
                .size(px(14.))
                .text_color(muted),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.))
                .child(div().text_sm().child("Recipes"))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Tasks taught on this screen"),
                ),
        )
}

/// Grok **Needs your attention** while Open the screen is live.
/// Skip this step = decline; I'm done, continue = hand back.
/// Shared by the Computer pane and the launched noVNC window.
///
/// Glass: translucent peach/bronze over the sidebar (`theme.sidebar` shows
/// through). GPUI has no element backdrop-filter; alpha + warm shadow is the
/// frost. Orange is the title only — I'm done is a black (light) / white (dark) pill.
pub(crate) fn computer_attention_banner(
    banner_id: impl Into<ElementId>,
    skip_id: impl Into<ElementId>,
    done_id: impl Into<ElementId>,
    key: String,
    instruction: String,
    can_resolve: bool,
    app: Entity<AppState>,
    cx: &App,
) -> impl IntoElement {
    let skip_app = app.clone();
    let done_app = app;
    let skip_key = key.clone();
    let done_key = key;
    let dark = cx.theme().is_dark();
    let glass = attention_glass(dark);
    let ctas = attention_ctas(dark);
    v_flex()
        .id(banner_id)
        .w_full()
        .flex_shrink_0()
        .gap(px(10.))
        .px(px(18.))
        .py(px(16.))
        .rounded(px(14.))
        .border_1()
        .border_color(glass.border)
        .bg(glass.bg)
        .shadow(attention_shadow(dark))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(glass.title)
                .child("Needs your attention"),
        )
        .child(div().text_sm().text_color(glass.body).child(instruction))
        .child(
            h_flex()
                .w_full()
                .justify_end()
                .gap(px(8.))
                .flex_shrink_0()
                .items_center()
                .flex_wrap()
                .child(attention_cta(
                    skip_id,
                    "Skip this step",
                    ctas.tertiary,
                    false,
                    !can_resolve,
                    can_resolve.then_some(move |cx: &mut App| {
                        skip_app.update(cx, |state, cx| {
                            state.resolve_user_form_handoff(
                                skip_key.clone(),
                                BoxHandoffResolution::Declined,
                                cx,
                            );
                        });
                    }),
                ))
                .child(attention_cta(
                    done_id,
                    "I'm done, continue",
                    ctas.primary,
                    true,
                    !can_resolve,
                    can_resolve.then_some(move |cx: &mut App| {
                        done_app.update(cx, |state, cx| {
                            state.resolve_user_form_handoff(
                                done_key.clone(),
                                BoxHandoffResolution::HandedBack,
                                cx,
                            );
                        });
                    }),
                )),
        )
}

/// The coworker's screen. The Open pill is the control: it appears on hover
/// only once the box has a screen URL, and only then does the tile take a
/// click — a blank monitor must not provision a box behind the person's back.
fn screen_tile(
    has_screen: bool,
    screen: Option<Arc<gpui_kit::Image>>,
    box_id: Option<String>,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    // The tile is the screen's own 1280×800 shape — same AR as the in-chat
    // Computer card well (`BOX_SCREEN_ASPECT`) so neither letterboxes nor
    // clips the taskbar.
    let width = computer_pane_screen_width();
    let height = box_screen_height_for_width(width);
    div()
        .id("agent-screen")
        .group("agent-screen")
        .relative()
        .w(px(width))
        .h(px(height))
        .aspect_ratio(BOX_SCREEN_ASPECT)
        .flex_shrink_0()
        .rounded(px(12.))
        .border_1()
        .border_color(theme.border)
        // The dark plate is for the empty tile only: under a picture it showed through the
        // corners as a thick dark arc between the border and the image's own rounding.
        .when(screen.is_none(), |this| this.bg(rgb(0x2a2a2a)))
        .overflow_hidden()
        .when(has_screen, |this| {
            this.cursor_pointer().on_mouse_down(MouseButton::Left, {
                let app = app.clone();
                move |_, _, cx| {
                    app.update(cx, |state, cx| {
                        state.open_coworker_screen(cx);
                    });
                }
            })
        })
        // The screen itself when we have it; a monitor glyph until then.
        .map(|this| match screen {
            // The image carries its own rounding: the tile's overflow clip does not round a
            // painted picture. Fill, not Cover: the tile already has the screen's aspect, and a
            // Cover that overflowed the tile lost its bottom corners to the rectangular clip.
            Some(image) => this.child(
                img(image)
                    .size_full()
                    .rounded(px(12.))
                    .object_fit(ObjectFit::Fill),
            ),
            None => this.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::default()
                            .path("icons/monitor.svg")
                            .size(px(22.))
                            .text_color(rgb(0x888888)),
                    ),
            ),
        })
        // The box's id, small, in the corner of the screen it names.
        .when_some(box_id, |this, id| {
            this.child(
                div()
                    .absolute()
                    .bottom(px(6.))
                    .right(px(8.))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .bg(gpui::black().opacity(0.45))
                    .text_xs()
                    .text_color(gpui::white())
                    .child(id),
            )
        })
        .when(has_screen, |this| {
            this.child(
                div()
                    .id("agent-screen-open")
                    .absolute()
                    .inset_0()
                    // Same corners as the tile, or the hover shade sticks out past them.
                    .rounded(px(11.))
                    .opacity(0.)
                    .group_hover("agent-screen", |style| style.opacity(1.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(0x000000).opacity(0.35))
                    .child(
                        div()
                            .px(px(16.))
                            .py(px(8.))
                            .rounded_full()
                            .bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                Icon::default()
                                    .path("icons/expand.svg")
                                    .size(px(14.))
                                    .text_color(theme.primary_foreground),
                            )
                            .child("Open"),
                    ),
            )
        })
}

/// Update / Reset next to this bot's screen. Route traffic for a dedicated
/// provisioned box is the header icon (`icons/route-traffic.svg`, analogue of
/// SF Symbol arrow.triangle.swap). Status copy is a tooltip on download.
/// `computer-update` / `computer-reset` stay the remasure ids.
fn box_chrome(
    controls: &ComputerControls,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
    cx: &App,
) -> impl IntoElement {
    let can_update = !controls.update_disabled();
    let can_reset = controls.present && !controls.updating;
    let stale = controls.stale;
    let update_tip = if stale {
        "Update available"
    } else if controls.current {
        "Up to date"
    } else if controls.present {
        "Update this computer"
    } else {
        "No computer yet"
    };
    let update_app = app.clone();
    let reset_app = app.clone();
    let show_route = app.read(cx).show_route_traffic_on_bot_pane();
    h_flex()
        .id("computer-box-chrome")
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(h_flex().items_center().when(show_route, |this| {
            this.child(route_traffic_icon(app.clone(), theme, cx))
        }))
        .child(
            h_flex()
                .gap(px(4.))
                .child(
                    icon_btn_enabled(
                        "computer-update",
                        "icons/download.svg",
                        can_update,
                        move |cx| {
                            update_app.update(cx, |state, cx| {
                                state
                                    .open_computer_confirm(crate::state::ComputerAction::Update, cx)
                            });
                        },
                    )
                    .when(stale, |this| this.text_color(gpui::blue()))
                    .tooltip(move |window, cx| Tooltip::new(update_tip).build(window, cx)),
                )
                .child(
                    icon_btn_enabled("computer-reset", "icons/reset.svg", can_reset, move |cx| {
                        reset_app.update(cx, |state, cx| {
                            state.open_computer_confirm(crate::state::ComputerAction::Reset, cx)
                        });
                    })
                    .tooltip(|window, cx| Tooltip::new("Reset this computer").build(window, cx)),
                ),
        )
}

fn route_traffic_icon(
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
    cx: &App,
) -> impl IntoElement {
    let enabled = app.read(cx).egress_tunnel_enabled;
    let color = if enabled {
        theme.primary
    } else {
        theme.muted_foreground
    };
    let tip = if enabled {
        "Routing traffic through this computer. New connections go out through this desktop."
    } else {
        "Route traffic through this computer. Web traffic from this Bot's computer goes out through this desktop instead of the cloud."
    };
    div()
        .id("route-traffic-this-computer")
        .size(px(28.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .on_mouse_down(MouseButton::Left, {
            let app = app.clone();
            move |_, _, cx| {
                app.update(cx, |state, cx| {
                    state.set_egress_tunnel_enabled(!state.egress_tunnel_enabled, cx);
                });
            }
        })
        .child(
            Icon::default()
                .path("icons/route-traffic.svg")
                .size(px(16.))
                .text_color(color),
        )
}

fn pane_header(
    back: Option<Rc<dyn Fn(&mut App)>>,
    title: &'static str,
    actions: Option<&ComputerControls>,
    app: Entity<AppState>,
    drag: bool,
) -> impl IntoElement {
    // Close chevron (and Routine back). Update / Reset sit next to the screen
    // in `box_chrome` so they are visible on the right sidebar, not only in
    // an empty title-bar strip.
    let actions_app = app.clone();
    let actions = actions.map(|controls| {
        (
            !controls.update_disabled(),
            controls.present && !controls.updating,
            controls.stale,
        )
    });
    h_flex()
        .id("computer-header")
        .w_full()
        .px(px(HEADER_PX))
        .h(px(TITLE_BAR_H))
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .child(
            h_flex()
                .gap(px(4.))
                .items_center()
                .when_some(back, |this, back| {
                    this.child(icon_btn(
                        "computer-back",
                        "icons/chevron-left.svg",
                        move |cx| back(cx),
                    ))
                })
                .when(!title.is_empty(), |this| {
                    this.child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title)
                            .when(drag, |this| {
                                this.on_mouse_down(MouseButton::Left, |_, window, _| {
                                    window.start_window_move()
                                })
                            }),
                    )
                })
                .when_some(actions, |this, (can_update, can_reset, stale)| {
                    let update_app = actions_app.clone();
                    let reset_app = actions_app.clone();
                    this.child(
                        icon_btn_enabled(
                            "computer-update",
                            "icons/download.svg",
                            can_update,
                            move |cx| {
                                update_app.update(cx, |state, cx| {
                                    state.open_computer_confirm(
                                        crate::state::ComputerAction::Update,
                                        cx,
                                    )
                                });
                            },
                        )
                        .when(stale, |this| this.text_color(gpui::blue())),
                    )
                    .child(icon_btn_enabled(
                        "computer-reset",
                        "icons/reset.svg",
                        can_reset,
                        move |cx| {
                            reset_app.update(cx, |state, cx| {
                                state.open_computer_confirm(crate::state::ComputerAction::Reset, cx)
                            });
                        },
                    ))
                }),
        )
        // The empty run between the controls: in the title bar, the handle to drag the
        // window by.
        .child(div().flex_1().h_full().when(drag, |this| {
            this.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
        }))
        .child(icon_btn(
            "computer-close",
            "icons/chevrons-right.svg",
            move |cx| {
                app.update(cx, |state, cx| state.close_right_pane(cx));
            },
        ))
}

/// `icon_btn` that can be greyed out: no hover, no click, until there is something to do.
fn icon_btn_enabled(
    id: &'static str,
    path: &'static str,
    enabled: bool,
    on_click: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| on_click(cx))
        })
        .when(!enabled, |this| this.opacity(0.35))
        .child(Icon::default().path(path).size(px(16.)))
}

fn icon_btn(
    id: &'static str,
    path: &'static str,
    on_click: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(28.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| on_click(cx))
        .child(Icon::default().path(path).size(px(16.)))
}

fn field_label(label: &'static str, muted: Hsla) -> impl IntoElement {
    div().text_xs().text_color(muted).child(label)
}

fn field_textarea(
    state: &Entity<TextareaState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    Textarea::new(state)
        .appearance(false)
        .w_full()
        .h(px(96.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.input)
        .bg(theme.input_background())
}

fn trigger_row(trigger: &RoutineTrigger, muted: Hsla) -> AnyElement {
    let icon = match trigger {
        RoutineTrigger::Webhook { .. } => "icons/globe.svg",
        RoutineTrigger::Schedule { .. } => "icons/clock.svg",
        RoutineTrigger::Event { kind, .. } if *kind == "git" => "icons/sparkles.svg",
        _ => "icons/sparkles.svg",
    };
    h_flex()
        .w_full()
        .gap(px(8.))
        .items_center()
        .child(Icon::default().path(icon).size(px(14.)).text_color(muted))
        .child(div().text_sm().child(trigger.label()))
        .into_any_element()
}

#[derive(IntoElement)]
struct WebhookRowTrigger {
    selected: bool,
    muted: Hsla,
}

impl Selectable for WebhookRowTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for WebhookRowTrigger {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap(px(8.))
            .items_center()
            .cursor_pointer()
            .py(px(2.))
            .child(
                Icon::default()
                    .path("icons/globe.svg")
                    .size(px(14.))
                    .text_color(self.muted),
            )
            .child(div().text_sm().child("When a webhook fires"))
    }
}

/// The webhook trigger's row, and what opens under it: the URL, the key and the header the
/// server minted, shown as they are.
///
/// Read-only, and that is the change. These three used to be fields somebody could type into,
/// over values this app had made up — a URL pointing at a route that did not exist and a key
/// nothing had ever been told about. Now they are the server's, and a field somebody could
/// type into would be a field somebody could believe they had changed. Each copies, and the
/// key can be replaced by asking the server for another one.
fn webhook_popover_row(
    routine_id: String,
    coworker_id: String,
    url: String,
    key: String,
    header: String,
    muted: Hsla,
    open: bool,
    view: Entity<ComputerPane>,
    app: Entity<AppState>,
    theme: gpui_kit::component::Theme,
) -> AnyElement {
    let panel_bg = theme.sidebar;
    Popover::new(SharedString::from(format!("webhook-pop-{routine_id}")))
        .appearance(false)
        .overlay_closable(true)
        .open(open)
        .on_open_change({
            let view = view.clone();
            let routine_id = routine_id.clone();
            move |is_open, _, cx| {
                view.update(cx, |this, cx| {
                    this.webhook_popover = if *is_open {
                        Some(routine_id.clone())
                    } else {
                        None
                    };
                    cx.notify();
                });
            }
        })
        .trigger(WebhookRowTrigger {
            selected: open,
            muted,
        })
        .content(move |_, _, _| {
            let rotate_id = SharedString::from(format!("routine-{routine_id}-rotate"));
            let app = app.clone();
            let coworker_id = coworker_id.clone();
            let rotating_id = routine_id.clone();
            v_flex()
                .id("webhook-pop-panel")
                .w(px(280.))
                .p(px(12.))
                .gap(px(8.))
                .rounded(px(12.))
                .border_1()
                .border_color(theme.border)
                .bg(panel_bg)
                .shadow_lg()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(field_label("POST to", muted))
                .child(copy_row(
                    format!("routine-{routine_id}-webhook-url"),
                    url.clone(),
                    muted,
                    &theme,
                ))
                .child(field_label("key", muted))
                .child(copy_row(
                    format!("routine-{routine_id}-webhook-key"),
                    key.clone(),
                    muted,
                    &theme,
                ))
                .child(field_label("header", muted))
                .child(copy_row(
                    format!("routine-{routine_id}-webhook-header"),
                    header.clone(),
                    muted,
                    &theme,
                ))
                .child(
                    Button::new(rotate_id)
                        .ghost()
                        .label("Rotate key")
                        .tooltip("The old key stops working at once.")
                        .on_click(move |_, _, cx| {
                            let coworker_id = coworker_id.clone();
                            let routine_id = rotating_id.clone();
                            app.update(cx, |state, cx| {
                                state.rotate_routine_webhook(&coworker_id, &routine_id, cx);
                            });
                        }),
                )
        })
        .into_any_element()
}

/// One value the server minted, as it is, with the button that puts it on the clipboard.
fn copy_row(
    id: String,
    value: String,
    muted: Hsla,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    let copied = value.clone();
    h_flex()
        .w_full()
        .gap(px(6.))
        .items_center()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.input)
        .bg(theme.input_background())
        .px(px(8.))
        .py(px(4.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_xs()
                .truncate()
                .child(value),
        )
        .child(
            div()
                .id(SharedString::from(id))
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                .tooltip(|window, cx| Tooltip::new("Copy").build(window, cx))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                })
                .child(
                    Icon::default()
                        .path("icons/copy.svg")
                        .size(px(12.))
                        .text_color(muted),
                ),
        )
}

fn add_trigger_button(
    label: &'static str,
    coworker_id: String,
    routine_id: Option<String>,
    app: Entity<AppState>,
    persist: Rc<dyn Fn(&mut App) + 'static>,
) -> impl IntoElement {
    Button::new("add-trigger")
        .ghost()
        .label(label)
        .dropdown_menu(move |menu, window, cx| {
            let webhook = {
                let app = app.clone();
                let persist = persist.clone();
                let coworker_id = coworker_id.clone();
                let routine_id = routine_id.clone();
                move |cx: &mut App| {
                    // The name and the prompt go to the server with the trigger, so whatever
                    // is in the fields has to be on the routine before this asks for one.
                    persist(cx);
                    let Some(rid) = routine_id.clone() else {
                        return;
                    };
                    app.update(cx, |state, cx| {
                        state.add_routine_trigger(&coworker_id, &rid, NewTrigger::Webhook, cx);
                    });
                }
            };
            menu.submenu("On a schedule", window, cx, {
                let push_sched: Rc<dyn Fn(&'static str, &mut App)> = Rc::new({
                    let persist = persist.clone();
                    let app = app.clone();
                    let coworker_id = coworker_id.clone();
                    let routine_id = routine_id.clone();
                    move |preset: &'static str, cx: &mut App| {
                        persist(cx);
                        if let Some(rid) = routine_id.clone() {
                            app.update(cx, |state, cx| {
                                state.add_routine_trigger(
                                    &coworker_id,
                                    &rid,
                                    NewTrigger::Schedule(ScheduleSpec::from_preset(preset)),
                                    cx,
                                );
                            });
                        }
                    }
                });
                move |menu, _, _| {
                    let item =
                        |label: &'static str, push_sched: Rc<dyn Fn(&'static str, &mut App)>| {
                            PopupMenuItem::new(label).on_click({
                                let push_sched = push_sched.clone();
                                move |_, _, cx| push_sched(label, cx)
                            })
                        };
                    menu.item(item("Every hour", push_sched.clone()))
                        .item(item("Every day", push_sched.clone()))
                        .item(item("Weekdays", push_sched.clone()))
                        .item(item("Every week", push_sched.clone()))
                        .item(item("Every month", push_sched.clone()))
                        .item(item("Interval", push_sched.clone()))
                        .item(item("Advanced...", push_sched.clone()))
                }
            })
            .item(PopupMenuItem::new("Webhook").on_click(move |_, _, cx| webhook(cx)))
        })
}

fn schedule_editor(
    coworker_id: String,
    routine_id: Option<String>,
    trigger_id: String,
    spec: ScheduleSpec,
    app: Entity<AppState>,
    custom_cron: Entity<InputState>,
    muted: Hsla,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    let patch = {
        let app = app.clone();
        let coworker_id = coworker_id.clone();
        let routine_id = routine_id.clone();
        let trigger_id = trigger_id.clone();
        Rc::new(move |spec: ScheduleSpec, cx: &mut App| {
            if let Some(rid) = routine_id.clone() {
                app.update(cx, |state, cx| {
                    state.update_schedule_spec(&coworker_id, &rid, &trigger_id, spec, cx);
                });
            }
        })
    };

    v_flex()
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .p(px(10.))
        .gap(px(8.))
        .child(mode_row(spec.clone(), patch.clone()))
        .child(match spec.mode {
            ScheduleUiMode::Interval => interval_row(spec, patch).into_any_element(),
            ScheduleUiMode::Custom => field_input(&custom_cron).into_any_element(),
            ScheduleUiMode::Advanced => advanced_editor(spec, patch, muted).into_any_element(),
        })
}

fn mode_row(spec: ScheduleSpec, patch: Rc<dyn Fn(ScheduleSpec, &mut App)>) -> impl IntoElement {
    let label = match spec.mode {
        ScheduleUiMode::Interval => "Interval",
        ScheduleUiMode::Custom => "Custom",
        ScheduleUiMode::Advanced => "Advanced",
    };
    Button::new("sched-mode")
        .ghost()
        .label(label)
        .dropdown_menu(move |menu, _, _| {
            let item = |name: &'static str,
                        mode: ScheduleUiMode,
                        spec: ScheduleSpec,
                        patch: Rc<dyn Fn(ScheduleSpec, &mut App)>| {
                PopupMenuItem::new(name).on_click({
                    let patch = patch.clone();
                    move |_, _, cx| {
                        let mut next = spec.clone();
                        next.mode = mode;
                        if mode == ScheduleUiMode::Custom && next.expr.is_empty() {
                            next.expr = match next.unit {
                                ScheduleUnit::Minutes => format!("@every {}m", next.every),
                                ScheduleUnit::Hours => format!("@every {}h", next.every),
                                ScheduleUnit::Days => format!("@every {}d", next.every),
                            };
                        }
                        patch(next, cx);
                    }
                })
            };
            menu.item(item(
                "Interval",
                ScheduleUiMode::Interval,
                spec.clone(),
                patch.clone(),
            ))
            .item(item(
                "Custom",
                ScheduleUiMode::Custom,
                spec.clone(),
                patch.clone(),
            ))
            .item(item(
                "Advanced",
                ScheduleUiMode::Advanced,
                spec.clone(),
                patch.clone(),
            ))
        })
}

fn interval_row(spec: ScheduleSpec, patch: Rc<dyn Fn(ScheduleSpec, &mut App)>) -> impl IntoElement {
    h_flex()
        .w_full()
        .gap(px(6.))
        .items_center()
        .child(div().text_sm().text_color(rgb(0x888888)).child("every"))
        .child({
            let spec = spec.clone();
            let patch = patch.clone();
            Button::new("sched-every")
                .ghost()
                .label(spec.every.to_string())
                .dropdown_menu(move |menu, _, _| {
                    let mut menu = menu;
                    for n in [1, 2, 5, 10, 15, 30, 45, 60] {
                        let spec = spec.clone();
                        let patch = patch.clone();
                        menu = menu.item(PopupMenuItem::new(n.to_string()).on_click(
                            move |_, _, cx| {
                                let mut next = spec.clone();
                                next.every = n;
                                patch(next, cx);
                            },
                        ));
                    }
                    menu
                })
        })
        .child({
            let unit_label = match spec.unit {
                ScheduleUnit::Minutes => "minutes",
                ScheduleUnit::Hours => "hours",
                ScheduleUnit::Days => "days",
            };
            Button::new("sched-unit")
                .ghost()
                .label(unit_label)
                .dropdown_menu(move |menu, _, _| {
                    let item =
                        |name: &'static str,
                         unit: ScheduleUnit,
                         spec: ScheduleSpec,
                         patch: Rc<dyn Fn(ScheduleSpec, &mut App)>| {
                            PopupMenuItem::new(name).on_click(move |_, _, cx| {
                                let mut next = spec.clone();
                                next.unit = unit;
                                patch(next, cx);
                            })
                        };
                    menu.item(item(
                        "minutes",
                        ScheduleUnit::Minutes,
                        spec.clone(),
                        patch.clone(),
                    ))
                    .item(item(
                        "hours",
                        ScheduleUnit::Hours,
                        spec.clone(),
                        patch.clone(),
                    ))
                    .item(item(
                        "days",
                        ScheduleUnit::Days,
                        spec.clone(),
                        patch.clone(),
                    ))
                })
        })
}

fn advanced_editor(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(px(8.))
        .child(
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(div().text_xs().text_color(muted).w(px(52.)).child("Months"))
                .child(months_menu(spec.clone(), patch.clone())),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(div().text_xs().text_color(muted).w(px(52.)).child("Days"))
                .child(days_menu(spec.clone(), patch.clone()))
                .when(spec.day_kind == ScheduleDayKind::DaysOfMonth, |this| {
                    this.child(month_day_menu(spec.clone(), patch.clone()))
                }),
        )
        .when(spec.day_kind == ScheduleDayKind::Weekdays, |this| {
            this.child(weekday_chips(spec.clone(), patch.clone()))
        })
        .child(
            h_flex()
                .gap(px(8.))
                .items_start()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .w(px(52.))
                        .pt(px(6.))
                        .child("Time"),
                )
                .child(
                    v_flex()
                        .gap(px(6.))
                        .children(
                            spec.times.iter().enumerate().map(|(i, (h, m))| {
                                time_row(i, *h, *m, spec.clone(), patch.clone())
                            }),
                        )
                        .child({
                            let spec = spec.clone();
                            let patch = patch.clone();
                            div()
                                .id("add-time")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    let mut next = spec.clone();
                                    next.times.push((9, 0));
                                    patch(next, cx);
                                })
                                .child(div().text_sm().child("+ Add time"))
                        }),
                ),
        )
}

fn months_menu(spec: ScheduleSpec, patch: Rc<dyn Fn(ScheduleSpec, &mut App)>) -> impl IntoElement {
    let label = if spec.months.is_empty() {
        "Any month"
    } else {
        "Selected months"
    };
    Button::new("sched-months")
        .ghost()
        .label(label)
        .dropdown_menu(move |menu, _, _| {
            let names = [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ];
            let mut menu = menu.item(PopupMenuItem::new("Any month").on_click({
                let spec = spec.clone();
                let patch = patch.clone();
                move |_, _, cx| {
                    let mut next = spec.clone();
                    next.months.clear();
                    patch(next, cx);
                }
            }));
            for (i, name) in names.iter().enumerate() {
                let month = (i + 1) as u8;
                let on = spec.months.contains(&month);
                let spec = spec.clone();
                let patch = patch.clone();
                menu = menu.item(PopupMenuItem::new(*name).checked(on).on_click(
                    move |_, _, cx| {
                        let mut next = spec.clone();
                        if let Some(pos) = next.months.iter().position(|m| *m == month) {
                            next.months.remove(pos);
                        } else {
                            next.months.push(month);
                            next.months.sort();
                        }
                        patch(next, cx);
                    },
                ));
            }
            menu
        })
}

fn days_menu(spec: ScheduleSpec, patch: Rc<dyn Fn(ScheduleSpec, &mut App)>) -> impl IntoElement {
    let label = match spec.day_kind {
        ScheduleDayKind::EveryDay => "Every day",
        ScheduleDayKind::Weekdays => "Days of the week",
        ScheduleDayKind::DaysOfMonth => "Days of the month",
    };
    Button::new("sched-days")
        .ghost()
        .label(label)
        .dropdown_menu(move |menu, _, _| {
            let item = |name: &'static str,
                        kind: ScheduleDayKind,
                        spec: ScheduleSpec,
                        patch: Rc<dyn Fn(ScheduleSpec, &mut App)>| {
                PopupMenuItem::new(name).on_click(move |_, _, cx| {
                    let mut next = spec.clone();
                    next.day_kind = kind;
                    patch(next, cx);
                })
            };
            menu.item(item(
                "Every day",
                ScheduleDayKind::EveryDay,
                spec.clone(),
                patch.clone(),
            ))
            .item(item(
                "Days of the week",
                ScheduleDayKind::Weekdays,
                spec.clone(),
                patch.clone(),
            ))
            .item(item(
                "Days of the month",
                ScheduleDayKind::DaysOfMonth,
                spec.clone(),
                patch.clone(),
            ))
        })
}

fn month_day_menu(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
) -> impl IntoElement {
    let label = spec
        .month_days
        .first()
        .copied()
        .map(|d| match d {
            1 => "1st".into(),
            2 => "2nd".into(),
            3 => "3rd".into(),
            n => format!("{n}th"),
        })
        .unwrap_or_else(|| "1st".into());
    Button::new("sched-mdays")
        .ghost()
        .label(label)
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu;
            for d in 1u8..=31 {
                let on = spec.month_days.contains(&d);
                let name = match d {
                    1 => "1st".into(),
                    2 => "2nd".into(),
                    3 => "3rd".into(),
                    n => format!("{n}th"),
                };
                let spec = spec.clone();
                let patch = patch.clone();
                menu = menu.item(
                    PopupMenuItem::new(name)
                        .checked(on)
                        .on_click(move |_, _, cx| {
                            let mut next = spec.clone();
                            if let Some(pos) = next.month_days.iter().position(|x| *x == d) {
                                next.month_days.remove(pos);
                            } else {
                                next.month_days.push(d);
                                next.month_days.sort();
                            }
                            patch(next, cx);
                        }),
                );
            }
            menu
        })
}

fn weekday_chips(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
) -> impl IntoElement {
    let names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    h_flex().gap(px(4.)).children((0u8..7).map(|d| {
        let on = spec.weekdays.contains(&d);
        let spec = spec.clone();
        let patch = patch.clone();
        div()
            .id(SharedString::from(format!("wd-{d}")))
            .px(px(8.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(rgb(0x777777).opacity(if on { 0.28 } else { 0.1 }))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                let mut next = spec.clone();
                if let Some(pos) = next.weekdays.iter().position(|x| *x == d) {
                    next.weekdays.remove(pos);
                } else {
                    next.weekdays.push(d);
                    next.weekdays.sort();
                }
                patch(next, cx);
            })
            .child(div().text_xs().child(names[d as usize]))
    }))
}

fn time_row(
    index: usize,
    hour: u8,
    minute: u8,
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
) -> impl IntoElement {
    let label = {
        let (h12, am) = if hour == 0 {
            (12, true)
        } else if hour < 12 {
            (hour, true)
        } else if hour == 12 {
            (12, false)
        } else {
            (hour - 12, false)
        };
        format!("{}:{:02} {}", h12, minute, if am { "AM" } else { "PM" })
    };
    h_flex()
        .gap(px(6.))
        .items_center()
        .child(
            Button::new(SharedString::from(format!("time-{index}")))
                .ghost()
                .label(label)
                .dropdown_menu({
                    let spec = spec.clone();
                    let patch = patch.clone();
                    move |menu, _, _| {
                        let mut menu = menu;
                        for h in 0u8..24 {
                            for m in [0u8, 15, 30, 45] {
                                let spec = spec.clone();
                                let patch = patch.clone();
                                let (h12, am) = if h == 0 {
                                    (12, true)
                                } else if h < 12 {
                                    (h, true)
                                } else if h == 12 {
                                    (12, false)
                                } else {
                                    (h - 12, false)
                                };
                                let name =
                                    format!("{}:{:02} {}", h12, m, if am { "AM" } else { "PM" });
                                menu = menu.item(PopupMenuItem::new(name).on_click(
                                    move |_, _, cx| {
                                        let mut next = spec.clone();
                                        if let Some(slot) = next.times.get_mut(index) {
                                            *slot = (h, m);
                                        }
                                        patch(next, cx);
                                    },
                                ));
                            }
                        }
                        menu
                    }
                }),
        )
        .child(
            div()
                .id(SharedString::from(format!("time-x-{index}")))
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, {
                    let spec = spec.clone();
                    let patch = patch.clone();
                    move |_, _, cx| {
                        let mut next = spec.clone();
                        if index < next.times.len() {
                            next.times.remove(index);
                        }
                        patch(next, cx);
                    }
                })
                .child(div().text_sm().child("×")),
        )
}
