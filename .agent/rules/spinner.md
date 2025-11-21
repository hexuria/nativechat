---
trigger: model_decision
description: only use this when we need to show some action needs to be awaited to be completed like network request form submission
---

Spinner Component

Import:

```rust
use gpui_component::spinner::Spinner;
```

### Basic Usage

```rust
Spinner::new(); // Default loader
```

### Custom Color

```rust
Spinner::new().color(cx.theme().blue);   // Blue
Spinner::new().color(cx.theme().green);  // Green
Spinner::new().color(cx.theme().cyan);   // Custom
```

### Sizes

```rust
Spinner::new().xsmall();             // ~12px
Spinner::new().small();              // ~14px
Spinner::new();                      // Medium (16px default)
Spinner::new().large();              // ~24px
Spinner::new().with_size(px(64.));   // Custom
```

### Custom Icon

```rust
use gpui_component::IconName;

Spinner::new().icon(IconName::LoaderCircle);
Spinner::new().icon(IconName::LoaderCircle).large().color(cx.theme().cyan);
Spinner::new().icon(IconName::Loader).color(cx.theme().primary);
```

### Available Icons

* `Loader` (default, rotating line)
* `LoaderCircle` (circular spinner)
* Any `IconName` can be used, but loading icons rotate best

### Animation

* 360° rotation
* Duration: 0.8s (configurable)
* Easing: ease-in-out
* Repeat: infinite

### Examples

**Loading States**

```rust
Spinner::new();
Spinner::new().color(cx.theme().blue);
Spinner::new().large().color(cx.theme().primary);
```

**Different Loading Icons**

```rust
Spinner::new().color(cx.theme().muted_foreground);
Spinner::new().icon(IconName::LoaderCircle).color(cx.theme().blue);
Spinner::new().icon(IconName::LoaderCircle).large().color(cx.theme().green);
```

**Status Spinners**

```rust
Spinner::new().small().color(cx.theme().muted_foreground); // loading
Spinner::new().icon(IconName::LoaderCircle).color(cx.theme().blue); // processing
Spinner::new().icon(IconName::LoaderCircle).color(cx.theme().green); // success
```

**Size Variations**

```rust
Spinner::new().xsmall().color(cx.theme().muted_foreground);
Spinner::new().small().color(cx.theme().primary_foreground);
Spinner::new().color(cx.theme().primary); // medium default
Spinner::new().large().color(cx.theme().blue);
Spinner::new().with_size(px(32.)).color(cx.theme().orange);
```

**In UI Components**

```rust
Button::new("submit-btn")
    .loading(true)
    .icon(Spinner::new().small().color(cx.theme().primary_foreground))
    .label("Loading...");

div()
    .flex()
    .items_center()
    .gap_2()
    .child("Processing...")
    .child(Spinner::new().small().color(cx.theme().muted_foreground));

div()
    .flex()
    .items_center()
    .justify_center()
    .h_full()
    .w_full()
    .child(Spinner::new().large().color(cx.theme().primary));
```

### Performance

* Uses CSS transforms for efficiency
* Shared animation timing for multiple spinners
* Lightweight and suitable for frequent updates
* Smaller sizes recommended for many spinners

### Common Patterns

**Conditional Loading**

```rust
.when(is_loading, |this| {
    this.child(Spinner::new().small().color(cx.theme().muted_foreground))
});
```

**Loading with Text**

```rust
h_flex()
    .items_center()
    .gap_2()
    .child(Spinner::new().small().color(cx.theme().primary))
    .child("Loading data...");
```

**Overlay Loading**

```rust
div()
    .absolute()
    .inset_0()
    .flex()
    .items_center()
    .justify_center()
    .bg(cx.theme().background.alpha(0.8))
    .child(
        v_flex()
            .items_center()
            .gap_3()
            .child(Spinner::new().large().color(cx.theme().primary))
            .child("Loading...")
    );
```
