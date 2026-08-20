//! Test fixtures.
//!
//! Git code is tested against a real repository, never against a mock of the
//! `git` binary. A mock agrees with the code that wrote it, not with git.

// A fixture exposes a whole builder surface. A method waits for the first
// suite that needs it, and an unused one is not a defect here.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tempfile::TempDir;
use tokio::process::Command;

/// A commit to create. `merge` makes the same builder produce a merge.
#[derive(Clone, Debug)]
pub struct Commit {
    subject: String,
    body: Vec<String>,
    files: Vec<(String, String)>,
    deletes: Vec<String>,
    change_id: Option<String>,
    author: Option<(String, String)>,
    branch: Option<String>,
    merge_from: Option<String>,
    tag: Option<String>,
}

pub fn commit(subject: &str) -> Commit {
    Commit::new(subject)
}

/// A merge commit. Name the branch it takes with `from`.
pub fn merge(subject: &str) -> Commit {
    Commit::new(subject)
}

impl Commit {
    pub fn new(subject: &str) -> Self {
        Self {
            subject: subject.to_owned(),
            body: Vec::new(),
            files: Vec::new(),
            deletes: Vec::new(),
            change_id: None,
            author: None,
            branch: None,
            merge_from: None,
            tag: None,
        }
    }

    /// Write a file. The content replaces whatever was there.
    pub fn file(mut self, path: &str, content: &str) -> Self {
        self.files.push((path.to_owned(), content.to_owned()));
        self
    }

    pub fn delete(mut self, path: &str) -> Self {
        self.deletes.push(path.to_owned());
        self
    }

    /// Add a `Change-Id` trailer, the way Gerrit does.
    pub fn change_id(mut self, id: &str) -> Self {
        self.change_id = Some(id.to_owned());
        self
    }

    pub fn author(mut self, name: &str, email: &str) -> Self {
        self.author = Some((name.to_owned(), email.to_owned()));
        self
    }

    pub fn body(mut self, line: &str) -> Self {
        self.body.push(line.to_owned());
        self
    }

    /// Create the branch, or switch to it, before committing.
    pub fn on_branch(mut self, name: &str) -> Self {
        self.branch = Some(name.to_owned());
        self
    }

    /// Merge that branch into the current one. Files written by this commit
    /// are the conflict resolution.
    pub fn from(mut self, branch: &str) -> Self {
        self.merge_from = Some(branch.to_owned());
        self
    }

    pub fn tag(mut self, name: &str) -> Self {
        self.tag = Some(name.to_owned());
        self
    }

    fn message(&self) -> String {
        let mut msg = self.subject.clone();
        if !self.body.is_empty() {
            msg.push_str("\n\n");
            msg.push_str(&self.body.join("\n"));
        }
        if let Some(id) = &self.change_id {
            msg.push_str(&format!("\n\nChange-Id: {id}\n"));
        }
        msg
    }
}

/// A real repository in a temporary directory. It is removed on drop.
pub struct Repo {
    dir: TempDir,
    clock: std::cell::Cell<u32>,
}

impl Repo {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Run a git command and return its trimmed output. It panics on failure,
    /// because a fixture that cannot be built is a broken test, not a result.
    pub async fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(self.path())
            .args(args)
            .stdin(Stdio::null())
            .env("LC_ALL", "C")
            .output()
            .await
            .unwrap_or_else(|e| panic!("cannot run git {args:?}: {e}"));

        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// The same, but a failure is a value rather than a panic.
    async fn try_git(&self, args: &[&str]) -> bool {
        Command::new("git")
            .current_dir(self.path())
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("LC_ALL", "C")
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Add a remote, with the fetch refspec a clone would have.
    ///
    /// `@{upstream}` needs it: git maps a branch to its tracking ref through
    /// the refspec, and answers "not stored as a remote-tracking branch"
    /// without one.
    pub async fn remote(&self, name: &str, url: &str) {
        self.git(&["remote", "add", name, url]).await;
    }

    /// Point a branch at a remote-tracking ref, and create that ref.
    pub async fn track(&self, branch: &str, remote: &str, at: &str) {
        let sha = self.sha(at).await;
        self.git(&[
            "update-ref",
            &format!("refs/remotes/{remote}/{branch}"),
            &sha,
        ])
        .await;
        self.git(&["config", &format!("branch.{branch}.remote"), remote])
            .await;
        self.git(&[
            "config",
            &format!("branch.{branch}.merge"),
            &format!("refs/heads/{branch}"),
        ])
        .await;
    }

    /// The full hash of a revision.
    pub async fn sha(&self, rev: &str) -> String {
        self.git(&["rev-parse", rev]).await
    }

    /// Add one commit to the repository as it stands.
    pub async fn add(&self, spec: &Commit) -> String {
        if let Some(branch) = &spec.branch
            && !self.try_git(&["switch", branch]).await
        {
            self.git(&["switch", "-c", branch]).await;
        }

        if let Some(from) = &spec.merge_from {
            // A conflict is expected and is the point of the fixture, so the
            // failure of `merge` is not an error here.
            self.try_git(&["merge", "--no-commit", "--no-ff", from])
                .await;
        }

        for (path, content) in &spec.files {
            let full: PathBuf = self.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full, content).unwrap();
        }
        for path in &spec.deletes {
            let _ = std::fs::remove_file(self.path().join(path));
        }

        self.git(&["add", "-A"]).await;

        // A fixed, increasing clock. A test that depends on the wall clock is
        // a test that fails on a Monday.
        let n = self.clock.get();
        self.clock.set(n + 1);
        let date = format!("2026-01-01T{:02}:{:02}:00+00:00", n / 60, n % 60);

        let (name, email) = spec
            .author
            .clone()
            .unwrap_or_else(|| ("Test Author".to_owned(), "author@example.com".to_owned()));

        self.git(&[
            "-c",
            &format!("user.name={name}"),
            "-c",
            &format!("user.email={email}"),
            "-c",
            &format!("author.date={date}"),
            "commit",
            "--allow-empty",
            "--date",
            &date,
            "-m",
            &spec.message(),
        ])
        .await;

        if let Some(tag) = &spec.tag {
            self.git(&["tag", tag]).await;
        }
        self.sha("HEAD").await
    }
}

/// Build a repository from a list of commits, in order.
pub async fn build_repo(commits: &[Commit]) -> Repo {
    let repo = Repo {
        dir: tempfile::tempdir().unwrap(),
        clock: std::cell::Cell::new(0),
    };

    repo.git(&["init", "--quiet", "--initial-branch=main", "."])
        .await;
    repo.git(&["config", "user.name", "Test Author"]).await;
    repo.git(&["config", "user.email", "author@example.com"])
        .await;
    repo.git(&["config", "commit.gpgsign", "false"]).await;
    // The fixture must not depend on the machine it runs on. A developer with
    // core.autocrlf=input would otherwise commit different bytes than one
    // without it, and the CRLF case would pass or fail by accident.
    repo.git(&["config", "core.autocrlf", "false"]).await;
    repo.git(&["config", "core.safecrlf", "false"]).await;
    // Keep the fixture readable: no rename detection surprises from a limit.
    repo.git(&["config", "diff.renames", "true"]).await;

    for spec in commits {
        repo.add(spec).await;
    }
    repo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_repository_has_the_commits_in_order() {
        let repo = build_repo(&[
            commit("first").file("a.txt", "a\n"),
            commit("second").file("a.txt", "b\n").change_id("I8f3a"),
        ])
        .await;

        let log = repo.git(&["log", "--format=%s", "--reverse"]).await;
        assert_eq!(log, "first\nsecond");

        let msg = repo.git(&["log", "-1", "--format=%B"]).await;
        assert!(msg.contains("Change-Id: I8f3a"), "{msg}");
    }

    #[tokio::test]
    async fn the_dates_are_fixed_and_increasing() {
        let repo = build_repo(&[
            commit("first").file("a.txt", "a\n"),
            commit("second").file("a.txt", "b\n"),
        ])
        .await;

        let dates = repo.git(&["log", "--format=%aI", "--reverse"]).await;
        assert_eq!(
            dates,
            "2026-01-01T00:00:00+00:00\n2026-01-01T00:01:00+00:00"
        );
    }

    #[tokio::test]
    async fn a_merge_carries_two_parents_and_the_resolution() {
        let repo = build_repo(&[
            commit("base").file("f", "a\nb\nc\n"),
            commit("side change")
                .on_branch("side")
                .file("f", "a\nB2\nc\n"),
            commit("main change")
                .on_branch("main")
                .file("f", "a\nB1\nc\n"),
            merge("Merge branch side into main")
                .from("side")
                .file("f", "a\nRESOLVED\nc\n"),
        ])
        .await;

        let parents = repo.git(&["log", "-1", "--format=%P"]).await;
        assert_eq!(parents.split(' ').count(), 2, "{parents}");

        let content = std::fs::read_to_string(repo.path().join("f")).unwrap();
        assert_eq!(content, "a\nRESOLVED\nc\n");
    }

    #[tokio::test]
    async fn a_tag_points_at_its_commit() {
        let repo = build_repo(&[
            commit("first").file("a.txt", "a\n").tag("v1.0"),
            commit("second").file("a.txt", "b\n"),
        ])
        .await;

        assert_eq!(repo.sha("v1.0").await, repo.sha("HEAD~1").await);
    }
}
