---
trigger: model_decision
description: When we need to use a webview use this
---

# **GPUI WebView**

## **Enable Feature**

```toml
gpui-component = { version = "0.4.1", features = ["webview"] }
```

## **Imports**

```rust
use gpui_component::webview::WebView;
use gpui_component::wry;
```

## **Platforms**

* Windows: WebView2
* macOS: WKWebView
* Linux: WebKitGTK
* iOS/Android: native webviews

## **Create WebView**

```rust
let webview = cx.new(|cx| {
    let builder = wry::WebViewBuilder::new();
    // platform-specific build…
    WebView::new(webview, window, cx)
});
```

## **Basic Ops**

```rust
webview.update(cx, |v, _| v.load_url("https://example.com"));
webview.update(cx, |v, _| v.show());
webview.update(cx, |v, _| v.hide());
let visible = webview.read(cx).visible();
```

## **Navigation**

```rust
webview.update(cx, |v, _| v.back().unwrap());
webview.update(cx, |v, _| v.evaluate_script("location.href='https://x.com'"));
```

## **JavaScript**

```rust
webview.update(cx, |v, _| v.evaluate_script("console.log('hi')"));
```

## **Load HTML**

```rust
webview.update(cx, |v, _| {
    v.load_html("<html>...</html>").unwrap();
});
```

## **Custom Config**

```rust
let webview = wry::WebViewBuilder::new()
    .with_url("https://example.com")
    .with_user_agent("MyApp/1.0")
    .with_devtools(true)
    .build_as_child(&handle)
    .unwrap();
```

## **Use in UI**

Browser-style:

```rust
webview.update(cx, |v, _| v.load_url(&url));
```

## **WRY Access**

```rust
webview.evaluate_script("console.log('test')");
webview.set_bounds(rect).unwrap();
```

## **Platform Notes**

* Windows: needs WebView2 runtime
* macOS: WKWebView
* Linux: needs WebKitGTK + GTK init

Linux init:

```rust
gtk::init().unwrap();
```



