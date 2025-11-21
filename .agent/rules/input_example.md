---
trigger: model_decision
description: use this piece of code as guide when you need more info on using input field
---

```
use std::ops::Range;
use gpui::*;
use unicode_segmentation::*;

actions!(
    text_input,
    [
        Backspace, Delete, Left, Right, SelectLeft, SelectRight, SelectAll,
        Home, End, ShowCharacterPalette, Paste, Cut, Copy, Quit,
    ]
);

struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    fn move_cursor(&mut self, to: Option<usize>, select: bool, cx: &mut Context<Self>) {
        if let Some(pos) = to {
            if select { self.select_to(pos, cx) } else { self.move_to(pos, cx) }
        }
    }

    fn backspace_or_delete(&mut self, delete: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let target = if delete { self.next_boundary(self.cursor_offset()) }
                         else { self.previous_boundary(self.cursor_offset()) };
            self.select_to(target, cx);
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn paste_text(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
            self.replace_text_in_range(None, &text.replace("\n", " "), window, cx);
        }
    }

    fn copy_cut(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            let text = self.content[self.selected_range.clone()].to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            if cut { self.replace_text_in_range(None, "", window, cx) }
        }
    }
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let actions = [
            (Backspace, Self::backspace_or_delete as fn(&mut _, &mut _, &mut _) ),
            (Delete, Self::backspace_or_delete),
            (Left, Self::move_cursor), (Right, Self::move_cursor),
            (SelectLeft, Self::move_cursor), (SelectRight, Self::move_cursor),
            (SelectAll, Self::move_cursor),
            (Home, Self::move_cursor), (End, Self::move_cursor),
            (ShowCharacterPalette, Self::show_character_palette),
            (Paste, Self::paste_text), (Cut, Self::copy_cut), (Copy, Self::copy_cut),
        ];
        let mut div_elem = div().flex().key_context("TextInput").track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .bg(rgb(0xeeeeee)).line_height(px(30.)).text_size(px(24.))
            .child(div().h(px(30.+4.*2.)).w_full().p(px(4.)).bg(white())
                .child(TextElement { input: cx.entity() }));

        for (action, f) in actions {
            div_elem = div_elem.on_action(cx.listener(f));
        }

        div_elem
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
    }
}
```