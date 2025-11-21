---
trigger: model_decision
description: only use when we need to show tab
---

### Tabs Component Overview

**Import:**

```rust
use gpui_component::tab::{Tab, TabBar};
```

---

### Basic Tabs

```rust
TabBar::new("tabs")
    .selected_index(0)
    .on_click(|selected_index, _, _| {
        println!("Tab {} selected", selected_index);
    })
    .child(Tab::new().label("Account"))
    .child(Tab::new().label("Profile"))
    .child(Tab::new().label("Settings"));
```

---

### Tab Variants

* **Default:** `TabBar::new(...).child(Tab::new().label("Tab"))`
* **Underline:** `.underline()`
* **Pill:** `.pill()`
* **Outline:** `.outline()`
* **Segmented:** `.segmented()`

**Segmented Example:**

```rust
TabBar::new("segmented-tabs")
    .segmented()
    .child(IconName::Bot)
    .child(IconName::Calendar)
    .children(vec!["Settings", "About"]);
```

---

### Sizes

```rust
TabBar::new("tabs").xsmall(); // Extra small
TabBar::new("tabs").small();  // Small
TabBar::new("tabs");          // Medium (default)
TabBar::new("tabs").large();  // Large
```

---

### Tabs with Icons

```rust
TabBar::new("icon-tabs")
    .child(Tab::default().icon(IconName::User))
    .child(Tab::default().icon(IconName::Settings))
    .child(Tab::default().icon(IconName::Mail));
```

---

### Prefix/Suffix Controls

```rust
TabBar::new("tabs-with-controls")
    .prefix(h_flex().gap_1()
        .child(Button::new("back").ghost().xsmall().icon(IconName::ArrowLeft))
        .child(Button::new("forward").ghost().xsmall().icon(IconName::ArrowRight))
    )
    .suffix(h_flex().gap_1()
        .child(Button::new("inbox").ghost().xsmall().icon(IconName::Inbox))
        .child(Button::new("more").ghost().xsmall().icon(IconName::Ellipsis))
    )
    .child(Tab::new().label("Account"))
    .child(Tab::new().label("Profile"))
    .child(Tab::new().label("Settings"));
```

---

### Disabled Tabs

```rust
TabBar::new("tabs-with-disabled")
    .child(Tab::new().label("Account"))
    .child(Tab::new().label("Profile").disabled(true))
    .child(Tab::new().label("Settings"));
```

---

### Dynamic Tabs

```rust
struct TabsView {
    active_tab: usize,
    tabs: Vec<String>,
}

impl Render for TabsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        TabBar::new("dynamic-tabs")
            .selected_index(self.active_tab)
            .on_click(cx.listener(|view, index, _, cx| {
                view.active_tab = *index;
                cx.notify();
            }))
            .children(self.tabs.iter().map(|tab_name| Tab::new().label(tab_name.clone())));
    }
}
```

---

### Tabs with Menu

```rust
TabBar::new("tabs-with-menu")
    .menu(true)
    .selected_index(0)
    .child(Tab::new().label("Account"))
    .child(Tab::new().label("Profile"))
    .child(Tab::new().label("Documents"))
    .child(Tab::new().label("Mail"))
    .child(Tab::new().label("Settings"));
```

---

### Scrollable Tabs

```rust
TabBar::new("scrollable-tabs")
    .track_scroll(&self.scroll_handle)
    .child(Tab::new().label("Very Long Tab Name 1"))
    .child(Tab::new().label("Very Long Tab Name 2"));
```

---

### Individual Tab Config

```rust
Tab::new().label("Custom Tab")
    .id("custom-id")
    .prefix(IconName::Star)
    .suffix(IconName::X)
    .on_click(|_, _, _| println!("Custom tab clicked"));
```

---

### Tab Variants Enum

```rust
pub enum TabVariant {
    Tab,       // Default
    Outline,   // Rounded outline
    Pill,      // Pill-shaped
    Segmented, // Segmented control
    Underline, // Underline indicator
}
```

---

### Advanced Example: Custom Tab Content

```rust
Tab::empty()
    .child(
        h_flex().items_center().gap_2()
            .child(IconName::Folder)
            .child("Documents")
            .child(
                div().px_1().py_0p5().text_xs()
                    .bg(cx.theme().accent)
                    .text_color(cx.theme().accent_foreground)
                    .rounded_sm()
                    .child("12")
            )
    );
```

---

### Tabs with State Management

```rust
struct TabsWithContent {
    active_tab: usize,
    tab_contents: Vec<String>,
}

impl TabsWithContent {
    fn render_tab_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.active_tab {
            0 => div().child("Account content"),
            1 => div().child("Profile content"),
            2 => div().child("Settings content"),
            _ => div().child("Unknown content"),
        }
    }
}

impl Render for TabsWithContent {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .child(
                TabBar::new("content-tabs")
                    .selected_index(self.active_tab)
                    .on_click(cx.listener(|view, index, _, cx| {
                        view.active_tab = *index;
                        cx.notify();
                    }))
                    .child(Tab::new().label("Account"))
                    .child(Tab::new().label("Profile"))
                    .child(Tab::new().label("Settings"))
            )
            .child(div().flex_1().p_4().child(self.render_tab_content(cx)));
    }
}
```

---

### Closeable Tabs

```rust
Tab::new().label(tab_name.clone())
    .suffix(
        Button::new(format!("close-{}", index))
            .icon(IconName::X)
            .ghost()
            .xsmall()
            .on_click(cx.listener(move |view, _, _, cx| {
                view.close_tab(index, cx);
            }))
    );
```

---

### Notes

* `TabBar` manages active selection.
* Tab variants and sizes are inherited by children.
* `with_menu(true)` enables a dropdown for many tabs.
* Scrolling handles overflow automatically.
* Closeable tabs use suffix elements with custom click handlers.
