---
trigger: model_decision
description: use this guide if you need to apply shadow
---



```
use gpui::{
    App, Application, Bounds, BoxShadow, Context, Div, SharedString, Window, WindowBounds,
    WindowOptions, div, hsla, point, prelude::*, px, relative, rgb, size,
};

struct Shadow;

impl Shadow {
    fn shape(kind: &str) -> Div {
        let base = div().size_16().bg(rgb(0xffffff)).border_1().border_color(hsla(0.0, 0.0, 0.0, 0.1));
        match kind {
            "circle" => base.rounded_full(),
            "square" => base,
            "rounded_small" => base.rounded(px(4.)),
            "rounded_medium" => base.rounded(px(8.)),
            "rounded_large" => base.rounded(px(12.)),
            _ => base,
        }
    }
}

fn example(label: impl Into<SharedString>, element: impl IntoElement) -> impl IntoElement {
    let label = label.into();
    div()
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .w(relative(1. / 6.))
        .border_r_1()
        .border_color(hsla(0.0, 0.0, 0.0, 1.0))
        .child(div().flex().items_center().justify_center().flex_1().py_12().child(element))
        .child(div().w_full().border_t_1().border_color(hsla(0.0, 0.0, 0.0, 1.0)).p_1().flex().items_center().child(label))
}

// Macro to generate a shadow example with given properties
macro_rules! shadow_example {
    ($label:expr, $shape:expr, $color:expr, $offset_x:expr, $offset_y:expr, $blur:expr, $spread:expr) => {
        example($label, Shadow::shape($shape).shadow(vec![BoxShadow {
            color: $color,
            offset: point(px($offset_x), px($offset_y)),
            blur_radius: px($blur),
            spread_radius: px($spread),
        }]))
    };
}

impl Render for Shadow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = Vec::new();

        // Simple shadow sizes
        let sizes = vec![
            ("None", 0.0),
            ("2X Small", 1.0),
            ("Extra Small", 2.0),
            ("Small", 4.0),
            ("Medium", 8.0),
            ("Large", 12.0),
            ("Extra Large", 16.0),
            ("2X Large", 24.0),
        ];
        let mut simple_row = Vec::new();
        for (label, blur) in sizes {
            simple_row.push(shadow_example!(label, "circle", hsla(0.0, 0.0, 0.0, 0.3), 0.0, 8.0, blur, 0.0));
        }
        rows.push(div().flex().children(simple_row));

        // Multiple colored shadows
        let multi_colors = vec![(0.0, "Red"), (60.0, "Yellow"), (120.0, "Green"), (240.0, "Blue")];
        let mut multi_row = Vec::new();
        for &(hue, label) in &multi_colors {
            multi_row.push(BoxShadow {
                color: hsla(hue / 360., 1.0, 0.5, 0.3),
                offset: point(px((hue / 30.0 - 6.0) * 2.0), px((hue / 30.0 - 6.0) * 2.0)),
                blur_radius: px(8.),
                spread_radius: px(2.),
            });
        }
        rows.push(example("Circle Multiple", Shadow::shape("circle").shadow(multi_row)));

        div().id("shadow-example").overflow_y_scroll().bg(rgb(0xffffff)).size_full().text_xs().child(div().flex().flex_col().w_full().children(rows))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Shadow),
        )
        .unwrap();

        cx.activate(true);
    });
}

```