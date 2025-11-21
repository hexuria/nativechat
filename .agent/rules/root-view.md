---
trigger: model_decision
description: use this when creating entry point of app, usually when you still getting started and boostraping your app
---

### GPUI Root View Overview

The **Root** component in GPUI acts as the **entry point for all GPUI Component features** in a window. It must be the **first-level child** of a window; otherwise, some features may behave unexpectedly. Think of it as the “foundation” for the component system.

---

#### 1. **Using Root**

* Initialize GPUI components first with `gpui_component::init(cx)`.
* Wrap your top-level view inside `Root::new(...)`.

Example:

```rust
fn main() {
    let app = Application::new();

    app.run(move |cx| {
        gpui_component::init(cx); // Initialize GPUI Components

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|_| Example);
                // Root must be the first-level child
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
```

* `Example` is your main app content.
* `Root` ensures overlays, dialogs, sheets, notifications, and other component features work correctly.

---

#### 2. **Overlay Layers**

`Root` provides built-in methods to handle **modals, drawers, and notifications**:

* `Root::render_dialog_layer(cx)` → renders opened dialogs/modals.
* `Root::render_sheet_layer(cx)` → renders open drawers/sheets.
* `Root::render_notification_layer(cx)` → renders notifications.

Example integration in your main view:

```rust
struct MyApp;

impl Render for MyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child("My App Content")
            .children(Root::render_dialog_layer(cx))
            .children(Root::render_sheet_layer(cx))
            .children(Root::render_notification_layer(cx))
    }
}
```

* `children(...)` is used because if no overlays are active, these methods return `None`—GPUI will not render anything.

---

#### 3. **Key Points**

* **Always use `Root` as the first-level child** of the window.
* **Overlays** (dialogs, sheets, notifications) are managed by Root.
* Ensures the **full GPUI Component ecosystem** works seamlessly.

In short: **Root is your foundation for GPUI components and overlay management**. Without it, features like dialogs or notifications won’t function properly.
