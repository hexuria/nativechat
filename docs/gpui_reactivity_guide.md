# GPUI Reactivity & State Management Guide

This guide explains the core concepts of GPUI's reactivity system, focusing on how to manage state, handle updates, and work with async tasks without losing your sanity.

## 1. The Trinity: Entity, Context, and State

In GPUI, you rarely hold data directly. Instead, you hold **Entities**.

*   **`Entity<T>`**: A smart pointer (like `Rc<RefCell<T>>`) to your state `T`. It's cheap to clone and pass around.
*   **`Context` (`cx`)**: The "God Object". You need it to access the data inside an `Entity`.
*   **`State`**: Your actual struct (e.g., `ProfileSettingsModal`, `InputState`).

### Accessing Data

*   **Reading (Immutable)**:
    ```rust
    // Returns &T
    let state = entity.read(cx); 
    println!("Name: {}", state.name);
    ```

*   **Writing (Mutable)**:
    ```rust
    // Gives &mut T inside the closure
    entity.update(cx, |state, cx| {
        state.name = "New Name".to_string();
        // AUTOMATICALLY triggers 'observe' callbacks and re-renders
        cx.notify(); 
    });
    ```

> **Key Rule**: You cannot hold a reference from `read()` while calling `update()`. Rust's borrow checker will yell at you (or you'll panic at runtime).

---

## 2. Reactivity: How things update

GPUI is **pull-based** for rendering (it calls `render` when it thinks it needs to) but **push-based** for state changes.

### A. `cx.notify()`
*   **What it does**: Tells GPUI "This view is dirty, please re-render it next frame."
*   **When to use**: When you change a field on your struct that affects the UI.
    ```rust
    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify(); // Re-render me!
    }
    ```

### B. `cx.observe(&entity, callback)`
*   **What it does**: Runs the callback whenever `entity.update(...)` is called.
*   **When to use**: When your View needs to react to changes in *another* Entity (like a global store or a child component).
    ```rust
    // In ProfileSettingsModal::new
    cx.observe(&this.list_state, |this, list, cx| {
        // Runs whenever the list state changes
        let selected = list.read(cx).selected_index;
        this.load_profile(selected, cx);
    }).detach();
    ```

### C. `cx.subscribe(&entity, callback)`
*   **What it does**: Runs the callback when `entity` explicitly emits an **Event**.
*   **When to use**: For specific actions like "Form Submitted", "Item Selected", "Error Occurred".
    ```rust
    // In SelectState
    cx.emit(SelectEvent::Confirm(value));

    // In Parent
    cx.subscribe(&self.select, |this, _, event, cx| {
        match event {
            SelectEvent::Confirm(value) => this.handle_selection(value, cx),
        }
    }).detach();
    ```

---

## 3. The "Madness": Async Tasks & Closures

This is where most pain comes from. You cannot keep `&mut Context` across an `await` point.

### The Pattern

```rust
// 1. Clone what you need BEFORE the async block
let db = self.database_service.clone();
let profile_id = self.profile.id;

// 2. Spawn the task
// 'this' is a WeakEntity<Self>. It won't keep the View alive if it closes.
cx.spawn(move |this, mut cx| async move {
    // 3. Do async work (NO UI ACCESS HERE)
    let result = db.update_profile(profile_id).await;

    // 4. Hop back to the Main Thread to update UI
    // 'this.update' fails gracefully if the view is already closed.
    this.update(&mut cx, |this, cx| {
        match result {
            Ok(_) => {
                this.show_success(cx);
                cx.notify();
            }
            Err(e) => this.show_error(e, cx),
        }
    }).ok(); // .ok() ignores the error if view is closed
}).detach();
```

### Common Pitfalls

1.  **Missing `move`**: If you use variables from the outer scope inside `async move`, you usually need `move` on the `spawn` closure too: `cx.spawn(move |...| ...)`
2.  **Reading Input in Async**: You cannot read `self.input.read(cx)` *inside* the `async` block. Read the value **before** spawning.
    *   *Wrong*: `async move { let text = input.read(cx).text(); ... }`
    *   *Right*: `let text = input.read(cx).text(); cx.spawn(move |...| async move { use(text); ... })`
3.  **`&mut Window` vs `&mut Context`**:
    *   `load_profile` often takes `&mut Window` because it's called from a view event.
    *   But `cx.observe` gives you `&mut Context`.
    *   **Fix**: Use `cx.window()` (if available) or structure your methods to take `&mut Context<Self>`. Most `Input` and `Select` methods accept `&mut Context`.

---

## 4. Cheat Sheet: Input & Select

### Input (`InputState`)
*   **Read**: `input.read(cx).value()` (or `.text()`, check implementation)
*   **Write**: `input.update(cx, |i, cx| i.set_value("new text", cx))`

### Select (`SelectState`)
*   **Read**: `select.read(cx).selected_value()`
*   **Write**: `select.update(cx, |s, cx| s.set_selected_value(val, cx))`
*   **Events**: Subscribe to `SelectEvent::Confirm`.

---


## 6. The "Madness" Part 2: Common Pitfalls & Fixes

Based on real-world debugging, here are the subtle things that will break your app:

### A. The `ElementId` Trap
*   **Problem**: "I clicked the button but nothing happened."
*   **Cause**: Duplicate IDs or missing IDs. GPUI uses IDs to route events. If two elements have the same ID (e.g., "save_btn"), the event might go to the wrong one or be dropped.
*   **Fix**:
    *   Give every interactive element a unique ID: `Button::new("save_profile_btn")`.
    *   In lists, use the index or unique data ID: `div().id(("item", index))`.

### B. Input: `.text()` vs `.value()`
*   **Problem**: "I typed in the input, hit save, but the value is empty."
*   **Cause**: `InputState` has multiple methods. `.text()` might return the *masked* text or internal buffer, while `.value()` returns the actual content you want.
*   **Fix**: Always check the component's API. For `InputState`, use `.value()` to get the current string.

### C. The `Window` Argument
*   **Problem**: "My listener runs, but UI updates don't show up."
*   **Cause**: Your handler might be taking `&mut Context` but the listener provides `&mut Window`.
*   **Fix**: Pass `window` through!
    ```rust
    // Listener
    .on_click(cx.listener(|this, _, window, cx| {
        this.save_profile(window, cx)
    }))

    // Handler
    fn save_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) { ... }
    ```

### D. Async `move`
*   **Problem**: "Compiler says `db` does not live long enough."
*   **Cause**: `cx.spawn` creates a future that might outlive the current scope. It needs to *own* its data.
*   **Fix**: Use `move` on **both** closures if you're capturing variables from the outer scope.
    ```rust
    let db = self.db.clone();
    //       v-- 1. Move 'db' into the spawn closure
    cx.spawn(move |this, cx| async move {
    //                       ^-- 2. Move 'db' into the async block
        db.update().await; 
    })
    ```
