//! What one run of qreview knows.
//!
//! The command line builds it, the server shares it, and the interface reads
//! it through the API. Nothing here touches the working tree.

use anyhow::{Context, Result};

use crate::diff;
use crate::git::commit;
use crate::git::exec::Git;
use crate::lang::Languages;
use crate::model::{FileDiff, FileEntry, RepoInfo, Series};
use crate::repo;
use crate::series::{self, Options, Plan};

pub struct Session {
    pub git: Git,
    pub repo: RepoInfo,
    pub langs: Languages,
    pub plan: Plan,
    pub series: Series,
}

impl Session {
    /// Open a repository and load the first batch of its series.
    pub async fn open(cwd: &std::path::Path, opts: &Options, langs: Languages) -> Result<Self> {
        let git = Git::discover(cwd).await?;
        let repo = repo::info(&git).await?;
        let (plan, batch) = series::first_batch(&git, opts).await?;

        let oldest = batch
            .changes
            .last()
            .map(|c| c.commit.clone())
            .unwrap_or_else(|| plan.head.clone());

        let series = Series {
            repo: repo.clone(),
            head: plan.head.clone(),
            oldest,
            changes: batch.changes,
            boundary: batch.boundary,
        };

        Ok(Self {
            git,
            repo,
            langs,
            plan,
            series,
        })
    }

    /// Load the next batch, from the commit the boundary named.
    ///
    /// Loading more only appends. It never changes a diff already shown,
    /// because every change is diffed against its own parent.
    pub async fn extend(&mut self, count: usize) -> Result<usize> {
        let Some(from) = self.series.boundary.commit.clone() else {
            return Ok(0);
        };

        let batch = series::extend(&self.git, &self.plan, &from, count).await?;
        let added = batch.changes.len();

        if let Some(last) = batch.changes.last() {
            self.series.oldest = last.commit.clone();
        }
        self.series.changes.extend(batch.changes);
        self.series.boundary = batch.boundary;

        Ok(added)
    }

    /// The base a change is diffed against: its first parent, or the empty
    /// tree when it has none.
    pub async fn base_of(&self, rev: &str) -> Result<String> {
        let info = commit::info(&self.git, rev).await?;

        Ok(info
            .parents
            .first()
            .cloned()
            .unwrap_or_else(|| diff::EMPTY_TREE.to_owned()))
    }

    /// The files a change touches.
    pub async fn files(&self, rev: &str) -> Result<Vec<FileEntry>> {
        let base = self.base_of(rev).await?;
        let mut entries = diff::files(&self.git, &base, rev).await?;

        for entry in &mut entries {
            entry.language = self.langs.of(&entry.path).unwrap_or_default().to_owned();
        }
        Ok(entries)
    }

    /// The diff of one file of a change.
    pub async fn diff(&self, rev: &str, path: &str) -> Result<Option<FileDiff>> {
        let base = self.base_of(rev).await?;
        let old = diff::files(&self.git, &base, rev)
            .await?
            .into_iter()
            .find(|e| e.path == path)
            .and_then(|e| e.old_path);

        let mut found = diff::file(&self.git, &base, rev, path, old.as_deref()).await?;

        if let Some(file) = found.as_mut() {
            file.file.language = self.langs.of(path).unwrap_or_default().to_owned();
        }
        Ok(found)
    }

    /// The commit a change key names, inside the loaded series.
    pub fn commit_of(&self, key: &str) -> Result<String> {
        self.series
            .changes
            .iter()
            .find(|c| c.key == key)
            .map(|c| c.commit.clone())
            .with_context(|| format!("no change {key} in the series"))
    }
}
