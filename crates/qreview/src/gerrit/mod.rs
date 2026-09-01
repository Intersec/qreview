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
    let mut terms = vec![
        format!("change:{change_id}"),
        format!("project:{}", coords.project),
    ];
    if let Some(branch) = &coords.branch {
        terms.push(format!("branch:{branch}"));
    }
    let terms = terms.join(" ");

    // A server that does not know an option answers nothing at all, and the
    // patch sets would go with the remarks. So the question is asked again
    // without them. Only when the server refused: a server that did not
    // answer at all will not answer a second time either, and the reader is
    // already waiting.
    let text = match ask(coords, &terms, true).await {
        Ok(text) => text,
        Err(Refusal::Unreachable(error)) => return Err(error),
        Err(Refusal::Refused(_)) => {
            ask(coords, &terms, false)
                .await
                .map_err(|refusal| match refusal {
                    Refusal::Unreachable(error) | Refusal::Refused(error) => error,
                })?
        }
    };

    Ok(answer::parse(&text).into_iter().next())
}

/// Why a query gave nothing.
enum Refusal {
    /// The server answered, and said no.
    Refused(anyhow::Error),
    /// Nothing answered: no route, no host key, no time left.
    Unreachable(anyhow::Error),
}

/// One query, with or without the remarks.
async fn ask(coords: &Coordinates, terms: &str, comments: bool) -> Result<String, Refusal> {
    let mut args = vec!["gerrit", "query", "--format=JSON", "--patch-sets"];
    if comments {
        args.push("--comments");
    }
    args.push(terms);

    ssh(coords, &args).await
}

async fn ssh(coords: &Coordinates, args: &[&str]) -> Result<String, Refusal> {
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
            return Err(Refusal::Unreachable(
                anyhow::Error::new(error).context("cannot run ssh"),
            ));
        }
        Err(_) => {
            return Err(Refusal::Unreachable(anyhow!(
                "{} did not answer in {}s",
                coords.host,
                TIMEOUT.as_secs()
            )));
        }
    };

    if !out.status.success() {
        let error = String::from_utf8_lossy(&out.stderr);
        return Err(Refusal::Refused(anyhow!(
            "the Gerrit query failed: {}",
            error.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
