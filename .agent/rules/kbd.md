---
trigger: model_decision
description: only use this when we need to show the user set of keyboard keys on the ui to press
---

Kbd Component

Import

```rust
use gpui_component::kbd::Kbd;
use gpui::Keystroke;
```

### Basic Usage

```rust
let kbd = Kbd::new(Keystroke::parse("cmd-shift-p").unwrap());
let kbd2: Kbd = Keystroke::parse("escape").unwrap().into();
```

### Common Shortcuts

```rust
Kbd::new(Keystroke::parse("cmd-t").unwrap());      // New tab
Kbd::new(Keystroke::parse("cmd--").unwrap());      // Zoom out
Kbd::new(Keystroke::parse("cmd-+").unwrap());      // Zoom in
Kbd::new(Keystroke::parse("escape").unwrap());
Kbd::new(Keystroke::parse("enter").unwrap());
Kbd::new(Keystroke::parse("backspace").unwrap());
```

### Multiple Modifiers

```rust
Kbd::new(Keystroke::parse("cmd-ctrl-shift-a").unwrap());
Kbd::new(Keystroke::parse("cmd-alt-backspace").unwrap());
Kbd::new(Keystroke::parse("ctrl-alt-shift-a").unwrap());
```

### Arrow & Function Keys

```rust
Kbd::new(Keystroke::parse("left").unwrap());
Kbd::new(Keystroke::parse("right").unwrap());
Kbd::new(Keystroke::parse("up").unwrap());
Kbd::new(Keystroke::parse("down").unwrap());

Kbd::new(Keystroke::parse("f12").unwrap());
Kbd::new(Keystroke::parse("secondary-f12").unwrap());

Kbd::new(Keystroke::parse("pageup").unwrap());
Kbd::new(Keystroke::parse("pagedown").unwrap());
```

### Without Visual Styling

```rust
Kbd::new(Keystroke::parse("cmd-s").unwrap()).appearance(false);
```

### From Action Bindings

```rust
if let Some(kbd) = Kbd::binding_for_action(&MyAction {}, None, window) { }
if let Some(kbd) = Kbd::binding_for_action(&MyAction {}, Some("Editor"), window) { }
if let Some(kbd) = Kbd::binding_for_action_in(&MyAction {}, &focus_handle, window) { }
```

### Platform Differences

**macOS:** symbols ⌃ ⌥ ⇧ ⌘, no separators, order: Control, Option, Shift, Command, special keys: ⌫ ⎋ ⏎ ← → ↑ ↓

**Windows/Linux:** text labels Ctrl, Alt, Shift, Win, plus (+) separators, order: Ctrl, Alt, Shift, Win, special keys: Backspace, Esc, Enter, Left, Right, Up, Down

| Input               | macOS | Windows/Linux     |
| ------------------- | ----- | ----------------- |
| `cmd-a`             | ⌘A    | Win+A             |
| `ctrl-shift-a`      | ⌃⇧A   | Ctrl+Shift+A      |
| `cmd-alt-backspace` | ⌥⌘⌫   | Win+Alt+Backspace |
| `escape`            | ⎋     | Esc               |
| `enter`             | ⏎     | Enter             |
| `left`              | ←     | Left              |

### Examples

**Shortcut Help**

```rust
v_flex()
    .gap_2()
    .child(h_flex().gap_2().items_center().child("Open command palette:").child(Kbd::new(Keystroke::parse("cmd-shift-p").unwrap())))
    .child(h_flex().gap_2().items_center().child("Save file:").child(Kbd::new(Keystroke::parse("cmd-s").unwrap())))
    .child(h_flex().gap_2().items_center().child("Find in files:").child(Kbd::new(Keystroke::parse("cmd-shift-f").unwrap())));
```

**Menu Item**

```rust
h_flex()
    .justify_between()
    .items_center()
    .child("New File")
    .child(Kbd::new(Keystroke::parse("cmd-n").unwrap()));
```

**Inline Documentation**

```rust
div()
    .child("Press ")
    .child(Kbd::new(Keystroke::parse("escape").unwrap()))
    .child(" to cancel or ")
    .child(Kbd::new(Keystroke::parse("enter").unwrap()))
    .child(" to confirm.");
```

**Custom Styling**

```rust
Kbd::new(Keystroke::parse("cmd-k").unwrap())
    .text_color(cx.theme().accent)
    .border_color(cx.theme().accent)
    .bg(cx.theme().accent.opacity(0.1));
```

**Text-Only Format**

```rust
let shortcut_text = Kbd::format(&Keystroke::parse("cmd-shift-p").unwrap());
div().child(format!("Shortcut: {}", shortcut_text));
```

### Styling Defaults

* Border: theme border color
* Text: muted foreground
* Background: theme background
* Small rounded corners
* Centered text
* Extra small font
* Padding: 0.5px vertical, 1px horizontal
* Min width: 5 units
* Flex shrink disabled

All styles can be customized via `Styled` trait methods.
