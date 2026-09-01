//! Syntax colors, computed in the server.
//!
//! The browser receives rows that already carry their spans, so it never
//! downloads a grammar and a large file costs it the text and nothing else.
//! The output is a CSS class, never a color: a theme is a stylesheet, and the
//! light and dark pair costs one pass, not two.
//!
//! A parse starts at line 1, because a block comment or a multi-line string
//! needs the lines before it. It does not have to reach the end: a diff shows
//! a few hunks, and the reader asks for the rest one piece at a time. So the
//! parse stops where the caller stops, and keeps its state to go further.

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

/// A file that is painted down to some line, and can go further.
#[derive(Default)]
struct Painted {
    lines: Vec<Vec<Span>>,
    rest: Rest,
    /// The last list handed out, kept while nothing has grown since.
    handed: Option<Lines>,
}

/// How far a file is painted.
#[derive(Default)]
enum Rest {
    /// Nothing is painted yet.
    #[default]
    Fresh,
    /// The parser, frozen between two lines.
    At(Box<Pending>),
    /// The last line is painted. There is nothing left to do.
    Done,
}

struct Pending {
    state: ParseState,
    stack: ScopeStack,
    /// The byte the next line starts at.
    offset: usize,
}

pub struct Highlighter {
    syntaxes: SyntaxSet,
    /// Keyed by blob hash. Two patch sets that share a file highlight it once.
    ///
    /// One lock per file, so painting a large one holds nothing that another
    /// file needs.
    cache: Mutex<HashMap<String, Arc<Mutex<Painted>>>>,
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
        self.lines_upto(blob, text, language, path, usize::MAX)
    }

    /// The same, painted no further than line `upto`.
    ///
    /// A diff of a large file shows a few hunks near the top and nothing
    /// below, and painting the rest costs seconds that no reader waits for
    /// on purpose. What is painted stays painted, and a later call for a
    /// deeper line carries on from where this one stopped.
    pub fn lines_upto(
        &self,
        blob: &str,
        text: &str,
        language: Option<&str>,
        path: &str,
        upto: usize,
    ) -> Lines {
        let entry = self.entry(blob);
        let mut painted = entry.lock().unwrap();

        if painted.lines.len() >= upto || matches!(painted.rest, Rest::Done) {
            if painted.handed.is_none() {
                painted.handed = Some(Arc::new(painted.lines.clone()));
            }
            crate::trace::note(|| format!("highlight {path}, from the cache"));
            return painted.handed.clone().unwrap_or_default();
        }

        let started = crate::trace::start();
        let from = painted.lines.len();
        self.paint(&mut painted, text, language, path, upto);
        crate::trace::since(started, || {
            format!(
                "highlight {path}, lines {} to {}, of {} bytes",
                from + 1,
                painted.lines.len(),
                text.len()
            )
        });

        let lines = Arc::new(painted.lines.clone());
        painted.handed = Some(lines.clone());

        lines
    }

    /// The entry of a blob, made empty on the first ask.
    fn entry(&self, blob: &str) -> Arc<Mutex<Painted>> {
        self.cache
            .lock()
            .unwrap()
            .entry(blob.to_owned())
            .or_default()
            .clone()
    }

    /// Carry the parse of one file down to line `upto`.
    fn paint(
        &self,
        painted: &mut Painted,
        text: &str,
        language: Option<&str>,
        path: &str,
        upto: usize,
    ) {
        if text.len() > MAX_BYTES {
            painted.rest = Rest::Done;
            return;
        }

        let mut rest = match std::mem::replace(&mut painted.rest, Rest::Done) {
            Rest::At(rest) => rest,
            Rest::Done => return,
            Rest::Fresh => Box::new(Pending {
                state: ParseState::new(self.syntax_of(text, language, path)),
                stack: ScopeStack::new(),
                offset: 0,
            }),
        };

        // The blob hash is the hash of the text, so the offset always lands
        // inside it. A slice that panicked would poison the lock of this
        // file for the rest of the run, which is a high price for a rule
        // that already holds.
        let tail = text.get(rest.offset..).unwrap_or_default();

        for line in LinesWithEndings::from(tail) {
            if painted.lines.len() >= upto {
                painted.rest = Rest::At(rest);
                return;
            }
            match rest.state.parse_line(line, &self.syntaxes) {
                Ok(ops) => painted.lines.push(spans_of(line, &ops, &mut rest.stack)),
                // A grammar that fails on one line still colors the rest.
                Err(_) => painted.lines.push(Vec::new()),
            }
            rest.offset += line.len();
        }
    }

    fn syntax_of(
        &self,
        text: &str,
        language: Option<&str>,
        path: &str,
    ) -> &syntect::parsing::SyntaxReference {
        language
            .and_then(|name| self.syntaxes.find_syntax_by_token(name))
            .or_else(|| self.syntaxes.find_syntax_by_extension(extension(path)))
            .or_else(|| {
                self.syntaxes
                    .find_syntax_by_first_line(text.lines().next()?)
            })
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text())
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

    /// A parse that stops and carries on must paint what one pass paints.
    ///
    /// The state matters most: the block comment opens before the first
    /// stop and closes after it, so a second call that started over would
    /// paint line 5 as code.
    #[test]
    fn a_stop_and_a_carry_on_paint_what_one_pass_paints() {
        let text = "/* one\n two\n three\n four\n five */\nint x = 1;\n";
        let step = Highlighter::new();

        let short = step.lines_upto("blob-a", text, Some("c"), "a.c", 2);
        assert_eq!(short.len(), 2, "a stop paints no further");

        let whole = step.lines_upto("blob-a", text, Some("c"), "a.c", 6);
        let once = Highlighter::new().lines("blob-a", text, Some("c"), "a.c");

        assert_eq!(*whole, *once);
        assert!(
            classes(&whole[4], " five */")
                .iter()
                .any(|(cls, _)| cls.starts_with("tok-comment")),
            "the comment must still be open on line 5: {:?}",
            whole[4]
        );
    }

    /// A line that is painted is never painted again.
    #[test]
    fn a_deeper_ask_only_paints_what_is_missing() {
        let text = "int a;
int b;
int c;
int d;
";
        let h = Highlighter::new();

        assert_eq!(h.lines_upto("blob-a", text, Some("c"), "a.c", 1).len(), 1);
        assert_eq!(h.lines_upto("blob-a", text, Some("c"), "a.c", 3).len(), 3);
        assert_eq!(
            h.lines_upto("blob-a", text, Some("c"), "a.c", 2).len(),
            3,
            "a shallower ask paints nothing back out"
        );
        assert_eq!(h.lines("blob-a", text, Some("c"), "a.c").len(), 4);
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
