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

/// Ask Gerrit about one change.
///
/// `None` when the server knows nothing about it, which is the normal answer
/// for a change that was never pushed.
pub async fn query(coords: &Coordinates, change_id: &str) -> Result<Option<Change>> {
    match query_many(coords, &[change_id]).await {
        Ok(found) => Ok(found.into_iter().next()),
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
    let text = match ask(coords, &terms, true).await {
        Ok(text) => text,
        Err(Failed::Unreachable(error)) => return Err(Failed::Unreachable(error)),
        Err(Failed::Refused(_)) => ask(coords, &terms, false).await?,
    };

    Ok(answer::parse(&text))
}

/// What one query asks: the changes, and where they live.
///
/// The changes are one `OR` group of their own, so the project and the
/// branch still apply to every one of them.
fn terms_of(coords: &Coordinates, change_ids: &[&str]) -> String {
    let ids = change_ids
        .iter()
        .map(|id| format!("change:{id}"))
        .collect::<Vec<_>>()
        .join(" OR ");

    let mut terms = vec![format!("({ids})"), format!("project:{}", coords.project)];
    if let Some(branch) = &coords.branch {
        terms.push(format!("branch:{branch}"));
    }
    terms.join(" ")
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
async fn ask(coords: &Coordinates, terms: &str, comments: bool) -> Result<String, Failed> {
    let mut args = vec!["gerrit", "query", "--format=JSON", "--patch-sets"];
    if comments {
        args.push("--comments");
    }
    args.push(terms);

    ssh(coords, &args).await
}

async fn ssh(coords: &Coordinates, args: &[&str]) -> Result<String, Failed> {
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
    let answered = tokio::time::timeout(TIMEOUT, command.output()).await;
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
                TIMEOUT.as_secs()
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

    #[test]
    fn one_change_asks_for_that_change_on_that_branch() {
        let terms = terms_of(&coords(Some("main")), &["I1111"]);

        assert_eq!(terms, "(change:I1111) project:myproject branch:main");
    }

    /// The whole series goes in one query, and the project and the branch
    /// must still hold for every change of it. So the changes are a group.
    #[test]
    fn a_series_is_one_group_of_changes() {
        let terms = terms_of(&coords(Some("main")), &["I1111", "I2222", "I3333"]);

        assert_eq!(
            terms,
            "(change:I1111 OR change:I2222 OR change:I3333) project:myproject branch:main"
        );
    }

    #[test]
    fn a_remote_with_no_branch_asks_about_the_project_alone() {
        let terms = terms_of(&coords(None), &["I1111"]);

        assert_eq!(terms, "(change:I1111) project:myproject");
    }
}
