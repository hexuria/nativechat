---
trigger: model_decision
description: Use when we need to group components
---

GroupBox

Import

```rust
use gpui_component::group_box::{GroupBox, GroupBoxVariant};
```

Basic

```rust
GroupBox::new()
    .child("Subscriptions")
    .child(Checkbox::new("all").label("All"))
    .child(Checkbox::new("newsletter").label("Newsletter"))
    .child(Button::new("save").primary().label("Save"))
```

Variants

```rust
GroupBox::new()
    .child("Content without visual container")

GroupBox::new()
    .fill()
    .title("Settings")
    .child("Content with background")

GroupBox::new()
    .outline()
    .title("Preferences")
    .child("Content with border")
```

With title

```rust
GroupBox::new()
    .fill()
    .title("Account Settings")
    .child(
        h_flex()
            .justify_between()
            .child("Make profile private")
            .child(Switch::new("privacy").checked(false))
    )
    .child(Button::new("save").primary().label("Save Changes"))
```

Custom id

```rust
GroupBox::new()
    .id("user-preferences")
    .outline()
    .title("User Preferences")
    .child("Preference controls...")
```

Custom title style

```rust
GroupBox::new()
    .outline()
    .title("Custom Title")
    .title_style(
        StyleRefinement::default()
            .font_semibold()
            .line_height(relative(1.0))
            .px_3()
            .text_color(cx.theme().accent)
    )
    .child("Content with custom title styling")
```

Custom content style

```rust
GroupBox::new()
    .fill()
    .title("Custom Content Area")
    .content_style(
        StyleRefinement::default()
            .rounded_xl()
            .py_3()
            .px_4()
            .border_2()
            .border_color(cx.theme().accent)
    )
    .child("Content with custom styling")
```

Complex

```rust
GroupBox::new()
    .id("notification-settings")
    .outline()
    .bg(cx.theme().group_box)
    .rounded_xl()
    .p_5()
    .title("Notification Preferences")
    .title_style(
        StyleRefinement::default()
            .font_semibold()
            .line_height(relative(1.0))
            .px_3()
    )
    .content_style(
        StyleRefinement::default()
            .rounded_xl()
            .py_3()
            .px_4()
            .border_2()
    )
    .child(
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .child("Email notifications")
                    .child(Switch::new("email").checked(true))
            )
            .child(
                h_flex()
                    .justify_between()
                    .child("Push notifications")
                    .child(Switch::new("push").checked(false))
            )
            .child(
                h_flex()
                    .justify_between()
                    .child("SMS notifications")
                    .child(Switch::new("sms").checked(false))
            )
    )
    .child(
        h_flex()
            .justify_end()
            .gap_2()
            .child(Button::new("cancel").label("Cancel"))
            .child(Button::new("save").primary().label("Save Settings"))
    )
```

API

* new()
* variant(...)
* fill()
* outline()
* id(...)
* title(...)
* title_style(...)
* content_style(...)

GroupBoxVariant

* Normal
* Fill
* Outline

Examples

Form section

```rust
GroupBox::new()
    .fill()
    .title("Personal Information")
    .child(
        v_flex()
            .gap_4()
            .child(
                h_flex()
                    .gap_2()
                    .child(Input::new("first-name").placeholder("First Name"))
                    .child(Input::new("last-name").placeholder("Last Name"))
            )
            .child(Input::new("email").placeholder("Email Address"))
            .child(
                h_flex()
                    .justify_end()
                    .child(Button::new("update").primary().label("Update Profile"))
            )
    )
```

Settings panel

```rust
GroupBox::new()
    .outline()
    .title("Display Settings")
    .child(
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .child(Label::new("Theme"))
                    .child(
                        RadioGroup::horizontal("theme")
                            .child(Radio::new("light").label("Light"))
                            .child(Radio::new("dark").label("Dark"))
                            .child(Radio::new("auto").label("Auto"))
                    )
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(Label::new("Font Size"))
                    .child(
                        Select::new("font-size")
                            .option("small", "Small")
                            .option("medium", "Medium")
                            .option("large", "Large")
                    )
            )
    )
```

Subscriptions

```rust
GroupBox::new()
    .title("Email Subscriptions")
    .child(
        v_flex()
            .gap_2()
            .child(Checkbox::new("newsletter").label("Weekly Newsletter"))
            .child(Checkbox::new("updates").label("Product Updates"))
            .child(Checkbox::new("security").label("Security Alerts"))
            .child(Checkbox::new("marketing").label("Marketing Communications"))
    )
    .child(
        h_flex()
            .justify_between()
            .mt_4()
            .child(Button::new("unsubscribe-all").link().label("Unsubscribe All"))
            .child(Button::new("save").primary().label("Update Preferences"))
    )
```

Without title

```rust
GroupBox::new()
    .outline()
    .child(
        h_flex()
            .justify_between()
            .items_center()
            .child("Enable two-factor authentication")
            .child(Switch::new("2fa").checked(false))
    )
```

Styling

Theme

```rust
GroupBox::new()
    .fill()
    .bg(cx.theme().group_box)
    .title("Themed Group Box")
```

Custom appearance

```rust
GroupBox::new()
    .outline()
    .border_2()
    .border_color(cx.theme().accent)
    .rounded_lg()
    .title("Custom Styled Group Box")
    .title_style(
        StyleRefinement::default()
            .text_color(cx.theme().accent)
            .font_bold()
    )
```