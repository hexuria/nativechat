---
trigger: model_decision
description: only use when we need a ui for clipboard e.g. commands we need to run or content we wanna copy paste
---

Clipboard

Import

```rust
use gpui_component::clipboard::Clipboard;
```

Basic

```rust
Clipboard::new("my-clipboard")
    .value("Text to copy")
    .on_copied(|value, window, cx| {
        window.push_notification(format!("Copied: {}", value), cx)
    })
```

Dynamic values

```rust
let state = some_state.clone();
Clipboard::new("dynamic-clipboard")
    .value_fn(move |_, cx| {
        state.read(cx).get_current_value()
    })
    .on_copied(|value, window, cx| {
        window.push_notification(format!("Copied: {}", value), cx)
    })
```

Custom content

```rust
use gpui_component::label::Label;

 h_flex()
     .gap_2()
     .child(Label::new("Share URL"))
     .child(Icon::new(IconName::Share))
     .child(
        Clipboard::new("custom-clipboard")
        .value("https://example.com")
     )
```

Input fields

```rust
use gpui_component::input::{InputState, Input};

let url_state = cx.new(|cx| InputState::new(window, cx).default_value("https://github.com"));

Input::new(&url_state)
    .suffix(
        Clipboard::new("url-clipboard")
            .value_fn({
                let state = url_state.clone();
                move |_, cx| state.read(cx).value()
            })
            .on_copied(|value, window, cx| {
                window.push_notification(format!("URL copied: {}", value), cx)
            })
    )
```

Simple text

```rust
Clipboard::new("simple")
    .value("Hello, World!")
```

With feedback

```rust
h_flex()
    .gap_2()
    .child(Label::new("Your API Key:"))
    .child(
        Clipboard::new("feedback")
            .value("sk-1234567890abcdef")
            .on_copied(|_, window, cx| {
                window.push_notification("API key copied to clipboard", cx)
            })
    )
```

Form field integration

```rust
use gpui_component::{
    input::{InputState, Input},
    h_flex, label::Label
};

let api_key = "sk-1234567890abcdef";

h_flex()
    .gap_2()
    .items_center()
    .child(Label::new("API Key:"))
    .child(
        Input::new(&input_state)
            .value(api_key)
            .readonly(true)
            .suffix(
                Clipboard::new("api-key-copy")
                    .value(api_key)
                    .on_copied(|_, window, cx| {
                        window.push_notification("API key copied!", cx)
                    })
            )
    )
```

Dynamic content

```rust
struct AppState {
    current_url: String,
}

let app_state = cx.new(|_| AppState {
    current_url: "https://example.com".to_string()
});

Clipboard::new("current-url")
    .value_fn({
        let state = app_state.clone();
        move |_, cx| {
            SharedString::from(state.read(cx).current_url.clone())
        }
    })
    .on_copied(|url, window, cx| {
        window.push_notification(format!("Shared: {}", url), cx)
    })
```

Data types

* Plain text
* UTF-8
* Cross-platform clipboard
