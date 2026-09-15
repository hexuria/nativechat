use crate::chrome::{AVATAR_COLORS, AVATAR_SHAPES, AVATAR_TRIGGER_PX, INFO_PANE_WIDTH};
use crate::components::fields::field_input;
use crate::components::persona::PersonaMark;
use crate::opengrok::{CoworkerPatch, ModelEntry};
use crate::state::AppState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme, Icon, Selectable, v_flex};
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

#[derive(IntoElement)]
struct ModelFieldTrigger {
    selected: bool,
    field: Input,
}

impl Selectable for ModelFieldTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for ModelFieldTrigger {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .id("agent-model-field")
            .w(px(PANE_INNER))
            .child(self.field)
    }
}

pub struct AgentSettings {
    state: Entity<AppState>,
    name_input: Entity<InputState>,
    label_input: Entity<InputState>,
    role_input: Entity<TextareaState>,
    model_input: Entity<InputState>,
    synced_id: Option<String>,
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
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        Self {
            state,
            name_input,
            label_input,
            role_input,
            model_input,
            synced_id: None,
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
    }

    fn patch(&self, patch: CoworkerPatch, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.patch_active_agent(patch, cx);
        });
    }

    fn commit_profile(&mut self, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).value().to_string();
        let title = self.label_input.read(cx).value().to_string();
        let role = self.role_input.read(cx).value().to_string();
        self.patch(
            CoworkerPatch {
                name: Some(name),
                title: Some(title),
                role: Some(role),
                ..Default::default()
            },
            cx,
        );
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
        let usage_open = self.usage_open;
        let auto_review_open = self.auto_review_open;
        let auto_review_mode = self.auto_review_mode;
        let has_custom = shape.is_some() || color.is_some();
        let app = self.state.clone();

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
            .child(
                div()
                    .id("agent-settings-header")
                    .w_full()
                    .px(px(12.))
                    .h(px(44.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Settings"),
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
                            .on_mouse_down(MouseButton::Left, {
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |state, cx| {
                                        state.close_right_pane(cx);
                                    });
                                }
                            })
                            .child(
                                Icon::default()
                                    .path("icons/chevrons-right.svg")
                                    .size(px(16.)),
                            ),
                    ),
            )
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
                                                                        .child("Get notified when this agent finishes or needs input"),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .id("agent-notify-switch")
                                                                .cursor_pointer()
                                                                .on_mouse_down(
                                                                    MouseButton::Left,
                                                                    {
                                                                        let app = app.clone();
                                                                        move |_, _, cx| {
                                                                            app.update(cx, |state, cx| {
                                                                                let next = !notify;
                                                                                state.patch_active_agent(
                                                                                    CoworkerPatch {
                                                                                        notify_on_updates: Some(next),
                                                                                        ..Default::default()
                                                                                    },
                                                                                    cx,
                                                                                );
                                                                            });
                                                                        }
                                                                    },
                                                                )
                                                                .child(notify_switch(notify)),
                                                        ),
                                                ),
                                        ),
                                    )
                                    .child(heading("Model", muted))
                                    .child(
                                        div()
                                            .id("agent-model")
                                            .w_full()
                                            .child(
                                                Popover::new("agent-model-pop")
                                                    .appearance(false)
                                                    .overlay_closable(true)
                                                    .open(model_open)
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
                                                    .trigger(ModelFieldTrigger {
                                                        selected: model_open,
                                                        field: settings_input(&self.model_input),
                                                    })
                                                    .content({
                                                        let app = app.clone();
                                                        let theme = theme.clone();
                                                        let model = model.clone();
                                                        let catalogue = catalogue.clone();
                                                        move |_, _, _| {
                                                            model_picker_panel(
                                                                app.clone(),
                                                                catalogue.clone(),
                                                                model.clone(),
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

fn model_picker_panel(
    app: Entity<AppState>,
    catalogue: Vec<ModelEntry>,
    current: String,
    dark: bool,
    theme: gpui_kit::component::Theme,
) -> impl IntoElement {
    let list_bg = if dark { rgb(0x1c1c1c) } else { rgb(0xffffff) };
    v_flex()
        .id("agent-model-list")
        .w(px(PANE_INNER))
        .max_h(px(262.))
        .overflow_y_scroll()
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
        .children(catalogue.into_iter().map(move |m| {
            let mid = m.id;
            let selected = mid == current;
            let app = app.clone();
            div()
                .id(SharedString::from(format!("model-{mid}")))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(selected, |this| this.bg(rgb(0x777777).opacity(0.16)))
                .hover(|s| s.bg(rgb(0x777777).opacity(0.16)))
                .on_mouse_down(MouseButton::Left, {
                    let mid = mid.clone();
                    move |_, _, cx| {
                        app.update(cx, |state, cx| {
                            state.set_model_picker_open(false, cx);
                            state.patch_active_agent(
                                CoworkerPatch {
                                    model: Some(mid.clone()),
                                    ..Default::default()
                                },
                                cx,
                            );
                        });
                    }
                })
                .child(div().text_sm().child(mid))
        }))
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
