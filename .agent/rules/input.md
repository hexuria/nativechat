---
trigger: model_decision
description: only use when we need input field
---

Input Component

Import

```rust
use gpui_component::input::{InputState, Input};
```

### Basic Usage

```rust
let input = cx.new(|cx| InputState::new(window, cx));
Input::new(&input);
```

### Placeholder / Default Value

```rust
let input = cx.new(|cx| InputState::new(window, cx).placeholder("Enter your name..."));
let input2 = cx.new(|cx| InputState::new(window, cx).default_value("John Doe"));
```

### Cleanable Input

```rust
Input::new(&input).cleanable(true); // Show clear button when input has value
```

### Prefix / Suffix

```rust
Input::new(&input).prefix(Icon::new(IconName::Search).small());
Input::new(&input).suffix(Button::new("info").ghost().icon(IconName::Info).xsmall());
Input::new(&input)
    .prefix(Icon::new(IconName::Search).small())
    .suffix(Button::new("btn").ghost().icon(IconName::Info).xsmall());
```

### Password / Masked Input

```rust
let input = cx.new(|cx| InputState::new(window, cx).masked(true).default_value("password123"));
Input::new(&input).mask_toggle(); // Toggle button to reveal password
```

### Sizes

```rust
Input::new(&input).large();
Input::new(&input); // medium default
Input::new(&input).small();
```

### Disabled / Clean on ESC

```rust
Input::new(&input).disabled(true);
let input = cx.new(|cx| InputState::new(window, cx).clean_on_escape());
```

### Validation / Pattern

```rust
let input = cx.new(|cx| InputState::new(window, cx).validate(|s, _| s.parse::<f32>().is_ok()));
let input2 = cx.new(|cx| InputState::new(window, cx).pattern(regex::Regex::new(r"^[a-zA-Z0-9]*$").unwrap()));
```

### Masking

```rust
let phone_input = cx.new(|cx| InputState::new(window, cx).mask_pattern("(999)-999-9999"));
let custom = cx.new(|cx| InputState::new(window, cx).mask_pattern("AAA-###-AAA"));
let number_input = cx.new(|cx| InputState::new(window, cx).mask_pattern(MaskPattern::Number { separator: Some(','), fraction: Some(3) }));
```

### Handling Input Events

```rust
cx.subscribe_in(&input, window, |_, state, event, _, _| match event {
    InputEvent::Change => println!("Changed: {}", state.read(cx).value()),
    InputEvent::PressEnter { secondary } => println!("Enter pressed, secondary: {}", secondary),
    InputEvent::Focus => println!("Focused"),
    InputEvent::Blur => println!("Blurred"),
});
```

### Custom Appearance

```rust
Input::new(&input).appearance(false); // Remove default styling
div()
    .border_b_2()
    .px_6()
    .py_3()
    .border_color(cx.theme().border)
    .bg(cx.theme().secondary)
    .child(Input::new(&input).appearance(false));
```

### Examples

**Search Input**

```rust
let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
Input::new(&search).prefix(Icon::new(IconName::Search).small());
```

**Currency Input**

```rust
let amount = cx.new(|cx| InputState::new(window, cx).mask_pattern(MaskPattern::Number { separator: Some(','), fraction: Some(2) }));
div()
    .child(Input::new(&amount))
    .child(format!("Value: {}", amount.read(cx).value()));
```

**Form with Multiple Inputs**

```rust
struct FormView { name_input: Entity<InputState>, email_input: Entity<InputState> }

v_flex()
    .gap_3()
    .child(Input::new(&self.name_input))
    .child(Input::new(&self.email_input));
```
