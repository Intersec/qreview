//! The diff of a change: which files, and what changed inside one.

pub mod parse;
pub mod words;

use anyhow::Result;

use crate::git::exec::Git;
use crate::model::{FileDiff, FileEntry, FileStatus};

pub use parse::parse;

/// The tree of an empty commit. The base of a root commit, which has no
/// parent to diff against.
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// The options every diff call shares.
///
/// Rename and copy detection are on, and the algorithm comes from the local
/// configuration, so the tool agrees with what the terminal shows.
const DETECT: [&str; 4] = ["-r", "-M", "-C", "--find-copies-harder"];

/// How the reader wants the diff read.
#[derive(Clone, Copy, Debug)]
pub struct How {
    /// Lines of unchanged code kept around a change.
    pub context: usize,
    /// Leave out what differs only by spacing.
    pub ignore_ws: bool,
    /// Colour the code.
    pub syntax: bool,
}

impl Default for How {
    fn default() -> Self {
        // Three lines is what git gives, and it is too few to judge a change.
        Self {
            context: 10,
            ignore_ws: false,
            syntax: true,
        }
    }
}

/// The file lists a run has already read, keyed by the pair of trees.
///
/// Rename and copy detection is the expensive half of a diff, and the answer
/// never moves: a commit is immutable, and the synthetic commit of the
/// working tree gets a new hash whenever the tree changes.
///
/// It is shared rather than owned, so the task that reads the series ahead of
/// the reader fills the same one, without holding the session.
#[derive(Default)]
pub struct FileLists {
    kept: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<Vec<FileEntry>>>>,
}

impl FileLists {
    /// The files of a pair of trees, from what is kept or from git.
    ///
    /// Every caller gets its own copy, because a reader edits what it takes:
    /// the language of each entry is filled in, and between two patch sets
    /// the rows the rebase brought are dropped.
    pub async fn of(&self, git: &Git, base: &str, rev: &str, how: &How) -> Result<Vec<FileEntry>> {
        // Of the three options, only `-w` changes which files differ.
        let key = format!("{base} {rev} {}", how.ignore_ws);

        if let Some(hit) = self.kept.lock().unwrap().get(&key) {
            crate::trace::note(|| format!("file list of {rev}, from the cache"));
            return Ok(hit.as_ref().clone());
        }

        let entries = files(git, base, rev, how).await?;
        self.kept
            .lock()
            .unwrap()
            .insert(key, std::sync::Arc::new(entries.clone()));

        Ok(entries)
    }

    /// How many lists are kept. What the read-ahead task is judged by.
    pub fn count(&self) -> usize {
        self.kept.lock().unwrap().len()
    }
}

/// The files a change touches, with the counts and no content.
///
/// One call, two output formats. `--raw` names the status of each file and
/// `--numstat` counts its lines, and git prints the raw block first. Asking
/// twice would run the rename and copy detection twice, and on a large
/// repository that pass alone costs about a second.
pub async fn files(git: &Git, base: &str, target: &str, how: &How) -> Result<Vec<FileEntry>> {
    let mut call: Vec<&str> = vec!["diff-tree"];
    call.extend_from_slice(&DETECT);
    if how.ignore_ws {
        call.push("-w");
    }
    call.extend_from_slice(&["--raw", "--numstat", "-z", base, target]);

    Ok(parse_files(&git.text(&call).await?))
}

/// The diff of one file. `None` when the change does not touch it.
///
/// One file at a time: a change of 200 files must not build 200 diffs to
/// show the first one.
pub async fn file(
    git: &Git,
    base: &str,
    target: &str,
    path: &str,
    old_path: Option<&str>,
    how: &How,
) -> Result<Option<FileDiff>> {
    let unified = format!("-U{}", how.context);
    let mut call: Vec<&str> = vec!["diff-tree", "-p", "--no-color", "--full-index", &unified];
    call.extend_from_slice(&DETECT);
    if how.ignore_ws {
        call.push("-w");
    }
    call.extend_from_slice(&[base, target, "--", path]);
    // A rename is only visible when both sides are in the pathspec.
    if let Some(old) = old_path {
        call.push(old);
    }

    let patch = git.text(&call).await?;

    let started = crate::trace::start();
    let mut found = parse(&patch).into_iter().find(|f| f.file.path == path);
    crate::trace::since(started, || {
        format!("parse the patch of {path}, {} bytes", patch.len())
    });

    if let Some(diff) = found.as_mut() {
        let started = crate::trace::start();
        for hunk in &mut diff.hunks {
            words::mark(hunk);
        }
        crate::trace::since(started, || {
            format!("word diff of {path}, {} hunks", diff.hunks.len())
        });
    }
    Ok(found)
}

/// The raw block, then the numstat block, out of one `-z` stream.
fn parse_files(out: &str) -> Vec<FileEntry> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty()).peekable();

    // A raw record opens with its mode and hash line, which starts with a
    // colon. The first field that does not is where the counts begin.
    let statuses = parse_raw(&mut fields);
    let mut entries = parse_numstat(&mut fields);

    for entry in &mut entries {
        if let Some((status, old, _)) = statuses.iter().find(|(_, _, path)| *path == entry.path) {
            entry.status = *status;
            entry.old_path = old.clone();
        }
    }
    entries
}

/// `:100644 100644 <sha> <sha> R100`, then the paths of that file.
fn parse_raw<'a>(
    fields: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Vec<(FileStatus, Option<String>, String)> {
    let mut out = Vec::new();

    while let Some(info) = fields.next_if(|field| field.starts_with(':')) {
        // The status is the last word of the line, `R` and `C` with a score.
        let code = info.rsplit(' ').next().unwrap_or("");
        let status = match code.as_bytes().first() {
            Some(b'A') => FileStatus::Added,
            Some(b'D') => FileStatus::Deleted,
            Some(b'R') => FileStatus::Renamed,
            Some(b'C') => FileStatus::Copied,
            _ => FileStatus::Modified,
        };

        let first = fields.next().unwrap_or("").to_owned();
        match status {
            FileStatus::Renamed | FileStatus::Copied => {
                let new = fields.next().unwrap_or("").to_owned();
                out.push((status, Some(first), new));
            }
            _ => out.push((status, None, first)),
        }
    }
    out
}

fn parse_numstat<'a>(
    fields: &mut std::iter::Peekable<impl Iterator<Item = &'a str>>,
) -> Vec<FileEntry> {
    let mut entries = Vec::new();

    while let Some(record) = fields.next() {
        let mut parts = record.splitn(3, '\t');
        let added = parts.next().unwrap_or("0");
        let removed = parts.next().unwrap_or("0");
        let path = parts.next().unwrap_or("");

        // A rename prints an empty path, then the old and the new one.
        let (old_path, path) = if path.is_empty() {
            let old = fields.next().unwrap_or("").to_owned();
            let new = fields.next().unwrap_or("").to_owned();
            (Some(old), new)
        } else {
            (None, path.to_owned())
        };

        // git prints a dash for both counts of a binary file.
        let binary = added == "-" || removed == "-";

        entries.push(FileEntry {
            path,
            old_path,
            status: FileStatus::Modified,
            language: String::new(),
            binary,
            added: added.parse().unwrap_or(0),
            removed: removed.parse().unwrap_or(0),
        });
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RowKind;
    use crate::testutil::{Repo, build_repo, commit};

    async fn two_commits(first: &[(&str, &str)], second: &[(&str, &str)]) -> Repo {
        let mut a = commit("before");
        for (path, content) in first {
            a = a.file(path, content);
        }
        let mut b = commit("after");
        for (path, content) in second {
            b = b.file(path, content);
        }
        build_repo(&[a, b]).await
    }

    async fn head_files(repo: &Repo) -> (Git, Vec<FileEntry>) {
        let git = Git::discover(repo.path()).await.unwrap();
        let entries = files(&git, "HEAD^", "HEAD", &How::default()).await.unwrap();

        (git, entries)
    }

    async fn head_diff(repo: &Repo, path: &str) -> FileDiff {
        let (git, entries) = head_files(repo).await;
        let old = entries
            .iter()
            .find(|e| e.path == path)
            .and_then(|e| e.old_path.clone());

        file(&git, "HEAD^", "HEAD", path, old.as_deref(), &How::default())
            .await
            .unwrap()
            .expect("the change must touch the file")
    }

    #[tokio::test]
    async fn a_modified_file_carries_its_rows_and_line_numbers() {
        let repo = two_commits(
            &[("a.txt", "one\ntwo\nthree\n")],
            &[("a.txt", "one\nTWO\nthree\n")],
        )
        .await;
        let diff = head_diff(&repo, "a.txt").await;

        assert_eq!(diff.file.status, FileStatus::Modified);
        assert_eq!((diff.file.added, diff.file.removed), (1, 1));
        assert_eq!(diff.hunks.len(), 1);

        let rows = &diff.hunks[0].rows;
        let shape: Vec<_> = rows.iter().map(|r| (r.kind, r.text.as_str())).collect();
        assert_eq!(
            shape,
            [
                (RowKind::Context, "one"),
                (RowKind::Remove, "two"),
                (RowKind::Add, "TWO"),
                (RowKind::Context, "three"),
            ]
        );

        assert_eq!((rows[0].old_line, rows[0].new_line), (Some(1), Some(1)));
        assert_eq!((rows[1].old_line, rows[1].new_line), (Some(2), None));
        assert_eq!((rows[2].old_line, rows[2].new_line), (None, Some(2)));
        assert_eq!((rows[3].old_line, rows[3].new_line), (Some(3), Some(3)));
    }

    #[tokio::test]
    async fn an_added_file_has_no_old_side() {
        let repo = two_commits(&[("a.txt", "a\n")], &[("b.txt", "new\n")]).await;
        let diff = head_diff(&repo, "b.txt").await;

        assert_eq!(diff.file.status, FileStatus::Added);
        assert_eq!(diff.file.old_path, None);
        assert_eq!((diff.file.added, diff.file.removed), (1, 0));
        assert!(diff.hunks[0].rows.iter().all(|r| r.kind == RowKind::Add));
    }

    #[tokio::test]
    async fn a_deleted_file_is_all_removals() {
        let repo = build_repo(&[
            commit("before").file("a.txt", "a\n").file("b.txt", "b\n"),
            commit("after").delete("b.txt").file("a.txt", "a2\n"),
        ])
        .await;
        let diff = head_diff(&repo, "b.txt").await;

        assert_eq!(diff.file.status, FileStatus::Deleted);
        assert!(diff.hunks[0].rows.iter().all(|r| r.kind == RowKind::Remove));
    }

    #[tokio::test]
    async fn a_rename_is_one_file_and_not_two() {
        let content = "one\ntwo\nthree\nfour\nfive\nsix\n";
        let repo = build_repo(&[
            commit("before").file("old.txt", content),
            commit("after").delete("old.txt").file("new.txt", content),
        ])
        .await;
        let (_, entries) = head_files(&repo).await;

        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].path, "new.txt");
        assert_eq!(entries[0].status, FileStatus::Renamed);
        assert_eq!(entries[0].old_path.as_deref(), Some("old.txt"));
    }

    /// The status of each file and the count of its lines come out of one
    /// call, as two blocks of the same stream. They are matched by path, so
    /// a change that renames one file and edits another must not swap them.
    #[tokio::test]
    async fn the_status_and_the_counts_land_on_the_right_file() {
        let content = "one\ntwo\nthree\nfour\nfive\nsix\n";
        let repo = build_repo(&[
            commit("before")
                .file("old.txt", content)
                .file("kept.txt", "a\n")
                .file("gone.txt", "g\n"),
            commit("after")
                .delete("old.txt")
                .delete("gone.txt")
                .file("new.txt", content)
                .file("kept.txt", "a\nb\n"),
        ])
        .await;
        let (_, entries) = head_files(&repo).await;

        let of = |path: &str| {
            entries
                .iter()
                .find(|e| e.path == path)
                .unwrap_or_else(|| panic!("{path} is not in {entries:?}"))
                .clone()
        };

        let renamed = of("new.txt");
        assert_eq!(renamed.status, FileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
        assert_eq!((renamed.added, renamed.removed), (0, 0));

        let kept = of("kept.txt");
        assert_eq!(kept.status, FileStatus::Modified);
        assert_eq!(kept.old_path, None);
        assert_eq!((kept.added, kept.removed), (1, 0));

        let gone = of("gone.txt");
        assert_eq!(gone.status, FileStatus::Deleted);
        assert_eq!((gone.added, gone.removed), (0, 1));
    }

    #[tokio::test]
    async fn a_copy_names_the_file_it_came_from() {
        let content = "one\ntwo\nthree\nfour\nfive\nsix\n";
        let repo = build_repo(&[
            commit("before").file("old.txt", content),
            commit("after").file("copy.txt", content),
        ])
        .await;
        let (_, entries) = head_files(&repo).await;
        let copy = entries.iter().find(|e| e.path == "copy.txt").unwrap();

        assert_eq!(copy.status, FileStatus::Copied);
        assert_eq!(copy.old_path.as_deref(), Some("old.txt"));
    }

    #[tokio::test]
    async fn a_binary_file_has_no_hunks() {
        let repo = two_commits(&[("a.txt", "a\n")], &[("blob.bin", "\0\u{1}\u{2}binary\0")]).await;
        let (_, entries) = head_files(&repo).await;
        let entry = entries.iter().find(|e| e.path == "blob.bin").unwrap();

        assert!(entry.binary);
        assert_eq!((entry.added, entry.removed), (0, 0));

        let diff = head_diff(&repo, "blob.bin").await;
        assert!(diff.file.binary);
        assert!(diff.hunks.is_empty());
    }

    #[tokio::test]
    async fn a_missing_trailing_newline_is_marked_on_the_row() {
        let repo = two_commits(&[("a.txt", "one\ntwo\n")], &[("a.txt", "one\ntwo")]).await;
        let diff = head_diff(&repo, "a.txt").await;
        let rows = &diff.hunks[0].rows;

        let removed = rows.iter().find(|r| r.kind == RowKind::Remove).unwrap();
        let added = rows.iter().find(|r| r.kind == RowKind::Add).unwrap();
        assert!(!removed.no_newline, "the old side ended with a newline");
        assert!(added.no_newline, "the new side does not");
    }

    #[tokio::test]
    async fn carriage_returns_stay_in_the_text() {
        let repo = two_commits(
            &[("a.txt", "one\r\ntwo\r\n")],
            &[("a.txt", "one\r\nTWO\r\n")],
        )
        .await;
        let diff = head_diff(&repo, "a.txt").await;
        let added = diff.hunks[0]
            .rows
            .iter()
            .find(|r| r.kind == RowKind::Add)
            .unwrap();

        assert_eq!(added.text, "TWO\r");
    }

    #[tokio::test]
    async fn an_empty_file_counts_nothing() {
        let repo = two_commits(&[("a.txt", "a\n")], &[("empty.txt", "")]).await;
        let (_, entries) = head_files(&repo).await;
        let entry = entries.iter().find(|e| e.path == "empty.txt").unwrap();

        assert_eq!(entry.status, FileStatus::Added);
        assert_eq!((entry.added, entry.removed), (0, 0));
    }

    #[tokio::test]
    async fn two_hunks_keep_their_own_line_numbers() {
        // Far enough apart that ten lines of context on each side of the
        // two changes still leave a gap between the hunks.
        let before = (1..=60).map(|i| format!("line {i}\n")).collect::<String>();
        let after = before
            .replace("line 3\n", "LINE 3\n")
            .replace("line 50\n", "LINE 50\n");
        let repo = two_commits(&[("a.txt", &before)], &[("a.txt", &after)]).await;
        let diff = head_diff(&repo, "a.txt").await;

        assert_eq!(diff.hunks.len(), 2, "far apart changes make two hunks");
        assert!(diff.hunks[0].old_start <= 3);
        assert!(
            diff.hunks[1].old_start > diff.hunks[0].old_start + diff.hunks[0].old_lines,
            "the second hunk starts after the first one ends"
        );

        let second = diff.hunks[1]
            .rows
            .iter()
            .find(|r| r.kind == RowKind::Add)
            .unwrap();
        assert_eq!(second.text, "LINE 50");
        assert_eq!(second.new_line, Some(50));
    }

    #[tokio::test]
    async fn a_root_commit_diffs_against_the_empty_tree() {
        let repo = build_repo(&[commit("first").file("a.txt", "one\ntwo\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let entries = files(&git, EMPTY_TREE, "HEAD", &How::default())
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, FileStatus::Added);
        assert_eq!(entries[0].added, 2);
    }

    #[tokio::test]
    async fn a_path_with_a_space_survives_the_round_trip() {
        let repo = two_commits(&[("a.txt", "a\n")], &[("dir/a file.txt", "one\n")]).await;
        let (_, entries) = head_files(&repo).await;

        assert!(
            entries.iter().any(|e| e.path == "dir/a file.txt"),
            "{entries:?}"
        );
        let diff = head_diff(&repo, "dir/a file.txt").await;
        assert_eq!(diff.hunks[0].rows[0].text, "one");
    }

    #[tokio::test]
    async fn a_file_the_change_does_not_touch_is_none() {
        let repo = two_commits(&[("a.txt", "a\n")], &[("a.txt", "b\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let missing = file(&git, "HEAD^", "HEAD", "other.txt", None, &How::default())
            .await
            .unwrap();
        assert!(missing.is_none());
    }
}
