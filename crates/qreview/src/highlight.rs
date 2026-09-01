//! Syntax colors, computed in the server.
//!
//! The browser receives rows that already carry their spans, so it never
//! downloads a grammar and a large file costs it the text and nothing else.
//! The output is a CSS class, never a color: a theme is a stylesheet, and the
//! light and dark pair costs one pass, not two.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use syntect::parsing::{ParseState, ScopeStack, SyntaxSet, SyntaxSetBuilder};
use syntect::util::LinesWithEndings;

use crate::model::Span;

/// Above this size, a file is shown without colors.
///
/// Highlighting is linear, but a very large file still costs seconds and
/// megabytes of spans that nobody reads.
pub const MAX_BYTES: usize = 512 * 1024;

/// The scopes that say nothing about a token. `source.c` is on every line.
const GENERIC: [&str; 3] = ["source", "text", "meta"];

/// One highlighted file: the spans of every line, in order.
pub type Lines = Arc<Vec<Vec<Span>>>;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    /// Keyed by blob hash. Two patch sets that share a file highlight it once.
    cache: Mutex<HashMap<String, Lines>>,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntaxes: two_face::syntax::extra_newlines(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The same, plus every grammar file in a directory.
    ///
    /// A grammar is data. A house file format needs no change to this
    /// repository and no rebuild of the binary.
    pub fn with_grammars(dir: &Path) -> Self {
        let mut builder: SyntaxSetBuilder = two_face::syntax::extra_newlines().into_builder();

        // A broken grammar file must not stop the tool from starting.
        if let Err(error) = builder.add_from_folder(dir, true) {
            eprintln!(
                "qreview: cannot read the grammars in {}: {error}",
                dir.display()
            );
        }

        Self {
            syntaxes: builder.build(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The spans of every line of a file.
    ///
    /// `blob` is the git hash of the content, which is what the cache is
    /// keyed by. `language` comes from the map, and `path` is the fallback.
    pub fn lines(&self, blob: &str, text: &str, language: Option<&str>, path: &str) -> Lines {
        if let Some(hit) = self.cache.lock().unwrap().get(blob) {
            crate::trace::note(|| format!("highlight {path}, from the cache"));
            return hit.clone();
        }

        let started = crate::trace::start();
        let lines = Arc::new(self.compute(text, language, path));
        crate::trace::since(started, || {
            format!(
                "highlight {path}, {} bytes, {} lines",
                text.len(),
                lines.len()
            )
        });

        self.cache
            .lock()
            .unwrap()
            .insert(blob.to_owned(), lines.clone());

        lines
    }

    fn compute(&self, text: &str, language: Option<&str>, path: &str) -> Vec<Vec<Span>> {
        if text.len() > MAX_BYTES {
            return Vec::new();
        }

        let syntax = language
            .and_then(|name| self.syntaxes.find_syntax_by_token(name))
            .or_else(|| self.syntaxes.find_syntax_by_extension(extension(path)))
            .or_else(|| {
                self.syntaxes
                    .find_syntax_by_first_line(text.lines().next()?)
            })
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());

        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut out = Vec::new();

        for line in LinesWithEndings::from(text) {
            let Ok(ops) = state.parse_line(line, &self.syntaxes) else {
                // A grammar that fails on one line still colors the rest.
                out.push(Vec::new());
                continue;
            };
            out.push(spans_of(line, &ops, &mut stack));
        }
        out
    }
}

fn spans_of(
    line: &str,
    ops: &[(usize, syntect::parsing::ScopeStackOp)],
    stack: &mut ScopeStack,
) -> Vec<Span> {
    // The newline is not part of the row text the interface shows.
    let end_of_text = line.trim_end_matches(['\n', '\r']).len();
    let mut spans: Vec<Span> = Vec::new();
    let mut last = 0usize;

    for (pos, op) in ops {
        let pos = (*pos).min(end_of_text);
        if pos > last
            && let Some(cls) = class_of(stack)
        {
            push(&mut spans, last, pos, cls);
        }
        if stack.apply(op).is_err() {
            break;
        }
        last = last.max(pos);
    }

    if last < end_of_text
        && let Some(cls) = class_of(stack)
    {
        push(&mut spans, last, end_of_text, cls);
    }
    spans
}

/// Add a span, merging it into the one before when the class is the same.
fn push(spans: &mut Vec<Span>, start: usize, end: usize, cls: String) {
    match spans.last_mut() {
        Some(last) if last.end == start && last.cls == cls => last.end = end,
        _ => spans.push(Span { start, end, cls }),
    }
}

/// The class of the most specific scope that says something.
///
/// Every class is prefixed. A scope atom is an ordinary English word, and
/// the interface is built on a framework whose utilities are ordinary
/// English words too: `comment.block.documentation` became `comment block`,
/// and `block` sets `display: block`, so every doc comment broke its own
/// line in two.
fn class_of(stack: &ScopeStack) -> Option<String> {
    for scope in stack.as_slice().iter().rev() {
        let name = scope.build_string();
        let mut atoms = name.split('.');
        let Some(first) = atoms.next() else {
            continue;
        };
        if GENERIC.contains(&first) {
            continue;
        }
        return match atoms.next() {
            Some(second) => Some(format!("tok-{first} tok-{second}")),
            None => Some(format!("tok-{first}")),
        };
    }
    None
}

fn extension(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str, language: Option<&str>, path: &str) -> Vec<Vec<Span>> {
        let h = Highlighter::new();

        (*h.lines("blob-of-the-test", text, language, path)).clone()
    }

    fn classes(line: &[Span], text: &str) -> Vec<(String, String)> {
        line.iter()
            .map(|s| (s.cls.clone(), text[s.start..s.end].to_owned()))
            .collect()
    }

    #[test]
    fn a_keyword_and_a_string_get_their_classes() {
        let text = "int main(void) {\n    return \"hi\";\n}\n";
        let out = spans(text, Some("c"), "a.c");

        let second: Vec<_> = out[1].iter().map(|s| s.cls.clone()).collect();
        assert!(
            second.iter().any(|c| c.starts_with("tok-keyword")),
            "no keyword class: {second:?}"
        );
        assert!(
            second.iter().any(|c| c.starts_with("tok-string")),
            "no string class: {second:?}"
        );
    }

    #[test]
    fn a_span_is_a_byte_range_of_its_own_line() {
        let text = "int x = 1;\nchar *s = \"héllo\";\n";
        let out = spans(text, Some("c"), "a.c");

        for (index, line) in text.lines().enumerate() {
            for span in &out[index] {
                assert!(
                    line.get(span.start..span.end).is_some(),
                    "line {index} span {span:?} does not slice {line:?}"
                );
            }
        }
    }

    #[test]
    fn the_newline_is_never_inside_a_span() {
        let text = "// a comment\n";
        let out = spans(text, Some("c"), "a.c");
        let longest = out[0].iter().map(|s| s.end).max().unwrap_or(0);

        assert_eq!(longest, "// a comment".len());
    }

    #[test]
    fn a_block_comment_stays_a_comment_on_the_second_line() {
        let text = "/* one\n   two */\nint x;\n";
        let out = spans(text, Some("c"), "a.c");
        let second = classes(&out[1], "   two */");

        assert!(
            second.iter().any(|(cls, _)| cls.starts_with("tok-comment")),
            "the parser must carry the state across lines: {second:?}"
        );
    }

    #[test]
    fn the_language_of_the_map_wins_over_the_extension() {
        // .blk is not a C extension anywhere. The map says it is C.
        let text = "int x = 1;\n";
        let out = spans(text, Some("c"), "net.blk");
        let first: Vec<_> = out[0].iter().map(|s| s.cls.clone()).collect();

        assert!(
            first
                .iter()
                .any(|c| c.starts_with("tok-storage") || c.starts_with("tok-keyword")),
            "{first:?}"
        );
    }

    #[test]
    fn an_unknown_language_is_plain_and_does_not_fail() {
        let out = spans("some text\n", None, "notes.unknown-ext");

        assert_eq!(out.len(), 1);
        assert!(out[0].is_empty(), "{:?}", out[0]);
    }

    #[test]
    fn a_file_above_the_cap_is_not_highlighted() {
        let text = "int x = 1;\n".repeat(MAX_BYTES / 5);
        let out = spans(&text, Some("c"), "big.c");

        assert!(out.is_empty(), "a huge file is shown without colors");
    }

    #[test]
    fn the_cache_answers_the_second_call() {
        let h = Highlighter::new();
        let one = h.lines("blob-a", "int x;\n", Some("c"), "a.c");
        let two = h.lines("blob-a", "THIS IS NOT READ", Some("c"), "a.c");

        assert!(Arc::ptr_eq(&one, &two), "the blob hash is the key");
    }

    #[test]
    fn an_empty_file_has_no_lines() {
        assert!(spans("", Some("c"), "a.c").is_empty());
    }
}

#[cfg(test)]
mod names {
    use super::*;

    /// Every class the highlighter emits is prefixed.
    ///
    /// A scope atom is an English word, and so is a utility class. One that
    /// slips through unprefixed can set `display`, `float` or `position` on
    /// a piece of code and break the line it sits on.
    #[test]
    fn every_class_carries_the_prefix() {
        let h = Highlighter::new();
        let text = "/** A doc comment.\n *\n * More words.\n */\nint main(void) { return 0; }\n";
        let lines = h.lines("blob-of-the-test", text, Some("c"), "a.c");

        let mut seen = 0;
        for line in lines.iter() {
            for span in line {
                for class in span.cls.split_whitespace() {
                    assert!(class.starts_with("tok-"), "{class} is not prefixed");
                    seen += 1;
                }
            }
        }
        assert!(seen > 0, "the fixture must produce some classes");
    }
}
