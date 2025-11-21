---
trigger: model_decision
description: only  use when we need to show notifications
---

Notification

Import

```rust
use gpui_component::notification::{Notification, NotificationType};
use gpui_component::WindowExt;
```

Setup root

```rust
struct MyApp { view: AnyView }

impl Render for MyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let notification_layer = Root::render_notification_layer(window, cx);

        div().size_full()
            .child(v_flex().size_full()
                .child(TitleBar::new())
                .child(div().flex_1().overflow_hidden().child(self.view.clone()))
            )
            .children(notification_layer)
    }
}
```

Basic

```rust
window.push_notification("This is a notification.", cx);
Notification::new().message("Your changes have been saved.");
```

Types

```rust
window.push_notification((NotificationType::Info, "File saved."), cx);
window.push_notification((NotificationType::Success, "Payment processed."), cx);
window.push_notification((NotificationType::Warning, "Network unstable."), cx);
window.push_notification((NotificationType::Error, "Failed to save file."), cx);
```

With title

```rust
Notification::new()
    .title("Update Available")
    .message("New version ready")
    .with_type(NotificationType::Info);
```

Auto-hide

```rust
Notification::new().message("Manual dismiss only").autohide(false);
Notification::new().message("Auto-hide after 5s").autohide(true);
```

Action button

```rust
Notification::new()
    .title("Connection Lost")
    .message("Unable to connect")
    .with_type(NotificationType::Error)
    .autohide(false)
    .action(|_, cx| {
        Button::new("retry").primary().label("Retry").on_click(cx.listener(|this, _, window, cx| {
            println!("Retrying connection...");
            this.dismiss(window, cx);
        }))
    });
```

Clickable

```rust
Notification::new()
    .message("Click to view details")
    .on_click(cx.listener(|_, _, _, cx| {
        println!("Notification clicked");
        cx.notify();
    }));
```

Custom content

```rust
use gpui_component::text::TextView;

let markdown_content = r#"
## Custom Notification
- **Feature**: New dashboard
- **Status**: Ready
- [Learn more](https://example.com)
"#;

Notification::new()
    .content(|_, window, cx| {
        TextView::markdown("custom-content", markdown_content, window, cx).into_any_element()
    });
```

Unique notifications

```rust
struct UpdateNotification;
Notification::new().id::<UpdateNotification>().message("System update available").autohide(false);

struct TaskNotification;
Notification::warning("Task failed").id1::<TaskNotification>("task-123").title("Task Failed");

// Remove later
window.remove_notification::<UpdateNotification>(cx);
```

Examples

Form validation

```rust
Notification::error("Please correct errors")
    .title("Validation Failed")
    .autohide(false)
    .action(|_, _, cx| {
        Button::new("review").outline().label("Review Form").on_click(cx.listener(|this, _, window, cx| {
            this.dismiss(window, cx);
        }))
    });
```

File upload

```rust
struct UploadNotification;

window.push_notification(
    Notification::info("Uploading file...").id::<UploadNotification>().title("File Upload").autohide(false),
    cx,
);

window.push_notification(
    Notification::success("File uploaded!").id::<UploadNotification>().title("Upload Complete"),
    cx,
);
```

System status

```rust
Notification::warning("Maintenance in 30 min.")
    .title("Scheduled Maintenance")
    .autohide(false)
    .action(|_, cx| {
        Button::new("details").link().label("View Details").on_click(cx.listener(|this, _, window, cx| {
            this.dismiss(window, cx);
        }))
    });
```

Batch results

```rust
let results_content = r#"
## Batch Operation Complete
**Processed**: 150 items
**Success**: 147
**Failed**: 3
[View failed items](/)
"#;

Notification::success("Batch completed with some failures.")
    .title("Operation Results")
    .content(|window, cx| TextView::markdown("results", results_content, window, cx).into_any_element())
    .autohide(false);
```

Interactive confirmation

```rust
struct SaveConfirmation;

Notification::new()
    .id::<SaveConfirmation>()
    .title("Unsaved Changes")
    .message("Save before leaving?")
    .autohide(false)
    .action(|_, cx| {
        Button::new("save").primary().label("Save").on_click(cx.listener(|this, _, window, cx| {
            println!("Saving changes...");
            this.dismiss(window, cx);
        }))
    })
    .on_click(cx.listener(|_, _, _, cx| {
        println!("Save reminder clicked");
        cx.notify();
    }));
```

Positioning

* Fixed top-right: `absolute().top_4().right_4()`
* Stack: newer below older
* Max visible: 10
* Animation: slide down on show, slide right on dismiss
* Hover expands list

Animation & timing

* Show: 0.25s, cubic-bezier(0.4,0,0.2,1), slide down + fade in
* Dismiss: 0.15s, slide right + fade out
* Auto-hide: default 5s, hover pauses timer, manual dismiss immediate

Best practices

* Keep titles short (1–3 words)
* Clear, actionable messages
* Use correct notification type
* Auto-hide for info/success, manual for errors
* Action buttons for actionable notifications
* Use unique IDs to prevent duplicates
* Limit frequency and clean up subscriptions
* Timing: Success/info 5s, warnings 7–10s, errors/manual, progress updates/manual with updates
