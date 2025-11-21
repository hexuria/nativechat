---
trigger: model_decision
description: only use when we need to toggle something on or off
---

### Toggle Component Overview

**Import:**

```rust
use gpui_component::button::{Toggle, ToggleGroup};
```

---

### Basic Toggle

```rust
Toggle::new("toggle1")
    .label("Toggle me")
    .checked(false)
    .on_click(|checked, _, _| {
        println!("Toggle is now: {}", checked);
    });
```

---

### Icon Toggle

```rust
Toggle::new("toggle2")
    .icon(IconName::Eye)
    .checked(true)
    .on_click(|checked, _, _| {
        println!("Visibility: {}", if *checked { "shown" } else { "hidden" });
    });
```

---

### Controlled Toggle

```rust
struct MyView {
    is_active: bool,
}

impl Render for MyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Toggle::new("active")
            .label("Active")
            .checked(self.is_active)
            .on_click(cx.listener(|view, checked, _, cx| {
                view.is_active = *checked;
                cx.notify();
            }))
    }
}
```

---

### Toggle Variants

```rust
// Ghost toggle (default)
Toggle::new("ghost-toggle").ghost().label("Ghost");

// Outline toggle
Toggle::new("outline-toggle").outline().label("Outline");
```

---

### Sizes

```rust
Toggle::new("xs-toggle").xsmall();
Toggle::new("small-toggle").small();
Toggle::new("medium-toggle"); // default
Toggle::new("large-toggle").large();
```

---

### Disabled State

```rust
Toggle::new("disabled-toggle").disabled(true).checked(false);
Toggle::new("disabled-checked-toggle").disabled(true).checked(true);
```

---

### Toggle vs Switch

| Feature          | Toggle                   | Switch                   |
| ---------------- | ------------------------ | ------------------------ |
| Appearance       | Button-like              | Sliding switch           |
| Use Case         | Toolbar buttons, filters | Settings, preferences    |
| Visual Style     | Rectangular              | Rounded track with thumb |
| State Indication | Background/pressed       | Thumb position           |
| Multi-selection  | Yes (ToggleGroup)        | No                       |

---

### ToggleGroup

**Basic Toggle Group**

```rust
ToggleGroup::new("filter-group")
    .child(Toggle::new(0).icon(IconName::Bell))
    .child(Toggle::new(1).icon(IconName::Bot))
    .child(Toggle::new(2).icon(IconName::Inbox))
    .child(Toggle::new(3).label("Other"))
    .on_click(|checkeds, _, _| {
        println!("Selected toggles: {:?}", checkeds);
    });
```

**Controlled Toggle Group**

```rust
struct FilterView {
    notifications: bool,
    bots: bool,
    inbox: bool,
    other: bool,
}

impl Render for FilterView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ToggleGroup::new("filters")
            .child(Toggle::new(0).icon(IconName::Bell).checked(self.notifications))
            .child(Toggle::new(1).icon(IconName::Bot).checked(self.bots))
            .child(Toggle::new(2).icon(IconName::Inbox).checked(self.inbox))
            .child(Toggle::new(3).label("Other").checked(self.other))
            .on_click(cx.listener(|view, checkeds, _, cx| {
                view.notifications = checkeds[0];
                view.bots = checkeds[1];
                view.inbox = checkeds[2];
                view.other = checkeds[3];
                cx.notify();
            }));
    }
}
```

---

### Variants & Sizes for ToggleGroup

```rust
// Outline variant, small size
ToggleGroup::new("compact-filters")
    .outline()
    .small()
    .child(Toggle::new(0).icon(IconName::Filter))
    .child(Toggle::new(1).icon(IconName::Sort))
    .child(Toggle::new(2).icon(IconName::Search));

// Ghost variant (default), extra small
ToggleGroup::new("mini-toolbar")
    .xsmall()
    .child(Toggle::new(0).icon(IconName::Bold))
    .child(Toggle::new(1).icon(IconName::Italic))
    .child(Toggle::new(2).icon(IconName::Underline));
```

---

### Event Handling

**Individual Toggle**

```rust
Toggle::new("subscribe-toggle")
    .label("Subscribe")
    .on_click(|checked, _, _| {
        println!("{}", if *checked { "Subscribed!" } else { "Unsubscribed!" });
    });
```

---

### Examples

**Toolbar**

```rust
ToggleGroup::new("formatting")
    .small()
    .child(Toggle::new(0).icon(IconName::Bold).checked(self.bold))
    .child(Toggle::new(1).icon(IconName::Italic).checked(self.italic))
    .child(Toggle::new(2).icon(IconName::Underline).checked(self.underline))
    .child(Toggle::new(3).icon(IconName::Strikethrough).checked(self.strikethrough))
    .on_click(cx.listener(|view, states, _, cx| {
        view.bold = states[0];
        view.italic = states[1];
        view.underline = states[2];
        view.strikethrough = states[3];
        cx.notify();
    }));
```

**Filter Panel**

```rust
ToggleGroup::new("status-filters")
    .outline()
    .child(Toggle::new(0).label("Completed").checked(self.show_completed))
    .child(Toggle::new(1).label("Pending").checked(self.show_pending))
    .child(Toggle::new(2).label("Cancelled").checked(self.show_cancelled))
    .on_click(cx.listener(|view, states, _, cx| {
        view.show_completed = states[0];
        view.show_pending = states[1];
        view.show_cancelled = states[2];
        cx.notify();
    }));

Toggle::new("urgent-filter")
    .label("Show urgent only")
    .checked(self.show_urgent)
    .on_click(cx.listener(|view, checked, _, cx| {
        view.show_urgent = *checked;
        cx.notify();
    }));
```

**Settings**

```rust
Toggle::new("email-notifications")
    .icon(IconName::Mail)
    .checked(self.email_notifications)
    .on_click(cx.listener(|view, checked, _, cx| { view.email_notifications = *checked; cx.notify(); }));

Toggle::new("push-notifications")
    .icon(IconName::Bell)
    .checked(self.push_notifications)
    .on_click(cx.listener(|view, checked, _, cx| { view.push_notifications = *checked; cx.notify(); }));
```

**Multi-select Categories**

```rust
ToggleGroup::new("categories")
    .children(Self::categories().into_iter().enumerate().map(|(i, category)| {
        Toggle::new(i).label(category).checked(self.selected_categories.get(i).copied().unwrap_or(false))
    }))
    .on_click(cx.listener(|view, states, _, cx| {
        view.selected_categories = states.clone();
        cx.notify();
    }));
```

---

### Best Practices

1. Use descriptive labels.
2. Group related options with `ToggleGroup`.
3. Provide clear visual feedback for checked state.
4. Use toggles for "selection"-style options, not settings.
5. Keep toggle state consistent with application state.
6. Use ARIA labels/tooltips for icon-only toggles.
