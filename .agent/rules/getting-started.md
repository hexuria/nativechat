---
trigger: model_decision
description: when getting started with gpui and gpui component use this
---

# **GPUI Component**

## **TL;DR**

A **60+ component**, **cross-platform**, **high-performance** UI kit for **Rust** built on top of **GPUI**.
If you’re building a serious desktop app in Rust, this is the best UI layer available today.

Version: **0.4.1**

---

# **Why it’s worth using**

### **1. Huge Component Library (60+ components)**

You get everything you'd expect from a modern UI framework:

* Buttons, Inputs, Tags, Tooltips
* Tables, Lists (virtualized!)
* Charts (line, bar, area, pie)
* Dock layouts, panels, resizable containers
* Tree views, menus, modals
* Code editor with LSP + syntax highlighting

This saves months of custom UI work.

---

### **2. Serious Performance**

* Virtualized lists & tables → fast even with **100K+ rows**
* Zero-cost abstractions from Rust
* GPU-accelerated rendering from GPUI

This is a desktop-grade UI system, not a toy.

---

### **3. Themes Built-In**

* 20+ themes
* Dark mode out of the box
* Everything is theme-aware

You get a consistent design system without doing anything.

---

### **4. Flexible Layouts**

* Dock systems
* Resizable panels
* Freeform layouts
* Flexbox-style containers

This matches what professional tools like VSCode and Figma use.

---

### **5. Data Visualization**

Charts included, zero setup:

* **Line**
* **Bar**
* **Area**
* **Pie**

For dashboards, analytics, admin tools — immediately usable.

---

### **6. Real Code Editor Inside**

* Tree-sitter
* Rope-based text model
* LSP integration

This is unusually powerful for a Rust UI framework.

---

# **Install**

```toml
gpui = "0.2.2"
gpui-component = "0.4.1"
```

---

# **Minimal App Setup**

This is the only boilerplate you really need:

```rust
gpui_component::init(cx);

cx.open_window(WindowOptions::default(), |window, cx| {
    let view = cx.new(|_| HelloWorld);
    cx.new(|cx| Root::new(view, window, cx))
});
```

---

# **Simple UI Example**

```rust
Button::new("ok")
    .primary()
    .label("Click Me")
    .on_click(|_, _, _| println!("Button clicked!"))
```

GPUI Component is intentionally stateless — you pass data in, render out, and let the framework handle the rest.

---

# **Mid-Level Opinion (the honest part)**

### **Why choose this library?**

Because it’s the only Rust UI system that feels modern, scalable, and production-ready.

### **Who is it best for?**

* POS
* Accounting / dashboard / admin products
* Developer tools
* Multi-pane apps
* Data-heavy clients

### **Who should avoid it?**

* Videogames
* Ultra-custom animated UX
* Electron-style DOM-based projects

