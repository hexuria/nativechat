---
trigger: model_decision
description: use this when we need to show very long list scrollable items 
---

### Scrollable Component Overview

**Imports:**

```rust
use gpui_component::{
    scroll::{Scrollable, ScrollbarState, ScrollbarAxis, ScrollbarShow},
    StyledExt as _,
};
```

---

### Basic Scrollable Container

```rust
div().size_full().child("Your content here").scrollable(Axis::Vertical);
```

---

### Scrollable with Content

```rust
v_flex().gap_2().p_4()
    .children((0..100).map(|i| {
        div().h(px(40.)).w_full().bg(cx.theme().secondary).child(format!("Item {}", i))
    }))
    .scrollable(Axis::Vertical);
```

---

### Horizontal Scrolling

```rust
h_flex().gap_2().p_4()
    .children((0..50).map(|i| {
        div().min_w(px(120.)).h(px(80.)).bg(cx.theme().accent).child(format!("Card {}", i))
    }))
    .scrollable(Axis::Horizontal);
```

---

### Both Directions

```rust
div().size_full()
    .child(
        div().w(px(2000.)).h(px(2000.)).bg(cx.theme().background).child("Large content area")
    )
    .scrollable(ScrollbarAxis::Both);
```

---

### Custom Scrollbars

```rust
Scrollbar::vertical(&scroll_state, &scroll_handle)
    .axis(ScrollbarAxis::Vertical)
    .scroll_size(size(px(1000.), px(2000.)));
```

---

### Scroll Tracking

```rust
div()
    .on_scroll_wheel(|view, event, _, cx| {
        if event.delta.y != px(0.) {
            println!("Scrolled vertically: {:?}", event.delta.y);
        }
    })
    .scrollable(Axis::Vertical);
```

Programmatic scrolling:

```rust
self.scroll_handle.set_offset(point(px(0.), px(0.))); // Top
let max_offset = self.scroll_handle.max_offset();
self.scroll_handle.set_offset(point(px(0.), max_offset.y)); // Bottom
```

---

### Virtualization with VirtualList

```rust
VirtualList::new(
    self.scroll_handle.clone(),
    self.items.len(),
    |ix, _window, _cx| size(px(300.), px(40.)), // Item size
    |ix, bounds, selected, _window, cx| {
        div().size(bounds.size)
            .bg(if selected { cx.theme().accent } else { cx.theme().background })
            .child(format!("Item {}: {}", ix, self.items[ix]))
            .into_any_element()
    },
);
```

Variable item sizes:

```rust
let height = if items[ix].len() > 50 { px(80.) } else { px(40.) };
size(px(300.), height)
```

Scroll to item:

```rust
self.scroll_handle.scroll_to_item(index, ScrollStrategy::Top);
self.scroll_handle.scroll_to_item(index, ScrollStrategy::Center);
```

---

### Theme Customization

Scrollbar appearance:

```json
{
    "scrollbar.background": "#ffffff20",
    "scrollbar.thumb.background": "#00000060",
    "scrollbar.thumb.hover.background": "#000000"
}
```

Scrollbar show modes:

```rust
theme.scrollbar_show = ScrollbarShow::Scrolling; // Only when scrolling
theme.scrollbar_show = ScrollbarShow::Hover;     // On hover
theme.scrollbar_show = ScrollbarShow::Always;    // Always visible
```

Sync with system settings:

```rust
Theme::sync_scrollbar_appearance(cx);
```

---

### Advanced Usage

**ScrollableMask for custom scroll areas:**

```rust
ScrollableMask::new(Axis::Vertical, &scroll_handle).debug();
```

**Nested scrollable areas:**

```rust
v_flex().size_full()
    .child(
        v_flex().flex_1().scrollable(Axis::Vertical)
            .child(
                h_flex().w_full().scrollable(Axis::Horizontal)
                    .child("Nested scrollable content")
            )
    );
```

**Limit scroll update frequency for performance:**

```rust
Scrollbar::vertical(&state, &handle).max_fps(60);
```

---

### Examples

**File Browser:**

```rust
v_flex().gap_1().p_2()
    .children(self.files.iter().map(|file| {
        div().h(px(32.)).w_full().px_2().flex().items_center()
            .hover(|style| style.bg(cx.theme().secondary_hover))
            .child(file.clone())
    }))
    .scrollable(Axis::Vertical);
```

**Chat with auto-scroll:**

```rust
if self.should_auto_scroll {
    let max_offset = self.scroll_handle.max_offset();
    self.scroll_handle.set_offset(point(px(0.), max_offset.y));
}
```

**Data table with virtual scrolling:**

```rust
VirtualList::new(
    self.scroll_handle.clone(),
    self.data.len(),
    |_ix, _window, _cx| size(px(800.), px(32.)),
    |ix, bounds, _selected, _window, cx| {
        h_flex().size(bounds.size).border_b_1().border_color(cx.theme().border)
            .children(self.data[ix].iter().map(|cell| div().flex_1().px_2().flex().items_center().child(cell.clone())))
            .into_any_element()
    },
);
```

---

### Performance Tips

* Use `VirtualList` for large lists (>100 items)
* Limit scroll updates with `max_fps()`
* Avoid heavy rendering inside scroll events
* Batch updates when possible
* Only render visible items

---

### Best Practices

* Keep scroll behavior consistent
* Provide clear scroll indicators
* Ensure responsiveness across devices
* Handle empty content gracefully
* Test with different content sizes and screen readers
