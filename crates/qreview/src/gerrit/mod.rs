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

use anyhow::{Context, Result, bail};
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

    let text = ssh(
        coords,
        &[
            "gerrit",
            "query",
            "--format=JSON",
            "--patch-sets",
            // The remarks already posted on each version. A server that does
            // not know the option answers without them, and the review goes
            // on with the patch sets alone.
            "--comments",
            &terms.join(" "),
        ],
    )
    .await?;

    Ok(answer::parse(&text).into_iter().next())
}

async fn ssh(coords: &Coordinates, args: &[&str]) -> Result<String> {
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

    let run = command.output();
    let out = match tokio::time::timeout(TIMEOUT, run).await {
        Ok(Ok(out)) => out,
        Ok(Err(error)) => return Err(error).context("cannot run ssh"),
        Err(_) => bail!("{} did not answer in {}s", coords.host, TIMEOUT.as_secs()),
    };

    if !out.status.success() {
        let error = String::from_utf8_lossy(&out.stderr);
        bail!("the Gerrit query failed: {}", error.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
