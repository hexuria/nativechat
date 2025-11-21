---
trigger: model_decision
description: use this when you need to add a new sidebar or chat side panel
---

### Sidebar Component Overview

**Imports:**

```rust
use gpui_component::sidebar::{
    Sidebar, SidebarHeader, SidebarFooter, SidebarGroup,
    SidebarMenu, SidebarMenuItem, SidebarToggleButton
};
```

---

### Basic Sidebar

```rust
Sidebar::new(Side::Left)
    .header(SidebarHeader::new().child("My Application"))
    .child(
        SidebarGroup::new("Navigation")
            .child(
                SidebarMenu::new()
                    .child(SidebarMenuItem::new("Dashboard").icon(IconName::LayoutDashboard))
                    .child(SidebarMenuItem::new("Settings").icon(IconName::Settings))
            )
    )
    .footer(SidebarFooter::new().child("User Profile"));
```

---

### Collapsible Sidebar

```rust
let mut collapsed = false;

Sidebar::new(Side::Left)
    .collapsed(collapsed)
    .collapsible(true)
    .header(
        SidebarHeader::new()
            .child(
                h_flex()
                    .child(Icon::new(IconName::Home))
                    .when(!collapsed, |this| this.child("Home"))
            )
    )
    .child(
        SidebarGroup::new("Menu")
            .child(
                SidebarMenu::new()
                    .child(SidebarMenuItem::new("Files").icon(IconName::Folder))
            )
    );

SidebarToggleButton::left()
    .collapsed(collapsed)
    .on_click(|_, _, _| collapsed = !collapsed);
```

---

### Nested Menu Items

```rust
SidebarMenuItem::new("Projects")
    .icon(IconName::FolderOpen)
    .active(true)
    .children([
        SidebarMenuItem::new("Web App").active(false),
        SidebarMenuItem::new("Mobile App").active(true),
        SidebarMenuItem::new("Desktop App")
    ]);
```

---

### Multiple Groups

```rust
Sidebar::new(Side::Left)
    .child(SidebarGroup::new("Main")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("Dashboard").icon(IconName::Home))
            .child(SidebarMenuItem::new("Analytics").icon(IconName::BarChart))
        )
    )
    .child(SidebarGroup::new("Content")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("Posts").icon(IconName::FileText))
            .child(SidebarMenuItem::new("Media").icon(IconName::Image))
            .child(SidebarMenuItem::new("Comments").icon(IconName::MessageCircle))
        )
    )
    .child(SidebarGroup::new("Settings")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("General").icon(IconName::Settings))
            .child(SidebarMenuItem::new("Users").icon(IconName::Users))
        )
    );
```

---

### With Badges and Suffixes

```rust
SidebarMenuItem::new("Notifications")
    .icon(IconName::Bell)
    .suffix(Badge::new().count(5));

SidebarMenuItem::new("Dark Mode")
    .icon(IconName::Moon)
    .suffix(Switch::new("dark-mode").checked(true).xsmall());

SidebarMenuItem::new("Settings")
    .icon(IconName::Settings)
    .suffix(IconName::ChevronRight);
```

---

### Right-Side Placement

```rust
Sidebar::new(Side::Right)
    .width(300)
    .header(SidebarHeader::new().child("Right Panel"))
    .child(SidebarGroup::new("Tools")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("Inspector").icon(IconName::Search))
            .child(SidebarMenuItem::new("Console").icon(IconName::Terminal))
        )
    );
```

---

### Custom Width and Styling

```rust
Sidebar::new(Side::Left)
    .width(280)
    .border_width(2)
    .header(
        SidebarHeader::new()
            .p_4()
            .rounded(cx.theme().radius)
            .child("Custom Styled Sidebar")
    );
```

---

### Interactive Header with Popup Menu

```rust
SidebarHeader::new()
    .child(
        h_flex().gap_2()
            .child(Icon::new(IconName::Building))
            .child("Company Name")
            .child(Icon::new(IconName::ChevronsUpDown))
    )
    .dropdown_menu(|menu, _, _| {
        menu.menu("Acme Corp", Box::new(SelectCompany("acme")))
            .menu("Tech Solutions", Box::new(SelectCompany("tech")))
            .separator()
            .menu("Switch Organization", Box::new(SwitchOrg))
    });
```

---

### Footer with User Information

```rust
SidebarFooter::new()
    .justify_between()
    .child(
        h_flex().gap_2()
            .child(Icon::new(IconName::User))
            .when(!collapsed, |this| {
                this.child(
                    v_flex()
                        .child("John Doe")
                        .child(div().text_xs().text_color(cx.theme().muted_foreground).child("john@example.com"))
                )
            })
    )
    .when(!collapsed, |this| this.child(Icon::new(IconName::MoreHorizontal)));
```

---

### Responsive Sidebar

```rust
let is_mobile = window_width < 768;

Sidebar::new(Side::Left)
    .collapsed(is_mobile || manually_collapsed)
    .width(if is_mobile { 60 } else { 240 })
    .header(
        SidebarHeader::new()
            .child(
                div()
                    .when(!is_mobile, |this| this.child("Full App Name"))
                    .when(is_mobile, |this| this.child(Icon::new(IconName::Menu)))
            )
    );
```

---

### Theming

```rust
cx.theme().sidebar                    // Background
cx.theme().sidebar_foreground         // Text
cx.theme().sidebar_border             // Border
cx.theme().sidebar_accent             // Hover/active background
cx.theme().sidebar_accent_foreground  // Hover/active text
cx.theme().sidebar_primary            // Primary elements
cx.theme().sidebar_primary_foreground // Primary text
```

---

### Examples

**File Explorer Sidebar**

```rust
Sidebar::new(Side::Left)
    .header(SidebarHeader::new()
        .child(h_flex().gap_2()
            .child(IconName::Folder)
            .child("Explorer")
        )
    )
    .child(SidebarGroup::new("Folders")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("src").icon(IconName::FolderOpen).active(true)
                .children([
                    SidebarMenuItem::new("components").icon(IconName::Folder),
                    SidebarMenuItem::new("utils").icon(IconName::Folder),
                    SidebarMenuItem::new("main.rs").icon(IconName::FileCode).active(true)
                ])
            )
            .child(SidebarMenuItem::new("tests").icon(IconName::Folder))
            .child(SidebarMenuItem::new("Cargo.toml").icon(IconName::FileText))
        )
    );
```

**Admin Dashboard Sidebar**

```rust
Sidebar::new(Side::Left)
    .header(SidebarHeader::new()
        .child(h_flex().gap_2()
            .child(div().size_8().rounded_full().bg(cx.theme().primary).child(Icon::new(IconName::Crown)))
            .child("Admin Panel")
        )
    )
    .child(SidebarGroup::new("Overview")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("Dashboard").icon(IconName::LayoutDashboard).active(true))
            .child(SidebarMenuItem::new("Analytics").icon(IconName::TrendingUp).suffix(Badge::new().count(2)))
        )
    )
    .child(SidebarGroup::new("Management")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("Users").icon(IconName::Users).suffix("1,234"))
            .child(SidebarMenuItem::new("Orders").icon(IconName::ShoppingCart).suffix(Badge::new().dot().variant_destructive()))
            .child(SidebarMenuItem::new("Products").icon(IconName::Package))
        )
    )
    .footer(SidebarFooter::new()
        .child(h_flex().gap_2().child(IconName::User).child("Administrator"))
        .child(IconName::LogOut)
    );
```

**Settings Sidebar**

```rust
Sidebar::new(Side::Left)
    .width(300)
    .header(SidebarHeader::new().child("Settings"))
    .child(SidebarGroup::new("General")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("Appearance").icon(IconName::Palette).active(true))
            .child(SidebarMenuItem::new("Notifications").icon(IconName::Bell).suffix(Switch::new("notifications").checked(true).xsmall()))
            .child(SidebarMenuItem::new("Privacy").icon(IconName::Shield))
        )
    )
    .child(SidebarGroup::new("Advanced")
        .child(SidebarMenu::new()
            .child(SidebarMenuItem::new("Developer").icon(IconName::Code)
                .children([
                    SidebarMenuItem::new("Debug Mode").suffix(Switch::new("debug").checked(false).xsmall()),
                    SidebarMenuItem::new("Console").on_click(|_, _, _| println!("Open console"))
                ])
            )
            .child(SidebarMenuItem::new("Performance").icon(IconName::Zap))
        )
    );
```
