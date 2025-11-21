---
trigger: model_decision
description: only use when you need to manage theme or coloscheme
---

### GPUI Theme System Overview

GPUI provides a **built-in theming system** that allows all components to adapt to a consistent visual style. The key parts are the **ActiveTheme** trait and the **ThemeRegistry**.

---

#### 1. **ActiveTheme**

* Provides access to the current theme's colors and styles.
* Can be accessed from a component’s **App context (`cx`)**.

Example:

```rust
use gpui_component::{ActiveTheme as _};

// Access colors in your component
let primary_color = cx.theme().primary;
let bg_color = cx.theme().background;
let fg_color = cx.theme().foreground;
```

> This ensures that your components automatically use the theme currently active in the application.

---

#### 2. **ThemeRegistry**

* GPUI ships with **20+ built-in themes** (found in the `themes` folder of the repo).
* `ThemeRegistry` helps **load, switch, and watch themes** dynamically.

Example:

```rust
use std::path::PathBuf;
use gpui::{App, SharedString};
use gpui_component::{Theme, ThemeRegistry};

pub fn init(cx: &mut App) {
    let theme_name = SharedString::from("Ayu Light");

    // Load and watch themes from ./themes directory
    if let Err(err) = ThemeRegistry::watch_dir(PathBuf::from("./themes"), cx, move |cx| {
        if let Some(theme) = ThemeRegistry::global(cx)
            .themes()
            .get(&theme_name)
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&theme);
        }
    }) {
        tracing::error!("Failed to watch themes directory: {}", err);
    }
}
```

* **Dynamic updates**: Changing theme files or selecting a new theme automatically applies it to the app.

---

#### 3. **Best Practices**

* Use `cx.theme()` inside components to fetch colors instead of hardcoding.
* Load themes at application startup and optionally watch the directory for live updates.
* Make your components **theme-aware** for consistency and easy customization.

---

Essentially, GPUI’s theme system lets you **separate styling from layout logic**, making your UI both flexible and visually consistent across the app.
