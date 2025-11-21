---
trigger: model_decision
description: only use this if we need to show popover
---

Popover Component

Import

```rust
use gpui_component::popover::Popover;
```

### Basic Popover

```rust
Popover::new("basic-popover")
    .trigger(Button::new("trigger").label("Click me").outline())
    .child("Hello, this is a popover!")
    .child("It appears when you click the button.");
```

### Custom Positioning

```rust
use gpui::Corner;

Popover::new("positioned-popover")
    .anchor(Corner::TopRight)
    .trigger(Button::new("top-right").label("Top Right").outline())
    .child("This popover appears at the top right");
```

### Render a View in Popover

```rust
let view = cx.new(|_| MyView::new());

Popover::new("form-popover")
    .anchor(Corner::BottomLeft)
    .trigger(Button::new("show-form").label("Open Form").outline())
    .child(view.clone());
```

### Complex Content via `content` Method

```rust
Popover::new("complex-popover")
    .anchor(Corner::BottomLeft)
    .trigger(Button::new("complex").label("Complex Content").outline())
    .content(|_, _, _| {
        div()
            .child("This popover has complex content.")
            .child(Button::new("action-btn").label("Perform Action").outline())
    });
```

### Right-Click Popover

```rust
use gpui::MouseButton;

Popover::new("context-menu")
    .anchor(Corner::BottomRight)
    .mouse_button(MouseButton::Right)
    .trigger(Button::new("right-click").label("Right Click Me").outline())
    .child("Context Menu")
    .child(Divider::horizontal())
    .child("This is a custom context menu.");
```

### Dismiss Popover Programmatically

```rust
Popover::new("dismiss-popover")
    .trigger(Button::new("dismiss").label("Dismiss Popover").outline())
    .content(|_, cx| {
        div()
            .child("Click the button below to dismiss this popover.")
            .child(
                Button::new("close-btn")
                    .label("Close Popover")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            )
    });
```

### Styling Popover

```rust
Popover::new("custom-popover")
    .appearance(false)
    .trigger(Button::new("custom").label("Custom Style"))
    .bg(cx.theme().accent)
    .text_color(cx.theme().accent_foreground)
    .p_6()
    .rounded_xl()
    .shadow_2xl()
    .child("Fully custom styled popover");
```

### Control Open State

```rust
Popover::new("controlled-popover")
    .open(self.popover_open)
    .on_open_change(cx.listener(|this, open: &bool, _, cx| {
        this.popover_open = *open;
        cx.notify();
    }))
    .trigger(Button::new("control-btn").label("Control Popover").outline())
    .child("This popover's open state is controlled programmatically.");
```

### Default Open

```rust
Popover::new("default-open-popover")
    .default_open(true)
    .trigger(Button::new("default-open-btn").label("Default Open").outline())
    .child("This popover is open by default when first rendered.");
```
