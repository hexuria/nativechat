---
trigger: model_decision
description: use this only when we need to show label
---

### Label Component Overview

**Import:**

```rust
use gpui_component::label::{Label, HighlightsMatch};
```

---

### Basic Usage

```rust
Label::new("This is a label");
```

**With secondary text:**

```rust
Label::new("Company Address").secondary("(optional)");
Label::new("Email Address").secondary("(required)");
```

---

### Text Alignment

```rust
Label::new("Left aligned"); // default
Label::new("Center aligned").text_center();
Label::new("Right aligned").text_right();
```

---

### Text Highlighting

```rust
Label::new("Hello World Hello").highlights("Hello"); // highlight all
Label::new("Hello World").highlights(HighlightsMatch::Prefix("Hello".into())); // only prefix
Label::new("Company Name").secondary("(optional)").highlights("Company");
```

---

### Color & Styling

```rust
use gpui_component::green_500;

// Text color
Label::new("Green Text").text_color(green_500());

// Font styling
Label::new("Styled Label")
    .text_size(px(20.))
    .font_semibold()
    .line_height(rems(1.8));
```

---

### Masked Labels

```rust
Label::new("9,182.1 USD").text_2xl().masked(true);
Label::new("500 USD").text_xl().masked(self.masked);
```

---

### Multi-line Text

```rust
div().w(px(200.)).child(
    Label::new("This text wraps to the next line if too long.")
        .line_height(rems(1.8))
);
```

---

### Sizes

```rust
Label::new("Extra Large").text_2xl();
Label::new("Large").text_xl();
Label::new("Medium").text_base(); // default
Label::new("Small").text_sm();
Label::new("Extra Small").text_xs();
```

---

### API Reference

**Label Methods**

* `new(text)` – create label
* `secondary(text)` – add secondary text
* `masked(bool)` – hide/show text
* `highlights(match)` – highlight text

**HighlightsMatch Variants**

* `Full(text)` – all occurrences
* `Prefix(text)` – only if at start
* `as_str()` – get text
* `is_prefix()` – check prefix

**Styling Methods**

* `text_color(color)` – set color
* `text_size(size)` – font size
* `text_center()` / `text_right()` – alignment
* `font_semibold()` / `font_bold()` – weight
* `line_height(height)` – line height
* `text_xs()`, `text_sm()`, `text_base()`, `text_lg()`, `text_xl()`, `text_2xl()` – sizes

---

### Examples

**Form Labels**

```rust
Label::new("Email Address").secondary("*").text_color(cx.theme().destructive);
Label::new("Phone Number").secondary("(optional)");
Label::new("Password").secondary("(minimum 8 characters)");
```

**Search Highlighting**

```rust
let search_term = "Hello";
Label::new("Hello World Hello Universe").highlights(search_term);
```

**Sensitive Information**

```rust
h_flex()
    .child(Label::new("$9,182.50 USD").text_2xl().masked(self.is_masked))
    .child(Button::new("toggle-mask")
        .ghost()
        .icon(if self.is_masked { IconName::EyeOff } else { IconName::Eye })
        .on_click(|this, _, _, _| { this.is_masked = !this.is_masked; }));
```

**Multi-language Support**

```rust
Label::new("这是一个标签");       // Chinese
Label::new("こんにちは世界");      // Japanese
Label::new("🌍 Hello World 🚀");  // Emojis
```

**Status Indicators**

```rust
Label::new("✓ Verified").text_color(cx.theme().success);
Label::new("⚠ Pending Review").text_color(cx.theme().warning);
Label::new("✗ Failed").text_color(cx.theme().destructive);
```

**Custom Layouts**

```rust
// Flex layout
h_flex()
    .justify_between()
    .child(Label::new("Total Amount"))
    .child(Label::new("$1,234.56").font_semibold());

// Grid layout
v_flex()
    .gap_2()
    .child(Label::new("Name:").font_semibold())
    .child(Label::new("John Doe"))
    .child(Label::new("Email:").font_semibold())
    .child(Label::new("john@example.com"));
```
