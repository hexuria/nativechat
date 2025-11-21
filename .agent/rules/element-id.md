---
trigger: model_decision
description: Use this if you need to manage state of the app
---

### GPUI ElementId Overview

In GPUI, **`ElementId`** is a unique identifier assigned to each element in the component tree. It serves several key purposes:

---

#### 1. **Event Binding**

* Elements with an `id` can receive events such as `on_click` or `on_mouse_move`.
* Internally, GPUI uses the `id` to map the element to its **stateful representation** (`Stateful<E>`).

Example:

```rust
div().id("my-element").child("Hello, World!")
```

Here `"my-element"` allows GPUI to track events and state for this `div`.

---

#### 2. **State Management**

* GPUI uses **`GlobalElementId`** internally, derived from the element’s `id` and its parent hierarchy.
* This enables **keyed state management**, using functions like `window.use_keyed_state`.

---

#### 3. **Uniqueness Rules**

* Each `id` must be unique **within its parent layout scope**.
* Sibling elements share a common parent, so their `id`s should not conflict.

Example with nested lists:

```rust
div().id("app").child(
    div().id("list1").child(vec![
        div().id(1).child("Item 1"),
        div().id(2).child("Item 2"),
        div().id(3).child("Item 3"),
    ])
).child(
    div().id("list2").child(vec![
        div().id(1).child("Item 1"),
    ])
)
```

* **Global IDs generated internally:**

  * `Item 1` in `list1`: `["app", "list1", 1]`
  * `Item 1` in `list2`: `["app", "list2", 1]`

> This hierarchy allows reusing simple `id`s safely under different parents.

---

#### 4. **Best Practices**

* Keep `id`s unique within their parent scope.
* Use simple numeric or string IDs for children, especially in lists.
* Avoid duplicate IDs to prevent conflicts in event handling or state management.

---

In essence, `ElementId` is GPUI’s mechanism to **track, manage state, and handle events** reliably across complex UI trees.
