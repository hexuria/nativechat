---
trigger: model_decision
description: only use when we need an editor or text editor
---

### Editor Component Overview

**Import:**

```rust
use gpui_component::input::{InputState, Input};
```

---

### Core Features

* Multi-line input with optional auto-resizing
* Syntax highlighting for code
* Line numbers
* Search functionality (Ctrl+F)
* Soft wrap control
* Character counting and validation
* Cursor manipulation and text insertion

---

### Basic Usage

**Simple Multi-line Textarea**

```rust
let textarea = cx.new(|cx|
    InputState::new(window, cx)
        .multi_line()
        .placeholder("Enter your message...")
);

Input::new(&textarea)
```

**Fixed Height**

```rust
.rows(10)   // visible rows
.h(px(320.)) // explicit height
```

**Auto-Resizing**

```rust
.auto_grow(1, 5) // min_rows, max_rows
```

**Default Content**

```rust
.default_value("Hello World!\nMulti-line textarea with default content")
```

---

### Code Editor Mode

```rust
.code_editor("rust")
.line_number(true)
.searchable(true)
.default_value("fn main() {\n println!(\"Hello, world!\");\n}")
.h_full()
```

**Custom Tab Size**

```rust
.tab_size(TabSize { tab_size: 4, hard_tabs: false })
```

---

### Text Features

**Soft Wrap**

* Enabled (default) for prose
* Disabled for horizontal scrolling in code

**Character Counting**

```rust
let char_count = textarea.read(cx).value().len();
```

**Text Manipulation**

```rust
textarea.update(cx, |input, cx| input.insert("text", window, cx));
textarea.update(cx, |input, cx| input.replace("new content", window, cx));
textarea.update(cx, |input, cx| input.set_cursor_position(Position { line: 2, character: 5 }, window, cx));
```

**Validation**

```rust
.validate(|text, _| !text.trim().is_empty() && text.len() <= 1000)
```

**Event Handling**

* `InputEvent::Change` → content changed
* `InputEvent::PressEnter` → Enter or Shift+Enter
* `InputEvent::Focus` / `InputEvent::Blur`

**Disabled State**

```rust
.disabled(true)
.h(px(200.))
```

---

### Custom Styling

```rust
.appearance(false) // remove default input styles
.div()
    .bg(cx.theme().background)
    .border_2()
    .border_color(cx.theme().input)
    .rounded_lg()
    .p_4
```

---

### API Reference

**InputState Methods**

* `multi_line()`, `auto_grow(min, max)`, `code_editor(lang)`, `rows(n)`, `tab_size()`, `searchable()`, `soft_wrap()`, `line_number()`, `cursor_position()`, `set_cursor_position()`, `insert()`, `replace()`

**Input Methods**

* `h(height)`
* `h_full()`

**Position**

* `line` (0-based)
* `character` (0-based)

**TabSize**

* `tab_size` (spaces per tab)
* `hard_tabs` (use real tab characters)

**Keyboard Shortcuts**

* Enter / Shift+Enter → new line
* Tab / Shift+Tab → indent/outdent
* Ctrl/Cmd+A → select all
* Ctrl/Cmd+Z / Ctrl/Cmd+Y → undo/redo
* Ctrl/Cmd+F → search

---

### Example Components

**Comment Box**

* Auto-grow
* Character counter
* Submit button disabled if empty or exceeding limit

**Code Editor with Language Selection**

* Change syntax highlighting dynamically

**Text Editor with Toolbar**

* Supports bold/italic formatting using selection replacement

---

### Performance Notes

* Optimized for large text (up to 200K lines)
* Virtual scrolling
* Efficient syntax highlighting (tree-sitter)
* Minimal re-renders
* Smart auto-grow calculations

---

### Best Practices

1. Use `auto_grow()` for dynamic content (comments, chat)
2. Use `multi_line().rows(n)` for fixed layouts (forms)
3. Use `code_editor()` for syntax-aware editing
4. Validate user content on client side
5. Show character counters for guidance
6. Enable search for long content
7. Enable soft wrap for prose, disable for code
