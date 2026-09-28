//! Where everything in the composer sits: lines, words and chips, in pixels.
//!
//! A chip is a box with real width in the line, the way GPUI's own wrapper takes a
//! `LineFragment::Element { width, .. }` (gpui-pre `text_system/line_wrapper.rs`). That is what the
//! painted-text chip could never have: room for an icon. Everything here is plain arithmetic over a
//! [`Measure`], so it is tested with a fixed-advance font and painted with the real one.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::model::{Doc, MARK};

/// How wide a run of text is in the composer's font.
pub trait Measure {
    fn width(&self, text: &str) -> f32;
}

/// The chip's box: padding either side, the icon, a gap, then the label.
#[derive(Clone, Copy, Debug)]
pub struct ChipMetrics {
    pub pad: f32,
    pub icon: f32,
    pub gap: f32,
}

impl Default for ChipMetrics {
    fn default() -> Self {
        Self {
            pad: 6.,
            icon: 13.,
            gap: 4.,
        }
    }
}

impl ChipMetrics {
    fn chrome(&self) -> f32 {
        self.pad * 2. + self.icon + self.gap
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// A run of text on one line: its buffer range, where it starts, and how wide it is.
    Text { range: Range<usize>, x: f32, w: f32 },
    /// A chip: which one, its marker's buffer offset, where it starts, and its box width. `label`
    /// is what fits: the whole label, or a shortened one ending in "…" when the chip is wider than
    /// the line.
    Chip {
        index: usize,
        at: usize,
        x: f32,
        w: f32,
        label: String,
    },
}

impl Piece {
    fn x(&self) -> f32 {
        match self {
            Piece::Text { x, .. } | Piece::Chip { x, .. } => *x,
        }
    }

    fn end_x(&self) -> f32 {
        match self {
            Piece::Text { x, w, .. } | Piece::Chip { x, w, .. } => x + w,
        }
    }

    fn range(&self) -> Range<usize> {
        match self {
            Piece::Text { range, .. } => range.clone(),
            Piece::Chip { at, .. } => *at..*at + MARK.len_utf8(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The buffer range the line shows. A newline that ends it is not in it.
    pub range: Range<usize>,
    pub pieces: Vec<Piece>,
    /// How wide the line's content is.
    pub width: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub lines: Vec<Line>,
    pub line_height: f32,
}

/// One thing to place: a word with the spaces after it, a chip, or a line break.
enum Item {
    Word(Range<usize>),
    Chip(usize, usize),
    Break(usize),
}

fn items(doc: &Doc) -> Vec<Item> {
    let text = doc.text();
    let mut items = Vec::new();
    let mut chip = 0;
    let mut word_start: Option<usize> = None;
    let mut in_space = false;
    for (at, ch) in text.char_indices() {
        if ch == '\n' || ch == MARK {
            if let Some(start) = word_start.take() {
                items.push(Item::Word(start..at));
            }
            in_space = false;
            if ch == '\n' {
                items.push(Item::Break(at));
            } else {
                items.push(Item::Chip(chip, at));
                chip += 1;
            }
            continue;
        }
        let space = ch.is_whitespace();
        match word_start {
            // A new word starts at the first letter after spaces; spaces belong to the word
            // before them, as a line's trailing spaces do.
            Some(start) if in_space && !space => {
                items.push(Item::Word(start..at));
                word_start = Some(at);
            }
            None => word_start = Some(at),
            _ => {}
        }
        in_space = space;
    }
    if let Some(start) = word_start {
        items.push(Item::Word(start..text.len()));
    }
    items
}

/// Lay the document out in lines no wider than `max_width`.
pub fn lay_out(
    doc: &Doc,
    measure: &dyn Measure,
    max_width: f32,
    line_height: f32,
    chip: ChipMetrics,
) -> Layout {
    let text = doc.text();
    let max_width = max_width.max(1.);
    let mut lines = Vec::new();
    let mut current = Line {
        range: 0..0,
        pieces: Vec::new(),
        width: 0.,
    };
    let mut x = 0.;

    fn push_text(line: &mut Line, range: Range<usize>, x: f32, w: f32) {
        if let Some(Piece::Text {
            range: last, w: lw, ..
        }) = line.pieces.last_mut()
            && last.end == range.start
        {
            last.end = range.end;
            *lw += w;
            return;
        }
        line.pieces.push(Piece::Text { range, x, w });
    }

    let mut finish = |line: &mut Line, end: usize, next_start: usize, x: &mut f32| {
        line.range.end = end;
        line.width = *x;
        let done = std::mem::replace(
            line,
            Line {
                range: next_start..next_start,
                pieces: Vec::new(),
                width: 0.,
            },
        );
        lines.push(done);
        *x = 0.;
    };

    for item in items(doc) {
        match item {
            Item::Break(at) => finish(&mut current, at, at + 1, &mut x),
            Item::Chip(index, at) => {
                let label = doc.chips().get(index).map_or("", |c| c.label.as_str());
                let natural = chip.chrome() + measure.width(label);
                let w = natural.min(max_width);
                if x > 0. && x + w > max_width {
                    finish(&mut current, at, at, &mut x);
                }
                let shown = if natural > max_width {
                    fit_label(label, max_width - chip.chrome(), measure)
                } else {
                    label.to_string()
                };
                current.pieces.push(Piece::Chip {
                    index,
                    at,
                    x,
                    w,
                    label: shown,
                });
                x += w;
            }
            Item::Word(range) => {
                let word = &text[range.clone()];
                let visible = word.trim_end();
                let visible_w = measure.width(visible);
                if x > 0. && x + visible_w > max_width {
                    finish(&mut current, range.start, range.start, &mut x);
                }
                if visible_w <= max_width {
                    let w = measure.width(word);
                    push_text(&mut current, range, x, w);
                    x += w;
                    continue;
                }
                // A word longer than a whole line is broken where it has to be, by grapheme.
                let mut start = range.start;
                for (offset, grapheme) in word.grapheme_indices(true) {
                    let at = range.start + offset;
                    let gw = measure.width(grapheme);
                    if x > 0. && x + gw > max_width && !grapheme.trim().is_empty() {
                        if at > start {
                            let w = measure.width(&text[start..at]);
                            push_text(&mut current, start..at, x - w, w);
                        }
                        finish(&mut current, at, at, &mut x);
                        start = at;
                    }
                    x += gw;
                }
                if range.end > start {
                    let w = measure.width(&text[start..range.end]);
                    push_text(&mut current, start..range.end, x - w, w);
                }
            }
        }
    }
    current.range.end = text.len();
    current.width = x;
    lines.push(current);
    Layout { lines, line_height }
}

/// The longest start of `label` that fits in `room`, with "…" on the end.
fn fit_label(label: &str, room: f32, measure: &dyn Measure) -> String {
    let mut out = String::new();
    for grapheme in label.graphemes(true) {
        let candidate = format!("{out}{grapheme}…");
        if measure.width(&candidate) > room && !out.is_empty() {
            break;
        }
        out.push_str(grapheme);
    }
    format!("{}…", out.trim_end())
}

impl Layout {
    pub fn height(&self) -> f32 {
        self.lines.len() as f32 * self.line_height
    }

    /// The line an offset is shown on. An offset where a line wraps belongs to the next line (the
    /// caret after the last letter of a wrapped line is drawn at the start of the next), except
    /// at the very end.
    pub fn line_of(&self, offset: usize) -> usize {
        let last = self.lines.len().saturating_sub(1);
        for (i, line) in self.lines.iter().enumerate() {
            if offset < line.range.end
                || (offset == line.range.end && (i == last || self.ends_with_break(i)))
            {
                return i;
            }
            if offset == line.range.end && i < last && self.lines[i + 1].range.start > offset {
                return i;
            }
        }
        last
    }

    fn ends_with_break(&self, i: usize) -> bool {
        self.lines
            .get(i + 1)
            .is_some_and(|next| next.range.start > self.lines[i].range.end)
    }

    /// Where the caret is drawn for an offset: x from the line's left, and the line's index.
    pub fn caret(&self, doc: &Doc, measure: &dyn Measure, offset: usize) -> (f32, usize) {
        let row = self.line_of(offset);
        (self.x_in_line(doc, measure, row, offset), row)
    }

    fn x_in_line(&self, doc: &Doc, measure: &dyn Measure, row: usize, offset: usize) -> f32 {
        let Some(line) = self.lines.get(row) else {
            return 0.;
        };
        for piece in &line.pieces {
            let range = piece.range();
            if offset <= range.start {
                return piece.x();
            }
            if offset < range.end {
                return match piece {
                    // `get`, not indexing: a layout from before an edit can be asked about the
                    // document after it, and a stale range must never panic (review of #133).
                    Piece::Text { range, x, .. } => {
                        x + doc
                            .text()
                            .get(range.start..offset)
                            .map_or(0., |text| measure.width(text))
                    }
                    // Inside a chip's marker cannot happen on a character boundary; before it is
                    // its left edge.
                    Piece::Chip { x, .. } => *x,
                };
            }
        }
        line.pieces.last().map_or(0., Piece::end_x)
    }

    /// The offset nearest a point, with (x, y) from the top-left of the text. A click on a chip's
    /// left half puts the caret before it, and on its right half after it: never inside.
    pub fn offset_at(&self, doc: &Doc, measure: &dyn Measure, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let row = ((y / self.line_height).floor().max(0.) as usize).min(self.lines.len() - 1);
        self.offset_in_line(doc, measure, row, x)
    }

    pub fn offset_in_line(&self, doc: &Doc, measure: &dyn Measure, row: usize, x: f32) -> usize {
        let Some(line) = self.lines.get(row) else {
            return doc.len();
        };
        for piece in &line.pieces {
            if x >= piece.end_x() {
                continue;
            }
            return match piece {
                Piece::Chip { at, x: px, w, .. } => {
                    if x < px + w / 2. {
                        *at
                    } else {
                        at + MARK.len_utf8()
                    }
                }
                Piece::Text { range, x: px, .. } => {
                    let Some(text) = doc.text().get(range.clone()) else {
                        return range.start.min(doc.len());
                    };
                    let mut best = range.start;
                    let mut best_d = f32::MAX;
                    for (offset, _) in text
                        .grapheme_indices(true)
                        .chain(std::iter::once((text.len(), "")))
                    {
                        let d = (px + measure.width(&text[..offset]) - x).abs();
                        if d < best_d {
                            best_d = d;
                            best = range.start + offset;
                        }
                    }
                    best
                }
            };
        }
        // Past the end of the line: its end, before any trailing space a wrap left there so the
        // caret does not appear at the start of the next line.
        let end = line.range.end;
        if row + 1 < self.lines.len() && self.lines[row + 1].range.start == end {
            self.wrap_end(doc, row)
        } else {
            end
        }
    }

    /// Where a wrapped line ends for the caret: before the space the wrap left at its end, so the
    /// caret stays on this line, and at the end itself when the wrap broke a word or came before
    /// a chip — stepping back there would leave the last letter behind (review of #133).
    pub fn wrap_end(&self, doc: &Doc, row: usize) -> usize {
        let Some(line) = self.lines.get(row) else {
            return doc.len();
        };
        let end = line.range.end.min(doc.len());
        let trailing_space = doc
            .text()
            .get(line.range.start.min(end)..end)
            .and_then(|text| text.chars().next_back())
            .is_some_and(char::is_whitespace);
        if trailing_space {
            doc.prev_boundary(end).max(line.range.start)
        } else {
            end
        }
    }

    /// The rectangles a buffer range covers, one per line: (x, row, width).
    pub fn rects(
        &self,
        doc: &Doc,
        measure: &dyn Measure,
        range: Range<usize>,
    ) -> Vec<(f32, usize, f32)> {
        let mut out = Vec::new();
        for (row, line) in self.lines.iter().enumerate() {
            let start = range.start.max(line.range.start);
            let end = range.end.min(line.range.end);
            if start > end || (start == end && range.start != range.end) {
                continue;
            }
            let from = self.x_in_line(doc, measure, row, start);
            let mut to = self.x_in_line(doc, measure, row, end);
            // A selection that runs on past this line shows it reaching the line's end.
            if range.end > line.range.end {
                to = to.max(line.width);
            }
            if to > from {
                out.push((from, row, to - from));
            }
        }
        out
    }

    /// The offset on the row above or below at the same x, for the up and down arrows.
    pub fn vertical(
        &self,
        doc: &Doc,
        measure: &dyn Measure,
        offset: usize,
        goal_x: f32,
        down: bool,
    ) -> Option<usize> {
        let row = self.line_of(offset);
        let target = if down {
            (row + 1 < self.lines.len()).then_some(row + 1)?
        } else {
            row.checked_sub(1)?
        };
        Some(self.offset_in_line(doc, measure, target, goal_x))
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::chip;
    use super::*;
    use crate::components::chat_input::TokenKind;

    /// Ten pixels a character, the chip's chrome aside.
    struct Mono;
    impl Measure for Mono {
        fn width(&self, text: &str) -> f32 {
            text.chars().count() as f32 * 10.
        }
    }

    const CHROME: ChipMetrics = ChipMetrics {
        pad: 5.,
        icon: 10.,
        gap: 0.,
    };

    fn doc(text: &str, chip_at: Option<(usize, &str)>) -> Doc {
        let mut doc = Doc::from_plain(text);
        if let Some((at, label)) = chip_at {
            doc.insert_chip(at, chip(TokenKind::Recipe, label));
        }
        doc
    }

    fn line_texts(doc: &Doc, layout: &Layout) -> Vec<String> {
        layout
            .lines
            .iter()
            .map(|line| doc.plain_of(line.range.clone()))
            .collect()
    }

    /// A chip takes room in the line: the word after it starts after its box, not under it.
    #[test]
    fn a_chip_takes_its_own_width() {
        let d = doc("ab  cd", Some((3, "xyz")));
        let layout = lay_out(&d, &Mono, 1000., 20., CHROME);
        let pieces = &layout.lines[0].pieces;
        let Piece::Chip { x, w, .. } = &pieces[1] else {
            panic!("{pieces:?}");
        };
        assert_eq!((*x, *w), (30., 50.), "20 of chrome + 3 letters");
        let Piece::Text { x, .. } = &pieces[2] else {
            panic!("{pieces:?}");
        };
        assert_eq!(*x, 80., "the next text starts after the chip");
    }

    /// A chip that does not fit at the end of a line goes to the next line whole.
    #[test]
    fn a_chip_that_does_not_fit_wraps_as_one() {
        let d = doc("hello world ", Some((12, "report")));
        let layout = lay_out(&d, &Mono, 150., 20., CHROME);
        assert_eq!(line_texts(&d, &layout), vec!["hello world ", "report"]);
        let (x, row) = layout.caret(&d, &Mono, 12);
        assert_eq!(
            (x, row),
            (0., 1),
            "the caret before the chip is on the chip's line"
        );
    }

    /// A chip wider than the whole line is shortened with "…" to fit, rather than overflowing.
    #[test]
    fn a_chip_wider_than_the_line_is_shortened() {
        let d = doc("", Some((0, "a very long recipe name")));
        let layout = lay_out(&d, &Mono, 100., 20., CHROME);
        let Piece::Chip { w, label, .. } = &layout.lines[0].pieces[0] else {
            panic!();
        };
        assert_eq!(*w, 100.);
        assert!(
            label.ends_with('…') && label.chars().count() <= 8,
            "{label}"
        );
    }

    /// A click on a chip's left half lands before it and on its right half after it.
    #[test]
    fn a_click_on_a_chip_lands_beside_it() {
        let d = doc("ab ", Some((3, "xyz")));
        let layout = lay_out(&d, &Mono, 1000., 20., CHROME);
        assert_eq!(layout.offset_at(&d, &Mono, 35., 5.), 3);
        assert_eq!(layout.offset_at(&d, &Mono, 70., 5.), 3 + MARK.len_utf8());
        assert_eq!(
            layout.offset_at(&d, &Mono, 11., 5.),
            1,
            "text still hits by letter"
        );
    }

    /// Lines break at words; a newline is a line of its own even when empty; the caret after a
    /// trailing newline is on the empty last line.
    #[test]
    fn words_and_newlines_make_lines() {
        let d = doc("one two three\n", None);
        let layout = lay_out(&d, &Mono, 80., 20., CHROME);
        assert_eq!(line_texts(&d, &layout), vec!["one two ", "three", ""]);
        assert_eq!(layout.caret(&d, &Mono, d.len()), (0., 2));
        assert_eq!(
            layout.caret(&d, &Mono, 8),
            (0., 1),
            "at a wrap: the next line's start"
        );
        assert_eq!(layout.height(), 60.);
    }

    /// A selection across a wrap and a chip is one rectangle per line, and the chip is inside it.
    #[test]
    fn a_selection_across_lines_is_a_rect_per_line() {
        let d = doc("hello world ", Some((12, "abc")));
        let layout = lay_out(&d, &Mono, 150., 20., CHROME);
        let rects = layout.rects(&d, &Mono, 6..d.len());
        assert_eq!(rects, vec![(60., 0, 60.), (0., 1, 50.)]);
    }

    /// Up and down keep the column.
    #[test]
    fn up_and_down_keep_the_column() {
        let d = doc("abcdef ghijkl", None);
        let layout = lay_out(&d, &Mono, 70., 20., CHROME);
        assert_eq!(layout.vertical(&d, &Mono, 2, 20., true), Some(9));
        assert_eq!(layout.vertical(&d, &Mono, 9, 20., false), Some(2));
        assert_eq!(layout.vertical(&d, &Mono, 9, 20., true), None);
    }

    /// End on a line that wrapped inside a word goes to the line's real end, and on a line that
    /// wrapped at a space, to before the space.
    #[test]
    fn a_wrapped_line_ends_where_its_letters_do() {
        let d = doc("abcdefghij", None);
        let layout = lay_out(&d, &Mono, 40., 20., CHROME);
        assert_eq!(
            layout.wrap_end(&d, 0),
            4,
            "no space: nothing is left behind"
        );
        let d = doc("abc defg", None);
        let layout = lay_out(&d, &Mono, 50., 20., CHROME);
        assert_eq!(layout.wrap_end(&d, 0), 3, "before the space the wrap left");
    }

    /// A layout asked about a document it was not made from answers without panicking.
    #[test]
    fn a_stale_layout_never_panics() {
        let before = doc("中文中文sh中文", None);
        let layout = lay_out(&before, &Mono, 60., 20., CHROME);
        let after = doc("中文中文し中文", None);
        for offset in 0..=after.len() {
            let offset = after.floor_char(offset);
            let _ = layout.caret(&after, &Mono, offset);
            let _ = layout.rects(&after, &Mono, 0..offset);
        }
        let _ = layout.offset_at(&after, &Mono, 35., 25.);
    }

    /// A word longer than a line is broken inside, and nothing is lost.
    #[test]
    fn a_word_longer_than_a_line_breaks_inside() {
        let d = doc("abcdefghij", None);
        let layout = lay_out(&d, &Mono, 40., 20., CHROME);
        assert_eq!(line_texts(&d, &layout), vec!["abcd", "efgh", "ij"]);
    }
}
