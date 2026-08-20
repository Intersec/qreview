//! Turn the patch that git prints into files, hunks, and rows.
//!
//! A pure function from text to structure, so every shape we have seen can
//! become a case in the corpus.

use crate::model::{FileDiff, FileEntry, FileStatus, Hunk, Row, RowKind};

/// Parse the output of `git diff-tree -p`.
pub fn parse(patch: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current: Option<Builder> = None;

    for line in patch.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(builder) = current.take() {
                files.push(builder.build());
            }
            current = Some(Builder::new(rest));
            continue;
        }

        let Some(builder) = current.as_mut() else {
            continue;
        };
        builder.line(line);
    }

    if let Some(builder) = current {
        files.push(builder.build());
    }
    files
}

struct Builder {
    path: String,
    old_path: Option<String>,
    status: FileStatus,
    binary: bool,
    added: usize,
    removed: usize,
    hunks: Vec<Hunk>,
    old_line: usize,
    new_line: usize,
    in_hunk: bool,
}

impl Builder {
    fn new(header: &str) -> Self {
        let (a, b) = split_header(header);
        Self {
            path: b.unwrap_or_default(),
            old_path: a,
            status: FileStatus::Modified,
            binary: false,
            added: 0,
            removed: 0,
            hunks: Vec::new(),
            old_line: 0,
            new_line: 0,
            in_hunk: false,
        }
    }

    fn line(&mut self, line: &str) {
        if let Some(rest) = line.strip_prefix("@@ ") {
            self.start_hunk(rest);
            return;
        }

        if !self.in_hunk {
            self.header_line(line);
            return;
        }

        // A line that starts a new file header ends the hunk. `split` gives
        // us the whole patch, so an empty trailing line is normal.
        match line.as_bytes().first() {
            Some(b' ') => self.row(RowKind::Context, &line[1..]),
            Some(b'+') => self.row(RowKind::Add, &line[1..]),
            Some(b'-') => self.row(RowKind::Remove, &line[1..]),
            Some(b'\\') => self.no_newline(),
            // A blank source line is a single space in a unified diff, so an
            // empty line is the trailing one that `split` leaves behind, not
            // content. Anything else ends the hunk.
            _ => self.in_hunk = false,
        }
    }

    fn header_line(&mut self, line: &str) {
        if line.starts_with("new file mode") {
            self.status = FileStatus::Added;
        } else if line.starts_with("deleted file mode") {
            self.status = FileStatus::Deleted;
        } else if let Some(p) = line.strip_prefix("rename from ") {
            self.status = FileStatus::Renamed;
            self.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("rename to ") {
            self.path = unquote(p);
        } else if let Some(p) = line.strip_prefix("copy from ") {
            self.status = FileStatus::Copied;
            self.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("copy to ") {
            self.path = unquote(p);
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            self.binary = true;
        } else if let Some(p) = line.strip_prefix("--- ")
            && let Some(p) = strip_side(p)
        {
            self.old_path = Some(p);
        } else if let Some(p) = line.strip_prefix("+++ ")
            && let Some(p) = strip_side(p)
        {
            self.path = p;
        }
    }

    fn start_hunk(&mut self, rest: &str) {
        let Some((ranges, header)) = rest.split_once("@@") else {
            return;
        };
        let mut parts = ranges.split_whitespace();
        let Some(old) = parts.next().and_then(|p| range(p, '-')) else {
            return;
        };
        let Some(new) = parts.next().and_then(|p| range(p, '+')) else {
            return;
        };

        self.old_line = old.0;
        self.new_line = new.0;
        self.in_hunk = true;
        self.hunks.push(Hunk {
            old_start: old.0,
            old_lines: old.1,
            new_start: new.0,
            new_lines: new.1,
            header: header.trim().to_owned(),
            rows: Vec::new(),
        });
    }

    fn row(&mut self, kind: RowKind, text: &str) {
        let (old_line, new_line) = match kind {
            RowKind::Context => {
                let pair = (Some(self.old_line), Some(self.new_line));
                self.old_line += 1;
                self.new_line += 1;
                pair
            }
            RowKind::Add => {
                let pair = (None, Some(self.new_line));
                self.new_line += 1;
                self.added += 1;
                pair
            }
            RowKind::Remove => {
                let pair = (Some(self.old_line), None);
                self.old_line += 1;
                self.removed += 1;
                pair
            }
        };

        if let Some(hunk) = self.hunks.last_mut() {
            hunk.rows.push(Row {
                kind,
                old_line,
                new_line,
                text: text.to_owned(),
                no_newline: false,
                tokens: Vec::new(),
                words: Vec::new(),
            });
        }
    }

    fn no_newline(&mut self) {
        if let Some(row) = self.hunks.last_mut().and_then(|h| h.rows.last_mut()) {
            row.no_newline = true;
        }
    }

    fn build(self) -> FileDiff {
        FileDiff {
            file: FileEntry {
                path: self.path,
                old_path: match self.status {
                    FileStatus::Renamed | FileStatus::Copied => self.old_path,
                    _ => None,
                },
                status: self.status,
                language: String::new(),
                binary: self.binary,
                added: self.added,
                removed: self.removed,
            },
            hunks: self.hunks,
        }
    }
}

/// `a/old b/new`, with either side possibly quoted.
///
/// The two paths are only a fallback: `---` and `+++` are more reliable, and
/// a rename names both sides on its own lines.
fn split_header(header: &str) -> (Option<String>, Option<String>) {
    if let Some(rest) = header.strip_prefix("a/")
        && let Some(at) = rest.find(" b/")
    {
        let a = rest[..at].to_owned();
        let b = rest[at + 3..].to_owned();
        return (Some(a), Some(b));
    }
    (None, None)
}

/// `a/path`, `b/path`, or `/dev/null` on the side that has no file.
fn strip_side(value: &str) -> Option<String> {
    let value = value.split('\t').next().unwrap_or(value);
    if value == "/dev/null" {
        return None;
    }
    let value = unquote(value);
    let cut = value
        .strip_prefix("a/")
        .or_else(|| value.strip_prefix("b/"));

    Some(cut.map(str::to_owned).unwrap_or(value))
}

/// `-12,7` or `+3` becomes `(start, count)`.
fn range(part: &str, sign: char) -> Option<(usize, usize)> {
    let part = part.strip_prefix(sign)?;
    match part.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        // A range of one line has no count.
        None => Some((part.parse().ok()?, 1)),
    }
}

/// Undo the C-style quoting git uses for a path with a special character.
fn unquote(path: &str) -> String {
    let path = path.trim_end_matches('\t');
    if !path.starts_with('"') || !path.ends_with('"') || path.len() < 2 {
        return path.to_owned();
    }

    let inner = &path[1..path.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();

    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
