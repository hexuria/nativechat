---
trigger: model_decision
description: only use when we need to show a sheet
---

Sheet Component

Import

```rust
use gpui_component::WindowExt;
use gpui_component::Placement;
```

### Basic Sheet

```rust
window.open_sheet(cx, |sheet, _, _| {
    sheet.title("Navigation").child("Sheet content goes here");
});
```

### Sheet Placement

```rust
window.open_sheet_at(Placement::Left, cx, |sheet, _, _| sheet.title("Left Sheet"));
window.open_sheet_at(Placement::Right, cx, |sheet, _, _| sheet.title("Right Sheet"));
window.open_sheet_at(Placement::Top, cx, |sheet, _, _| sheet.title("Top Sheet"));
window.open_sheet_at(Placement::Bottom, cx, |sheet, _, _| sheet.title("Bottom Sheet"));
```

### Custom Size

```rust
window.open_sheet(cx, |sheet, _, _| {
    sheet.title("Wide Sheet").size(px(500.)).child("This sheet is 500px wide");
});
```

### Form Content

```rust
let input = cx.new(|cx| InputState::new(window, cx));
let date = cx.new(|cx| DatePickerState::new(window, cx));

window.open_sheet(cx, |sheet, _, _| {
    sheet.title("User Profile")
        .child(
            v_flex()
                .gap_4()
                .child("Enter your information:")
                .child(Input::new(&input).placeholder("Full Name"))
                .child(DatePicker::new(&date).placeholder("Date of Birth"))
        )
        .footer(
            h_flex()
                .gap_3()
                .child(Button::new("save").primary().label("Save"))
                .child(Button::new("cancel").label("Cancel"))
        );
});
```

### Overlay Options

```rust
// With overlay
sheet.overlay(true).overlay_closable(true);
// Without overlay
sheet.overlay(false);
```

### Resizable Sheet

```rust
sheet.resizable(true).size(px(300.)).child("Resizable content");
```

### Custom Margin

```rust
sheet.margin_top(px(32.)).child("Appears below title bar");
```

### Sheet with List

```rust
let delegate = ListDelegate::new(items);
let list = cx.new(|cx| List::new(delegate, window, cx));

window.open_sheet_at(Placement::Left, cx, |sheet, _, _| {
    sheet.title("File Explorer").size(px(400.))
        .child(div().border_1().border_color(cx.theme().border).rounded(cx.theme().radius).size_full().child(list.clone()));
});
```

### Close Event Handling

```rust
sheet.on_close(|_, window, cx| { window.push_notification("Sheet was closed", cx); });
```

### Navigation Sheet

```rust
sheet.title("Navigation").size(px(280.))
    .child(
        v_flex()
            .gap_2()
            .child(Button::new("home").ghost().label("Home").w_full())
            .child(Button::new("profile").ghost().label("Profile").w_full())
            .child(Button::new("settings").ghost().label("Settings").w_full())
            .child(Button::new("logout").ghost().label("Logout").w_full())
    );
```

### Custom Styling

```rust
sheet.bg(cx.theme().accent)
     .text_color(cx.theme().accent_foreground)
     .border_color(cx.theme().primary)
     .child("Custom styled content");
```

### Programmatic Close

```rust
Button::new("close").label("Close Sheet").on_click(|_, window, cx| window.close_sheet(cx));
window.close_sheet(cx);
```

### Best Practices

1. Left/right for navigation, top/bottom for temporary content.
2. Maintain consistent sheet sizes.
3. Provide clear descriptive titles.
4. Allow multiple ways to close (ESC, overlay, button).
5. Organize content with spacing and grouping.
6. Ensure responsive design.
7. Lazy load content for performance.
