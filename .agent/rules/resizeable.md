---
trigger: model_decision
description: only use when we need to resize a container
---

Resizable Component

Import

```rust
use gpui_component::resizable::{
    h_resizable, v_resizable, resizable_panel,
    ResizablePanelGroup, ResizablePanel, ResizableState, ResizablePanelEvent
};
```

Horizontal Layout

```rust
h_resizable("my-layout")
    .on_resize(|state, window, cx| {
        let state = state.read(cx);
        let sizes = state.sizes();
    })
    .child(resizable_panel().size(px(200.)).child("Left Panel"))
    .child(div().child("Right Panel").into_any_element());
```

Vertical Layout

```rust
v_resizable("vertical-layout")
    .child(resizable_panel().size(px(100.)).child("Top Panel"))
    .child(div().child("Bottom Panel").into_any_element());
```

Panel Size Constraints

```rust
resizable_panel()
    .size(px(200.))
    .size_range(px(150.)..px(400.))
    .child("Constrained Panel");
```

Multiple Panels

```rust
h_resizable("multi-panel", state)
    .child(resizable_panel().size(px(200.)).size_range(px(150.)..px(300.)).child("Left Panel"))
    .child(resizable_panel().child("Center Panel"))
    .child(resizable_panel().size(px(250.)).child("Right Panel"));
```

Nested Layouts

```rust
v_resizable("main-layout", window, cx)
    .child(resizable_panel()
        .size(px(300.))
        .child(h_resizable("nested-layout", window, cx)
            .child(resizable_panel().size(px(200.)).child("Top Left"))
            .child(resizable_panel().child("Top Right"))
        )
    )
    .child(resizable_panel().child("Bottom Panel"));
```

Nested Panel Groups

```rust
h_resizable("outer", window, cx)
    .child(resizable_panel().size(px(200.)).child("Left Panel"))
    .group(v_resizable("inner", window, cx)
        .child(resizable_panel().size(px(150.)).child("Top Right"))
        .child(resizable_panel().child("Bottom Right")));
```

Conditional Panel Visibility

```rust
resizable_panel()
    .visible(self.show_sidebar)
    .size(px(250.))
    .child("Sidebar Content");
```

Panel with Size Limits

```rust
resizable_panel().size_range(px(100.)..Pixels::MAX).child("Flexible Panel");
resizable_panel().size_range(px(200.)..px(500.)).child("Constrained Panel");
resizable_panel().size(px(300.)).size_range(px(300.)..px(300.)).child("Fixed Panel");
```

Examples

**File Explorer**

```rust
h_resizable("file-explorer", window, cx)
    .child(resizable_panel()
        .visible(self.show_sidebar)
        .size(px(250.))
        .size_range(px(200.)..px(400.))
        .child(v_flex().p_4().child("📁 Folders").child("• Documents").child("• Pictures").child("• Downloads"))
    )
    .child(v_flex().p_4().child("📄 Files").child("file1.txt").child("file2.pdf").child("image.png").into_any_element());
```

**IDE Layout**

```rust
h_resizable("ide-main", self.main_state.clone())
    .child(resizable_panel().size(px(300.)).size_range(px(200.)..px(500.))
        .child(v_resizable("sidebar", self.sidebar_state.clone())
            .child(resizable_panel().size(px(200.)).child("File Explorer"))
            .child(resizable_panel().child("Outline"))
        )
    )
    .child(resizable_panel()
        .child(v_resizable("editor-area", self.bottom_state.clone())
            .child(resizable_panel().child("Code Editor"))
            .child(resizable_panel().size(px(150.)).size_range(px(100.)..px(300.)).child("Terminal / Output"))
        )
    );
```

**Dashboard with Widgets**

```rust
v_resizable("dashboard", self.layout_state.clone())
    .child(resizable_panel().size(px(120.)).child("Header / Navigation"))
    .child(resizable_panel()
        .child(h_resizable("widgets", self.widget_state.clone())
            .child(resizable_panel().size(px(300.)).child("Chart Widget"))
            .child(resizable_panel().child("Data Table"))
            .child(resizable_panel().size(px(250.)).child("Stats Panel"))
        )
    )
    .child(resizable_panel().size(px(60.)).child("Footer"));
```

**Settings Panel**

```rust
h_resizable("settings", self.settings_state.clone())
    .child(resizable_panel().size(px(200.)).size_range(px(150.)..px(300.))
        .child(v_flex().gap_2().p_4().child("Categories").child("• General").child("• Appearance").child("• Advanced"))
    )
    .child(resizable_panel().child(div().p_6().child("Settings Content Area")));
```

Best Practices

1. Use separate `ResizableState` for independent layouts.
2. Set reasonable min/max sizes for panels.
3. Subscribe to `ResizablePanelEvent` for layout persistence.
4. Use `.group()` for clean nested structures.
5. Avoid excessive nesting for performance.
6. Provide adequate handle padding for UX.
