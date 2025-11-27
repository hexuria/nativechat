use crate::services::database::Credential;
use crate::state::AppState;
use gpui::prelude::*;
use gpui::{InteractiveElement, *};
use ui::Icon;
use ui::IconName;
use ui::IndexPath;
use ui::SearchableVec;
use ui::button::{Button, ButtonVariants};
use ui::input::{Input, InputState};
use ui::label::Label;
use ui::list::{List, ListDelegate, ListItem, ListState};
use ui::select::{Select, SelectState};
use ui::theme::ActiveTheme;

actions!(credentials_modal, [SubmitCredential]);

const PROVIDERS: &[&str] = &["Gemini", "OpenAI", "Anthropic", "Ollama", "Groq"];

pub struct CredentialsModal {
    state: Entity<AppState>,
    credentials: Vec<Credential>,
    list_state: Entity<ListState<CredentialsListDelegate>>,
    name_input: Entity<InputState>,

    provider_select: Entity<SelectState<SearchableVec<String>>>,
    api_key_input: Entity<InputState>,
    should_clear_inputs: bool,
    error_message: Option<String>,
}

#[derive(Clone)]
pub struct CredentialsListDelegate {
    view: WeakEntity<CredentialsModal>,
}

impl CredentialsListDelegate {
    fn new(view: WeakEntity<CredentialsModal>) -> Self {
        Self { view }
    }
}

impl ListDelegate for CredentialsListDelegate {
    type Item = ui::list::ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.view
            .upgrade()
            .map(|view| view.read(_cx).credentials.len())
            .unwrap_or(0)
    }

    fn render_item(&self, ix: IndexPath, _window: &mut Window, cx: &mut App) -> Option<Self::Item> {
        let entity = self.view.upgrade()?;
        let modal = entity.read(cx);
        let credential = modal.credentials.get(ix.row)?;
        let id = credential.id;
        let view_weak = self.view.clone();

        Some(
            ListItem::new(ix).child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .w_full()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                Label::new(credential.name.clone()).font_weight(FontWeight::BOLD),
                            )
                            .child(
                                Label::new(format!(
                                    "{} • Created: {}",
                                    credential.provider, credential.created_at
                                ))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                            ),
                    )
                    .child(
                        Button::new("delete")
                            .icon(IconName::Delete)
                            .ghost()
                            .on_click(move |_, window, cx| {
                                if let Some(view) = view_weak.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.delete_credential(id, window, cx)
                                    });
                                }
                            }),
                    ),
            ),
        )
    }

    fn set_selected_index(
        &mut self,
        _ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) {
    }
}

impl CredentialsModal {
    pub fn new(state: Entity<AppState>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let name_input =
                cx.new(|cx| InputState::new(window, cx).placeholder("Name (e.g. My Gemini Key)"));

            let provider_items =
                SearchableVec::new(PROVIDERS.iter().map(|s| s.to_string()).collect::<Vec<_>>());
            let provider_select = cx.new(|cx| {
                SelectState::new(provider_items, Some(IndexPath::default()), window, cx)
                    .searchable(true)
            });
            let api_key_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("API Key")
                    .masked(true)
            });

            let delegate = CredentialsListDelegate::new(cx.entity().downgrade());
            let list_state = cx.new(|cx| ListState::new(delegate, window, cx));

            let mut this = Self {
                state,
                credentials: Vec::new(),
                list_state,
                name_input,
                provider_select,
                api_key_input,
                should_clear_inputs: false,
                error_message: None,
            };
            this.fetch_credentials(cx);
            this
        })
    }

    fn fetch_credentials(&mut self, cx: &mut Context<Self>) {
        let state = self.state.read(cx);

        if let Some(db) = &state.database_service {
            let db = db.clone();
            cx.spawn(
                move |view: WeakEntity<CredentialsModal>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        match db.get_credentials().await {
                            Ok(creds) => {
                                view.update(&mut cx, |this, cx| {
                                    this.credentials = creds;
                                    this.list_state.update(cx, |_list, cx| {
                                        // list.reset_delegate(cx);
                                        cx.notify();
                                    });
                                    cx.notify();
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to fetch credentials: {}", e),
                        }
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
            "Attempting to add credential: name='{}', provider='{}', api_key='{}'",
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

        if let Some(db) = &state.database_service {
            let db = db.clone();
            cx.spawn(
                move |view: WeakEntity<CredentialsModal>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        match db.create_credential(&name, &provider, &api_key).await {
                            Ok(_) => {
                                view.update(&mut cx, |this, cx| {
                                    this.should_clear_inputs = true;
                                    // Keep provider
                                    this.fetch_credentials(cx);
                                    cx.notify();
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to create credential: {}", e),
                        }
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
            cx.spawn(
                move |view: WeakEntity<CredentialsModal>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        match db.delete_credential(id).await {
                            Ok(_) => {
                                view.update(&mut cx, |this, cx| {
                                    this.fetch_credentials(cx);
                                })
                                .ok();
                            }
                            Err(e) => eprintln!("Failed to delete credential: {}", e),
                        }
                    }
                },
            )
            .detach();
        }
    }
}

impl Render for CredentialsModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.should_clear_inputs {
            self.should_clear_inputs = false;
            self.name_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.api_key_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.provider_select.update(cx, |select, cx| {
                select.set_selected_index(Some(ui::IndexPath::default()), window, cx);
            });
        }

        let is_mobile = window.viewport_size().width < px(768.);

        div()
            .id("credentials_modal")
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_4()
            .when(is_mobile, |this| this.size_full())
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        Label::new("API Credentials")
                            .font_weight(FontWeight::BOLD)
                            .text_xl(),
                    )
                    .child(
                        gpui::div()
                            .id("close-credentials-modal")
                            .cursor_pointer()
                            .child(Icon::new(IconName::Close))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |state, cx| {
                                    state.toggle_credentials_modal(cx);
                                });
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Label::new("Add New Credential"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Input::new(&self.name_input).id("name-input"))
                            .child(
                                Select::new(&self.provider_select)
                                    .id("provider-select")
                                    .placeholder("Select Provider")
                                    .search_placeholder("Search provider..."),
                            )
                            .child(Input::new(&self.api_key_input).id("api-key-input"))
                            .child(if let Some(error) = &self.error_message {
                                div().child(
                                    Label::new(error.clone())
                                        .text_color(cx.theme().danger_foreground),
                                )
                            } else {
                                div()
                            })
                            .child(
                                Button::new("add-credential-btn")
                                    .label("Add Credential")
                                    .w_full()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_credential(window, cx);
                                    })),
                            )
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                if event.keystroke.key == "Enter" {
                                    this.add_credential(window, cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_1()
                    .min_h_0()
                    .child(Label::new("Existing Credentials"))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .child(div().size_full().child(List::new(&self.list_state))),
                    ),
            )
            .on_action(cx.listener(|this, _: &SubmitCredential, window, cx| {
                this.add_credential(window, cx);
            }))
    }
}
