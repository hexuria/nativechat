---
trigger: model_decision
description: only use when we need to show data in tabular form
---

### Table Component Overview

**Imports:**

```rust
use gpui_component::table::{Table, TableState, TableDelegate, Column, ColumnSort, ColumnFixed, TableEvent};
```

---

### Basic Table

1. Implement `TableDelegate` to provide data and columns.
2. Use `TableState` to manage the table.

```rust
struct MyData { id: usize, name: String, age: u32, email: String }

struct MyTableDelegate {
    data: Vec<MyData>,
    columns: Vec<Column>,
}

impl MyTableDelegate {
    fn new() -> Self {
        Self {
            data: vec![
                MyData { id: 1, name: "John".to_string(), age: 30, email: "john@example.com".to_string() },
                MyData { id: 2, name: "Jane".to_string(), age: 25, email: "jane@example.com".to_string() },
            ],
            columns: vec![
                Column::new("id", "ID").width(60.),
                Column::new("name", "Name").width(150.).sortable(),
                Column::new("age", "Age").width(80.).sortable(),
                Column::new("email", "Email").width(200.),
            ],
        }
    }
}

impl TableDelegate for MyTableDelegate {
    fn columns_count(&self, _: &App) -> usize { self.columns.len() }
    fn rows_count(&self, _: &App) -> usize { self.data.len() }
    fn column(&self, col_ix: usize, _: &App) -> &Column { &self.columns[col_ix] }
    fn render_td(&self, row_ix: usize, col_ix: usize, _: &mut Window, _: &mut App) -> impl IntoElement {
        let row = &self.data[row_ix];
        match self.columns[col_ix].key.as_ref() {
            "id" => row.id.to_string(),
            "name" => row.name.clone(),
            "age" => row.age.to_string(),
            "email" => row.email.clone(),
            _ => "".to_string(),
        }
    }
}

// Initialize
let delegate = MyTableDelegate::new();
let state = cx.new(|cx| TableState::new(delegate, window, cx));
```

---

### Column Configuration

```rust
Column::new("name", "Name").sortable().width(150.);
Column::new("price", "Price").text_right().sortable();
Column::new("actions", "Actions").fixed(ColumnFixed::Left).resizable(false).movable(false);
Column::new("status", "Status").width(100.).resizable(false);
Column::new("created", "Created").ascending();
Column::new("modified", "Modified").descending();
```

---

### Virtual Scrolling

Only visible rows are rendered, allowing efficient handling of large datasets.

```rust
fn render_td(&self, row_ix: usize, col_ix: usize, _: &mut Window, _: &mut Context<TableState<Self>>) -> impl IntoElement {
    let row = &self.data[row_ix];
    format_cell_data(row, col_ix)
}
```

---

### Sorting

```rust
fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, _: &mut Window, _: &mut Context<TableState<Self>>) {
    let col = &self.columns[col_ix];
    match col.key.as_ref() {
        "name" => match sort {
            ColumnSort::Ascending => self.data.sort_by(|a,b| a.name.cmp(&b.name)),
            ColumnSort::Descending => self.data.sort_by(|a,b| b.name.cmp(&a.name)),
            ColumnSort::Default => self.data.sort_by(|a,b| a.id.cmp(&b.id)),
        },
        "age" => match sort {
            ColumnSort::Ascending => self.data.sort_by(|a,b| a.age.cmp(&b.age)),
            ColumnSort::Descending => self.data.sort_by(|a,b| b.age.cmp(&a.age)),
            ColumnSort::Default => self.data.sort_by(|a,b| a.id.cmp(&b.id)),
        },
        _ => {}
    }
}
```

---

### Row Selection & Context Menu

```rust
fn render_tr(&self, row_ix: usize, _: &mut Window, cx: &mut App) -> Stateful<Div> {
    div().id(row_ix).on_click(move |ev, _, _| {
        if ev.modifiers().secondary() {
            println!("Right-clicked row {}", row_ix);
        } else {
            println!("Selected row {}", row_ix);
        }
    })
}

fn context_menu(&self, row_ix: usize, menu: PopupMenu, _: &mut Window, _: &mut App) -> PopupMenu {
    let row = &self.data[row_ix];
    menu.menu(format!("Edit {}", row.name), Box::new(EditRowAction(row_ix)))
        .menu("Delete", Box::new(DeleteRowAction(row_ix)))
        .separator()
        .menu("Duplicate", Box::new(DuplicateRowAction(row_ix)))
}
```

---

### Custom Cell Rendering

```rust
match col.key.as_ref() {
    "status" => div().px_2().py_1().rounded(px(4.))
                  .bg(color.opacity(0.1)).text_color(color).child(text),
    "progress" => div().w_full().h(px(8.)).bg(cx.theme().muted)
                       .rounded(px(4.)).child(div().h_full().w(percentage(row.progress))
                       .bg(cx.theme().primary).rounded(px(4.))),
    "actions" => h_flex().gap_1()
                  .child(Button::new(format!("edit-{}", row_ix)).text().icon(IconName::Edit))
                  .child(Button::new(format!("delete-{}", row_ix)).text().icon(IconName::Trash)),
    "avatar" => h_flex().items_center().gap_2()
                  .child(div().w(px(32.)).h(px(32.)).rounded_full().bg(cx.theme().accent)
                  .flex().items_center().justify_center()
                  .child(row.name.chars().next().unwrap_or('?').to_string()))
                  .child(row.name.clone()),
    _ => row.get_field_value(col.key.as_ref()).into_any_element(),
}
```

---

### Column Resizing & Moving

```rust
TableState::new(delegate, window, cx)
    .col_resizable(true)
    .col_movable(true)
    .sortable(true)
    .col_selectable(true)
    .row_selectable(true);
```

Listen for events:

```rust
cx.subscribe_in(&state, window, |view, table, event, _, cx| {
    match event {
        TableEvent::ColumnWidthsChanged(widths) => save_column_widths(widths),
        TableEvent::MoveColumn(from, to) => save_column_order(from, to),
        _ => {}
    }
}).detach();
```

---

### Infinite Loading / Pagination

```rust
fn is_eof(&self, _: &App) -> bool { !self.has_more_data }
fn load_more_threshold(&self) -> usize { 50 }
fn load_more(&mut self, _: &mut Window, cx: &mut Context<TableState<Self>>) { /* async fetch */ }
fn loading(&self, _: &App) -> bool { self.loading }
```

---

### Styling

```rust
Table::new(&state)
    .stripe(true)               // Alternating rows
    .bordered(true)             // Table border
    .scrollbar_visible(true, true);
```

---

### Examples

**Financial Data Table:**

```rust
"price" => div().text_right().child(format!("${:.2}", stock.price)),
"change" => div().text_right().text_color(color).child(format!("{:+.2}", stock.change)),
```

**User Management Table:**

```rust
Column::new("avatar", "").width(50.).resizable(false).movable(false);
Column::new("name", "Name").width(150.).sortable().fixed_left();
Column::new("email", "Email").width(200.).sortable();
Column::new("role", "Role").width(100.).sortable();
Column::new("status", "Status").width(100.);
Column::new("last_login", "Last Login").width(120.).sortable();
Column::new("actions", "Actions").width(100.).resizable(false);
```

---

### Keyboard Shortcuts

* `↑/↓` → Navigate rows
* `←/→` → Navigate columns
* `Enter/Space` → Select row/column
* `Escape` → Clear selection
