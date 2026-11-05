//! The hunks that a rebase brought into the diff between two patch sets.
//!
//! Two patch sets on two bases differ by the work of the change and by what
//! the rebase brought. Gerrit marks the second, and this module follows its
//! rules, so that the two tools agree on the same pair of versions.
//!
//! The diff of the two parents is the rebase. Each of its edits moves onto
//! both patch sets, through the diff of each patch set against its own
//! parent. An edit that overlaps a change of the patch set is dropped. An
//! edit of the diff between the patch sets is due to rebase when it equals
//! one of the moved edits.
//!
//! Every diff comes from `git diff-tree`, the call that builds the diff on
//! the screen, so that the edits of all of them agree.

use std::collections::{BTreeSet, HashSet};
use std::ops::Range;

use anyhow::Result;

use crate::diff::parse::parse;
use crate::git::exec::Git;
use crate::model::{FileDiff, FileEntry, FileStatus, Hunk, RowKind};

/// A run of lines that a diff replaces with another run. Each side is
/// 0-based and half-open, as an `Edit` of JGit, which Gerrit compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Edit {
    pub begin_a: usize,
    pub end_a: usize,
    pub begin_b: usize,
    pub end_b: usize,
}

/// The two patch sets, and the parent of each one.
#[derive(Clone, Debug)]
pub struct Sides {
    pub old: String,
    pub new: String,
    pub old_parent: String,
    pub new_parent: String,
}

/// Where one file of the diff stands in each of the four trees. `None`
/// where the tree has no such file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paths {
    pub old: Option<String>,
    pub new: Option<String>,
    pub old_parent: Option<String>,
    pub new_parent: Option<String>,
}

/// What the rebase brought into one file of the diff.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Brought {
    pub added: usize,
    pub removed: usize,
    /// Every edit of the file came with the rebase.
    pub all: bool,
}

/// Where a file of a patch set stands in the parent of that patch set, from
/// the files of the patch set against that parent.
///
/// If the patch set has no such file, it may have deleted it. `other`, the
/// name of the file in the other patch set, finds that.
pub fn parent_path(own: &[FileEntry], here: Option<&str>, other: Option<&str>) -> Option<String> {
    let Some(path) = here else {
        let name = other?;
        return own
            .iter()
            .find(|e| e.status == FileStatus::Deleted && e.path == name)
            .map(|e| e.path.clone());
    };
    match own.iter().find(|e| e.path == path) {
        Some(e) if matches!(e.status, FileStatus::Added | FileStatus::Copied) => None,
        Some(e) => Some(e.old_path.clone().unwrap_or_else(|| path.to_owned())),
        None => Some(path.to_owned()),
    }
}

/// The diffs that the marks of some files need, read once for all of them.
pub struct Reads {
    /// From the old patch set to the new one. Absent when the diff on the
    /// screen already holds its edits.
    main: Option<Diff>,
    old_side: Diff,
    new_side: Diff,
    parents: Diff,
}

impl Reads {
    /// One call of git for each pair of trees, whatever the count of files.
    pub async fn of(
        git: &Git,
        sides: &Sides,
        files: &[Paths],
        ignore_ws: bool,
        with_main: bool,
    ) -> Result<Self> {
        let names = |pick: fn(&Paths) -> [&Option<String>; 2]| -> Vec<&str> {
            let set: BTreeSet<&str> = files
                .iter()
                .flat_map(|file| pick(file).into_iter().flatten())
                .map(String::as_str)
                .collect();
            set.into_iter().collect()
        };
        let main_paths = match with_main {
            true => names(|f| [&f.old, &f.new]),
            false => Vec::new(),
        };
        let (main, old_side, new_side, parents) = tokio::join!(
            Diff::read(git, &sides.old, &sides.new, main_paths, ignore_ws),
            Diff::read(
                git,
                &sides.old_parent,
                &sides.old,
                names(|f| [&f.old_parent, &f.old]),
                ignore_ws,
            ),
            Diff::read(
                git,
                &sides.new_parent,
                &sides.new,
                names(|f| [&f.new_parent, &f.new]),
                ignore_ws,
            ),
            Diff::read(
                git,
                &sides.old_parent,
                &sides.new_parent,
                names(|f| [&f.old_parent, &f.new_parent]),
                ignore_ws,
            ),
        );

        Ok(Self {
            main: with_main.then_some(main?),
            old_side: old_side?,
            new_side: new_side?,
            parents: parents?,
        })
    }

    /// The edits of the rebase in one file, moved onto the two patch sets.
    pub fn rebase(&self, file: &Paths) -> HashSet<Edit> {
        let parents = self
            .parents
            .edits(file.old_parent.as_deref(), file.new_parent.as_deref());
        let Some(mut edits) = parents else {
            return HashSet::new();
        };
        let moves = [
            (Side::A, &file.old_parent, &file.old, &self.old_side),
            (Side::B, &file.new_parent, &file.new, &self.new_side),
        ];
        for (side, parent, here, diff) in moves {
            // A parent without the file holds no line to move, so Gerrit
            // keeps the edits of that side as they are.
            let Some(parent) = parent.as_deref() else {
                continue;
            };
            // The patch set deleted the file, so no edit stands in it.
            let Some(here) = here.as_deref() else {
                return HashSet::new();
            };
            // The diff does not pair the two files, so no edit can move.
            let Some(through) = diff.edits(Some(parent), Some(here)) else {
                return HashSet::new();
            };
            edits = transform(edits, &through, side);
        }
        edits.into_iter().collect()
    }

    /// What the rebase brought into one file of the list.
    pub fn brought(&self, file: &Paths) -> Brought {
        let main = self
            .main
            .as_ref()
            .and_then(|diff| diff.edits(file.old.as_deref(), file.new.as_deref()))
            .unwrap_or_default();
        let rebase = self.rebase(file);
        let ours: Vec<&Edit> = main.iter().filter(|e| rebase.contains(e)).collect();

        Brought {
            added: ours.iter().map(|e| e.end_b - e.begin_b).sum(),
            removed: ours.iter().map(|e| e.end_a - e.begin_a).sum(),
            all: !main.is_empty() && ours.len() == main.len(),
        }
    }
}

/// Mark the rows of the edits that the rebase brought, and take them out of
/// the counts of the file.
pub fn mark(file: &mut FileDiff, rebase: &HashSet<Edit>) {
    for block in blocks(&file.hunks) {
        if !rebase.contains(&block.edit) {
            continue;
        }
        for row in &mut file.hunks[block.hunk].rows[block.rows] {
            row.rebase = true;
        }
        let edit = block.edit;
        file.file.added = file.file.added.saturating_sub(edit.end_b - edit.begin_b);
        file.file.removed = file.file.removed.saturating_sub(edit.end_a - edit.begin_a);
    }
}

/// The `-U0` diff of some files between two trees.
struct Diff(Vec<FileDiff>);

impl Diff {
    async fn read(
        git: &Git,
        from: &str,
        to: &str,
        paths: Vec<&str>,
        ignore_ws: bool,
    ) -> Result<Self> {
        // No path would read the diff of the whole tree.
        if paths.is_empty() {
            return Ok(Self(Vec::new()));
        }
        let mut call = vec![
            "diff-tree",
            "-r",
            "-p",
            "--no-color",
            "--no-ext-diff",
            "-U0",
            "-M",
        ];
        if ignore_ws {
            call.push("-w");
        }
        call.extend([from, to, "--"]);
        call.extend(paths);

        Ok(Self(parse(&git.text(&call).await?)))
    }

    /// The edits from the file `from` to the file `to`. Empty when the diff
    /// does not touch the file, and `None` when it does not pair the two.
    fn edits(&self, from: Option<&str>, to: Option<&str>) -> Option<Vec<Edit>> {
        let found = self.0.iter().find(|f| {
            let status = f.file.status;
            match (from, to) {
                (Some(a), Some(b)) => {
                    !matches!(status, FileStatus::Added | FileStatus::Deleted)
                        && f.file.old_path.as_deref().unwrap_or(&f.file.path) == a
                        && f.file.path == b
                }
                (None, Some(b)) => status == FileStatus::Added && f.file.path == b,
                (Some(a), None) => status == FileStatus::Deleted && f.file.path == a,
                (None, None) => false,
            }
        });
        match found {
            Some(file) => Some(edits_of(&file.hunks)),
            None if from.is_some() && from == to => Some(Vec::new()),
            None => None,
        }
    }
}

/// The edits of hunks that carry any number of context lines: each run of
/// rows that are not context is one edit.
pub fn edits_of(hunks: &[Hunk]) -> Vec<Edit> {
    blocks(hunks).into_iter().map(|block| block.edit).collect()
}

/// One edit, and the rows of the hunk that show it.
struct Block {
    edit: Edit,
    hunk: usize,
    rows: Range<usize>,
}

fn blocks(hunks: &[Hunk]) -> Vec<Block> {
    let mut out = Vec::new();

    for (index, hunk) in hunks.iter().enumerate() {
        let mut a = start(hunk.old_start, hunk.old_lines);
        let mut b = start(hunk.new_start, hunk.new_lines);
        // The first row of the run, and the line of each side it starts on.
        let mut open: Option<(usize, usize, usize)> = None;
        let close = |(row, begin_a, begin_b): (usize, usize, usize), end: usize, a, b| Block {
            edit: Edit {
                begin_a,
                end_a: a,
                begin_b,
                end_b: b,
            },
            hunk: index,
            rows: row..end,
        };

        for (at, row) in hunk.rows.iter().enumerate() {
            match row.kind {
                RowKind::Context => {
                    if let Some(run) = open.take() {
                        out.push(close(run, at, a, b));
                    }
                    a += 1;
                    b += 1;
                }
                RowKind::Remove => {
                    open.get_or_insert((at, a, b));
                    a += 1;
                }
                RowKind::Add => {
                    open.get_or_insert((at, a, b));
                    b += 1;
                }
            }
        }
        if let Some(run) = open {
            out.push(close(run, hunk.rows.len(), a, b));
        }
    }
    out
}

/// The 0-based line where one side of a hunk starts. A side with no line
/// names the line before the hole, which is the hole once 0-based.
fn start(first: usize, count: usize) -> usize {
    match count {
        0 => first,
        _ => first.saturating_sub(1),
    }
}

/// Which side of an edit of the rebase a move reads.
#[derive(Clone, Copy, Debug)]
enum Side {
    /// The old parent, which the old patch set is written on.
    A,
    /// The new parent.
    B,
}

/// Move one side of the edits of the rebase onto a patch set, through the
/// edits of the patch set against that parent.
///
/// The rule of Gerrit: an edit that overlaps an edit of `through` is
/// dropped, and the others shift by what `through` added and removed above
/// them. Two ranges that only touch do not overlap.
fn transform(edits: Vec<Edit>, through: &[Edit], side: Side) -> Vec<Edit> {
    edits
        .into_iter()
        .filter_map(|edit| {
            let (begin, end) = match side {
                Side::A => (edit.begin_a, edit.end_a),
                Side::B => (edit.begin_b, edit.end_b),
            };
            let mut shift = 0isize;
            for step in through {
                if step.end_a <= begin {
                    shift = step.end_b as isize - step.end_a as isize;
                    continue;
                }
                if end <= step.begin_a {
                    break;
                }
                return None;
            }
            let (begin, end) = (
                begin.checked_add_signed(shift)?,
                end.checked_add_signed(shift)?,
            );
            Some(match side {
                Side::A => Edit {
                    begin_a: begin,
                    end_a: end,
                    ..edit
                },
                Side::B => Edit {
                    begin_b: begin,
                    end_b: end,
                    ..edit
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileEntry;
    use crate::testutil::{Repo, build_repo, commit};

    fn edit(begin_a: usize, end_a: usize, begin_b: usize, end_b: usize) -> Edit {
        Edit {
            begin_a,
            end_a,
            begin_b,
            end_b,
        }
    }

    /// 100 lines, `Line 1` to `Line 100`, with some of them replaced.
    fn hundred(changes: &[(usize, &str)]) -> String {
        (1..=100)
            .map(|n| {
                let line = changes
                    .iter()
                    .find(|(at, _)| *at == n)
                    .map_or(format!("Line {n}"), |(_, text)| (*text).to_owned());
                format!("{line}\n")
            })
            .collect()
    }

    /// One file, `f.txt`, in two versions of a change. The second version
    /// sits on the first base with the rebase on top of it.
    async fn rebased(base: &str, ps1: &str, upstream: &str, ps2: &str) -> (Repo, Sides) {
        let repo = build_repo(&[commit("base").file("f.txt", base)]).await;
        let old_parent = repo.sha("HEAD").await;
        repo.add(&commit("work").file("f.txt", ps1)).await;
        let old = repo.sha("HEAD").await;

        repo.git(&["switch", "--quiet", "--detach", &old_parent])
            .await;
        repo.add(&commit("upstream").file("f.txt", upstream)).await;
        let new_parent = repo.sha("HEAD").await;
        repo.add(&commit("work").file("f.txt", ps2)).await;
        let new = repo.sha("HEAD").await;

        let sides = Sides {
            old,
            new,
            old_parent,
            new_parent,
        };
        (repo, sides)
    }

    fn everywhere(path: &str) -> Paths {
        Paths {
            old: Some(path.to_owned()),
            new: Some(path.to_owned()),
            old_parent: Some(path.to_owned()),
            new_parent: Some(path.to_owned()),
        }
    }

    /// Each edit of the diff between the two versions, by the first line it
    /// changes in the new one, and whether the rebase brought it.
    async fn marks(repo: &Repo, sides: &Sides) -> Vec<(usize, bool)> {
        let git = Git::discover(repo.path()).await.unwrap();
        let file = everywhere("f.txt");
        let reads = Reads::of(&git, sides, std::slice::from_ref(&file), false, true)
            .await
            .unwrap();
        let rebase = reads.rebase(&file);
        let main = reads.main.as_ref().unwrap();

        main.edits(Some("f.txt"), Some("f.txt"))
            .unwrap()
            .into_iter()
            .map(|e| (e.begin_b + 1, rebase.contains(&e)))
            .collect()
    }

    #[test]
    fn an_edit_shifts_by_what_was_added_above_it() {
        let moved = transform(vec![edit(10, 11, 10, 11)], &[edit(0, 0, 0, 2)], Side::A);

        assert_eq!(moved, [edit(12, 13, 10, 11)]);
    }

    #[test]
    fn an_edit_that_only_touches_another_one_moves() {
        let before = transform(vec![edit(10, 11, 10, 11)], &[edit(9, 10, 9, 11)], Side::B);
        let after = transform(vec![edit(10, 11, 10, 11)], &[edit(11, 12, 11, 11)], Side::B);

        assert_eq!(before, [edit(10, 11, 11, 12)]);
        assert_eq!(after, [edit(10, 11, 10, 11)]);
    }

    #[test]
    fn an_edit_that_overlaps_another_one_is_dropped() {
        let edits = vec![edit(10, 12, 10, 12)];

        assert_eq!(
            transform(edits.clone(), &[edit(11, 12, 11, 13)], Side::A),
            []
        );
        // An insertion inside the range splits it, so it overlaps too.
        assert_eq!(transform(edits, &[edit(11, 11, 11, 13)], Side::A), []);
    }

    #[test]
    fn an_insertion_moves_past_an_insertion_at_the_same_place() {
        let moved = transform(vec![edit(5, 5, 5, 6)], &[edit(5, 5, 5, 7)], Side::A);

        assert_eq!(moved, [edit(7, 7, 5, 6)]);
    }

    #[test]
    fn each_run_of_changed_rows_is_one_edit() {
        let patch = "diff --git a/f b/f\n--- a/f\n+++ b/f\n\
                     @@ -1,6 +1,6 @@\n a\n-b\n+B\n c\n d\n-e\n f\n\
                     @@ -20,0 +20,2 @@\n+x\n+y\n";
        let file = &parse(patch)[0];

        assert_eq!(
            edits_of(&file.hunks),
            [edit(1, 2, 1, 2), edit(4, 5, 4, 4), edit(20, 20, 19, 21)]
        );
    }

    #[test]
    fn a_parent_path_follows_the_rename_of_the_patch_set() {
        let own = [
            entry("new.txt", Some("old.txt"), FileStatus::Renamed),
            entry("added.txt", None, FileStatus::Added),
            entry("gone.txt", None, FileStatus::Deleted),
        ];

        assert_eq!(
            parent_path(&own, Some("new.txt"), None).as_deref(),
            Some("old.txt")
        );
        assert_eq!(parent_path(&own, Some("added.txt"), None), None);
        assert_eq!(
            parent_path(&own, Some("same.txt"), None).as_deref(),
            Some("same.txt")
        );
        // The patch set deleted it, so the parent has it.
        assert_eq!(
            parent_path(&own, None, Some("gone.txt")).as_deref(),
            Some("gone.txt")
        );
        assert_eq!(parent_path(&own, None, Some("other.txt")), None);
    }

    fn entry(path: &str, old_path: Option<&str>, status: FileStatus) -> FileEntry {
        FileEntry {
            path: path.to_owned(),
            old_path: old_path.map(str::to_owned),
            status,
            language: String::new(),
            binary: false,
            added: 0,
            removed: 0,
        }
    }

    // The cases below are the ones Gerrit tests in `RevisionDiffIT`, so the
    // two tools agree on the same pair of versions.

    #[tokio::test]
    async fn rebase_edits_at_the_start_of_a_file_are_marked() {
        let (repo, sides) = rebased(
            &hundred(&[]),
            &hundred(&[(50, "Line fifty")]),
            &hundred(&[(1, "Line one"), (5, "Line five")]),
            &hundred(&[(1, "Line one"), (5, "Line five"), (50, "Line 50, again")]),
        )
        .await;

        assert_eq!(
            marks(&repo, &sides).await,
            [(1, true), (5, true), (50, false)]
        );
    }

    #[tokio::test]
    async fn rebase_edits_at_the_end_of_a_file_are_marked() {
        let (repo, sides) = rebased(
            &hundred(&[]),
            &hundred(&[(50, "Line fifty")]),
            &hundred(&[(60, "Line sixty"), (100, "Line hundred")]),
            &hundred(&[
                (50, "Line 50, again"),
                (60, "Line sixty"),
                (100, "Line hundred"),
            ]),
        )
        .await;

        assert_eq!(
            marks(&repo, &sides).await,
            [(50, false), (60, true), (100, true)]
        );
    }

    #[tokio::test]
    async fn a_rebase_edit_is_marked_when_the_old_version_moved_it_down() {
        let (repo, sides) = rebased(
            &hundred(&[]),
            &format!("Line zero\n{}", hundred(&[(100, "Line hundred")])),
            &hundred(&[(40, "Line forty")]),
            &format!(
                "Line zero\n{}",
                hundred(&[(40, "Line forty"), (100, "Line 100, again")])
            ),
        )
        .await;

        assert_eq!(marks(&repo, &sides).await, [(41, true), (101, false)]);
    }

    #[tokio::test]
    async fn a_rebase_edit_is_marked_when_the_new_version_moved_it_down() {
        let (repo, sides) = rebased(
            &hundred(&[]),
            &hundred(&[(100, "Line hundred")]),
            &hundred(&[(40, "Line forty")]),
            &format!(
                "Line zero\n{}",
                hundred(&[(40, "Line forty"), (100, "Line hundred")])
            ),
        )
        .await;

        assert_eq!(marks(&repo, &sides).await, [(1, false), (41, true)]);
    }

    #[tokio::test]
    async fn a_rebase_edit_that_the_new_version_changes_again_is_its_work() {
        let (repo, sides) = rebased(
            &hundred(&[]),
            &hundred(&[(50, "Line fifty")]),
            &hundred(&[(40, "Line forty")]),
            &hundred(&[(40, "Line forty-two"), (50, "Line fifty")]),
        )
        .await;

        assert_eq!(marks(&repo, &sides).await, [(40, false)]);
    }

    #[tokio::test]
    async fn a_rebase_edit_next_to_a_change_is_the_work_of_the_change() {
        // Git makes one edit of lines 39 and 40, and the rebase changed only
        // the first of them.
        let (repo, sides) = rebased(
            &hundred(&[]),
            &hundred(&[(50, "Line fifty")]),
            &hundred(&[(39, "Line thirty-nine")]),
            &hundred(&[
                (39, "Line thirty-nine"),
                (40, "Line forty"),
                (50, "Line fifty"),
            ]),
        )
        .await;

        assert_eq!(marks(&repo, &sides).await, [(39, false)]);
    }

    #[tokio::test]
    async fn rebase_edits_one_line_from_a_change_are_marked() {
        let (repo, sides) = rebased(
            &hundred(&[]),
            &hundred(&[(50, "Line fifty")]),
            &hundred(&[(1, "Line one"), (5, "Line five")]),
            &hundred(&[
                (1, "Line one"),
                (3, "Line three"),
                (5, "Line five"),
                (50, "Line fifty"),
            ]),
        )
        .await;

        assert_eq!(
            marks(&repo, &sides).await,
            [(1, true), (3, false), (5, true)]
        );
    }

    #[tokio::test]
    async fn a_file_that_the_rebase_added_and_the_change_edited_is_the_work_of_the_change() {
        let repo = build_repo(&[commit("base").file("keep.txt", "k\n")]).await;
        let old_parent = repo.sha("HEAD").await;
        repo.add(&commit("work").file("keep.txt", "K\n")).await;
        let old = repo.sha("HEAD").await;
        repo.git(&["switch", "--quiet", "--detach", &old_parent])
            .await;
        repo.add(
            &commit("upstream")
                .file("edited.txt", "u1\nu2\n")
                .file("left.txt", "l1\nl2\n"),
        )
        .await;
        let new_parent = repo.sha("HEAD").await;
        repo.add(
            &commit("work")
                .file("keep.txt", "K\n")
                .file("edited.txt", "u1\nU2\n"),
        )
        .await;
        let new = repo.sha("HEAD").await;
        let sides = Sides {
            old,
            new,
            old_parent,
            new_parent,
        };
        let added = |path: &str| Paths {
            old: None,
            new: Some(path.to_owned()),
            old_parent: None,
            new_parent: Some(path.to_owned()),
        };
        let files = [added("edited.txt"), added("left.txt")];
        let git = Git::discover(repo.path()).await.unwrap();
        let reads = Reads::of(&git, &sides, &files, false, true).await.unwrap();

        // The edit of the rebase overlaps the edit of the change, so Gerrit
        // drops it, and the file reads as added in whole.
        assert_eq!(reads.brought(&files[0]), Brought::default());
        // The file that the change left alone came with the rebase.
        assert_eq!(
            reads.brought(&files[1]),
            Brought {
                added: 2,
                removed: 0,
                all: true
            }
        );
    }
}
