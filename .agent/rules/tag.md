---
trigger: model_decision
description: When a Component tag is being required to use use this
---

# **GPUI Tag**

## **Import**

```rust
use gpui_component::tag::Tag;
```

---

# **Core Patterns**

## **1. The Essentials**

```rust
Tag::primary().child("Primary")
Tag::secondary().child("Secondary")
Tag::success().child("Success")
Tag::warning().child("Warning")
Tag::danger().child("Danger")
Tag::info().child("Info")
```

These six cover 90% of real-world usage.

---

# **Size & Shape**

## **Small vs Medium**

```rust
Tag::primary().small().child("Small")
Tag::primary().child("Medium") // default
```

## **Pills**

```rust
Tag::primary().rounded_full().child("Pill")
Tag::primary().small().rounded_full().child("Small Pill")
```

## **Custom radius**

```rust
use gpui::px;

Tag::primary().rounded(px(4.0)).child("Rounded 4px")
Tag::primary().rounded(px(0.0)).child("Square")
```

---

# **Outline Tags**

```rust
Tag::primary().outline().child("Primary")
Tag::warning().outline().child("Warning")
Tag::danger().outline().child("Danger")
```

---

# **Custom Colors**

## **Predefined theme colors**

```rust
use gpui_component::ColorName;

Tag::color(ColorName::Blue).child("Blue")
Tag::color(ColorName::Green).child("Green")
Tag::color(ColorName::Purple).child("Purple")
```

## **Full custom HSLA**

```rust
use gpui::{hsla, Hsla};

let bg = hsla(220.0/360.0, 0.8, 0.5, 1.0);
let fg = hsla(0.0, 0.0, 1.0, 1.0);
let border = hsla(220.0/360.0, 0.8, 0.4, 1.0);

Tag::custom(bg, fg, border).child("Custom")
```

---

# **Most Useful Real-World Patterns**

## **Status indicators**

```rust
Tag::success().child("Operational")
Tag::warning().child("Maintenance")
Tag::danger().child("Down")
```

## **Categories**

```rust
Tag::secondary().child("Tech")
Tag::color(ColorName::Green).child("Dev")
Tag::color(ColorName::Purple).child("Marketing")
```

## **Priority**

```rust
Tag::danger().child("High")
Tag::warning().child("Medium")
Tag::secondary().child("Low")
```

## **Feature flags / metadata**

```rust
Tag::primary().small().child("New")
Tag::info().small().child("Beta")
Tag::success().small().child("Popular")
```

---

# **Tag Groups (practical UI)**

## **Horizontal**

```rust
h_flex()
    .gap_2()
    .child(Tag::primary().child("Rust"))
    .child(Tag::success().child("TypeScript"))
    .child(Tag::info().child("Next.js"))
```

## **Vertical**

```rust
v_flex()
    .gap_1()
    .child(Tag::danger().small().child("Critical"))
    .child(Tag::warning().small().child("Important"))
    .child(Tag::secondary().small().child("Normal"))
```

## **Tag cloud / category set**

```rust
h_flex()
    .flex_wrap()
    .gap_2()
    .child(Tag::color(ColorName::Red).child("Bug"))
    .child(Tag::color(ColorName::Blue).child("Feature"))
    .child(Tag::color(ColorName::Green).child("Enhancement"))
```

---

# **Pill Skills (always looks good)**

```rust
h_flex()
    .gap_2()
    .flex_wrap()
    .child(Tag::color(ColorName::Blue).rounded_full().small().child("Rust"))
    .child(Tag::color(ColorName::Green).rounded_full().small().child("JavaScript"))
    .child(Tag::color(ColorName::Purple).rounded_full().small().child("Python"))
```

---

# **Opinionated Best Practices**

* Use **semantic tags** for state (success, warning, danger).
* Use **ColorName** for categories (consistent + scalable).
* Use **small + rounded_full** for metadata and skill clouds.
* Use **outline** when the background is visually noisy.
* Avoid custom HSLA unless you’re aligning with brand colors.
