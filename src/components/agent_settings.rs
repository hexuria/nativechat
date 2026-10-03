use crate::chrome::{
    AVATAR_COLORS, AVATAR_SHAPES, AVATAR_TRIGGER_PX, HEADER_PX, INFO_PANE_WIDTH, TITLE_BAR_H,
    chrome_floats,
};
use crate::components::fields::field_input;
use crate::components::model_picker::ModelPicker;
use crate::components::persona::PersonaMark;
use crate::components::title_bar::window_drag;
use crate::opengrok::{
    BotSkillRow, BotSkillScope, CeilingRow, CoworkerPatch, CoworkerTool, USER_MACHINE_SHELL,
};
use crate::state::{
    AppState, BotSkills, CeilingBlock, CeilingCard, CeilingSwitch, PickerFor, SkillSwitch,
    SkillsBlock, SkillsCard, ToolCeiling, ToolList, UsageReport,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
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

pub struct AgentSettings {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    label_input: Entity<InputState>,
    role_input: Entity<TextareaState>,
    /// The Bot's model, door, fast tier and effort, as the model picker's card, the one place a
    /// Bot's model is picked: every change is saved on the Bot at once, and none waits for Save.
    model_card: Entity<ModelPicker>,
    synced_id: Option<String>,
    /// The profile is with the server. The Save button is out of the person's hands until the
    /// answer comes back, whichever way it goes.
    saving: bool,
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
        let model_card = cx.new(|cx| ModelPicker::new(window, state.clone(), PickerFor::Bot, cx));
        cx.observe(&state, |this, state, cx| {
            // A driver's Save, which comes by way of the app because the button and the fields
            // it sends are this pane's. Only for the bot the fields were filled for: a switch the
            // pane has not drawn yet leaves them holding the last bot's words, and those must
            // not be saved onto this one.
            if state.update(cx, |state, _| state.take_agent_save_request())
                && this.synced_id == state.read(cx).active_coworker_id
            {
                this.commit_profile(cx);
            }
            cx.notify();
        })
        .detach();
        Self {
            state,
            name_input,
            label_input,
            role_input,
            model_card,
            synced_id: None,
            saving: false,
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
    }

    fn commit_profile(&mut self, cx: &mut Context<Self>) {
        // A second Save while the first is still out would send the same profile twice and,
        // with the two answers arriving in any order, settle the roster on whichever landed
        // last. The button is inert while it spins, and this is the same rule for a Save that
        // arrives by any other road.
        if self.saving {
            return;
        }
        // The words in the fields. The model, its door and the effort are the picker's, saved
        // the moment they are picked; sent back with every Save, what the pane last read would
        // undo a pick made since on the card or from another Mac.
        let patch = CoworkerPatch {
            name: Some(self.name_input.read(cx).value().to_string()),
            title: Some(self.label_input.read(cx).value().to_string()),
            role: Some(self.role_input.read(cx).value().to_string()),
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

/// The pane's header row. "Settings" is a handle to drag the window by, whether the row is in
/// the title bar over a docked pane or at the top of the pane itself. With `close`, the row ends
/// in the control that closes the pane; the chat page leaves it off, because the chat's title
/// bar floats its one window-level pane toggle over that end of the row.
pub fn settings_header(app: Entity<AppState>, close: bool) -> impl IntoElement {
    div()
        .id("agent-settings-header")
        .debug_selector(|| "agent-settings-header".into())
        .w_full()
        .h(px(TITLE_BAR_H))
        .px(px(HEADER_PX))
        .flex()
        .items_center()
        .justify_between()
        .flex_shrink_0()
        .child(window_drag(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Settings"),
        ))
        .when(close, |this| {
            this.child(
                div()
                    .id("header-settings")
                    .occlude()
                    .size(px(28.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(rgb(0x777777).opacity(0.2)))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        app.update(cx, |state, cx| state.close_right_pane(cx));
                    })
                    .child(Icon::default().path("icons/panel-right.svg").size(px(16.))),
            )
        })
}

impl Render for AgentSettings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_fields(window, cx);
        let theme = cx.theme().clone();
        let dark = theme.is_dark();
        let muted = theme.muted_foreground;
        let card_fill: Hsla = rgb(0x777777).opacity(0.173).into();
        let (id, shape, color, error, editor_open) = {
            let state = self.state.read(cx);
            let coworker = state
                .active_coworker_id
                .as_ref()
                .and_then(|id| state.coworkers.iter().find(|c| &c.id == id));
            (
                coworker
                    .map(|c| c.id.clone())
                    .unwrap_or_else(|| "agent".into()),
                coworker.and_then(|c| c.avatar_shape.clone()),
                coworker.and_then(|c| c.avatar_color.clone()),
                state.auth_error.clone(),
                state.avatar_editor_open,
            )
        };
        let on_plan = self.state.read(cx).replies_on_plan();
        let saving = self.saving;
        let usage_open = self.state.read(cx).agent_usage_open;
        let auto_review_open = self.auto_review_open;
        let tools_open = self.state.read(cx).agent_tools_open;
        let tools = {
            let state = self.state.read(cx);
            state
                .coworker_tools
                .as_ref()
                .filter(|(owner, _)| state.active_coworker_id.as_deref() == Some(owner.as_str()))
                .map(|(_, list)| list.clone())
        };
        let ceiling = self.state.read(cx).ceiling_card();
        let tools_line = tools.as_ref().map(tools_summary);
        let allowed_line = ceiling
            .as_ref()
            .and_then(|card| ceiling_line(&card.ceiling, card.pending.as_ref()));
        let (ceiling_rows, card_lines) = ceiling
            .as_ref()
            .map(|card| (shown_ceiling_rows(card), ceiling_card_lines(card)))
            .unwrap_or_default();
        let has_switches = !ceiling_rows.is_empty();
        let offered = offered_without_switches(ceiling.as_ref(), tools.as_ref()).to_vec();
        let has_list = !offered.is_empty();
        let danger = theme.danger;
        let skills_open = self.state.read(cx).agent_skills_open;
        let skills = self.state.read(cx).skills_card();
        let skills_line = skills
            .as_ref()
            .map(|card| skills_summary(&card.skills, card.pending.as_ref()));
        let (skill_rows, skill_lines) = skills
            .as_ref()
            .map(|card| (shown_skill_rows(card), skills_card_lines(card)))
            .unwrap_or_default();
        let has_skill_rows = !skill_rows.is_empty();
        let skills_shared = skills.as_ref().is_some_and(|card| card.shared);
        let usage = {
            let state = self.state.read(cx);
            state
                .coworker_usage
                .as_ref()
                .filter(|(owner, _)| state.active_coworker_id.as_deref() == Some(owner.as_str()))
                .map(|(_, report)| report.clone())
        };
        let usage_line = usage
            .as_ref()
            .map_or_else(|| "Asking the server…".to_string(), usage_summary);
        let usage_rows: Vec<String> = match &usage {
            Some(UsageReport::Read(read)) if usage_open => {
                read.models.iter().map(model_line).collect()
            }
            _ => Vec::new(),
        };
        let auto_review_mode = self.auto_review_mode;
        let has_custom = shape.is_some() || color.is_some();
        let app = self.state.clone();
        // The person's connections, each lendable to this bot (#2).
        let connections_card = self
            .state
            .read(cx)
            .active_coworker_id
            .clone()
            .map(|coworker_id| {
                crate::components::connections::agent_card(app.clone(), &coworker_id, cx)
                    .into_any_element()
            });
        let chat_page = self.state.read(cx).page == crate::state::MainPage::Chat;

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
            // On the chat page the pane reaches the window's top edge, under the floating title
            // bar, so its header is the bar's row over the pane: one row, which drags the window
            // as a title bar does. A blank row above a second header put the avatar 104px down
            // and left the header that showed unable to move the window.
            .when(chat_page, |this| this.child(settings_header(app.clone(), false)))
            .when(!chat_page && chrome_floats(f32::from(window.viewport_size().width)), |this| {
                this.child(settings_header(app.clone(), true))
            })
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
                                                                        .child("Get notified when this agent finishes or needs input. Not available yet: this Mac has nowhere to keep it."),
                                                                ),
                                                        )
                                                        // Off, and it takes no orders. The server
                                                        // keeps no such setting and never will:
                                                        // its coworker patch reads the key nowhere
                                                        // and refuses a patch carrying only that
                                                        // (opengrok-server `agui/routes.rs`), and
                                                        // the desktop client keeps it on the
                                                        // machine. NativeChat has no notification
                                                        // of its own for it to turn on yet. It
                                                        // stays on show, dimmed, because the
                                                        // setting is a real one waiting on
                                                        // somewhere to live, and the line beside
                                                        // it says as much.
                                                        .child(
                                                            div()
                                                                .id("agent-notify-switch")
                                                                .opacity(0.5)
                                                                .child(notify_switch(false)),
                                                        ),
                                                ),
                                        ),
                                    )
                                    .child(heading("Model", muted))
                                    // The model, its door, the fast tier and the effort, all in the
                                    // one control, and the only place they are picked.
                                    .child(self.model_card.clone())
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
                                                                    // What the server says the bot used,
                                                                    // not a word the app made up (#138).
                                                                    .child(usage_line),
                                                            ),
                                                    )
                                                    // Only a list of models has anything to open to.
                                                    .when(
                                                        matches!(usage, Some(UsageReport::Read(ref read)) if !read.models.is_empty()),
                                                        |this| {
                                                            this.child(
                                                                div()
                                                                    .id("agent-usage-toggle")
                                                                    .px(px(11.))
                                                                    .py(px(5.))
                                                                    .rounded(px(8.))
                                                                    .border_1()
                                                                    .border_color(
                                                                        rgb(0x7f7f7f).opacity(0.4),
                                                                    )
                                                                    .text_xs()
                                                                    .cursor_pointer()
                                                                    .on_mouse_down(MouseButton::Left, {
                                                                        let app = app.clone();
                                                                        move |_, _, cx| {
                                                                            app.update(cx, |state, cx| state.toggle_agent_usage(cx));
                                                                        }
                                                                    })
                                                                    .child(if usage_open { "Hide" } else { "Show" }),
                                                            )
                                                        },
                                                    ),
                                            )
                                            // A turn on the person's own plan is not metered and
                                            // carries no gateway key, so the report above never
                                            // counts it; while replies go that way, the card says so.
                                            .when(on_plan, |this| {
                                                this.child(
                                                    div()
                                                        .id(crate::components::reply_source::BOT_USAGE_PLAN)
                                                        .pt(px(6.))
                                                        .text_xs()
                                                        .text_color(muted)
                                                        .child(crate::components::reply_source::PLAN_USAGE_NOTE),
                                                )
                                            })
                                            // Under the header row, as the Tools card's list is, so
                                            // the Hide button stays beside the card's own line.
                                            .when(!usage_rows.is_empty(), |this| {
                                                this.child(
                                                    v_flex().pt(px(8.)).gap(px(4.)).children(
                                                        usage_rows.into_iter().enumerate().map(|(i, line)| {
                                                            div()
                                                                .id(SharedString::from(format!("agent-usage-model-{i}")))
                                                                .text_xs()
                                                                .text_color(muted)
                                                                .child(line)
                                                        }),
                                                    ),
                                                )
                                            }),
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
                                    .when(tools.is_some() || ceiling.is_some(), |this| {
                                        this.child(
                                            div()
                                                .id("agent-tools")
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
                                                        .child(
                                                            v_flex()
                                                                .min_w(px(0.))
                                                                .gap(px(2.))
                                                                .child(div().text_sm().child("Tools"))
                                                                // What the next turn is offered, as
                                                                // the server lists it: the switches
                                                                // below are one of what decides that,
                                                                // not the whole of it.
                                                                .when_some(tools_line, |this, line| {
                                                                    this.child(
                                                                        div()
                                                                            .text_xs()
                                                                            .text_color(muted)
                                                                            .child(line),
                                                                    )
                                                                })
                                                                .when_some(allowed_line, |this, line| {
                                                                    this.child(
                                                                        div()
                                                                            .id("agent-ceiling")
                                                                            .text_xs()
                                                                            .text_color(muted)
                                                                            .child(line),
                                                                    )
                                                                }),
                                                        )
                                                        .when(
                                                            has_switches || has_list,
                                                            |this| {
                                                                this.child(
                                                                    div()
                                                                        .id("agent-tools-toggle")
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
                                                                            {
                                                                                let app = app.clone();
                                                                                move |_, _, cx| {
                                                                                    app.update(cx, |state, cx| state.toggle_agent_tools(cx));
                                                                                }
                                                                            },
                                                                        )
                                                                        .child(if tools_open { "Hide" } else { "Show" }),
                                                                )
                                                            },
                                                        ),
                                                )
                                                .when(tools_open && has_switches, |this| {
                                                    this.child(ceiling_body(
                                                        app.clone(),
                                                        ceiling_rows,
                                                        card_lines,
                                                        muted,
                                                        danger,
                                                    ))
                                                })
                                                .when(tools_open && has_list, |this| {
                                                    this.child(tools_body(&offered, muted))
                                                }),
                                        )
                                    })
                                    // Below Tools: what the Bot is told about on every turn,
                                    // beside what it may do (opengrok-server#270).
                                    .when_some(skills_line, |this, line| {
                                        this.child(
                                            div()
                                                .id("agent-skills")
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
                                                        .child(
                                                            v_flex()
                                                                .min_w(px(0.))
                                                                .gap(px(2.))
                                                                .child(div().text_sm().child("Skills"))
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .text_color(muted)
                                                                        .child(line),
                                                                ),
                                                        )
                                                        .when(has_skill_rows, |this| {
                                                            this.child(
                                                                div()
                                                                    .id("agent-skills-toggle")
                                                                    .px(px(11.))
                                                                    .py(px(5.))
                                                                    .rounded(px(8.))
                                                                    .border_1()
                                                                    .border_color(
                                                                        rgb(0x7f7f7f).opacity(0.4),
                                                                    )
                                                                    .text_xs()
                                                                    .cursor_pointer()
                                                                    .on_mouse_down(MouseButton::Left, {
                                                                        let app = app.clone();
                                                                        move |_, _, cx| {
                                                                            app.update(cx, |state, cx| state.toggle_agent_skills(cx));
                                                                        }
                                                                    })
                                                                    .child(if skills_open { "Hide" } else { "Show" }),
                                                            )
                                                        }),
                                                )
                                                .when(skills_open && has_skill_rows, |this| {
                                                    this.child(skills_body(
                                                        app.clone(),
                                                        skill_rows,
                                                        skill_lines,
                                                        skills_shared,
                                                        muted,
                                                        danger,
                                                    ))
                                                }),
                                        )
                                    })
                                    .when_some(connections_card, |this, card| this.child(card))
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

/// The Usage card's second line: what the bot used this month, or why the app cannot say.
pub(crate) fn usage_summary(report: &UsageReport) -> String {
    match report {
        UsageReport::Loading => "Asking the server…".to_string(),
        UsageReport::Unavailable(why) => why.clone(),
        // A note means the numbers are not a measurement: the bot is not metered, or it is and
        // the gateway could not be asked, which the server answers with zero totals. Either way
        // the note is what the card says, never "No requests". The note is a clause ("this
        // coworker has no key of its own yet, …"); on the card it stands as its own line, so it
        // starts with a capital.
        UsageReport::Read(usage) if usage.note.is_some() || !usage.metered => usage
            .note
            .as_deref()
            .map_or_else(|| "This bot's use is not measured.".to_string(), sentence),
        // The gateway leaves attempts that were paid for but lost out of `requests`, so a month
        // of none can still have models to show; only a month with nothing at all is "No".
        UsageReport::Read(usage) => match usage.totals.requests.unwrap_or(0) {
            0 if usage.models.is_empty() => "No requests this month".to_string(),
            requests => {
                let totals = &usage.totals;
                let tokens = [
                    totals.input_tokens,
                    totals.output_tokens,
                    totals.cache_read_tokens,
                    totals.cache_write_tokens,
                ]
                .into_iter()
                .flatten()
                .fold(0i64, i64::saturating_add);
                let mut line = format!(
                    "{} this month · {} tokens",
                    plural(requests, "request"),
                    grouped(tokens)
                );
                if let Some(cost) = usage.totals.cost_usd.as_deref().and_then(dollars) {
                    line.push_str(&format!(" · {cost}"));
                }
                line
            }
        },
    }
}

/// `text` with its first letter capitalised.
pub(crate) fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// One model's line once the card is open.
pub(crate) fn model_line(model: &crate::opengrok::ModelUsage) -> String {
    let mut line = format!(
        "{} · {} · {} tokens",
        model.model_id,
        plural(model.requests, "request"),
        grouped(
            model
                .input_tokens
                .saturating_add(model.output_tokens)
                .saturating_add(model.cache_read_tokens)
                .saturating_add(model.cache_write_tokens)
        )
    );
    if let Some(cost) = dollars(&model.cost_usd) {
        line.push_str(&format!(" · {cost}"));
    }
    line
}

fn plural(n: i64, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{} {word}s", grouped(n))
    }
}

/// A count with thousands separated, the way a person reads a big number.
fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// The server's six-decimal dollars as a person reads them, to the cent; a sum under a cent that
/// is not zero reads as "under $0.01" rather than as nothing spent. The decimal string is
/// rounded as written, half a cent up: through a float, "1.005000" is 1.00499… and would read
/// a cent short.
fn dollars(six: &str) -> Option<String> {
    let (whole, fraction) = six.trim().split_once('.').unwrap_or((six.trim(), ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digit = |at: usize| u64::from(fraction.as_bytes().get(at).map_or(0, |b| b - b'0'));
    let whole: u64 = whole.parse().ok()?;
    let cents = whole
        .checked_mul(100)?
        .checked_add(digit(0) * 10 + digit(1) + u64::from(digit(2) >= 5))?;
    if cents == 0 {
        return Some(if fraction.bytes().any(|b| b != b'0') {
            "under $0.01".to_string()
        } else {
            "$0.00".to_string()
        });
    }
    Some(format!(
        "${}.{:02}",
        grouped(i64::try_from(cents / 100).ok()?),
        cents % 100
    ))
}

/// The Tools card's second line: how many the bot is offered, and from where, or why the app
/// cannot say.
pub(crate) fn tools_summary(list: &ToolList) -> String {
    match list {
        ToolList::Loading => "Asking the server…".to_string(),
        ToolList::Unavailable(why) => why.clone(),
        ToolList::Listed(all) if all.is_empty() => "Offered no tools on its next turn.".to_string(),
        ToolList::Listed(all) => {
            let built_in = all.iter().filter(|tool| tool.is_builtin()).count();
            let plugins = all.len() - built_in;
            match (built_in, plugins) {
                (_, 0) => format!("{built_in} built in"),
                (0, _) => format!("{plugins} from plugins"),
                _ => format!("{built_in} built in · {plugins} from plugins"),
            }
        }
    }
}

/// A tool's words as its row shows them: the first line, cut at a word near 160 characters so
/// one long description cannot fill the pane.
fn first_line_of(description: &str) -> String {
    const MOST: usize = 160;
    let line = description.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= MOST {
        return line.to_string();
    }
    let cut: String = line.chars().take(MOST).collect();
    let at_word = cut
        .rfind(' ')
        .filter(|at| *at > MOST / 2)
        .unwrap_or(cut.len());
    format!("{}…", cut[..at_word].trim_end())
}

/// What the next turn is offered, for the read-only list the Tools card shows where it has no
/// switches to show it by: the ceiling could not be read (a server without the route, a Bot this
/// person does not own, the server out of reach), and the card still says what the Bot is
/// offered. Nothing while the switches are there or on their way.
pub(crate) fn offered_without_switches<'a>(
    ceiling: Option<&CeilingCard>,
    tools: Option<&'a ToolList>,
) -> &'a [CoworkerTool] {
    match (ceiling.map(|card| &card.ceiling), tools) {
        (Some(ToolCeiling::Read(_) | ToolCeiling::Loading), _) => &[],
        (_, Some(ToolList::Listed(all))) => all,
        _ => &[],
    }
}

/// The list of what the next turn is offered, read-only, for a card with no switches to show it
/// by: each tool by its wire name, which is what the model is told, with the first line of what
/// the server says it does.
fn tools_body(all: &[CoworkerTool], muted: Hsla) -> impl IntoElement {
    let (built_in, plugins): (Vec<_>, Vec<_>) = all.iter().partition(|tool| tool.is_builtin());
    let group = |title: &'static str, tools: Vec<&CoworkerTool>| {
        v_flex()
            .gap(px(6.))
            .child(div().text_xs().text_color(muted).child(title))
            .children(tools.into_iter().enumerate().map(|(at, tool)| {
                let first_line = first_line_of(&tool.description);
                // By place, not by name: the server's names are unique per turn, but a row's id
                // must not depend on that holding.
                v_flex()
                    .id(SharedString::from(format!("agent-tool-{title}-{at}")))
                    .gap(px(1.))
                    .child(
                        div()
                            .text_sm()
                            .font_family("Menlo")
                            .child(tool.name.clone()),
                    )
                    .when(!first_line.is_empty(), |this| {
                        this.child(div().text_xs().text_color(muted).child(first_line))
                    })
            }))
    };
    v_flex()
        .pt(px(12.))
        .gap(px(12.))
        .when(!built_in.is_empty(), |this| {
            this.child(group("Built in", built_in))
        })
        .when(!plugins.is_empty(), |this| {
            this.child(group("From plugins", plugins))
        })
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("What this Bot is offered on its next turn."),
        )
}

/// Why `user_machine_shell` cannot be offered now: it runs its commands on a machine of the
/// person's, and the server has none that does (none enrolled, or its local commands set to
/// Never). The app cannot tell which from here — the machine may not be this Mac — so the line
/// says what would make it available rather than guess why it is not.
pub(crate) const NO_MAC_RUNS_COMMANDS: &str = "Not available until this Mac runs commands for Bots";

/// Why a plugin the server no longer loads cannot be offered. It stays listed while it is on,
/// and its switch can only take it off.
pub(crate) const NO_LONGER_ON_THE_SERVER: &str = "No longer on the server";

/// Why any other row cannot be offered. The server says that it cannot and not why, so this says
/// no more than that.
pub(crate) const NOT_AVAILABLE_NOW: &str = "Not available on this server now";

/// The ceiling's line when the server has no rows in it at all.
pub(crate) const NOTHING_TO_SWITCH: &str = "Nothing to switch on or off.";

/// One row of the Tools card, as it is drawn and as the driver is told it is drawn: both are made
/// from this, so the two cannot disagree about where a switch stands or whether it can move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShownCeilingRow {
    /// The server's name for it, which is what its switch is known by.
    pub name: String,
    /// What the row is headed with: its label where the server gives one, as a plugin's, and
    /// the routine tools' one row's `Routines` (opengrok-server #316); else its name, which for
    /// a builtin is its wire name, what the model is told.
    pub title: String,
    pub builtin: bool,
    /// The first line of what the server says it is.
    pub first_line: String,
    /// Where the switch stands: where the server has it, or where it was asked to go while that
    /// is with the server.
    pub on: bool,
    /// This row's switch is the one with the server.
    pub switching: bool,
    /// The switch can be moved now. It cannot while the card is blocked ([`CeilingCard::blocked`])
    /// or to switch on a plugin the server no longer loads.
    pub live: bool,
    /// Why the server could not offer it, when it could not.
    pub unavailable: Option<&'static str>,
    /// The connection a plugin works through, as a note.
    pub connector: Option<String>,
    /// What the card says under this row about its last switch: the server's words for why it did
    /// not take it, or that nobody knows whether it did.
    pub note: Option<String>,
}

/// A line on the Tools card above its switches, as it is drawn and as the driver is told it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CeilingCardLine {
    /// Why every switch is dead for good: the server's words for a 403.
    ReadOnly(String),
    /// Why every switch is dead for now: another Bot's switch, or a read of this one's ceiling,
    /// is with the server.
    Wait(String),
    /// What the server said about the last switch, when it is not about a row on the card.
    Note(String),
}

/// The rows of the open Bot's ceiling as the Tools card draws them, in the server's order, or
/// none while it is not read.
pub(crate) fn shown_ceiling_rows(card: &CeilingCard) -> Vec<ShownCeilingRow> {
    let ToolCeiling::Read(read) = &card.ceiling else {
        return Vec::new();
    };
    let pending = card.pending.as_ref();
    read.rows
        .iter()
        .map(|row| {
            let on = read.shown_on(row, pending);
            ShownCeilingRow {
                name: row.name.clone(),
                title: ceiling_title(row),
                builtin: row.is_builtin(),
                first_line: row
                    .description
                    .as_deref()
                    .map(first_line_of)
                    .unwrap_or_default(),
                on,
                switching: pending.is_some_and(|switch| switch.name == row.name),
                live: read
                    .may_switch(&row.name, !on, card.blocked.as_ref())
                    .is_ok(),
                unavailable: unavailable_why(row),
                connector: connector_note(row),
                note: card
                    .note
                    .as_ref()
                    .filter(|note| note.row() == Some(row.name.as_str()))
                    .map(|note| note.words.clone()),
            }
        })
        .collect()
}

/// The lines above the open Bot's switches: why none of them can move, when that is not a switch
/// of this Bot's already showing it on its row, and what the server said about the last switch
/// when there is no row on the card for it to be said under.
pub(crate) fn ceiling_card_lines(card: &CeilingCard) -> Vec<CeilingCardLine> {
    let ToolCeiling::Read(read) = &card.ceiling else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    match &card.blocked {
        Some(CeilingBlock::ReadOnly(words)) => lines.push(CeilingCardLine::ReadOnly(words.clone())),
        Some(blocked @ (CeilingBlock::AnotherBot | CeilingBlock::Reading)) => {
            lines.push(CeilingCardLine::Wait(blocked.why().to_string()));
        }
        Some(CeilingBlock::Switching) | None => {}
    }
    if let Some(note) = &card.note {
        let under_a_row = note
            .row()
            .is_some_and(|name| read.rows.iter().any(|row| row.name == name));
        // A read-only card's reason is already its first line.
        if !under_a_row && !matches!(card.blocked, Some(CeilingBlock::ReadOnly(_))) {
            lines.push(CeilingCardLine::Note(note.words.clone()));
        }
    }
    lines
}

/// The Tools card's line about the ceiling, under the line about the next turn: how many rows are
/// allowed as the switches stand, or why there are no switches. Nothing while it is still being
/// asked for, which the card's own line is already saying.
pub(crate) fn ceiling_line(
    ceiling: &ToolCeiling,
    pending: Option<&CeilingSwitch>,
) -> Option<String> {
    match ceiling {
        ToolCeiling::Loading => None,
        ToolCeiling::Unavailable(why) => Some(why.clone()),
        ToolCeiling::Read(read) if read.rows.is_empty() => Some(NOTHING_TO_SWITCH.to_string()),
        ToolCeiling::Read(read) => {
            let (on, of) = read.allowed(pending);
            Some(format!("{on} of {of} allowed"))
        }
    }
}

/// What a row is headed with; see [`ShownCeilingRow::title`].
fn ceiling_title(row: &CeilingRow) -> String {
    row.label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or(&row.name)
        .to_string()
}

/// Why the server could not offer a row, in words for the row, when it could not.
pub(crate) fn unavailable_why(row: &CeilingRow) -> Option<&'static str> {
    if row.is_available() {
        return None;
    }
    Some(if row.name == USER_MACHINE_SHELL {
        NO_MAC_RUNS_COMMANDS
    } else if row.is_builtin() {
        NOT_AVAILABLE_NOW
    } else {
        NO_LONGER_ON_THE_SERVER
    })
}

/// The note on a plugin that works through one of the person's connections. The server sends the
/// connector's id, and nothing says how its service spells its own name, so the id is not dressed
/// up as one: a plugin whose connector is its own name is named by its label, as its row is, and
/// any other connector is shown as the server sent it. The connection itself is made and lent
/// elsewhere, and the plugin's switch does neither.
pub(crate) fn connector_note(row: &CeilingRow) -> Option<String> {
    if row.is_builtin() {
        return None;
    }
    let connector = row
        .connector
        .as_deref()
        .map(str::trim)
        .filter(|connector| !connector.is_empty())?;
    let label = row
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty());
    let named = match label {
        Some(label) if connector == row.name => label,
        _ => connector,
    };
    Some(format!("Uses your {named} connection"))
}

/// The switches: every tool the server has and every plugin it knows, under the two headings the
/// card has always had, below the card's own lines. A switch is sent the moment it is clicked,
/// and only a live one takes a click; one that cannot move now is drawn dimmed.
fn ceiling_body(
    app: Entity<AppState>,
    rows: Vec<ShownCeilingRow>,
    lines: Vec<CeilingCardLine>,
    muted: Hsla,
    danger: Hsla,
) -> impl IntoElement {
    let (built_in, plugins): (Vec<_>, Vec<_>) = rows.into_iter().partition(|row| row.builtin);
    let group = |title: &'static str, rows: Vec<ShownCeilingRow>| {
        v_flex()
            .gap(px(8.))
            .child(div().text_xs().text_color(muted).child(title))
            .children(
                rows.into_iter()
                    .map(|row| ceiling_row(app.clone(), row, muted, danger)),
            )
    };
    v_flex()
        .pt(px(12.))
        .gap(px(12.))
        // Above the switches, because each is about all of them.
        .children(lines.into_iter().map(|line| {
            let (id, words) = match line {
                CeilingCardLine::ReadOnly(words) => ("agent-ceiling-read-only", words),
                CeilingCardLine::Wait(words) => ("agent-ceiling-wait", words),
                CeilingCardLine::Note(words) => ("agent-ceiling-note", words),
            };
            div().id(id).text_xs().text_color(muted).child(words)
        }))
        .when(!built_in.is_empty(), |this| this.child(group("Built in", built_in)))
        .when(!plugins.is_empty(), |this| this.child(group("From plugins", plugins)))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("What this Bot may be offered. Its next turn is not offered anything switched off here."),
        )
}

/// One row: what it is, and its switch at the right, with "Saving…" beside it while it is with
/// the server. A row the server could not offer is dimmed with why under it, and what the card
/// says about the row's last switch is under the row it is about.
fn ceiling_row(
    app: Entity<AppState>,
    row: ShownCeilingRow,
    muted: Hsla,
    danger: Hsla,
) -> impl IntoElement {
    let ShownCeilingRow {
        name,
        title,
        builtin,
        first_line,
        on,
        switching,
        live,
        unavailable,
        connector,
        note,
    } = row;
    // A builtin headed by its wire name is drawn as code, as the model is told it; one the server
    // labels for people, as the routine tools' row, is drawn as a name.
    let wire_name = builtin && title == name;
    // Every id under a row has a fixed word between `agent-ceiling-` and the server's name, and
    // the card's own lines have none, so no name a plugin can have makes one id another's.
    let switch_id = SharedString::from(format!("agent-ceiling-switch-{name}"));
    let why_id = SharedString::from(format!("agent-ceiling-why-{name}"));
    let connector_id = SharedString::from(format!("agent-ceiling-connector-{name}"));
    let error_id = SharedString::from(format!("agent-ceiling-error-{name}"));
    v_flex()
        .id(SharedString::from(format!("agent-ceiling-row-{name}")))
        .gap(px(2.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .child(
                    v_flex()
                        .min_w(px(0.))
                        .flex_1()
                        .gap(px(1.))
                        .when(unavailable.is_some(), |this| this.opacity(0.5))
                        .child(
                            div()
                                .text_sm()
                                .when(wire_name, |this| this.font_family("Menlo"))
                                .child(title),
                        )
                        .when(!first_line.is_empty(), |this| {
                            this.child(div().text_xs().text_color(muted).child(first_line))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .flex_shrink_0()
                        .when(switching, |this| {
                            this.child(div().text_xs().text_color(muted).child("Saving…"))
                        })
                        .child(
                            div()
                                .id(switch_id)
                                .when(!live, |this| this.opacity(0.5))
                                .when(live, |this| {
                                    this.cursor_pointer().on_mouse_down(
                                        MouseButton::Left,
                                        move |_, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.switch_ceiling_tool(name.clone(), !on, cx);
                                            });
                                        },
                                    )
                                })
                                .child(notify_switch(on)),
                        ),
                ),
        )
        .when_some(unavailable, |this, why| {
            this.child(div().id(why_id).text_xs().text_color(muted).child(why))
        })
        .when_some(connector, |this, words| {
            this.child(
                div()
                    .id(connector_id)
                    .text_xs()
                    .text_color(muted)
                    .child(words),
            )
        })
        .when_some(note, |this, words| {
            this.child(div().id(error_id).text_xs().text_color(danger).child(words))
        })
}

/// The Skills card's line when the account's library has nothing in it the owner may attach.
pub(crate) const NO_SKILLS_TO_ATTACH: &str =
    "No skills to attach yet. Write one in Settings → Skills.";

/// Under a skill switched off in Settings → Skills, which is dimmed: why, and where it is
/// switched back on. It can be detached here, and not attached.
pub(crate) const SWITCHED_OFF_IN_SETTINGS: &str = "Switched off in Settings → Skills";

/// On a shared Bot's Skills card: whoever uses the Bot can read the skills attached to it, which
/// the owner should know before attaching one (opengrok-server#270). Only where the roster says
/// the Bot is shared ([`SkillsCard::shared`]).
pub(crate) const SHARED_BOT_READS_SKILLS: &str =
    "People who use this Bot can read its attached skills.";

/// Under the switches: what attaching a skill does.
pub(crate) const WHAT_ATTACHING_DOES: &str = "This Bot is told about every skill attached here that is switched on, and reads one when it needs it.";

/// The Skills card's line: how many skills the switches show attached, and how many of those are
/// switched off, or why there are no switches. Nothing is counted that the server did not send.
pub(crate) fn skills_summary(skills: &BotSkills, pending: Option<&SkillSwitch>) -> String {
    match skills {
        BotSkills::Loading => "Asking the server…".to_string(),
        BotSkills::Unavailable(why) => why.clone(),
        BotSkills::Read(read) if read.rows.is_empty() => NO_SKILLS_TO_ATTACH.to_string(),
        BotSkills::Read(read) => match read.attached(pending) {
            (0, _) => "None attached".to_string(),
            (attached, 0) => format!("{attached} attached"),
            (attached, off) => format!("{attached} attached · {off} switched off"),
        },
    }
}

/// One skill on the Skills card, as it is drawn and as the driver is told it is drawn: both are
/// made from this, so the two cannot disagree about where a switch stands or whether it can move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShownSkillRow {
    /// The skill's id, which is what its switch is known by.
    pub id: String,
    /// The skill's name, which is what a person types after a slash for it.
    pub title: String,
    /// The first line of what the skill is for.
    pub first_line: String,
    /// The owner's own, filed under "Yours"; otherwise their organization's.
    pub mine: bool,
    /// Where the switch stands: attached as the server has it, or as it was asked to be while
    /// that is with the server.
    pub on: bool,
    /// This row's switch is the one with the server.
    pub switching: bool,
    /// The switch can be moved now. It cannot while the card is blocked ([`SkillsCard::blocked`])
    /// or to attach a skill switched off in Settings → Skills.
    pub live: bool,
    /// Switched off in Settings → Skills: the row is dimmed and says so.
    pub switched_off: bool,
    /// What the card says under this row about its last switch.
    pub note: Option<String>,
}

/// A line on the Skills card above its switches, as it is drawn and as the driver is told it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkillsCardLine {
    /// Why every switch is dead for good: the server's words for a 403.
    ReadOnly(String),
    /// Why every switch is dead for now: another Bot's skill switch, or a read of this one's
    /// skills, is with the server.
    Wait(String),
    /// What the server said about the last switch, when it is not about a row on the card: a
    /// stale version, or the cap on attached skills.
    Note(String),
}

/// The open Bot's skills as the Skills card draws them, in the server's order, or none while they
/// are not read.
pub(crate) fn shown_skill_rows(card: &SkillsCard) -> Vec<ShownSkillRow> {
    let BotSkills::Read(read) = &card.skills else {
        return Vec::new();
    };
    let pending = card.pending.as_ref();
    read.rows
        .iter()
        .map(|row| {
            let on = read.shown_attached(row, pending);
            ShownSkillRow {
                id: row.id.clone(),
                title: skill_title(row),
                first_line: first_line_of(&row.description),
                mine: row.scope == BotSkillScope::Mine,
                on,
                switching: pending.is_some_and(|switch| switch.skill_id == row.id),
                live: read.may_switch(&row.id, !on, card.blocked.as_ref()).is_ok(),
                switched_off: !row.enabled,
                note: card
                    .note
                    .as_ref()
                    .filter(|note| note.row() == Some(row.id.as_str()))
                    .map(|note| note.words.clone()),
            }
        })
        .collect()
}

/// The lines above the open Bot's skill switches, by the Tools card's rules
/// ([`ceiling_card_lines`]): why none can move, unless this Bot's own switch is showing it on its
/// row, and what the server said about the last switch when no row on the card is the one it is
/// about.
pub(crate) fn skills_card_lines(card: &SkillsCard) -> Vec<SkillsCardLine> {
    let BotSkills::Read(read) = &card.skills else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    match &card.blocked {
        Some(SkillsBlock::ReadOnly(words)) => lines.push(SkillsCardLine::ReadOnly(words.clone())),
        Some(blocked @ (SkillsBlock::AnotherBot | SkillsBlock::Reading)) => {
            lines.push(SkillsCardLine::Wait(blocked.why().to_string()));
        }
        Some(SkillsBlock::Switching) | None => {}
    }
    if let Some(note) = &card.note {
        let under_a_row = note
            .row()
            .is_some_and(|id| read.rows.iter().any(|row| row.id == id));
        // A read-only card's reason is already its first line.
        if !under_a_row && !matches!(card.blocked, Some(SkillsBlock::ReadOnly(_))) {
            lines.push(SkillsCardLine::Note(note.words.clone()));
        }
    }
    lines
}

/// A skill's name as its row is headed, as the library and the composer show it: the name with
/// its spaces run together, and a skill with none called what it is.
fn skill_title(row: &BotSkillRow) -> String {
    let name = row.name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        "Untitled skill".to_string()
    } else {
        name
    }
}

/// The switches: the owner's own skills and their organization's, under a heading each, below the
/// card's own lines, and on a shared Bot the line saying who can read what is attached. A switch
/// is sent the moment it is clicked, and only a live one takes a click.
fn skills_body(
    app: Entity<AppState>,
    rows: Vec<ShownSkillRow>,
    lines: Vec<SkillsCardLine>,
    shared: bool,
    muted: Hsla,
    danger: Hsla,
) -> impl IntoElement {
    let (mine, org): (Vec<_>, Vec<_>) = rows.into_iter().partition(|row| row.mine);
    let group = |title: &'static str, rows: Vec<ShownSkillRow>| {
        v_flex()
            .gap(px(8.))
            .child(div().text_xs().text_color(muted).child(title))
            .children(
                rows.into_iter()
                    .map(|row| skill_row(app.clone(), row, muted, danger)),
            )
    };
    v_flex()
        .pt(px(12.))
        .gap(px(12.))
        // Above the switches, because each is about all of them.
        .children(lines.into_iter().map(|line| {
            let (id, words) = match line {
                SkillsCardLine::ReadOnly(words) => ("agent-skills-read-only", words),
                SkillsCardLine::Wait(words) => ("agent-skills-wait", words),
                SkillsCardLine::Note(words) => ("agent-skills-note", words),
            };
            div().id(id).text_xs().text_color(muted).child(words)
        }))
        .when(!mine.is_empty(), |this| this.child(group("Yours", mine)))
        .when(!org.is_empty(), |this| {
            this.child(group("Your organization's", org))
        })
        .when(shared, |this| {
            this.child(
                div()
                    .id("agent-skills-shared")
                    .text_xs()
                    .text_color(muted)
                    .child(SHARED_BOT_READS_SKILLS),
            )
        })
        .child(div().text_xs().text_color(muted).child(WHAT_ATTACHING_DOES))
}

/// One skill: its name and the first line of what it is for, and its switch at the right, with
/// "Saving…" beside it while it is with the server. A skill switched off in Settings → Skills is
/// dimmed with why under it, and what the card says about the row's last switch is under it too.
fn skill_row(
    app: Entity<AppState>,
    row: ShownSkillRow,
    muted: Hsla,
    danger: Hsla,
) -> impl IntoElement {
    let ShownSkillRow {
        id,
        title,
        first_line,
        mine: _,
        on,
        switching,
        live,
        switched_off,
        note,
    } = row;
    // Every id under a row has a fixed word between `agent-skills-` and the skill's id, and the
    // card's own ids have none, so no id a skill can have makes one id another's.
    let switch_id = SharedString::from(format!("agent-skills-switch-{id}"));
    let off_id = SharedString::from(format!("agent-skills-off-{id}"));
    let error_id = SharedString::from(format!("agent-skills-error-{id}"));
    v_flex()
        .id(SharedString::from(format!("agent-skills-row-{id}")))
        .gap(px(2.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .child(
                    v_flex()
                        .min_w(px(0.))
                        .flex_1()
                        .gap(px(1.))
                        .when(switched_off, |this| this.opacity(0.5))
                        .child(div().text_sm().child(title))
                        .when(!first_line.is_empty(), |this| {
                            this.child(div().text_xs().text_color(muted).child(first_line))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .flex_shrink_0()
                        .when(switching, |this| {
                            this.child(div().text_xs().text_color(muted).child("Saving…"))
                        })
                        .child(
                            div()
                                .id(switch_id)
                                .when(!live, |this| this.opacity(0.5))
                                .when(live, |this| {
                                    this.cursor_pointer().on_mouse_down(
                                        MouseButton::Left,
                                        move |_, _, cx| {
                                            app.update(cx, |state, cx| {
                                                state.switch_bot_skill(id.clone(), !on, cx);
                                            });
                                        },
                                    )
                                })
                                .child(notify_switch(on)),
                        ),
                ),
        )
        .when(switched_off, |this| {
            this.child(
                div()
                    .id(off_id)
                    .text_xs()
                    .text_color(muted)
                    .child(SWITCHED_OFF_IN_SETTINGS),
            )
        })
        .when_some(note, |this, words| {
            this.child(div().id(error_id).text_xs().text_color(danger).child(words))
        })
}

#[cfg(test)]
mod tools_tests {
    use super::{
        CeilingCardLine, NO_LONGER_ON_THE_SERVER, NO_MAC_RUNS_COMMANDS, NOT_AVAILABLE_NOW,
        NOTHING_TO_SWITCH, ceiling_card_lines, ceiling_line, connector_note, first_line_of,
        model_line, shown_ceiling_rows, tools_summary, usage_summary,
    };
    use crate::opengrok::{CeilingRow, CoworkerTool};
    use crate::state::{
        ANOTHER_BOTS_SWITCH, CeilingBlock, CeilingCard, CeilingNote, CeilingRead, CeilingSwitch,
        NOT_THE_OWNER, NotePlace, ToolCeiling, ToolList,
    };

    fn rows(json: serde_json::Value) -> Vec<CeilingRow> {
        serde_json::from_value(json).expect("ceiling rows")
    }

    /// A ceiling with every kind of row the card draws: a builtin with words, `user_machine_shell`
    /// with no machine to run it, a plugin with its label and connection, a plugin with a label
    /// and nothing else, and one the server no longer loads, still on.
    fn a_read() -> CeilingRead {
        CeilingRead {
            rows: rows(serde_json::json!([
                {"name": "shell", "kind": "builtin", "enabled": true,
                    "description": "Run a shell command.\nIt runs on the Bot's own computer."},
                {"name": "user_machine_shell", "kind": "builtin", "enabled": false,
                    "available": false},
                {"name": "gmail", "kind": "plugin", "enabled": false, "label": "Gmail",
                    "description": "Read and send mail.", "connector": "gmail"},
                {"name": "notes", "kind": "plugin", "enabled": true, "label": "  "},
                {"name": "old_crm", "kind": "plugin", "enabled": true, "available": false}
            ])),
            version: Some(3),
        }
    }

    /// The card for [`a_read`], with nothing with the server and nothing said.
    fn a_card() -> CeilingCard {
        CeilingCard {
            ceiling: ToolCeiling::Read(a_read()),
            pending: None,
            blocked: None,
            note: None,
        }
    }

    fn asked(name: &str, enabled: bool) -> CeilingSwitch {
        CeilingSwitch {
            coworker_id: "cw_1".into(),
            name: name.into(),
            enabled,
            token: 1,
        }
    }

    /// The routine tools are one row of the ceiling, a builtin the server labels for people
    /// (opengrok-server #316, recorded at #334, on main 8e7387f: `{name: "routines", kind:
    /// "builtin", label: "Routines"}`), and it is headed by that label; its switch is still known
    /// by its name. Its line is the server's description as #349 words it, a Bot's own routines
    /// (`ROW_DESCRIPTION`, recorded in opengrok-server #351 at 9a2b011). A builtin with no label is
    /// headed by its wire name, as before.
    #[test]
    fn a_builtin_the_server_labels_is_headed_by_its_label() {
        let card = CeilingCard {
            ceiling: ToolCeiling::Read(CeilingRead {
                rows: rows(serde_json::json!([
                    {"name": "routines", "kind": "builtin", "enabled": true, "label": "Routines",
                        "description": "List, make, edit, delete and run this Bot's routines \
                                        when you ask in chat. Deleting one always asks you \
                                        first."},
                    {"name": "message_bot", "kind": "builtin", "enabled": true}
                ])),
                version: Some(1),
            }),
            pending: None,
            blocked: None,
            note: None,
        };
        let shown = shown_ceiling_rows(&card);
        assert_eq!(
            shown
                .iter()
                .map(|row| (row.name.as_str(), row.title.as_str(), row.builtin))
                .collect::<Vec<_>>(),
            [
                ("routines", "Routines", true),
                ("message_bot", "message_bot", true)
            ]
        );
        assert_eq!(
            shown[0].first_line,
            "List, make, edit, delete and run this Bot's routines when you ask in chat. \
             Deleting one always asks you first."
        );
    }

    /// The card's line about the ceiling counts the switches as they stand on screen, and says
    /// why there are none when there are none. While it is being asked for it says nothing: the
    /// card's own line is saying that already.
    #[test]
    fn the_tools_card_says_how_many_are_allowed() {
        let read = a_read();
        let line = |pending: Option<&CeilingSwitch>| {
            ceiling_line(&ToolCeiling::Read(read.clone()), pending)
        };
        assert_eq!(line(None).as_deref(), Some("3 of 5 allowed"));
        assert_eq!(
            line(Some(&asked("gmail", true))).as_deref(),
            Some("4 of 5 allowed"),
            "the count says what the switches say"
        );
        assert_eq!(ceiling_line(&ToolCeiling::Loading, None), None);
        assert_eq!(
            ceiling_line(&ToolCeiling::Unavailable(NOT_THE_OWNER.into()), None).as_deref(),
            Some(NOT_THE_OWNER)
        );
        let empty = CeilingRead {
            rows: Vec::new(),
            ..read
        };
        assert_eq!(
            ceiling_line(&ToolCeiling::Read(empty), None).as_deref(),
            Some(NOTHING_TO_SWITCH)
        );
    }

    /// Each row is drawn as the server describes it: by its label or else its name, which for a
    /// builtin is its wire name, the first line of its words, and why it cannot be offered when it
    /// cannot. Only a switch a click would send is live, and a builtin the server cannot offer
    /// now is one: the ceiling records what the owner allows.
    #[test]
    fn a_ceiling_row_is_drawn_as_the_server_describes_it() {
        let shown = shown_ceiling_rows(&a_card());
        let titles: Vec<&str> = shown.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(
            titles,
            ["shell", "user_machine_shell", "Gmail", "notes", "old_crm"],
            "a blank label is no label"
        );
        assert_eq!(shown[0].first_line, "Run a shell command.");
        assert!(shown[0].builtin && !shown[2].builtin);
        assert_eq!(
            shown.iter().map(|row| row.unavailable).collect::<Vec<_>>(),
            [
                None,
                Some(NO_MAC_RUNS_COMMANDS),
                None,
                None,
                Some(NO_LONGER_ON_THE_SERVER)
            ]
        );
        assert_eq!(
            shown[2].connector.as_deref(),
            Some("Uses your Gmail connection")
        );
        assert_eq!(
            shown.iter().map(|row| row.live).collect::<Vec<_>>(),
            [true, true, true, true, true],
            "an unavailable builtin goes either way; an unavailable plugin that is on can go off"
        );
        assert!(
            shown_ceiling_rows(&CeilingCard {
                ceiling: ToolCeiling::Loading,
                ..a_card()
            })
            .is_empty()
        );
    }

    /// Nothing is live while the card is blocked, and the card says why above the rows — except
    /// for this Bot's own switch, which its row shows where it was asked to go. The server's
    /// words sit under the row they are about and no other, and on the card when the row is not
    /// there to say them under.
    #[test]
    fn a_blocked_card_says_why_and_its_rows_are_dead() {
        let own = CeilingCard {
            pending: Some(asked("gmail", true)),
            blocked: Some(CeilingBlock::Switching),
            ..a_card()
        };
        let shown = shown_ceiling_rows(&own);
        assert!(shown.iter().all(|row| !row.live));
        assert!(shown[2].on && shown[2].switching);
        assert!(!shown[0].switching);
        assert!(
            ceiling_card_lines(&own).is_empty(),
            "the row itself says it is saving"
        );

        let another_bots = CeilingCard {
            blocked: Some(CeilingBlock::AnotherBot),
            ..a_card()
        };
        let shown = shown_ceiling_rows(&another_bots);
        assert!(shown.iter().all(|row| !row.live));
        assert!(!shown[2].on, "and nothing of another Bot's switch is drawn");
        assert_eq!(
            ceiling_card_lines(&another_bots),
            [CeilingCardLine::Wait(ANOTHER_BOTS_SWITCH.to_string())]
        );

        let read_only = CeilingCard {
            blocked: Some(CeilingBlock::ReadOnly("no grant to change".into())),
            note: Some(CeilingNote {
                coworker_id: "cw_1".into(),
                words: "no grant to change".into(),
                place: NotePlace::ReadOnly,
            }),
            ..a_card()
        };
        assert!(shown_ceiling_rows(&read_only).iter().all(|row| !row.live));
        assert_eq!(
            ceiling_card_lines(&read_only),
            [CeilingCardLine::ReadOnly("no grant to change".to_string())],
            "said once"
        );

        let said = |place: NotePlace| CeilingCard {
            note: Some(CeilingNote {
                coworker_id: "cw_1".into(),
                words: "no tool or plugin named gmail".into(),
                place,
            }),
            ..a_card()
        };
        let under_gmail = said(NotePlace::Row("gmail".into()));
        assert_eq!(
            shown_ceiling_rows(&under_gmail)
                .iter()
                .map(|row| row.note.as_deref())
                .collect::<Vec<_>>(),
            [
                None,
                None,
                Some("no tool or plugin named gmail"),
                None,
                None
            ]
        );
        assert!(ceiling_card_lines(&under_gmail).is_empty());
        let gone = said(NotePlace::Row("calendar".into()));
        assert_eq!(
            ceiling_card_lines(&gone),
            [CeilingCardLine::Note(
                "no tool or plugin named gmail".to_string()
            )],
            "no row to say it under"
        );
        assert!(
            shown_ceiling_rows(&gone)
                .iter()
                .all(|row| row.note.is_none())
        );
    }

    /// A connection is named as the server names it, and never dressed up: by the plugin's label
    /// when the connector is the plugin's own name, and by its id as sent otherwise. Only on a
    /// plugin.
    #[test]
    fn a_plugins_connection_is_named_as_the_server_names_it() {
        let row = |name: &str, label: Option<&str>, kind: &str, connector: &str| {
            rows(serde_json::json!([
                {"name": name, "kind": kind, "enabled": true, "label": label,
                    "connector": connector}
            ]))
            .remove(0)
        };
        assert_eq!(
            connector_note(&row("gmail", Some("Gmail"), "plugin", "gmail")).as_deref(),
            Some("Uses your Gmail connection")
        );
        assert_eq!(
            connector_note(&row("gmail", Some("gmail"), "plugin", "gmail")).as_deref(),
            Some("Uses your gmail connection"),
            "the server's own label, as it spells it"
        );
        assert_eq!(
            connector_note(&row(
                "drive_sync",
                Some("Drive sync"),
                "plugin",
                "google_drive"
            ))
            .as_deref(),
            Some("Uses your google_drive connection"),
            "another connector's id, as sent"
        );
        assert_eq!(
            connector_note(&row("gmail", None, "plugin", "gmail")).as_deref(),
            Some("Uses your gmail connection")
        );
        assert_eq!(connector_note(&row("x", None, "plugin", " ")), None);
        assert_eq!(connector_note(&row("x", None, "builtin", "gmail")), None);
        let odd = rows(serde_json::json!([
            {"name": "fetch", "kind": "builtin", "enabled": false, "available": false}
        ]));
        assert_eq!(
            super::unavailable_why(&odd[0]),
            Some(NOT_AVAILABLE_NOW),
            "a builtin the server could not offer, and did not say why"
        );
    }

    fn tool(name: &str, kind: &str) -> CoworkerTool {
        CoworkerTool {
            name: name.into(),
            description: String::new(),
            kind: kind.into(),
        }
    }

    /// The card's second line says where the bot's tools come from, and says so plainly when
    /// there is no list rather than showing an empty card.
    #[test]
    fn the_tools_card_says_what_the_bot_is_offered() {
        let both = ToolList::Listed(vec![
            tool("shell", "builtin"),
            tool("read_file", "builtin"),
            tool("gmail_api_send", "plugin"),
        ]);
        assert_eq!(tools_summary(&both), "2 built in · 1 from plugins");
        assert_eq!(
            tools_summary(&ToolList::Listed(vec![tool("shell", "builtin")])),
            "1 built in"
        );
        assert_eq!(
            tools_summary(&ToolList::Listed(vec![tool("x", "connector")])),
            "1 from plugins"
        );
        assert_eq!(
            tools_summary(&ToolList::Listed(Vec::new())),
            "Offered no tools on its next turn."
        );
        assert_eq!(tools_summary(&ToolList::Loading), "Asking the server…");
        assert_eq!(
            tools_summary(&ToolList::Unavailable(
                "Only this bot's owner can see its tools.".into()
            )),
            "Only this bot's owner can see its tools."
        );
    }

    #[test]
    fn a_long_description_is_cut_at_a_word() {
        assert_eq!(first_line_of("Short.\nMore below."), "Short.");
        let long = "word ".repeat(60);
        let shown = first_line_of(&long);
        assert!(
            shown.ends_with('…') && shown.chars().count() <= 161,
            "{shown}"
        );
        assert!(!shown.contains("wor…"), "not mid-word: {shown}");
        let cjk = "字".repeat(300);
        assert_eq!(
            first_line_of(&cjk).chars().count(),
            161,
            "no spaces: cut on a char"
        );
    }

    /// The Usage card says what the server says the bot used, and never "no usage" for a bot that
    /// used some, nor for one the server does not measure (#138).
    #[test]
    fn the_usage_card_says_what_the_server_measured() {
        use crate::opengrok::{CoworkerUsage, ModelUsage};
        use crate::state::UsageReport;
        let used = CoworkerUsage {
            metered: true,
            note: None,
            window: "month".into(),
            models: vec![ModelUsage {
                model_id: "oag/cheap".into(),
                requests: 1234,
                input_tokens: 20000,
                output_tokens: 1000,
                cache_read_tokens: 500,
                cache_write_tokens: 0,
                cost_usd: "2.000000".into(),
            }],
            totals: crate::opengrok::UsageTotals {
                requests: Some(1234),
                input_tokens: Some(20000),
                output_tokens: Some(1000),
                cache_read_tokens: Some(500),
                cache_write_tokens: Some(0),
                cost_usd: Some("2.000000".into()),
            },
        };
        assert_eq!(
            usage_summary(&UsageReport::Read(used.clone())),
            "1,234 requests this month · 21,500 tokens · $2.00"
        );
        assert_eq!(
            model_line(&used.models[0]),
            "oag/cheap · 1,234 requests · 21,500 tokens · $2.00"
        );
        let one = ModelUsage {
            requests: 1,
            input_tokens: 900,
            output_tokens: 1,
            cache_read_tokens: 0,
            cost_usd: "0.004000".into(),
            ..used.models[0].clone()
        };
        assert_eq!(
            model_line(&one),
            "oag/cheap · 1 request · 901 tokens · under $0.01"
        );
        let idle = CoworkerUsage {
            models: Vec::new(),
            totals: crate::opengrok::UsageTotals {
                requests: Some(0),
                ..Default::default()
            },
            ..used.clone()
        };
        assert_eq!(
            usage_summary(&UsageReport::Read(idle.clone())),
            "No requests this month",
            "a fresh bot with real zeros and no note"
        );
        // Every attempt paid for but lost: no requests counted, yet a model with tokens and cost.
        let lost = CoworkerUsage {
            models: vec![ModelUsage {
                requests: 0,
                ..used.models[0].clone()
            }],
            totals: crate::opengrok::UsageTotals {
                requests: Some(0),
                ..used.totals.clone()
            },
            ..used.clone()
        };
        assert_eq!(
            usage_summary(&UsageReport::Read(lost)),
            "0 requests this month · 21,500 tokens · $2.00"
        );
        // Metered, but the gateway could not be asked: the server sends zero totals with a
        // note, and those zeros are not a measurement.
        let unread = CoworkerUsage {
            note: Some("the gateway could not be asked: timed out".into()),
            ..idle
        };
        assert_eq!(
            usage_summary(&UsageReport::Read(unread)),
            "The gateway could not be asked: timed out"
        );
        let unmetered = CoworkerUsage {
            metered: false,
            note: Some("this coworker's key cannot serve".into()),
            models: Vec::new(),
            totals: Default::default(),
            ..used
        };
        assert_eq!(
            usage_summary(&UsageReport::Read(unmetered.clone())),
            "This coworker's key cannot serve"
        );
        let unexplained = CoworkerUsage {
            metered: false,
            note: None,
            ..unmetered
        };
        assert_eq!(
            usage_summary(&UsageReport::Read(unexplained)),
            "This bot's use is not measured."
        );
        assert_eq!(usage_summary(&UsageReport::Loading), "Asking the server…");
        assert_eq!(
            usage_summary(&UsageReport::Unavailable(
                "Only this bot's owner can see its usage.".into()
            )),
            "Only this bot's owner can see its usage."
        );
    }

    /// The cents are rounded from the server's decimal string, not through a float, which puts
    /// "1.005000" at 1.00499… and a cent short.
    #[test]
    fn dollars_round_the_servers_decimals_to_the_cent() {
        for (six, read) in [
            ("2.000000", Some("$2.00")),
            ("0.015000", Some("$0.02")),
            ("1.005000", Some("$1.01")),
            ("1.004999", Some("$1.00")),
            ("0.005000", Some("$0.01")),
            ("0.004000", Some("under $0.01")),
            ("0.000001", Some("under $0.01")),
            ("0.000000", Some("$0.00")),
            ("1234.5", Some("$1,234.50")),
            ("7", Some("$7.00")),
            ("-1.000000", None),
            ("", None),
            ("abc", None),
            ("1.2.3", None),
        ] {
            assert_eq!(super::dollars(six).as_deref(), read, "{six:?}");
        }
    }
}

#[cfg(test)]
mod skills_tests {
    use super::{
        NO_SKILLS_TO_ATTACH, SkillsCardLine, shown_skill_rows, skills_card_lines, skills_summary,
    };
    use crate::state::{
        ANOTHER_BOTS_SKILL_SWITCH, BotSkills, NOT_THE_SKILLS_OWNER, SKILLS_BEING_READ, SkillNote,
        SkillNotePlace, SkillSwitch, SkillsBlock, SkillsCard, SkillsRead,
    };

    /// Every kind of skill the card draws: the owner's own attached with two lines of words, one
    /// of theirs not attached whose name has stray spaces, a colleague's attached, one of the
    /// owner's switched off and still attached, one switched off and not, and one with no name.
    fn a_read() -> SkillsRead {
        SkillsRead {
            rows: serde_json::from_value(serde_json::json!([
                {"id": "sk_triage", "name": "triage", "description": "Sort the inbox.\nThen reply.",
                    "scope": "mine", "attached": true, "enabled": true},
                {"id": "sk_draft", "name": "  first   draft ", "description": "",
                    "scope": "mine", "attached": false, "enabled": true},
                {"id": "sk_review", "name": "review", "description": "The team's checklist.",
                    "scope": "org", "attached": true, "enabled": true},
                {"id": "sk_old", "name": "old-notes", "description": "Last year's.",
                    "scope": "mine", "attached": true, "enabled": false},
                {"id": "sk_archive", "name": "archive", "description": "",
                    "scope": "org", "attached": false, "enabled": false},
                {"id": "sk_blank", "name": " ", "description": "",
                    "scope": "mine", "attached": false, "enabled": true}
            ]))
            .expect("skill rows"),
            version: Some(4),
        }
    }

    /// The card for [`a_read`], with nothing with the server and nothing said.
    fn a_card() -> SkillsCard {
        SkillsCard {
            skills: BotSkills::Read(a_read()),
            pending: None,
            blocked: None,
            note: None,
            shared: false,
        }
    }

    fn asked(skill_id: &str, attached: bool) -> SkillSwitch {
        SkillSwitch {
            coworker_id: "cw_1".into(),
            skill_id: skill_id.into(),
            attached,
            token: 1,
        }
    }

    /// The card's line counts the switches as they stand on screen — a skill switched off in
    /// Settings → Skills and still attached is attached, and said to be switched off — and says
    /// why there are none when there are none.
    #[test]
    fn the_skills_card_says_how_many_are_attached() {
        let read = BotSkills::Read(a_read());
        assert_eq!(skills_summary(&read, None), "3 attached · 1 switched off");
        assert_eq!(
            skills_summary(&read, Some(&asked("sk_draft", true))),
            "4 attached · 1 switched off",
            "the count says what the switches say"
        );
        assert_eq!(
            skills_summary(&read, Some(&asked("sk_old", false))),
            "2 attached",
            "nothing switched off is attached any more"
        );
        let mut none = a_read();
        for row in &mut none.rows {
            row.attached = false;
        }
        assert_eq!(
            skills_summary(&BotSkills::Read(none), None),
            "None attached"
        );
        let empty = SkillsRead {
            rows: Vec::new(),
            version: Some(1),
        };
        assert_eq!(
            skills_summary(&BotSkills::Read(empty), None),
            NO_SKILLS_TO_ATTACH
        );
        assert_eq!(
            skills_summary(&BotSkills::Loading, None),
            "Asking the server…"
        );
        assert_eq!(
            skills_summary(&BotSkills::Unavailable(NOT_THE_SKILLS_OWNER.into()), None),
            NOT_THE_SKILLS_OWNER
        );
    }

    /// Each skill is drawn as the server describes it: its name as a person types it, the first
    /// line of what it is for, filed under the owner's or the organization's, and dimmed when it
    /// is switched off. Every switch is live but one that would attach a skill switched off.
    #[test]
    fn a_skill_row_is_drawn_as_the_server_describes_it() {
        let shown = shown_skill_rows(&a_card());
        let titles: Vec<&str> = shown.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "triage",
                "first draft",
                "review",
                "old-notes",
                "archive",
                "Untitled skill"
            ]
        );
        assert_eq!(shown[0].first_line, "Sort the inbox.");
        assert_eq!(
            shown.iter().map(|row| row.mine).collect::<Vec<_>>(),
            [true, true, false, true, false, true]
        );
        assert_eq!(
            shown.iter().map(|row| row.switched_off).collect::<Vec<_>>(),
            [false, false, false, true, true, false]
        );
        assert_eq!(
            shown.iter().map(|row| row.live).collect::<Vec<_>>(),
            [true, true, true, true, false, true],
            "a switched-off skill can be detached, and not attached"
        );
        assert!(
            shown_skill_rows(&SkillsCard {
                skills: BotSkills::Loading,
                ..a_card()
            })
            .is_empty()
        );
    }

    /// Nothing is live while the card is blocked, and the card says why above the rows — except
    /// for this Bot's own switch, which its row shows where it was asked to go. The server's words
    /// sit under the skill they are about, and on the card when no skill on it is.
    #[test]
    fn a_blocked_skills_card_says_why_and_its_rows_are_dead() {
        let own = SkillsCard {
            pending: Some(asked("sk_draft", true)),
            blocked: Some(SkillsBlock::Switching),
            ..a_card()
        };
        let shown = shown_skill_rows(&own);
        assert!(shown.iter().all(|row| !row.live));
        assert!(shown[1].on && shown[1].switching);
        assert!(
            skills_card_lines(&own).is_empty(),
            "the row says it is saving"
        );

        for (blocked, why) in [
            (SkillsBlock::AnotherBot, ANOTHER_BOTS_SKILL_SWITCH),
            (SkillsBlock::Reading, SKILLS_BEING_READ),
        ] {
            let waiting = SkillsCard {
                blocked: Some(blocked),
                ..a_card()
            };
            assert!(shown_skill_rows(&waiting).iter().all(|row| !row.live));
            assert_eq!(
                skills_card_lines(&waiting),
                [SkillsCardLine::Wait(why.to_string())]
            );
        }

        let said = |place: SkillNotePlace| SkillNote {
            coworker_id: "cw_1".into(),
            words: "no skill sk_review".into(),
            place,
        };
        let under_review = SkillsCard {
            note: Some(said(SkillNotePlace::Row("sk_review".into()))),
            ..a_card()
        };
        assert_eq!(
            shown_skill_rows(&under_review)
                .iter()
                .map(|row| row.note.as_deref())
                .collect::<Vec<_>>(),
            [None, None, Some("no skill sk_review"), None, None, None]
        );
        assert!(skills_card_lines(&under_review).is_empty());
        let gone = SkillsCard {
            note: Some(said(SkillNotePlace::Row("sk_gone".into()))),
            ..a_card()
        };
        assert_eq!(
            skills_card_lines(&gone),
            [SkillsCardLine::Note("no skill sk_review".to_string())],
            "no skill on the card to say it under"
        );

        let read_only = SkillsCard {
            blocked: Some(SkillsBlock::ReadOnly("your grant was withdrawn".into())),
            note: Some(SkillNote {
                coworker_id: "cw_1".into(),
                words: "your grant was withdrawn".into(),
                place: SkillNotePlace::ReadOnly,
            }),
            ..a_card()
        };
        assert!(shown_skill_rows(&read_only).iter().all(|row| !row.live));
        assert_eq!(
            skills_card_lines(&read_only),
            [SkillsCardLine::ReadOnly(
                "your grant was withdrawn".to_string()
            )],
            "said once"
        );
    }
}
