//! Every call to git goes through here.
//!
//! git runs as a child process, never as a library. The tool must agree with
//! what the developer sees in the terminal, `diff.algorithm` included.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tokio::process::Command;

/// How long a single git call may take. Local git reads the object database
/// and answers in milliseconds. A call that runs this long is stuck.
const TIMEOUT: Duration = Duration::from_secs(30);

/// A repository, addressed by its top level directory.
///
/// A clone of it is the same repository, and shares what it has already
/// read. This is what lets a background task hold one.
#[derive(Clone, Debug)]
pub struct Git {
    root: PathBuf,
    /// The answers of the calls that can only ever answer one thing. See
    /// `text_of_object`.
    answers: Arc<Mutex<HashMap<String, String>>>,
}

impl Git {
    /// Find the repository that contains `cwd`.
    pub async fn discover(cwd: &Path) -> Result<Self> {
        let out = run_in(cwd, &["rev-parse", "--show-toplevel"], &[]).await?;
        let root = String::from_utf8(out.stdout)
            .context("git printed a path that is not UTF-8")?
            .trim()
            .to_owned();

        if root.is_empty() {
            bail!("{} is not inside a git repository", cwd.display());
        }
        Ok(Self {
            root: PathBuf::from(root),
            answers: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Run git and return its standard output as text.
    pub async fn text<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<String> {
        self.text_with(args, &[]).await
    }

    /// The same, answered from memory when this run made the call before.
    ///
    /// Only for a call that asks about an object named by its full hash. A
    /// hash names content, so such a call has one answer for as long as the
    /// process lives. Anything else — a ref, `HEAD`, the working tree —
    /// must go to git every time.
    ///
    /// A call that failed is never kept. `cat-file -e` on a commit this
    /// clone does not hold is the case that matters: the reader can fetch
    /// it, and then the answer is yes.
    pub async fn text_of_object<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<String> {
        let key = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\0");

        if let Some(hit) = self.answers.lock().unwrap().get(&key) {
            crate::trace::note(|| format!("git {}, from the cache", describe(args)));
            return Ok(hit.clone());
        }

        let text = self.text(args).await?;
        self.answers.lock().unwrap().insert(key, text.clone());

        Ok(text)
    }

    /// The same, with a few variables added to the environment.
    ///
    /// Only `commit-tree` needs this, to fix the dates it stamps. Everything
    /// else git takes from a `-c` option or from the repository.
    pub async fn text_with<S: AsRef<OsStr>>(
        &self,
        args: &[S],
        env: &[(&str, &str)],
    ) -> Result<String> {
        let out = run_in(&self.root, args, env).await?;
        String::from_utf8(out.stdout).context("git printed output that is not UTF-8")
    }

    /// Run git and return its output whether it succeeded or not.
    ///
    /// Some commands answer with a non-zero code and a useful stdout.
    /// `merge-tree` does exactly that when the merge conflicts, which is the
    /// case we care about most.
    pub async fn output<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<(bool, String)> {
        let out = raw(&self.root, args, &[]).await?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();

        Ok((out.status.success(), text))
    }
}

async fn run_in<S: AsRef<OsStr>>(
    dir: &Path,
    args: &[S],
    env: &[(&str, &str)],
) -> Result<std::process::Output> {
    let out = raw(dir, args, env).await?;

    if !out.status.success() {
        let cmd = describe(args);
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        bail!("`git {cmd}` failed: {err}");
    }
    Ok(out)
}

async fn raw<S: AsRef<OsStr>>(
    dir: &Path,
    args: &[S],
    env: &[(&str, &str)],
) -> Result<std::process::Output> {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A pager, a prompt, or a translated message would all break parsing.
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");

    for (name, value) in env {
        cmd.env(name, value);
    }

    let started = crate::trace::start();
    let out = tokio::time::timeout(TIMEOUT, cmd.output()).await;
    crate::trace::since(started, || format!("git {}", describe(args)));

    match out {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(anyhow!("git is not on the PATH"))
        }
        Ok(Err(e)) => Err(e).with_context(|| format!("cannot run `git {}`", describe(args))),
        Err(_) => Err(anyhow!(
            "`git {}` did not answer in {}s",
            describe(args),
            TIMEOUT.as_secs()
        )),
    }
}

fn describe<S: AsRef<OsStr>>(args: &[S]) -> String {
    args.iter()
        .map(|a| a.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Commit, build_repo};

    #[tokio::test]
    async fn discover_finds_the_top_level() {
        let repo = build_repo(&[Commit::new("first").file("a.txt", "a\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();

        assert_eq!(
            git.root().canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn discover_works_from_a_subdirectory() {
        let repo = build_repo(&[Commit::new("first").file("src/a.txt", "a\n")]).await;
        let git = Git::discover(&repo.path().join("src")).await.unwrap();

        assert_eq!(
            git.root().canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn a_directory_outside_a_repository_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = Git::discover(dir.path()).await.unwrap_err().to_string();

        assert!(
            err.contains("not inside a git repository") || err.contains("failed"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_failing_command_carries_the_message_of_git() {
        let repo = build_repo(&[Commit::new("first").file("a.txt", "a\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let err = git
            .text(&["rev-parse", "no-such-ref"])
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("rev-parse no-such-ref"), "{err}");
    }
}
