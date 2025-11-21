---
trigger: model_decision
description: only show when we need to show metrics , analytics e.g. dashboard
---

### Chart Component Overview

**Import:**

```rust
use gpui_component::chart::{LineChart, BarChart, AreaChart, PieChart};
```

---

### Chart Types

**LineChart**

* Connects data points with lines, ideal for trends.

```rust
LineChart::new(data)
    .x(|d| d.x.clone())
    .y(|d| d.y)
```

Variants:

* Curved (default)
* Linear: `.linear()`
* Step: `.step_after()`
* Dots: `.dot()`
* Custom stroke color: `.stroke(cx.theme().success)`

Tick control:

```rust
.tick_margin(2) // every 2nd tick
```

**BarChart**

* Rectangular bars for category comparison.

```rust
BarChart::new(data)
    .x(|d| d.category.clone())
    .y(|d| d.value)
```

Customization:

* `.fill(|d| d.color)` for bar colors
* `.label(|d| format!("{}", d.value))`
* `.tick_margin(n)` for tick spacing

**AreaChart**

* Similar to line chart but fills area below line.

```rust
AreaChart::new(data)
    .x(|d| d.date.clone())
    .y(|d| d.value)
```

* Stacked area charts: chain `.y()` for multiple series with `.stroke()` and `.fill()`
* Gradient fills: `linear_gradient()`
* Interpolation styles: `.linear()` or `.step_after()`

**PieChart**

* Displays data as slices of a circle.

```rust
PieChart::new(data)
    .value(|d| d.amount as f32)
    .outer_radius(100.)
```

* Donut chart: `.inner_radius(60.)`
* Custom colors: `.color(|d| d.color)`
* Slice padding: `.pad_angle(4./100.)`

---

### Data Structures

```rust
#[derive(Clone)]
struct DailyDevice { date: String, desktop: f64, mobile: f64 }

#[derive(Clone)]
struct MonthlyDevice { month: String, desktop: f64, color_alpha: f32 }

#[derive(Clone)]
struct StockPrice { date: String, open: f64, high: f64, low: f64, close: f64, volume: u64 }
```

---

### Chart Configuration

**Container Setup**

```rust
fn chart_container(title: &str, chart: impl IntoElement, center: bool, cx: &mut Context<ChartStory>) -> impl IntoElement { ... }
```

* Adds title, period label, chart, summary, and context
* Flexible styling with `v_flex()` and theme integration

**Theme Colors**

```rust
.stroke(cx.theme().chart_1)
```

* Theme colors: `chart_1` … `chart_5`

---

### Examples

**Sales Dashboard**

* Line, Bar, Pie charts side-by-side
* Custom colors and labels
* Theme-based styling

**Multi-Series Time Chart**

* AreaChart with multiple series
* Gradient fills, tick control

**Financial Chart**

* LineChart for stock price
* BarChart for trading volume
* Conditional fill colors based on thresholds

---

### Customization Options

**Color Schemes**

* Theme colors or custom palette

```rust
.fill(|d| colors[d.category_index % colors.len()])
```

**Responsive Design**

```rust
div().flex_1().min_h(px(300.)).max_h(px(600.)).w_full().child(LineChart::new(data))
```

**Grid & Axis Styling**

* Automatic grid lines, X/Y axis, responsive tick spacing

---

### Performance Tips

**Large Datasets**

```rust
let sampled_data: Vec<_> = data.iter().step_by(5).cloned().collect();
```

**Memory Optimization**

* Efficient access: `.x(|d| d.date.clone()).y(|d| d.value)`

---

### Integration Examples

**State Management**

* Filter data based on time range and chart type
* Render Line, Bar, or Area charts dynamically

**Real-time Updates**

* Add new data points, remove oldest beyond `max_points`
* Render chart continuously with `.linear()` and `.dot()`

