---
trigger: model_decision
description: only use when we need to show file structure
---

Tree Component

Import

```rust
use gpui_component::tree::{tree, TreeState, TreeItem, TreeEntry};
```

Basic Tree

```rust
let tree_state = cx.new(|cx| {
    TreeState::new(cx).items(vec![
        TreeItem::new("src", "src")
            .expanded(true)
            .child(TreeItem::new("src/lib.rs", "lib.rs"))
            .child(TreeItem::new("src/main.rs", "main.rs")),
        TreeItem::new("Cargo.toml", "Cargo.toml"),
        TreeItem::new("README.md", "README.md"),
    ])
});

tree(&tree_state, |ix, entry, selected, window, cx| {
    ListItem::new(ix)
        .child(h_flex().gap_2().child(entry.item().label.clone()))
});
```

File Tree with Icons

```rust
tree(&tree_state, |ix, entry, selected, window, cx| {
    let icon = if !entry.is_folder() { IconName::File }
               else if entry.is_expanded() { IconName::FolderOpen }
               else { IconName::Folder };

    ListItem::new(ix)
        .selected(selected)
        .pl(px(16.) * entry.depth() + px(12.))
        .child(h_flex().gap_2().child(icon).child(entry.item().label.clone()))
        .on_click(cx.listener(move |_, _, _, _| {}))
});
```

Dynamic Tree Loading

```rust
fn build_file_items(path: &Path) -> Vec<TreeItem> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Unknown").to_string();
            if path.is_dir() {
                let children = build_file_items(&path);
                items.push(TreeItem::new(path.to_string_lossy(), name).children(children));
            } else {
                items.push(TreeItem::new(path.to_string_lossy(), name));
            }
        }
    }
    items
}
```

Selection Handling

```rust
struct MyTreeView { tree_state: Entity<TreeState>, selected_item: Option<TreeItem> }

impl MyTreeView {
    fn handle_selection(&mut self, item: TreeItem, cx: &mut Context<Self>) {
        self.selected_item = Some(item.clone());
        println!("Selected: {} ({})", item.label, item.id);
        cx.notify();
    }
}
```

Context Menu

```rust
.on_secondary_mouse_down(MouseButton::Right, {
    let item = entry.item().clone();
    move |_, _, cx| {
        cx.show_context_menu(ContextMenu::build(cx, |menu, cx| {
            menu.action("Rename", Rename).action("Delete", Delete).separator().action("Copy Path", CopyPath)
        }));
    }
})
```

Disabled Items

```rust
TreeItem::new("protected", "Protected Folder")
    .disabled(true)
    .child(TreeItem::new("secret.txt", "secret.txt"))
```

Programmatic Control

```rust
tree_state.update(cx, |state, cx| state.set_selected_index(Some(2), cx)); // Select third item
tree_state.update(cx, |state, _| state.scroll_to_item(5, gpui::ScrollStrategy::Center));
tree_state.update(cx, |state, cx| state.set_selected_index(None, cx)); // Clear
```

Lazy Loading

```rust
if path.is_dir() && !self.loaded_paths.contains(item_id) {
    cx.spawn(async move |cx| {
        let children = load_directory_children(&path).await;
        tree_state.update(cx, |state, cx| state.update_item_children(&item_id, children, cx));
    }).detach();
    self.loaded_paths.insert(item_id.to_string());
}
```

Search and Filter

```rust
fn filter_tree_items(items: &[TreeItem], query: &str) -> Vec<TreeItem> {
    items.iter().filter_map(|item| {
        if item.label.to_lowercase().contains(&query.to_lowercase()) {
            Some(item.clone().expanded(true))
        } else {
            let filtered_children = filter_tree_items(&item.children, query);
            if !filtered_children.is_empty() {
                Some(item.clone().children(filtered_children).expanded(true))
            } else { None }
        }
    }).collect()
}
```

Multi-Select Tree

```rust
struct MultiSelectTree { tree_state: Entity<TreeState>, selected_items: HashSet<String> }

fn toggle_selection(&mut self, item_id: &str, cx: &mut Context<Self>) {
    if self.selected_items.contains(item_id) { self.selected_items.remove(item_id); }
    else { self.selected_items.insert(item_id.to_string()); }
    cx.notify();
}
```

Keyboard Navigation

| Key   | Action                 |
| ----- | ---------------------- |
| ↑     | Previous item          |
| ↓     | Next item              |
| ←     | Collapse or parent     |
| →     | Expand folder          |
| Enter | Toggle expand/collapse |
| Space | Custom action          |
