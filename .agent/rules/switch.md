---
trigger: model_decision
description: only use when we need to toggle something on and off
---

Switch

Import

```rust
use gpui_component::switch::Switch;
```

Basic

```rust
Switch::new("my-switch")
    .checked(false)
    .on_click(|checked, _, _| println!("Switch is now: {}", checked));
```

Controlled

```rust
struct MyView { is_enabled: bool }

impl Render for MyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Switch::new("switch")
            .checked(self.is_enabled)
            .on_click(cx.listener(|view, checked, _, cx| {
                view.is_enabled = *checked;
                cx.notify();
            }))
    }
}
```

With label

```rust
Switch::new("notifications")
    .label("Enable notifications")
    .checked(true)
    .on_click(|checked, _, _| println!("Notifications: {}", if *checked { "enabled" } else { "disabled" }));
```

Sizes

```rust
Switch::new("small-switch").small().label("Small switch");
Switch::new("medium-switch").label("Medium switch"); // default
Switch::new("custom-switch").with_size(Size::Small).label("Custom size");
```

Disabled

```rust
Switch::new("disabled-off").label("Disabled (off)").disabled(true).checked(false);
Switch::new("disabled-on").label("Disabled (on)").disabled(true).checked(true);
```

Tooltip

```rust
Switch::new("switch")
    .label("Airplane mode")
    .tooltip("Enable airplane mode to disable all wireless connections")
    .checked(false);
```

API

| Method             | Description           |
| ------------------ | --------------------- |
| `new(id)`          | Create a switch       |
| `checked(bool)`    | Set on/off state      |
| `label(text)`      | Set label             |
| `label_side(side)` | Left or right         |
| `disabled(bool)`   | Disable switch        |
| `tooltip(text)`    | Tooltip text          |
| `on_click(fn)`     | Callback with `&bool` |

Styling

* `small()` / `medium()` / `with_size(size)`
* `disabled(bool)`
* `w(width)` / `h(height)` and standard styling methods

Examples

Settings panel

```rust
Switch::new("marketing").checked(self.marketing_emails).on_click(cx.listener(|view, checked, _, cx| { view.marketing_emails = *checked; cx.notify(); }));
Switch::new("push").checked(self.push_notifications).on_click(cx.listener(|view, checked, _, cx| { view.push_notifications = *checked; cx.notify(); }));
```

Compact list

```rust
Switch::new("wifi").label("Wi-Fi").label_side(Side::Left).checked(true).small();
Switch::new("bluetooth").label("Bluetooth").label_side(Side::Left).checked(false).small();
Switch::new("airplane").label("Airplane Mode").label_side(Side::Left).checked(false).disabled(true).small();
```

Form integration

```rust
Switch::new("newsletter").label("Subscribe to newsletter").checked(self.subscribe_newsletter).tooltip("Receive updates").on_click(cx.listener(|view, checked, _, cx| { view.subscribe_newsletter = *checked; cx.notify(); }));
Switch::new("notifications").label("Enable notifications").checked(self.enable_notifications).on_click(cx.listener(|view, checked, _, cx| { view.enable_notifications = *checked; cx.notify(); }));
Switch::new("remember").label("Remember me").checked(self.remember_me).small().on_click(cx.listener(|view, checked, _, cx| { view.remember_me = *checked; cx.notify(); }));
```

Custom styling

```rust
Switch::new("custom").label("Custom styled switch").w(px(200.)).checked(true).on_click(|checked, _, _| println!("Custom switch: {}", checked));
```

Animation

* Toggle: 150ms
* Background color transition
* Smooth toggle indicator movement
* Disabled switch has no animation
