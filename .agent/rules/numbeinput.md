---
trigger: model_decision
description: only use when we need an input field but specific to number
---

### NumberInput Component Overview

**Import:**

```rust
use gpui_component::input::{InputState, NumberInput, NumberInputEvent, StepAction};
```

---

### Core Features

* Numeric input with increment/decrement buttons
* Min/max validation and step control
* Supports integer and floating-point numbers
* Number formatting with thousands separators
* Prefix/suffix elements (currency symbols, info buttons)
* Multiple sizes: small, medium (default), large
* Disabled state and custom styling
* Programmatic control via `increment()` and `decrement()`
* Keyboard navigation: ↑ / ↓ to step, Enter to confirm

---

### Basic Usage

```rust
let number_input = cx.new(|cx|
    InputState::new(window, cx)
        .placeholder("Enter number")
        .default_value("1")
);

NumberInput::new(&number_input)
```

**With Min/Max Validation**

```rust
InputState::new(window, cx)
    .pattern(Regex::new(r"^\d+$").unwrap()) // Only positive integers
```

**With Number Formatting**

```rust
use gpui_component::input::MaskPattern;

InputState::new(window, cx)
    .mask_pattern(MaskPattern::Number {
        separator: Some(','),
        fraction: Some(2),
    })
```

---

### Sizes

```rust
NumberInput::new(&input).small()
NumberInput::new(&input)        // medium (default)
NumberInput::new(&input).large()
```

### Prefix & Suffix

```rust
NumberInput::new(&input).prefix(div().child("$"))

NumberInput::new(&input).suffix(
    Button::new("info").ghost().icon(IconName::Info).xsmall()
)
```

### Disabled & Styling

```rust
NumberInput::new(&input).disabled(true)

div()
    .w_full()
    .bg(cx.theme().secondary)
    .rounded_md()
    .child(NumberInput::new(&input).appearance(false))
```

---

### Handling Events

```rust
cx.subscribe_in(&number_input, window, |view, state, event, window, cx| {
    match event {
        NumberInputEvent::Step(step_action) => match step_action {
            StepAction::Increment => {
                view.value += 1;
                state.update(cx, |input, cx| input.set_value(view.value.to_string(), window, cx));
            }
            StepAction::Decrement => {
                view.value -= 1;
                state.update(cx, |input, cx| input.set_value(view.value.to_string(), window, cx));
            }
        },
        _ => {}
    }
});
```

**Programmatic Control**

```rust
NumberInput::increment(&number_input, window, cx);
NumberInput::decrement(&number_input, window, cx);
```

---

### Examples

**Integer Counter**

```rust
InputState::new(window, cx)
    .default_value("0")
    .pattern(Regex::new(r"^-?\d+$").unwrap())
```

**Currency Input**

```rust
InputState::new(window, cx)
    .mask_pattern(MaskPattern::Number { separator: Some(','), fraction: Some(2) })
```

**Quantity Selector with Limits**

```rust
pattern(Regex::new(r"^[1-9]\d*$").unwrap()) // restrict 1-99
```

**Floating Point Input**

```rust
pattern(Regex::new(r"^-?\d*\.?\d*$").unwrap()) // decimal numbers
```

---

### Best Practices

1. Validate numeric input both client & server side
2. Set min/max limits for safety
3. Choose step size appropriate for your use case
4. Provide clear error feedback
5. Use consistent number formatting
6. Debounce rapid increments/decrements if needed
7. Ensure accessibility with proper labels

---

This component is perfect for counters, price fields, quantity selectors, and any numeric input where precision, formatting, and step control are needed.
