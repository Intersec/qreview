//! The versions of one change.
//!
//! A patch set is one version of a change. The local commit is always the
//! last one. A commit named with `--prev` joins the list when it carries the
//! same Change-Id, and Gerrit adds the ones already pushed.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::git::commit::{self, CommitInfo};
use crate::git::exec::Git;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    /// The commit under review.
    Local,
    /// A commit named with `--prev`.
    Prev,
    /// A ref fetched from Gerrit.
    Gerrit,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchSet {
    /// 1 based. The local commit has the highest number.
    pub number: usize,
    pub commit: String,
    /// The first parent, which is what this version was written on.
    pub parent: Option<String>,
    pub origin: Origin,
    pub created_at: String,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gerrit_ref: Option<String>,
}

/// The patch sets of a change: the older versions, then the local commit.
///
/// A `--prev` commit that carries another Change-Id belongs to another
/// change, and one that carries none cannot be placed at all.
pub async fn of_change(git: &Git, local: &CommitInfo, prevs: &[String]) -> Result<Vec<PatchSet>> {
    let mut older = Vec::new();

    for rev in prevs {
        let Ok(info) = commit::info(git, rev).await else {
            continue;
        };
        if info.hash == local.hash {
            continue;
        }
        if info.change_id() != local.change_id() {
            continue;
        }
        older.push(info);
    }

    // Oldest first, so the numbers grow the way Gerrit numbers them.
    older.sort_by(|a, b| a.date.cmp(&b.date));
    older.dedup_by(|a, b| a.hash == b.hash);

    let mut sets: Vec<PatchSet> = older
        .into_iter()
        .map(|info| entry(info, Origin::Prev, None))
        .collect();
    sets.push(entry(local.clone(), Origin::Local, None));

    for (index, set) in sets.iter_mut().enumerate() {
        set.number = index + 1;
    }
    Ok(sets)
}

fn entry(info: CommitInfo, origin: Origin, gerrit_ref: Option<String>) -> PatchSet {
    PatchSet {
        number: 0,
        commit: info.hash,
        parent: info.parents.first().cloned(),
        origin,
        created_at: info.date,
        subject: info.subject,
        gerrit_ref,
    }
}

/// Do two patch sets sit on the same base?
///
/// When they do not, a diff between them carries the rebase noise, and the
/// reader has to be told rather than surprised.
pub fn same_base(a: &PatchSet, b: &PatchSet) -> bool {
    match (&a.parent, &b.parent) {
        (Some(one), Some(two)) => one == two,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{build_repo, commit};

    /// Two versions of one change, the older one kept out of the branch.
    async fn amended() -> (crate::testutil::Repo, String) {
        let repo = build_repo(&[
            commit("base").file("a", "0\n"),
            commit("work: do a thing")
                .file("a", "1\n")
                .change_id("I8f3ac21"),
        ])
        .await;

        let first = repo.sha("HEAD").await;
        std::fs::write(repo.path().join("a"), "2\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;

        (repo, first)
    }

    #[tokio::test]
    async fn the_local_commit_is_the_last_patch_set() {
        let (repo, first) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let local = commit::info(&git, "HEAD").await.unwrap();

        let sets = of_change(&git, &local, std::slice::from_ref(&first))
            .await
            .unwrap();

        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].number, 1);
        assert_eq!(sets[0].commit, first);
        assert_eq!(sets[0].origin, Origin::Prev);
        assert_eq!(sets[1].number, 2);
        assert_eq!(sets[1].commit, local.hash);
        assert_eq!(sets[1].origin, Origin::Local);
    }

    #[tokio::test]
    async fn a_change_with_no_prev_has_one_patch_set() {
        let (repo, _) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let local = commit::info(&git, "HEAD").await.unwrap();

        let sets = of_change(&git, &local, &[]).await.unwrap();
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].number, 1);
    }

    #[tokio::test]
    async fn a_prev_of_another_change_is_left_out() {
        let repo = build_repo(&[
            commit("other").file("a", "0\n").change_id("Iother"),
            commit("mine").file("b", "1\n").change_id("Imine"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let local = commit::info(&git, "HEAD").await.unwrap();
        let other = repo.sha("HEAD~1").await;

        let sets = of_change(&git, &local, &[other]).await.unwrap();

        assert_eq!(sets.len(), 1, "a Change-Id that differs is another change");
    }

    #[tokio::test]
    async fn a_prev_that_does_not_exist_is_skipped() {
        let (repo, first) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let local = commit::info(&git, "HEAD").await.unwrap();

        let sets = of_change(&git, &local, &["deadbeef".to_owned(), first])
            .await
            .unwrap();

        assert_eq!(sets.len(), 2);
    }

    #[tokio::test]
    async fn the_same_prev_twice_is_one_patch_set() {
        let (repo, first) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let local = commit::info(&git, "HEAD").await.unwrap();

        let sets = of_change(&git, &local, &[first.clone(), first])
            .await
            .unwrap();

        assert_eq!(sets.len(), 2);
    }

    #[tokio::test]
    async fn two_versions_of_one_change_share_their_base() {
        let (repo, first) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let local = commit::info(&git, "HEAD").await.unwrap();

        let sets = of_change(&git, &local, &[first]).await.unwrap();
        assert!(same_base(&sets[0], &sets[1]), "an amend keeps the parent");
    }

    #[tokio::test]
    async fn a_rebase_moves_the_base_and_it_shows() {
        let repo = build_repo(&[
            commit("base one").file("a", "0\n"),
            commit("work").file("b", "1\n").change_id("Iwork"),
        ])
        .await;
        let first = repo.sha("HEAD").await;

        // Rebase the change onto a new base.
        repo.git(&["switch", "--detach", "HEAD~1"]).await;
        std::fs::write(repo.path().join("c"), "new base\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "-m", "base two"]).await;
        repo.git(&["cherry-pick", &first]).await;

        let git = Git::discover(repo.path()).await.unwrap();
        let local = commit::info(&git, "HEAD").await.unwrap();
        let sets = of_change(&git, &local, &[first]).await.unwrap();

        assert_eq!(sets.len(), 2);
        assert!(
            !same_base(&sets[0], &sets[1]),
            "the parents differ, so a diff between them carries the rebase"
        );
    }
}
