---
trigger: always_on
---

# **GPUI Tooltip — Condensed Guide**

## **Import**

```rust
use gpui_component::tooltip::Tooltip;
```

---

# **Core Patterns**

## **1. Simple Text Tooltip**

```rust
div()
    .child("Hover me")
    .tooltip(|w, cx| Tooltip::new("Helpful text").build(w, cx))
```

## **2. Built-in Tooltip on Components**

```rust
Button::new("save")
    .label("Save")
    .tooltip("Save the current document");
```

## **3. Tooltip + Action / Keybinding**

```rust
actions!(my_actions, [SaveDocument]);

Button::new("save")
    .label("Save")
    .tooltip_with_action("Save file", &SaveDocument, Some("MyContext"));
```

## **4. Custom Element Tooltip (Rich Content)**

```rust
div()
    .child("Hover")
    .tooltip(|w, cx| {
        Tooltip::element(|_, cx| {
            h_flex()
                .gap_x_1()
                .child(IconName::Info)
                .child(div().child("Muted").text_color(cx.theme().muted_foreground))
                .child(div().child("Danger").text_color(cx.theme().danger))
        })
        .build(w, cx)
    })
```

## **5. Manual Keybinding**

```rust
div()
    .child("Custom KB")
    .tooltip(|w, cx| {
        Tooltip::new("Delete item")
            .key_binding(Some(Kbd::new("Delete")))
            .build(w, cx)
    })
```

---

# **Advanced Use**

## **Components with Native Tooltip Support**

Works the same everywhere:

```rust
Button::new("btn").tooltip("Helpful tip");
Switch::new("toggle").tooltip("Toggle notifications");
Checkbox::new("check").tooltip("Keep me logged in");
Radio::new("opt").tooltip("Enable feature");
```

## **Complex Content Tooltip**

```rust
div()
    .child("Details")
    .tooltip(|w, cx| {
        Tooltip::element(|_, cx| {
            v_flex()
                .gap_2()
                .child(h_flex().gap_1().child(IconName::User).child("User Info"))
                .child(div().child("Last login: 2h ago").text_xs().text_color(cx.theme().muted_foreground))
                .child(div().child("Status: Active").text_xs().text_color(cx.theme().success))
        })
        .build(w, cx)
    })
```

## **Tooltips in Form Inputs**

```rust
Input::new("email")
    .placeholder("Email")
    .tooltip("We’ll never share your email");
```

---

# **API Reference (Essential Only)**

### **Tooltip**

* `Tooltip::new(text)`
* `Tooltip::element(builder)`
* `tooltip(text)` — simple
* `tooltip_with_action(text, action, context)`
* `key_binding(Some(Kbd))`
* `build(window, cx)`

---

# **Styling (Minimal)**

Built-in style uses the theme. You can override:

```rust
Tooltip::new("Styled")
    .bg(cx.theme().accent)
    .text_color(cx.theme().accent_foreground)
    .build(window, cx)
```

---

# **Useful Real-World Patterns**

## **Toolbar buttons**

```rust
h_flex()
    .gap_1()
    .child(Button::new("new").icon(IconName::Plus).tooltip_with_action("New file", &NewFile, Some("Editor")))
    .child(Button::new("open").icon(IconName::FolderOpen).tooltip_with_action("Open", &OpenFile, Some("Editor")))
    .child(Button::new("save").icon(IconName::Save).tooltip_with_action("Save", &SaveFile, Some("Editor")))
```

## **Status Indicators**

```rust
div()
    .size_3()
    .rounded_full()
    .bg(cx.theme().success)
    .tooltip(|w, cx| Tooltip::new("Connected").build(w, cx));
```

## **Rich File Tooltip**

```rust
div()
    .child("document.txt")
    .tooltip(|w, cx| {
        Tooltip::element(|_, cx| {
            v_flex()
                .gap_1()
                .child(h_flex().child(IconName::File).child("document.txt"))
                .child(div().child("Size: 2.4 KB").text_xs())
                .child(div().child("Modified: 2h ago").text_xs())
        })
        .build(w, cx)
    })
```

