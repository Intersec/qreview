//! Offsets the browser can slice with.
//!
//! Rust counts bytes, JavaScript counts UTF-16 code units. A line with one
//! accent in it makes every span after that point land in the wrong place, so
//! the offsets are converted once, where a row leaves the server.

use crate::model::{Row, Span, WordSpan};

/// Convert every span of a row from byte offsets to UTF-16 offsets.
pub fn to_utf16(row: &mut Row) {
    // An ASCII line is the common case and needs no work at all.
    if row.text.is_ascii() {
        return;
    }

    let map = Map::of(&row.text);
    for span in &mut row.tokens {
        *span = Span {
            start: map.at(span.start),
            end: map.at(span.end),
            cls: std::mem::take(&mut span.cls),
        };
    }
    for span in &mut row.words {
        *span = WordSpan {
            start: map.at(span.start),
            end: map.at(span.end),
        };
    }
}

/// Byte offset to UTF-16 offset, for one line.
struct Map {
    /// One entry per character boundary: the byte index and the UTF-16 index.
    points: Vec<(usize, usize)>,
    end: usize,
}

impl Map {
    fn of(text: &str) -> Self {
        let mut points = Vec::with_capacity(text.len() + 1);
        let mut utf16 = 0;

        for (byte, ch) in text.char_indices() {
            points.push((byte, utf16));
            utf16 += ch.len_utf16();
        }
        points.push((text.len(), utf16));

        Self { points, end: utf16 }
    }

    fn at(&self, byte: usize) -> usize {
        match self.points.binary_search_by_key(&byte, |(b, _)| *b) {
            Ok(index) => self.points[index].1,
            // An offset inside a character can only come from a bug. Round to
            // the boundary before it rather than move the whole line.
            Err(0) => 0,
            Err(index) => self
                .points
                .get(index - 1)
                .map(|(_, u)| *u)
                .unwrap_or(self.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RowKind;

    fn row(text: &str, tokens: &[(usize, usize)], words: &[(usize, usize)]) -> Row {
        Row {
            kind: RowKind::Add,
            old_line: None,
            new_line: Some(1),
            text: text.to_owned(),
            no_newline: false,
            tokens: tokens
                .iter()
                .map(|(s, e)| Span {
                    start: *s,
                    end: *e,
                    cls: "string".to_owned(),
                })
                .collect(),
            words: words
                .iter()
                .map(|(s, e)| WordSpan { start: *s, end: *e })
                .collect(),
        }
    }

    #[test]
    fn an_ascii_line_is_unchanged() {
        let mut r = row("let a = 1;", &[(4, 5)], &[(8, 9)]);
        to_utf16(&mut r);

        assert_eq!((r.tokens[0].start, r.tokens[0].end), (4, 5));
        assert_eq!((r.words[0].start, r.words[0].end), (8, 9));
    }

    #[test]
    fn an_accent_moves_the_offsets_after_it() {
        // "é" is two bytes and one UTF-16 unit.
        let text = "héllo world";
        let byte_start = text.find("world").unwrap();
        let mut r = row(text, &[(byte_start, text.len())], &[]);
        to_utf16(&mut r);

        assert_eq!(byte_start, 7, "the byte offset counts é as two");
        assert_eq!(r.tokens[0].start, 6, "the browser counts it as one");
        assert_eq!(r.tokens[0].end, 11);
    }

    #[test]
    fn an_emoji_counts_as_two_in_the_browser() {
        // A character outside the basic plane is one char, four bytes, and
        // two UTF-16 units.
        let text = "x 🚀 y";
        let byte_start = text.find(" y").unwrap();
        let mut r = row(text, &[(byte_start, text.len())], &[]);
        to_utf16(&mut r);

        assert_eq!(byte_start, 6);
        assert_eq!(r.tokens[0].start, 4, "one x, one space, two for the rocket");
    }

    #[test]
    fn the_class_survives_the_conversion() {
        let mut r = row("héllo", &[(0, 6)], &[]);
        to_utf16(&mut r);

        assert_eq!(r.tokens[0].cls, "string");
    }

    #[test]
    fn an_offset_inside_a_character_rounds_down() {
        let mut r = row("héllo", &[(2, 6)], &[]);
        to_utf16(&mut r);

        // Byte 2 is the second byte of é. It becomes the boundary before it.
        assert_eq!(r.tokens[0].start, 1);
    }
}
