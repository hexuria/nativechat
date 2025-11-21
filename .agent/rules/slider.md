---
trigger: model_decision
description: only use when we need a slider
---

### Slider Component Overview

**Import:**

```rust
use gpui_component::slider::{Slider, SliderState, SliderEvent, SliderValue};
```

---

### Basic Slider

```rust
let slider_state = cx.new(|_| {
    SliderState::new()
        .min(0.0)
        .max(100.0)
        .default_value(50.0)
        .step(1.0)
});

Slider::new(&slider_state)
```

---

### Slider Variants

**Range Slider**

```rust
let range_slider = cx.new(|_| {
    SliderState::new()
        .min(0.0)
        .max(100.0)
        .default_value(20.0..80.0)
        .step(1.0)
});
Slider::new(&range_slider)
```

**Vertical Slider**

```rust
Slider::new(&slider_state).vertical().h(px(200.))
```

**Custom Step Intervals**

```rust
SliderState::new().min(0.0).max(10.0).step(1.0).default_value(5.0);   // integer steps
SliderState::new().min(0.0).max(1.0).step(0.01).default_value(0.5);    // decimal steps
```

**Min/Max Configuration**

```rust
SliderState::new().min(-10.0).max(40.0).default_value(20.0).step(0.5);  // temperature
SliderState::new().min(0.0).max(100.0).default_value(75.0).step(5.0);   // percentage
```

**Disabled State**

```rust
Slider::new(&slider_state).disabled(true)
```

---

### Event Handling

```rust
cx.subscribe(&slider_state, |this, _, event: &SliderEvent, cx| {
    if let SliderEvent::Change(value) = event {
        this.current_value = value.start();
        cx.notify();
    }
});
```

---

### Custom Styling

```rust
Slider::new(&slider_state)
    .bg(cx.theme().success)
    .text_color(cx.theme().success_foreground)
    .rounded(px(4.))
```

**Scale Options**

* `Linear` (default)
* `Logarithmic`:

```rust
SliderState::new()
    .min(1.0)
    .max(1000.0)
    .default_value(10.0)
    .step(1.0)
    .scale(SliderScale::Logarithmic)
```

> Formula: `v = min * (max/min)^p` where `p` = slider percentage (0–1)

---

### SliderValue Conversions

```rust
let single_value: SliderValue = 42.0.into();
let range_value: SliderValue = (10.0, 90.0).into();
let range_value: SliderValue = (10.0..90.0).into();
```

---

### Examples

**Color Picker**

* Separate sliders for hue, saturation, lightness, alpha
* Subscribe to changes to update `current_color`

**Volume Control**

* Single slider bound to `volume` variable
* Update audio system on change

**Price Range Filter**

* Range slider controlling `min_price` and `max_price`
* Updates product filtering on change

**Temperature Control**

* Slider with dynamic background color based on value
* Example: blue for cold, green for comfortable, red for hot

---

### Keyboard Shortcuts

| Key           | Action                    |
| ------------- | ------------------------- |
| `←` / `↓`     | Decrease value by step    |
| `→` / `↑`     | Increase value by step    |
| `Page Down`   | Decrease by larger amount |
| `Page Up`     | Increase by larger amount |
| `Home`        | Set to minimum value      |
| `End`         | Set to maximum value      |
| `Tab`         | Focus next element        |
| `Shift + Tab` | Focus previous element    |

This provides a full **overview of single, range, vertical sliders, event handling, custom styling, scaling, and practical examples** with `gpui_component`.
