---
trigger: model_decision
description: only use when we need a form
---

### Form Component Overview

**Import:**

```rust
use gpui_component::form::{field, v_form, h_form, Form, Field};
```

---

### Basic Form

```rust
v_form()
    .child(field().label("Name").child(Input::new(&name_input)))
    .child(field().label("Email").child(Input::new(&email_input)).required(true))
```

---

### Layout Options

**Horizontal Form**

```rust
h_form()
    .label_width(px(120.))
    .child(field().label("First Name").child(Input::new(&first_name)))
    .child(field().label("Last Name").child(Input::new(&last_name)))
```

**Multi-Column Form**

```rust
v_form()
    .columns(2)
    .child(field().label("First Name").child(Input::new(&first_name)))
    .child(field().label("Last Name").child(Input::new(&last_name)))
    .child(field().label("Bio").col_span(2).child(Input::new(&bio_input)))
```

**Custom Sizing**

```rust
v_form().large().label_text_size(rems(1.2))
v_form().small()
```

---

### Validation

**Required Fields**

```rust
field().label("Email").required(true).child(Input::new(&email_input))
```

**Field Descriptions**

```rust
field().label("Password").description("At least 8 characters").child(Input::new(&password_input))
```

**Dynamic Descriptions**

```rust
field().label("Bio").description_fn(|_, _| div().child("Use max 100 words")).child(Input::new(&bio_input))
```

**Conditional Visibility**

```rust
field().label("Admin Settings").visible(user.is_admin()).child(Switch::new("admin-mode"))
```

---

### Submit Handling

**Basic Submit**

```rust
Button::new("submit")
    .primary()
    .child("Submit")
    .on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
```

**Action Buttons**

```rust
h_flex()
    .gap_2()
    .child(Button::new("save").primary().child("Save"))
    .child(Button::new("cancel").child("Cancel"))
    .child(Button::new("preview").outline().child("Preview"))
```

---

### Field Groups

**Related Fields**

```rust
field().label("Name").child(
    h_flex()
        .gap_2()
        .child(div().flex_1().child(Input::new(&first_name)))
        .child(div().flex_1().child(Input::new(&last_name)))
)
```

**Custom Field Components**

```rust
field().label("Theme Color").child(ColorPicker::new(&color_state).small())
field().label("Birth Date").child(DatePicker::new(&date_state))
field().label("Notifications").child(
    v_flex()
        .gap_2()
        .child(Switch::new("email").label("Email notifications"))
        .child(Switch::new("push").label("Push notifications"))
)
```

**Conditional Fields**

```rust
field().label("Company Name").visible(is_business_account).child(Input::new(&company_name))
field().label("Tax ID").visible(is_business_account).required(is_business_account).child(Input::new(&tax_id))
```

---

### Grid Layout

**Column Spanning**

```rust
field().label("Full Width").col_span(3).child(Input::new(&full_width))
```

**Column Positioning**

```rust
field().label("Positioned").col_start(1).col_span(2).child(input_positioned)
```

**Responsive Layout**

```rust
v_form()
    .columns(if is_mobile { 1 } else { 2 })
    .child(field().label("Name").child(name_input))
    .child(field().label("Bio").when(!is_mobile, |f| f.col_span(2)).child(bio_input))
```

---

### Examples

**User Registration**

```rust
v_form()
    .large()
    .child(field().label("First Name").child(Input::new(&first_name)))
    .child(field().label("Last Name").child(Input::new(&last_name)))
    .child(field().label("Email").required(true).child(Input::new(&email)))
    .child(field().label("Password").required(true).description("At least 8 characters").child(Input::new(&password)))
    .child(field().label_indent(false).child(Checkbox::new("terms").label("Agree to Terms")))
    .child(field().label_indent(false).child(Button::new("register").primary().w_full().child("Create Account")))
```

**Settings Form with Sections**

```rust
v_form()
    .columns(2)
    .child(field().label("Profile").col_span(2).child(Divider::horizontal()))
    .child(field().label("Display Name").child(Input::new(&display_name)))
    .child(field().label("Email").child(Input::new(&email)))
    .child(field().label("Theme").child(Select::new(&theme_state)))
    .child(field().label_indent(false).child(Switch::new("notifications").label("Enable notifications")))
```

**Contact Form**

```rust
v_form()
    .child(field().label("Name").child(Input::new(&name_input)))
    .child(field().label("Email").required(true).child(Input::new(&email_input)))
    .child(field().label("Message").required(true).items_start().description("Please describe your inquiry").child(Input::new(&message_input)))
    .child(field().label_indent(false).child(
        h_flex()
            .gap_2()
            .child(Checkbox::new("copy").label("Send me a copy"))
            .child(h_flex().gap_2().child(Button::new("cancel").child("Cancel")).child(Button::new("send").primary().child("Send Message")))
    ))
```

This covers the **core usage**, **layout options**, **validation**, **field grouping**, and **examples** for building complex, responsive forms with `gpui_component`.
