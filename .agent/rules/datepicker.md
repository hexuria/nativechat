---
trigger: model_decision
description: only use when we need to choose a date
---

### DatePicker Component Overview

**Import:**

```rust
use gpui_component::{
    date_picker::{DatePicker, DatePickerState, DateRangePreset, DatePickerEvent},
    calendar::{Date, Matcher},
};
```

---

### Basic Usage

**Single Date Picker**

```rust
let date_picker = cx.new(|cx| DatePickerState::new(window, cx));
DatePicker::new(&date_picker)
```

**With Initial Date**

```rust
use chrono::Local;

let date_picker = cx.new(|cx| {
    let mut picker = DatePickerState::new(window, cx);
    picker.set_date(Local::now().naive_local().date(), window, cx);
    picker
});
```

**Date Range Picker**

```rust
let range_picker = cx.new(|cx| DatePickerState::range(window, cx));
DatePicker::new(&range_picker).number_of_months(2)
```

**With Initial Range**

```rust
let range_picker = cx.new(|cx| {
    let now = Local::now().naive_local().date();
    let mut picker = DatePickerState::new(window, cx);
    picker.set_date((now, now.checked_add_days(Days::new(7)).unwrap()), window, cx);
    picker
});
DatePicker::new(&range_picker).number_of_months(2)
```

---

### Customization

**Date Format**

```rust
DatePickerState::new(window, cx).date_format("%Y-%m-%d")
```

**Placeholder & Cleanable**

```rust
DatePicker::new(&date_picker)
    .placeholder("Select a date...")
    .cleanable(true)
```

**Sizes**

```rust
DatePicker::new(&date_picker).small();
DatePicker::new(&date_picker); // medium (default)
DatePicker::new(&date_picker).large();
```

**Disabled State**

```rust
DatePicker::new(&date_picker).disabled(true)
```

**Custom Appearance**

```rust
DatePicker::new(&date_picker).appearance(false)
```

---

### Date Restrictions

**Disable Weekends**

```rust
DatePickerState::new(window, cx).disabled_matcher(vec![0, 6])
```

**Disable Specific Range or Interval**

```rust
DatePickerState::new(window, cx).disabled_matcher(Matcher::range(Some(start), Some(end)))
DatePickerState::new(window, cx).disabled_matcher(Matcher::interval(Some(start), Some(end)))
```

**Custom Disabled Dates**

```rust
DatePickerState::new(window, cx)
    .disabled_matcher(Matcher::custom(|date| date.day0() < 5)) // first 5 days of month
```

---

### Preset Ranges

**Single Date Presets**

```rust
DatePicker::new(&date_picker).presets(vec![
    DateRangePreset::single("Yesterday", (Utc::now() - Duration::days(1)).naive_local().date()),
]);
```

**Date Range Presets**

```rust
DatePicker::new(&date_picker)
    .number_of_months(2)
    .presets(vec![
        DateRangePreset::range("Last 7 Days", start_date, end_date),
    ]);
```

**Quarterly Presets**

```rust
DatePicker::new(&date_picker).presets(quarterly_presets)
```

---

### Handling Selection Events

```rust
cx.subscribe(&date_picker, |view, _, event, _| {
    if let DatePickerEvent::Change(date) = event {
        match date {
            Date::Single(Some(d)) => println!("Single date: {}", d),
            Date::Range(Some(start), Some(end)) => println!("Range: {} to {}", start, end),
            Date::Range(Some(start), None) => println!("Range start: {}", start),
            _ => println!("Date cleared"),
        }
    }
});
```

---

### Multiple Months Display

```rust
DatePicker::new(&date_picker).number_of_months(2) // or 3
```

---

### Advanced Examples

**Business Days Only**

```rust
DatePickerState::new(window, cx)
    .disabled_matcher(Matcher::custom(|date| matches!(date.weekday(), Weekday::Sat | Weekday::Sun)));
```

**Date Range with Max Duration**

```rust
cx.subscribe(&picker, |view, state, event, _| {
    if let DatePickerEvent::Change(Date::Range(Some(start), Some(end))) = event {
        if (end - start).num_days() > 30 {
            state.set_date(Date::Range(Some(*start), None), window, cx);
        }
    }
});
```

**Event Date Picker**

```rust
DatePickerState::new(window, cx)
    .date_format("%B %d, %Y")
    .disabled_matcher(Matcher::custom(|date| *date < Local::now().naive_local().date()));
```

**Booking System / Financial Period Selector**

```rust
DatePicker::new(&picker)
    .number_of_months(2)
    .presets(booking_presets)
    .placeholder("Select check-in and check-out dates");
```

This covers **single/range selection, custom formatting, placeholders, cleaning, disabled dates, presets, event handling, multi-month view, and advanced restrictions**.
