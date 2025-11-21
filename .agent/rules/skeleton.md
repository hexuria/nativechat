---
trigger: model_decision
description: only use this when we need to show a content is being loaded and we wanna represent those to exact same size or orientation while loading so it would just replace the whole item when those are resolved
---

### Skeleton Component Overview

**Import:**

```rust
use gpui_component::skeleton::Skeleton;
```

---

### Basic Skeleton

```rust
Skeleton::new();
```

---

### Shapes

**Text line:**

```rust
Skeleton::new().w(px(250.)).h_4().rounded_md();
v_flex()
    .gap_2()
    .child(Skeleton::new().w(px(250.)).h_4().rounded_md())
    .child(Skeleton::new().w(px(200.)).h_4().rounded_md())
    .child(Skeleton::new().w(px(180.)).h_4().rounded_md());
```

**Circle / Avatar:**

```rust
Skeleton::new().size_12().rounded_full();
Skeleton::new().w(px(64.)).h(px(64.)).rounded_full();
```

**Rectangle / Card:**

```rust
Skeleton::new().w(px(250.)).h(px(125.)).rounded_md();
Skeleton::new().w(px(120.)).h(px(40.)).rounded_md();
```

**Other shapes:**

```rust
Skeleton::new().size_20().rounded_md();    // Square
Skeleton::new().w_full().h(px(200.)).rounded_lg(); // Banner
Skeleton::new().size_6().rounded_md();     // Small icon
```

---

### Secondary Variant

```rust
Skeleton::new().secondary().w(px(200.)).h_4().rounded_md();
```

---

### Sizes Utilities

**Height:**

```rust
Skeleton::new().h_3(); // 12px
Skeleton::new().h_4(); // 16px
Skeleton::new().h_5(); // 20px
Skeleton::new().h_6(); // 24px
```

**Width:**

```rust
Skeleton::new().w(px(100.));
Skeleton::new().w(px(200.));
Skeleton::new().w_full();
Skeleton::new().w_1_2(); // 50%
```

**Square sizes:**

```rust
Skeleton::new().size_4();  // 16x16px
Skeleton::new().size_8();  // 32x32px
Skeleton::new().size_12(); // 48x48px
Skeleton::new().size_16(); // 64x64px
```

---

### Animation

* Continuous pulse animation (2s duration)
* Ease-in-out bounce
* Opacity animates from 100% → 50% → 100%
* Cannot be disabled (essential for loading state)

---

### Examples

**Profile Card Loading:**

```rust
v_flex()
    .gap_4()
    .p_4()
    .border_1()
    .border_color(cx.theme().border)
    .rounded_lg()
    .child(
        h_flex()
            .gap_3()
            .items_center()
            .child(Skeleton::new().size_12().rounded_full()) // Avatar
            .child(
                v_flex()
                    .gap_2()
                    .child(Skeleton::new().w(px(120.)).h_4().rounded_md()) // Name
                    .child(Skeleton::new().w(px(100.)).h_3().rounded_md()) // Email
            )
    )
    .child(
        v_flex()
            .gap_2()
            .child(Skeleton::new().w_full().h_4().rounded_md()) // Bio line 1
            .child(Skeleton::new().w(px(200.)).h_4().rounded_md()) // Bio line 2
    );
```

**Article List Loading:**

```rust
v_flex()
    .gap_6()
    .children((0..3).map(|_| {
        h_flex()
            .gap_4()
            .child(Skeleton::new().w(px(120.)).h(px(80.)).rounded_md()) // Thumbnail
            .child(
                v_flex()
                    .gap_2()
                    .flex_1()
                    .child(Skeleton::new().w_full().h_5().rounded_md()) // Title
                    .child(Skeleton::new().w(px(300.)).h_4().rounded_md()) // Line 1
                    .child(Skeleton::new().w(px(250.)).h_4().rounded_md()) // Line 2
                    .child(Skeleton::new().w(px(100.)).h_3().rounded_md()) // Date
            )
    }));
```

**Table Rows Loading:**

```rust
v_flex()
    .gap_2()
    .children((0..5).map(|_| {
        h_flex()
            .gap_4()
            .p_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(Skeleton::new().size_8().rounded_full()) // Status
            .child(Skeleton::new().w(px(150.)).h_4().rounded_md()) // Name
            .child(Skeleton::new().w(px(200.)).h_4().rounded_md()) // Email
            .child(Skeleton::new().w(px(80.)).h_4().rounded_md())  // Role
            .child(Skeleton::new().w(px(60.)).h_4().rounded_md())  // Actions
    }));
```

**Buttons / Form Fields Loading:**

```rust
h_flex()
    .gap_3()
    .child(Skeleton::new().w(px(80.)).h(px(36.)).rounded_md()) // Primary
    .child(Skeleton::new().w(px(70.)).h(px(36.)).rounded_md()) // Secondary
    .child(Skeleton::new().size_9().rounded_md());              // Icon button

v_flex()
    .gap_4()
    .child(
        v_flex()
            .gap_1()
            .child(Skeleton::new().w(px(60.)).h_4().rounded_md()) // Label
            .child(Skeleton::new().w_full().h(px(40.)).rounded_md()) // Input
    )
    .child(
        v_flex()
            .gap_1()
            .child(Skeleton::new().w(px(80.)).h_4().rounded_md()) // Label
            .child(Skeleton::new().w_full().h(px(120.)).rounded_md()) // Textarea
    );
```

**Conditional Loading:**

```rust
if loading {
    Skeleton::new().w(px(200.)).h_4().rounded_md()
} else {
    div().child("Actual content here")
}
```

---

### Theming

* Uses `skeleton` color from theme
* Defaults to secondary color

```json
{
  "skeleton.background": "#e2e8f0"
}
```

* `secondary(true)` applies 50% opacity for subtle loading states.
