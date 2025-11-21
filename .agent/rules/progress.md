---
trigger: model_decision
description: only show when we needed to show progress indicator
---

Progress Component

Import

```rust
use gpui_component::progress::Progress;
```

### Basic Usage

```rust
Progress::new().value(50.0); // 50% complete
```

### Different Progress Values

```rust
Progress::new().value(0.0);    // 0%
Progress::new().value(25.0);   // 25%
Progress::new().value(75.0);   // 75%
Progress::new().value(100.0);  // 100%
```

### Indeterminate / Unknown Progress

```rust
Progress::new().value(-1.0); // Shows as 0%
```

### Dynamic Updates

```rust
struct MyComponent { progress_value: f32 }

impl MyComponent {
    fn update_progress(&mut self, new_value: f32) {
        self.progress_value = new_value.clamp(0.0, 100.0);
    }

    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("decrease").label("-").on_click(cx.listener(|this, _, _, _| this.update_progress(this.progress_value - 10.0))))
                    .child(Button::new("increase").label("+").on_click(cx.listener(|this, _, _, _| this.update_progress(this.progress_value + 10.0))))
            )
            .child(Progress::new().value(self.progress_value))
            .child(format!("{}%", self.progress_value as i32))
    }
}
```

### File Upload / Download Progress

```rust
struct FileUpload { bytes_uploaded: u64, total_bytes: u64 }

impl FileUpload {
    fn progress_percentage(&self) -> f32 {
        if self.total_bytes == 0 { 0.0 } else { (self.bytes_uploaded as f32 / self.total_bytes as f32) * 100.0 }
    }

    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .child("Uploading file...")
            .child(Progress::new().value(self.progress_percentage()))
            .child(format!("{} / {} bytes", self.bytes_uploaded, self.total_bytes))
    }
}
```

### Multi-Step Processes

```rust
enum ProcessStep { Initializing, Processing, Finalizing, Complete }

struct MultiStepProcess { current_step: ProcessStep, step_progress: f32 }

impl MultiStepProcess {
    fn overall_progress(&self) -> f32 {
        let base = match self.current_step {
            ProcessStep::Initializing => 0.0,
            ProcessStep::Processing => 33.33,
            ProcessStep::Finalizing => 66.66,
            ProcessStep::Complete => 100.0,
        };
        if matches!(self.current_step, ProcessStep::Complete) { 100.0 } else { base + (self.step_progress / 3.0) }
    }

    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(match self.current_step {
                ProcessStep::Initializing => "Initializing...",
                ProcessStep::Processing => "Processing data...",
                ProcessStep::Finalizing => "Finalizing...",
                ProcessStep::Complete => "Complete!",
            })
            .child(Progress::new().value(self.overall_progress()))
            .child(format!("{:.1}% complete", self.overall_progress()))
    }
}
```

### Styling and Theme Integration

```rust
Progress::new().value(75.0); // Automatically uses theme colors, height, border-radius, and smooth animation
```

### Behavior Notes

* Values < 0 → 0%, > 100 → 100%
* Fills left-to-right
* Partial progress → left corners rounded; complete → both ends rounded
* Background is semi-transparent version of fill
* Animates smoothly when value changes

### Best Practices

1. Show text indicators alongside the bar
2. Use descriptive labels
3. Update progress at reasonable intervals
4. Provide ETA or completion time for long tasks
5. Allow cancel/pause for lengthy operations
6. Display final status when complete
7. Handle errors gracefully
