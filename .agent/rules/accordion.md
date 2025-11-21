---
trigger: model_decision
description: only use when we need to use an accordion
---

### Accordion Component Overview

**Import:**

```rust
use gpui_component::accordion::Accordion;
```

---

### Basic Accordion

```rust
Accordion::new("my-accordion")
    .item(|item| item.title("Section 1").child("Content for section 1"))
    .item(|item| item.title("Section 2").child("Content for section 2"))
    .item(|item| item.title("Section 3").child("Content for section 3"))
```

---

### Allow Multiple Open Items

```rust
Accordion::new("my-accordion")
    .multiple(true) // allow multiple items open
    .item(|item| item.title("Section 1").child("Content 1"))
    .item(|item| item.title("Section 2").child("Content 2"))
```

---

### Bordered Accordion

```rust
Accordion::new("my-accordion")
    .bordered(true)
    .item(|item| item.title("Section 1").child("Content 1"))
```

---

### Different Sizes

```rust
use gpui_component::{Sizable as _, Size};

Accordion::new("my-accordion")
    .small()
    .item(|item| item.title("Small Section").child("Content"))

Accordion::new("my-accordion")
    .large()
    .item(|item| item.title("Large Section").child("Content"))
```

---

### Handle Toggle Events

```rust
Accordion::new("my-accordion")
    .on_toggle_click(|open_indices, window, cx| {
        println!("Open items: {:?}", open_indices);
    })
    .item(|item| item.title("Section 1").child("Content 1"))
```

---

### Disabled State

```rust
Accordion::new("my-accordion")
    .disabled(true)
    .item(|item| item.title("Disabled Section").child("Content"))
```

---

### Examples

**Custom Icons**

```rust
Accordion::new("my-accordion")
    .item(|item| {
        item.title(
            h_flex()
                .gap_2()
                .child(Icon::new(IconName::Settings))
                .child("Settings")
        )
        .child("Settings content here")
    })
```

**Nested Accordions**

```rust
Accordion::new("outer")
    .item(|item| {
        item.title("Parent Section")
            .content(
                Accordion::new("inner")
                    .item(|item| item.title("Child 1").child("Content"))
                    .item(|item| item.title("Child 2").child("Content"))
            )
    })
```

---

### Key Features

* **Single or multiple open items** (`multiple(true)`).
* **Customizable size** (`small()`, `medium()`, `large()`, `xsmall()`).
* **Borders** with `bordered(true)`.
* **Custom toggle handling** via `on_toggle_click`.
* **Disabled state** per accordion.
* Supports **nested accordions** and **custom icons**.
