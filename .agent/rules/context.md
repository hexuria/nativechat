---
trigger: model_decision
description: use this when you need to manage state on the app
---

### GPUI Context Overview

In GPUI, four core objects form the backbone of UI management:

---

#### 1. **Window**

* Represents the current window instance.
* Handles **window-level operations** like rendering, events, and layout for that particular window.

---

#### 2. **App**

* Represents the running application.
* Handles **application-level operations**, such as global state, resource management, and top-level lifecycle.

---

#### 3. **Context (`cx`)**

* Entity-specific context, usually passed as `&mut Context<Self>` in rendering functions.
* Handles **context-level operations**, such as managing child entities, subscriptions, and state updates.
* Standard convention: name it `cx` for readability and consistency.

---

#### 4. **Entity**

* Represents a UI component or data entity.
* Handles **entity-level operations**, like its state, events, and rendering.

---

### Typical Usage

```rust
fn new(window: &mut Window, cx: &mut App) {}

impl RenderOnce for MyElement {
    fn render(self, window: &mut Window, cx: &mut App) {}
}

impl Render for MyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) {}
}
```

* `cx` is used consistently for `App` or `Context<Self>` references.
* This convention improves code readability and maintainability.

---

In short:

| Object           | Scope       | Responsibility                                  |
| ---------------- | ----------- | ----------------------------------------------- |
| `Window`         | Window      | Window-level rendering and events               |
| `App`            | Application | Global app state and lifecycle                  |
| `Context` (`cx`) | Context     | Entity management, subscriptions, state updates |
| `Entity`         | Entity      | Component-level state and events                |

---

This hierarchy ensures GPUI cleanly separates concerns between global, window, context, and entity levels.
