---
trigger: model_decision
description: only use when we need to select something on a form
---

### Select Component Overview

**Import:**

```rust
use gpui_component::select::{
    Select, SelectState, SelectItem, SelectDelegate,
    SelectEvent, SearchableVec, SelectGroup
};
```

---

### Basic Select

```rust
let state = cx.new(|cx| {
    SelectState::new(
        vec!["Apple", "Orange", "Banana"],
        Some(IndexPath::default()), // select first item
        window,
        cx
    )
});

Select::new(&state);
```

**With placeholder:**

```rust
Select::new(&state).placeholder("Select a language...");
```

---

### Searchable Dropdown

```rust
let fruits = SearchableVec::new(vec!["Apple", "Orange", "Banana", "Grape", "Pineapple"]);

let state = cx.new(|cx| {
    SelectState::new(fruits, None, window, cx).searchable(true)
});

Select::new(&state).icon(IconName::Search);
```

---

### Custom Item Type

```rust
#[derive(Debug, Clone)]
struct Country { name: SharedString, code: SharedString }

impl SelectItem for Country {
    type Value = SharedString;

    fn title(&self) -> SharedString { self.name.clone() }
    fn display_title(&self) -> Option<gpui::AnyElement> {
        Some(format!("{} ({})", self.name, self.code).into_any_element())
    }
    fn value(&self) -> &Self::Value { &self.code }
    fn matches(&self, query: &str) -> bool {
        self.name.to_lowercase().contains(&query.to_lowercase()) ||
        self.code.to_lowercase().contains(&query.to_lowercase())
    }
}
```

---

### Grouped Items

```rust
let mut grouped_items = SearchableVec::new();

grouped_items.push(
    SelectGroup::new("A")
        .items(vec![
            Country { name: "Australia".into(), code: "AU".into() },
            Country { name: "Austria".into(), code: "AT".into() },
        ])
);

grouped_items.push(
    SelectGroup::new("B")
        .items(vec![
            Country { name: "Brazil".into(), code: "BR".into() },
            Country { name: "Belgium".into(), code: "BE".into() },
        ])
);

let state = cx.new(|cx| SelectState::new(grouped_items, None, window, cx));
Select::new(&state);
```

---

### Sizes & States

```rust
Select::new(&state).large();
Select::new(&state); // medium default
Select::new(&state).small();
Select::new(&state).disabled(true);
Select::new(&state).cleanable(true); // show clear button
```

---

### Custom Appearance

```rust
Select::new(&state)
    .w(px(320.))
    .menu_width(px(400.))
    .appearance(false)
    .title_prefix("Country: ");
```

**Empty State:**

```rust
Select::new(&state)
    .empty(h_flex()
        .h_24()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child("No options available")
    );
```

---

### Events

```rust
cx.subscribe_in(&state, window, |_, _, event, _, _| {
    if let SelectEvent::Confirm(value) = event {
        println!("Selected: {:?}", value);
    }
});
```

---

### Mutating State

```rust
// Set by index
state.update(cx, |state, cx| {
    state.set_selected_index(Some(IndexPath::default().row(2)), window, cx);
});

// Set by value
state.update(cx, |state, cx| {
    state.set_selected_value(&"US".into(), window, cx);
});

// Get current selection
let current_value = state.read(cx).selected_value();

// Update items
state.update(cx, |state, cx| {
    state.set_items(vec!["New Option 1".into(), "New Option 2".into()], window, cx);
});
```

---

### Examples

**Language Selector**

```rust
let languages = SearchableVec::new(vec!["Rust".into(), "TypeScript".into(), "Go".into()]);
let state = cx.new(|cx| SelectState::new(languages, None, window, cx));
Select::new(&state).placeholder("Select language...").title_prefix("Language: ");
```

**Country Selector with Flag**

```rust
#[derive(Debug, Clone)]
struct Region { name: SharedString, code: SharedString, flag: SharedString }

impl SelectItem for Region {
    type Value = SharedString;
    fn title(&self) -> SharedString { self.name.clone() }
    fn display_title(&self) -> Option<gpui::AnyElement> {
        Some(h_flex().items_center().gap_2()
            .child(self.flag.clone())
            .child(format!("{} ({})", self.name, self.code))
            .into_any_element()
        )
    }
    fn value(&self) -> &Self::Value { &self.code }
}

let regions = vec![
    Region { name: "United States".into(), code: "US".into(), flag: "🇺🇸".into() },
    Region { name: "Canada".into(), code: "CA".into(), flag: "🇨🇦".into() },
];

let state = cx.new(|cx| SelectState::new(regions, None, window, cx));
Select::new(&state).placeholder("Select country...");
```

**Integrated with Input**

```rust
h_flex()
    .border_1().border_color(cx.theme().input).rounded_lg().w_full().gap_1()
    .child(div().w(px(140.)).child(Select::new(&country_state).appearance(false).py_2().pl_3()))
    .child(Divider::vertical())
    .child(div().flex_1().child(Input::new(&phone_input).appearance(false).placeholder("Phone number").pr_3().py_2()));
```

---

### Keyboard Shortcuts

| Key     | Action                   |
| ------- | ------------------------ |
| Tab     | Focus dropdown           |
| Enter   | Open menu or select item |
| Up/Down | Navigate options         |
| Escape  | Close menu               |
| Space   | Open menu                |

**Theming:** respects theme tokens like `background`, `input`, `foreground`, `muted_foreground`, `accent`, `accent_foreground`, `border`, and `radius`.
