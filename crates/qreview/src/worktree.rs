//! The work that is not committed yet, as a change of the series.
//!
//! A reviewer reads their own series before pushing it. The last thing they
//! write is not in it: it sits in the working tree, and the commit that will
//! carry it does not exist. This module gives that work a commit, so every
//! other part of the tool can read it the way it reads any other.
//!
//! Nothing here reads a file of the working tree, and nothing writes one.
//! `git stash create` hashes the tracked changes into the object database and
//! returns a commit for them, without touching the index, the working tree or
//! a ref. From that point on the change is an ordinary commit, read from the
//! object database like the rest. See `roadmap/stack.md`, 2026-08-27.

use crate::git::commit;
use crate::git::exec::Git;
use crate::model::ChangeSummary;

/// The key the store files the remarks under.
///
/// Not a `Change-Id` and not a sha: the commit changes at every keystroke,
/// and a remark on the work must outlive that.
pub const KEY: &str = "working-tree";

/// What the series pane calls it.
pub const SUBJECT: &str = "Uncommitted changes";

/// The dates the synthetic commit is stamped with.
///
/// Fixed, so the same working tree always gives the same commit. A commit
/// that changed at every reload would change the sha on the screen, and write
/// one more object for nothing. Nothing shows this date: the change has none.
const EPOCH: &str = "@0 +0000";

/// The commit that holds the tracked changes that are not committed yet.
///
/// `None` when the working tree is clean, when there is no commit to stand
/// on, or when git says anything unexpected. The feature is a convenience;
/// losing it must never stop a review.
pub async fn commit_of(git: &Git) -> Option<String> {
    let stash = git.text(&["stash", "create"]).await.ok()?;
    let stash = stash.trim();
    if stash.is_empty() {
        return None;
    }

    // The stash commit is a merge of HEAD and the index, and a merge is a
    // boundary everywhere else in the tool. Only its tree is wanted.
    let tree = git
        .text(&["rev-parse", &format!("{stash}^{{tree}}")])
        .await
        .ok()?;
    let head = commit::resolve(git, "HEAD").await.ok()?;

    let made = git
        .text_with(
            &[
                // A signed synthetic commit would ask for a passphrase, and
                // nothing will ever read the signature.
                "-c",
                "commit.gpgsign=false",
                "commit-tree",
                tree.trim(),
                "-p",
                &head,
                "-m",
                SUBJECT,
            ],
            &[("GIT_AUTHOR_DATE", EPOCH), ("GIT_COMMITTER_DATE", EPOCH)],
        )
        .await
        .ok()?;
    let made = made.trim().to_owned();

    (!made.is_empty()).then_some(made)
}

/// The entry the series pane shows for it.
pub fn summary(hash: &str, author: &str) -> ChangeSummary {
    ChangeSummary {
        key: KEY.to_owned(),
        change_id: None,
        subject: SUBJECT.to_owned(),
        author: author.to_owned(),
        commit: hash.to_owned(),
        patch_set_count: 1,
        comment_count: 0,
        reviewed: false,
        is_merge: false,
        worktree: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{build_repo, commit as make};

    #[tokio::test]
    async fn a_clean_tree_is_no_change() {
        let repo = build_repo(&[make("one").file("a.txt", "1\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();

        assert_eq!(commit_of(&git).await, None);
    }

    #[tokio::test]
    async fn a_changed_file_becomes_a_commit_on_head() {
        let repo = build_repo(&[make("one").file("a.txt", "one\ntwo\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").unwrap();

        let hash = commit_of(&git).await.expect("the tree is dirty");
        let info = commit::info(&git, &hash).await.unwrap();

        assert_eq!(info.subject, SUBJECT);
        assert_eq!(info.parents, vec![repo.sha("HEAD").await]);
        assert!(!info.is_merge(), "a merge would be a boundary");

        let shown = git.text(&["show", &format!("{hash}:a.txt")]).await.unwrap();
        assert_eq!(shown, "one\nTWO\n");
    }

    #[tokio::test]
    async fn the_same_tree_gives_the_same_commit() {
        let repo = build_repo(&[make("one").file("a.txt", "one\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();

        // Two reloads must not move the sha on the screen, nor write a
        // second object for the same work.
        assert_eq!(commit_of(&git).await, commit_of(&git).await);
    }

    #[tokio::test]
    async fn a_staged_change_counts_and_an_untracked_file_does_not() {
        let repo = build_repo(&[make("one").file("a.txt", "one\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();

        std::fs::write(repo.path().join("b.txt"), "staged\n").unwrap();
        repo.git(&["add", "b.txt"]).await;
        std::fs::write(repo.path().join("c.txt"), "untracked\n").unwrap();

        let hash = commit_of(&git).await.expect("the index is dirty");
        let names = git
            .text(&["ls-tree", "--name-only", &format!("{hash}^{{tree}}")])
            .await
            .unwrap();

        assert_eq!(
            names.split_whitespace().collect::<Vec<_>>(),
            ["a.txt", "b.txt"]
        );
    }

    #[tokio::test]
    async fn nothing_of_the_working_tree_moves() {
        let repo = build_repo(&[make("one").file("a.txt", "one\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        std::fs::write(repo.path().join("a.txt"), "two\n").unwrap();

        commit_of(&git).await.expect("the tree is dirty");

        let status = git.text(&["status", "--porcelain"]).await.unwrap();
        assert_eq!(status.trim(), "M a.txt".trim_start());
        assert_eq!(
            std::fs::read_to_string(repo.path().join("a.txt")).unwrap(),
            "two\n"
        );
    }
}
