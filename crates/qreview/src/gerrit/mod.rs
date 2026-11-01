//! Reading the patch sets already pushed to Gerrit.
//!
//! qreview only reads. It never votes, never comments on the server, and
//! never pushes. Gerrit is optional at every point: a query that fails, times
//! out, or finds nothing leaves the local review working.

pub mod answer;
pub mod coords;
pub mod posted;

use std::process::Stdio;
use std::time::Duration;

use anyhow::{Result, anyhow};
use tokio::process::Command;

pub use answer::{Change, InlineComment, PatchSet, Person};
pub use coords::Coordinates;

/// How long the query may take before the local review goes on without it.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// How long a query that nobody waits for may take. The column of a blame
/// is on the screen before its links come.
pub const BACKGROUND: Duration = Duration::from_secs(30);

/// Ask Gerrit about one change.
///
/// `None` when the server knows nothing about it, which is the normal answer
/// for a change that was never pushed.
pub async fn query(coords: &Coordinates, change_id: &str, commit: &str) -> Result<Option<Change>> {
    match query_many(coords, &[change_id]).await {
        Ok(found) => Ok(pick(&found, change_id, commit).cloned()),
        Err(failed) => Err(failed.error()),
    }
}

/// Ask Gerrit about a whole series, in one round trip.
///
/// The cost of a query is the round trip, not the work: a server on the
/// other side of a company network answers in half a second whether it is
/// asked about one change or ten. So the changes are asked for together.
///
/// The answer holds only the changes the server knows. A change that was
/// never pushed is simply absent, which is not a failure.
pub async fn query_many(coords: &Coordinates, change_ids: &[&str]) -> Result<Vec<Change>, Failed> {
    if change_ids.is_empty() {
        return Ok(Vec::new());
    }

    let terms = terms_of(coords, change_ids);

    // A server that does not know an option answers nothing at all, and the
    // patch sets would go with the remarks. So the question is asked again
    // without them. Only when the server refused: a server that did not
    // answer at all will not answer a second time either, and the reader is
    // already waiting.
    let text = match ask(coords, &terms, true, TIMEOUT).await {
        Ok(text) => text,
        Err(Failed::Unreachable(error)) => return Err(Failed::Unreachable(error)),
        Err(Failed::Refused(_)) => ask(coords, &terms, false, TIMEOUT).await?,
    };

    Ok(answer::parse(&text))
}

/// What one query asks: the changes, and the project they live in.
///
/// The changes are one `OR` group of their own, so the project applies to
/// every one of them.
///
/// The branch is **not** asked for. A clone cannot read the branch a change
/// was pushed to: `.gerrit-branch` names the integration branch, and work
/// pushed to a feature branch under it carries the same file. A query
/// filtered on the wrong branch matches nothing, and a change that is on the
/// server reads as one that was never pushed. `pick` sorts out the rare
/// answer that holds a change twice.
fn terms_of(coords: &Coordinates, change_ids: &[&str]) -> String {
    let ids = change_ids
        .iter()
        .map(|id| format!("change:{id}"))
        .collect::<Vec<_>>()
        .join(" OR ");

    format!("({ids}) project:{}", coords.project)
}

/// Ask Gerrit which changes hold some commits, in one round trip.
///
/// This is how a line of the blame finds its change on the server. The
/// answer holds only the changes that hold one of the commits. A commit that
/// was never pushed is absent, and that is not a failure.
pub async fn query_commits(coords: &Coordinates, commits: &[&str]) -> Result<Vec<Change>, Failed> {
    if commits.is_empty() {
        return Ok(Vec::new());
    }
    let text = ask(coords, &commit_terms(coords, commits), false, BACKGROUND).await?;

    Ok(answer::parse(&text))
}

/// The commits are one `OR` group, so that the project applies to each.
fn commit_terms(coords: &Coordinates, commits: &[&str]) -> String {
    let commits = commits
        .iter()
        .map(|commit| format!("commit:{commit}"))
        .collect::<Vec<_>>()
        .join(" OR ");

    format!("({commits}) project:{}", coords.project)
}

/// The page of the change that holds a commit in one of its patch sets.
///
/// A server with no web address of its own gives no `url`, and then there
/// is no page to open.
pub fn url_of(changes: &[Change], commit: &str) -> Option<String> {
    changes
        .iter()
        .filter(|change| !change.url.is_empty())
        .find(|change| change.patch_sets.iter().any(|set| set.revision == commit))
        .map(|change| change.url.clone())
}

/// The change an answer means, out of the ones that carry a `Change-Id`.
///
/// A cherry-pick keeps the `Change-Id`, so one project can hold the same one
/// on two branches. The change that holds the commit under review is the
/// right one. Failing that, one still open beats one that is closed: a
/// cherry-pick that landed elsewhere is not what the reader has in front of
/// them.
pub fn pick<'a>(changes: &'a [Change], change_id: &str, commit: &str) -> Option<&'a Change> {
    let mine: Vec<&Change> = changes.iter().filter(|c| c.id == change_id).collect();
    if mine.len() < 2 {
        return mine.first().copied();
    }

    let holds_it = mine
        .iter()
        .find(|c| c.patch_sets.iter().any(|set| set.revision == commit));
    let open = mine.iter().find(|c| c.status == "NEW");

    holds_it.or(open).or(mine.first()).copied()
}

/// Why a query gave nothing.
///
/// The caller decides what to do next, and the two cases differ: a server
/// that said no may still answer another question, and one that said nothing
/// will only make the reader wait again.
pub enum Failed {
    /// The server answered, and said no.
    Refused(anyhow::Error),
    /// Nothing answered: no route, no host key, no time left.
    Unreachable(anyhow::Error),
}

impl Failed {
    pub fn error(self) -> anyhow::Error {
        match self {
            Self::Refused(error) | Self::Unreachable(error) => error,
        }
    }
}

/// One query, with or without the remarks.
async fn ask(
    coords: &Coordinates,
    terms: &str,
    comments: bool,
    limit: Duration,
) -> Result<String, Failed> {
    let mut args = vec!["gerrit", "query", "--format=JSON", "--patch-sets"];
    if comments {
        args.push("--comments");
    }
    args.push(terms);

    ssh(coords, &args, limit).await
}

async fn ssh(coords: &Coordinates, args: &[&str], limit: Duration) -> Result<String, Failed> {
    let mut command = Command::new("ssh");
    command
        .arg("-oBatchMode=yes")
        .arg("-oStrictHostKeyChecking=accept-new")
        .arg("-p")
        .arg(coords.port.to_string())
        .arg(&coords.host)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Never stop on a prompt. An answer that never comes is worse than
        // no answer, because the review waits for it.
        .env("SSH_ASKPASS_REQUIRE", "never")
        .env("GIT_TERMINAL_PROMPT", "0");

    let started = crate::trace::start();
    let answered = tokio::time::timeout(limit, command.output()).await;
    crate::trace::since(started, || {
        format!("ssh {} {}", coords.host, args.join(" "))
    });

    let out = match answered {
        Ok(Ok(out)) => out,
        Ok(Err(error)) => {
            return Err(Failed::Unreachable(
                anyhow::Error::new(error).context("cannot run ssh"),
            ));
        }
        Err(_) => {
            return Err(Failed::Unreachable(anyhow!(
                "{} did not answer in {}s",
                coords.host,
                limit.as_secs()
            )));
        }
    };

    if !out.status.success() {
        let error = String::from_utf8_lossy(&out.stderr);
        return Err(Failed::Refused(anyhow!(
            "the Gerrit query failed: {}",
            error.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coords(branch: Option<&str>) -> Coordinates {
        Coordinates {
            host: "review.example.com".to_owned(),
            port: 29418,
            project: "myproject".to_owned(),
            branch: branch.map(str::to_owned),
        }
    }

    fn change(id: &str, branch: &str, status: &str, revision: &str) -> Change {
        Change {
            project: "myproject".to_owned(),
            branch: branch.to_owned(),
            id: id.to_owned(),
            number: 1,
            subject: "a change".to_owned(),
            url: String::new(),
            status: status.to_owned(),
            patch_sets: vec![PatchSet {
                number: 1,
                revision: revision.to_owned(),
                git_ref: "refs/changes/01/1/1".to_owned(),
                created_on: 0,
                kind: String::new(),
                comments: Vec::new(),
            }],
        }
    }

    #[test]
    fn one_change_asks_for_that_change_in_that_project() {
        let terms = terms_of(&coords(Some("main")), &["I1111"]);

        // The branch is never a term of the query. A clone cannot read the
        // branch a change was pushed to, and a wrong one matches nothing.
        assert_eq!(terms, "(change:I1111) project:myproject");
    }

    /// The whole series goes in one query, and the project must still hold
    /// for every change of it. So the changes are a group.
    #[test]
    fn a_series_is_one_group_of_changes() {
        let terms = terms_of(&coords(Some("main")), &["I1111", "I2222", "I3333"]);

        assert_eq!(
            terms,
            "(change:I1111 OR change:I2222 OR change:I3333) project:myproject"
        );
    }

    #[test]
    fn a_remote_with_no_branch_asks_about_the_project_alone() {
        let terms = terms_of(&coords(None), &["I1111"]);

        assert_eq!(terms, "(change:I1111) project:myproject");
    }

    /// The commits of a blame go in one query, and the project holds for
    /// each of them.
    #[test]
    fn the_commits_of_a_blame_are_one_group() {
        let terms = commit_terms(&coords(None), &["aaaa", "bbbb"]);

        assert_eq!(terms, "(commit:aaaa OR commit:bbbb) project:myproject");
    }

    #[test]
    fn a_commit_opens_the_change_of_the_patch_set_that_holds_it() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/gerrit/one-change.json"
        ))
        .unwrap();
        let mut found = answer::parse(&text);

        // The second patch set of the three, not the current one.
        let url = url_of(&found, "774faaa9e1d2c3b4a5968778695a4b3c2d1e0f9a");
        assert_eq!(
            url.as_deref(),
            Some("https://review.example.com/c/myproject/+/12321")
        );
        assert_eq!(
            url_of(&found, "0000000000000000000000000000000000000000"),
            None
        );

        // A server with no web address gives no page to open.
        found[0].url = String::new();
        assert_eq!(
            url_of(&found, "774faaa9e1d2c3b4a5968778695a4b3c2d1e0f9a"),
            None
        );
    }

    #[test]
    fn one_answer_is_the_answer() {
        let found = [change("I1111", "main", "NEW", "aaa")];

        assert_eq!(pick(&found, "I1111", "zzz").unwrap().branch, "main");
        assert!(pick(&found, "I2222", "aaa").is_none());
    }

    #[test]
    fn the_change_that_holds_the_commit_wins() {
        let found = [
            change("I1111", "rel-3.0", "MERGED", "aaa"),
            change("I1111", "f/rel-3.0/work", "NEW", "bbb"),
        ];

        assert_eq!(
            pick(&found, "I1111", "bbb").unwrap().branch,
            "f/rel-3.0/work"
        );
        assert_eq!(pick(&found, "I1111", "aaa").unwrap().branch, "rel-3.0");
    }

    /// A local amend is on no patch set yet, so nothing matches the commit.
    /// A cherry-pick that landed elsewhere is not what the reader has open.
    #[test]
    fn an_open_change_beats_a_closed_one() {
        let found = [
            change("I1111", "rel-3.0", "MERGED", "aaa"),
            change("I1111", "f/rel-3.0/work", "NEW", "bbb"),
        ];

        assert_eq!(
            pick(&found, "I1111", "zzz").unwrap().branch,
            "f/rel-3.0/work"
        );
    }
}
