//! Where a line of one version lands in the next one.
//!
//! A remark moves with its line when a new version is declared. The hunks
//! of `git diff` between the two versions say where every line went, and
//! `git` reads them the way the developer's terminal does.

use anyhow::Result;

use crate::diff::parse::parse;
use crate::git::exec::Git;
use crate::model::{FileStatus, Hunk};

/// A line of the old version, in the new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapped {
    pub line: usize,
    /// The new version changed this line. It stands at the start of what
    /// replaced it.
    pub touched: bool,
}

/// The hunks of one file between two versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMap {
    /// The path in the new version, which a rename changes.
    pub path: String,
    hunks: Vec<(usize, usize, usize, usize)>,
}

impl FileMap {
    fn of(path: &str, hunks: &[Hunk]) -> Self {
        Self {
            path: path.to_owned(),
            hunks: hunks
                .iter()
                .map(|h| (h.old_start, h.old_lines, h.new_start, h.new_lines))
                .collect(),
        }
    }

    /// The same file in both versions, unchanged.
    fn same(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            hunks: Vec::new(),
        }
    }

    pub fn map(&self, line: usize) -> Mapped {
        map_line(&self.hunks, line)
    }
}

/// Move a line through the hunks of a `-U0` diff.
///
/// With no context, a hunk that only adds names the line it adds after, and
/// a hunk that only removes names the line it removes after in the new
/// version.
fn map_line(hunks: &[(usize, usize, usize, usize)], line: usize) -> Mapped {
    let mut shift: isize = 0;

    for &(old_start, old_lines, new_start, new_lines) in hunks {
        if old_lines == 0 {
            if line > old_start {
                shift += new_lines as isize;
                continue;
            }
            break;
        }
        if line < old_start {
            break;
        }
        if line < old_start + old_lines {
            let at = match new_lines {
                0 => new_start.max(1),
                _ => new_start,
            };
            return Mapped {
                line: at,
                touched: true,
            };
        }
        shift += new_lines as isize - old_lines as isize;
    }

    Mapped {
        line: line.saturating_add_signed(shift).max(1),
        touched: false,
    }
}

/// The map of a file from one version to the next. `None` when the new
/// version deleted the file.
pub async fn of_file(git: &Git, old: &str, new: &str, path: &str) -> Result<Option<FileMap>> {
    let patch = git
        .text(&[
            "diff-tree",
            "-p",
            "--no-color",
            "-U0",
            "-M",
            "--no-ext-diff",
            old,
            new,
        ])
        .await?;

    for file in parse(&patch) {
        let was = file.file.old_path.as_deref().unwrap_or(&file.file.path);
        if was != path {
            continue;
        }
        return Ok(match file.file.status {
            FileStatus::Deleted => None,
            _ => Some(FileMap::of(&file.file.path, &file.hunks)),
        });
    }
    // The new version did not touch the file.
    Ok(Some(FileMap::same(path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{build_repo, commit};

    fn at(hunks: &[(usize, usize, usize, usize)], line: usize) -> (usize, bool) {
        let mapped = map_line(hunks, line);
        (mapped.line, mapped.touched)
    }

    #[test]
    fn a_line_above_every_hunk_stays() {
        assert_eq!(at(&[(10, 2, 10, 5)], 3), (3, false));
    }

    #[test]
    fn a_line_below_a_hunk_moves_by_what_it_added() {
        // Lines 10 and 11 became five lines.
        assert_eq!(at(&[(10, 2, 10, 5)], 20), (23, false));
    }

    #[test]
    fn a_line_inside_a_hunk_goes_to_the_start_of_its_new_text() {
        assert_eq!(at(&[(10, 2, 12, 5)], 11), (12, true));
    }

    #[test]
    fn a_hunk_that_only_adds_pushes_the_lines_after_it() {
        // Three lines added after line 4.
        assert_eq!(at(&[(4, 0, 5, 3)], 4), (4, false));
        assert_eq!(at(&[(4, 0, 5, 3)], 5), (8, false));
    }

    #[test]
    fn a_removed_line_goes_to_the_line_before_the_hole() {
        // Lines 5 to 7 removed; line 4 is the one before them.
        assert_eq!(at(&[(5, 3, 4, 0)], 6), (4, true));
        assert_eq!(at(&[(5, 3, 4, 0)], 9), (6, false));
        // Removed at the top of the file.
        assert_eq!(at(&[(1, 2, 0, 0)], 1), (1, true));
    }

    #[test]
    fn several_hunks_add_up() {
        let hunks = [(2, 0, 3, 2), (10, 1, 12, 0), (20, 1, 21, 3)];
        assert_eq!(at(&hunks, 15), (16, false));
        assert_eq!(at(&hunks, 25), (28, false));
    }

    #[tokio::test]
    async fn a_line_follows_an_edit_made_above_it() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\ntwo\nthree\n"),
            commit("second").file("a.c", "zero\none\ntwo\nTHREE\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (old, new) = (repo.sha("HEAD~1").await, repo.sha("HEAD").await);

        let map = of_file(&git, &old, &new, "a.c").await.unwrap().unwrap();

        assert_eq!(map.path, "a.c");
        assert_eq!(map.map(2).line, 3, "`two` moved down one line");
        assert_eq!(
            map.map(3),
            Mapped {
                line: 4,
                touched: true
            }
        );
    }

    #[tokio::test]
    async fn a_line_follows_its_file_through_a_rename() {
        let body = "one\ntwo\nthree\nfour\nfive\nsix\n";
        let repo = build_repo(&[
            commit("first").file("old.c", body),
            commit("second").delete("old.c").file("new.c", body),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (old, new) = (repo.sha("HEAD~1").await, repo.sha("HEAD").await);

        let map = of_file(&git, &old, &new, "old.c").await.unwrap().unwrap();

        assert_eq!(map.path, "new.c");
        assert_eq!(
            map.map(4),
            Mapped {
                line: 4,
                touched: false
            }
        );
    }

    #[tokio::test]
    async fn a_deleted_file_has_no_map_and_an_untouched_one_stays() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n").file("b.c", "two\n"),
            commit("second").delete("a.c").file("c.c", "three\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (old, new) = (repo.sha("HEAD~1").await, repo.sha("HEAD").await);

        assert_eq!(of_file(&git, &old, &new, "a.c").await.unwrap(), None);
        let same = of_file(&git, &old, &new, "b.c").await.unwrap().unwrap();
        assert_eq!(
            same.map(1),
            Mapped {
                line: 1,
                touched: false
            }
        );
    }
}
