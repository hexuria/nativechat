---
trigger: model_decision
description: only use when we need to pick one option among many options
---

### Radio Component Overview

**Imports:**

```rust
use gpui_component::radio::{Radio, RadioGroup};
```

---

### Basic Radio Button

```rust
Radio::new("radio-option-1")
    .label("Option 1")
    .checked(false)
    .on_click(|checked, _, _| println!("Radio is now: {}", checked));
```

---

### Controlled Radio Button

```rust
struct MyView {
    radio_checked: bool,
}

impl Render for MyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Radio::new("radio")
            .label("Select this option")
            .checked(self.radio_checked)
            .on_click(cx.listener(|view, checked, _, cx| {
                view.radio_checked = *checked;
                cx.notify();
            }))
    }
}
```

---

### Radio Group (Recommended)

```rust
struct MyView {
    selected_option: Option<usize>,
}

impl Render for MyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        RadioGroup::horizontal("options")
            .children(["Option 1", "Option 2", "Option 3"])
            .selected_index(self.selected_option)
            .on_change(cx.listener(|view, selected_index: &usize, _, cx| {
                view.selected_option = Some(*selected_index);
                cx.notify();
            }))
    }
}
```

---

### Different Sizes

```rust
Radio::new("small").label("Small").xsmall();
Radio::new("medium").label("Medium"); // default
Radio::new("large").label("Large").large();
```

---

### Disabled State

```rust
Radio::new("disabled")
    .label("Disabled option")
    .checked(false)
    .disabled(true);

Radio::new("disabled-checked")
    .label("Disabled and checked")
    .checked(true)
    .disabled(true);
```

---

### Multi-line Label with Custom Content

```rust
Radio::new("custom")
    .label("Primary option")
    .child(
        div()
            .text_color(cx.theme().muted_foreground)
            .child("This is additional descriptive text that provides more context.")
    )
    .w(px(300.));
```

---

### Custom Tab Order

```rust
Radio::new("radio")
    .label("Custom tab order")
    .tab_index(2)
    .tab_stop(true);
```

---

### Radio Group Layouts

**Horizontal:**

```rust
RadioGroup::horizontal("horizontal-group")
    .children(["First", "Second", "Third"])
    .selected_index(Some(0))
    .on_change(cx.listener(|view, index, _, cx| println!("Selected index: {}", index)));
```

**Vertical:**

```rust
RadioGroup::vertical("vertical-group")
    .child(Radio::new("option1").label("United States"))
    .child(Radio::new("option2").label("Canada"))
    .child(Radio::new("option3").label("Mexico"))
    .selected_index(Some(1))
    .disabled(false);
```

**Styled:**

```rust
RadioGroup::vertical("styled-group")
    .w(px(220.))
    .p_2()
    .border_1()
    .border_color(cx.theme().border)
    .rounded_md()
    .child(Radio::new("option1").label("Option 1"))
    .child(Radio::new("option2").label("Option 2"))
    .child(Radio::new("option3").label("Option 3"))
    .selected_index(Some(0));
```

**Disabled Group:**

```rust
RadioGroup::vertical("disabled-group")
    .children(["Option A", "Option B", "Option C"])
    .selected_index(Some(1))
    .disabled(true);
```

---

### Examples

**Settings Panel**

```rust
struct SettingsView {
    theme: Option<usize>,    // 0: Light, 1: Dark, 2: Auto
    language: Option<usize>, // 0: English, 1: Spanish, 2: French
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_6()
            .child(v_flex().gap_2()
                .child(div().text_sm().font_semibold().child("Theme"))
                .child(
                    RadioGroup::vertical("theme")
                        .child(Radio::new("light").label("Light"))
                        .child(Radio::new("dark").label("Dark"))
                        .child(Radio::new("auto").label("Auto"))
                        .selected_index(self.theme)
                        .on_change(cx.listener(|view, index, _, cx| {
                            view.theme = Some(*index);
                            cx.notify();
                        }))
                )
            )
            .child(v_flex().gap_2()
                .child(div().text_sm().font_semibold().child("Language"))
                .child(
                    RadioGroup::horizontal("language")
                        .children(["English", "Español", "Français"])
                        .selected_index(self.language)
                        .on_change(cx.listener(|view, index, _, cx| {
                            view.language = Some(*index);
                            cx.notify();
                        }))
                )
            );
    }
}
```

**Survey Form**

```rust
struct SurveyView {
    satisfaction: Option<usize>,
    recommendation: Option<usize>,
}

impl Render for SurveyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_8()
            .child(v_flex().gap_3()
                .child(div().text_base().font_medium().child("How satisfied are you with our service?"))
                .child(
                    RadioGroup::vertical("satisfaction")
                        .child(Radio::new("very-satisfied").label("Very satisfied"))
                        .child(Radio::new("satisfied").label("Satisfied"))
                        .child(Radio::new("neutral").label("Neutral"))
                        .child(Radio::new("dissatisfied").label("Dissatisfied"))
                        .child(Radio::new("very-dissatisfied").label("Very dissatisfied"))
                        .selected_index(self.satisfaction)
                        .on_change(cx.listener(|view, index, _, cx| { view.satisfaction = Some(*index); cx.notify(); }))
                )
            )
            .child(v_flex().gap_3()
                .child(div().text_base().font_medium().child("How likely are you to recommend us?"))
                .child(
                    RadioGroup::horizontal("recommendation")
                        .children((0..=10).map(|i| i.to_string()))
                        .selected_index(self.recommendation)
                        .on_change(cx.listener(|view, index, _, cx| { view.recommendation = Some(*index); cx.notify(); }))
                )
            );
    }
}
```

**Payment Method Selection**

```rust
struct PaymentView {
    payment_method: Option<usize>,
}

impl Render for PaymentView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_4()
            .child(div().text_lg().font_semibold().child("Select Payment Method"))
            .child(
                RadioGroup::vertical("payment")
                    .child(Radio::new("credit-card").label("Credit Card")
                        .child(div().text_color(cx.theme().muted_foreground).child("Visa, MasterCard, American Express"))
                    )
                    .child(Radio::new("paypal").label("PayPal")
                        .child(div().text_color(cx.theme().muted_foreground).child("Pay with your PayPal account"))
                    )
                    .child(Radio::new("bank-transfer").label("Bank Transfer")
                        .child(div().text_color(cx.theme().muted_foreground).child("Direct bank account transfer"))
                    )
                    .selected_index(self.payment_method)
                    .on_change(cx.listener(|view, index, _, cx| { view.payment_method = Some(*index); cx.notify(); }))
            );
    }
}
```

---

### Best Practices

1. Prefer `RadioGroup` over individual `Radio` buttons.
2. Use descriptive labels for clarity.
3. Provide sensible default selections if required.
4. Arrange options in a logical order.
5. Limit options to 2–7 for usability.
6. Group related options with headings.
7. Use horizontal layout for few options, vertical for many.
