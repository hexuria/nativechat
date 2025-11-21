---
trigger: model_decision
description: Use this only when we need to show a dialog , and an action needs the user confirmation
---

# **Dialog**

## **Imports**

```rust
use gpui_component::dialog::DialogButtonProps;
use gpui_component::WindowExt;
```

---

# **Root Setup (Don’t Skip This)**

Your root view **must** render the dialog layer.

```rust
impl Render for MyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layer = Root::render_dialog_layer(window, cx);

        div()
            .size_full()
            .child(self.view.clone())
            .children(layer) // ← overlays dialogs above everything
    }
}
```

You only do this **once**.

---

# **Basic Dialog**

```rust
window.open_dialog(cx, |dialog, _, _| {
    dialog.title("Welcome").child("This is a dialog.")
});
```

---

# **Form Dialog**

```rust
let input = cx.new(|cx| InputState::new(window, cx));

window.open_dialog(cx, |dialog, _, _| {
    dialog
        .title("User Information")
        .child(
            v_flex()
                .gap_3()
                .child("Please enter your details:")
                .child(Input::new(&input))
        )
        .footer(|_, _, _, _| {
            vec![
                Button::new("ok")
                    .primary()
                    .label("Submit")
                    .on_click(|_, window, cx| window.close_dialog(cx)),
                Button::new("cancel")
                    .label("Cancel")
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            ]
        })
});
```

---

# **Confirm Dialog**

```rust
window.open_dialog(cx, |dialog, _, _| {
    dialog
        .confirm()
        .child("Are you sure?")
        .on_ok(|_, window, cx| {
            window.push_notification("OK", cx);
            true
        })
        .on_cancel(|_, window, cx| {
            window.push_notification("Cancelled", cx);
            true
        })
});
```

---

# **Alert Dialog**

```rust
window.open_dialog(cx, |dialog, _, cx| {
    dialog
        .alert()
        .child("Done!")
        .on_close(|_, window, cx| {
            window.push_notification("Closed", cx);
        })
});
```

---

# **Custom Button Labels**

```rust
window.open_dialog(cx, |dialog, _, _| {
    dialog
        .confirm()
        .child("Restart now?")
        .button_props(
            DialogButtonProps::default()
                .cancel_text("Later")
                .ok_text("Restart Now")
                .ok_variant(ButtonVariant::Danger)
        )
        .on_ok(|_, window, cx| {
            window.push_notification("Restarting…", cx);
            true
        })
});
```

---

# **Dialog with Icon**

```rust
window.open_dialog(cx, |dialog, _, cx| {
    dialog
        .confirm()
        .child(
            h_flex()
                .gap_3()
                .child(
                    Icon::new(IconName::TriangleAlert)
                        .size_6()
                        .text_color(cx.theme().warning)
                )
                .child("This action cannot be undone.")
        )
});
```

---

# **Scrollable Dialog**

```rust
window.open_dialog(cx, |dialog, window, cx| {
    dialog
        .h(px(450.))
        .title("Long Content")
        .child(TextView::markdown(
            "content",
            long_markdown_text,
            window,
            cx
        ))
});
```

---

# **Dialog Options**

```rust
window.open_dialog(cx, |dialog, _, _| {
    dialog
        .title("Options")
        .overlay(true)
        .overlay_closable(true)
        .keyboard(true)
        .close_button(false)
        .child("Content")
});
```

---

# **Nested Dialogs**

```rust
window.open_dialog(cx, |dialog, _, _| {
    dialog
        .title("First")
        .child("This is the first dialog")
        .footer(|_, _, _, _| {
            vec![
                Button::new("open-another")
                    .label("Open Another")
                    .on_click(|_, window, cx| {
                        window.open_dialog(cx, |dialog, _, _| {
                            dialog.title("Second").child("Nested")
                        });
                    })
            ]
        })
});
```

---

# **Custom Styling**

```rust
window.open_dialog(cx, |dialog, _, cx| {
    dialog
        .rounded_lg()
        .bg(cx.theme().cyan)
        .text_color(cx.theme().info_foreground)
        .title("Styled")
        .child("Custom dialog")
});
```

---

# **Custom Padding**

```rust
window.open_dialog(cx, |dialog, _, _| {
    dialog
        .p_3()
        .title("Padding")
        .child("Custom spacing")
});
```

---

# **Programmatically Close Dialog**

```rust
window.close_dialog(cx);
```

From a button:

```rust
Button::new("submit")
    .primary()
    .label("Submit")
    .on_click(|_, window, cx| {
        // do logic
        window.close_dialog(cx);
    })
```

---

# **Examples**

## **Delete Confirmation**

```rust
Button::new("delete")
    .danger()
    .label("Delete")
    .on_click(|_, window, cx| {
        window.open_dialog(cx, |dialog, _, _| {
            dialog
                .confirm()
                .child("Delete this item?")
                .on_ok(|_, window, cx| {
                    window.push_notification("Deleted", cx);
                    true
                })
        });
    });
```

## **Success Alert**

```rust
window.open_dialog(cx, |dialog, _, _| {
    dialog
        .confirm()
        .alert()
        .child("Saved successfully!")
});
```

---

# **Opinionated Recommendations**

### **Use confirm dialogs for anything destructive**

Always `.confirm()` when deleting, resetting, logging out, etc.

### **Use alert dialogs only for “inform and dismiss”**

No choices, no complexity.

### **Never over-style dialogs**

The built-in style is already good. Only style when there’s a clear purpose.

### **Use custom buttons to elevate UX**

Changing “Cancel / OK” to real-world actions boosts clarity.

### **Don’t nest dialogs unless absolutely required**

Use sparingly — stack depth becomes UX noise.

