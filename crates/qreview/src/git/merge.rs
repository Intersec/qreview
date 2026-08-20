//! Reviewing a merge.
//!
//! The default base of a merge is not a parent. It is the tree git produces
//! on its own from the two parents, so the diff shows the work a person did,
//! which is the conflict resolution and nothing else.

use anyhow::Result;

use super::commit::{self, CommitInfo};
use super::exec::Git;

/// What a merge is diffed against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    /// The tree git merges on its own. The conflict resolution, and nothing
    /// that was already reviewed on either branch.
    AutoMerge,
    Parent(usize),
}

/// The auto-merge tree of a merge commit.
///
/// `None` when git cannot make one: `merge-tree --write-tree` arrived in git
/// 2.38, and an octopus merge has no two-parent answer.
pub async fn auto_merge_tree(git: &Git, info: &CommitInfo) -> Option<String> {
    if info.parents.len() != 2 {
        return None;
    }

    let (ok, out) = git
        .output(&[
            "merge-tree",
            "--write-tree",
            &info.parents[0],
            &info.parents[1],
        ])
        .await
        .ok()?;

    // A conflict makes the exit code non-zero and the tree is still on the
    // first line, which is exactly the case we want to show.
    let first = out.lines().next()?.trim();
    let looks_like_a_tree = first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit());

    if !looks_like_a_tree {
        if !ok {
            // An old git prints usage instead. Say so once, plainly.
            eprintln!("qreview: `git merge-tree --write-tree` failed, git 2.38 or later is needed");
        }
        return None;
    }
    Some(first.to_owned())
}

/// The tree-ish a merge is diffed against, for the base the reader picked.
pub async fn base_of(git: &Git, info: &CommitInfo, base: Base) -> Option<String> {
    match base {
        Base::AutoMerge => auto_merge_tree(git, info).await,
        Base::Parent(n) => info.parents.get(n.saturating_sub(1)).cloned(),
    }
}

/// The commits a merge brings in, newest first.
///
/// A list, not a review surface: the work in them was reviewed on the branch
/// they came from.
pub async fn merge_list(git: &Git, info: &CommitInfo) -> Result<Vec<CommitInfo>> {
    let (Some(first), Some(second)) = (info.parents.first(), info.parents.get(1)) else {
        return Ok(Vec::new());
    };

    commit::range(git, &[&format!("{first}..{second}")]).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff;
    use crate::model::RowKind;
    use crate::testutil::{Repo, build_repo, commit, merge};

    /// A merge whose resolution is a line that is on neither side.
    async fn conflicted() -> Repo {
        build_repo(&[
            commit("base").file("f", "a\nb\nc\n"),
            commit("side work")
                .on_branch("side")
                .file("f", "a\nB2\nc\n")
                .file("only-side.txt", "side\n"),
            commit("main work")
                .on_branch("main")
                .file("f", "a\nB1\nc\n"),
            merge("Merge side into main")
                .from("side")
                .file("f", "a\nRESOLVED\nc\n"),
        ])
        .await
    }

    #[tokio::test]
    async fn the_auto_merge_shows_the_resolution_and_nothing_else() {
        let repo = conflicted().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = commit::info(&git, "HEAD").await.unwrap();

        let tree = auto_merge_tree(&git, &head)
            .await
            .expect("git 2.38 or later makes an auto-merge tree");
        let files = diff::files(&git, &tree, &head.hash, false).await.unwrap();

        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["f"],
            "only the resolved file, not what the merge brought"
        );

        let file = diff::file(&git, &tree, &head.hash, "f", None, false)
            .await
            .unwrap()
            .unwrap();
        let added: Vec<_> = file.hunks[0]
            .rows
            .iter()
            .filter(|r| r.kind == RowKind::Add)
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(added, ["RESOLVED"]);
    }

    #[tokio::test]
    async fn the_first_parent_shows_everything_the_merge_brought() {
        let repo = conflicted().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = commit::info(&git, "HEAD").await.unwrap();

        let base = base_of(&git, &head, Base::Parent(1)).await.unwrap();
        let files = diff::files(&git, &base, &head.hash, false).await.unwrap();

        let mut paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(
            paths,
            ["f", "only-side.txt"],
            "the whole branch, which is what makes it unreadable"
        );
    }

    #[tokio::test]
    async fn the_second_parent_is_the_other_direction() {
        let repo = conflicted().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = commit::info(&git, "HEAD").await.unwrap();

        let one = base_of(&git, &head, Base::Parent(1)).await.unwrap();
        let two = base_of(&git, &head, Base::Parent(2)).await.unwrap();

        assert_eq!(one, head.parents[0]);
        assert_eq!(two, head.parents[1]);
    }

    #[tokio::test]
    async fn the_merge_list_names_what_the_merge_brings_in() {
        let repo = conflicted().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = commit::info(&git, "HEAD").await.unwrap();

        let list = merge_list(&git, &head).await.unwrap();
        let subjects: Vec<_> = list.iter().map(|c| c.subject.as_str()).collect();

        assert_eq!(subjects, ["side work"]);
    }

    #[tokio::test]
    async fn a_commit_that_is_not_a_merge_has_no_auto_merge_tree() {
        let repo = build_repo(&[commit("one").file("a", "1\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = commit::info(&git, "HEAD").await.unwrap();

        assert!(auto_merge_tree(&git, &head).await.is_none());
        assert!(merge_list(&git, &head).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_clean_merge_has_an_empty_auto_merge_diff() {
        let repo = build_repo(&[
            commit("base").file("f", "a\n"),
            commit("side").on_branch("side").file("g", "1\n"),
            commit("main").on_branch("main").file("h", "1\n"),
            merge("Merge side into main").from("side"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = commit::info(&git, "HEAD").await.unwrap();

        let tree = auto_merge_tree(&git, &head).await.unwrap();
        let files = diff::files(&git, &tree, &head.hash, false).await.unwrap();

        assert!(files.is_empty(), "nobody resolved anything: {files:?}");
    }
}
