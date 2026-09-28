//! The composer's own text field: a document of text and chips, where a chip is one object with
//! its own icon rather than letters with a fill painted behind them (#40, option B).
//!
//! Three plain modules do the work and are tested without a window: [`model`] (the document),
//! [`edit`] (every edit, the selection and undo) and [`layout`] (lines, chips as boxes with real
//! width, hit testing). This file is only the GPUI side: keys and the platform's text input
//! (IME, dictation, the emoji palette) turned into those edits, and the element that paints the
//! layout. It is modelled on gpui-pre's own `examples/input.rs`.

pub mod edit;
pub mod layout;
pub mod model;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::actions::{SelectDown, SelectLeft, SelectRight, SelectUp};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::input::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, DeleteToEndOfLine, DeleteToNextWordEnd,
    DeleteToPreviousWordStart, Enter, InputEvent, MoveDown, MoveEnd, MoveHome, MoveLeft, MoveRight,
    MoveToEnd, MoveToNextWord, MoveToPreviousWord, MoveToStart, MoveUp, Paste, Redo, SelectAll,
    SelectToEnd, SelectToEndOfLine, SelectToNextWordEnd, SelectToPreviousWordStart, SelectToStart,
    SelectToStartOfLine, ShowCharacterPalette, Undo,
};
use gpui_kit::*;

use crate::components::chat_input::TokenKind;
use edit::Editor;
use layout::{ChipMetrics, Layout, Measure, Piece};
use model::{Chip, Doc, PlainChip};

const CONTEXT: &str = "ComposerEditor";

/// The keys the field answers to: the ones gpui-base binds for its own text field, bound to the
/// same public actions, so the composer's keys are what they were before it had its own field.
pub fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("shift-backspace", Backspace, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("shift-delete", Delete, c),
        KeyBinding::new("cmd-backspace", DeleteToBeginningOfLine, c),
        KeyBinding::new("cmd-delete", DeleteToEndOfLine, c),
        KeyBinding::new("alt-backspace", DeleteToPreviousWordStart, c),
        KeyBinding::new("ctrl-backspace", DeleteToPreviousWordStart, c),
        KeyBinding::new("alt-delete", DeleteToNextWordEnd, c),
        KeyBinding::new(
            "enter",
            Enter {
                secondary: false,
                shift: false,
            },
            c,
        ),
        KeyBinding::new(
            "shift-enter",
            Enter {
                secondary: false,
                shift: true,
            },
            c,
        ),
        KeyBinding::new(
            "secondary-enter",
            Enter {
                secondary: true,
                shift: false,
            },
            c,
        ),
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("left", MoveLeft, c),
        KeyBinding::new("right", MoveRight, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("home", MoveHome, c),
        KeyBinding::new("end", MoveEnd, c),
        KeyBinding::new("cmd-left", MoveHome, c),
        KeyBinding::new("cmd-right", MoveEnd, c),
        KeyBinding::new("ctrl-a", MoveHome, c),
        KeyBinding::new("ctrl-e", MoveEnd, c),
        KeyBinding::new("cmd-up", MoveToStart, c),
        KeyBinding::new("cmd-down", MoveToEnd, c),
        KeyBinding::new("alt-left", MoveToPreviousWord, c),
        KeyBinding::new("alt-right", MoveToNextWord, c),
        KeyBinding::new("shift-home", SelectToStartOfLine, c),
        KeyBinding::new("shift-end", SelectToEndOfLine, c),
        KeyBinding::new("shift-cmd-left", SelectToStartOfLine, c),
        KeyBinding::new("shift-cmd-right", SelectToEndOfLine, c),
        KeyBinding::new("cmd-shift-up", SelectToStart, c),
        KeyBinding::new("cmd-shift-down", SelectToEnd, c),
        KeyBinding::new("alt-shift-left", SelectToPreviousWordStart, c),
        KeyBinding::new("alt-shift-right", SelectToNextWordEnd, c),
        KeyBinding::new("cmd-a", SelectAll, c),
        KeyBinding::new("cmd-c", Copy, c),
        KeyBinding::new("cmd-x", Cut, c),
        KeyBinding::new("cmd-v", Paste, c),
        KeyBinding::new("cmd-z", Undo, c),
        KeyBinding::new("cmd-shift-z", Redo, c),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, c),
    ]);
}

/// The icon a chip wears, from the app's own icon set.
pub fn chip_icon(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Tool => "icons/wrench.svg",
        TokenKind::Recipe => "icons/play.svg",
        TokenKind::Workflow => "icons/branch.svg",
        TokenKind::Skill => "icons/study.svg",
    }
}

/// Widths in the field's font, from the window's text system, remembered per string: the same
/// words are measured again on every frame and every hit test.
#[derive(Clone)]
struct Widths {
    text_system: Arc<WindowTextSystem>,
    font: Font,
    size: Pixels,
    cache: Arc<RefCell<HashMap<String, f32>>>,
}

impl Measure for Widths {
    fn width(&self, text: &str) -> f32 {
        if text.is_empty() {
            return 0.;
        }
        if let Some(width) = self.cache.borrow().get(text) {
            return *width;
        }
        let run = TextRun {
            len: text.len(),
            font: self.font.clone(),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = f32::from(
            self.text_system
                .shape_line(text.to_string().into(), self.size, &[run], None)
                .width,
        );
        let mut cache = self.cache.borrow_mut();
        // A long session types many drafts; the cache is for the words on screen, not all of them.
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(text.to_string(), width);
        width
    }
}

pub struct ComposerEditor {
    focus_handle: FocusHandle,
    editor: Editor,
    placeholder: SharedString,
    min_rows: usize,
    max_rows: usize,
    /// Plain Enter sends (the composer's default). With it off, as when the person chose ⌘↵ to
    /// send, plain Enter is a new line, the way gpui-base's field treats the same setting.
    submit_on_enter: bool,
    /// What the last paint laid out and where: hit tests and the platform's questions about
    /// where text is are answered from it once it is brought up to date with [`Self::fresh`].
    last: Option<Laid>,
    /// Widths in the field's font, kept across frames: the same words are measured on every
    /// frame and every hit test, and a cache that lived one layout saved nothing.
    widths: Option<Widths>,
    /// Counts edits, so a layout knows whether it was made from the document as it is now.
    revision: u64,
    /// Bring the caret into view on the next paint: set by an edit or a caret move, and only
    /// then, so a draft scrolled by the wheel stays where it was put.
    autoscroll: bool,
    /// The last change was an undo or redo, which can bring back a chip whose recipe or skill
    /// has since gone; the composer asks with [`Self::take_restored`].
    restored: bool,
    /// How far the text is scrolled up once it is taller than the field.
    scroll: f32,
    /// The column Up and Down keep, set by the first of a run of them.
    goal_x: Option<f32>,
    selecting: bool,
    /// Whether the blinking caret is in its lit half.
    caret_on: bool,
    /// Held lit until this many blink ticks have passed, so the caret does not vanish while the
    /// person is typing or moving it.
    caret_hold: u8,
    _blink: Task<()>,
}

/// A layout and what it was made with, so it can be made again for a newer document.
#[derive(Clone)]
struct Laid {
    layout: Layout,
    revision: u64,
    bounds: Bounds<Pixels>,
    chip: ChipMetrics,
}

impl EventEmitter<InputEvent> for ComposerEditor {}

/// How long each half of the caret's blink lasts, as in gpui-base's field.
const BLINK: std::time::Duration = std::time::Duration::from_millis(500);

impl Focusable for ComposerEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ComposerEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // The caret blinks while the field has the focus, and the timer asks for nothing
        // otherwise: an idle composer draws no frames of its own.
        let blink = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK).await;
                let alive = this.update_in(cx, |this, window, cx| {
                    // Unfocused, or the window in the background: nothing blinks and nothing is
                    // drawn, so an idle composer costs a timer and no frames.
                    if !this.focus_handle.is_focused(window) || !window.is_window_active() {
                        this.caret_on = true;
                        return;
                    }
                    if this.caret_hold > 0 {
                        this.caret_hold -= 1;
                        this.caret_on = true;
                    } else {
                        this.caret_on = !this.caret_on;
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            focus_handle: cx.focus_handle(),
            editor: Editor::default(),
            placeholder: SharedString::default(),
            min_rows: 1,
            max_rows: 20,
            submit_on_enter: true,
            last: None,
            widths: None,
            revision: 0,
            autoscroll: true,
            restored: false,
            scroll: 0.,
            goal_x: None,
            selecting: false,
            caret_on: true,
            caret_hold: 0,
            _blink: blink,
        }
    }

    /// Keep the caret lit for a moment: it is being typed at or moved.
    fn wake_caret(&mut self) {
        self.caret_on = true;
        self.caret_hold = 1;
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn auto_grow(mut self, min_rows: usize, max_rows: usize) -> Self {
        self.min_rows = min_rows.max(1);
        self.max_rows = max_rows.max(self.min_rows);
        self
    }

    pub fn set_submit_on_enter(&mut self, submit: bool, cx: &mut Context<Self>) {
        self.submit_on_enter = submit;
        cx.notify();
    }

    // The surface the composer uses. Offsets here are in the plain text, the text as it is sent:
    // the composer's triggers, its caret memory and its tokens all speak plain text.

    /// The draft as it is sent: each chip reads as its label.
    pub fn value(&self) -> String {
        self.editor.doc.plain()
    }

    pub fn is_empty(&self) -> bool {
        self.editor.doc.is_empty()
    }

    /// Replace the whole draft. Like the old field's `set_value`, it clears the undo history
    /// and emits no Change: the composer, which called it, already knows.
    pub fn set_value(&mut self, value: impl Into<String>, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.set_doc(Doc::from_plain(&value.into()));
        self.revision += 1;
        self.goal_x = None;
        self.autoscroll = true;
        self.wake_caret();
        cx.notify();
    }

    /// Whether the last change was an undo or redo, clearing the answer.
    pub fn take_restored(&mut self) -> bool {
        std::mem::take(&mut self.restored)
    }

    /// Turn a chip back into its words, as when what it stood for is no longer on the draft.
    pub fn unchip(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.editor.unchip(index) {
            self.changed(cx);
        }
    }

    pub fn cursor(&self) -> usize {
        self.editor.doc.buf_to_plain(self.editor.head())
    }

    pub fn selected_range(&self) -> Range<usize> {
        let range = &self.editor.selection.range;
        self.editor.doc.buf_to_plain(range.start)..self.editor.doc.buf_to_plain(range.end)
    }

    pub fn set_selected_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let doc = &self.editor.doc;
        let (start, end) = (doc.plain_to_buf(range.start), doc.plain_to_buf(range.end));
        self.editor.move_to(start);
        self.editor.select_to(end);
        cx.notify();
    }

    /// Put text at the caret, over any selection, as dictation and a picked row's words do.
    pub fn insert(&mut self, text: impl Into<String>, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.type_text(&text.into());
        self.changed(cx);
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    /// Put a chip at the caret. Returns its index among the chips.
    pub fn insert_chip(
        &mut self,
        kind: TokenKind,
        id: impl Into<String>,
        label: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> usize {
        let index = self.editor.insert_chip(Chip {
            kind,
            id: id.into(),
            label: label.into(),
        });
        self.changed(cx);
        index
    }

    /// Turn words already in the draft (a plain range) into a chip standing for them.
    pub fn chip_over(
        &mut self,
        range: Range<usize>,
        kind: TokenKind,
        id: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> usize {
        let doc = &self.editor.doc;
        let (start, end) = (doc.plain_to_buf(range.start), doc.plain_to_buf(range.end));
        let label = doc.plain_of(start..end);
        self.editor.move_to(start);
        self.editor.select_to(end);
        let index = self.editor.chip_over_selection(Chip {
            kind,
            id: id.into(),
            label,
        });
        self.changed(cx);
        index
    }

    /// Take a chip out, as picking a second recipe takes out the first.
    pub fn remove_chip(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let removed = self.editor.remove_chip(index);
        if removed {
            self.changed(cx);
        }
        removed
    }

    pub fn chips(&self) -> &[Chip] {
        self.editor.doc.chips()
    }

    /// Each chip and where its label sits in [`Self::value`].
    pub fn plain_chips(&self) -> Vec<PlainChip> {
        self.editor.doc.plain_chips()
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.revision += 1;
        self.autoscroll = true;
        self.wake_caret();
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    fn moved(&mut self, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.autoscroll = true;
        self.wake_caret();
        cx.notify();
    }

    // Keys.

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.editor.backspace() {
            self.changed(cx);
        }
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.editor.delete() {
            self.changed(cx);
        }
    }

    fn delete_word_back(
        &mut self,
        _: &DeleteToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.delete_word_back() {
            self.changed(cx);
        }
    }

    fn delete_word_forward(
        &mut self,
        _: &DeleteToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.delete_word_forward() {
            self.changed(cx);
        }
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToBeginningOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.visual_line_edge(false, window);
        if self.editor.delete_to(to) {
            self.changed(cx);
        }
    }

    fn delete_to_line_end(
        &mut self,
        _: &DeleteToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.visual_line_edge(true, window);
        if self.editor.delete_to(to) {
            self.changed(cx);
        }
    }

    fn enter(&mut self, action: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        // Shift-Enter is a new line in the draft, and so is plain Enter when Enter does not
        // send; the composer decides what a sending Enter and ⌘Enter do, as it did for the old
        // field.
        if !action.secondary && (action.shift || !self.submit_on_enter) {
            self.editor.type_text("\n");
            self.changed(cx);
        } else {
            // An Enter that is not a new line goes on to the composer's own bindings, as the old
            // field let it: that is how ⌘↵ reaches `SendDraft` (review of #133).
            cx.propagate();
        }
        cx.emit(InputEvent::PressEnter {
            secondary: action.secondary,
            shift: action.shift,
        });
    }

    fn left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.left();
        self.moved(cx);
    }

    fn right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.right();
        self.moved(cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.editor.doc.prev_boundary(self.editor.head());
        self.editor.select_to(to);
        self.moved(cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.editor.doc.next_boundary(self.editor.head());
        self.editor.select_to(to);
        self.moved(cx);
    }

    fn vertical_target(&mut self, down: bool, window: &Window) -> usize {
        let head = self.editor.head();
        let Some((layout, _, widths)) = self.fresh(window) else {
            return if down { self.editor.doc.len() } else { 0 };
        };
        let doc = &self.editor.doc;
        let goal = *self
            .goal_x
            .get_or_insert_with(|| layout.caret(doc, &widths, head).0);
        layout
            .vertical(doc, &widths, head, goal, down)
            .unwrap_or(if down { doc.len() } else { 0 })
    }

    fn up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        let goal = self.goal_x;
        let to = self.vertical_target(false, window);
        self.editor.move_to(to);
        self.autoscroll = true;
        cx.notify();
        self.goal_x = self.goal_x.or(goal);
    }

    fn down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.vertical_target(true, window);
        self.editor.move_to(to);
        self.autoscroll = true;
        cx.notify();
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.vertical_target(false, window);
        self.editor.select_to(to);
        self.autoscroll = true;
        cx.notify();
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.vertical_target(true, window);
        self.editor.select_to(to);
        self.autoscroll = true;
        cx.notify();
    }

    /// The start or end of the line the caret is on as it is drawn, so Home and End go where the
    /// eye says the line ends even when it wrapped.
    fn visual_line_edge(&mut self, end: bool, window: &Window) -> usize {
        let head = self.editor.head();
        let fresh = self.fresh(window);
        let doc = &self.editor.doc;
        let Some((layout, _, _)) = fresh else {
            return if end {
                doc.line_end(head)
            } else {
                doc.line_start(head)
            };
        };
        let row = layout.line_of(head);
        let line = &layout.lines[row];
        if !end {
            return line.range.start;
        }
        let wrapped = layout
            .lines
            .get(row + 1)
            .is_some_and(|next| next.range.start == line.range.end);
        if wrapped {
            layout.wrap_end(doc, row)
        } else {
            line.range.end
        }
    }

    fn home(&mut self, _: &MoveHome, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.visual_line_edge(false, window);
        self.editor.move_to(to);
        self.moved(cx);
    }

    fn end(&mut self, _: &MoveEnd, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.visual_line_edge(true, window);
        self.editor.move_to(to);
        self.moved(cx);
    }

    fn select_to_line_start(
        &mut self,
        _: &SelectToStartOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.visual_line_edge(false, window);
        self.editor.select_to(to);
        self.moved(cx);
    }

    fn select_to_line_end(
        &mut self,
        _: &SelectToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.visual_line_edge(true, window);
        self.editor.select_to(to);
        self.moved(cx);
    }

    fn move_to_start(&mut self, _: &MoveToStart, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.move_to(0);
        self.moved(cx);
    }

    fn move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        let end = self.editor.doc.len();
        self.editor.move_to(end);
        self.moved(cx);
    }

    fn select_to_start(&mut self, _: &SelectToStart, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.select_to(0);
        self.moved(cx);
    }

    fn select_to_end(&mut self, _: &SelectToEnd, _: &mut Window, cx: &mut Context<Self>) {
        let end = self.editor.doc.len();
        self.editor.select_to(end);
        self.moved(cx);
    }

    fn word_left(&mut self, _: &MoveToPreviousWord, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.editor.doc.prev_word_start(self.editor.head());
        self.editor.move_to(to);
        self.moved(cx);
    }

    fn word_right(&mut self, _: &MoveToNextWord, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.editor.doc.next_word_end(self.editor.head());
        self.editor.move_to(to);
        self.moved(cx);
    }

    fn select_word_left(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.editor.doc.prev_word_start(self.editor.head());
        self.editor.select_to(to);
        self.moved(cx);
    }

    fn select_word_right(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.editor.doc.next_word_end(self.editor.head());
        self.editor.select_to(to);
        self.moved(cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.editor.select_all();
        self.moved(cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.editor.copy() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.editor.copy() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.editor.type_text("");
            self.changed(cx);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.editor.type_text(&text);
            self.changed(cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.editor.undo() {
            self.restored = true;
            self.changed(cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.editor.redo() {
            self.restored = true;
            self.changed(cx);
        }
    }

    fn character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // The mouse.

    /// The last paint's layout, made again first if the document has changed since: macOS asks
    /// where the composing text is straight after changing it, before anything is drawn, and an
    /// answer from the old layout is wrong at best (review of #133).
    fn fresh(&mut self, window: &Window) -> Option<(Layout, Bounds<Pixels>, Widths)> {
        let widths = self.widths_for(window);
        let laid = self.last.as_mut()?;
        if laid.revision != self.revision {
            laid.layout = layout::lay_out(
                &self.editor.doc,
                &widths,
                f32::from(laid.bounds.size.width),
                laid.layout.line_height,
                laid.chip,
            );
            laid.revision = self.revision;
        }
        Some((laid.layout.clone(), laid.bounds, widths))
    }

    /// The field's widths, made once and kept while the font stays the same.
    fn widths_for(&mut self, window: &Window) -> Widths {
        let fresh = widths(window);
        match &self.widths {
            Some(kept) if kept.font == fresh.font && kept.size == fresh.size => kept.clone(),
            _ => {
                self.widths = Some(fresh.clone());
                fresh
            }
        }
    }

    /// The offset under a window position.
    fn offset_at(&mut self, position: Point<Pixels>, window: &Window) -> usize {
        let Some((layout, bounds, widths)) = self.fresh(window) else {
            return self.editor.doc.len();
        };
        let x = f32::from(position.x - bounds.left());
        let y = f32::from(position.y - bounds.top()) + self.scroll;
        layout.offset_at(&self.editor.doc, &widths, x, y)
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
        self.selecting = true;
        let at = self.offset_at(event.position, window);
        if event.modifiers.shift {
            self.editor.select_to(at);
        } else if event.click_count >= 2 {
            let word = self.editor.doc.word_at(at);
            self.editor.move_to(word.start);
            self.editor.select_to(word.end);
        } else {
            self.editor.move_to(at);
        }
        self.moved(cx);
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            let at = self.offset_at(event.position, window);
            self.editor.select_to(at);
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
        self.scroll = (self.scroll - delta).max(0.);
        self.autoscroll = false;
        cx.notify();
    }
}

impl EntityInputHandler for ComposerEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.editor.range_from_utf16(&range_utf16);
        actual.replace(self.editor.range_to_utf16(&range));
        Some(self.editor.doc.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.editor.range_to_utf16(&self.editor.selection.range),
            reversed: self.editor.selection.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.editor
            .marked
            .as_ref()
            .map(|range| self.editor.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.editor.unmark();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|range| self.editor.range_from_utf16(&range));
        self.editor.ime_replace(range, text);
        self.changed(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|range| self.editor.range_from_utf16(&range));
        // The selection inside the new text is in UTF-16 units of that text.
        let selected = selected_utf16.map(|inner| {
            let to_bytes = |units: usize| {
                let mut count = 0;
                for (at, ch) in text.char_indices() {
                    if count >= units {
                        return at;
                    }
                    count += ch.len_utf16();
                }
                text.len()
            };
            to_bytes(inner.start)..to_bytes(inner.end)
        });
        self.editor.ime_mark(range, text, selected);
        self.changed(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (layout, _, widths) = self.fresh(window)?;
        let range = self.editor.range_from_utf16(&range_utf16);
        let doc = &self.editor.doc;
        let (x, row) = layout.caret(doc, &widths, range.start);
        let (end_x, end_row) = layout.caret(doc, &widths, range.end);
        let end_x = if end_row == row { end_x } else { x + 1. };
        let top = element_bounds.top() + px(row as f32 * layout.line_height - self.scroll);
        Some(Bounds::from_corners(
            point(element_bounds.left() + px(x), top),
            point(
                element_bounds.left() + px(end_x.max(x + 1.)),
                top + px(layout.line_height),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let at = self.offset_at(position, window);
        Some(self.editor.to_utf16(at))
    }
}

impl Render for ComposerEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("composer-editor")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .w_full()
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_back))
            .on_action(cx.listener(Self::delete_word_forward))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::delete_to_line_end))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_to_line_start))
            .on_action(cx.listener(Self::select_to_line_end))
            .on_action(cx.listener(Self::move_to_start))
            .on_action(cx.listener(Self::move_to_end))
            .on_action(cx.listener(Self::select_to_start))
            .on_action(cx.listener(Self::select_to_end))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .child(ComposerElement {
                editor: cx.entity(),
            })
    }
}

/// The element that lays the document out and paints it.
struct ComposerElement {
    editor: Entity<ComposerEditor>,
}

impl IntoElement for ComposerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn widths(window: &Window) -> Widths {
    let style = window.text_style();
    Widths {
        text_system: window.text_system().clone(),
        font: style.font(),
        size: style.font_size.to_pixels(window.rem_size()),
        cache: Arc::default(),
    }
}

fn chip_metrics(window: &Window) -> ChipMetrics {
    let size = f32::from(window.text_style().font_size.to_pixels(window.rem_size()));
    ChipMetrics {
        pad: (size * 0.4).round(),
        icon: (size * 0.85).round(),
        gap: (size * 0.3).round(),
    }
}

struct Prepaint {
    layout: Layout,
    widths: Widths,
    chip: ChipMetrics,
}

impl Element for ComposerElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let editor = self.editor.clone();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        // The font, line height and chip sizes are read here, where this element's inherited
        // text style is in force. The closure below runs later, from the root's layout pass,
        // when that style has been popped, and reading them there measured the field in the
        // default font (review of #133).
        let measure = self.editor.update(cx, |this, _| this.widths_for(window));
        let line_height = f32::from(window.line_height());
        let chip = chip_metrics(window);
        // The field is as tall as its lines, between the composer's minimum and maximum, and
        // that depends on the width it is given, so it is measured once the width is known.
        let layout_id = window.request_measured_layout(style, move |known, available, _, cx| {
            let width = known.width.unwrap_or(match available.width {
                AvailableSpace::Definite(width) => width,
                _ => px(600.),
            });
            let this = editor.read(cx);
            let laid = layout::lay_out(
                &this.editor.doc,
                &measure,
                f32::from(width),
                line_height,
                chip,
            );
            let rows = laid.lines.len().clamp(this.min_rows, this.max_rows);
            size(width, px(rows as f32 * line_height))
        });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let measure = self.editor.update(cx, |this, _| this.widths_for(window));
        let chip = chip_metrics(window);
        let line_height = f32::from(window.line_height());
        // The last paint's layout is used again when nothing it was made from has changed: a
        // blinking caret redraws twice a second and lays nothing out.
        let layout = {
            let this = self.editor.read(cx);
            match &this.last {
                Some(laid)
                    if laid.revision == this.revision
                        && laid.bounds.size.width == bounds.size.width
                        && laid.layout.line_height == line_height =>
                {
                    laid.layout.clone()
                }
                _ => layout::lay_out(
                    &this.editor.doc,
                    &measure,
                    f32::from(bounds.size.width),
                    line_height,
                    chip,
                ),
            }
        };
        Prepaint {
            layout,
            widths: measure,
            chip,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepaint: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        let theme = cx.theme().clone();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let Prepaint {
            layout,
            widths,
            chip,
        } = prepaint;

        // Keep the caret in view once the text is taller than the field.
        let (doc, selection, marked, placeholder, mut scroll, caret_on, autoscroll, revision) = {
            let this = self.editor.read(cx);
            (
                this.editor.doc.clone(),
                this.editor.selection.clone(),
                this.editor.marked.clone(),
                this.placeholder.clone(),
                this.scroll,
                this.caret_on,
                this.autoscroll,
                this.revision,
            )
        };
        let view_h = f32::from(bounds.size.height);
        let (caret_x, caret_row) = layout.caret(&doc, widths, selection.head());
        let caret_top = caret_row as f32 * layout.line_height;
        let max_scroll = (layout.height() - view_h).max(0.);
        // Only after an edit or a caret move: a draft the person scrolled stays where they put it.
        if autoscroll {
            if caret_top < scroll {
                scroll = caret_top;
            } else if caret_top + layout.line_height > scroll + view_h {
                scroll = caret_top + layout.line_height - view_h;
            }
        }
        scroll = scroll.clamp(0., max_scroll);
        let origin = |x: f32, row: usize| {
            point(
                bounds.left() + px(x),
                bounds.top() + px(row as f32 * layout.line_height - scroll),
            )
        };

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            // The selection, under everything.
            for (x, row, w) in layout.rects(&doc, widths, selection.range.clone()) {
                window.paint_quad(fill(
                    Bounds::new(origin(x, row), size(px(w), line_height)),
                    theme.selection,
                ));
            }

            if doc.is_empty() {
                let run = TextRun {
                    len: placeholder.len(),
                    font: style.font(),
                    color: theme.muted_foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped =
                    window
                        .text_system()
                        .shape_line(placeholder.clone(), font_size, &[run], None);
                let _ = shaped.paint(
                    origin(0., 0),
                    line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }

            for (row, line) in layout.lines.iter().enumerate() {
                for piece in &line.pieces {
                    match piece {
                        Piece::Text { range, x, .. } => {
                            let text: SharedString = doc.text()[range.clone()]
                                .trim_end_matches('\n')
                                .to_string()
                                .into();
                            if text.is_empty() {
                                continue;
                            }
                            let runs =
                                text_runs(&text, range.start, marked.as_ref(), &style, &theme);
                            let shaped = window
                                .text_system()
                                .shape_line(text, font_size, &runs, None);
                            let _ = shaped.paint(
                                origin(*x, row),
                                line_height,
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        }
                        Piece::Chip {
                            index, x, w, label, ..
                        } => {
                            let Some(kind) = doc.chips().get(*index).map(|c| c.kind) else {
                                continue;
                            };
                            paint_chip(
                                window,
                                cx,
                                origin(*x, row),
                                *w,
                                label,
                                kind,
                                *chip,
                                line_height,
                                font_size,
                                &style,
                                &theme,
                            );
                        }
                    }
                }
            }

            if focus.is_focused(window) && selection.range.is_empty() && caret_on {
                window.paint_quad(fill(
                    Bounds::new(origin(caret_x, caret_row), size(px(1.5), line_height)),
                    theme.caret,
                ));
            }
        });

        let chip = *chip;
        let layout = layout.clone();
        self.editor.update(cx, |this, _| {
            this.scroll = scroll;
            this.autoscroll = false;
            this.last = Some(Laid {
                layout,
                revision,
                bounds,
                chip,
            });
        });
    }
}

/// The text's runs, with the IME's composing text underlined.
fn text_runs(
    text: &str,
    start: usize,
    marked: Option<&Range<usize>>,
    style: &TextStyle,
    theme: &gpui_kit::component::Theme,
) -> Vec<TextRun> {
    let base = TextRun {
        len: text.len(),
        font: style.font(),
        color: theme.foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Some(marked) = marked else {
        return vec![base];
    };
    let from = marked.start.saturating_sub(start).min(text.len());
    let to = marked.end.saturating_sub(start).min(text.len());
    if from >= to {
        return vec![base];
    }
    [
        TextRun {
            len: from,
            ..base.clone()
        },
        TextRun {
            len: to - from,
            underline: Some(UnderlineStyle {
                color: Some(theme.foreground),
                thickness: px(1.),
                wavy: false,
            }),
            ..base.clone()
        },
        TextRun {
            len: text.len() - to,
            ..base
        },
    ]
    .into_iter()
    .filter(|run| run.len > 0)
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn paint_chip(
    window: &mut Window,
    cx: &mut App,
    at: Point<Pixels>,
    w: f32,
    label: &str,
    kind: TokenKind,
    chip: ChipMetrics,
    line_height: Pixels,
    font_size: Pixels,
    style: &TextStyle,
    theme: &gpui_kit::component::Theme,
) {
    // The box sits inside the line with a little air above and below, so chips on lines next to
    // each other do not touch.
    let inset = px(2.);
    let box_bounds = Bounds::new(
        point(at.x, at.y + inset),
        size(px(w), line_height - inset * 2.),
    );
    window.paint_quad(quad(
        box_bounds,
        px(6.),
        theme.secondary,
        px(0.),
        transparent_black(),
        BorderStyle::default(),
    ));
    let icon = px(chip.icon);
    let icon_bounds = Bounds::new(
        point(at.x + px(chip.pad), at.y + (line_height - icon) / 2.),
        size(icon, icon),
    );
    let _ = window.paint_svg(
        icon_bounds,
        chip_icon(kind).into(),
        None,
        TransformationMatrix::unit(),
        theme.secondary_foreground,
        cx,
    );
    let run = TextRun {
        len: label.len(),
        font: style.font(),
        color: theme.secondary_foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(label.to_string().into(), font_size, &[run], None);
    let _ = shaped.paint(
        point(at.x + px(chip.pad + chip.icon + chip.gap), at.y),
        line_height,
        TextAlign::Left,
        None,
        window,
        cx,
    );
}
