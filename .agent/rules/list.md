---
trigger: model_decision
description: show only when we need to render list of items
---

### List Component Overview

**Imports:**

```rust
use gpui_component::list::{List, ListState, ListDelegate, ListItem, ListEvent, ListSeparatorItem};
use gpui_component::IndexPath;
```

---

### Basic List

Implement `ListDelegate` for your data:

```rust
struct MyListDelegate {
    items: Vec<String>,
    selected_index: Option<IndexPath>,
}

impl ListDelegate for MyListDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.items.len()
    }

    fn render_item(&self, ix: IndexPath, _window: &mut Window, _cx: &mut App) -> Option<Self::Item> {
        self.items.get(ix.row).map(|item| {
            ListItem::new(ix)
                .child(Label::new(item.clone()))
                .selected(Some(ix) == self.selected_index)
        })
    }

    fn set_selected_index(&mut self, ix: Option<IndexPath>, _window: &mut Window, cx: &mut Context<ListState<Self>>) {
        self.selected_index = ix;
        cx.notify();
    }
}

let state = cx.new(|cx| ListState::new(delegate, window, cx));
div().child(List::new(&state));
```

---

### List with Sections

```rust
fn sections_count(&self, _cx: &App) -> usize { 3 }

fn items_count(&self, section: usize, _cx: &App) -> usize {
    match section { 0 => 5, 1 => 3, 2 => 7, _ => 0 }
}

fn render_section_header(&self, section: usize, _window: &mut Window, cx: &mut App) -> Option<impl IntoElement> {
    let title = match section { 0 => "Section 1", 1 => "Section 2", 2 => "Section 3", _ => return None };
    Some(h_flex().px_2().py_1().gap_2().text_sm().text_color(cx.theme().muted_foreground)
        .child(Icon::new(IconName::Folder)).child(title))
}

fn render_section_footer(&self, section: usize, _window: &mut Window, cx: &mut App) -> Option<impl IntoElement> {
    Some(div().px_2().py_1().text_xs().text_color(cx.theme().muted_foreground)
        .child(format!("End of section {}", section + 1)))
}
```

---

### List Items with Icons and Actions

```rust
ListItem::new(ix)
    .child(h_flex().items_center().gap_2()
        .child(Icon::new(IconName::File))
        .child(Label::new(item.title.clone()))
    )
    .suffix(|_, _| Button::new("action").ghost().small().icon(IconName::MoreHorizontal))
    .selected(Some(ix) == self.selected_index)
    .on_click(cx.listener(move |this, _, window, cx| {
        this.delegate_mut().select_item(ix, window, cx);
    }));
```

---

### List with Search

```rust
impl ListDelegate for MyListDelegate {
    fn perform_search(&mut self, query: &str, _window: &mut Window, _cx: &mut Context<ListState<Self>>) -> Task<()> {
        self.filtered_items = self.all_items.iter()
            .filter(|item| item.to_lowercase().contains(&query.to_lowercase()))
            .cloned()
            .collect();
        Task::ready(())
    }
}

let state = cx.new(|cx| ListState::new(delegate, window, cx).searchable(true));
List::new(&state);
```

---

### Loading State

```rust
fn loading(&self, _cx: &App) -> bool { self.is_loading }

fn render_loading(&self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    v_flex().justify_center().items_center().py_4()
        .child(Skeleton::new().h_4().w_full())
        .child(Skeleton::new().h_4().w_3_4())
}
```

---

### Infinite Scrolling

```rust
fn is_eof(&self, _cx: &App) -> bool { !self.has_more_data }
fn load_more_threshold(&self) -> usize { 20 }

fn load_more(&mut self, window: &mut Window, cx: &mut Context<ListState<Self>>) {
    if self.is_loading { return; }
    self.is_loading = true;

    cx.spawn_in(window, async move |view, window| {
        Timer::after(Duration::from_secs(1)).await;
        view.update_in(window, |view, _, cx| {
            view.delegate_mut().load_more_items();
            view.delegate_mut().is_loading = false;
            cx.notify();
        });
    }).detach();
}
```

---

### List Events

```rust
cx.subscribe(&state, |_, _, event: &ListEvent, _| {
    match event {
        ListEvent::Select(ix) => println!("Item selected at: {:?}", ix),
        ListEvent::Confirm(ix) => println!("Item confirmed at: {:?}", ix),
        ListEvent::Cancel => println!("Selection cancelled"),
    }
});
```

---

### Custom Empty State

```rust
fn render_empty(&self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    v_flex().size_full().justify_center().items_center().gap_2()
        .child(Icon::new(IconName::Search).size_16().text_color(cx.theme().muted_foreground))
        .child(Label::new("No items found").text_color(cx.theme().muted_foreground))
        .child(Label::new("Try adjusting your search terms").text_sm().text_color(cx.theme().muted_foreground.opacity(0.7)))
}
```

---

### List Configuration

```rust
List::new(&state)
    .max_h(px(400.))
    .scrollbar_visible(false)
    .paddings(Edges::all(px(8)));
```

**Scrolling Control:**

```rust
state.update(cx, |state, cx| {
    state.scroll_to_item(IndexPath::new(0).section(1), ScrollStrategy::Center, window, cx);
    state.scroll_to_selected_item(window, cx);
    state.set_selected_index(Some(IndexPath::new(5)), window, cx);
});
```

---

### Performance

* Virtualized rendering for large datasets
* Efficient updates with change detection
* Memory cleanup for off-screen items
* Smooth, hardware-accelerated scrolling
* Lazy loading and infinite scrolling support
