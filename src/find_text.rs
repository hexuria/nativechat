use std::ops::Range;

/// One case-insensitive hit inside a transcript row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindHit {
    pub row: usize,
    pub range: Range<usize>,
}

/// Case-insensitive, non-overlapping substring ranges in `haystack`.
///
/// Offsets are UTF-8 bytes into the original string. Empty / whitespace-only
/// needles yield no hits — Grok's find bar stays idle until you type a word.
pub fn find_ranges(haystack: &str, needle: &str) -> Vec<Range<usize>> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    let needle_chars: Vec<char> = needle.chars().collect();
    let hay: Vec<(usize, char)> = haystack.char_indices().collect();
    let nlen = needle_chars.len();
    if nlen == 0 || hay.len() < nlen {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + nlen <= hay.len() {
        let matched = (0..nlen).all(|k| eq_ignore_case(hay[i + k].1, needle_chars[k]));
        if matched {
            let start = hay[i].0;
            let end = if i + nlen < hay.len() {
                hay[i + nlen].0
            } else {
                haystack.len()
            };
            out.push(start..end);
            i += nlen;
        } else {
            i += 1;
        }
    }
    out
}

fn eq_ignore_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

pub fn project_hits(texts: impl IntoIterator<Item = impl AsRef<str>>, query: &str) -> Vec<FindHit> {
    let mut hits = Vec::new();
    for (row, text) in texts.into_iter().enumerate() {
        for range in find_ranges(text.as_ref(), query) {
            hits.push(FindHit { row, range });
        }
    }
    hits
}

pub fn marks_for_row(
    row: usize,
    hits: &[FindHit],
    current: Option<usize>,
) -> Vec<(Range<usize>, bool)> {
    hits.iter()
        .enumerate()
        .filter(|(_, hit)| hit.row == row)
        .map(|(index, hit)| (hit.range.clone(), current == Some(index)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_ascii_case_insensitive() {
        assert_eq!(
            find_ranges("Hello egress EGRESS", "egress"),
            vec![6..12, 13..19]
        );
    }

    #[test]
    fn skips_blank_needles() {
        assert!(find_ranges("hello", "  ").is_empty());
        assert!(find_ranges("hello", "").is_empty());
    }

    #[test]
    fn non_overlapping_windows() {
        assert_eq!(find_ranges("aaaa", "aa"), vec![0..2, 2..4]);
    }

    #[test]
    fn projects_per_row_occurrence() {
        let hits = project_hits(["alpha", "egress once egress", "nope"], "egress");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].row, 1);
        assert_eq!(hits[1].row, 1);
        let marks = marks_for_row(1, &hits, Some(1));
        assert_eq!(marks.len(), 2);
        assert!(!marks[0].1);
        assert!(marks[1].1);
    }
}
