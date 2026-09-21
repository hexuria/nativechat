//! Settings → Logins: the two panes of a passwords app. The list at the left is searched,
//! grouped by kind, and picked from; the pane at the right is the picked row: who, where,
//! the person's notes, and where the password is.
//!
//! The password itself is never read here. The pane shows dots and offers no copy; a saved
//! login is used from a login card, after Touch ID (see [`crate::site_login`]).

mod add_sheet;
mod detail;
mod list;

use std::sync::Arc;

use crate::site_login::grouped_logins;
use crate::state::AppState;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::{ActiveTheme, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

pub use add_sheet::AddSheetInputs;

pub struct LoginsPage {
    state: Entity<AppState>,
    search: Entity<InputState>,
    notes: Entity<TextareaState>,
    /// Whose notes the textarea holds. It is refilled when the pick moves.
    notes_for: Option<String>,
    /// The textarea holds words the row does not, yet.
    notes_dirty: bool,
    /// The Add sheet's fields, made the first time the sheet opens (an input needs a window).
    add: Option<AddSheetInputs>,
}

impl LoginsPage {
    pub fn new(window: &mut Window, state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let notes = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Notes")
                .auto_grow(4, 12)
        });
        cx.observe(&state, |_this, _, cx| cx.notify()).detach();
        // A row with an authenticator code shows the current digits and the seconds they
        // have left; while one is on the pane the page redraws once a second so the
        // countdown is honest. With no code shown there is nothing to count, and the timer
        // is not started (this page outlives a visit to it).
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                let ticking = this.update(cx, |page: &mut Self, cx| {
                    if page.counting_down(cx) {
                        cx.notify();
                    }
                });
                if ticking.is_err() {
                    break;
                }
            }
        })
        .detach();
        // What is typed in the field is what the list filters by; the state keeps the copy
        // so the driver can write it too.
        cx.subscribe(&search, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().to_string();
                this.state
                    .update(cx, |state, cx| state.set_site_login_query(query, cx));
            }
        })
        .detach();
        cx.subscribe(&notes, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                this.notes_dirty = true;
                cx.notify();
            }
            InputEvent::Blur => this.save_notes(cx),
            _ => {}
        })
        .detach();
        Self {
            state,
            search,
            notes,
            notes_for: None,
            notes_dirty: false,
            add: None,
        }
    }

    /// The textarea's words go to the row, once, when they differ from what the row has.
    fn save_notes(&mut self, cx: &mut Context<Self>) {
        if !self.notes_dirty {
            return;
        }
        self.notes_dirty = false;
        let Some(id) = self.notes_for.clone() else {
            return;
        };
        let text = self.notes.read(cx).value().to_string();
        self.state
            .update(cx, |state, cx| state.update_site_login_notes(id, text, cx));
    }

    /// The fields follow the state: a search the driver wrote lands in the field, and the
    /// notes textarea is refilled when the pick moves or the row's notes change under it —
    /// never while the person is mid-edit.
    /// Whether anything on the page is counting down: the picked row has a live code.
    fn counting_down(&self, cx: &App) -> bool {
        let state = self.state.read(cx);
        state.app_settings_tab == crate::state::AppSettingsTab::Logins
            && state
                .site_login_selected
                .as_ref()
                .is_some_and(|id| state.site_login_codes.contains_key(id))
    }

    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (query, selected, row_notes, add_open) = {
            let state = self.state.read(cx);
            let selected = state.site_login_selected.clone();
            let row_notes = selected
                .as_ref()
                .and_then(|id| state.site_logins.iter().find(|row| &row.id == id))
                .map(|row| row.notes.clone());
            (
                state.site_login_query.clone(),
                selected,
                row_notes,
                state.site_login_add_open,
            )
        };
        if self.search.read(cx).value().as_ref() != query.as_str() {
            self.search
                .update(cx, |input, cx| input.set_value(query, window, cx));
        }
        if self.notes_for != selected {
            // Words left on the previous row go to it before the textarea moves on.
            self.save_notes(cx);
            self.notes_for = selected;
            self.notes_dirty = false;
            let text = row_notes.unwrap_or_default();
            self.notes
                .update(cx, |input, cx| input.set_value(text, window, cx));
        } else if !self.notes_dirty
            && let Some(row_notes) = row_notes
            && self.notes.read(cx).value().as_ref() != row_notes.as_str()
        {
            self.notes
                .update(cx, |input, cx| input.set_value(row_notes, window, cx));
        }
        if add_open && self.add.is_none() {
            self.add = Some(AddSheetInputs::new(window, cx));
        }
    }
}

impl Render for LoginsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(window, cx);
        let theme = cx.theme().clone();
        let view = cx.entity();
        let app = self.state.clone();
        let (rows, query, selected, on_this_mac, icons, notice, error, add_open) = {
            let state = self.state.read(cx);
            (
                state.site_logins.clone(),
                state.site_login_query.clone(),
                state.site_login_selected.clone(),
                state.site_logins_on_this_mac.clone(),
                state.site_login_icons.clone(),
                state.site_login_notice.clone(),
                state.site_login_error.clone(),
                state.site_login_add_open,
            )
        };
        let with_code: std::collections::HashSet<String> = self
            .state
            .read(cx)
            .site_login_codes
            .keys()
            .cloned()
            .collect();
        let groups = grouped_logins(&rows, &query, &with_code);
        // The pick stays on the pane even when a search hides its row.
        let picked = selected
            .as_ref()
            .and_then(|id| rows.iter().find(|row| &row.id == id))
            .cloned();
        let picked_here = picked
            .as_ref()
            .is_some_and(|row| on_this_mac.contains(&row.id));
        let picked_code = picked.as_ref().and_then(|row| {
            let totp = self.state.read(cx).site_login_codes.get(&row.id)?;
            crate::site_login::totp::current(totp).ok()
        });
        let picked_icon = picked
            .as_ref()
            .and_then(|row| icons.get(&row.origin))
            .and_then(|icon| icon.clone());
        let sheet = add_open.then(|| self.add.clone()).flatten();

        div()
            .id("settings-logins")
            .flex()
            .flex_row()
            .size_full()
            .relative()
            .child(list::render(
                &self.search,
                &groups,
                selected.as_deref(),
                &icons,
                notice,
                // While the Add sheet is up it shows the error itself; the list does not
                // repeat it behind the dimmed page.
                error.clone().filter(|_| !add_open),
                rows.is_empty(),
                &theme,
                app.clone(),
            ))
            .child(detail::render(
                picked.as_ref(),
                picked_here,
                picked_code,
                picked_icon.as_ref(),
                &self.notes,
                self.notes_dirty,
                view,
                app.clone(),
                &theme,
            ))
            .when_some(sheet, |this, inputs| {
                this.child(add_sheet::render(inputs, error, app, &theme))
            })
    }
}

/// The site's icon, or the grey tile with its first letter that stands in for one.
pub(crate) fn site_icon(
    origin: &str,
    image: Option<&Arc<Image>>,
    size: f32,
    theme: &Theme,
) -> AnyElement {
    let radius = px(size * 0.22);
    match image {
        Some(image) => img(image.clone())
            .size(px(size))
            .flex_shrink_0()
            .rounded(radius)
            .object_fit(ObjectFit::Cover)
            .into_any_element(),
        None => div()
            .size(px(size))
            .flex_shrink_0()
            .rounded(radius)
            .bg(theme.muted)
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.muted_foreground)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(size * 0.5))
            .child(first_letter(origin))
            .into_any_element(),
    }
}

fn first_letter(origin: &str) -> String {
    origin
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}
