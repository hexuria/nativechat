---
trigger: model_decision
description: only show when we need to show menu context when we right click
---

### PopupMenu Overview

**Imports:**

```rust
use gpui_component::{
    menu::{PopupMenu, PopupMenuItem, ContextMenuExt, DropdownMenu},
    Button
};
use gpui::{actions, Action};
```

---

### Context Menu

Right-click on an element to show menu:

```rust
div()
    .child("Right click me")
    .context_menu(|menu, window, cx| {
        menu.menu("Copy", Box::new(Copy))
            .menu("Paste", Box::new(Paste))
            .separator()
            .menu("Delete", Box::new(Delete))
    });
```

---

### Dropdown Menu

Attach menu to buttons or triggers:

```rust
Button::new("menu-btn")
    .label("Open Menu")
    .dropdown_menu(|menu, window, cx| {
        menu.menu("New File", Box::new(NewFile))
            .menu("Open File", Box::new(OpenFile))
            .link("Documentation", "https://longbridge.github.io/gpui-component/")
            .separator()
            .item(PopupMenuItem::new("Custom Action")
                .on_click(|window, cx| {
                    println!("Custom Action Clicked!");
                })
            )
            .separator()
            .menu("Exit", Box::new(Exit));
    });
```

---

### Menu Features

**Anchor Position:**

```rust
use gpui::Corner;

Button::new("menu-btn")
    .label("Options")
    .dropdown_menu_with_anchor(Corner::TopRight, |menu, window, cx| {
        menu.menu("Option 1", Box::new(Action1))
            .menu("Option 2", Box::new(Action2))
    });
```

**Icons:**

```rust
menu.menu_with_icon("Search", IconName::Search, Box::new(Search))
    .menu_with_icon("Settings", IconName::Settings, Box::new(OpenSettings));
```

**Checkable Items:**

```rust
menu.menu_with_check("Enable Feature", is_enabled, Box::new(ToggleFeature));
```

**Keyboard Shortcuts:**

```rust
actions!(my_app, [Copy, Paste, Cut]);
cx.bind_keys([KeyBinding::new("ctrl-c", Copy, Some("editor"))]);
menu.action_context(focus_handle)
    .menu("Copy", Box::new(Copy));
```

**Submenus:**

```rust
menu.submenu("File", window, cx, |submenu, window, cx| {
    submenu.menu("New", Box::new(NewFile))
           .menu("Open", Box::new(OpenFile));
});
```

**Disabled Items:**

```rust
menu.menu_with_disabled("Disabled Action", Box::new(Action2), true);
```

**Separators and Labels:**

```rust
menu.label("File Operations")
    .menu("New", Box::new(NewFile))
    .separator()
    .label("Edit Operations")
    .menu("Copy", Box::new(Copy));
```

**External Links:**

```rust
menu.link("Documentation", "https://docs.example.com")
    .link_with_icon("GitHub", IconName::GitHub, "https://github.com/example/repo");
```

**Custom Menu Elements:**

```rust
menu.menu_element(Box::new(CustomAction), |window, cx| {
    v_flex().child("Custom Element");
})
.menu_element_with_icon(IconName::Info, Box::new(InfoAction), |window, cx| {
    h_flex().gap_1().child("Status").child("✓ Connected")
});
```

**Scrollable Menus:**

```rust
Button::new("large-menu")
    .label("Many Options")
    .dropdown_menu(|menu, window, cx| {
        menu.scrollable(true).max_h(px(300.));
        for i in 0..100 {
            menu.menu(format!("Option {}", i), Box::new(SelectOption(i)));
        }
        menu
    });
```

**Sizing:**

```rust
menu.min_w(px(200))
    .max_w(px(400))
    .max_h(px(300))
    .scrollable(true);
```

**Action Context:**

```rust
menu.action_context(focus_handle)
    .menu("Copy", Box::new(Copy))
    .menu("Paste", Box::new(Paste));
```

---

### Keyboard Navigation

| Key               | Action                 |
| ----------------- | ---------------------- |
| `↑` / `↓`         | Navigate items         |
| `←` / `→`         | Navigate submenus      |
| `Enter` / `Space` | Activate item          |
| `Escape`          | Close menu             |
| `Tab`             | Close menu, focus next |

---

### Best Practices

* Group related items with separators
* Use consistent icons
* Order frequent actions at top
* Provide keyboard shortcuts
* Show context-relevant items only
* Use submenus for complex hierarchies
* Label clearly
* Scroll menus when exceeding ~10–15 items
