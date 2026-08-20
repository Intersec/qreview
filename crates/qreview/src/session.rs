//! What one run of qreview knows.
//!
//! The command line builds it, the server shares it, and the interface reads
//! it through the API. Nothing here touches the working tree.

use anyhow::Result;

use std::sync::Arc;

use crate::diff;
use crate::git::commit;
use crate::git::exec::Git;
use crate::git::merge::{self, Base};
use crate::highlight::Highlighter;
use crate::lang::Languages;
use crate::model::{FileDiff, FileEntry, RepoInfo, RowKind, Series};
use crate::repo;
use crate::series::{self, Options, Plan};

pub struct Session {
    pub git: Git,
    pub repo: RepoInfo,
    pub langs: Languages,
    pub highlighter: Arc<Highlighter>,
    pub plan: Plan,
    pub series: Series,
}

impl Session {
    /// Open a repository and load the first batch of its series.
    pub async fn open(cwd: &std::path::Path, opts: &Options, langs: Languages) -> Result<Self> {
        Self::with_highlighter(cwd, opts, langs, Arc::new(Highlighter::new())).await
    }

    /// The same, with a highlighter the caller built, so a user grammar
    /// directory is loaded once and not per session.
    pub async fn with_highlighter(
        cwd: &std::path::Path,
        opts: &Options,
        langs: Languages,
        highlighter: Arc<Highlighter>,
    ) -> Result<Self> {
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
            highlighter,
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

    /// The tree a change is diffed against.
    ///
    /// A normal change is diffed against its first parent, or against the
    /// empty tree when it is a root commit. A merge takes the base the reader
    /// picked, and the auto-merge by default.
    pub async fn base_of(&self, rev: &str, base: Option<Base>) -> Result<String> {
        let info = commit::info(&self.git, rev).await?;

        if let Some(base) = base
            && let Some(tree) = merge::base_of(&self.git, &info, base).await
        {
            return Ok(tree);
        }

        // A merge with no base asked for reads against the auto-merge, the
        // way Gerrit shows one.
        if info.is_merge()
            && base.is_none()
            && let Some(tree) = merge::auto_merge_tree(&self.git, &info).await
        {
            return Ok(tree);
        }

        Ok(info
            .parents
            .first()
            .cloned()
            .unwrap_or_else(|| diff::EMPTY_TREE.to_owned()))
    }

    /// The files a change touches.
    pub async fn files(&self, rev: &str, base: Option<Base>) -> Result<Vec<FileEntry>> {
        let base = self.base_of(rev, base).await?;
        let mut entries = diff::files(&self.git, &base, rev).await?;

        for entry in &mut entries {
            entry.language = self.langs.of(&entry.path).unwrap_or_default().to_owned();
        }
        Ok(entries)
    }

    /// The diff of one file of a change.
    pub async fn diff(
        &self,
        rev: &str,
        path: &str,
        base: Option<Base>,
    ) -> Result<Option<FileDiff>> {
        let base = self.base_of(rev, base).await?;
        let old = diff::files(&self.git, &base, rev)
            .await?
            .into_iter()
            .find(|e| e.path == path)
            .and_then(|e| e.old_path);

        let mut found = diff::file(&self.git, &base, rev, path, old.as_deref()).await?;

        if let Some(file) = found.as_mut() {
            let language = self.langs.of(path).map(str::to_owned);
            file.file.language = language.clone().unwrap_or_default();

            if !file.file.binary {
                self.paint(file, rev, &base, old.as_deref(), language.as_deref())
                    .await;
            }

            // The last step before a row leaves the server: the browser
            // slices by UTF-16 units, and everything above counts bytes.
            for hunk in &mut file.hunks {
                for row in &mut hunk.rows {
                    crate::offsets::to_utf16(row);
                }
            }
        }
        Ok(found)
    }

    /// Put the syntax spans on every row.
    ///
    /// A line is highlighted with the whole file around it, never alone: a
    /// block comment or a multi-line string needs the lines before it.
    async fn paint(
        &self,
        file: &mut FileDiff,
        rev: &str,
        base: &str,
        old_path: Option<&str>,
        language: Option<&str>,
    ) {
        let path = file.file.path.clone();
        let new_side = self.blob(rev, &path, language, &path).await;
        let old_side = self
            .blob(base, old_path.unwrap_or(&path), language, &path)
            .await;

        for hunk in &mut file.hunks {
            for row in &mut hunk.rows {
                let (side, line) = match row.kind {
                    RowKind::Remove => (&old_side, row.old_line),
                    _ => (&new_side, row.new_line),
                };
                let Some(spans) = side
                    .as_ref()
                    .and_then(|lines| lines.get(line?.checked_sub(1)?))
                else {
                    continue;
                };
                row.tokens = spans.clone();
            }
        }
    }

    /// Read one side of a file and highlight it, keyed by its blob hash.
    async fn blob(
        &self,
        rev: &str,
        path: &str,
        language: Option<&str>,
        for_path: &str,
    ) -> Option<crate::highlight::Lines> {
        let spec = format!("{rev}:{path}");
        let blob = self.git.text(&["rev-parse", &spec]).await.ok()?;
        let blob = blob.trim().to_owned();
        let text = self.git.text(&["cat-file", "blob", &blob]).await.ok()?;

        Some(self.highlighter.lines(&blob, &text, language, for_path))
    }

    /// The commit a change key names, inside the loaded series.
    ///
    /// A key that is not there is not a failure. It is a question with the
    /// answer "no", and the route turns it into a 404.
    pub fn commit_of(&self, key: &str) -> Option<String> {
        if let Some(change) = self.series.changes.iter().find(|c| c.key == key) {
            return Some(change.commit.clone());
        }

        // The merge under the boundary is reviewable, and it is not in the
        // list: the card is where the reader opens it.
        self.series
            .boundary
            .commit
            .clone()
            .filter(|commit| key == commit || key == format!("sha-{commit}"))
    }

    /// The commits a merge brings in.
    pub async fn merge_list(&self, rev: &str) -> Result<Vec<crate::git::commit::CommitInfo>> {
        let info = commit::info(&self.git, rev).await?;

        merge::merge_list(&self.git, &info).await
    }
}
