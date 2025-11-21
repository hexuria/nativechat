---
trigger: model_decision
description: only use when we need to render very long list
---

### VirtualList Component Overview

**Import:**

```rust
use gpui_component::{
    v_virtual_list, h_virtual_list, VirtualListScrollHandle,
    scroll::{Scrollbar, ScrollbarState, ScrollbarAxis},
};
use std::rc::Rc;
use gpui::{px, size, ScrollStrategy, Size, Pixels};
```

---

### Basic Vertical Virtual List

```rust
v_virtual_list(
    cx.entity().clone(),
    "my-list",
    item_sizes.clone(),
    |view, visible_range, _, cx| {
        visible_range
            .map(|ix| div().h(px(30.)).w_full().bg(cx.theme().secondary).child(format!("Item {}", ix)))
            .collect()
    },
).track_scroll(&scroll_handle)
```

---

### Horizontal Virtual List

```rust
h_virtual_list(
    cx.entity().clone(),
    "horizontal-list",
    item_sizes.clone(),
    |view, visible_range, _, cx| {
        visible_range
            .map(|ix| div().w(px(120.)).h_full().bg(cx.theme().accent).child(format!("Card {}", ix)))
            .collect()
    },
).track_scroll(&scroll_handle)
```

---

### Variable Item Sizes

```rust
let item_sizes = Rc::new(
    (0..1000).map(|i| {
        let height = if i % 5 == 0 { px(60.) } else if i % 3 == 0 { px(45.) } else { px(30.) };
        size(px(300.), height)
    }).collect::<Vec<_>>()
);

v_virtual_list(
    cx.entity().clone(),
    "variable-list",
    item_sizes.clone(),
    |view, visible_range, _, cx| {
        visible_range
            .map(|ix| {
                let content = if ix % 5 == 0 { format!("Header {}", ix / 5) } else { format!("Item {}", ix) };
                let bg_color = if ix % 5 == 0 { cx.theme().accent } else { cx.theme().secondary };

                div().w_full().h(item_sizes[ix].height).bg(bg_color).flex().items_center().px_4().child(content)
            })
            .collect()
    },
)
```

---

### Table-like Layout

```rust
v_virtual_list(
    cx.entity().clone(),
    "table-list",
    item_sizes.clone(),
    |view, visible_range, _, cx| {
        visible_range
            .map(|row_ix| h_flex().w_full().h(px(40.)).border_b_1().border_color(cx.theme().border)
                .children((0..5).map(|col_ix| div().flex_1().h_full().px_3().flex().items_center().child(format!("R{}C{}", row_ix, col_ix)))))
            .collect()
    },
)
```

---

### Scroll Handling

```rust
v_virtual_list(/* ... */).track_scroll(&scroll_handle);
Scrollbar::both(&scroll_state, &scroll_handle).axis(ScrollbarAxis::Vertical);
```

**Programmatic Scrolling**

```rust
scroll_handle.scroll_to_item(index, ScrollStrategy::Top);
scroll_handle.scroll_to_item(index, ScrollStrategy::Center);
scroll_handle.scroll_to_bottom();
scroll_handle.set_offset(offset);
```

---

### Both Axis Scrolling

```rust
v_virtual_list(
    cx.entity().clone(),
    "both-axis",
    item_sizes.clone(),
    |view, visible_range, _, cx| {
        visible_range
            .map(|ix| h_flex().gap_2().children((0..20).map(|col| div().min_w(px(100.)).h(px(30.)).bg(cx.theme().secondary).child(format!("R{}C{}", ix, col))))))
            .collect()
    },
)
.track_scroll(&scroll_handle)
.child(Scrollbar::both(&scroll_state, &scroll_handle).axis(ScrollbarAxis::Both))
```

---

### Performance Optimization

* **Efficient Rendering**: Only visible items are rendered.
* **Memory Management**: Reuse elements, calculate precise visible ranges.
* **Variable Heights**: Cache calculated item sizes to avoid repeated computation.

---

### Example: File Explorer

```rust
v_virtual_list(
    cx.entity().clone(),
    "file-list",
    item_sizes.clone(),
    |view, visible_range, _, cx| {
        visible_range.map(|ix| {
            let file = &view.files[ix];
            let is_selected = view.selected_index == Some(ix);

            div().w_full().h(view.item_sizes[ix].height).px_3().py_1().flex().items_center().gap_2()
                .bg(if is_selected { cx.theme().accent } else { Color::transparent() })
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .child(file_icon(&file.file_type))
                .child(file.name.clone())
                .child(div().flex_1().text_right().text_xs().text_color(cx.theme().muted_foreground).child(format_file_size(file.size)))
                .on_click(cx.listener(move |view, _, _, cx| { view.selected_index = Some(ix); cx.notify(); }))
        }).collect()
    },
)
.track_scroll(&scroll_handle)
```

---

### Example: Chat Window with Auto-scroll

```rust
v_virtual_list(
    cx.entity().clone(),
    "chat-messages",
    self.item_sizes.clone(),
    |view, visible_range, _, cx| {
        visible_range.map(|ix| {
            let msg = &view.messages[ix];
            div().w_full().px_4().py_2()
                .child(v_flex().gap_1()
                    .child(h_flex().justify_between()
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(msg.author.clone()))
                        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format_timestamp(msg.timestamp))))
                    .child(div().text_sm().child(msg.content.clone()))
                )
        }).collect()
    },
)
.track_scroll(&scroll_handle)
```

---

### Example: Data Grid with Fixed Headers

```rust
v_flex()
    .size_full()
    .child(
        h_flex().w_full().h(px(40.)).bg(cx.theme().secondary).border_b_1().border_color(cx.theme().border)
            .children(self.headers.iter().zip(&self.column_widths).map(|(header, &width)| {
                div().w(width).h_full().px_3().flex().items_center().font_weight(FontWeight::SEMIBOLD).child(header.clone())
            }))
    )
    .child(
        v_virtual_list(
            cx.entity().clone(),
            "data-rows",
            Rc::new(vec![size(px(800.), px(32.)); self.data.len()]),
            |view, visible_range, _, cx| {
                visible_range.map(|row_ix| {
                    h_flex().w_full().h(px(32.)).border_b_1().border_color(cx.theme().border.opacity(0.5))
                        .children(view.data[row_ix].iter().zip(&view.column_widths).map(|(cell, &width)| {
                            div().w(width).h_full().px_3().flex().items_center().child(cell.clone())
                        }))
                }).collect()
            },
        ).track_scroll(&scroll_handle).flex_1()
    )
```

---

### Best Practices

1. Pre-calculate item sizes for performance.
2. Use VirtualList for lists >50 items.
3. Avoid heavy computation inside render functions.
4. Keep item state separate from rendering logic.
5. Handle empty lists and edge cases gracefully.
6. Test with various data sizes and scroll positions.

**Performance Tips:**

* Calculate sizes upfront.
* Minimize re-renders with stable keys.
* Batch multiple data updates.
* Keep render functions lightweight.
* Monitor memory usage with large datasets.
