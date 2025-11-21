---
trigger: model_decision
description: only use when we need to render an image
---

Image

Import

```rust
use gpui::{img, ImageSource, ObjectFit};
use gpui_component::{v_flex, h_flex, div, Icon, IconName};
```

Basic

```rust
img("https://example.com/image.jpg")
img("assets/logo.png")
img("icons/star.svg")
```

Sizing

```rust
img("https://example.com/photo.jpg").w(px(300.)).h(px(200.))
img("https://example.com/banner.jpg").w(relative(1.)).max_w(px(800.)).h(px(400.))
img("https://example.com/avatar.jpg").size(px(100.))
```

Object fit

```rust
img(src).object_fit(ObjectFit::Cover)
img(src).object_fit(ObjectFit::Contain)
img(src).object_fit(ObjectFit::Fill)
img(src).object_fit(ObjectFit::ScaleDown)
img(src).object_fit(ObjectFit::None)
```

Fallback handling

```rust
fn image_with_fallback(src: &str, alt: &str) -> impl IntoElement {
    div().w(px(300.)).h(px(200.)).bg(cx.theme().surface).border_1().border_color(cx.theme().border).rounded(px(8.)).overflow_hidden()
        .child(img(src).w_full().h_full().object_fit(ObjectFit::Cover))
}

fn image_with_icon_fallback(src: &str) -> impl IntoElement {
    div().size(px(200.)).bg(cx.theme().surface).border_1().border_color(cx.theme().border).rounded(px(8.)).flex().items_center().justify_center()
        .child(img(src).size_full().object_fit(ObjectFit::Cover))
}
```

Loading states

```rust
fn image_with_loading(src: &str, is_loading: bool) -> impl IntoElement {
    div().w(px(400.)).h(px(300.)).rounded(px(8.)).overflow_hidden()
        .map(|this| {
            if is_loading {
                this.bg(cx.theme().muted).flex().items_center().justify_center().child("Loading...")
            } else {
                this.child(img(src).w_full().h_full().object_fit(ObjectFit::Cover))
            }
        })
}

fn progressive_image(src: &str, placeholder_src: &str) -> impl IntoElement {
    div().relative().w(px(400.)).h(px(300.)).rounded(px(8.)).overflow_hidden()
        .child(img(placeholder_src).absolute().inset_0().w_full().h_full().object_fit(ObjectFit::Cover).opacity(0.5))
        .child(img(src).absolute().inset_0().w_full().h_full().object_fit(ObjectFit::Cover))
}
```

Responsive images

```rust
fn responsive_image_grid() -> impl IntoElement {
    div().grid().grid_cols(3).gap_4()
        .child(img("https://example.com/photo1.jpg").w_full().aspect_ratio(1.0).object_fit(ObjectFit::Cover).rounded(px(8.)))
        .child(img("https://example.com/photo2.jpg").w_full().aspect_ratio(1.0).object_fit(ObjectFit::Cover).rounded(px(8.)))
        .child(img("https://example.com/photo3.jpg").w_full().aspect_ratio(1.0).object_fit(ObjectFit::Cover).rounded(px(8.)))
}

fn hero_image() -> impl IntoElement {
    div().relative().w_full().h(px(500.)).rounded(px(12.)).overflow_hidden()
        .child(img("https://example.com/hero-image.jpg").absolute().inset_0().w_full().h_full().object_fit(ObjectFit::Cover))
        .child(div().absolute().inset_0().bg(rgba(0, 0, 0, 0.4)).flex().items_center().justify_center()
            .child(v_flex().items_center().gap_4().child("Hero Title").child("Subtitle text here")) )
}
```

Image gallery

```rust
fn image_gallery(images: Vec<&str>) -> impl IntoElement {
    v_flex().gap_6()
        .child(div().w_full().h(px(400.)).rounded(px(12.)).overflow_hidden().child(img(images[0]).w_full().h_full().object_fit(ObjectFit::Cover)))
        .child(h_flex().gap_3().children(images.iter().map(|src| {
            div().size(px(80.)).rounded(px(6.)).overflow_hidden().border_2().border_color(cx.theme().border).cursor_pointer().hover(|this| this.border_color(cx.theme().primary))
                .child(img(*src).size_full().object_fit(ObjectFit::Cover))
        })))
}
```

SVG images

```rust
img("assets/icons/logo.svg").size(px(64.)).text_color(cx.theme().primary)
img("data:image/svg+xml;base64,...").w(px(32.)).h(px(32.))
img("assets/spinner.svg").size(px(24.)).text_color(cx.theme().primary)
```

API Reference

* `img(source)` — create image element
* ImageSource: URL, local path, SharedUri, or Base64
* Sizing: `w()`, `h()`, `size()`, `w_full()`, `h_full()`, `size_full()`
* ObjectFit: `Cover`, `Contain`, `Fill`, `ScaleDown`, `None`
* Styling: `rounded()`, `border_1()`, `border_color()`, `opacity()`, `shadow_sm()`, `shadow_lg()`

Examples

Product card

```rust
fn product_card(image_src: &str, title: &str, price: &str) -> impl IntoElement {
    v_flex().gap_3().p_4().bg(cx.theme().card).rounded(px(12.)).shadow_sm()
        .child(div().relative().w_full().h(px(200.)).rounded(px(8.)).overflow_hidden().bg(cx.theme().muted)
            .child(img(image_src).w_full().h_full().object_fit(ObjectFit::Cover))
            .child(div().absolute().top_2().right_2().size(px(32.)).bg(rgba(255,255,255,0.9)).rounded_full().flex().items_center().justify_center().cursor_pointer()
                .child(Icon::new(IconName::Heart).size(px(16.))) )
        )
        .child(title)
        .child(price)
}
```

Avatar

```rust
fn custom_avatar(src: &str, name: &str, size: f32) -> impl IntoElement {
    div().size(px(size)).rounded_full().overflow_hidden().border_2().border_color(cx.theme().background).shadow_sm()
        .child(img(src).size_full().object_fit(ObjectFit::Cover))
}
```

Image comparison slider

```rust
fn image_comparison(before_src: &str, after_src: &str) -> impl IntoElement {
    div().relative().w_full().h(px(400.)).rounded(px(12.)).overflow_hidden()
        .child(img(before_src).absolute().inset_0().w_full().h_full().object_fit(ObjectFit::Cover))
        .child(div().absolute().top_0().left_0().w(relative(0.5)).h_full().overflow_hidden().child(img(after_src).w(px(800.)).h_full().object_fit(ObjectFit::Cover)))
        .child(div().absolute().top_0().left(relative(0.5)).w(px(2.)).h_full().bg(cx.theme().primary))
}
```

Error handling

```rust
enum ImageState { Loading, Loaded, Error }

fn robust_image(src: &str, state: ImageState) -> impl IntoElement {
    div().w(px(300.)).h(px(200.)).bg(cx.theme().muted).rounded(px(8.)).border_1().border_color(cx.theme().border).flex().items_center().justify_center()
        .map(|this| match state {
            ImageState::Loading => this.child(v_flex().items_center().gap_2().child(Icon::new(IconName::Loader2).size(px(24.))).child("Loading...")),
            ImageState::Loaded => this.p_0().overflow_hidden().child(img(src).w_full().h_full().object_fit(ObjectFit::Cover)),
            ImageState::Error => this.child(v_flex().items_center().gap_2().child(Icon::new(IconName::ImageOff).size(px(32.)).text_color(cx.theme().muted_foreground)).child("Failed to load image")),
        })
}
```

Best practices

* Optimize images for size and format (WebP/AVIF)
* Provide fallbacks and loading skeletons
* Lazy load off-screen images
* Maintain consistent aspect ratios
* Use proper object-fit and smooth transitions
* Consider zoom for detail images

Implementation notes

* GPUI native image support, automatic caching, cross-platform
* Full SVG support with scaling and theming
* Memory handled automatically, no manual cleanup needed
