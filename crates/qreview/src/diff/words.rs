//! What changed inside a line.
//!
//! A row that differs from its opposite by one word reads much better when
//! only that word is marked. The work is done in the server, once, and the
//! browser only paints the spans.

use similar::{ChangeTag, TextDiff};

use crate::model::{Hunk, Row, RowKind, WordSpan};

/// Below this share of shared text, the two lines are not versions of each
/// other. Marking every word then is noise, so the whole line stays plain.
const MIN_SHARED: f32 = 0.25;

/// Mark the intra-line changes of every removed and added row of a hunk.
pub fn mark(hunk: &mut Hunk) {
    let mut index = 0;

    while index < hunk.rows.len() {
        let removes = run(&hunk.rows, index, RowKind::Remove);
        let adds = run(&hunk.rows, index + removes, RowKind::Add);

        if removes == 0 || adds == 0 {
            index += (removes + adds).max(1);
            continue;
        }

        // Pair them in order, the way the reader reads them. A run of three
        // removes and two adds leaves the third remove unpaired.
        for offset in 0..removes.min(adds) {
            let old = index + offset;
            let new = index + removes + offset;
            let (old_spans, new_spans) = compare(&hunk.rows[old].text, &hunk.rows[new].text);
            hunk.rows[old].words = old_spans;
            hunk.rows[new].words = new_spans;
        }
        index += removes + adds;
    }
}

/// How many rows of that kind start at `from`.
fn run(rows: &[Row], from: usize, kind: RowKind) -> usize {
    rows.iter()
        .skip(from)
        .take_while(|r| r.kind == kind)
        .count()
}

/// The byte ranges that differ, on the old side and on the new side.
fn compare(old: &str, new: &str) -> (Vec<WordSpan>, Vec<WordSpan>) {
    if old == new {
        return (Vec::new(), Vec::new());
    }

    // Unicode word segmentation, not whitespace: it separates punctuation,
    // so `two;` and `three;` differ by `two`, not by the whole token.
    let diff = TextDiff::from_unicode_words(old, new);
    let mut old_spans = Vec::new();
    let mut new_spans = Vec::new();
    let (mut o, mut n, mut shared) = (0usize, 0usize, 0usize);

    for change in diff.iter_all_changes() {
        let len = change.value().len();
        match change.tag() {
            ChangeTag::Equal => {
                shared += len;
                o += len;
                n += len;
            }
            ChangeTag::Delete => {
                old_spans.push(WordSpan {
                    start: o,
                    end: o + len,
                });
                o += len;
            }
            ChangeTag::Insert => {
                new_spans.push(WordSpan {
                    start: n,
                    end: n + len,
                });
                n += len;
            }
        }
    }

    let longest = old.len().max(new.len()).max(1);
    if (shared as f32) / (longest as f32) < MIN_SHARED {
        return (Vec::new(), Vec::new());
    }

    (join(old_spans), join(new_spans))
}

/// Merge spans that touch, so one word split into three tokens is one mark.
fn join(spans: Vec<WordSpan>) -> Vec<WordSpan> {
    let mut out: Vec<WordSpan> = Vec::with_capacity(spans.len());

    for span in spans {
        match out.last_mut() {
            Some(last) if last.end == span.start => last.end = span.end,
            _ => out.push(span),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: RowKind, text: &str) -> Row {
        Row {
            kind,
            old_line: None,
            new_line: None,
            text: text.to_owned(),
            no_newline: false,
            tokens: Vec::new(),
            words: Vec::new(),
        }
    }

    fn hunk(rows: Vec<Row>) -> Hunk {
        Hunk {
            old_start: 1,
            old_lines: 0,
            new_start: 1,
            new_lines: 0,
            header: String::new(),
            rows,
        }
    }

    fn marked(row: &Row) -> Vec<&str> {
        row.words
            .iter()
            .map(|w| &row.text[w.start..w.end])
            .collect()
    }

    #[test]
    fn one_changed_word_is_the_only_mark() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "let total = one + two;"),
            row(RowKind::Add, "let total = one + three;"),
        ]);
        mark(&mut h);

        assert_eq!(marked(&h.rows[0]), ["two"]);
        assert_eq!(marked(&h.rows[1]), ["three"]);
    }

    #[test]
    fn a_line_that_changed_completely_carries_no_mark() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "int a = compute(x);"),
            row(RowKind::Add, "return None;"),
        ]);
        mark(&mut h);

        assert!(h.rows[0].words.is_empty(), "{:?}", marked(&h.rows[0]));
        assert!(h.rows[1].words.is_empty());
    }

    #[test]
    fn a_context_row_is_never_marked() {
        let mut h = hunk(vec![
            row(RowKind::Context, "unchanged"),
            row(RowKind::Remove, "a b c"),
            row(RowKind::Add, "a B c"),
            row(RowKind::Context, "also unchanged"),
        ]);
        mark(&mut h);

        assert!(h.rows[0].words.is_empty());
        assert!(h.rows[3].words.is_empty());
        assert_eq!(marked(&h.rows[1]), ["b"]);
    }

    #[test]
    fn a_removal_with_no_addition_is_left_alone() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "gone"),
            row(RowKind::Context, "kept"),
        ]);
        mark(&mut h);

        assert!(h.rows[0].words.is_empty());
    }

    #[test]
    fn rows_are_paired_in_order() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "alpha one"),
            row(RowKind::Remove, "beta two"),
            row(RowKind::Add, "alpha ONE"),
            row(RowKind::Add, "beta TWO"),
        ]);
        mark(&mut h);

        assert_eq!(marked(&h.rows[0]), ["one"]);
        assert_eq!(marked(&h.rows[1]), ["two"]);
        assert_eq!(marked(&h.rows[2]), ["ONE"]);
        assert_eq!(marked(&h.rows[3]), ["TWO"]);
    }

    #[test]
    fn a_third_removal_with_only_two_additions_stays_unpaired() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "a one"),
            row(RowKind::Remove, "b two"),
            row(RowKind::Remove, "c three"),
            row(RowKind::Add, "a ONE"),
            row(RowKind::Add, "b TWO"),
        ]);
        mark(&mut h);

        assert_eq!(marked(&h.rows[0]), ["one"]);
        assert_eq!(marked(&h.rows[1]), ["two"]);
        assert!(h.rows[2].words.is_empty());
    }

    #[test]
    fn the_unchanged_argument_stays_plain() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "value = alpha.beta(1)"),
            row(RowKind::Add, "value = gamma.delta(1)"),
        ]);
        mark(&mut h);

        // Unicode word segmentation keeps a dot between letters inside the
        // word, so `alpha.beta` is one token. The call and its argument did
        // not change, and they stay plain, which is the part that matters.
        assert_eq!(marked(&h.rows[0]), ["alpha.beta"]);
        assert_eq!(marked(&h.rows[1]), ["gamma.delta"]);
    }

    #[test]
    fn touching_spans_become_one_mark() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "n = 1+2 end"),
            row(RowKind::Add, "n = 3-4 end"),
        ]);
        mark(&mut h);

        // Three tokens changed in a row. That is one mark, not three.
        assert_eq!(marked(&h.rows[0]), ["1+2"]);
    }

    #[test]
    fn a_span_is_a_byte_range_of_its_own_text() {
        let mut h = hunk(vec![
            row(RowKind::Remove, "greeting = \"héllo wörld\""),
            row(RowKind::Add, "greeting = \"héllo there\""),
        ]);
        mark(&mut h);

        // Slicing must not panic on a multi-byte character.
        for row in &h.rows {
            for span in &row.words {
                assert!(row.text.get(span.start..span.end).is_some(), "{span:?}");
            }
        }
        assert_eq!(marked(&h.rows[0]), ["wörld"]);
    }
}
