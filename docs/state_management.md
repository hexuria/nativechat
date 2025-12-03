# Chat Session Management Architecture

This document outlines the architecture for managing chat sessions in the sidebar, specifically focusing on the **Edit** and **Delete** workflows. It explains the design patterns used to handle interactions between `RenderOnce` components and the main application state.

## Core Concepts

### 1. Component Structure

*   **`SidebarView` (`src/components/sidebar.rs`)**: The parent `View` component. It holds the state (which session is being edited/deleted) and handles the actual logic (updating the DB, refreshing the UI).
*   **`ChatSessionItem` (`src/components/sidebar_chat_item.rs`)**: A `RenderOnce` component. It is purely presentational and re-created on every frame. It renders the session title, buttons, or the edit input field.

### 2. The Context Problem

In GPUI, `RenderOnce` components render in the `App` context, while `View` components live in a `View` context.
*   **Problem**: If a `RenderOnce` component dispatches an action via `cx.dispatch_action()`, it starts from the `App` root. If the `View` (Sidebar) is not focused, it might not receive the action.
*   **Solution**: We use a **Callback Pattern**. The parent (`SidebarView`) passes closures to the child (`ChatSessionItem`). When an event occurs (click, keypress), the child calls the closure. The closure, defined in the parent's scope, has access to the parent's `ViewEntity` and can call its methods directly.

---

## Workflow: Editing a Session

### 1. Triggering Edit Mode
1.  **User Action**: User clicks the "Edit" (pencil) icon on a chat item.
2.  **Child Component**: `ChatSessionItem`'s `on_click` handler invokes the `on_edit` callback.
3.  **Parent Component**: The callback in `SidebarView` calls `view.update(...)` to invoke `self.start_editing(...)`.
4.  **State Update**: `start_editing` sets `self.editing_session_id` to the session ID and creates a new `Input` component.
5.  **Re-render**: The UI updates. `SidebarView` passes `is_editing=true` to the specific `ChatSessionItem`.

### 2. The Edit Interface
When `is_editing` is true, `ChatSessionItem` renders:
*   An `Input` component (wrapped in `div`) instead of the title label.
*   "Check" (Submit) and "X" (Cancel) buttons.

### 3. Handling Input
*   **Typing**: Handled internally by the `Input` component.
*   **Enter Key**: Caught by a subscription in `SidebarView::start_editing`. Triggers `submit_rename`.
*   **Escape Key**: Caught by `on_key_down` in `ChatSessionItem`. Triggers `on_cancel_edit` callback.

### 4. Submitting Changes
1.  **User Action**: User presses Enter or clicks the "Check" button.
2.  **Action**: `SubmitRenameSession` action is dispatched (or method called directly).
3.  **Handler**: `SidebarView::submit_rename` is executed.
    *   Reads the new title from the `Input`.
    *   Updates the database via `AppState`.
    *   Clears `editing_session_id`.
    *   Refreshes the list.

---

## Workflow: Deleting a Session

### 1. Triggering Confirmation
1.  **User Action**: User clicks the "Delete" (trash) icon.
2.  **Callback**: `on_delete` callback is invoked.
3.  **State Update**: `SidebarView` sets `self.delete_confirmation_id`.
4.  **Re-render**: `ChatSessionItem` renders the "Delete this chat?" confirmation UI.

### 2. Confirming Delete
1.  **User Action**: User clicks the red "Delete" button.
2.  **Callback**: `on_confirm_delete` is invoked.
3.  **Handler**: `SidebarView::confirm_delete` is executed.
    *   Calls `AppState::delete_conversation`.
    *   Clears `delete_confirmation_id`.
    *   Refreshes the list.

---

## Code Reference

### Defining Callbacks (`sidebar_chat_item.rs`)

We use `Rc<dyn Fn(...)>` to allow cloning closures, which is required for event handlers.

```rust
pub struct ChatSessionItem {
    // ...
    on_edit: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    // ...
}

impl ChatSessionItem {
    pub fn on_edit(mut self, callback: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_edit = Some(Rc::new(callback));
        self
    }
}
```

### Wiring Callbacks (`sidebar.rs`)

We capture the `view_entity` to ensure we can call methods on the specific `SidebarView` instance.

```rust
let view_entity = cx.entity().clone();

ChatSessionItem::new(...)
    .on_edit({
        let id = id.clone();
        let view = view_entity.clone();
        move |window, cx| {
            // Update the view directly
            view.update(cx, |this, cx| {
                this.start_editing(&StartRenameSession { id: id.clone(), ... }, window, cx);
            });
        }
    })
```

## Best Practices for Future Features

1.  **Avoid Action Dispatch from RenderOnce**: If a component is `RenderOnce` (like list items), avoid `cx.dispatch_action`. It's brittle.
2.  **Use Callbacks**: Pass `Rc<dyn Fn>` callbacks for user interactions.
3.  **Capture View Entity**: In the parent `render` method, capture `cx.entity().clone()` and use `view.update(cx, ...)` inside callbacks to safely modify state.
4.  **Centralize Logic**: Keep the business logic (DB calls, state mutations) in the `View` (e.g., `SidebarView`), not in the item components.
