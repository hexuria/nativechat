---
trigger: model_decision
description: only show when we wanna hide other contents and just show the main one e.g. q and a , faq 
---

Collapsible Component

Import:

```rust
use gpui_component::collapsible::Collapsible;
```

### Basic Usage

```rust
Collapsible::new()
    .max_w_128()        // Set maximum width
    .gap_1()            // Gap between child and content
    .open(self.open)    // Control whether it's expanded
    .child(
        "Click the header to expand or collapse this section."
    )
    .content(
        "This is the full content visible only when expanded. \
        You can include text, images, or other UI elements here."
    )
    .child(
        h_flex()
            .justify_center()
            .child(
                Button::new("toggle1")
                    .icon(IconName::ChevronDown)
                    .label("Show more")
                    .when(open, |this| {
                        this.icon(IconName::ChevronUp).label("Show less")
                    })
                    .xsmall()
                    .link()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open = !this.open;
                        cx.notify();
                    }))
            )
    );
```

**Notes:**

* `.open(bool)` controls the collapsed/expanded state.
* `.child(...)` adds visible header or controls.
* `.content(...)` defines what is shown only when expanded.
* You can include buttons or other interactive elements to toggle the state dynamically.
