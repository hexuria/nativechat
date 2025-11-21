---
trigger: model_decision
description: only use when we need to show alert messages to user e.g. when error has happen or some event has successfully completed in action like form submission
---

Alert Component

Import:

```rust
use gpui_component::alert::Alert;
```

### Basic Usage

```rust
Alert::new("alert-id", "This is a basic alert message.");
```

### With Title

```rust
Alert::new("alert-with-title", "Your changes have been saved successfully.")
    .title("Success!");
```

### Variants

```rust
Alert::info("info-alert", "This is an informational message.").title("Information");
Alert::success("success-alert", "Operation completed successfully.").title("Success!");
Alert::warning("warning-alert", "Please review your settings.").title("Warning");
Alert::error("error-alert", "An error occurred during processing.").title("Error");
```

### Sizes

```rust
Alert::info("alert", "Message").xsmall().title("XSmall Alert");
Alert::info("alert", "Message").small().title("Small Alert");
Alert::info("alert", "Message").title("Medium Alert"); // default
Alert::info("alert", "Message").large().title("Large Alert");
```

### Closable Alerts

```rust
Alert::info("closable-alert", "This alert can be dismissed.")
    .title("Dismissible")
    .on_close(|_event, _window, _cx| {
        println!("Alert was closed");
    });
```

### Banner Mode

```rust
Alert::info("banner-alert", "Full-width banner").banner();
Alert::success("banner-success", "Operation completed!").banner();
Alert::warning("banner-warning", "Maintenance tonight.").banner();
Alert::error("banner-error", "Service temporarily unavailable.").banner();
```

### Custom Icons

```rust
use gpui_component::IconName;

Alert::new("custom-icon", "Meeting scheduled for 3 PM.")
    .title("Calendar Reminder")
    .icon(IconName::Calendar);
```

### Markdown Content

```rust
use gpui_component::text::TextView;

Alert::error(
    "error-with-markdown",
    TextView::markdown(
        "error-message",
        "- Check your card details\n- Ensure sufficient funds\n- Verify billing address",
        window,
        cx,
    ),
)
.title("Payment Failed");
```

### Conditional Visibility

```rust
Alert::info("conditional-alert", "May be hidden.")
    .title("Conditional")
    .visible(should_show_alert);
```

### Examples

**Form Validation Errors**

```rust
Alert::error(
    "validation-error",
    "- Email is required\n- Password must be ≥ 8 chars\n- Accept terms"
).title("Validation Failed");
```

**Success Notification**

```rust
Alert::success("save-success", "Profile updated successfully.")
    .title("Changes Saved")
    .on_close(|_, _, _| {});
```

**System Status Banner**

```rust
Alert::warning(
    "maintenance-banner",
    "Scheduled maintenance tonight 2-4 AM EST. Some services may be down."
)
.banner()
.large();
```

**Interactive Alert with Custom Action**

```rust
Alert::info("update-available", "A new app version is available.")
    .title("Update Available")
    .icon(IconName::Download)
    .on_close(cx.listener(|this, _, _, cx| {
        this.handle_update_notification(cx);
    }));
```

**Multi-line Content with Formatting**

```rust
Alert::warning(
    "security-alert",
    TextView::markdown(
        "security-content",
        "**Security Notice**: Unusual activity detected.\n- Login from new device\n- Location: SF, CA\n- Time: 2:30 PM\nIf not you, [change password](/).",
        window,
        cx,
    )
)
.title("Security Alert")
.icon(IconName::Shield);
```
