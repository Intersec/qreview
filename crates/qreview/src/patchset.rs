//! The versions of one change.
//!
//! A patch set is one version of a change. The local commit is always the
//! last one. A commit named with `--prev` joins the list when it carries the
//! same Change-Id, and Gerrit adds the ones already pushed.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::git::commit::{self, CommitInfo};
use crate::git::exec::Git;
use crate::model::ChangeSummary;

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
    /// False when the commit is not in this clone yet. Fetch it first.
    pub available: bool,
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

/// The commits the reader named that no change of the series can claim.
///
/// A `--prev` is placed by its `Change-Id`. A series rewritten commit by
/// commit, rather than amended, carries new ones, and then the commit belongs
/// to nothing. Saying so is the whole of this: a `--prev` that is dropped
/// without a word is a long way to look for a reason.
pub fn unclaimed<'a>(prevs: &'a [CommitInfo], changes: &[ChangeSummary]) -> Vec<&'a CommitInfo> {
    prevs
        .iter()
        .filter(|prev| !changes.iter().any(|change| claims(change, prev)))
        .collect()
}

fn claims(change: &ChangeSummary, prev: &CommitInfo) -> bool {
    if change.commit == prev.hash {
        return true;
    }
    match (change.change_id.as_deref(), prev.change_id()) {
        (Some(mine), Some(theirs)) => mine == theirs,
        _ => false,
    }
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
        available: true,
    }
}

/// Merge what Gerrit knows into the local list.
///
/// Gerrit owns the numbering: those numbers are what a reviewer on the server
/// sees, and two people must mean the same thing by "patch set 2". A local
/// commit that Gerrit already has keeps its number and is marked local.
///
/// A version Gerrit never saw is not a patch set of it and takes no number of
/// its own. It gets one past the highest, as a handle to open it by, and the
/// interface calls it local rather than reading a number out to the reader.
///
/// The list is ordered the way the versions were written, and the commit
/// under review is last whatever the dates say: it is the newest thing there
/// is, and it is what a reader opens on.
pub fn merge_gerrit(local: Vec<PatchSet>, remote: &[crate::gerrit::PatchSet]) -> Vec<PatchSet> {
    if remote.is_empty() {
        return local;
    }
    let reviewed = local.last().map(|set| set.commit.clone());

    let mut out: Vec<PatchSet> = remote
        .iter()
        .map(|set| PatchSet {
            number: set.number,
            commit: set.revision.clone(),
            parent: None,
            origin: Origin::Gerrit,
            created_at: commit::iso_utc(&set.created_on.to_string()),
            subject: String::new(),
            gerrit_ref: Some(set.git_ref.clone()),
            available: false,
        })
        .collect();

    let mut next = out.iter().map(|s| s.number).max().unwrap_or(0);

    for mut set in local {
        match out.iter_mut().find(|known| known.commit == set.commit) {
            // The server has this commit already. Keep its number, and say
            // that it is here, because a fetch is not needed.
            Some(known) => {
                set.number = known.number;
                set.gerrit_ref = known.gerrit_ref.clone();
                *known = set;
            }
            None => {
                next += 1;
                set.number = next;
                out.push(set);
            }
        }
    }

    let under_review = |set: &PatchSet| Some(&set.commit) == reviewed.as_ref();
    out.sort_by(|a, b| {
        under_review(a)
            .cmp(&under_review(b))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.number.cmp(&b.number))
    });
    out
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

    fn summary(commit: &str, change_id: Option<&str>) -> ChangeSummary {
        ChangeSummary {
            key: change_id.unwrap_or(commit).to_owned(),
            change_id: change_id.map(str::to_owned),
            subject: "work".to_owned(),
            author: "T".to_owned(),
            commit: commit.to_owned(),
            patch_set_count: 1,
            comment_count: 0,
            reviewed: false,
            is_merge: false,
            worktree: false,
        }
    }

    fn named(hash: &str, change_id: Option<&str>) -> CommitInfo {
        let trailer = change_id
            .map(|id| format!("\n\nChange-Id: {id}"))
            .unwrap_or_default();

        CommitInfo {
            hash: hash.to_owned(),
            parents: Vec::new(),
            author: "T".to_owned(),
            email: "t@e".to_owned(),
            date: String::new(),
            subject: "work".to_owned(),
            message: format!("work{trailer}"),
        }
    }

    #[test]
    fn a_prev_with_the_change_id_of_a_change_is_claimed() {
        let changes = [summary("new", Some("Iwork"))];
        let prevs = [named("old", Some("Iwork"))];

        assert!(unclaimed(&prevs, &changes).is_empty());
    }

    #[test]
    fn a_prev_that_is_a_commit_of_the_series_is_claimed() {
        let changes = [summary("here", None)];
        let prevs = [named("here", None)];

        assert!(unclaimed(&prevs, &changes).is_empty());
    }

    #[test]
    fn a_prev_whose_change_id_names_nothing_here_is_unclaimed() {
        // What a series rewritten commit by commit does: every commit comes
        // back with a new Change-Id, and the version reviewed belongs to
        // none of them.
        let changes = [summary("new", Some("Ifresh"))];
        let prevs = [named("old", Some("Ibefore"))];

        let lost = unclaimed(&prevs, &changes);
        assert_eq!(lost.len(), 1);
        assert_eq!(lost[0].hash, "old");
    }

    #[test]
    fn a_prev_with_no_change_id_is_unclaimed() {
        let changes = [summary("new", Some("Iwork"))];
        let prevs = [named("old", None)];

        assert_eq!(unclaimed(&prevs, &changes).len(), 1);
    }

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

    fn remote(number: usize, revision: &str) -> crate::gerrit::PatchSet {
        crate::gerrit::PatchSet {
            number,
            revision: revision.to_owned(),
            git_ref: format!("refs/changes/21/12321/{number}"),
            created_on: 0,
            kind: "REWORK".to_owned(),
            comments: Vec::new(),
        }
    }

    #[test]
    fn gerrit_owns_the_numbering() {
        let local = vec![PatchSet {
            number: 1,
            commit: "local".to_owned(),
            parent: None,
            origin: Origin::Local,
            created_at: String::new(),
            subject: "mine".to_owned(),
            gerrit_ref: None,
            available: true,
        }];

        let merged = merge_gerrit(local, &[remote(1, "aaa"), remote(2, "bbb")]);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].number, 1);
        assert_eq!(merged[0].origin, Origin::Gerrit);
        assert!(!merged[0].available, "a pushed version is not here yet");
        assert_eq!(merged[2].number, 3, "the local commit comes after them");
        assert_eq!(merged[2].origin, Origin::Local);
    }

    #[test]
    fn a_local_commit_that_gerrit_has_keeps_its_number() {
        let local = vec![PatchSet {
            number: 1,
            commit: "bbb".to_owned(),
            parent: None,
            origin: Origin::Local,
            created_at: String::new(),
            subject: "mine".to_owned(),
            gerrit_ref: None,
            available: true,
        }];

        let merged = merge_gerrit(local, &[remote(1, "aaa"), remote(2, "bbb")]);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].number, 2);
        assert_eq!(merged[1].origin, Origin::Local, "it is here, so no fetch");
        assert!(merged[1].available);
        assert_eq!(
            merged[1].gerrit_ref.as_deref(),
            Some("refs/changes/21/12321/2")
        );
    }

    /// A version on the server, pushed at a moment of its own.
    fn pushed(number: usize, revision: &str, created_on: i64) -> crate::gerrit::PatchSet {
        crate::gerrit::PatchSet {
            created_on,
            ..remote(number, revision)
        }
    }

    fn version(commit: &str, when: &str, origin: Origin) -> PatchSet {
        PatchSet {
            number: 0,
            commit: commit.to_owned(),
            parent: None,
            origin,
            created_at: when.to_owned(),
            subject: "work".to_owned(),
            gerrit_ref: None,
            available: true,
        }
    }

    #[test]
    fn the_commit_under_review_is_the_last_version() {
        // A version reviewed before anything was pushed, and never pushed
        // itself. It used to land after the newest, because a local commit
        // the server does not know was taken for a newer one.
        let local = vec![
            version("older", "2001-01-01T00:00:00Z", Origin::Prev),
            version("head", "2002-01-01T00:00:00Z", Origin::Local),
        ];
        // 1000000000 is 2001-09-09, between the two.
        let merged = merge_gerrit(
            local,
            &[pushed(1, "pushed", 1_000_000_000), remote(2, "head")],
        );

        let order: Vec<&str> = merged.iter().map(|set| set.commit.as_str()).collect();
        assert_eq!(order, ["older", "pushed", "head"]);
        assert_eq!(merged[2].origin, Origin::Local, "the one being reviewed");
        assert_eq!(merged[1].number, 1, "Gerrit still owns its numbering");
        assert_eq!(merged[2].number, 2);
    }

    #[test]
    fn a_version_gerrit_never_saw_takes_a_number_past_the_highest() {
        let local = vec![
            version("older", "2001-01-01T00:00:00Z", Origin::Prev),
            version("head", "2002-01-01T00:00:00Z", Origin::Local),
        ];
        let merged = merge_gerrit(
            local,
            &[pushed(1, "pushed", 1_000_000_000), remote(2, "head")],
        );

        // It is not a patch set of the change and takes no number of one.
        // The interface calls it local; this is only a handle to open it by.
        assert_eq!(merged[0].commit, "older");
        assert_eq!(merged[0].number, 3);
        assert!(merged[0].gerrit_ref.is_none());
    }

    #[test]
    fn a_version_only_gerrit_has_carries_the_date_it_was_pushed() {
        let merged = merge_gerrit(
            vec![version("head", "2002-01-01T00:00:00Z", Origin::Local)],
            &[remote(1, "pushed"), remote(2, "head")],
        );

        // `remote` pushes at the epoch. Without a date nothing can be put in
        // order, and the picker showed a version with no date at all.
        assert_eq!(merged[0].commit, "pushed");
        assert_eq!(merged[0].created_at, "1970-01-01T00:00:00Z");
    }

    #[test]
    fn no_answer_from_gerrit_leaves_the_local_list_alone() {
        let local = vec![PatchSet {
            number: 1,
            commit: "local".to_owned(),
            parent: None,
            origin: Origin::Local,
            created_at: String::new(),
            subject: "mine".to_owned(),
            gerrit_ref: None,
            available: true,
        }];

        assert_eq!(merge_gerrit(local.clone(), &[]), local);
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
