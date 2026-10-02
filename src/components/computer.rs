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
    AgentRoutine, AppState, ComputerView, LAST_WAKE_STAYS, NUMBERED_WEEKDAYS, ONE_WAKE_PER_ROUTINE,
    ROUTINE_RUN_UNAVAILABLE, ROUTINE_RUNS_UNAVAILABLE, RoutineTrigger, ScheduleUnit, WakeBox,
    WakeEditor, WakeTab, routine_notes, routine_trouble_line, unsaved_lines,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub struct ComputerPane {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    instruction_input: Entity<TextareaState>,
    /// The routine the fields were last filled from, and the `routine_resync` they were
    /// filled at.
    loaded_editor: Option<(Option<String>, u64)>,
    /// The wake editor's typed boxes: the Every number, the hour, the minute and the Cron line.
    wake_boxes: [(WakeBox, Entity<InputState>); 4],
    /// The opening of the wake editor the boxes were last written for, and its `resync` then.
    loaded_wake: Option<(u64, u64)>,
}

/// One of the wake editor's typed boxes, empty, saying what goes in it.
fn wake_input(
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<ComputerPane>,
) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

impl ComputerPane {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Name this routine"));
        let instruction_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What should this routine do each time it runs?")
                .auto_grow(3, 8)
        });
        let wake_boxes = [
            (WakeBox::Every, wake_input("30", window, cx)),
            (WakeBox::Hour, wake_input("9", window, cx)),
            (WakeBox::Minute, wake_input("00", window, cx)),
            // A real line, and one the server reads as meant: it counts days of the week from
            // Sunday as 1, so a numbered example would teach the wrong days.
            (WakeBox::Cron, wake_input("0 9 * * MON-FRI", window, cx)),
        ];
        // What a person types goes to the editor as it is typed. A value written into a box from
        // outside it (`set_value`) says nothing, so the box and the editor never chase each other.
        for (which, input) in &wake_boxes {
            let which = *which;
            cx.subscribe_in(
                input,
                window,
                move |this, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let text = input.read(cx).value().to_string();
                        this.state
                            .update(cx, |state, cx| state.set_wake_box(which, text, true, cx));
                    }
                },
            )
            .detach();
        }
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        Self {
            state,
            name_input,
            instruction_input,
            loaded_editor: None,
            wake_boxes,
            loaded_wake: None,
        }
    }

    /// The wake editor's boxes, written from what it holds when it opens or a value changes from
    /// outside them; never while a person types in one, which is the box's own.
    fn sync_wake(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.state.read(cx).routine_wake_editor.clone() else {
            self.loaded_wake = None;
            return;
        };
        let key = (editor.opened, editor.resync);
        if self.loaded_wake == Some(key) {
            return;
        }
        self.loaded_wake = Some(key);
        for (which, input) in &self.wake_boxes {
            let text = match which {
                WakeBox::Every => editor.every.clone(),
                WakeBox::Hour => editor.hour.clone(),
                WakeBox::Minute => editor.minute.clone(),
                WakeBox::Cron => editor.spec.expr.clone(),
            };
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    fn wake_box(&self, which: WakeBox) -> Entity<InputState> {
        self.wake_boxes
            .iter()
            .find(|(box_of, _)| *box_of == which)
            .map(|(_, input)| input.clone())
            .expect("every box is made with the pane")
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
    }
}

impl Render for ComputerPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_editor(window, cx);
        self.sync_wake(window, cx);
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

        // On the chat page the pane reaches the window's top edge, under the floating title bar,
        // so its header is the bar's row over the pane: one row, overview and routine editor
        // alike, which drags the window as a title bar does. The bar's window-level pane toggle
        // replaces the row's close control there. Other pages still need their own header,
        // close control and all, when the pane floats over their content.
        let chat_page = app.read(cx).page == crate::state::MainPage::Chat;
        v_flex()
            .id("computer-pane")
            .h_full()
            .w(px(INFO_PANE_WIDTH))
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .text_color(theme.foreground)
            .when(chat_page, |this| this.child(self.header(cx, false)))
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
        Rc::new(move |cx: &mut App| {
            let Some(rid) = id.clone() else {
                return;
            };
            let name = name_input.read(cx).value().to_string();
            let instruction = instruction_input.read(cx).value().to_string();
            app.update(cx, |state, cx| {
                state.save_routine_fields(&coworker_id, &rid, name, instruction, cx);
            });
        })
    }

    /// The pane's header row, whose title and empty run drag the window: in the title bar over
    /// the pane while it is docked beside another page, and at the top of the pane itself on the
    /// chat page or while it floats. With `close`, the row ends in the control that closes the
    /// pane; the chat page leaves it off, because the chat's title bar floats its one
    /// window-level pane toggle, and the computer's button beside it, over that end of the row,
    /// which the row keeps clear of its own controls.
    /// Overview: nothing else (Update / Reset sit next to the screen). Routine: back, title and
    /// the routine's four icons.
    pub fn header(&self, cx: &App, close: bool) -> AnyElement {
        let app = self.state.clone();
        let state = self.state.read(cx);
        match state.computer_view.clone() {
            ComputerView::Overview => {
                pane_header(None, "", None, None, app, close).into_any_element()
            }
            ComputerView::Editor { id } => {
                let coworker_id = state.active_coworker_id.clone().unwrap_or_default();
                let persist = self.persist_routine(app.clone(), coworker_id.clone(), id.clone());
                let back = {
                    let app = app.clone();
                    let persist = persist.clone();
                    Rc::new(move |cx: &mut App| {
                        persist(cx);
                        app.update(cx, |state, cx| state.back_to_computer(cx));
                    }) as Rc<dyn Fn(&mut App)>
                };
                let on_the_server = id.as_ref().is_some_and(|rid| {
                    state
                        .coworker_routines(&coworker_id)
                        .iter()
                        .any(|row| &row.id == rid && row.saved.is_some())
                });
                let icons = id.map(|id| {
                    routine_icons(RoutineIcons {
                        app: app.clone(),
                        coworker_id,
                        id,
                        on_the_server,
                        can_run: !state.routine_routes_missing.run_now,
                        history_open: state.routine_history_open,
                        persist,
                        theme: cx.theme().clone(),
                    })
                    .into_any_element()
                });
                pane_header(Some(back), "Routine", None, icons, app, close).into_any_element()
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
        let rid = id.clone().unwrap_or_default();
        // What this server cannot do, said beside the controls it leaves dead, for as long as it
        // is so: a control that does nothing must say why it does nothing. Test run's is in the
        // header, as its icon's tooltip and here.
        let notes = routine_notes(missing, on_the_server);
        // The same line the overview puts a refused Update on. A routine's calls go out from
        // this page, so their refusals have to be readable from it.
        let trouble = routine_trouble_line(app.read(cx).computer_action_error.as_deref(), &notes);
        let persist = self.persist_routine(app.clone(), coworker_id.clone(), id.clone());

        let history_open = app.read(cx).routine_history_open;
        let notes = notes.into_iter().map(|(tail, note)| {
            div()
                .id(SharedString::from(format!("routine-{rid}-{tail}")))
                .w_full()
                .text_xs()
                .text_color(muted)
                .child(note)
        });
        let trouble = trouble.map(|trouble| {
            div()
                .id("routine-error")
                .w_full()
                .text_xs()
                .text_color(theme.danger)
                .child(trouble)
        });

        v_flex().size_full().child(
            v_flex()
                .id("routine-editor-scroll")
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .px(px(16.))
                .py(px(12.))
                .gap(px(16.))
                // What happened to the last thing asked of the routine, and what the server
                // cannot do with it, are said over either view: the header's icons, which they
                // are about, are over both.
                .children(notes)
                .children(trouble)
                .map(|this| {
                    if history_open {
                        // The Run history, and nothing else: the routine's fields are behind the
                        // header's first icon, which now says so.
                        this.child(routine_history(
                            &rid,
                            runs,
                            on_the_server && missing.runs,
                            muted,
                            app.clone(),
                            theme,
                            persist.clone(),
                        ))
                    } else {
                        this.child(
                            div().debug_selector(|| "routine-active".into()).child(
                                Switch::new("routine-active")
                                    .checked(active)
                                    // Says what the position means, so off reads as a state the
                                    // routine is in and not as a label the switch has lost.
                                    .label(if active { "Active" } else { "Paused" })
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
                            ),
                        )
                        .when_some(unsaved, |this, edit| {
                            this.child(unsaved_block(&rid, &edit, muted, theme))
                        })
                        .child(field_label("Name", muted))
                        .child(
                            div()
                                .debug_selector(|| "routine-name".into())
                                .child(field_input(&self.name_input).disabled(!can_change)),
                        )
                        .child(field_label("Instruction", muted))
                        .child(field_textarea(&self.instruction_input, theme).disabled(!can_change))
                        .child(self.wake_section(
                            existing.as_ref(),
                            can_change,
                            muted,
                            app.clone(),
                            theme,
                            cx,
                        ))
                    }
                }),
        )
    }

    /// "When to run": the routine's wakes, one row each, with + to add one, and in their place
    /// the wake editor while one is being picked.
    fn wake_section(
        &self,
        routine: Option<&AgentRoutine>,
        can_change: bool,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
        cx: &App,
    ) -> AnyElement {
        let editor = app
            .read(cx)
            .routine_wake_editor
            .clone()
            .filter(|editor| routine.is_some_and(|row| row.id == editor.routine_id));
        let can_add = routine.is_some_and(AppState::can_add_wake);
        let routine_id = routine.map(|row| row.id.clone()).unwrap_or_default();
        let heading = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .child(field_label("When to run", muted))
            // One wake per routine is the server's rule today (`ONE_WAKE_PER_ROUTINE`);
            // opengrok-server#315 lifts it, and is what turns + on for a routine that has one.
            .child(routine_icon(
                "routine-wake-add",
                "icons/plus.svg",
                if can_add {
                    "Choose when it runs"
                } else {
                    ONE_WAKE_PER_ROUTINE
                },
                can_add && editor.is_none(),
                false,
                theme,
                {
                    let app = app.clone();
                    let routine_id = routine_id.clone();
                    move |cx| {
                        app.update(cx, |state, cx| {
                            state.open_wake_editor(&routine_id, None, cx)
                        })
                    }
                },
            ));
        v_flex()
            .w_full()
            .gap(px(8.))
            .child(heading)
            .child(match editor {
                Some(editor) => self
                    .wake_editor(&editor, routine, muted, app, theme)
                    .into_any_element(),
                None => wake_rows(routine, can_change, muted, app, theme),
            })
            .into_any_element()
    }

    /// The wake editor: a tab for each shape of schedule, what that tab picks, one sentence
    /// saying exactly what was picked and when it would next run, and Cancel and Save. Nothing
    /// reaches the routine before Save.
    fn wake_editor(
        &self,
        editor: &WakeEditor,
        routine: Option<&AgentRoutine>,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
    ) -> impl IntoElement {
        let status = editor.status(chrono::Utc::now());
        let time = || self.time_picker(editor, muted, app.clone(), theme);
        let body = match editor.tab {
            WakeTab::Every => self
                .every_picker(editor, muted, app.clone(), theme)
                .into_any_element(),
            WakeTab::Daily => v_flex()
                .gap(px(12.))
                .child(time())
                .child(month_chips(editor, muted, app.clone(), theme))
                .into_any_element(),
            WakeTab::Weekly => v_flex()
                .gap(px(12.))
                .child(weekday_chips(editor, muted, app.clone(), theme))
                .child(time())
                .child(month_chips(editor, muted, app.clone(), theme))
                .into_any_element(),
            WakeTab::Monthly => v_flex()
                .gap(px(12.))
                .child(date_grid(editor, muted, app.clone(), theme))
                .child(time())
                .child(month_chips(editor, muted, app.clone(), theme))
                .into_any_element(),
            WakeTab::Webhook => webhook_details(routine, muted, app.clone(), theme),
            WakeTab::Cron => v_flex()
                .gap(px(6.))
                .child(field_label(
                    "Minute, hour, day of the month, month and day of the week",
                    muted,
                ))
                .child(
                    div()
                        .debug_selector(|| "routine-wake-cron".into())
                        .child(field_input(&self.wake_box(WakeBox::Cron))),
                )
                .when(status.numbered_weekdays, |this| {
                    this.child(
                        div()
                            .id("routine-wake-cron-note")
                            .text_xs()
                            .text_color(muted)
                            .child(NUMBERED_WEEKDAYS),
                    )
                })
                .into_any_element(),
        };
        let done = if editor.kind == Some(crate::opengrok::ScheduleKind::Webhook) {
            "Done"
        } else {
            "Save"
        };
        v_flex()
            .id("routine-wake-editor")
            .debug_selector(|| "routine-wake-editor".into())
            .w_full()
            .gap(px(12.))
            .p(px(10.))
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border)
            .child(wake_tabs(editor, app.clone(), theme))
            .child(body)
            .child(
                div()
                    .id("routine-wake-summary")
                    .debug_selector(|| "routine-wake-summary".into())
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(status.summary.clone()),
            )
            .when_some(status.next.clone(), |this, next| {
                this.child(
                    div()
                        .id("routine-wake-next")
                        .text_xs()
                        .text_color(muted)
                        .child(next),
                )
            })
            .when_some(status.error.clone(), |this, error| {
                this.child(
                    div()
                        .id("routine-wake-error")
                        .text_xs()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("routine-wake-cancel")
                            .ghost()
                            .label("Cancel")
                            .on_click({
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| state.close_wake_editor(cx))
                                }
                            }),
                    )
                    .child(
                        Button::new("routine-wake-save")
                            .debug_selector(|| "routine-wake-save".into())
                            .primary()
                            .label(done)
                            .disabled(!status.can_save())
                            .on_click({
                                let name_input = self.name_input.clone();
                                let instruction_input = self.instruction_input.clone();
                                move |_, _, cx| {
                                    // What is typed for the routine goes with its wake: a
                                    // routine's first wake is what makes it on the server.
                                    let name = name_input.read(cx).value().to_string();
                                    let instruction =
                                        instruction_input.read(cx).value().to_string();
                                    app.update(cx, |state, cx| {
                                        state.save_wake(name, instruction, cx);
                                    });
                                }
                            }),
                    ),
            )
    }

    /// The Every tab: a number, and minutes, hours or days.
    fn every_picker(
        &self,
        editor: &WakeEditor,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(8.))
            .items_center()
            .child(div().text_sm().text_color(muted).child("Every"))
            .child(number_box(
                "routine-wake-every",
                &self.wake_box(WakeBox::Every),
                WakeBox::Every,
                app.clone(),
                theme,
            ))
            .child(segments(
                [
                    ("routine-wake-unit-minutes", "min", ScheduleUnit::Minutes),
                    ("routine-wake-unit-hours", "hours", ScheduleUnit::Hours),
                    ("routine-wake-unit-days", "days", ScheduleUnit::Days),
                ]
                .map(|(id, label, unit)| {
                    let app = app.clone();
                    Segment {
                        id,
                        label,
                        selected: editor.spec.unit == unit,
                        on_press: Rc::new(move |cx: &mut App| {
                            app.update(cx, |state, cx| state.set_wake_unit(unit, cx))
                        }),
                    }
                }),
                theme,
            ))
    }

    /// A time of day: the hour and the minute, typed or stepped, and AM or PM. On the server's
    /// clock, which is UTC, and the label says so.
    fn time_picker(
        &self,
        editor: &WakeEditor,
        muted: Hsla,
        app: Entity<AppState>,
        theme: &gpui_kit::component::Theme,
    ) -> impl IntoElement {
        v_flex()
            .gap(px(4.))
            .child(field_label("Time (UTC)", muted))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .items_center()
                    .child(number_box(
                        "routine-wake-hour",
                        &self.wake_box(WakeBox::Hour),
                        WakeBox::Hour,
                        app.clone(),
                        theme,
                    ))
                    .child(div().text_sm().child(":"))
                    .child(number_box(
                        "routine-wake-minute",
                        &self.wake_box(WakeBox::Minute),
                        WakeBox::Minute,
                        app.clone(),
                        theme,
                    ))
                    .child(segments(
                        [
                            ("routine-wake-am", "AM", false),
                            ("routine-wake-pm", "PM", true),
                        ]
                        .map(|(id, label, pm)| {
                            let app = app.clone();
                            Segment {
                                id,
                                label,
                                selected: editor.pm == pm,
                                on_press: Rc::new(move |cx: &mut App| {
                                    app.update(cx, |state, cx| state.set_wake_pm(pm, cx))
                                }),
                            }
                        }),
                        theme,
                    )),
            )
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

/// A routine's Run history: one line per run, newest first, each opening the routine's thread
/// at that run. In its place, on a server that cannot list a routine's runs, the sentence saying
/// so; on a routine with none, "No runs yet".
#[allow(clippy::too_many_arguments)]
fn routine_history(
    routine_id: &str,
    runs: Vec<crate::state::RoutineRun>,
    unavailable: bool,
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
    persist: Rc<dyn Fn(&mut App)>,
) -> AnyElement {
    if unavailable {
        // Not "No runs yet": nobody knows that, because this server cannot say.
        return div()
            .id(SharedString::from(format!(
                "routine-{routine_id}-runs-unavailable"
            )))
            .text_sm()
            .text_color(muted)
            .child(ROUTINE_RUNS_UNAVAILABLE)
            .into_any_element();
    }
    if runs.is_empty() {
        return div()
            .debug_selector(|| "routine-history".into())
            .text_sm()
            .text_color(muted)
            .child("No runs yet")
            .into_any_element();
    }
    v_flex()
        .debug_selector(|| "routine-history".into())
        .w_full()
        .gap(px(6.))
        .children(runs.into_iter().enumerate().map(|(i, run)| {
            let routine_id = routine_id.to_string();
            h_flex()
                .id(SharedString::from(format!("run-{i}")))
                // A line of the history opens the thread it ran in at that run, which is where
                // what it said is. On click, not on press, so a scroll that starts on a line
                // stays in the panel.
                .cursor_pointer()
                .on_click({
                    let persist = persist.clone();
                    let app = app.clone();
                    let run_id = run.run_id.clone();
                    move |_, _, cx| {
                        persist(cx);
                        app.update(cx, |state, cx| {
                            state.open_routine_run(&routine_id, &run_id, cx);
                        });
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
                        .child(div().text_xs().text_color(muted).child(run.cause_label())),
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
}

/// What a routine's header icons act on, and whether each can.
struct RoutineIcons {
    app: Entity<AppState>,
    coworker_id: String,
    id: String,
    /// The server has the routine: it has a thread, and can be run.
    on_the_server: bool,
    /// The server can run a routine when asked.
    can_run: bool,
    /// The panel shows the Run history in place of the routine's fields.
    history_open: bool,
    persist: Rc<dyn Fn(&mut App)>,
    theme: gpui_kit::component::Theme,
}

/// A routine's four icons, on the right of its header: Run history, Open in thread, Run it now
/// and Delete. Icons, not words, so the four fit the panel's header at any width, each saying
/// what it does in its tooltip, and why when it cannot.
fn routine_icons(icons: RoutineIcons) -> impl IntoElement {
    let RoutineIcons {
        app,
        coworker_id,
        id,
        on_the_server,
        can_run,
        history_open,
        persist,
        theme,
    } = icons;
    let run_why = if !can_run {
        ROUTINE_RUN_UNAVAILABLE
    } else if !on_the_server {
        "Choose when it runs first, then run it"
    } else {
        "Run it now"
    };
    h_flex()
        .gap(px(2.))
        .items_center()
        .child(
            // Its fields again, while the history is shown: the icon says where it goes.
            routine_icon(
                "routine-history-toggle",
                if history_open {
                    "icons/list.svg"
                } else {
                    "icons/rotate-ccw-clock.svg"
                },
                if history_open {
                    "Back to the routine"
                } else {
                    "Run history"
                },
                true,
                history_open,
                &theme,
                {
                    let app = app.clone();
                    move |cx| app.update(cx, |state, cx| state.toggle_routine_history(cx))
                },
            ),
        )
        .child(routine_icon(
            "routine-open-thread",
            "icons/message-circle.svg",
            if on_the_server {
                "Open in thread"
            } else {
                "Choose when it runs first: its thread is on the server"
            },
            on_the_server,
            false,
            &theme,
            {
                let app = app.clone();
                let persist = persist.clone();
                let id = id.clone();
                move |cx| {
                    persist(cx);
                    app.update(cx, |state, cx| state.open_routine_thread(&id, cx));
                }
            },
        ))
        .child(routine_icon(
            "routine-run-now",
            "icons/play.svg",
            run_why,
            on_the_server && can_run,
            false,
            &theme,
            {
                let app = app.clone();
                let coworker_id = coworker_id.clone();
                let id = id.clone();
                move |cx| {
                    persist(cx);
                    app.update(cx, |state, cx| state.run_routine_now(&coworker_id, &id, cx));
                }
            },
        ))
        .child(routine_icon(
            "routine-delete",
            "icons/trash.svg",
            "Delete",
            true,
            false,
            &theme,
            move |cx| {
                // Asks first, over the whole window (`RoutineDeletePrompt`).
                app.update(cx, |state, cx| state.ask_delete_routine(&id, cx));
            },
        ))
}

/// One icon of a routine's header, with its tooltip. Dead and dimmed while what it does cannot
/// be done, the tooltip then saying why; filled while `selected`, for the one that is a state.
#[allow(clippy::too_many_arguments)]
fn routine_icon(
    id: impl Into<SharedString>,
    icon: &'static str,
    tooltip: impl Into<SharedString>,
    enabled: bool,
    selected: bool,
    theme: &gpui_kit::component::Theme,
    on_press: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let tooltip: SharedString = tooltip.into();
    div()
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .size(px(28.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |this| this.bg(theme.primary))
        .when(enabled, |this| {
            this.cursor_pointer()
                .when(!selected, |this| {
                    this.hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                })
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    // Not a press on the bar under it, which drags the window.
                    cx.stop_propagation();
                    on_press(cx);
                })
        })
        .when(!enabled, |this| this.opacity(0.35))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        .child(
            Icon::default()
                .path(icon)
                .size(px(16.))
                .text_color(if selected {
                    theme.primary_foreground
                } else {
                    theme.foreground
                }),
        )
}

/// The question Delete asks about a routine, centred over the whole window: the sidebar, the
/// chat and the panel dimmed under it, because it is about the routine for good and not about
/// the panel. Cancel, Escape or a press beside it leaves the routine as it is; Delete deletes it.
///
/// A view of its own, mounted by the root as the picture overlay is, so that it can hold focus
/// while it asks: Escape is bound in its own key context, which is only in the dispatch path
/// while it has focus, so it takes focus the moment it opens.
pub struct RoutineDeletePrompt {
    state: Entity<AppState>,
    focus_handle: FocusHandle,
    was_open: bool,
    pending_focus: bool,
}

impl RoutineDeletePrompt {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let open = state.read(cx).routine_delete_prompt.is_some();
        cx.observe(&state, |this, state, cx| {
            let open = state.read(cx).routine_delete_prompt.is_some();
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

impl Render for RoutineDeletePrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.pending_focus {
            self.pending_focus = false;
            self.focus_handle.focus(window, cx);
        }
        let Some((title, what_happens)) = self.state.read(cx).routine_delete_question() else {
            return div().into_any_element();
        };
        let theme = cx.theme().clone();
        let cancel = {
            let app = self.state.clone();
            move |cx: &mut App| app.update(cx, |state, cx| state.cancel_routine_delete(cx))
        };
        div()
            .id("routine-delete-overlay")
            .track_focus(&self.focus_handle)
            .key_context("RoutineDelete")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.32))
            .on_mouse_down(MouseButton::Left, {
                let cancel = cancel.clone();
                move |_, _, cx| cancel(cx)
            })
            .on_action({
                let cancel = cancel.clone();
                move |_: &crate::actions::CancelRoutineDelete, _: &mut Window, cx: &mut App| {
                    cancel(cx)
                }
            })
            .child(
                v_flex()
                    .id("routine-delete-dialog")
                    .debug_selector(|| "routine-delete-dialog".into())
                    .w(px(420.))
                    .bg(theme.popover)
                    .text_color(theme.foreground)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(px(14.))
                    .shadow_lg()
                    .px(px(20.))
                    .py(px(18.))
                    .gap(px(10.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(what_happens),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_end()
                            .gap(px(8.))
                            .pt(px(6.))
                            .child(
                                Button::new("routine-delete-cancel")
                                    .debug_selector(|| "routine-delete-cancel".into())
                                    .label("Cancel")
                                    .on_click(move |_, _, cx| cancel(cx)),
                            )
                            .child(
                                Button::new("routine-delete-confirm")
                                    .debug_selector(|| "routine-delete-confirm".into())
                                    .danger()
                                    .label("Delete")
                                    .on_click({
                                        let app = self.state.clone();
                                        move |_, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.confirm_routine_delete(cx)
                                            });
                                        }
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }
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
    trailing: Option<AnyElement>,
    app: Entity<AppState>,
    close: bool,
) -> impl IntoElement {
    use crate::components::title_bar::{PANE_ROW_UNDER_BUTTONS, window_drag};
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
        .debug_selector(|| "computer-header".into())
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
                    this.child(window_drag(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    ))
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
        // The empty run between the controls: a handle to drag the window by.
        .child(window_drag(div().flex_1().h_full()))
        .children(trailing)
        // Beside the chat, where the row has no close control, the chat's title bar floats its
        // two buttons over the row's right end. The row's own controls (a routine's four icons)
        // stop short of them, and the run under them drags the window as the empty run does.
        .when(!close, |this| {
            this.child(window_drag(
                div()
                    .flex_shrink_0()
                    .w(px(PANE_ROW_UNDER_BUTTONS - HEADER_PX))
                    .h_full(),
            ))
        })
        .when(close, |this| {
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

/// A routine's wakes, one row each: what sets it off in words, ✎ and 🗑.
fn wake_rows(
    routine: Option<&AgentRoutine>,
    can_change: bool,
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    let Some(routine) = routine else {
        return div().into_any_element();
    };
    if routine.triggers.is_empty() {
        return div()
            .id("routine-wakes-empty")
            .text_sm()
            .text_color(muted)
            .child("Not set yet: + chooses when it runs.")
            .into_any_element();
    }
    let can_remove = AppState::can_remove_wake(routine);
    v_flex()
        .w_full()
        .gap(px(6.))
        .children(routine.triggers.iter().enumerate().map(|(at, trigger)| {
            let webhook = matches!(trigger, RoutineTrigger::Webhook { .. });
            // ✎ on a schedule changes it, which a server that cannot change a routine refuses;
            // on a webhook it shows the address and the key, which any server can.
            let can_edit = webhook || can_change;
            h_flex()
                .id(SharedString::from(format!("routine-wake-{at}")))
                .w_full()
                .gap(px(8.))
                .items_center()
                .px(px(10.))
                .py(px(6.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border)
                .child(
                    Icon::default()
                        .path(if webhook {
                            "icons/webhook.svg"
                        } else {
                            "icons/clock.svg"
                        })
                        .size(px(14.))
                        .text_color(muted),
                )
                .child(div().flex_1().min_w_0().text_sm().child(trigger.label()))
                .child(routine_icon(
                    format!("routine-wake-edit-{at}"),
                    "icons/pencil.svg",
                    if !can_edit {
                        crate::state::ROUTINE_EDIT_UNAVAILABLE
                    } else if webhook {
                        "Its address and key"
                    } else {
                        "Change when it runs"
                    },
                    can_edit,
                    false,
                    theme,
                    {
                        let app = app.clone();
                        let routine_id = routine.id.clone();
                        move |cx| {
                            app.update(cx, |state, cx| {
                                state.open_wake_editor(&routine_id, Some(at), cx)
                            })
                        }
                    },
                ))
                // Taking one wake off a routine with several is opengrok-server#315's to offer;
                // until then a routine has one, and it stays.
                .child(routine_icon(
                    format!("routine-wake-delete-{at}"),
                    "icons/trash.svg",
                    if can_remove {
                        "Remove this schedule"
                    } else {
                        LAST_WAKE_STAYS
                    },
                    false,
                    false,
                    theme,
                    |_| {},
                ))
        }))
        .into_any_element()
}

/// The wake editor's tabs, in two rows of three so that all six fit the panel: the selected one
/// filled, and one the routine's kind rules out dead, saying why.
fn wake_tabs(
    editor: &WakeEditor,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    let tab = |tab: WakeTab| {
        let open = editor.tab_open(tab);
        let selected = editor.tab == tab;
        let id = format!("routine-wake-tab-{}", tab.word());
        let why = if tab == WakeTab::Webhook {
            "A routine stays a schedule once made: make a new routine for a webhook"
        } else {
            "A routine stays a webhook once made: make a new routine for a schedule"
        };
        let app = app.clone();
        div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id)
            .flex_1()
            .py(px(4.))
            .rounded(px(6.))
            .text_xs()
            .text_center()
            .when(selected, |this| {
                this.bg(theme.primary).text_color(theme.primary_foreground)
            })
            .when(open && !selected, |this| {
                this.cursor_pointer()
                    .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        app.update(cx, |state, cx| state.pick_wake_tab(tab, cx));
                    })
            })
            .when(!open, |this| {
                this.opacity(0.35)
                    .tooltip(move |window, cx| Tooltip::new(why).build(window, cx))
            })
            .child(tab.label())
    };
    v_flex()
        .w_full()
        .gap(px(4.))
        .p(px(3.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .child(
            h_flex()
                .w_full()
                .gap(px(4.))
                .children([WakeTab::Every, WakeTab::Daily, WakeTab::Weekly].map(tab)),
        )
        .child(
            h_flex()
                .w_full()
                .gap(px(4.))
                .children([WakeTab::Monthly, WakeTab::Webhook, WakeTab::Cron].map(&tab)),
        )
}

/// One choice of a segmented control.
struct Segment {
    id: &'static str,
    label: &'static str,
    selected: bool,
    on_press: Rc<dyn Fn(&mut App)>,
}

/// A row of choices, one of them picked: the Every tab's unit, AM and PM.
fn segments<const N: usize>(
    choices: [Segment; N],
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    h_flex()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .children(choices.map(|choice| {
            let Segment {
                id,
                label,
                selected,
                on_press,
            } = choice;
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .px(px(8.))
                .py(px(3.))
                .rounded(px(6.))
                .text_xs()
                .when(selected, |this| {
                    this.bg(theme.primary).text_color(theme.primary_foreground)
                })
                .when(!selected, |this| {
                    this.cursor_pointer()
                        .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| on_press(cx))
                })
                .child(label)
        }))
}

/// A number typed in a box, with ▲ and ▼ beside it to step it.
fn number_box(
    id: &'static str,
    input: &Entity<InputState>,
    which: WakeBox,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    let step = |up: bool| {
        let app = app.clone();
        let stepper = format!("{id}-{}", if up { "up" } else { "down" });
        div()
            .id(SharedString::from(stepper.clone()))
            .debug_selector(move || stepper)
            .w(px(18.))
            .h(px(14.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                app.update(cx, |state, cx| state.step_wake_box(which, up, cx));
            })
            .child(
                Icon::default()
                    .path(if up {
                        "icons/chevron-up.svg"
                    } else {
                        "icons/chevron-down.svg"
                    })
                    .size(px(12.))
                    .text_color(theme.foreground),
            )
    };
    h_flex()
        .gap(px(2.))
        .items_center()
        .child(
            div()
                .debug_selector(move || id.to_string())
                .w(px(52.))
                .child(field_input(input)),
        )
        .child(v_flex().child(step(true)).child(step(false)))
}

/// A chip that is in or out: a day of the week, a date, a month.
fn chip(
    id: String,
    label: impl Into<SharedString>,
    on: bool,
    theme: &gpui_kit::component::Theme,
    on_press: impl Fn(&mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id)
        .min_w(px(30.))
        .h(px(28.))
        .px(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_xs()
        .border_1()
        .border_color(if on { theme.primary } else { theme.border })
        .when(on, |this| {
            this.bg(theme.primary).text_color(theme.primary_foreground)
        })
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| on_press(cx))
        .child(label.into())
}

/// The Weekly tab's days: S M T W T F S, Sunday first, wrapping rather than running off the
/// panel.
fn weekday_chips(
    editor: &WakeEditor,
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    const LETTERS: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];
    v_flex()
        .gap(px(4.))
        .child(field_label("Days of the week", muted))
        .child(
            h_flex()
                .debug_selector(|| "routine-wake-days".into())
                .w_full()
                .flex_wrap()
                .gap(px(4.))
                .children((0u8..7).map(|day| {
                    let app = app.clone();
                    chip(
                        format!("routine-wake-day-{day}"),
                        LETTERS[usize::from(day)],
                        editor.spec.weekdays.contains(&day),
                        theme,
                        move |cx| app.update(cx, |state, cx| state.toggle_wake_weekday(day, cx)),
                    )
                    .tooltip(move |window, cx| {
                        Tooltip::new(crate::cron_spec::WEEKDAYS[usize::from(day)]).build(window, cx)
                    })
                })),
        )
}

/// The Monthly tab's dates: a grid seven wide, 1 to 31. No "last day": the server's parser has
/// no `L` (opengrok-server reads lines with the `cron` crate 0.17).
fn date_grid(
    editor: &WakeEditor,
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    v_flex()
        .debug_selector(|| "routine-wake-dates".into())
        .w_full()
        .gap(px(4.))
        .child(field_label("Days of the month", muted))
        .children((0u8..5).map(|week| {
            h_flex()
                .w_full()
                .gap(px(4.))
                .children((1..=7u8).map(|place| {
                    let date = week * 7 + place;
                    if date > 31 {
                        // The last row's empty places keep the grid's columns.
                        return div().flex_1().into_any_element();
                    }
                    let app = app.clone();
                    chip(
                        format!("routine-wake-date-{date}"),
                        date.to_string(),
                        editor.spec.month_days.contains(&date),
                        theme,
                        move |cx| app.update(cx, |state, cx| state.toggle_wake_date(date, cx)),
                    )
                    .flex_1()
                    .min_w(px(0.))
                    .into_any_element()
                }))
        }))
}

/// The months a schedule is kept to: twelve chips, any number picked, none picked for every
/// month, wrapping rather than running off the panel.
fn month_chips(
    editor: &WakeEditor,
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> impl IntoElement {
    v_flex()
        .gap(px(4.))
        .child(field_label("Months", muted))
        .child(
            h_flex()
                .debug_selector(|| "routine-wake-months".into())
                .w_full()
                .flex_wrap()
                .gap(px(4.))
                .children((1u8..=12).map(|month| {
                    let app = app.clone();
                    chip(
                        format!("routine-wake-month-{month}"),
                        crate::cron_spec::MONTHS[usize::from(month) - 1],
                        editor.spec.months.contains(&month),
                        theme,
                        move |cx| app.update(cx, |state, cx| state.toggle_wake_month(month, cx)),
                    )
                })),
        )
        .when(editor.spec.months.is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("None picked: every month."),
            )
        })
}

/// The Webhook tab: for a routine that is one, the address the server made for it, the key and
/// the header to send it with, each to copy, and a new key on asking. For a routine's first
/// wake, what a webhook is; the address comes with Save.
fn webhook_details(
    routine: Option<&AgentRoutine>,
    muted: Hsla,
    app: Entity<AppState>,
    theme: &gpui_kit::component::Theme,
) -> AnyElement {
    let hook = routine.and_then(|row| {
        row.triggers.iter().find_map(|trigger| match trigger {
            RoutineTrigger::Webhook {
                url, key, header, ..
            } => Some((row.id.clone(), url.clone(), key.clone(), header.clone())),
            _ => None,
        })
    });
    let Some((routine_id, url, key, header)) = hook else {
        return div()
            .text_sm()
            .text_color(muted)
            .child(
                "It runs when something POSTs to an address the server makes for it. The \
                 address and its key are here once it is saved.",
            )
            .into_any_element();
    };
    v_flex()
        .w_full()
        .gap(px(8.))
        .child(field_label("POST to", muted))
        .child(copy_row(
            format!("routine-{routine_id}-webhook-url"),
            url,
            muted,
            theme,
        ))
        .child(field_label("key", muted))
        .child(copy_row(
            format!("routine-{routine_id}-webhook-key"),
            key,
            muted,
            theme,
        ))
        .child(field_label("header", muted))
        .child(copy_row(
            format!("routine-{routine_id}-webhook-header"),
            header,
            muted,
            theme,
        ))
        .child(
            Button::new(SharedString::from(format!("routine-{routine_id}-rotate")))
                .ghost()
                .label("Rotate key")
                .tooltip("The old key stops working at once.")
                .on_click(move |_, _, cx| {
                    app.update(cx, |state, cx| {
                        if let Some(coworker_id) = state.active_coworker_id.clone() {
                            state.rotate_routine_webhook(&coworker_id, &routine_id, cx);
                        }
                    });
                }),
        )
        .into_any_element()
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

    /// The open bot, with one routine the server has, open in the Computer pane.
    pub(super) fn routine_open() -> AppState {
        use crate::state::{AgentRoutine, ComputerView, RightPane, RoutineTrigger, ScheduleSpec};
        let mut state = AppState::new();
        state.coworkers = vec![
            serde_json::from_value(serde_json::json!({ "id": "cw_1", "name": "New Bot" }))
                .expect("a coworker"),
        ];
        state.active_coworker_id = Some("cw_1".into());
        state.routines.insert(
            "cw_1".into(),
            vec![AgentRoutine {
                id: "sch_1".into(),
                name: "Say hello".into(),
                instruction: "Say hello to the team".into(),
                active: true,
                triggers: vec![RoutineTrigger::Schedule {
                    id: "sch_1".into(),
                    spec: ScheduleSpec::from_cron("*/30 * * * *"),
                }],
                runs: Vec::new(),
                saved: Some(crate::opengrok::ScheduleEdit {
                    name: Some("Say hello".into()),
                    prompt: Some("Say hello to the team".into()),
                    cron: Some("0 */30 * * * *".into()),
                }),
            }],
        );
        state.right_pane = RightPane::Computer;
        state.computer_view = ComputerView::Editor {
            id: Some("sch_1".into()),
        };
        state
    }

    /// The routine's header carries its four icons, and the first switches the panel between
    /// the routine's fields and its Run history alone: by default Active, Name and the rest, and
    /// no history; pressed, the history and none of the fields; pressed again, the fields.
    #[gpui_kit::test]
    fn the_history_icon_swaps_the_routines_fields_for_its_runs(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::{AppContext as _, Modifiers};
        cx.update(gpui_kit::init);
        let (pane, cx) = cx.add_window_view(|window, cx| {
            let state = cx.new(|_| routine_open());
            super::ComputerPane::new(window, state, cx)
        });
        let drawn = |cx: &mut gpui_kit::VisualTestContext, id: &'static str| {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.debug_bounds(id)
        };
        for icon in [
            "routine-history-toggle",
            "routine-open-thread",
            "routine-run-now",
            "routine-delete",
        ] {
            assert!(drawn(cx, icon).is_some(), "{icon} is in the header");
        }
        assert!(drawn(cx, "routine-active").is_some());
        assert!(drawn(cx, "routine-name").is_some());
        assert!(
            drawn(cx, "routine-history").is_none(),
            "no history by default"
        );

        let toggle = drawn(cx, "routine-history-toggle").unwrap().center();
        cx.simulate_mouse_move(toggle, None, Modifiers::none());
        cx.simulate_click(toggle, Modifiers::none());
        assert!(pane.update(cx, |pane, cx| pane.state.read(cx).routine_history_open));
        assert!(
            drawn(cx, "routine-history").is_some(),
            "the runs, and only them"
        );
        assert!(drawn(cx, "routine-active").is_none());
        assert!(drawn(cx, "routine-name").is_none());

        cx.simulate_mouse_move(toggle, None, Modifiers::none());
        cx.simulate_click(toggle, Modifiers::none());
        assert!(drawn(cx, "routine-name").is_some(), "the fields again");
        assert!(drawn(cx, "routine-history").is_none());
    }

    /// "When to run" lists the routine's one wake, with + dead beside it (the server keeps one
    /// per routine), and ✎ opens the wake editor in its place: its chips wrap inside the editor
    /// rather than run off the panel, the dates are a grid seven wide, a stepper moves the time,
    /// and Save puts what was picked on the routine.
    #[gpui_kit::test]
    fn a_routines_wake_is_listed_and_edited_in_the_panel(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::{AppContext as _, Bounds, Modifiers, Pixels, px, size};
        cx.update(gpui_kit::init);
        let (pane, cx) = cx.add_window_view(|window, cx| {
            let state = cx.new(|_| routine_open());
            super::ComputerPane::new(window, state, cx)
        });
        // Tall enough that nothing of the editor is scrolled out of the panel.
        cx.simulate_resize(size(px(320.), px(2400.)));
        let drawn = |cx: &mut gpui_kit::VisualTestContext, id: &'static str| {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.debug_bounds(id)
        };
        let press = |cx: &mut gpui_kit::VisualTestContext, bounds: Bounds<Pixels>| {
            cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
            cx.simulate_click(bounds.center(), Modifiers::none());
        };
        let editor_open = |pane: &gpui_kit::Entity<super::ComputerPane>,
                           cx: &mut gpui_kit::VisualTestContext| {
            pane.update(cx, |pane, cx| {
                pane.state.read(cx).routine_wake_editor.is_some()
            })
        };

        let add = drawn(cx, "routine-wake-add").expect("+ is beside the heading");
        press(cx, add);
        assert!(
            !editor_open(&pane, cx),
            "+ is dead while the routine has its wake"
        );

        let edit = drawn(cx, "routine-wake-edit-0").expect("✎ on the wake");
        press(cx, edit);
        assert!(editor_open(&pane, cx));
        assert!(drawn(cx, "routine-wake-editor").is_some());

        let inside = |cx: &mut gpui_kit::VisualTestContext, id: String| {
            let editor = cx.debug_bounds("routine-wake-editor").unwrap();
            let bounds = cx
                .debug_bounds(Box::leak(id.clone().into_boxed_str()))
                .unwrap_or_else(|| panic!("{id} is drawn"));
            assert!(
                bounds.left() >= editor.left() && bounds.right() <= editor.right(),
                "{id} runs out of the editor: {bounds:?} in {editor:?}"
            );
            bounds
        };
        let weekly = drawn(cx, "routine-wake-tab-weekly").unwrap();
        press(cx, weekly);
        drawn(cx, "routine-wake-editor");
        for day in 0..7 {
            inside(cx, format!("routine-wake-day-{day}"));
        }
        for month in 1..=12 {
            inside(cx, format!("routine-wake-month-{month}"));
        }

        let monthly = drawn(cx, "routine-wake-tab-monthly").unwrap();
        press(cx, monthly);
        drawn(cx, "routine-wake-editor");
        let first = inside(cx, "routine-wake-date-1".into());
        let seventh = inside(cx, "routine-wake-date-7".into());
        let eighth = inside(cx, "routine-wake-date-8".into());
        inside(cx, "routine-wake-date-31".into());
        assert_eq!(first.top(), seventh.top(), "a week of dates to a row");
        assert!(eighth.top() > first.top(), "the 8th starts the next");

        let up = drawn(cx, "routine-wake-hour-up").unwrap();
        press(cx, up);
        assert_eq!(
            pane.update(cx, |pane, cx| pane
                .state
                .read(cx)
                .routine_wake_editor
                .as_ref()
                .map(|editor| editor.summary())),
            Some("Monthly on the 1st at 10:00 AM UTC".to_string())
        );

        let save = drawn(cx, "routine-wake-save").unwrap();
        press(cx, save);
        assert!(!editor_open(&pane, cx));
        assert_eq!(
            pane.update(cx, |pane, cx| pane.state.read(cx).coworker_routines("cw_1")
                [0]
            .triggers[0]
                .label()),
            "Monthly on the 1st at 10:00 AM UTC"
        );
    }

    /// Delete's question sits in the middle of the whole window, not of the panel, and holds
    /// focus: Escape answers Cancel, and so does a press beside it, and the routine is still
    /// there; its Delete deletes it.
    #[gpui_kit::test]
    fn delete_asks_in_the_middle_of_the_window_and_escape_cancels(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::{AppContext as _, KeyBinding, Modifiers, point, px, size};
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new(
                "escape",
                crate::actions::CancelRoutineDelete,
                Some("RoutineDelete"),
            )])
        });
        let asking = || {
            let mut state = routine_open();
            state.routine_delete_prompt = Some("sch_1".into());
            state
        };
        let state = cx.new(|_| asking());
        let (_, cx) = cx.add_window_view({
            let state = state.clone();
            move |_, cx| super::RoutineDeletePrompt::new(state, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let dialog = cx.debug_bounds("routine-delete-dialog").expect("asked");
        assert!(
            (dialog.center().x - px(600.)).abs() < px(1.)
                && (dialog.center().y - px(400.)).abs() < px(1.),
            "centred on the window: {dialog:?}"
        );
        assert_eq!(
            state.read_with(cx, |state, _| state.routine_delete_question()),
            Some((
                "Delete \"Say hello\"?".to_string(),
                crate::state::ROUTINE_DELETE_KEEPS
            ))
        );

        cx.simulate_keystrokes("escape");
        assert_eq!(
            state.read_with(cx, |state, _| state.routine_delete_prompt.clone()),
            None
        );
        assert_eq!(
            state.read_with(cx, |state, _| state.coworker_routines("cw_1").len()),
            1
        );

        state.update(cx, |state, cx| state.ask_delete_routine("sch_1", cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_move(point(px(20.), px(20.)), None, Modifiers::none());
        cx.simulate_click(point(px(20.), px(20.)), Modifiers::none());
        assert_eq!(
            state.read_with(cx, |state, _| state.routine_delete_prompt.clone()),
            None,
            "a press beside it is Cancel"
        );

        state.update(cx, |state, cx| state.ask_delete_routine("sch_1", cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let delete = cx
            .debug_bounds("routine-delete-confirm")
            .expect("its Delete")
            .center();
        cx.simulate_mouse_move(delete, None, Modifiers::none());
        cx.simulate_click(delete, Modifiers::none());
        assert!(state.read_with(cx, |state, _| state.coworker_routines("cw_1").is_empty()));
    }
}
