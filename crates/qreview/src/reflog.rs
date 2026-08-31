//! Finding an earlier version of a change that carries no `Change-Id`.
//!
//! A change with a `Change-Id` says who it is in its own message, and an
//! amend keeps it: the review stays under one key, and the versions it was
//! written on are remembered. A change without one is a new commit every
//! time. The key follows the sha, so an amend files the next round under a
//! new name and leaves the round before it under a name nothing claims.
//!
//! Git has not lost the old commit. An amend does not delete it, it stops
//! pointing at it, and the reflog keeps the pointer for ninety days. This
//! reads the reflog once and says which of those commits are earlier
//! versions of a change of the series.
//!
//! What makes one an earlier version:
//!
//! - the same subject, which is what survives an amend of the code;
//! - not reachable from the series head, so a commit of the history that
//!   happens to share a subject is left alone;
//! - and the subject names exactly one change of the series, so an ambiguous
//!   one is left alone too.
//!
//! It is a guess, and a narrow one. What it costs when it is wrong is a
//! patch set in a list and a read-only remark from a round that is over. It
//! moves nothing and it writes nothing.

use std::collections::HashMap;

use crate::git::exec::Git;
use crate::model::ChangeSummary;

/// Every commit the reflog reaches, with the subject it carries.
///
/// One walk for the whole series: the reflog of a repository worked in for a
/// year holds a few thousand entries, and reading it once is a third of a
/// second there.
async fn walk(git: &Git) -> Vec<(String, String)> {
    let Ok(text) = git.text(&["log", "-g", "--all", "--format=%H%x00%s"]).await else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();

    for line in text.lines() {
        let Some((hash, subject)) = line.split_once('\0') else {
            continue;
        };
        if seen.insert(hash.to_owned()) {
            out.push((hash.to_owned(), subject.to_owned()));
        }
    }
    out
}

/// The earlier versions of every change of the series that has no
/// `Change-Id`, by change key.
///
/// A change that carries one is left out: its identity is in its message,
/// and the store already follows it.
pub async fn earlier_versions(
    git: &Git,
    changes: &[ChangeSummary],
    head: &str,
) -> HashMap<String, Vec<String>> {
    let wanted: Vec<&ChangeSummary> = changes
        .iter()
        .filter(|change| change.change_id.is_none() && !change.worktree)
        .collect();
    if wanted.is_empty() {
        return HashMap::new();
    }

    // A subject that two changes of the series share names neither of them.
    let mut count: HashMap<&str, usize> = HashMap::new();
    for change in changes {
        *count.entry(change.subject.as_str()).or_default() += 1;
    }

    let here: std::collections::HashSet<&str> = changes
        .iter()
        .map(|change| change.commit.as_str())
        .collect();
    let mut found: HashMap<String, Vec<String>> = HashMap::new();

    for (hash, subject) in walk(git).await {
        if here.contains(hash.as_str()) {
            continue;
        }
        let Some(change) = wanted
            .iter()
            .find(|change| change.subject == subject && count.get(subject.as_str()) == Some(&1))
        else {
            continue;
        };
        // A commit the series can still reach is a commit of the history,
        // not a version that was amended away.
        if reachable(git, &hash, head).await {
            continue;
        }
        found.entry(change.key.clone()).or_default().push(hash);
    }
    found
}

async fn reachable(git: &Git, commit: &str, head: &str) -> bool {
    git.output(&["merge-base", "--is-ancestor", commit, head])
        .await
        .map(|(ok, _)| ok)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Repo, build_repo, commit};

    fn summary(key: &str, subject: &str, commit: &str, change_id: Option<&str>) -> ChangeSummary {
        ChangeSummary {
            key: key.to_owned(),
            change_id: change_id.map(str::to_owned),
            subject: subject.to_owned(),
            author: "T".to_owned(),
            commit: commit.to_owned(),
            patch_set_count: 1,
            comment_count: 0,
            reviewed: false,
            is_merge: false,
            worktree: false,
        }
    }

    /// A change with no `Change-Id`, amended once.
    async fn amended() -> (Repo, String, String) {
        let repo = build_repo(&[
            commit("base").file("base.txt", "0\n"),
            commit("work: a change with no Change-Id").file("a.txt", "one\ntwo\n"),
        ])
        .await;
        let before = repo.sha("HEAD").await;

        std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;
        let after = repo.sha("HEAD").await;

        (repo, before, after)
    }

    #[tokio::test]
    async fn the_version_before_an_amend_is_found_in_the_reflog() {
        let (repo, before, after) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let changes = [summary(
            &format!("sha-{after}"),
            "work: a change with no Change-Id",
            &after,
            None,
        )];

        let found = earlier_versions(&git, &changes, &after).await;

        assert_eq!(found.get(&format!("sha-{after}")), Some(&vec![before]));
    }

    #[tokio::test]
    async fn a_change_that_carries_a_change_id_is_left_alone() {
        let (repo, _, after) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let changes = [summary(
            "Iwork",
            "work: a change with no Change-Id",
            &after,
            Some("Iwork"),
        )];

        // The store follows a Change-Id on its own. Guessing beside it would
        // only be a way to be wrong.
        assert!(earlier_versions(&git, &changes, &after).await.is_empty());
    }

    #[tokio::test]
    async fn a_commit_of_the_history_is_not_a_version_of_the_change() {
        // Two commits with one subject, both still reachable. The older one
        // is history, not a version that was amended away.
        let repo = build_repo(&[
            commit("base").file("base.txt", "0\n"),
            commit("work: the same words").file("a.txt", "one\n"),
            commit("work: the same words").file("b.txt", "two\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;
        let changes = [summary(
            &format!("sha-{head}"),
            "work: the same words",
            &head,
            None,
        )];

        assert!(earlier_versions(&git, &changes, &head).await.is_empty());
    }

    #[tokio::test]
    async fn a_subject_two_changes_share_names_neither() {
        let (repo, _, after) = amended().await;
        let git = Git::discover(repo.path()).await.unwrap();
        let subject = "work: a change with no Change-Id";
        let changes = [
            summary("sha-one", subject, &after, None),
            summary(
                "sha-two",
                subject,
                "0000000000000000000000000000000000000000",
                None,
            ),
        ];

        assert!(earlier_versions(&git, &changes, &after).await.is_empty());
    }
}
