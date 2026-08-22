//! The commit message as a file of the change.
//!
//! A reviewer reads the message the way they read the code, so it is one
//! more row in the file list. Gerrit shows it under the same name.
//!
//! Against the parent, the whole message is new: the parent carries another
//! message, and a diff of the two says nothing. Against another patch set,
//! the two messages are diffed, and that is where an amend shows.

use similar::{ChangeTag, TextDiff};

use crate::diff::words;
use crate::git::commit;
use crate::git::exec::Git;
use crate::model::{FileDiff, FileEntry, FileStatus, Hunk, Row, RowKind};

/// The path the message is shown under.
///
/// It starts with a slash, which no path of a git tree can, so it collides
/// with nothing. Gerrit uses this name.
pub const PATH: &str = "/COMMIT_MSG";

/// True when the path names the message rather than a file of the tree.
pub fn is(path: &str) -> bool {
    path == PATH
}

/// The message of a commit, as the reviewer reads it.
pub async fn text(git: &Git, rev: &str) -> Option<String> {
    let info = commit::info(git, rev).await.ok()?;
    let message = info.message.trim_end();

    match message.is_empty() {
        true => None,
        false => Some(format!("{message}\n")),
    }
}

/// The entry of the file list, with the counts the diff produces.
pub fn entry(old: &str, new: &str) -> FileEntry {
    let (added, removed) = counts(old, new);

    FileEntry {
        path: PATH.to_owned(),
        old_path: None,
        status: match old.is_empty() {
            true => FileStatus::Added,
            false => FileStatus::Modified,
        },
        language: String::new(),
        binary: false,
        added,
        removed,
    }
}

/// How many lines the message gained and lost.
fn counts(old: &str, new: &str) -> (usize, usize) {
    if old == new {
        return (0, 0);
    }
    let diff = TextDiff::from_lines(old, new);
    let mut added = 0;
    let mut removed = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }
    (added, removed)
}

/// The diff of the message, ready to be painted.
pub fn diff(old: &str, new: &str, context: usize) -> FileDiff {
    let mut hunks = hunks(old, new, context);

    for hunk in &mut hunks {
        words::mark(hunk);
    }

    FileDiff {
        file: entry(old, new),
        hunks,
        line_count: Some(new.lines().count()),
    }
}

/// The hunks of the two messages, with `context` lines around each change.
///
/// Two messages that are the same still make one hunk of context rows. The
/// reviewer opened the message to read it, and an empty page is not a
/// reading.
fn hunks(old: &str, new: &str, context: usize) -> Vec<Hunk> {
    if old == new {
        return match new.is_empty() {
            true => Vec::new(),
            false => vec![whole(new)],
        };
    }

    let diff = TextDiff::from_lines(old, new);
    let mut out = Vec::new();

    // A radius wider than the text is the same as the whole text, and
    // `usize::MAX` would overflow the arithmetic inside `grouped_ops`.
    let radius = context.min(old.lines().count() + new.lines().count() + 1);

    for group in diff.grouped_ops(radius) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_start = first.old_range().start;
        let new_start = first.new_range().start;
        let old_lines = last.old_range().end - old_start;
        let new_lines = last.new_range().end - new_start;
        let mut rows = Vec::new();

        for op in &group {
            for change in diff.iter_changes(op) {
                rows.push(row(
                    match change.tag() {
                        ChangeTag::Equal => RowKind::Context,
                        ChangeTag::Delete => RowKind::Remove,
                        ChangeTag::Insert => RowKind::Add,
                    },
                    change.old_index().map(|index| index + 1),
                    change.new_index().map(|index| index + 1),
                    change.value(),
                ));
            }
        }

        out.push(Hunk {
            // git prints 0 when a side is empty, and the interface reads
            // these numbers the way git writes them.
            old_start: match old_lines {
                0 => 0,
                _ => old_start + 1,
            },
            old_lines,
            new_start: match new_lines {
                0 => 0,
                _ => new_start + 1,
            },
            new_lines,
            header: String::new(),
            rows,
        });
    }
    out
}

/// One hunk that holds the whole message as context.
fn whole(text: &str) -> Hunk {
    let rows: Vec<Row> = text
        .lines()
        .enumerate()
        .map(|(index, line)| row(RowKind::Context, Some(index + 1), Some(index + 1), line))
        .collect();

    Hunk {
        old_start: 1,
        old_lines: rows.len(),
        new_start: 1,
        new_lines: rows.len(),
        header: String::new(),
        rows,
    }
}

fn row(kind: RowKind, old_line: Option<usize>, new_line: Option<usize>, text: &str) -> Row {
    Row {
        kind,
        old_line,
        new_line,
        text: text.trim_end_matches('\n').to_owned(),
        no_newline: false,
        tokens: Vec::new(),
        words: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{build_repo, commit};

    fn texts(hunk: &Hunk) -> Vec<(RowKind, String)> {
        hunk.rows
            .iter()
            .map(|row| (row.kind, row.text.clone()))
            .collect()
    }

    #[tokio::test]
    async fn the_message_of_a_commit_is_its_text() {
        let repo = build_repo(&[commit("first").file("a.txt", "one\n")]).await;
        let git = crate::git::exec::Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;

        assert_eq!(text(&git, &head).await, Some("first\n".to_owned()));
    }

    #[test]
    fn against_the_parent_the_whole_message_is_new() {
        let file = diff("", "Add a thing\n\nBecause of that.\n", 10);

        assert_eq!(file.file.status, FileStatus::Added);
        assert_eq!(file.file.added, 3);
        assert_eq!(file.file.removed, 0);
        assert_eq!(file.hunks.len(), 1);
        assert_eq!(file.hunks[0].old_start, 0);
        assert_eq!(file.hunks[0].new_start, 1);
        assert!(file.hunks[0].rows.iter().all(|r| r.kind == RowKind::Add));
    }

    #[test]
    fn an_amend_of_the_message_reads_as_a_diff() {
        let file = diff(
            "Add a thing\n\nOld reason.\n",
            "Add a thing\n\nNew reason.\n",
            10,
        );

        assert_eq!(file.file.status, FileStatus::Modified);
        assert_eq!(file.file.added, 1);
        assert_eq!(file.file.removed, 1);
        assert_eq!(
            texts(&file.hunks[0]),
            vec![
                (RowKind::Context, "Add a thing".to_owned()),
                (RowKind::Context, String::new()),
                (RowKind::Remove, "Old reason.".to_owned()),
                (RowKind::Add, "New reason.".to_owned()),
            ]
        );
    }

    #[test]
    fn a_word_that_moved_is_marked_inside_the_line() {
        let file = diff("Fix the parser\n", "Fix the printer\n", 10);
        let add = file.hunks[0]
            .rows
            .iter()
            .find(|row| row.kind == RowKind::Add)
            .unwrap();

        assert!(!add.words.is_empty(), "the word span is missing");
    }

    #[test]
    fn two_messages_that_are_the_same_are_still_readable() {
        let file = diff("Add a thing\n", "Add a thing\n", 10);

        assert_eq!(file.file.added, 0);
        assert_eq!(file.file.removed, 0);
        assert_eq!(
            texts(&file.hunks[0]),
            vec![(RowKind::Context, "Add a thing".to_owned())]
        );
    }

    #[test]
    fn a_long_message_keeps_a_gap_between_two_changes() {
        let old: String = (1..=40).map(|n| format!("line {n}\n")).collect();
        let new = old
            .replace("line 1\n", "first\n")
            .replace("line 40", "last");
        let file = diff(&old, &new, 3);

        assert_eq!(file.hunks.len(), 2, "the two ends must not be one hunk");
        assert_eq!(file.line_count, Some(40));
    }

    #[test]
    fn the_path_belongs_to_no_tree() {
        assert!(is(PATH));
        assert!(!is("COMMIT_MSG"));
        assert!(PATH.starts_with('/'));
    }
}
