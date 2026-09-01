//! What a commit is, as the review needs it.

use anyhow::{Context, Result, bail};

use super::exec::Git;

/// The separator between two commits in a `git log` answer.
///
/// A record separator, because a commit message can hold anything else.
const RECORD: char = '\x1e';
/// The separator between two fields of one commit.
const FIELD: char = '\x00';

/// `%at` is the author date as a count of seconds, not as a written date.
///
/// A written date is not the same everywhere: one git prints `+00:00` for
/// UTC and another prints `Z`, so the pipeline disagreed with the machine
/// that wrote the test. A count of seconds says the same thing everywhere,
/// and the tool formats it once, its own way.
const FORMAT: &str = "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%s%x00%B%x1e";

/// One commit, with the parts a review reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitInfo {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// The author date, RFC 3339 in UTC, formatted by this tool.
    pub date: String,
    pub subject: String,
    pub message: String,
}

impl CommitInfo {
    /// A merge has more than one parent. Gerrit reviews one, and so does this
    /// tool, but the walk must never cross it in silence.
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }

    /// The `Change-Id` trailer, when the commit carries one.
    ///
    /// It survives an amend and a rebase, which is what a comment must
    /// survive. Only the last one counts: a rebase can leave two behind.
    pub fn change_id(&self) -> Option<&str> {
        self.message
            .lines()
            .rev()
            .find_map(|line| line.strip_prefix("Change-Id:"))
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }

    /// The key a review is stored under.
    ///
    /// The `Change-Id` when there is one. Otherwise the hash, which does not
    /// survive an amend, and the interface says so.
    pub fn key(&self) -> String {
        match self.change_id() {
            Some(id) => id.to_owned(),
            None => format!("sha-{}", self.hash),
        }
    }
}

/// Read one commit.
///
/// A revision that is a full hash is read once for the whole run. Every pane
/// of the interface asks about the commits of the series, and a page load
/// asked git for the same one a dozen times.
pub async fn info(git: &Git, rev: &str) -> Result<CommitInfo> {
    let call = ["log", "-1", FORMAT, rev, "--"];
    let out = match is_object_name(rev) {
        true => git.text_of_object(&call).await?,
        false => git.text(&call).await?,
    };
    let mut commits = parse(&out);

    match commits.len() {
        1 => Ok(commits.remove(0)),
        _ => bail!("{rev} does not name one commit"),
    }
}

/// Whether a revision names an object outright, rather than reaching one.
///
/// A full hash names content, and content never moves. `HEAD`, a branch, a
/// tag or `rev^` all reach a commit that can be another one a moment later.
pub fn is_object_name(rev: &str) -> bool {
    rev.len() == 40 && rev.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Read a range of commits, newest first.
pub async fn range(git: &Git, args: &[&str]) -> Result<Vec<CommitInfo>> {
    let mut call = vec!["log", FORMAT];
    call.extend_from_slice(args);
    call.push("--");

    Ok(parse(&git.text(&call).await?))
}

fn parse(out: &str) -> Vec<CommitInfo> {
    out.split(RECORD)
        .map(str::trim_start)
        .filter(|record| !record.is_empty())
        .filter_map(parse_one)
        .collect()
}

/// A count of seconds since the epoch, as a date the interface can show.
pub fn iso_utc(seconds: &str) -> String {
    let Ok(seconds) = seconds.trim().parse::<i64>() else {
        return String::new();
    };
    let Ok(moment) = time::OffsetDateTime::from_unix_timestamp(seconds) else {
        return String::new();
    };

    moment
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn parse_one(record: &str) -> Option<CommitInfo> {
    let mut fields = record.split(FIELD);
    let hash = fields.next()?.trim().to_owned();
    let parents = fields.next()?;
    let author = fields.next()?.to_owned();
    let email = fields.next()?.to_owned();
    let date = iso_utc(fields.next()?);
    let subject = fields.next()?.to_owned();
    let message = fields.next()?.to_owned();

    if hash.is_empty() {
        return None;
    }
    Some(CommitInfo {
        hash,
        parents: parents.split_whitespace().map(str::to_owned).collect(),
        author,
        email,
        date,
        subject,
        message,
    })
}

/// The `.gerrit-branch` file of a commit, when it has one.
///
/// Read from the commit, never from the working tree: the reviewed series
/// does not have to be the checkout.
pub async fn gerrit_branch(git: &Git, rev: &str) -> Option<String> {
    let out = git
        .text(&["show", &format!("{rev}:.gerrit-branch")])
        .await
        .ok()?;

    out.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
}

/// Resolve a revision to a full hash.
pub async fn resolve(git: &Git, rev: &str) -> Result<String> {
    let out = git
        .text(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ])
        .await
        .with_context(|| format!("{rev} does not name a commit"))?;

    let hash = out.trim().to_owned();
    if hash.is_empty() {
        bail!("{rev} does not name a commit");
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{build_repo, commit, merge};

    #[tokio::test]
    async fn a_commit_carries_its_fields() {
        let repo = build_repo(&[commit("first: do a thing").file("a.txt", "a\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = info(&git, "HEAD").await.unwrap();

        assert_eq!(head.subject, "first: do a thing");
        assert_eq!(head.author, "Test Author");
        assert_eq!(head.email, "author@example.com");
        assert_eq!(head.date, "2026-01-01T00:00:00Z");
        assert!(head.parents.is_empty());
        assert!(!head.is_merge());
    }

    #[test]
    fn only_a_full_hash_names_an_object() {
        assert!(is_object_name("4888eaa8f1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6"));
        assert!(!is_object_name("HEAD"));
        assert!(!is_object_name("main"));
        assert!(!is_object_name("4888eaa8"));
        assert!(!is_object_name("4888eaa8f1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6^"));
        assert!(!is_object_name("zzzzeaa8f1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6"));
    }

    /// A hash names content, so a commit read by hash is read once. A ref
    /// reaches whatever it points at now, so it goes to git every time.
    #[tokio::test]
    async fn a_ref_is_read_again_and_a_hash_is_not() {
        let repo = build_repo(&[commit("first").file("a.txt", "a\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let first = info(&git, "HEAD").await.unwrap();

        repo.add(&commit("second").file("a.txt", "b\n")).await;

        let now = info(&git, "HEAD").await.unwrap();
        assert_eq!(now.subject, "second", "a ref must not answer from memory");

        let by_hash = info(&git, &first.hash).await.unwrap();
        assert_eq!(by_hash, first);
    }

    #[tokio::test]
    async fn a_change_id_is_the_key_and_survives_an_amend() {
        let repo = build_repo(&[commit("first").file("a.txt", "a\n").change_id("I8f3ac21")]).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let before = info(&git, "HEAD").await.unwrap();
        assert_eq!(before.change_id(), Some("I8f3ac21"));
        assert_eq!(before.key(), "I8f3ac21");

        std::fs::write(repo.path().join("a.txt"), "b\n").unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "--no-edit"]).await;

        let after = info(&git, "HEAD").await.unwrap();
        assert_ne!(after.hash, before.hash, "the amend must make a new commit");
        assert_eq!(after.key(), before.key(), "the key must survive the amend");
    }

    #[tokio::test]
    async fn a_commit_without_a_trailer_falls_back_to_the_hash() {
        let repo = build_repo(&[commit("first").file("a.txt", "a\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = info(&git, "HEAD").await.unwrap();

        assert_eq!(head.change_id(), None);
        assert_eq!(head.key(), format!("sha-{}", head.hash));
    }

    #[tokio::test]
    async fn the_last_change_id_wins() {
        let repo = build_repo(&[commit("first")
            .file("a.txt", "a\n")
            .body("Change-Id: Iold")
            .change_id("Inew")])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();

        assert_eq!(info(&git, "HEAD").await.unwrap().change_id(), Some("Inew"));
    }

    #[tokio::test]
    async fn a_merge_has_two_parents() {
        let repo = build_repo(&[
            commit("base").file("f", "a\nb\nc\n"),
            commit("side").on_branch("side").file("f", "a\nB2\nc\n"),
            commit("main").on_branch("main").file("f", "a\nB1\nc\n"),
            merge("Merge side into main")
                .from("side")
                .file("f", "a\nR\nc\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = info(&git, "HEAD").await.unwrap();

        assert!(head.is_merge());
        assert_eq!(head.parents.len(), 2);
    }

    #[tokio::test]
    async fn a_range_comes_back_newest_first() {
        let repo = build_repo(&[
            commit("one").file("a", "1\n"),
            commit("two").file("a", "2\n"),
            commit("three").file("a", "3\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let all = range(&git, &["HEAD"]).await.unwrap();

        let subjects: Vec<_> = all.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["three", "two", "one"]);
    }

    #[tokio::test]
    async fn a_message_with_a_record_separator_does_not_break_the_parser() {
        let repo = build_repo(&[
            commit("odd")
                .file("a", "1\n")
                .body("a line with \x1e in it"),
            commit("next").file("a", "2\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let all = range(&git, &["HEAD"]).await.unwrap();

        // The separator inside the message splits one record in two. The
        // second half has no hash, so it is dropped rather than believed.
        let subjects: Vec<_> = all.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["next", "odd"]);
    }

    #[tokio::test]
    async fn the_gerrit_branch_is_read_from_the_commit() {
        let repo = build_repo(&[
            commit("with the file")
                .file(".gerrit-branch", "# a comment\nrel-3.0\n")
                .file("a", "1\n"),
            commit("without it")
                .delete(".gerrit-branch")
                .file("a", "2\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();

        assert_eq!(
            gerrit_branch(&git, "HEAD~1").await,
            Some("rel-3.0".to_owned())
        );
        assert_eq!(gerrit_branch(&git, "HEAD").await, None);
    }

    #[tokio::test]
    async fn resolve_names_a_commit_or_fails() {
        let repo = build_repo(&[commit("first").file("a", "1\n").tag("v1.0")]).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let head = resolve(&git, "HEAD").await.unwrap();
        assert_eq!(resolve(&git, "v1.0").await.unwrap(), head);
        assert!(resolve(&git, "no-such-ref").await.is_err());
    }
}
