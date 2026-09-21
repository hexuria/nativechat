use crate::chrome::{
    AVATAR_COLORS, AVATAR_SHAPES, AVATAR_TRIGGER_PX, HEADER_PX, INFO_PANE_WIDTH, TITLE_BAR_H,
    chrome_floats,
};
use crate::components::fields::field_input;
use crate::components::persona::PersonaMark;
use crate::opengrok::{CoworkerPatch, ModelEntry};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{
    IndentInline, Input, InputEvent, InputState, MoveDown, MoveUp, OutdentInline, Textarea,
    TextareaState,
};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Selectable, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

const PANE_INNER: f32 = INFO_PANE_WIDTH - 32.0;

#[derive(IntoElement)]
struct AvatarEditorTrigger {
    selected: bool,
    mark: PersonaMark,
}

impl Selectable for AvatarEditorTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for AvatarEditorTrigger {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id("avatar-trigger")
            .relative()
            .flex_shrink_0()
            .cursor_pointer()
            .child(self.mark.lit(self.selected))
    }
}

/// Where the model list hangs from: an element of its own under the field and the width of
/// it, so the list lines up with the field.
///
/// The field itself cannot be the popover's trigger. A trigger swallows every mouse down over
/// everything it holds, and the model field is a field a person has to be able to click into,
/// place the caret in and select text in. So the trigger anchors the list and nothing else:
/// the chevron beside the field opens it.
#[derive(IntoElement)]
struct ModelPopAnchor {
    selected: bool,
}

impl Selectable for ModelPopAnchor {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for ModelPopAnchor {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().id("agent-model-anchor").w(px(PANE_INNER)).h(px(0.))
    }
}

/// The chevron that opens the model list, drawn inside the field's own border at its right
/// edge so the two read as one control.
fn model_chevron(app: Entity<AppState>, open: bool, muted: Hsla) -> impl IntoElement {
    div()
        .id("agent-model-chevron")
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x777777).opacity(0.16)))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            // The pane shuts its popovers on any mouse down that reaches it, so the click that
            // opens one has to stop here.
            cx.stop_propagation();
            app.update(cx, |state, cx| {
                state.set_model_picker_open(!open, cx);
            });
        })
        .child(
            Icon::new(IconName::ChevronDown)
                .size(px(14.))
                .text_color(muted),
        )
}

pub struct AgentSettings {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    label_input: Entity<InputState>,
    role_input: Entity<TextareaState>,
    model_input: Entity<InputState>,
    /// The row Enter takes, counted over the routes the field's text leaves rather than over
    /// the whole catalogue: what the arrows walk is what a person can see.
    model_highlight: usize,
    /// Whether the field's text is a query or still the value it was opened with. A combobox
    /// field is each in turn, and which one it is decides whether the list is narrowed by it.
    model_query_live: bool,
    /// Whether the list was open at the last look. A highlight belongs to one opening of the
    /// list, and an opening the pane did not ask for itself — the chevron's — is only ever
    /// heard of here.
    model_open: bool,
    /// The list scrolls once there are more routes than fit, so a row walked onto has to be
    /// brought into view; a highlight below the fold is a highlight nobody can see.
    model_scroll: ScrollHandle,
    synced_id: Option<String>,
    /// The profile is with the server. The Save button is out of the person's hands until the
    /// answer comes back, whichever way it goes.
    saving: bool,
    usage_open: bool,
    auto_review_open: bool,
    auto_review_mode: AutoReviewMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AutoReviewMode {
    Inherit,
    On,
    Off,
}

impl AgentSettings {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Bob"));
        let label_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Research, marketing, admin"));
        let role_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx).placeholder("What this Bot is for");
            state.set_auto_grow(3, 7, cx);
            state
        });
        let model_input = cx.new(|cx| InputState::new(window, cx).placeholder("xai/grok-4.6@sub"));
        cx.observe(&state, |this, state, cx| {
            // The chevron opens the list through the app's state, so the pane learns of that
            // opening here or not at all. Every opening starts the highlight afresh: the row the
            // arrows were left on last time belongs to a list that is no longer up.
            let open = state.read(cx).model_picker_open;
            if open && !this.model_open {
                this.reset_model_highlight(cx);
            }
            // The list is down, so what stands in the field is the coworker's route again
            // rather than something being looked up, however it got there. The next opening
            // shows the whole catalogue.
            if !open && this.model_open {
                this.model_query_live = false;
            }
            this.model_open = open;
            cx.notify();
        })
        .detach();
        cx.subscribe_in(
            &model_input,
            window,
            |this, _input, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    // A keystroke is what turns the field's text from the coworker's route into
                    // something being looked for, and from here on the list is narrowed by it.
                    // Typing opens the list too: someone typing a route is choosing one, and the
                    // routes that answer to what they have typed are no use behind a shut list.
                    // Only a person's own edit arrives here — `set_value` says nothing — so
                    // taking a route does not reopen the list that taking it just closed.
                    this.model_query_live = true;
                    this.reset_model_highlight(cx);
                    this.state.update(cx, |state, cx| {
                        state.set_model_picker_open(true, cx);
                    });
                    cx.notify();
                }
                // Enter takes the row the highlight is on, which is the whole point of a list
                // that is walked from the field above it.
                InputEvent::PressEnter { .. } => this.take_highlighted_model(window, cx),
                _ => {}
            },
        )
        .detach();
        Self {
            state,
            name_input,
            label_input,
            role_input,
            model_input,
            model_highlight: FIRST_MATCH,
            model_query_live: false,
            model_open: false,
            model_scroll: ScrollHandle::new(),
            synced_id: None,
            saving: false,
            usage_open: false,
            auto_review_open: false,
            auto_review_mode: AutoReviewMode::Inherit,
        }
    }

    fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let coworker = {
            let state = self.state.read(cx);
            state
                .active_coworker_id
                .as_ref()
                .and_then(|id| state.coworkers.iter().find(|c| &c.id == id))
                .cloned()
        };
        let id = coworker.as_ref().map(|c| c.id.clone());
        if self.synced_id == id {
            return;
        }
        self.synced_id = id;
        let coworker = match coworker {
            Some(c) => c,
            None => return,
        };
        self.name_input.update(cx, |input, cx| {
            input.set_value(coworker.name.clone(), window, cx);
        });
        self.label_input.update(cx, |input, cx| {
            input.set_value(coworker.title.clone().unwrap_or_default(), window, cx);
        });
        self.role_input.update(cx, |input, cx| {
            input.set_value(coworker.role.clone().unwrap_or_default(), window, cx);
        });
        self.model_input.update(cx, |input, cx| {
            input.set_value(coworker.model.clone(), window, cx);
        });
        // The pane has just written another coworker's route into the field, so what stands
        // there is a value again whatever was being looked up before it.
        self.model_query_live = false;
        self.reset_model_highlight(cx);
    }

    /// The route the coworker is on, as the roster has it.
    fn current_model(&self, cx: &App) -> String {
        let state = self.state.read(cx);
        state
            .active_coworker_id
            .as_ref()
            .and_then(|id| state.coworkers.iter().find(|c| &c.id == id))
            .map(|coworker| coworker.model.clone())
            .unwrap_or_default()
    }

    /// What the list is narrowed by: nothing at all while the field's text is still the value
    /// the pane put there, the text itself once somebody has typed into it.
    fn model_filter(&self, cx: &App) -> String {
        let text = self.model_input.read(cx).value().to_string();
        filter_text(&text, self.model_query_live).to_string()
    }

    /// The routes the filter leaves, in the catalogue's order.
    fn model_matches(&self, cx: &App) -> Vec<String> {
        matching_models(
            &self.state.read(cx).model_catalogue.models,
            &self.model_filter(cx),
        )
    }

    /// Put the highlight where a fresh list starts: on the route the coworker is already on
    /// while the whole catalogue is on show, so the list opens on where they are and Enter takes
    /// what they have rather than moving them to the top of a list they have not read; on the
    /// first match once the text is a query.
    fn reset_model_highlight(&mut self, cx: &App) {
        self.model_highlight = if self.model_query_live {
            FIRST_MATCH
        } else {
            current_model_row(&self.model_matches(cx), &self.current_model(cx))
        };
        self.model_scroll.scroll_to_item(self.model_highlight);
    }

    /// Step the highlight, and claim the keystroke while the list is open: these keys belong to
    /// the list for as long as it is up. Shut, the keystroke is left to whoever else wants it,
    /// which is what keeps Tab moving the focus out of a field with no list under it.
    fn step_model_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        if !self.state.read(cx).model_picker_open {
            return;
        }
        cx.stop_propagation();
        let len = self.model_matches(cx).len();
        self.model_highlight = stepped_highlight(self.model_highlight, len, down);
        self.model_scroll.scroll_to_item(self.model_highlight);
        cx.notify();
    }

    /// Take the highlighted route, by the same road a click on that row takes.
    fn take_highlighted_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.read(cx).model_picker_open {
            return;
        }
        let query = self.model_filter(cx);
        let catalogue = self.state.read(cx).model_catalogue.models.clone();
        // Nothing matched what was typed, so there is nothing for Enter to take and the text
        // stands as it is: it may well be a route this catalogue has not heard of, and Save
        // sends what the field holds either way.
        let Some(id) = highlighted_model(&catalogue, &query, self.model_highlight) else {
            return;
        };
        take_model(&self.state, &self.model_input, id, window, cx);
    }

    fn commit_profile(&mut self, cx: &mut Context<Self>) {
        // A second Save while the first is still out would send the same profile twice and,
        // with the two answers arriving in any order, settle the roster on whichever landed
        // last. The button is inert while it spins, and this is the same rule for a Save that
        // arrives by any other road.
        if self.saving {
            return;
        }
        // The Model field is a field, so Save means it too. Picking from the list already
        // patches the model on the spot; a route id typed by hand had nowhere to go, and a
        // person who edits a box and presses the button beside it has said what they want just
        // as plainly as one who picked from a list. Blank is the exception and is left out: an
        // empty box is a field nobody filled, not an instruction to unpin the model, and a
        // coworker with no route cannot answer at all.
        let model = self.model_input.read(cx).value().trim().to_string();
        let patch = CoworkerPatch {
            name: Some(self.name_input.read(cx).value().to_string()),
            title: Some(self.label_input.read(cx).value().to_string()),
            role: Some(self.role_input.read(cx).value().to_string()),
            model: (!model.is_empty()).then_some(model),
            ..Default::default()
        };
        self.saving = true;
        let settings = cx.entity().downgrade();
        self.state.update(cx, |state, cx| {
            state.patch_active_agent_then(
                patch,
                Some(Box::new(move |error, cx| {
                    let _ = settings.update(cx, |settings, cx| {
                        settings.saving = false;
                        // The roster now holds what the server stored rather than what was
                        // typed. The fields have to say the same thing, or a name the server
                        // never took would go on sitting in the field as though it had.
                        if error.is_none() {
                            settings.synced_id = None;
                        }
                        cx.notify();
                    });
                })),
                cx,
            );
        });
        cx.notify();
    }
}

fn heading(label: &'static str, color: Hsla) -> Div {
    div()
        .h(px(28.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .text_xs()
        .text_color(color)
        .child(label)
}

fn settings_input(state: &Entity<InputState>) -> Input {
    field_input(state)
}

fn card(fill: Hsla) -> Div {
    div().w_full().p(px(8.)).rounded(px(12.)).bg(fill)
}

fn notify_switch(on: bool) -> Div {
    div()
        .w(px(32.))
        .h(px(20.))
        .rounded_full()
        .relative()
        .bg(if on {
            rgb(0x5a5a5a)
        } else {
            rgb(0x787878).opacity(0.32)
        })
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(if on { px(14.) } else { px(2.) })
                .size(px(16.))
                .rounded_full()
                .bg(rgb(0xfcfcfc)),
        )
}

/// The pane's header: "Settings", and the chevron that closes the pane. In the title bar
/// over the pane while the pane is docked (then the title is a handle to drag the window
/// by), in the pane itself while it floats over the chat.
pub fn settings_header(app: Entity<AppState>, drag: bool) -> impl IntoElement {
    div()
        .id("agent-settings-header")
        .w_full()
        .h(px(TITLE_BAR_H))
        .px(px(HEADER_PX))
        .flex()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Settings")
                .when(drag, |this| {
                    this.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                }),
        )
        .child(
            div()
                .id("header-settings")
                .size(px(28.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    app.update(cx, |state, cx| {
                        state.close_right_pane(cx);
                    });
                })
                .child(
                    Icon::default()
                        .path("icons/chevrons-right.svg")
                        .size(px(16.)),
                ),
        )
}

impl Render for AgentSettings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_fields(window, cx);
        let theme = cx.theme().clone();
        let dark = theme.is_dark();
        let muted = theme.muted_foreground;
        let card_fill: Hsla = rgb(0x777777).opacity(0.173).into();
        let (id, model, shape, color, notify, catalogue, note, error, model_open, editor_open) = {
            let state = self.state.read(cx);
            let coworker = state
                .active_coworker_id
                .as_ref()
                .and_then(|id| state.coworkers.iter().find(|c| &c.id == id));
            (
                coworker
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| "agent".into()),
                coworker.map(|c| c.model.clone()).unwrap_or_default(),
                coworker.and_then(|c| c.avatar_shape.clone()),
                coworker.and_then(|c| c.avatar_color.clone()),
                coworker.and_then(|c| c.notify_on_updates).unwrap_or(false),
                state.model_catalogue.models.clone(),
                state.model_catalogue.note.clone(),
                state.auth_error.clone(),
                state.model_picker_open,
                state.avatar_editor_open,
            )
        };
        let model_focus = self.model_input.read(cx).focus_handle(cx);
        // The list is what the filter leaves of the catalogue, and the row Enter takes is
        // counted over that rather than over the catalogue behind it.
        let model_query = self.model_filter(cx);
        let model_matches = matching_models(&catalogue, &model_query);
        let model_highlight = self.model_highlight;
        let model_scroll = self.model_scroll.clone();
        let saving = self.saving;
        let usage_open = self.usage_open;
        let auto_review_open = self.auto_review_open;
        let auto_review_mode = self.auto_review_mode;
        let has_custom = shape.is_some() || color.is_some();
        let app = self.state.clone();
        // The bot's own computer's network choice lives here, with the bot's other settings;
        // a shared computer's lives in Settings → Computer, with Route traffic.
        let egress_policy = {
            let state = self.state.read(cx);
            state
                .show_egress_policy_on_bot_pane()
                .then(|| state.egress_policy())
                .flatten()
        };
        // Floating over the chat (a narrow window), the pane carries its own header; docked,
        // the title bar shows it over the pane.
        let floats = chrome_floats(f32::from(window.viewport_size().width));

        v_flex()
            .id("agent-settings")
            .relative()
            .h_full()
            .w(px(INFO_PANE_WIDTH))
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .text_color(theme.foreground)
            .on_mouse_down(MouseButton::Left, {
                let app = app.clone();
                move |_, _, cx| {
                    app.update(cx, |state, cx| {
                        state.dismiss_popovers(cx);
                    });
                }
            })
            .when(floats, |this| this.child(settings_header(app.clone(), false)))
            .child(
                div()
                    .id("avatar-trigger-row")
                    .w_full()
                    .h(px(76.))
                    .min_h(px(76.))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        Popover::new("avatar-editor-pop")
                            .appearance(false)
                            .overlay_closable(true)
                            .open(editor_open)
                            .on_open_change({
                                let app = app.clone();
                                move |open, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.set_avatar_editor_open(*open, cx);
                                    });
                                }
                            })
                            .trigger(AvatarEditorTrigger {
                                selected: editor_open,
                                mark: PersonaMark::new(id.clone())
                                    .shape(shape.clone())
                                    .color(color.clone())
                                    .size(px(AVATAR_TRIGGER_PX))
                                    .dark(true),
                            })
                            .content({
                                let app = app.clone();
                                let id = id.clone();
                                let shape = shape.clone();
                                let color = color.clone();
                                let theme = theme.clone();
                                move |_, _, _| {
                                    avatar_editor_panel(
                                        app.clone(),
                                        id.clone(),
                                        shape.clone(),
                                        color.clone(),
                                        has_custom,
                                        dark,
                                        theme.clone(),
                                    )
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .id("agent-settings-body")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .id("agent-settings-stack")
                            .w_full()
                            .px(px(16.))
                            .child(heading("Name", muted))
                            .child(
                                div()
                                    .id("agent-settings-name")
                                    .w_full()
                                    .child(settings_input(&self.name_input)),
                            )
                            .child(heading("Label (optional)", muted))
                            .child(
                                div()
                                    .id("agent-label")
                                    .w_full()
                                    .child(settings_input(&self.label_input)),
                            )
                            .child(heading("Description", muted))
                            .child(
                                div()
                                    .id("agent-role")
                                    .w_full()
                                    .child(
                                        Textarea::new(&self.role_input)
                                            .appearance(false)
                                            .w_full()
                                            .rounded(px(8.))
                                            .border_1()
                                            .border_color(theme.input)
                                            .bg(theme.input_background()),
                                    ),
                            )
                                    .when_some(egress_policy, |this, current| {
                                        this.child(
                                            div().pt(px(12.)).child(
                                                card(card_fill).id("agent-network").child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .justify_between()
                                                        .gap(px(12.))
                                                        .child(
                                                            v_flex()
                                                                .min_w(px(0.))
                                                                .gap(px(2.))
                                                                .child(
                                                                    div()
                                                                        .text_sm()
                                                                        .child("Use your network"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .text_color(muted)
                                                                        .child("Whether this Bot's computer may reach the web through this desktop without asking each time. Never allow keeps its browser off while traffic is routed here."),
                                                                ),
                                                        )
                                                        .child(
                                                            crate::components::app_settings::egress_policy_picker(
                                                                current,
                                                                app.clone(),
                                                            ),
                                                        ),
                                                ),
                                            ),
                                        )
                                    })
                                    .child(
                                        div().pt(px(12.)).child(
                                            card(card_fill)
                                                .id("agent-notifications")
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .justify_between()
                                                        .gap(px(12.))
                                                        .child(
                                                            v_flex()
                                                                .min_w(px(0.))
                                                                .gap(px(2.))
                                                                .child(
                                                                    div()
                                                                        .text_sm()
                                                                        .child("Notifications"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .text_color(muted)
                                                                        .child("Get notified when this agent finishes or needs input. There is nowhere to keep this yet, so the switch does nothing."),
                                                                ),
                                                        )
                                                        // The switch shows what the roster says
                                                        // and takes no orders. Nothing stores
                                                        // this: the two handlers that mention it
                                                        // answer a fixed yes, and the patch route
                                                        // does not read the key at all, so a flip
                                                        // was thrown away in silence — and once
                                                        // that route refuses a patch with nothing
                                                        // in it to change, the same flip would
                                                        // come back as an error in the row below.
                                                        // It stays on show, dimmed, because the
                                                        // setting is a real one waiting on
                                                        // somewhere to live, and the line beside
                                                        // it says as much.
                                                        .child(
                                                            div()
                                                                .id("agent-notify-switch")
                                                                .opacity(0.5)
                                                                .child(notify_switch(notify)),
                                                        ),
                                                ),
                                        ),
                                    )
                                    .child(heading("Model", muted))
                                    .child(
                                        v_flex()
                                            .id("agent-model")
                                            .w_full()
                                            .on_key_down(cx.listener(
                                                |this, event: &KeyDownEvent, _, cx| {
                                                    // Focus is in the field and not in the
                                                    // panel, so the popover never hears the
                                                    // Escape itself. The pane shuts the list,
                                                    // and the field keeps what was typed.
                                                    if event.keystroke.key == "escape" {
                                                        this.state.update(cx, |state, cx| {
                                                            state.set_model_picker_open(false, cx);
                                                        });
                                                    }
                                                },
                                            ))
                                            // The field binds the arrows and Tab to actions of
                                            // its own, and actions are dispatched before any key
                                            // listener, so the capture phase — which runs from
                                            // the outside in — is the only place this row can
                                            // take them before the field does.
                                            .capture_action(cx.listener(
                                                |this, _: &MoveUp, _, cx| {
                                                    this.step_model_highlight(false, cx);
                                                },
                                            ))
                                            .capture_action(cx.listener(
                                                |this, _: &MoveDown, _, cx| {
                                                    this.step_model_highlight(true, cx);
                                                },
                                            ))
                                            // Tab is Down and Shift-Tab is Up: someone tabbing
                                            // with a list of choices in front of them means the
                                            // choices. The field binds the two to indent and
                                            // outdent, which a one-line field has no use for, so
                                            // stopping there is what keeps Tab from moving the
                                            // focus out of the field while the list is up.
                                            .capture_action(cx.listener(
                                                |this, _: &IndentInline, _, cx| {
                                                    this.step_model_highlight(true, cx);
                                                },
                                            ))
                                            .capture_action(cx.listener(
                                                |this, _: &OutdentInline, _, cx| {
                                                    this.step_model_highlight(false, cx);
                                                },
                                            ))
                                            .child(
                                                settings_input(&self.model_input)
                                                    .id("agent-model-field")
                                                    .w(px(PANE_INNER))
                                                    .suffix(model_chevron(
                                                        app.clone(),
                                                        model_open,
                                                        muted,
                                                    )),
                                            )
                                            .child(
                                                Popover::new("agent-model-pop")
                                                    .appearance(false)
                                                    .overlay_closable(true)
                                                    .open(model_open)
                                                    // Opening the list must not take the
                                                    // keyboard off the field: a popover focuses
                                                    // its own panel unless it is told whose
                                                    // keystrokes these are.
                                                    .track_focus(&model_focus)
                                                    .on_open_change({
                                                        let app = app.clone();
                                                        move |open, _, cx| {
                                                            app.update(cx, |state, cx| {
                                                                state.set_model_picker_open(
                                                                    *open, cx,
                                                                );
                                                            });
                                                        }
                                                    })
                                                    .trigger(ModelPopAnchor {
                                                        selected: model_open,
                                                    })
                                                    .content({
                                                        let app = app.clone();
                                                        let theme = theme.clone();
                                                        let model = model.clone();
                                                        let field = self.model_input.clone();
                                                        let matches = model_matches.clone();
                                                        let query = model_query.clone();
                                                        move |_, _, _| {
                                                            model_picker_panel(
                                                                app.clone(),
                                                                field.clone(),
                                                                ModelPicker {
                                                                    matches: matches.clone(),
                                                                    query: query.clone(),
                                                                    highlight: model_highlight,
                                                                    current: model.clone(),
                                                                },
                                                                model_scroll.clone(),
                                                                dark,
                                                                theme.clone(),
                                                            )
                                                        }
                                                    }),
                                            ),
                                    )
                                    .when_some(note, |this, note| {
                                        this.child(
                                            div()
                                                .pt(px(4.))
                                                .text_xs()
                                                .text_color(muted)
                                                .child(note),
                                        )
                                    })
                                    .child(
                                        div()
                                            .id("agent-usage")
                                            .mt(px(14.))
                                            .px(px(14.))
                                            .py(px(12.))
                                            .rounded(px(10.))
                                            .border_1()
                                            .border_color(theme.border)
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap(px(10.))
                                                    .child(
                                                        v_flex()
                                                            .min_w(px(0.))
                                                            .gap(px(2.))
                                                            .child(div().text_sm().child("Usage"))
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(muted)
                                                                    .child(if usage_open {
                                                                        "No requests this month"
                                                                    } else {
                                                                        "No usage this month"
                                                                    }),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("agent-usage-open")
                                                            .px(px(11.))
                                                            .py(px(5.))
                                                            .rounded(px(8.))
                                                            .border_1()
                                                            .border_color(
                                                                rgb(0x7f7f7f).opacity(0.4),
                                                            )
                                                            .text_xs()
                                                            .cursor_pointer()
                                                            .on_mouse_down(
                                                                MouseButton::Left,
                                                                cx.listener(|this, _, _, cx| {
                                                                    this.usage_open = !this.usage_open;
                                                                    cx.notify();
                                                                }),
                                                            )
                                                            .child("Open"),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("agent-auto-review")
                                            .mt(px(14.))
                                            .mb(px(16.))
                                            .px(px(14.))
                                            .py(px(12.))
                                            .rounded(px(10.))
                                            .border_1()
                                            .border_color(theme.border)
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap(px(10.))
                                                    .child(div().text_sm().child("Auto-review"))
                                                    .child(
                                                        div()
                                                            .id("agent-auto-review-manage")
                                                            .px(px(11.))
                                                            .py(px(5.))
                                                            .rounded(px(8.))
                                                            .border_1()
                                                            .border_color(
                                                                rgb(0x7f7f7f).opacity(0.4),
                                                            )
                                                            .text_xs()
                                                            .cursor_pointer()
                                                            .on_mouse_down(
                                                                MouseButton::Left,
                                                                cx.listener(|this, _, _, cx| {
                                                                    this.auto_review_open =
                                                                        !this.auto_review_open;
                                                                    cx.notify();
                                                                }),
                                                            )
                                                            .child("Manage…"),
                                                    ),
                                            )
                                            .when(auto_review_open, |this| {
                                                this.child(self.auto_review_body(auto_review_mode, cx))
                                            }),
                                    )
                                    .when_some(error, |this, message| {
                                        this.child(
                                            div()
                                                .id("agent-settings-error")
                                                .text_sm()
                                                .text_color(theme.danger)
                                                .child(message),
                                        )
                                    })
                                    .child(
                                        div().id("agent-save").pt(px(8.)).child(
                                            Button::new("agent-save-btn")
                                                .label("Save")
                                                .primary()
                                                // The icon slot is what the button spins, so
                                                // the spinner only appears while there is
                                                // something to wait for.
                                                .when(saving, |this| {
                                                    this.icon(IconName::Loader)
                                                })
                                                .loading(saving)
                                                .disabled(saving)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.commit_profile(cx);
                                                })),
                                        ),
                                    ),
                    ),
            )
    }
}

fn avatar_editor_panel(
    app: Entity<AppState>,
    agent_id: String,
    shape: Option<String>,
    color: Option<String>,
    has_custom: bool,
    dark: bool,
    theme: gpui_kit::component::Theme,
) -> impl IntoElement {
    let panel_bg = if dark { rgb(0x1c1c1c) } else { rgb(0xffffff) };
    v_flex()
        .id("avatar-editor")
        .w(px(PANE_INNER))
        .rounded(px(16.))
        .border_1()
        .border_color(theme.border)
        .bg(panel_bg)
        .text_color(theme.foreground)
        .shadow_lg()
        .overflow_hidden()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .p(px(8.))
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .px(px(8.))
                        .py(px(4.))
                        .rounded(px(8.))
                        .bg(rgb(0x777777).opacity(0.173))
                        .text_sm()
                        .child("Bot"),
                )
                .when(has_custom, |this| {
                    this.child(
                        div()
                            .id("avatar-reset")
                            .ml_auto()
                            .px(px(8.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .rounded(px(8.))
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
                            .on_mouse_down(MouseButton::Left, {
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.patch_active_agent(
                                            CoworkerPatch {
                                                avatar_shape: Some(String::new()),
                                                avatar_color: Some(String::new()),
                                                ..Default::default()
                                            },
                                            cx,
                                        );
                                    });
                                }
                            })
                            .child("Reset"),
                    )
                }),
        )
        .child(
            v_flex()
                .p(px(12.))
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_center()
                        .gap(px(8.))
                        .py(px(6.))
                        .children(AVATAR_SHAPES.iter().map(|candidate| {
                            let selected = shape.as_deref() == Some(*candidate);
                            let app = app.clone();
                            let agent_id = agent_id.clone();
                            let candidate_s = *candidate;
                            div()
                                .id(SharedString::from(format!("shape-{candidate}")))
                                .size(px(48.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.patch_active_agent(
                                            CoworkerPatch {
                                                avatar_shape: Some(candidate_s.into()),
                                                ..Default::default()
                                            },
                                            cx,
                                        );
                                    });
                                })
                                .child(
                                    PersonaMark::new(agent_id)
                                        .shape(Some(*candidate))
                                        .color(color.clone())
                                        .size(px(36.))
                                        .dark(dark)
                                        .lit(selected)
                                        .group(format!("shape-{candidate}")),
                                )
                        })),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_center()
                        .gap(px(8.))
                        .px(px(12.))
                        .py(px(8.))
                        .children(AVATAR_COLORS.iter().map(|candidate| {
                            let selected = color.as_deref() == Some(candidate.id);
                            let app = app.clone();
                            let id = candidate.id;
                            let group = SharedString::from(format!("color-{id}"));
                            let halo_fill: Hsla = rgb(candidate.swatch).opacity(0.32).into();
                            div()
                                .id(SharedString::from(format!("color-{id}")))
                                .relative()
                                .size(px(32.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .group(group.clone())
                                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.patch_active_agent(
                                            CoworkerPatch {
                                                avatar_color: Some(id.into()),
                                                ..Default::default()
                                            },
                                            cx,
                                        );
                                    });
                                })
                                .child(
                                    div()
                                        .absolute()
                                        .size(px(32.))
                                        .rounded_full()
                                        .bg(halo_fill)
                                        .opacity(if selected { 1. } else { 0. })
                                        .group_hover(group, |s| s.opacity(1.)),
                                )
                                .child(div().size(px(24.)).rounded_full().bg(rgb(candidate.swatch)))
                        })),
                ),
        )
}

/// The row the highlight goes back to whenever the filter changes: the first of the matches.
/// The row the arrows were left on stands for something else once the list under it has changed,
/// and the top of a list is where a person who has just typed is looking. It is the first row of
/// anything, so it is also where a list with nothing to point at starts.
const FIRST_MATCH: usize = 0;

/// What the list is narrowed by, given what the field holds and whether anybody has typed into
/// it since it was opened.
///
/// The field carries the coworker's own route the whole time the pane is up, so reading that
/// text as a query would leave the chevron — the one control that says here are your choices —
/// opening onto the single choice already made. The text is a value until a keystroke turns it
/// into a query, and shutting the list turns it back into a value.
fn filter_text(text: &str, live: bool) -> &str {
    if live { text.trim() } else { "" }
}

/// Where the highlight starts when the whole catalogue is on show: on the route the coworker is
/// already on, so the list opens on where they are and Enter takes what they have. A route this
/// catalogue does not hold, or no route at all, starts at the first row like anything else.
fn current_model_row(shown: &[String], current: &str) -> usize {
    shown
        .iter()
        .position(|id| id == current)
        .unwrap_or(FIRST_MATCH)
}

/// The routes a typed query leaves, in the order the catalogue gives them.
///
/// The match is a case-insensitive run of the route id anywhere in it rather than a prefix: a
/// route id is read as who serves it and what it is — `oag/cheap` — and someone who remembers
/// only the second half would be shown nothing at all by a prefix. An empty field is no filter,
/// so it lists everything on offer.
fn matching_models(catalogue: &[ModelEntry], query: &str) -> Vec<String> {
    let needle = query.trim().to_lowercase();
    catalogue
        .iter()
        .filter(|entry| needle.is_empty() || entry.id.to_lowercase().contains(&needle))
        .map(|entry| entry.id.clone())
        .collect()
}

/// The route Enter takes: the highlighted row of what the field's text leaves, and nothing at
/// all when nothing matches.
fn highlighted_model(catalogue: &[ModelEntry], query: &str, row: usize) -> Option<String> {
    matching_models(catalogue, query).into_iter().nth(row)
}

/// Where the highlight lands when it is stepped one row.
///
/// It goes round the list rather than stopping at the ends. Tab walks a set of choices in a
/// circle everywhere else it is used, and the ⌘K palette — this app's other list walked from a
/// field — goes round too; Tab wrapping while the arrows stopped would be two rules for one
/// list. Both keys step through here, so the two cannot drift apart.
fn stepped_highlight(row: usize, len: usize, down: bool) -> usize {
    if len == 0 {
        return FIRST_MATCH;
    }
    if down {
        (row + 1) % len
    } else {
        (row + len - 1) % len
    }
}

/// Take a route: the id lands in the field as text, the list shuts and the coworker is patched.
///
/// The field is a field, so a pick has to land in it as text. Nothing else puts the roster's
/// model back into the field while the same agent stays open. A click on a row and Enter on the
/// highlighted one are the same act, so they come through here rather than doing the same three
/// things twice.
fn take_model(
    app: &Entity<AppState>,
    field: &Entity<InputState>,
    id: String,
    window: &mut Window,
    cx: &mut App,
) {
    field.update(cx, |input, cx| {
        input.set_value(id.clone(), window, cx);
    });
    app.update(cx, |state, cx| {
        state.set_model_picker_open(false, cx);
        state.patch_active_agent(
            CoworkerPatch {
                model: Some(id),
                ..Default::default()
            },
            cx,
        );
    });
}

/// What the list needs to draw itself: the routes the field's text leaves, that text for when it
/// leaves none, the row Enter would take, and the route the coworker is on now.
struct ModelPicker {
    matches: Vec<String>,
    query: String,
    highlight: usize,
    current: String,
}

fn model_picker_panel(
    app: Entity<AppState>,
    field: Entity<InputState>,
    picker: ModelPicker,
    scroll: ScrollHandle,
    dark: bool,
    theme: gpui_kit::component::Theme,
) -> impl IntoElement {
    let list_bg = if dark { rgb(0x1c1c1c) } else { rgb(0xffffff) };
    let ModelPicker {
        matches,
        query,
        highlight,
        current,
    } = picker;
    let muted = theme.muted_foreground;
    let tick = theme.primary;
    let nothing_matches = matches.is_empty();
    v_flex()
        .id("agent-model-list")
        .w(px(PANE_INNER))
        .max_h(px(262.))
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .p(px(4.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border)
        .bg(list_bg)
        .text_color(theme.foreground)
        .shadow_lg()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .children(matches.into_iter().enumerate().map(move |(row, mid)| {
            let selected = mid == current;
            let app = app.clone();
            let field = field.clone();
            div()
                .id(SharedString::from(format!("model-{mid}")))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                // Two different things, told apart two different ways: the filled row is what
                // Enter would take, the ticked one is what the coworker is already on. One fill
                // for both left a person unable to read what they have off what they are about
                // to get.
                .when(row == highlight, |this| {
                    this.bg(rgb(0x777777).opacity(0.22))
                })
                .hover(|s| s.bg(rgb(0x777777).opacity(0.12)))
                .on_mouse_down(MouseButton::Left, {
                    let mid = mid.clone();
                    move |_, window, cx| take_model(&app, &field, mid.clone(), window, cx)
                })
                .child(div().flex_1().min_w(px(0.)).text_sm().truncate().child(mid))
                .when(selected, |this| {
                    this.child(
                        Icon::new(IconName::Check)
                            .size(px(13.))
                            .flex_shrink_0()
                            .text_color(tick),
                    )
                })
        }))
        // An empty box says nothing about why it is empty, and the answer is always the same
        // one: what was typed. The row names it and cannot be picked — there is no route behind
        // it to pick.
        .when(nothing_matches, |this| {
            this.child(
                div()
                    .id("agent-model-empty")
                    .px(px(8.))
                    .py(px(6.))
                    .text_sm()
                    .text_color(muted)
                    .child(format!("No route matches {query}")),
            )
        })
}

impl AgentSettings {
    fn auto_review_body(&self, mode: AutoReviewMode, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .pt(px(12.))
            .gap(px(8.))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0xfcfcfc).opacity(0.6))
                    .child("What this coworker may do. Inherit uses the global rules; On and Off set this coworker's own."),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .children(
                        [
                            (AutoReviewMode::Inherit, "Inherit from global"),
                            (AutoReviewMode::On, "On"),
                            (AutoReviewMode::Off, "Off"),
                        ]
                        .into_iter()
                        .map(|(id, label)| {
                            let selected = mode == id;
                            div()
                                .id(SharedString::from(format!("ar-{label}")))
                                .px(px(10.))
                                .py(px(5.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(rgb(0x808080).opacity(0.3))
                                .when(selected, |this| this.bg(rgb(0x808080).opacity(0.18)))
                                .text_xs()
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.auto_review_mode = id;
                                        cx.notify();
                                    }),
                                )
                                .child(label)
                        }),
                    ),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // Named imports, not a glob: `use super::*` would pull GPUI's `test` attribute in over the
    // one the test harness wants.
    use super::{
        FIRST_MATCH, current_model_row, filter_text, highlighted_model, matching_models,
        stepped_highlight,
    };
    use crate::opengrok::ModelEntry;

    fn catalogue() -> Vec<ModelEntry> {
        [
            "oag/cheap",
            "xai/grok-4.6@sub",
            "OAG/Fast",
            "anthropic/opus",
        ]
        .into_iter()
        .map(|id| ModelEntry { id: id.into() })
        .collect()
    }

    #[test]
    fn the_filter_reads_any_run_of_a_route_id_in_any_case() {
        let catalogue = catalogue();
        assert_eq!(
            matching_models(&catalogue, "oag"),
            vec!["oag/cheap", "OAG/Fast"]
        );
        assert_eq!(
            matching_models(&catalogue, "OAG"),
            vec!["oag/cheap", "OAG/Fast"],
            "nobody types a route id in the case the catalogue keeps it in"
        );
        assert_eq!(
            matching_models(&catalogue, "grok"),
            vec!["xai/grok-4.6@sub"],
            "a route is looked for by the part of it a person remembers, which is rarely its start"
        );
        assert_eq!(
            matching_models(&catalogue, "4.6@sub"),
            vec!["xai/grok-4.6@sub"]
        );
    }

    #[test]
    fn an_empty_field_is_no_filter_at_all() {
        let catalogue = catalogue();
        assert_eq!(
            matching_models(&catalogue, ""),
            vec![
                "oag/cheap",
                "xai/grok-4.6@sub",
                "OAG/Fast",
                "anthropic/opus"
            ],
            "everything on offer, in the order the catalogue gives it"
        );
        assert_eq!(
            matching_models(&catalogue, "   "),
            matching_models(&catalogue, ""),
            "a space is not something a route id is looked for by"
        );
    }

    #[test]
    fn a_query_no_route_answers_to_leaves_nothing() {
        let catalogue = catalogue();
        assert!(matching_models(&catalogue, "not showing the options").is_empty());
        assert_eq!(
            highlighted_model(&catalogue, "not showing the options", FIRST_MATCH),
            None,
            "Enter has nothing to take, so what was typed stands"
        );
    }

    #[test]
    fn an_untouched_field_is_a_value_and_the_chevron_opens_on_the_whole_catalogue() {
        let catalogue = catalogue();
        let current = "OAG/Fast";
        // The coworker's route is in the field whenever the pane is up, so narrowing the list by
        // it would open the chevron onto the one choice already made.
        let shown = matching_models(&catalogue, filter_text(current, false));
        assert_eq!(shown, matching_models(&catalogue, ""));
        assert_eq!(
            current_model_row(&shown, current),
            2,
            "the list opens on the row the coworker is already on"
        );
    }

    #[test]
    fn the_first_keystroke_turns_the_value_into_a_query() {
        let catalogue = catalogue();
        let shown = matching_models(&catalogue, filter_text("oag", true));
        assert_eq!(shown, vec!["oag/cheap", "OAG/Fast"]);
        assert_eq!(
            highlighted_model(&catalogue, filter_text("oag", true), FIRST_MATCH).unwrap(),
            "oag/cheap",
            "a query lights its first match, wherever the coworker's own route sits"
        );
    }

    #[test]
    fn a_route_the_catalogue_does_not_hold_starts_at_the_first_row() {
        let shown = matching_models(&catalogue(), "");
        assert_eq!(current_model_row(&shown, "who/knows"), FIRST_MATCH);
        assert_eq!(current_model_row(&[], "oag/cheap"), FIRST_MATCH);
    }

    #[test]
    fn the_highlight_goes_round_the_list_rather_than_stopping_at_its_ends() {
        // Four rows. Up from the first is the last and down from the last is the first, and it
        // is the same rule whether the key was an arrow or Tab: both step through here.
        assert_eq!(stepped_highlight(0, 4, false), 3);
        assert_eq!(stepped_highlight(3, 4, true), 0);
        assert_eq!(stepped_highlight(1, 4, true), 2);
        assert_eq!(stepped_highlight(1, 4, false), 0);
        // A step each way is where it started, from every row and from either end.
        for row in 0..4 {
            assert_eq!(
                stepped_highlight(stepped_highlight(row, 4, true), 4, false),
                row
            );
            assert_eq!(
                stepped_highlight(stepped_highlight(row, 4, false), 4, true),
                row
            );
        }
        assert_eq!(
            stepped_highlight(0, 0, true),
            FIRST_MATCH,
            "with no rows to walk there is nowhere to walk to"
        );
    }

    #[test]
    fn the_highlight_lands_on_the_first_match_whenever_the_filter_changes() {
        let catalogue = catalogue();
        // A row means something else once the list under it has changed: row 1 of everything is
        // the grok route, and row 1 of what "oag" leaves is another route altogether.
        assert_eq!(
            highlighted_model(&catalogue, "", 1).unwrap(),
            "xai/grok-4.6@sub"
        );
        assert_eq!(highlighted_model(&catalogue, "oag", 1).unwrap(), "OAG/Fast");
        // Which is why every change to the filter puts the highlight back to the first match,
        // and the first match is the first of the matches rather than of the catalogue.
        for query in ["", "oag", "fast", "anthropic"] {
            assert_eq!(
                highlighted_model(&catalogue, query, FIRST_MATCH),
                matching_models(&catalogue, query).first().cloned()
            );
        }
        assert_eq!(
            highlighted_model(&catalogue, "fast", FIRST_MATCH).unwrap(),
            "OAG/Fast"
        );
    }
}
