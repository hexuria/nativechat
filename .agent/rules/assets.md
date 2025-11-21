---
trigger: model_decision
description: use this if you wanna use icons and assets like svg, images
---

### GPUI Icons & Assets Overview

GPUI separates its icon assets from the core `gpui-component` crate to keep application size minimal. Here's how you can manage and use icons:

---

### 1. **Use Default Bundled Assets**

The `gpui-component-assets` crate provides all default icons.

**Add dependencies in `Cargo.toml`:**

```toml
[dependencies]
gpui-component = "0.4.1"
gpui-component-assets = "0.4.1"
```

**Register assets in your application:**

```rust
use gpui::*;
use gpui_component_assets::Assets;

let app = Application::new().with_assets(Assets);
```

You can now use `IconName` and `Icon` directly.

**Example:**

```rust
use gpui_component::{v_flex, IconName};

v_flex()
    .gap_2()
    .child(IconName::Inbox)
    .child(IconName::Bot)
```

---

### 2. **Build Your Own Assets**

For smaller binaries or custom icons:

1. Download or create SVG files following the `IconName` enum convention.
2. Use the `rust-embed` crate to embed your icons into the binary.
3. Implement `AssetSource` to load your assets.

**Example Asset Source:**

```rust
use gpui::*;
use gpui_component::Root;
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Self::get(path).map(|f| Some(f.data)).ok_or_else(|| anyhow!("Asset not found"))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter().filter_map(|p| p.starts_with(path).then(|| p.into())).collect())
    }
}
```

**Register in GPUI Application:**

```rust
let app = Application::new().with_assets(Assets);
```

---

### 3. **Use Icons in Your Application**

Once assets are registered:

```rust
pub struct Example;

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_2()
            .size_full()
            .items_center()
            .justify_center()
            .text_center()
            .child(IconName::Inbox)
            .child(IconName::Bot)
    }
}
```

---

### 4. **Resources**

* GPUI icons are based on **[Lucide Icons](https://lucide.dev/)** – a lightweight, customizable open-source SVG library.

---

This setup allows you to either include a full set of icons for rapid development or selectively embed only the icons your app needs for a smaller footprint.
