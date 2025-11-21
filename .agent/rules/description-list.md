---
trigger: model_decision
description: only use when we need description list
---

DescriptionList

Import

```rust
use gpui_component::description_list::{DescriptionList, DescriptionItem, DescriptionText};
```

Basic

```rust
DescriptionList::new()
    .child("Name", "GPUI Component", 1)
    .child("Version", "0.1.0", 1)
    .child("License", "Apache-2.0", 1)
```

Using DescriptionItem

```rust
DescriptionList::new()
    .children([
        DescriptionItem::new("Name").value("GPUI Component"),
        DescriptionItem::new("Description").value("UI components for building desktop applications"),
        DescriptionItem::new("Version").value("0.1.0"),
    ])
```

Layouts

```rust
DescriptionList::horizontal()
    .item("Platform", "macOS, Windows, Linux", 1)
    .item("Repository", "https://github.com/longbridge/gpui-component", 1)

DescriptionList::vertical()
    .item("Name", "GPUI Component", 1)
    .item("Description", "A comprehensive UI component library", 1)
```

Multiple columns

```rust
DescriptionList::new()
    .columns(3)
    .child(DescriptionItem::new("Name").value("GPUI Component").span(1))
    .children([
        DescriptionItem::new("Version").value("0.1.0").span(1),
        DescriptionItem::new("License").value("Apache-2.0").span(1),
        DescriptionItem::new("Description")
            .value("Full-featured UI components for desktop applications")
            .span(3),
        DescriptionItem::new("Repository")
            .value("https://github.com/longbridge/gpui-component")
            .span(2),
    ])
```

With dividers

```rust
DescriptionList::new()
    .item("Name", "GPUI Component", 1)
    .item("Version", "0.1.0", 1)
    .divider()
    .item("Author", "Longbridge", 1)
    .item("License", "Apache-2.0", 1)
```

Sizes

```rust
DescriptionList::new().large().item("Title", "Large Description List", 1)
DescriptionList::new().item("Title", "Medium Description List", 1)
DescriptionList::new().small().item("Title", "Small Description List", 1)
```

Without borders

```rust
DescriptionList::new().bordered(false)
    .item("Name", "GPUI Component", 1)
    .item("Type", "UI Library", 1)
```

Custom label width

```rust
use gpui::px;

DescriptionList::horizontal()
    .label_width(px(200.0))
    .item("Very Long Label Name", "Short Value", 1)
    .item("Short", "Very long value that needs more space", 1)
```

Rich content

```rust
use gpui_component::text::TextView;

DescriptionList::new()
    .columns(2)
    .children([
        DescriptionItem::new("Name").value("GPUI Component"),
        DescriptionItem::new("Description").value(
            TextView::markdown(0, "UI components for building **fantastic** desktop applications.", window, cx).into_any_element()
        ),
    ])
```

Complex example

```rust
DescriptionList::new()
    .columns(3)
    .label_width(px(150.0))
    .children([
        DescriptionItem::new("Project Name").value("GPUI Component").span(1),
        DescriptionItem::new("Version").value("0.1.0").span(1),
        DescriptionItem::new("Status").value("Active").span(1),
        DescriptionItem::Divider,
        DescriptionItem::new("Description").value("A comprehensive UI component library for building desktop applications with GPUI").span(3),
        DescriptionItem::new("Repository").value("https://github.com/longbridge/gpui-component").span(2),
        DescriptionItem::new("License").value("Apache-2.0").span(1),
        DescriptionItem::new("Platforms").value("macOS, Windows, Linux").span(2),
        DescriptionItem::new("Language").value("Rust").span(1),
    ])
```

User profile

```rust
DescriptionList::new()
    .columns(2)
    .bordered(true)
    .children([
        DescriptionItem::new("Full Name").value("John Doe"),
        DescriptionItem::new("Email").value("john@example.com"),
        DescriptionItem::new("Phone").value("+1 (555) 123-4567"),
        DescriptionItem::new("Department").value("Engineering"),
        DescriptionItem::Divider,
        DescriptionItem::new("Bio").value("Senior software engineer with 10+ years of experience in Rust and system programming.").span(2),
    ])
```

System info

```rust
DescriptionList::vertical()
    .small()
    .bordered(false)
    .children([
        DescriptionItem::new("Operating System").value("macOS 14.0"),
        DescriptionItem::new("Architecture").value("Apple Silicon (M2)"),
        DescriptionItem::new("Memory").value("16 GB"),
        DescriptionItem::new("Storage").value("512 GB SSD"),
        DescriptionItem::new("GPU").value("Apple M2 10-core GPU"),
    ])
```

Product specs

```rust
DescriptionList::new()
    .columns(3)
    .large()
    .children([
        DescriptionItem::new("Model").value("MacBook Pro").span(1),
        DescriptionItem::new("Year").value("2023").span(1),
        DescriptionItem::new("Screen Size").value("14-inch").span(1),
        DescriptionItem::new("Processor").value("Apple M2 Pro").span(2),
        DescriptionItem::new("Base Price").value("$1,999").span(1),
        DescriptionItem::Divider,
        DescriptionItem::new("Key Features").value("Liquid Retina XDR display, ProMotion technology, P3 wide color gamut").span(3),
    ])
```

Configuration

```rust
DescriptionList::horizontal()
    .label_width(px(180.0))
    .bordered(false)
    .children([
        DescriptionItem::new("Theme").value("Dark Mode"),
        DescriptionItem::new("Font Size").value("14px"),
        DescriptionItem::new("Auto Save").value("Enabled"),
        DescriptionItem::new("Backup Frequency").value("Every 30 minutes"),
        DescriptionItem::new("Language").value("English (US)"),
    ])
```
