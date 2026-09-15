//! Pure text helpers for chat rows and TTS highlighting. No GPUI.

use std::ops::Range;

/// Byte budget for one virtual list row.
pub const CHAT_ROW_CHUNK_BYTES: usize = 480;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChunk {
    pub text: String,
    pub byte_start: usize,
}

pub fn looks_like_markdown(text: &str) -> bool {
    text.contains("```")
        || text.contains("](")
        || text.contains("![")
        || text.contains("**")
        || text.starts_with('#')
        || text.contains("\n#")
        || text.contains("\n- ")
        || text.contains("\n* ")
        || text.contains("\n1. ")
}

pub fn map_utf16_range_to_utf8(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let mut utf16_index = 0;
    let mut utf8_start = None;

    if range.start == 0 {
        utf8_start = Some(0);
    }

    for (i, c) in text.char_indices() {
        if utf16_index == range.start {
            utf8_start = Some(i);
        }
        if utf16_index == range.end {
            return utf8_start.map(|start| start..i);
        }
        utf16_index += c.len_utf16();
    }

    if range.start == utf16_index {
        utf8_start = utf8_start.or(Some(text.len()));
    }
    if utf16_index == range.end {
        return utf8_start.map(|start| start..text.len());
    }

    None
}

/// Shift a full-message byte range into a chunk, or `None` if they do not overlap.
pub fn highlight_in_chunk(
    full_range: &Range<usize>,
    chunk_start: usize,
    chunk_len: usize,
) -> Option<Range<usize>> {
    let chunk_end = chunk_start + chunk_len;
    let start = full_range.start.max(chunk_start);
    let end = full_range.end.min(chunk_end);
    if start >= end {
        return None;
    }
    Some((start - chunk_start)..(end - chunk_start))
}

pub fn chunk_text(text: &str, budget: usize) -> Vec<TextChunk> {
    if text.len() <= budget {
        return vec![TextChunk {
            text: text.to_string(),
            byte_start: 0,
        }];
    }

    let mut out = Vec::new();
    let mut buf_start = 0;
    let mut buf = String::new();
    let mut search = 0;
    while search <= text.len() {
        let next_para = text[search..]
            .find("\n\n")
            .map(|i| search + i)
            .unwrap_or(text.len());
        let para = &text[search..next_para];
        if para.len() > budget {
            if !buf.is_empty() {
                out.push(TextChunk {
                    text: std::mem::take(&mut buf),
                    byte_start: buf_start,
                });
            }
            out.extend(chunk_wrapped(para, search, budget));
        } else {
            if !buf.is_empty() && buf.len() + 2 + para.len() > budget {
                out.push(TextChunk {
                    text: std::mem::take(&mut buf),
                    byte_start: buf_start,
                });
            }
            if buf.is_empty() {
                buf_start = search;
            } else {
                buf.push_str("\n\n");
            }
            buf.push_str(para);
        }
        if next_para == text.len() {
            break;
        }
        search = next_para + 2;
    }
    if !buf.is_empty() {
        out.push(TextChunk {
            text: buf,
            byte_start: buf_start,
        });
    }
    if out.is_empty() {
        vec![TextChunk {
            text: text.to_string(),
            byte_start: 0,
        }]
    } else {
        out
    }
}

fn chunk_wrapped(text: &str, origin: usize, budget: usize) -> Vec<TextChunk> {
    if text.len() <= budget {
        return vec![TextChunk {
            text: text.to_string(),
            byte_start: origin,
        }];
    }
    if text.contains('\n') {
        let mut out = Vec::new();
        let mut buf = String::new();
        let mut buf_start = origin;
        let mut line_origin = origin;
        for line in text.split('\n') {
            if line.len() > budget {
                if !buf.is_empty() {
                    out.push(TextChunk {
                        text: std::mem::take(&mut buf),
                        byte_start: buf_start,
                    });
                }
                out.extend(split_hard(line, line_origin, budget));
            } else {
                if !buf.is_empty() && buf.len() + 1 + line.len() > budget {
                    out.push(TextChunk {
                        text: std::mem::take(&mut buf),
                        byte_start: buf_start,
                    });
                }
                if buf.is_empty() {
                    buf_start = line_origin;
                } else {
                    buf.push('\n');
                }
                buf.push_str(line);
            }
            line_origin += line.len() + 1;
        }
        if !buf.is_empty() {
            out.push(TextChunk {
                text: buf,
                byte_start: buf_start,
            });
        }
        return out;
    }
    split_hard(text, origin, budget)
}

fn split_hard(text: &str, origin: usize, budget: usize) -> Vec<TextChunk> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        if text.len() - start <= budget {
            out.push(TextChunk {
                text: text[start..].to_string(),
                byte_start: origin + start,
            });
            break;
        }
        let mut end = start + budget;
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        let bytes = text.as_bytes();
        let mut split = end;
        while split > start && bytes[split] != b' ' {
            split -= 1;
            while split > start && !text.is_char_boundary(split) {
                split -= 1;
            }
        }
        if split == start {
            split = end;
        }
        if split > start {
            out.push(TextChunk {
                text: text[start..split].to_string(),
                byte_start: origin + start,
            });
        }
        start = split;
        while start < text.len() && text.as_bytes()[start] == b' ' {
            start += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_ascii() {
        assert_eq!(map_utf16_range_to_utf8("hello", 1..4), Some(1..4));
    }

    #[test]
    fn utf16_emoji() {
        // "hi👍!" — 👍 is one char, two UTF-16 units
        let text = "hi👍!";
        let thumbs = text.find('👍').unwrap();
        let utf16_start = text[..thumbs].chars().map(|c| c.len_utf16()).sum::<usize>();
        let utf16_end = utf16_start + '👍'.len_utf16();
        assert_eq!(
            map_utf16_range_to_utf8(text, utf16_start..utf16_end),
            Some(thumbs..thumbs + '👍'.len_utf8())
        );
    }

    #[test]
    fn utf16_cjk() {
        let text = "你好";
        assert_eq!(map_utf16_range_to_utf8(text, 0..1), Some(0..3));
    }

    #[test]
    fn utf16_empty_and_end() {
        assert_eq!(map_utf16_range_to_utf8("", 0..0), Some(0..0));
        assert_eq!(map_utf16_range_to_utf8("ab", 2..2), Some(2..2));
        assert_eq!(map_utf16_range_to_utf8("ab", 0..2), Some(0..2));
    }

    #[test]
    fn highlight_lands_on_owning_chunk() {
        let text = "a".repeat(600);
        let chunks = chunk_text(&text, CHAT_ROW_CHUNK_BYTES);
        assert!(chunks.len() >= 2);
        let range = 10..20;
        let hits: Vec<_> = chunks
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                highlight_in_chunk(&range, c.byte_start, c.text.len()).map(|r| (i, r))
            })
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, 0);
        assert_eq!(hits[0].1, 10..20);
        assert!(highlight_in_chunk(&range, chunks[1].byte_start, chunks[1].text.len()).is_none());
    }
}
