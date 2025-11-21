---
trigger: model_decision
description: only use when we need to override titlebar on a window
---

TitleBar Component

Import:

```rust
use gpui_component::TitleBar;
```

### Basic Usage

```rust
TitleBar::new()
    .child(div().child("My Application"));
```

### With Custom Content

```rust
TitleBar::new()
    .child(
        div()
            .flex()
            .items_center()
            .gap_3()
            .child("App Name")
            .child(Badge::new().count(5))
    )
    .child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(Button::new("settings").icon(IconName::Settings))
            .child(Button::new("profile").icon(IconName::User))
    );
```

### With Menu Bar

```rust
TitleBar::new()
    .child(div().flex().items_center().child(AppMenuBar::new(window, cx)))
    .child(
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap_2()
            .child(Button::new("github").icon(IconName::GitHub))
            .child(Button::new("notifications").icon(IconName::Bell))
    );
```

### Linux Custom Close Handler

```rust
TitleBar::new()
    .on_close_window(|_, window, cx| {
        window.push_notification("Saving before close...", cx);
        window.remove_window();
    })
    .child(div().child("Custom Close Behavior"));
```

### Styled Title Bar

```rust
TitleBar::new()
    .bg(cx.theme().primary)
    .border_color(cx.theme().primary_border)
    .child(
        div()
            .text_color(cx.theme().primary_foreground)
            .child("Styled Title Bar")
    );
```

### Window Options Integration

```rust
use gpui::{WindowOptions, TitlebarOptions};

WindowOptions {
    titlebar: Some(TitleBar::title_bar_options()),
    ..Default::default()
}
```

### Notes

* Automatically adapts to macOS, Windows, and Linux.
* Platform differences:

  * **macOS:** native traffic lights, transparent default, proper padding.
  * **Windows:** custom control buttons, fixed width, hover/active states.
  * **Linux:** manual close handling, window dragging, context menu on right-click.
* Use `.child()` to add elements to the bar, `.on_close_window()` for Linux-specific close actions, and `.bg()`, `.border_color()` etc., for styling.
