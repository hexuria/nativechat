---
trigger: model_decision
description: only use this when we need a color picker
---

### ColorPicker Component Overview

**Imports:**

```rust
use gpui_component::color_picker::{ColorPicker, ColorPickerState, ColorPickerEvent};
```

---

### Basic Usage

```rust
let color_picker = cx.new(|cx| 
    ColorPickerState::new(window, cx)
        .default_value(cx.theme().primary)
);

ColorPicker::new(&color_picker)
```

---

### Event Handling

```rust
let _subscription = cx.subscribe(&color_picker, |this, _, ev, _| match ev {
    ColorPickerEvent::Change(color) => {
        if let Some(color) = color {
            println!("Selected color: {}", color.to_hex());
        }
    }
});
```

---

### Setting Default Color

```rust
let color_picker = cx.new(|cx| 
    ColorPickerState::new(window, cx)
        .default_value(cx.theme().blue)
);
```

---

### Sizes

```rust
ColorPicker::new(&color_picker).xsmall();
ColorPicker::new(&color_picker).small();
ColorPicker::new(&color_picker);       // Medium (default)
ColorPicker::new(&color_picker).large();
```

---

### Custom Featured Colors

```rust
let featured_colors = vec![
    cx.theme().red,
    cx.theme().green,
    cx.theme().blue,
    cx.theme().yellow,
];

ColorPicker::new(&color_picker)
    .featured_colors(featured_colors)
```

---

### Additional Options

```rust
// Icon instead of color square
ColorPicker::new(&color_picker).icon(IconName::Palette);

// Label
ColorPicker::new(&color_picker).label("Background Color");

// Custom anchor position
ColorPicker::new(&color_picker).anchor(Corner::TopRight);
```

---

### Color Formats

**HSL (native):**

```rust
use gpui::Hsla;
let color = Hsla::hsl(240.0, 100.0, 50.0);
let hue = color.h;
let saturation = color.s;
let lightness = color.l;
```

**Hex:**

```rust
let hex_string = color.to_hex(); // "#3366FF"
if let Ok(color) = Hsla::parse_hex("#3366FF") { }
```

**Alpha Channel:**

```rust
use gpui::hsla;
let semi_transparent = hsla(0.5, 0.8, 0.6, 0.7);
let transparent_blue = cx.theme().blue.opacity(0.5);
```

---

### Examples

**Theme Editor:**

```rust
struct ThemeEditor {
    primary_color: Entity<ColorPickerState>,
    secondary_color: Entity<ColorPickerState>,
    accent_color: Entity<ColorPickerState>,
}

impl ThemeEditor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self { /* initialize pickers */ }

    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_4()
            .child(h_flex().gap_2().items_center().child("Primary:").child(ColorPicker::new(&self.primary_color)))
            .child(h_flex().gap_2().items_center().child("Secondary:").child(ColorPicker::new(&self.secondary_color)))
            .child(h_flex().gap_2().items_center().child("Accent:").child(ColorPicker::new(&self.accent_color)))
    }
}
```

**Brand Color Selector:**

```rust
let brand_colors = vec![
    Hsla::parse_hex("#FF6B6B").unwrap(),
    Hsla::parse_hex("#4ECDC4").unwrap(),
    Hsla::parse_hex("#45B7D1").unwrap(),
    Hsla::parse_hex("#96CEB4").unwrap(),
    Hsla::parse_hex("#FFEAA7").unwrap(),
];

ColorPicker::new(&color_picker)
    .featured_colors(brand_colors)
    .label("Brand Color")
    .large();
```

**Toolbar Picker:**

```rust
ColorPicker::new(&text_color_picker)
    .icon(IconName::Type)
    .small()
    .anchor(Corner::BottomLeft);
```

**Color Palette Builder:**

```rust
struct ColorPalette { colors: Vec<Entity<ColorPickerState>> }

impl ColorPalette {
    fn add_color(&mut self, window: &mut Window, cx: &mut Context<Self>) { /* subscribe and add */ }

    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .children(self.colors.iter().map(|c| ColorPicker::new(c).small()))
            .child(Button::new("add-color").icon(IconName::Plus).ghost().on_click(cx.listener(|this, _, w, cx| this.add_color(w, cx))));
    }
}
```

**With Validation:**

```rust
cx.subscribe(&color_picker, |this, _, ev, _| match ev {
    ColorPickerEvent::Change(color) => {
        if let Some(color) = color {
            if this.validate_contrast(color) {
                this.apply_color(color);
            } else {
                this.show_contrast_warning();
            }
        }
    }
});
```
