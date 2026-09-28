//! What the composer holds: text, and chips that are each one object in it.
//!
//! A chip is not a run of letters with a fill behind it. It is ONE character in the buffer, U+FFFC
//! OBJECT REPLACEMENT CHARACTER — the character macOS text itself uses for an attachment — paired
//! with the chip it stands for in `chips`. Being one character is what makes a chip atomic without
//! any bookkeeping: no range on a character boundary can cut into it, Backspace takes it whole,
//! the caret steps over it in one move, and IME and UTF-16 offsets count it as one. What goes to
//! the server and to the clipboard is [`Doc::plain`], where each chip reads as its label, byte for
//! byte what the composer sent when chips were painted text.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::components::chat_input::TokenKind;

/// The character a chip occupies in the buffer.
pub const MARK: char = '\u{FFFC}';
const MARK_LEN: usize = MARK.len_utf8();

/// One chip: what it stands for, and what it reads as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    pub kind: TokenKind,
    /// What the thing is called where it lives: a tool's name, a recipe's id.
    pub id: String,
    /// What the chip reads as in the message and on screen.
    pub label: String,
}

/// A chip as it sits in the plain text: which one, and where its label is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlainChip {
    pub index: usize,
    pub range: Range<usize>,
}

/// The buffer and its chips. `chips[i]` is the chip of the i-th [`MARK`] in `text`, and there
/// is no other [`MARK`]: every way in strips a stray one, so the two can never disagree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Doc {
    text: String,
    chips: Vec<Chip>,
}

impl Doc {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn chips(&self) -> &[Chip] {
        &self.chips
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// A document of plain text, as a draft refilled from a queued send arrives.
    pub fn from_plain(text: &str) -> Self {
        Self {
            text: strip_marks(text),
            chips: Vec::new(),
        }
    }

    /// Which chip, if any, sits at this buffer offset.
    pub fn chip_at(&self, offset: usize) -> Option<usize> {
        if self.text.get(offset..)?.starts_with(MARK) {
            Some(self.chips_before(offset))
        } else {
            None
        }
    }

    /// How many chips come before this buffer offset.
    pub fn chips_before(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .matches(MARK)
            .count()
    }

    /// Where each chip's marker is, in order.
    pub fn mark_offsets(&self) -> Vec<usize> {
        self.text.match_indices(MARK).map(|(at, _)| at).collect()
    }

    /// The text as it is sent and copied: each chip reads as its label.
    pub fn plain(&self) -> String {
        self.plain_of(0..self.text.len())
    }

    /// The plain form of part of the buffer.
    pub fn plain_of(&self, range: Range<usize>) -> String {
        let mut out = String::with_capacity(range.len());
        let mut chip = self.chips_before(range.start);
        for ch in self.text[range].chars() {
            if ch == MARK {
                if let Some(it) = self.chips.get(chip) {
                    out.push_str(&it.label);
                }
                chip += 1;
            } else {
                out.push(ch);
            }
        }
        out
    }

    /// Each chip and where its label sits in [`Self::plain`].
    pub fn plain_chips(&self) -> Vec<PlainChip> {
        let mut out = Vec::with_capacity(self.chips.len());
        let mut plain = 0;
        let mut chip = 0;
        for ch in self.text.chars() {
            if ch == MARK {
                let label = self.chips.get(chip).map_or(0, |it| it.label.len());
                out.push(PlainChip {
                    index: chip,
                    range: plain..plain + label,
                });
                plain += label;
                chip += 1;
            } else {
                plain += ch.len_utf8();
            }
        }
        out
    }

    /// A buffer offset in plain-text terms. An offset is always on a character boundary, so it is
    /// never inside a chip's marker; it can be before or after one.
    pub fn buf_to_plain(&self, offset: usize) -> usize {
        let offset = offset.min(self.text.len());
        let before = &self.text[..offset];
        let mut chip = 0;
        let mut plain = 0;
        for ch in before.chars() {
            if ch == MARK {
                plain += self.chips.get(chip).map_or(0, |it| it.label.len());
                chip += 1;
            } else {
                plain += ch.len_utf8();
            }
        }
        plain
    }

    /// A plain-text offset in buffer terms. An offset inside a chip's label lands after the chip:
    /// the chip is one thing, and the composer asking about a position inside it means "here,
    /// where this chip is", which is past it.
    pub fn plain_to_buf(&self, plain: usize) -> usize {
        let mut at_plain = 0;
        let mut chip = 0;
        for (at, ch) in self.text.char_indices() {
            if at_plain >= plain {
                return at;
            }
            if ch == MARK {
                at_plain += self.chips.get(chip).map_or(0, |it| it.label.len());
                chip += 1;
            } else {
                at_plain += ch.len_utf8();
            }
        }
        self.text.len()
    }

    /// Replace a buffer range with text. A [`MARK`] in the text is dropped: only
    /// [`Self::insert_chip`] makes a chip. Any chip in the range goes with it. The range is widened
    /// to character boundaries, so it can never leave half a character behind.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> Range<usize> {
        let range = self.char_range(range);
        let first = self.chips_before(range.start);
        let gone = self.text[range.clone()].matches(MARK).count();
        self.chips.drain(first..first + gone);
        let clean = strip_marks(text);
        self.text.replace_range(range.clone(), &clean);
        range.start..range.start + clean.len()
    }

    /// Put a chip at a buffer offset. Returns the range of its marker.
    pub fn insert_chip(&mut self, at: usize, chip: Chip) -> Range<usize> {
        let at = self.floor_char(at.min(self.text.len()));
        let index = self.chips_before(at);
        self.chips.insert(index, chip);
        self.text.insert(at, MARK);
        at..at + MARK_LEN
    }

    /// Take a chip out by its index. Returns where it was.
    pub fn remove_chip(&mut self, index: usize) -> Option<usize> {
        let at = *self.mark_offsets().get(index)?;
        self.chips.remove(index);
        self.text.replace_range(at..at + MARK_LEN, "");
        Some(at)
    }

    /// The nearest character boundary at or before `offset`.
    pub fn floor_char(&self, mut offset: usize) -> usize {
        offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn char_range(&self, range: Range<usize>) -> Range<usize> {
        let start = self.floor_char(range.start.min(range.end));
        let mut end = range.end.max(range.start).min(self.text.len());
        while !self.text.is_char_boundary(end) {
            end += 1;
        }
        start..end
    }

    /// Where the caret may stop: every grapheme boundary, and both sides of every chip. A chip is
    /// its own stop even when a combining mark was typed after it, so it is never merged into the
    /// letter beside it.
    pub fn boundaries(&self) -> Vec<usize> {
        let mut stops: Vec<usize> = self.text.grapheme_indices(true).map(|(at, _)| at).collect();
        for at in self.mark_offsets() {
            stops.push(at);
            stops.push(at + MARK_LEN);
        }
        stops.push(self.text.len());
        stops.sort_unstable();
        stops.dedup();
        stops
    }

    pub fn prev_boundary(&self, offset: usize) -> usize {
        self.boundaries()
            .into_iter()
            .rev()
            .find(|at| *at < offset)
            .unwrap_or(0)
    }

    pub fn next_boundary(&self, offset: usize) -> usize {
        self.boundaries()
            .into_iter()
            .find(|at| *at > offset)
            .unwrap_or(self.text.len())
    }

    /// The start of the word before `offset`: spaces are skipped, then a chip is one word, and
    /// otherwise the word runs back to the next space, newline or chip.
    pub fn prev_word_start(&self, offset: usize) -> usize {
        let before: Vec<(usize, char)> = self.text[..offset.min(self.text.len())]
            .char_indices()
            .collect();
        let mut i = before.len();
        while i > 0 && before[i - 1].1.is_whitespace() {
            i -= 1;
        }
        if i > 0 && before[i - 1].1 == MARK {
            return before[i - 1].0;
        }
        while i > 0 && !before[i - 1].1.is_whitespace() && before[i - 1].1 != MARK {
            i -= 1;
        }
        before
            .get(i)
            .map_or(offset.min(self.text.len()), |(at, _)| *at)
    }

    /// The end of the word after `offset`, the mirror of [`Self::prev_word_start`].
    pub fn next_word_end(&self, offset: usize) -> usize {
        let start = offset.min(self.text.len());
        let mut chars = self.text[start..].char_indices().peekable();
        let mut end = start;
        while let Some((at, ch)) = chars.peek().copied() {
            if !ch.is_whitespace() {
                break;
            }
            end = start + at + ch.len_utf8();
            chars.next();
        }
        if let Some((at, MARK)) = chars.peek().copied() {
            return start + at + MARK_LEN;
        }
        for (at, ch) in chars {
            if ch.is_whitespace() || ch == MARK {
                return start + at;
            }
            end = start + at + ch.len_utf8();
        }
        end
    }

    /// The word a double-click at `offset` means: the word the offset is in or touches, with a
    /// chip as a word of its own. A click on the right half of a word's last letter lands at the
    /// word's end and still means that word, not the one after the space.
    pub fn word_at(&self, offset: usize) -> Range<usize> {
        let offset = self.floor_char(offset);
        let is_word = |ch: char| !ch.is_whitespace() && ch != MARK;
        let after = self.text[offset..].chars().next();
        let before = self.text[..offset].chars().next_back();
        let at = match (before, after) {
            (_, Some(MARK)) => return offset..offset + MARK_LEN,
            (_, Some(ch)) if is_word(ch) => offset,
            (Some(MARK), _) => return offset - MARK_LEN..offset,
            (Some(ch), _) if is_word(ch) => offset - ch.len_utf8(),
            _ => return offset..offset,
        };
        let mut start = at;
        for (i, ch) in self.text[..at].char_indices().rev() {
            if !is_word(ch) {
                break;
            }
            start = i;
        }
        let mut end = at;
        for (i, ch) in self.text[at..].char_indices() {
            if !is_word(ch) {
                break;
            }
            end = at + i + ch.len_utf8();
        }
        start..end
    }

    /// The start of the logical line (after the last newline) that `offset` is on.
    pub fn line_start(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())]
            .rfind('\n')
            .map_or(0, |at| at + 1)
    }

    /// The end of the logical line (before the next newline) that `offset` is on.
    pub fn line_end(&self, offset: usize) -> usize {
        let start = offset.min(self.text.len());
        self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |at| start + at)
    }

    /// Whether the invariants hold: one chip per marker. Tests call this after every step.
    pub fn is_consistent(&self) -> bool {
        self.text.matches(MARK).count() == self.chips.len()
    }
}

/// Text with every [`MARK`] taken out: a pasted or refilled one is not a chip.
pub fn strip_marks(text: &str) -> String {
    if text.contains(MARK) {
        text.chars().filter(|ch| *ch != MARK).collect()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
pub(crate) fn chip(kind: TokenKind, label: &str) -> Chip {
    Chip {
        kind,
        id: label.to_lowercase().replace(' ', "-"),
        label: label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_chip() -> Doc {
        let mut doc = Doc::from_plain("run  now");
        doc.insert_chip(4, chip(TokenKind::Recipe, "Weekly report"));
        doc
    }

    /// What is sent is the text with each chip read as its label, and the chip's place in that
    /// text is where the composer's tokens say it is.
    #[test]
    fn a_chip_reads_as_its_label_in_what_is_sent() {
        let doc = doc_with_chip();
        assert_eq!(doc.plain(), "run Weekly report now");
        assert_eq!(
            doc.plain_chips(),
            vec![PlainChip {
                index: 0,
                range: 4..17
            }]
        );
        assert!(doc.is_consistent());
    }

    /// Offsets move between the buffer and the plain text both ways, and a plain offset inside a
    /// chip's label lands after the chip.
    #[test]
    fn offsets_map_between_the_buffer_and_the_plain_text() {
        let doc = doc_with_chip();
        let mark = 4;
        assert_eq!(doc.buf_to_plain(mark), 4);
        assert_eq!(doc.buf_to_plain(mark + MARK_LEN), 17);
        assert_eq!(doc.plain_to_buf(4), mark);
        assert_eq!(doc.plain_to_buf(17), mark + MARK_LEN);
        assert_eq!(
            doc.plain_to_buf(9),
            mark + MARK_LEN,
            "inside the label: after the chip"
        );
        assert_eq!(doc.plain_to_buf(100), doc.len());
    }

    /// A chip is one stop for the caret and one step for Backspace.
    #[test]
    fn a_chip_is_one_step() {
        let doc = doc_with_chip();
        let after = 4 + MARK_LEN;
        assert_eq!(doc.prev_boundary(after), 4);
        assert_eq!(doc.next_boundary(4), after);
        assert_eq!(doc.prev_word_start(after), 4, "a chip is a word of its own");
        assert_eq!(doc.next_word_end(4), after);
    }

    /// Replacing a range that holds a chip takes the chip with it, and a marker in the new text
    /// never becomes a chip.
    #[test]
    fn a_replace_takes_the_chips_in_its_range_and_makes_none() {
        let mut doc = doc_with_chip();
        doc.replace(2..8, "\u{FFFC}x");
        assert_eq!(
            doc.text(),
            "ruxnow",
            "bytes 2..8 are \"n\", the chip and the space after it"
        );
        assert!(doc.chips().is_empty());
        assert!(doc.is_consistent());
    }

    /// Chips keep their order however they are added and removed.
    #[test]
    fn several_chips_keep_their_order() {
        let mut doc = Doc::from_plain("a b c");
        doc.insert_chip(4, chip(TokenKind::Skill, "Second"));
        doc.insert_chip(2, chip(TokenKind::Recipe, "First"));
        assert_eq!(doc.plain(), "a Firstb Secondc");
        assert_eq!(doc.remove_chip(0), Some(2));
        assert_eq!(doc.plain(), "a b Secondc");
        assert_eq!(doc.chips()[0].label, "Second");
        assert!(doc.is_consistent());
    }

    /// A double-click means one word, whichever half of its last letter was hit, and a chip is a
    /// word of its own (review of #133).
    #[test]
    fn a_double_click_means_one_word() {
        let doc = Doc::from_plain("hello world");
        assert_eq!(
            doc.word_at(5),
            0..5,
            "the right half of 'o' is still 'hello'"
        );
        assert_eq!(doc.word_at(6), 6..11);
        assert_eq!(doc.word_at(2), 0..5);
        let doc = doc_with_chip();
        assert_eq!(doc.word_at(4), 4..4 + MARK_LEN);
        assert_eq!(
            doc.word_at(4 + MARK_LEN),
            4..4 + MARK_LEN,
            "just after the chip: the chip"
        );
    }

    /// A combining mark typed straight after a chip does not fuse with it: the chip stays its own
    /// step.
    #[test]
    fn a_combining_mark_after_a_chip_does_not_fuse_with_it() {
        let mut doc = Doc::from_plain("");
        doc.insert_chip(0, chip(TokenKind::Tool, "Search"));
        doc.replace(MARK_LEN..MARK_LEN, "\u{0301}e");
        assert_eq!(doc.next_boundary(0), MARK_LEN);
        assert_eq!(doc.prev_boundary(MARK_LEN), 0);
    }
}
