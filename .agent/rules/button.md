---
trigger: model_decision
description: only use when we needed a button to execute something e.g. submitting form , navigating from one page to another etc.
---

### Button Component Overview

**Import:**

```rust
use gpui_component::button::{Button, ButtonGroup};
```

---

### Basic Button

```rust
Button::new("my-button")
    .label("Click me")
    .on_click(|_, _, _| println!("Button clicked!"));
```

---

### Variants

```rust
Button::new("btn-primary").primary().label("Primary");
Button::new("btn-secondary").label("Secondary"); // default
Button::new("btn-danger").danger().label("Delete");
Button::new("btn-warning").warning().label("Warning");
Button::new("btn-success").success().label("Success");
Button::new("btn-info").info().label("Info");
Button::new("btn-ghost").ghost().label("Ghost");
Button::new("btn-link").link().label("Link");
Button::new("btn-text").text().label("Text");
```

**Outline Buttons**

```rust
Button::new("btn").primary().outline().label("Primary Outline");
Button::new("btn").danger().outline().label("Danger Outline");
```

---

### Sizes

```rust
Button::new("btn").xsmall().label("Extra Small");
Button::new("btn").small().label("Small");
Button::new("btn").label("Medium"); // default
Button::new("btn").large().label("Large");
```

**Compact:**

```rust
Button::new("btn").label("Compact").compact();
```

---

### With Icons

```rust
use gpui_component::{Icon, IconName};

// Icon before label
Button::new("btn").icon(IconName::Check).label("Confirm");

// Icon only
Button::new("btn").icon(IconName::Search);

// Custom icon
Button::new("btn").icon(Icon::new(IconName::Heart)).label("Like");
```

**Dropdown caret:**

```rust
Button::new("btn").label("Options").dropdown_caret(true);
```

---

### Button States

```rust
Button::new("btn").label("Disabled").disabled(true);
Button::new("btn").label("Loading").loading(true);
Button::new("btn").label("Selected").selected(true);
```

---

### Button Groups

**Basic:**

```rust
ButtonGroup::new("btn-group")
    .child(Button::new("btn1").label("One"))
    .child(Button::new("btn2").label("Two"))
    .child(Button::new("btn3").label("Three"));
```

**Toggle (multi-select):**

```rust
ButtonGroup::new("toggle-group")
    .multiple(true)
    .child(Button::new("btn1").label("Option 1").selected(true))
    .child(Button::new("btn2").label("Option 2"))
    .child(Button::new("btn3").label("Option 3"))
    .on_click(|selected_indices, _, _| println!("Selected: {:?}", selected_indices));
```

---

### Custom Variant

```rust
use gpui_component::button::ButtonCustomVariant;

let custom = ButtonCustomVariant::new(cx)
    .color(cx.theme().magenta)
    .foreground(cx.theme().primary_foreground)
    .border(cx.theme().magenta)
    .hover(cx.theme().magenta.opacity(0.1))
    .active(cx.theme().magenta);

Button::new("custom-btn")
    .custom(custom)
    .label("Custom Button");
```

---

### Additional Features

**Tooltip:**

```rust
Button::new("btn").label("Hover me").tooltip("This is a helpful tooltip");
```

**Custom children:**

```rust
Button::new("btn")
    .child(
        h_flex()
            .items_center()
            .gap_2()
            .child("Custom Content")
            .child(IconName::ChevronDown)
            .child(IconName::Eye)
    );
```
