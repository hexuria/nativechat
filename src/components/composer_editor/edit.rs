//! Editing: the document, the selection, and undo, with every operation a plain function so the
//! whole of it is tested without a window. The GPUI entity only turns keys and IME calls into
//! these.

use std::ops::Range;

use super::model::{Chip, Doc, MARK};

/// The selection. `range.start <= range.end` always; `reversed` says the caret is at the start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub range: Range<usize>,
    pub reversed: bool,
}

impl Selection {
    pub fn caret(at: usize) -> Self {
        Self {
            range: at..at,
            reversed: false,
        }
    }

    /// Where the caret is: the moving end.
    pub fn head(&self) -> usize {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }

    /// The end that stays put while the selection is extended.
    pub fn anchor(&self) -> usize {
        if self.reversed {
            self.range.end
        } else {
            self.range.start
        }
    }

    fn from_ends(anchor: usize, head: usize) -> Self {
        if head < anchor {
            Self {
                range: head..anchor,
                reversed: true,
            }
        } else {
            Self {
                range: anchor..head,
                reversed: false,
            }
        }
    }
}

/// What the last change was, so a run of typing is one undo step rather than one per letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Typing,
    /// A run of single Backspaces or Deletes: one step, as a run of typing is, rather than a
    /// copy of the whole draft per key held down.
    Deleting,
    Other,
}

#[derive(Clone, Debug)]
struct Snapshot {
    doc: Doc,
    selection: Selection,
}

/// The editor's whole state apart from its layout.
#[derive(Clone, Debug, Default)]
pub struct Editor {
    pub doc: Doc,
    pub selection: Selection,
    /// The IME's composing text, while there is one.
    pub marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last: Option<Kind>,
}

/// How many undo steps are kept. A draft is short; this is room for a long session of editing
/// one without the stack growing without end.
const UNDO_DEPTH: usize = 200;

impl Editor {
    pub fn head(&self) -> usize {
        self.selection.head()
    }

    fn remember(&mut self, kind: Kind) {
        // A run of typed letters is one step: the snapshot before the first letter is the one
        // Undo goes back to. Anything else starts a new step.
        if kind != Kind::Other && self.last == Some(kind) {
            return;
        }
        self.undo.push(Snapshot {
            doc: self.doc.clone(),
            selection: self.selection.clone(),
        });
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.last = Some(kind);
    }

    /// Anything that moves the caret without changing the text ends a run of typing.
    fn moved(&mut self) {
        self.last = None;
    }

    pub fn undo(&mut self) -> bool {
        let Some(back) = self.undo.pop() else {
            return false;
        };
        self.redo.push(Snapshot {
            doc: std::mem::replace(&mut self.doc, back.doc),
            selection: std::mem::replace(&mut self.selection, back.selection),
        });
        self.marked = None;
        self.last = None;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(forward) = self.redo.pop() else {
            return false;
        };
        self.undo.push(Snapshot {
            doc: std::mem::replace(&mut self.doc, forward.doc),
            selection: std::mem::replace(&mut self.selection, forward.selection),
        });
        self.marked = None;
        self.last = None;
        true
    }

    /// Replace the whole draft, as sending it (with nothing), dictation or a refilled queued
    /// send does. The undo history goes with it, as the old field's `set_value` cleared it: ⌘Z
    /// after sending must not bring back the message that has just gone.
    pub fn set_doc(&mut self, doc: Doc) {
        self.undo.clear();
        self.redo.clear();
        self.last = None;
        let end = doc.len();
        self.doc = doc;
        self.selection = Selection::caret(end);
        self.marked = None;
    }

    /// Type (or paste, or dictate) text over the selection.
    pub fn type_text(&mut self, text: &str) {
        let kind = if text.chars().count() == 1 && self.selection.range.is_empty() {
            Kind::Typing
        } else {
            Kind::Other
        };
        self.type_as(text, kind);
    }

    fn type_as(&mut self, text: &str, kind: Kind) {
        self.remember(kind);
        let range = self.selection.range.clone();
        let placed = self.doc.replace(range, text);
        self.selection = Selection::caret(placed.end);
        self.marked = None;
    }

    /// Put a chip at the caret, replacing any selection, with a space after it so the next word
    /// does not run into it.
    pub fn insert_chip(&mut self, chip: Chip) -> usize {
        self.place_chip(chip, true)
    }

    /// Turn the selected words into a chip in place, with no space added: a draft refilled from
    /// a queued send already has its words, and the chip stands for the ones it was.
    pub fn chip_over_selection(&mut self, chip: Chip) -> usize {
        self.place_chip(chip, false)
    }

    fn place_chip(&mut self, chip: Chip, space: bool) -> usize {
        self.remember(Kind::Other);
        let range = self.selection.range.clone();
        let at = self.doc.replace(range, "").start;
        let mark = self.doc.insert_chip(at, chip);
        let end = if space {
            self.doc.replace(mark.end..mark.end, " ").end
        } else {
            mark.end
        };
        self.selection = Selection::caret(end);
        self.marked = None;
        self.doc.chips_before(mark.start)
    }

    /// Take a chip out by index, as picking a second recipe removes the first.
    pub fn remove_chip(&mut self, index: usize) -> bool {
        let Some(&at) = self.doc.mark_offsets().get(index) else {
            return false;
        };
        self.remember(Kind::Other);
        let head = self.selection.head();
        self.doc.remove_chip(index);
        // A chip goes in with a space after it (see `insert_chip`); leaving that behind would
        // leave a gap where the chip was.
        let space = usize::from(self.doc.text()[at..].starts_with(' '));
        if space == 1 {
            self.doc.replace(at..at + 1, "");
        }
        let cut = MARK.len_utf8() + space;
        let shift = |offset: usize| {
            if offset >= at + cut {
                offset - cut
            } else if offset > at {
                at
            } else {
                offset
            }
        };
        self.selection = Selection::caret(shift(head));
        self.marked = None;
        true
    }

    /// Turn a chip back into the words it read as: what it stood for is gone, and a chip that
    /// means nothing would say otherwise. One undo step.
    pub fn unchip(&mut self, index: usize) -> bool {
        let Some(&at) = self.doc.mark_offsets().get(index) else {
            return false;
        };
        let label = self.doc.chips()[index].label.clone();
        self.remember(Kind::Other);
        let head = self.selection.head();
        let placed = self.doc.replace(at..at + MARK.len_utf8(), &label);
        let grow = placed.len().saturating_sub(MARK.len_utf8());
        let shift = |offset: usize| if offset > at { offset + grow } else { offset };
        self.selection = Selection::caret(shift(head));
        self.marked = None;
        true
    }

    /// Delete the selection, or with none, from the caret to `to` (a boundary on either side).
    fn delete_toward(&mut self, to: usize) -> bool {
        self.delete_as(to, Kind::Other)
    }

    fn delete_as(&mut self, to: usize, kind: Kind) -> bool {
        let range = if self.selection.range.is_empty() {
            let head = self.selection.head();
            head.min(to)..head.max(to)
        } else {
            self.selection.range.clone()
        };
        if range.is_empty() {
            return false;
        }
        let kind = if self.selection.range.is_empty() {
            kind
        } else {
            Kind::Other
        };
        self.remember(kind);
        let placed = self.doc.replace(range, "");
        self.selection = Selection::caret(placed.start);
        self.marked = None;
        true
    }

    pub fn backspace(&mut self) -> bool {
        let to = self.doc.prev_boundary(self.head());
        self.delete_as(to, Kind::Deleting)
    }

    pub fn delete(&mut self) -> bool {
        let to = self.doc.next_boundary(self.head());
        self.delete_as(to, Kind::Deleting)
    }

    pub fn delete_word_back(&mut self) -> bool {
        let to = self.doc.prev_word_start(self.head());
        self.delete_toward(to)
    }

    pub fn delete_word_forward(&mut self) -> bool {
        let to = self.doc.next_word_end(self.head());
        self.delete_toward(to)
    }

    pub fn delete_to(&mut self, to: usize) -> bool {
        self.delete_toward(to)
    }

    /// Move the caret, dropping any selection.
    pub fn move_to(&mut self, at: usize) {
        self.moved();
        self.selection = Selection::caret(self.doc.floor_char(at));
    }

    /// Move the caret's end, keeping the anchor: a shifted arrow, a drag.
    pub fn select_to(&mut self, at: usize) {
        self.moved();
        let at = self.doc.floor_char(at);
        self.selection = Selection::from_ends(self.selection.anchor(), at);
    }

    pub fn select_all(&mut self) {
        self.moved();
        self.selection = Selection {
            range: 0..self.doc.len(),
            reversed: false,
        };
    }

    /// Left: to the selection's start if there is one, otherwise one step back.
    pub fn left(&mut self) {
        let to = if self.selection.range.is_empty() {
            self.doc.prev_boundary(self.head())
        } else {
            self.selection.range.start
        };
        self.move_to(to);
    }

    pub fn right(&mut self) {
        let to = if self.selection.range.is_empty() {
            self.doc.next_boundary(self.head())
        } else {
            self.selection.range.end
        };
        self.move_to(to);
    }

    /// What Copy puts on the clipboard: the selection with each chip as its label.
    pub fn copy(&self) -> Option<String> {
        (!self.selection.range.is_empty()).then(|| self.doc.plain_of(self.selection.range.clone()))
    }

    // IME. The platform speaks in UTF-16 offsets over the buffer. A chip is one UTF-16 unit, so
    // any range it names falls on character boundaries and cannot cut into a chip.

    pub fn to_utf16(&self, offset: usize) -> usize {
        self.doc.text()[..self.doc.floor_char(offset)]
            .encode_utf16()
            .count()
    }

    pub fn from_utf16(&self, utf16: usize) -> usize {
        let mut units = 0;
        for (at, ch) in self.doc.text().char_indices() {
            if units >= utf16 {
                return at;
            }
            units += ch.len_utf16();
        }
        self.doc.len()
    }

    pub fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.to_utf16(range.start)..self.to_utf16(range.end)
    }

    pub fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.from_utf16(range.start)..self.from_utf16(range.end)
    }

    /// The IME's "replace this range with this text": a committed composition, a dictated
    /// phrase, an accent picked from the press-and-hold menu. With no range it replaces the
    /// composing text, or else the selection.
    pub fn ime_replace(&mut self, range: Option<Range<usize>>, text: &str) {
        let composing = self.marked.is_some();
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.range.clone());
        self.selection = Selection {
            range,
            reversed: false,
        };
        self.marked = None;
        if composing {
            // A commit is the end of the composition it replaces, and the composition's first
            // keystroke already remembered the draft from before it: one undo takes both away,
            // rather than leaving the uncommitted letters behind as text.
            self.type_as(text, Kind::Typing);
        } else {
            self.type_text(text);
        }
    }

    /// The IME's composing text: it replaces the range and stays marked until it is committed.
    /// `selected` is where the caret sits inside the new text.
    pub fn ime_mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) {
        let range = range
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.range.clone());
        // Composition is one undo step with what it commits, so it is remembered as typing.
        self.remember(Kind::Typing);
        let placed = self.doc.replace(range, text);
        self.marked = (!placed.is_empty()).then(|| placed.clone());
        self.selection = match selected {
            Some(inner) => Selection {
                range: (placed.start + inner.start).min(placed.end)
                    ..(placed.start + inner.end).min(placed.end),
                reversed: false,
            },
            None => Selection::caret(placed.end),
        };
    }

    pub fn unmark(&mut self) {
        self.marked = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{Doc, MARK, chip};
    use super::*;
    use crate::components::chat_input::TokenKind;

    const M: usize = MARK.len_utf8();

    fn editor(text: &str) -> Editor {
        let mut editor = Editor {
            doc: Doc::from_plain(text),
            ..Default::default()
        };
        editor.move_to(text.len());
        editor
    }

    /// The point of the whole change: Backspace right after a chip takes the chip, in one press.
    #[test]
    fn backspace_after_a_chip_takes_the_whole_chip() {
        let mut ed = editor("please ");
        ed.insert_chip(chip(TokenKind::Recipe, "Weekly report"));
        assert_eq!(ed.doc.plain(), "please Weekly report ");
        ed.backspace();
        assert_eq!(ed.doc.plain(), "please Weekly report");
        ed.backspace();
        assert_eq!(ed.doc.plain(), "please ", "one press takes the chip");
        assert!(ed.doc.chips().is_empty());
    }

    /// Typing over a selection that holds a chip removes the chip; the rest of the text stays.
    #[test]
    fn typing_over_a_chip_removes_it() {
        let mut ed = editor("a  b");
        ed.move_to(2);
        ed.insert_chip(chip(TokenKind::Skill, "Tone"));
        ed.select_all();
        ed.type_text("x");
        assert_eq!(ed.doc.plain(), "x");
        assert!(ed.doc.chips().is_empty() && ed.doc.is_consistent());
    }

    /// Undo brings a deleted chip back as the same chip, not as its letters.
    #[test]
    fn undo_brings_a_chip_back_as_a_chip() {
        let mut ed = editor("go ");
        ed.insert_chip(chip(TokenKind::Workflow, "Triage"));
        ed.backspace();
        ed.backspace();
        assert!(ed.doc.chips().is_empty());
        assert!(ed.undo());
        assert_eq!(ed.doc.chips().len(), 1);
        assert_eq!(ed.doc.chips()[0].label, "Triage");
        assert!(ed.redo());
        assert!(ed.doc.chips().is_empty());
    }

    /// A run of typed letters is one undo step.
    #[test]
    fn a_run_of_typing_is_one_undo_step() {
        let mut ed = editor("");
        for ch in ["h", "e", "y"] {
            ed.type_text(ch);
        }
        ed.left();
        ed.type_text("!");
        assert_eq!(ed.doc.text(), "he!y");
        ed.undo();
        assert_eq!(ed.doc.text(), "hey");
        ed.undo();
        assert_eq!(ed.doc.text(), "");
    }

    /// Arrows step over a chip in one move, and shift-arrows select it whole.
    #[test]
    fn arrows_step_over_a_chip() {
        let mut ed = editor("a ");
        ed.insert_chip(chip(TokenKind::Tool, "Search"));
        ed.move_to(2);
        ed.right();
        assert_eq!(ed.head(), 2 + M);
        ed.left();
        assert_eq!(ed.head(), 2);
        ed.select_to(ed.doc.next_boundary(2));
        assert_eq!(ed.copy().as_deref(), Some("Search"));
    }

    /// Copy gives the plain text, and a pasted marker never becomes a chip.
    #[test]
    fn copy_is_plain_and_paste_strips_markers() {
        let mut ed = editor("see ");
        ed.insert_chip(chip(TokenKind::Recipe, "Q3"));
        ed.select_all();
        let copied = ed.copy().unwrap();
        assert_eq!(copied, "see Q3 ");
        ed.move_to(ed.doc.len());
        ed.type_text("\u{FFFC}!");
        assert_eq!(ed.doc.chips().len(), 1);
        assert!(ed.doc.is_consistent());
    }

    /// IME composing beside a chip never swallows it: the marked range is the composing text only,
    /// and committing replaces just that.
    #[test]
    fn composing_beside_a_chip_leaves_it_alone() {
        let mut ed = editor("");
        ed.insert_chip(chip(TokenKind::Recipe, "财务"));
        ed.ime_mark(None, "ni", Some(2..2));
        assert_eq!(ed.marked, Some(M + 1..M + 3));
        ed.ime_mark(None, "你", Some(3..3));
        ed.ime_replace(None, "你好");
        assert_eq!(ed.doc.plain(), "财务 你好");
        assert_eq!(ed.doc.chips().len(), 1);
        assert_eq!(ed.marked, None);
        // A chip is one UTF-16 unit, so the platform's offsets map cleanly around it.
        assert_eq!(ed.to_utf16(M), 1);
        assert_eq!(ed.from_utf16(1), M);
    }

    /// Taking a chip out by index takes the space it came with, and the caret keeps its place in
    /// the words around it.
    #[test]
    fn removing_a_chip_takes_its_space_and_keeps_the_caret() {
        let mut ed = editor("run ");
        ed.insert_chip(chip(TokenKind::Recipe, "Old"));
        ed.type_text("now");
        let before = ed.head();
        assert!(ed.remove_chip(0));
        assert_eq!(ed.doc.plain(), "run now");
        assert_eq!(ed.head(), before - M - 1);
        assert!(!ed.remove_chip(0), "nothing left to remove");
    }

    /// Undo after a committed composition goes back to before it, not to the letters that were
    /// being composed (review of #133).
    #[test]
    fn undo_after_a_composition_leaves_no_composing_letters() {
        let mut ed = editor("a ");
        ed.ime_mark(None, "n", None);
        ed.ime_mark(None, "ni", None);
        ed.ime_replace(None, "你");
        assert_eq!(ed.doc.text(), "a 你");
        ed.undo();
        assert_eq!(ed.doc.text(), "a ");
    }

    /// Holding Backspace is one undo step, not a copy of the draft per key.
    #[test]
    fn a_run_of_backspaces_is_one_undo_step() {
        let mut ed = editor("hello");
        for _ in 0..3 {
            ed.backspace();
        }
        assert_eq!(ed.doc.text(), "he");
        ed.undo();
        assert_eq!(ed.doc.text(), "hello");
    }

    /// Replacing the draft wholesale (a send, dictation) takes the undo history with it.
    #[test]
    fn replacing_the_draft_forgets_its_history() {
        let mut ed = editor("");
        ed.type_text("sent");
        ed.set_doc(Doc::default());
        assert!(!ed.undo(), "nothing to bring back after a send");
        assert_eq!(ed.doc.text(), "");
    }

    /// A chip whose recipe is gone becomes its words again, and the caret keeps its place.
    #[test]
    fn a_chip_can_become_its_words_again() {
        let mut ed = editor("run ");
        ed.insert_chip(chip(TokenKind::Recipe, "Weekly"));
        ed.type_text("now");
        assert!(ed.unchip(0));
        assert_eq!(ed.doc.text(), "run Weekly now");
        assert!(ed.doc.chips().is_empty());
        assert_eq!(ed.head(), ed.doc.len());
    }

    /// A word delete stops at a chip, and a second one takes the chip.
    #[test]
    fn a_word_delete_treats_a_chip_as_a_word() {
        let mut ed = editor("x ");
        ed.insert_chip(chip(TokenKind::Skill, "Tone"));
        ed.type_text("hello");
        ed.delete_word_back();
        assert_eq!(ed.doc.plain(), "x Tone ");
        ed.delete_word_back();
        assert_eq!(ed.doc.plain(), "x ");
    }

    /// Ported from the painted-chip composer's tests, where each was a bug: typing right in front
    /// of a chip pushes it along whole, and deleting a chip never moves it onto words further
    /// on that read the same.
    #[test]
    fn a_chip_is_a_place_not_a_word() {
        let mut ed = editor("");
        ed.insert_chip(chip(TokenKind::Skill, "expense-report"));
        ed.type_text("and expense-report");
        ed.move_to(0);
        ed.type_text("oh ");
        assert_eq!(ed.doc.plain(), "oh expense-report and expense-report");
        assert_eq!(
            ed.doc.plain_chips()[0].range,
            3..17,
            "pushed along, still whole"
        );
        ed.move_to(3 + M);
        ed.backspace();
        assert_eq!(ed.doc.plain(), "oh  and expense-report");
        assert!(
            ed.doc.chips().is_empty(),
            "the same words later on are words, not the chip"
        );
    }

    /// Whatever the edits, the chips and their markers never disagree.
    #[test]
    fn scripted_edits_keep_every_chip_whole() {
        let words = ["a", "bc", " ", "\n", "字", "é", "\u{FFFC}"];
        for seed in 0u64..100 {
            let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let mut next = |n: usize| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((state >> 33) as usize) % n
            };
            let mut ed = editor("");
            for _ in 0..60 {
                match next(9) {
                    0 | 1 => ed.type_text(words[next(words.len())]),
                    2 => {
                        ed.insert_chip(chip(TokenKind::Recipe, "R"));
                    }
                    3 => {
                        ed.backspace();
                    }
                    4 => {
                        ed.delete();
                    }
                    5 => ed.move_to(next(ed.doc.len() + 1)),
                    6 => ed.select_to(next(ed.doc.len() + 1)),
                    7 => {
                        ed.undo();
                    }
                    _ => ed.ime_mark(None, "k", None),
                }
                assert!(ed.doc.is_consistent(), "seed {seed}");
                assert!(ed.doc.text().is_char_boundary(ed.selection.range.start));
                assert!(ed.doc.text().is_char_boundary(ed.selection.range.end));
                assert!(ed.selection.range.end <= ed.doc.len(), "seed {seed}");
            }
        }
    }
}
