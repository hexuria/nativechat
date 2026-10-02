use std::rc::Rc;
use std::sync::Arc;

use crate::chrome::{
    BOX_SCREEN_ASPECT, HEADER_PX, INFO_PANE_WIDTH, TITLE_BAR_H, box_screen_height_for_width,
    chrome_floats, computer_pane_screen_width,
};
use crate::components::agent_settings::sentence;
use crate::components::alert_chrome::{
    attention_cta, attention_ctas, attention_glass, attention_shadow,
};
use crate::components::fields::field_input;
use crate::components::switch::Switch;
use crate::opengrok::{
    BoxHandoffResolution, ComputerError, CoworkerComputer, LocalExecMode, ScheduleEdit,
    ScheduleRunStatus, computer_attention_done_id, computer_attention_id,
    computer_attention_skip_id,
};
use crate::state::{
    AgentRoutine, AppState, ComputerView, NewTrigger, ROUTINE_RUNS_UNAVAILABLE, RoutineTrigger,
    ScheduleDayKind, ScheduleSpec, ScheduleUiMode, ScheduleUnit, routine_notes,
    routine_trouble_line, unsaved_lines,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    ActiveTheme, Disableable as _, Icon, Selectable, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct ComputerPane {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    instruction_input: Entity<TextareaState>,
    custom_cron: Entity<InputState>,
    /// The routine the fields were last filled from, and the `routine_resync` they were
    /// filled at.
    loaded_editor: Option<(Option<String>, u64)>,
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
        // Again whenever the server's copy of the routine replaced the one on screen (an edit's
        // answer, or a refused edit put back): fields left on the old text would send it again
        // on the next Back or Test run, over what the server just said.
        // Only this routine's: an answer for another one must leave these fields, which may be
        // half-typed, as they are.
        let resync = id
            .as_deref()
            .map_or(0, |rid| self.state.read(cx).routine_resync(rid));
        if self.loaded_editor.as_ref() == Some(&(id.clone(), resync)) {
            return;
        }
        self.loaded_editor = Some((id.clone(), resync));
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

        // Chat's window-level controls replace the pane close button. Other pages
        // still need their own header when the pane floats over their content.
        let chat_page = app.read(cx).page == crate::state::MainPage::Chat;
        let editor_header = matches!(view, ComputerView::Editor { .. });
        v_flex()
            .id("computer-pane")
            .h_full()
            .w(px(INFO_PANE_WIDTH))
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .text_color(theme.foreground)
            .when(chat_page, |this| {
                this.child(div().h(px(TITLE_BAR_H)).flex_shrink_0())
                    .when(editor_header, |this| this.child(self.header(cx, false)))
            })
            .when(
                !chat_page && chrome_floats(f32::from(window.viewport_size().width)),
                |this| this.child(self.header(cx, true)),
            )
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
                // The typed cron line first, so the save below sends the line on screen.
                if let Some(sid) = state.routine_mut(&coworker_id, &rid).and_then(|row| {
                    row.triggers.iter().rev().find_map(|t| match t {
                        RoutineTrigger::Schedule { id, .. } => Some(id.clone()),
                        _ => None,
                    })
                }) && let Some(row) = state.routine_mut(&coworker_id, &rid)
                    && let Some(RoutineTrigger::Schedule { spec, .. }) =
                        row.triggers.iter_mut().find(|t| t.id() == sid)
                    && spec.mode == ScheduleUiMode::Custom
                {
                    spec.expr = cron;
                }
                state.save_routine_fields(&coworker_id, &rid, name, instruction, cx);
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

    #[allow(clippy::too_many_arguments)]
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
                // The server's reason, when it has said why it could not give this Bot a
                // computer, in place of the guess that the next turn may attach one.
                .map(|this| match controls.no_computer.clone() {
                    Some(why) => {
                        this.child(no_computer_notice(why, controls.asking, app.clone(), theme))
                    }
                    None if box_id.is_none() => this.child(
                        div()
                            .w_full()
                            .text_center()
                            .text_xs()
                            .text_color(muted)
                            .child("No computer yet. The next turn may attach a local box."),
                    ),
                    None => this,
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
                                .id("routine-new")
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
                                        .id("routine-new")
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
                        // `routine-{id}` here and in the driver's tree
                        // (`crate::agent::ids::routine`): one routine, one name, whether it is
                        // clicked by a person or by a test.
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

    #[allow(clippy::too_many_arguments)]
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
        let (missing, unsaved) = {
            let state = app.read(cx);
            (
                state.routine_routes_missing,
                id.as_ref()
                    .and_then(|rid| state.routine_unsaved.get(rid).cloned()),
            )
        };
        // A routine the server has. A draft is still this app's, and is made by a route every
        // server has, so what an older server cannot do to a routine does not touch it.
        let on_the_server = existing.as_ref().is_some_and(|r| r.saved.is_some());
        let can_change = !(on_the_server && missing.edit);
        let can_run = !missing.run_now;
        let rid = id.clone().unwrap_or_default();
        // What this server cannot do, said beside the controls it leaves dead, for as long as it
        // is so: a control that does nothing must say why it does nothing.
        let notes = routine_notes(missing, on_the_server);
        // The same line the overview puts a refused Update on. A routine's calls go out from
        // this page, so their refusals have to be readable from it.
        let trouble = routine_trouble_line(app.read(cx).computer_action_error.as_deref(), &notes);
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
                .child(routine_actions(
                    app.clone(),
                    coworker_id.clone(),
                    id.clone(),
                    active,
                    id.is_some() && !triggers.is_empty(),
                    can_run,
                    persist.clone(),
                ))
                .children(notes.into_iter().map(|(tail, note)| {
                    div()
                        .id(SharedString::from(format!("routine-{rid}-{tail}")))
                        .w_full()
                        .text_xs()
                        .text_color(muted)
                        .child(note)
                }))
                .when_some(unsaved, |this, edit| {
                    this.child(unsaved_block(&rid, &edit, muted, theme))
                })
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
                .child(field_input(&self.name_input).disabled(!can_change))
                .child(field_label("Instruction", muted))
                .child(field_textarea(&self.instruction_input, theme).disabled(!can_change))
                .child(field_label("When to run", muted))
                .child(self.triggers_box(
                    &triggers,
                    &coworker_id,
                    id.clone(),
                    muted,
                    app.clone(),
                    theme,
                    persist.clone(),
                    can_change,
                    cx,
                ))
                .child(field_label("Run history", muted))
                .child(if on_the_server && missing.runs {
                    // Not "No runs yet": nobody knows that, because this server cannot say.
                    div()
                        .id(SharedString::from(format!(
                            "routine-{rid}-runs-unavailable"
                        )))
                        .text_sm()
                        .text_color(muted)
                        .child(ROUTINE_RUNS_UNAVAILABLE)
                        .into_any_element()
                } else if runs.is_empty() {
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
                                // A line of the history opens the thread it ran in, which is
                                // where what it said is. On click, not on press, so a scroll
                                // that starts on a line stays in the editor.
                                .cursor_pointer()
                                .on_click({
                                    let persist = persist.clone();
                                    let app = app.clone();
                                    let id = id.clone();
                                    move |_, _, cx| {
                                        persist(cx);
                                        if let Some(id) = id.clone() {
                                            app.update(cx, |state, cx| {
                                                state.open_routine_thread(&id, cx);
                                            });
                                        }
                                    }
                                })
                                .w_full()
                                .justify_between()
                                .items_center()
                                .py(px(4.))
                                .child(
                                    h_flex()
                                        .gap(px(6.))
                                        .child(div().text_sm().child(run.at.clone()))
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(run.cause_label()),
                                        ),
                                )
                                .child(match run.status {
                                    ScheduleRunStatus::Ok => Icon::default()
                                        .path("icons/check.svg")
                                        .size(px(14.))
                                        .text_color(rgb(0x34c759))
                                        .into_any_element(),
                                    ScheduleRunStatus::Error => div()
                                        .text_xs()
                                        .text_color(theme.danger)
                                        .child("Failed")
                                        .into_any_element(),
                                    ScheduleRunStatus::Running => div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child("Running")
                                        .into_any_element(),
                                    ScheduleRunStatus::Waiting => div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child("Waiting on you")
                                        .into_any_element(),
                                    ScheduleRunStatus::Other => div().into_any_element(),
                                })
                        }))
                        .into_any_element()
                }),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn triggers_box(
        &self,
        triggers: &[RoutineTrigger],
        coworker_id: &str,
        routine_id: Option<String>,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
        persist: Rc<dyn Fn(&mut App) + 'static>,
        can_change: bool,
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
                        triggers.iter().map(move |trigger| {
                            if let RoutineTrigger::Webhook {
                                id,
                                url,
                                key,
                                header,
                            } = trigger
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
                                trigger_row(trigger, muted)
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
                    can_change,
                ))
            })
    }
}

/// What the person typed that a server unable to change a routine refused, beside the fields,
/// which went back to the server's values. Their words are not lost to a refusal they had no
/// part in, and nothing on screen claims they were saved.
fn unsaved_block(
    routine_id: &str,
    edit: &ScheduleEdit,
    muted: Hsla,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    v_flex()
        .id(SharedString::from(format!("routine-{routine_id}-unsaved")))
        .w_full()
        .gap(px(6.))
        .p(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .child(div().text_xs().text_color(theme.danger).child("Not saved"))
        .children(unsaved_lines(edit).into_iter().map(|(label, value)| {
            v_flex()
                .w_full()
                .child(div().text_xs().text_color(muted).child(label))
                .child(div().text_sm().child(value))
        }))
}

/// The space between the routine editor's controls, across and down alike.
const ACTION_GAP: f32 = 8.;

/// The routine editor's row of controls: the Active switch, then Delete, Open thread and Test
/// run.
///
/// It wraps rather than running on past the pane's edge. On one line the four are wider than the
/// pane, and Test run, the last of them, was the one cut off. The buttons keep together after the
/// switch, beside it while they fit and under it once they do not, and wrap among themselves in
/// turn, every gap the same across and down, so each control is whole at whatever width the pane
/// is.
fn routine_actions(
    app: Entity<AppState>,
    coworker_id: String,
    id: Option<String>,
    active: bool,
    has_thread: bool,
    can_run: bool,
    persist: Rc<dyn Fn(&mut App)>,
) -> impl IntoElement {
    h_flex()
        .debug_selector(|| "routine-actions".into())
        .w_full()
        .flex_wrap()
        .justify_between()
        .gap(px(ACTION_GAP))
        .child(
            div().debug_selector(|| "routine-active".into()).child(
                Switch::new("routine-active")
                    .checked(active)
                    // Says what the position means, so off reads as a state the routine is in
                    // and not as a label the switch has lost.
                    .label(if active { "Active" } else { "Paused" })
                    .on_click({
                        let app = app.clone();
                        let coworker_id = coworker_id.clone();
                        let id = id.clone();
                        move |checked, _, cx| {
                            if let Some(id) = id.clone() {
                                app.update(cx, |state, cx| {
                                    state.set_routine_active(&coworker_id, &id, *checked, cx);
                                });
                            }
                        }
                    }),
            ),
        )
        .child(
            h_flex()
                .flex_wrap()
                .gap(px(ACTION_GAP))
                .when(id.is_some(), |this| {
                    this.child(
                        Button::new(SharedString::from(format!(
                            "routine-{}-delete",
                            id.as_deref().unwrap_or_default()
                        )))
                        .debug_selector(|| "routine-delete".into())
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
                // What the routine said each time it ran, and any card it is waiting on, are in
                // its own thread; only a routine the server has has one.
                .when(has_thread, |this| {
                    this.child(
                        Button::new("routine-open-thread")
                            .debug_selector(|| "routine-open-thread".into())
                            .ghost()
                            .label("Open thread")
                            .on_click({
                                let persist = persist.clone();
                                let app = app.clone();
                                let id = id.clone();
                                move |_, _, cx| {
                                    persist(cx);
                                    if let Some(id) = id.clone() {
                                        app.update(cx, |state, cx| {
                                            state.open_routine_thread(&id, cx);
                                        });
                                    }
                                }
                            }),
                    )
                })
                .child(
                    Button::new("routine-test")
                        .debug_selector(|| "routine-test".into())
                        .primary()
                        .label("Test run")
                        .disabled(!can_run)
                        .on_click(move |_, _, cx| {
                            persist(cx);
                            if let Some(id) = id.clone() {
                                app.update(cx, |state, cx| {
                                    state.run_routine_now(&coworker_id, &id, cx);
                                });
                            }
                        }),
                ),
        )
}

/// What the Update, Reset and Get a computer controls need to know, read once per frame.
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
    /// Why the server could not give this Bot a computer, in its words ([`no_computer_line`]),
    /// shown in place of "No computer yet": a turn will not attach a box the server has just
    /// said it cannot make. Gone with the first status that names a box.
    pub no_computer: Option<String>,
    /// Get a computer is with the server; the button waits.
    pub asking: bool,
}

/// The button under a reason the server gave for a Bot having no computer, which asks it again.
pub const GET_A_COMPUTER: &str = "Get a computer";

/// The line between that reason and the button.
pub const GET_A_COMPUTER_AGAIN: &str = "Get a computer tries again.";

/// The server's reason for a Bot having no computer, as the pane shows it: its own sentence,
/// begun with a capital like the pane's other lines.
pub fn no_computer_line(error: &ComputerError) -> String {
    sentence(&error.message)
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
            no_computer: state
                .coworker_computer
                .as_ref()
                .and_then(CoworkerComputer::why_no_computer)
                .map(no_computer_line),
            asking: state.asking_for_computer(),
        }
    }

    /// Nothing to update, or nothing to update to: the Update button waits.
    pub fn update_disabled(&self) -> bool {
        !self.present || self.updating || (self.current && !self.stale)
    }
}

/// The way from this screen to the tasks taught on it: the Recipes page, in the main slot.
/// A cake in the header row, beside the network chrome, where the pane's actions live.
fn recipes_icon(app: Entity<AppState>, theme: &gpui_kit::component::Theme) -> impl IntoElement {
    let color = theme.muted_foreground;
    div()
        .id("computer-recipes")
        .size(px(28.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
        .tooltip(|window, cx| {
            Tooltip::new("Recipes: tasks taught on this screen").build(window, cx)
        })
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            app.update(cx, |state, cx| state.open_recipes(cx));
        })
        .child(
            Icon::default()
                .path("icons/cake.svg")
                .size(px(16.))
                .text_color(color),
        )
}

/// Grok **Needs your attention** while Open the screen is live.
/// Skip this step = decline; I'm done, continue = hand back.
/// Shared by the Computer pane and the launched noVNC window.
///
/// Glass: translucent peach/bronze over the sidebar (`theme.sidebar` shows
/// through). GPUI has no element backdrop-filter; alpha + warm shadow is the
/// frost. Orange is the title only — I'm done is a black (light) / white (dark) pill.
#[allow(clippy::too_many_arguments)]
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

/// Why the server could not give this Bot a computer, in its words and in the red of the pane's
/// other refusals, and the way to ask it again. Get a computer is the one control on the pane
/// that asks for a box while there is none: Update and Reset act on a box, and the pane asks by
/// itself only once per visit. It waits while an ask is with the server, as two at once could
/// each make a box.
fn no_computer_notice(
    why: String,
    asking: bool,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .id("computer-error")
                .w_full()
                .text_center()
                .text_xs()
                .text_color(theme.danger)
                .child(why),
        )
        .child(
            div()
                .w_full()
                .text_center()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(GET_A_COMPUTER_AGAIN),
        )
        .child(
            Button::new("computer-get")
                .label(GET_A_COMPUTER)
                .small()
                .loading(asking)
                .disabled(asking)
                .on_click(move |_, _, cx| {
                    app.update(cx, |state, cx| state.ensure_coworker_computer(cx));
                }),
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
    let network_policy = app
        .read(cx)
        .show_egress_policy_on_bot_pane()
        .then(|| app.read(cx).egress_policy())
        .flatten();
    h_flex()
        .id("computer-box-chrome")
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(
            h_flex()
                .items_center()
                .gap(px(2.))
                .when(show_route, |this| {
                    this.child(route_traffic_icon(app.clone(), theme, cx))
                })
                .when_some(network_policy, |this, current| {
                    this.child(network_policy_icon(app.clone(), current, theme))
                })
                // A task is taught on this screen, so the page that keeps those tasks belongs
                // next to it: the cake is the way in from where a recipe is born and used.
                .child(recipes_icon(app.clone(), theme)),
        )
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

/// The shield beside Route traffic: how this bot's own computer may use the person's network.
/// Opens the dialog that picks it; the badge is tinted when the answer is not "ask".
fn network_policy_icon(
    app: Entity<AppState>,
    current: LocalExecMode,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    let color = match current {
        LocalExecMode::Ask => theme.muted_foreground,
        LocalExecMode::Always => theme.primary,
        LocalExecMode::Never => theme.danger,
    };
    let tip = format!("Use your network: {}. Click to change.", current.label());
    div()
        .id("network-policy")
        .size(px(28.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
        .on_mouse_down(MouseButton::Left, {
            move |_, _, cx| {
                app.update(cx, |state, cx| state.open_network_policy(cx));
            }
        })
        .child(
            Icon::default()
                .path("icons/shield-badge.svg")
                .size(px(16.))
                .text_color(color),
        )
}

fn route_traffic_icon(
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
    cx: &App,
) -> impl IntoElement {
    // Two different pictures, so the state is readable at a glance: the route arrows in green
    // while traffic is routed through this desktop, a plain globe when it goes out on its own.
    let enabled = app.read(cx).egress_tunnel_enabled;
    let (icon, color) = if enabled {
        ("icons/route-traffic.svg", theme.success)
    } else {
        ("icons/globe.svg", theme.muted_foreground)
    };
    let tip = if enabled {
        "Routing traffic through this computer. New connections go out through this desktop. Click to stop."
    } else {
        "Traffic goes out on its own. Click to route this Bot's computer's web traffic through this desktop instead."
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
        .child(Icon::default().path(icon).size(px(16.)).text_color(color))
}

#[allow(clippy::type_complexity)]
fn pane_header(
    back: Option<Rc<dyn Fn(&mut App)>>,
    title: &'static str,
    actions: Option<&ComputerControls>,
    app: Entity<AppState>,
    drag: bool,
) -> impl IntoElement {
    // Routine back remains in the pane. Update / Reset sit next to the screen
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
        .when(drag, |this| {
            this.child(icon_btn(
                "computer-close",
                "icons/panel-right.svg",
                move |cx| {
                    app.update(cx, |state, cx| state.close_right_pane(cx));
                },
            ))
        })
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

fn field_textarea(state: &Entity<TextareaState>, theme: &gpui_kit::component::Theme) -> Textarea {
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
#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::type_complexity)]
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

#[allow(clippy::too_many_arguments)]
fn schedule_editor(
    coworker_id: String,
    routine_id: Option<String>,
    trigger_id: String,
    spec: ScheduleSpec,
    app: Entity<AppState>,
    custom_cron: Entity<InputState>,
    muted: Hsla,
    theme: &gpui_kit::component::Theme,
    enabled: bool,
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
        .child(mode_row(spec.clone(), patch.clone(), enabled))
        .child(match spec.mode {
            ScheduleUiMode::Interval => interval_row(spec, patch, enabled).into_any_element(),
            ScheduleUiMode::Custom => field_input(&custom_cron)
                .disabled(!enabled)
                .into_any_element(),
            ScheduleUiMode::Advanced => {
                advanced_editor(spec, patch, muted, enabled).into_any_element()
            }
        })
}

#[allow(clippy::type_complexity)]
fn mode_row(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    enabled: bool,
) -> impl IntoElement {
    let label = match spec.mode {
        ScheduleUiMode::Interval => "Interval",
        ScheduleUiMode::Custom => "Custom",
        ScheduleUiMode::Advanced => "Advanced",
    };
    Button::new("sched-mode")
        .ghost()
        .label(label)
        .disabled(!enabled)
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

#[allow(clippy::type_complexity)]
fn interval_row(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    enabled: bool,
) -> impl IntoElement {
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
                .disabled(!enabled)
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
                .disabled(!enabled)
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

#[allow(clippy::type_complexity)]
fn advanced_editor(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    muted: Hsla,
    enabled: bool,
) -> impl IntoElement {
    v_flex()
        .w_full()
        .gap(px(8.))
        .child(
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(div().text_xs().text_color(muted).w(px(52.)).child("Months"))
                .child(months_menu(spec.clone(), patch.clone(), enabled)),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(div().text_xs().text_color(muted).w(px(52.)).child("Days"))
                .child(days_menu(spec.clone(), patch.clone(), enabled))
                .when(spec.day_kind == ScheduleDayKind::DaysOfMonth, |this| {
                    this.child(month_day_menu(spec.clone(), patch.clone(), enabled))
                }),
        )
        .when(spec.day_kind == ScheduleDayKind::Weekdays, |this| {
            this.child(weekday_chips(spec.clone(), patch.clone(), enabled))
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
                        .children(spec.times.iter().enumerate().map(|(i, (h, m))| {
                            time_row(i, *h, *m, spec.clone(), patch.clone(), enabled)
                        }))
                        .child({
                            let spec = spec.clone();
                            let patch = patch.clone();
                            div()
                                .id("add-time")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(6.))
                                .when(enabled, |this| {
                                    this.cursor_pointer()
                                        .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
                                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                            let mut next = spec.clone();
                                            next.times.push((9, 0));
                                            patch(next, cx);
                                        })
                                })
                                .when(!enabled, |this| this.opacity(0.5))
                                .child(div().text_sm().child("+ Add time"))
                        }),
                ),
        )
}

#[allow(clippy::type_complexity)]
fn months_menu(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    enabled: bool,
) -> impl IntoElement {
    let label = if spec.months.is_empty() {
        "Any month"
    } else {
        "Selected months"
    };
    Button::new("sched-months")
        .ghost()
        .label(label)
        .disabled(!enabled)
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

#[allow(clippy::type_complexity)]
fn days_menu(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    enabled: bool,
) -> impl IntoElement {
    let label = match spec.day_kind {
        ScheduleDayKind::EveryDay => "Every day",
        ScheduleDayKind::Weekdays => "Days of the week",
        ScheduleDayKind::DaysOfMonth => "Days of the month",
    };
    Button::new("sched-days")
        .ghost()
        .label(label)
        .disabled(!enabled)
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

#[allow(clippy::type_complexity)]
fn month_day_menu(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    enabled: bool,
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
        .disabled(!enabled)
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

#[allow(clippy::type_complexity)]
fn weekday_chips(
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    enabled: bool,
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
            .when(enabled, |this| {
                this.cursor_pointer()
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
            })
            .when(!enabled, |this| this.opacity(0.5))
            .child(div().text_xs().child(names[d as usize]))
    }))
}

#[allow(clippy::type_complexity)]
fn time_row(
    index: usize,
    hour: u8,
    minute: u8,
    spec: ScheduleSpec,
    patch: Rc<dyn Fn(ScheduleSpec, &mut App)>,
    enabled: bool,
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
                .disabled(!enabled)
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
                .when(enabled, |this| {
                    this.cursor_pointer().on_mouse_down(MouseButton::Left, {
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
                })
                .when(!enabled, |this| this.opacity(0.5))
                .child(div().text_sm().child("×")),
        )
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob: `use super::*` would pull GPUI's `test` attribute in over the
    // one the test harness wants.
    use super::{AppState, ComputerControls, CoworkerComputer};

    fn recorded(recording: &str) -> CoworkerComputer {
        let fixture: serde_json::Value = serde_json::from_str(recording).expect("the recording");
        serde_json::from_value(fixture["body"].clone()).expect("the status")
    }

    /// A bot the server could not give a computer: the pane says the server's reason, in its
    /// words, where it said "No computer yet", with Get a computer live under it, since Update
    /// and Reset have no box to act on. The first status that names a box takes it away.
    #[test]
    fn the_pane_says_why_there_is_no_computer_until_a_box_comes() {
        let mut state = AppState::new();
        state.active_coworker_id = Some("cw_0199bb4e-0000-7000-8000-000000000001".into());
        state.coworker_computer = Some(recorded(include_str!(
            "../../fixtures/wire/rest/GET__coworkers__coworker_id__computer/200-a_hosted_hire_whose_ascii_create_fails_records_why_and_makes_no_box.json"
        )));
        let controls = ComputerControls::from_state(&state);
        assert_eq!(
            controls.no_computer.as_deref(),
            Some("The box refused: 429 {\"error\":\"box creation rate limit reached\"}")
        );
        assert!(!controls.asking, "Get a computer is live");
        assert!(!controls.present, "Update and Reset wait");

        state.coworker_computer = Some(recorded(include_str!(
            "../../fixtures/wire/rest/GET__coworkers__coworker_id__computer/200-a_healthy_local_vm_does_not_wear_another_scopes_failure.json"
        )));
        let controls = ComputerControls::from_state(&state);
        assert_eq!(controls.no_computer, None);
        assert!(controls.present);
    }

    /// The routine editor's row of controls as the pane lays it out: as wide as the pane's
    /// content, which is the pane less 16px either side.
    struct ActionRow {
        app: gpui_kit::Entity<AppState>,
        active: bool,
    }

    impl gpui_kit::Render for ActionRow {
        fn render(
            &mut self,
            _: &mut gpui_kit::Window,
            _: &mut gpui_kit::Context<Self>,
        ) -> impl gpui_kit::IntoElement {
            use gpui_kit::{ParentElement as _, Styled as _};
            gpui_kit::div()
                .w(gpui_kit::px(crate::chrome::INFO_PANE_WIDTH - 32.))
                .child(super::routine_actions(
                    self.app.clone(),
                    "cw_1".into(),
                    Some("sch_1".into()),
                    self.active,
                    true,
                    true,
                    std::rc::Rc::new(|_| {}),
                ))
        }
    }

    /// Every one of the routine editor's controls is whole inside the pane, whichever way the
    /// switch reads. On one line the four ran past the pane's edge and Test run, the last, was cut
    /// off; now the buttons go under the switch together and wrap among themselves, every gap the
    /// same across and down.
    #[gpui_kit::test]
    fn every_routine_control_is_whole_inside_the_pane(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::{AppContext as _, Bounds, Pixels, px};
        cx.update(gpui_kit::init);
        let gap = px(super::ACTION_GAP);
        let close = |a: Pixels, b: Pixels| (a - b).abs() < px(0.5);
        for active in [true, false] {
            let (_, window) = cx.add_window_view(|_, cx| ActionRow {
                app: cx.new(|_| AppState::new()),
                active,
            });
            window.update(|window, cx| window.draw(cx).clear(cx));
            let mut drawn = |id: &'static str| -> Bounds<Pixels> {
                window
                    .debug_bounds(id)
                    .unwrap_or_else(|| panic!("{id} is not drawn"))
            };
            let row = drawn("routine-actions");
            let controls = [
                "routine-active",
                "routine-delete",
                "routine-open-thread",
                "routine-test",
            ]
            .map(|id| (id, drawn(id)));
            for (id, control) in &controls {
                assert!(
                    control.left() >= row.left() && control.right() <= row.right(),
                    "{id} runs past the pane (switch {active}): {control:?} in {row:?}"
                );
            }
            let switch = controls[0].1;
            let delete = controls[1].1;
            assert!(
                close(delete.top() - switch.bottom(), gap),
                "the buttons go under the switch, {gap:?} below it: {switch:?}, {delete:?}"
            );
            for pair in controls[1..].windows(2) {
                let ((before_id, before), (after_id, after)) = (pair[0], pair[1]);
                if close(after.top(), before.top()) {
                    assert!(
                        close(after.left() - before.right(), gap),
                        "{before_id} and {after_id} are not {gap:?} apart: {before:?}, {after:?}"
                    );
                } else {
                    assert!(
                        close(after.top() - before.bottom(), gap)
                            && close(after.left(), row.left()),
                        "{after_id} does not wrap {gap:?} under {before_id}: {before:?}, {after:?}"
                    );
                }
            }
        }
    }
}
