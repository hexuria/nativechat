use crate::services::database::Credential;
use crate::state::AppState;
use chrono::NaiveDate;
use gpui_kit::prelude::*;
use gpui_kit::{InteractiveElement, *};
use std::rc::Rc;
use crate::icons::NativeIcon;
use gpui_kit::component::Icon;
use gpui_kit::component::IconName;
use gpui_kit::component::IndexPath;
use gpui_kit::component::select::SearchableVec;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::calendar::Date;
use gpui_kit::component::date_picker::{DatePicker, DatePickerEvent, DatePickerState};
use gpui_kit::component::input::InputEvent;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::label::Label;
use gpui_kit::component::list::{List, ListDelegate, ListItem, ListState};
use gpui_kit::component::scroll::ScrollbarAxis;
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::component::{Sizable, Size, StyledExt};

actions!(credentials_modal, [SubmitCredential]);

#[derive(Debug, Clone, PartialEq)]
pub enum CredentialMode {
    Editing(i64),
    Creating,
}

pub struct CredentialsModal {
    state: Entity<AppState>,
    credentials: Vec<Credential>,
    list_state: Entity<ListState<CredentialsListDelegate>>,
    name_input: Entity<InputState>,
    provider_select: Entity<SelectState<SearchableVec<String>>>,
    api_key_input: Entity<InputState>,

    should_clear_inputs: bool,
    error_message: Option<String>,

    // New fields for Master-Detail layout
    sidebar_open: bool,
    selected_index: Option<usize>,
    mode: CredentialMode,
    last_window_width: Option<Pixels>,
    is_saving: bool,
    show_form: bool,
    search_input: Entity<InputState>,
    date_picker: Entity<DatePickerState>,
    expiration_date: Option<NaiveDate>,
    pending_providers: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct CredentialsListDelegate {
    credentials: Vec<Credential>,
    selected_index: Option<usize>,
    on_click: Rc<dyn Fn(usize, &mut Window, &mut App)>,
}

impl CredentialsListDelegate {
    pub fn new(
        credentials: Vec<Credential>,
        on_click: Rc<dyn Fn(usize, &mut Window, &mut App)>,
    ) -> Self {
        Self {
            credentials,
            selected_index: None,
            on_click,
        }
    }
}

impl ListDelegate for CredentialsListDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.credentials.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let credential = self.credentials.get(ix.row)?;
        let theme = cx.theme();
        let is_selected = self.selected_index == Some(ix.row);
        let on_click = self.on_click.clone();

        Some(
            ListItem::new(ix)
                .p_1()
                .child(
                    div()
                        .w_full()
                        .p_2()
                        .rounded_md()
                        .hover(|s| s.bg(theme.secondary.opacity(0.5)))
                        .when(is_selected, |s| s.bg(theme.secondary))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .child(credential.name.clone())
                                .font_weight(FontWeight::MEDIUM)
                                .text_sm()
                                .text_color(theme.foreground),
                        )
                        .child(
                            div()
                                .child(credential.provider.clone())
                                .text_xs()
                                .text_color(theme.muted_foreground),
                        ),
                )
                .on_click(move |_, window, cx| {
                    on_click(ix.row, window, cx);
                }),
        )
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix.map(|ix| ix.row);
    }
}

impl CredentialsModal {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let name_input =
                cx.new(|cx| InputState::new(window, cx).placeholder("Name (e.g. My Gemini Key)"));

            let mut providers: Vec<String> = state
                .read(cx)
                .available_models
                .iter()
                .map(|m| m.provider.to_string())
                .collect();

            // Removed hardcoded defaults as per user request
            // Providers should be seeded from backend/database
            providers.sort();
            providers.dedup();

            let provider_items = SearchableVec::new(providers);
            let provider_select = cx.new(|cx| {
                SelectState::new(provider_items, Some(IndexPath::new(0)), window, cx)
                    .searchable(true)
            });
            let api_key_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("API Key")
                    .masked(true)
            });

            let weak_self = cx.entity().downgrade();
            let on_click = Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
                weak_self
                    .update(cx, |this: &mut CredentialsModal, cx| {
                        // Update selection state FIRST to avoid input observer overwriting old credential
                        this.selected_index = Some(index);
                        this.list_state.update(cx, |list, cx| {
                            list.delegate_mut().selected_index = Some(index);
                            cx.notify();
                        });

                        // THEN load the credential
                        this.load_credential(index, window, cx);
                    })
                    .ok();
            });

            let delegate = CredentialsListDelegate::new(vec![], on_click);
            let list_state = cx.new(|cx| ListState::new(delegate, window, cx));

            let search_input =
                cx.new(|cx| InputState::new(window, cx).placeholder("Search credentials..."));

            let date_picker = cx.new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d"));

            let state_clone = state.clone();

            let mut this = Self {
                state,
                credentials: Vec::new(),
                list_state,
                name_input,
                provider_select,
                api_key_input,
                // Keeping this for now to avoid breaking other code immediately, but will replace usage
                should_clear_inputs: false,
                error_message: None,
                sidebar_open: true,
                selected_index: None,
                mode: CredentialMode::Creating,
                last_window_width: None,
                is_saving: false,
                show_form: false,
                search_input,
                date_picker,
                expiration_date: None,
                pending_providers: None,
            };

            // Subscribe to search input changes
            cx.subscribe(&this.search_input, |this, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.filter_credentials(cx);
                }
            })
            .detach();

            // Observe name input changes for live update
            cx.observe(&this.name_input, |this, input, cx| {
                let name = input.read(cx).value();
                if let Some(selected_index) = this.selected_index {
                    this.list_state.update(cx, |list, cx| {
                        if let Some(cred) = list.delegate_mut().credentials.get_mut(selected_index)
                        {
                            cred.name = if name.is_empty() {
                                "Untitled".to_string()
                            } else {
                                name.to_string()
                            };
                            cx.notify();
                        }
                    });
                }
            })
            .detach();

            // Subscribe to date picker changes
            cx.subscribe(&this.date_picker, |this, _, event: &DatePickerEvent, _| {
                if let DatePickerEvent::Change(date) = event {
                    if let Some(naive_date) = date.start() {
                        this.expiration_date = Some(naive_date);
                    } else {
                        this.expiration_date = None;
                    }
                }
            })
            .detach();

            // Subscribe to modal open state to refresh data
            cx.observe(&state_clone, |this: &mut Self, state, cx| {
                let (is_open, providers) = {
                    let state = state.read(cx);
                    let providers: Vec<String> = state
                        .available_models
                        .iter()
                        .map(|m| m.provider.to_string())
                        .collect();
                    (state.is_credentials_modal_open, providers)
                };

                if is_open {
                    this.fetch_credentials(cx);

                    // Update providers list
                    let mut providers = providers;
                    providers.sort();
                    providers.dedup();

                    this.pending_providers = Some(providers);
                }
            })
            .detach();

            this.fetch_credentials(cx);
            this
        })
    }

    fn filter_credentials(&mut self, cx: &mut Context<Self>) {
        let query = self.search_input.read(cx).value().to_lowercase();
        let filtered: Vec<Credential> = self
            .credentials
            .iter()
            .filter(|c| {
                c.name.to_lowercase().contains(&query) || c.provider.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();

        self.list_state.update(cx, |list, cx| {
            list.delegate_mut().credentials = filtered;
            cx.notify();
        });
    }

    fn load_credential(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // If we were creating a credential and switched away, remove the ephemeral one
        if let CredentialMode::Creating = self.mode {
            self.list_state.update(cx, |list, cx| {
                let delegate = list.delegate_mut();
                if let Some(pos) = delegate.credentials.iter().position(|c| c.id == -1) {
                    if pos != index {
                        delegate.credentials.remove(pos);
                        cx.notify();
                    }
                }
            });
        }

        // Get the actual credential from the filtered list in the delegate
        let credential = {
            let list_state = self.list_state.read(cx);
            let delegate = list_state.delegate();

            // Adjust index if we removed an item
            let adjusted_index = if let CredentialMode::Creating = self.mode {
                index
            } else {
                index
            };

            delegate.credentials.get(adjusted_index).cloned()
        };

        if let Some(cred) = credential {
            self.mode = CredentialMode::Editing(cred.id);
            self.selected_index = Some(index);
            self.show_form = true;

            // Populate form
            self.name_input.update(cx, |input, cx| {
                input.set_value(cred.name.clone(), window, cx)
            });
            self.api_key_input.update(cx, |input, cx| {
                input.set_value(cred.api_key.clone(), window, cx)
            });

            // Set provider
            self.provider_select.update(cx, |select, cx| {
                select.set_selected_value(&cred.provider, window, cx);
            });

            // Set expiration if we had it (currently not in DB model, but preparing UI)
            // self.date_picker.update(cx, |picker, cx| picker.set_date(...));

            cx.notify();
        }
    }

    fn fetch_credentials(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);

        if let Some(db) = &state.database_service {
            let db = db.clone();
            cx.spawn(async move |view, cx| {
                        match db.get_credentials().await {
                            Ok(creds) => {
                                view.update(cx, |this, cx| {
                                    this.credentials = creds.clone();
                                    this.list_state.update(cx, |list, cx| {
                                        list.delegate_mut().credentials = creds;
                                        cx.notify();
                                    });
                                    cx.notify();
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to fetch credentials: {}", e),
                        }
                    },
            )
            .detach();
        }
    }

    fn add_credential(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).value().to_string();
        let provider = self
            .provider_select
            .read(cx)
            .selected_value()
            .cloned()
            .unwrap_or_default();
        let api_key = self.api_key_input.read(cx).value().to_string();

        println!(
            "Attempting to save credential: name='{}', provider='{}', api_key='{}'",
            name, provider, api_key
        );

        if name.trim().is_empty() || provider.trim().is_empty() || api_key.trim().is_empty() {
            println!("Validation failed: fields are empty");
            self.error_message = Some("All fields are required.".to_string());
            cx.notify();
            return;
        }

        self.error_message = None;

        let state = self.state.read(cx);
        let mode = self.mode.clone();

        if let Some(db) = &state.database_service {
            let db = db.clone();
            cx.spawn(async move |view, cx| {
                        let result = match mode {
                            CredentialMode::Creating => db
                                .create_credential(&name, &provider, &api_key)
                                .await
                                .map(|_| ()),
                            CredentialMode::Editing(id) => {
                                db.update_credential(id, &name, &provider, &api_key).await
                            }
                        };

                        match result {
                            Ok(_) => {
                                view.update(cx, |this, cx| {
                                    this.should_clear_inputs = true;
                                    // Reset mode to creating after save
                                    this.mode = CredentialMode::Creating;
                                    this.show_form = false;
                                    this.fetch_credentials(cx);
                                    // Trigger global state refresh to update LLM provider immediately
                                    this.state.update(cx, |state, cx| state.reload_from_db(cx));
                                    cx.notify();
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to save credential: {}", e),
                        }
                    },
            )
            .detach();
        }
    }

    fn delete_credential(&mut self, id: i64, _window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);

        if let Some(db) = &state.database_service {
            let db = db.clone();
            cx.spawn(async move |view, cx| {
                        match db.delete_credential(id).await {
                            Ok(_) => {
                                view.update(cx, |this, cx| {
                                    this.fetch_credentials(cx);
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to delete credential: {}", e),
                        }
                    },
            )
            .detach();
        }
    }
}

impl Render for CredentialsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(providers) = self.pending_providers.take() {
            self.provider_select.update(cx, |s, cx| {
                s.set_items(SearchableVec::new(providers), window, cx);
            });
        }

        let window_width = window.viewport_size().width;
        let is_small_screen = window_width < px(650.0);

        // Auto-hide/show sidebar on resize
        if let Some(last_width) = self.last_window_width {
            if last_width >= px(650.0) && window_width < px(650.0) {
                self.sidebar_open = false;
            } else if last_width < px(650.0) && window_width >= px(650.0) {
                self.sidebar_open = true;
            }
        }
        self.last_window_width = Some(window_width);

        let sidebar = if self.sidebar_open {
            let mut sidebar_div = div()
                .w_72()
                .border_r_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .flex()
                .flex_col()
                .child(
                    div()
                        .p_4()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(Label::new("Credentials").font_weight(FontWeight::BOLD))
                        .child(
                            Button::new("new_credential")
                                .icon(IconName::Plus)
                                .ghost()
                                .tooltip("Create New Credential")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.mode = CredentialMode::Creating;
                                    this.show_form = true;

                                    // Clear inputs
                                    this.name_input
                                        .update(cx, |i, cx| i.set_value("", window, cx));
                                    this.api_key_input
                                        .update(cx, |i, cx| i.set_value("", window, cx));
                                    this.provider_select
                                        .update(cx, |s, cx| s.set_selected_index(None, window, cx));
                                    this.date_picker.update(cx, |d, cx| {
                                        d.set_date(Date::Single(None), window, cx)
                                    });
                                    this.expiration_date = None;

                                    // Add ephemeral "Untitled" credential
                                    this.list_state.update(cx, |list, cx| {
                                        let delegate = list.delegate_mut();

                                        // Remove any existing ephemeral credentials first
                                        delegate.credentials.retain(|c| c.id != -1);

                                        let new_cred = Credential {
                                            id: -1,
                                            name: "Untitled".to_string(),
                                            provider: "".to_string(),
                                            api_key: "".to_string(),
                                            created_at: String::new(),
                                        };

                                        delegate.credentials.push(new_cred);
                                        let new_index = delegate.credentials.len() - 1;

                                        // Update ListState's selected_index via set_selected_index
                                        list.set_selected_index(
                                            Some(IndexPath::new(new_index)),
                                            window,
                                            cx,
                                        );
                                        this.selected_index = Some(new_index);

                                        cx.notify();
                                    });
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div().p_2().child(
                        Input::new(&self.search_input)
                            .prefix(
                                Icon::new(IconName::Search).text_color(cx.theme().muted_foreground),
                            )
                            .appearance(false)
                            .border_color(cx.theme().border)
                            .border_1()
                            .rounded(cx.theme().radius)
                            .focus_bordered(false)
                            .when(
                                self.search_input.read(cx).focus_handle(cx).is_focused(window),
                                |this| this.border_color(cx.theme().primary),
                            ),
                    ),
                )
                .child(div().flex_1().child(if !self.credentials.is_empty() {
                    List::new(&self.list_state)
                        .with_size(Size::Small)
                        .h_full()
                        .w_full()
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .h_full()
                        .p_4()
                        .gap_2()
                        .child(
                            Label::new("No credentials yet")
                                .text_sm()
                                .font_weight(FontWeight::BOLD)
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            Label::new("Create a new credential to get started")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .text_align(TextAlign::Center),
                        )
                        .into_any_element()
                }));

            if is_small_screen {
                sidebar_div = sidebar_div
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .occlude()
                    .shadow_lg();
            }

            sidebar_div
        } else {
            div().hidden()
        };

        let main_content = if self.show_form {
            div()
                .flex_1()
                .p_6()
                .flex()
                .flex_col()
                .gap_8()
                .id("credentials-form-scroll")
                .overflow_scroll()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            Label::new(match self.mode {
                                CredentialMode::Creating => "Add New Credential",
                                CredentialMode::Editing(_) => "Edit Credential",
                            })
                            .font_weight(FontWeight::BOLD)
                            .text_lg(),
                        )
                        .child(
                            div()
                                .p_4()
                                // Removed extra border and background as per user request
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_4()
                                        .when_some(self.error_message.clone(), |div, msg| {
                                            div.child(Label::new(msg).text_color(cx.theme().danger))
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    Label::new("Name")
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM),
                                                )
                                                .child(
                                                    Input::new(&self.name_input).id("name-input"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    Label::new("Provider")
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM),
                                                )
                                                .child(
                                                    Select::new(&self.provider_select)
                                                        .id("provider-select")
                                                        .placeholder("Select Provider")
                                                        .search_placeholder("Search provider..."),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    Label::new("API Key")
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM),
                                                )
                                                .child(
                                                    Input::new(&self.api_key_input)
                                                        .id("api-key-input"),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    Label::new("Expiration Date")
                                                        .text_sm()
                                                        .font_weight(FontWeight::MEDIUM),
                                                )
                                                .child(
                                                    DatePicker::new(&self.date_picker)
                                                        .cleanable(true),
                                                ),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .children(if let CredentialMode::Editing(id) = self.mode {
                                    Some(
                                        Button::new("delete-btn")
                                            .label("Delete")
                                            .danger()
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.delete_credential(id, window, cx);
                                            })),
                                    )
                                } else {
                                    None
                                })
                                .child(
                                    Button::new("save-btn")
                                        .label("Save Credential")
                                        .primary()
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.add_credential(window, cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .bg(cx.theme().background)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_6()
                        .child(
                            div()
                                .p_6()
                                .rounded_full()
                                .bg(cx.theme().secondary)
                                .child(
                                    Icon::new(IconName::Asterisk)
                                        .text_color(cx.theme().muted_foreground),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_2()
                                .child(
                                    Label::new("Select a Credential")
                                        .text_xl()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(cx.theme().foreground),
                                )
                                .child(
                                    Label::new("Choose a credential from the sidebar to edit\nor create a new one to get started.")
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .text_align(TextAlign::Center),
                                ),
                        )
                        .child(
                            Button::new("create-first-credential")
                                .label("Create New Credential")
                                .icon(IconName::Plus)
                                .with_variant(ButtonVariant::Primary)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.mode = CredentialMode::Creating;
                                    this.show_form = true;
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element()
        };

        let container = div().flex().flex_1().relative();

        let content_area = if is_small_screen {
            container.child(main_content).child(sidebar)
        } else {
            container.child(sidebar).child(main_content)
        };

        div()
            .absolute()
            .inset_0()
            .bg(gpui_kit::black().opacity(0.5))
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Middle, |_, _, cx| {
                cx.stop_propagation();
            })
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("toggle_sidebar")
                                    .icon(IconName::Menu)
                                    .ghost()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar_open = !this.sidebar_open;
                                        cx.notify();
                                    })),
                            )
                            .child(Icon::new(IconName::SquareTerminal).with_size(Size::Small))
                            .child(
                                Label::new("Credentials")
                                    .text_lg()
                                    .font_weight(FontWeight::BOLD),
                            ),
                    )
                    .child(Button::new("close").icon(NativeIcon::Close).ghost().on_click(
                        cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |state, cx| {
                                state.toggle_credentials_modal(cx);
                            });
                        }),
                    )),
            )
            .child(content_area)
    }
}
