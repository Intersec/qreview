//! What one run of qreview knows.
//!
//! The command line builds it, the server shares it, and the interface reads
//! it through the API. Nothing here touches the working tree.

use anyhow::{Context, Result};

use std::sync::Arc;

use crate::comments::{self, EditComment, NewComment, Target};
use crate::diff;
use crate::git::commit;
use crate::git::exec::Git;
use crate::git::merge::{self, Base};
use crate::highlight::Highlighter;
use crate::lang::Languages;
use crate::model::{FileDiff, FileEntry, RepoInfo, RowKind, Series};
use crate::patchset::{self, PatchSet};
use crate::repo;
use crate::series::{self, Options, Plan};
use crate::store::Store;
use crate::store::model::{ChangeFile, Comment};

pub struct Session {
    pub git: Git,
    pub repo: RepoInfo,
    pub langs: Languages,
    pub highlighter: Arc<Highlighter>,
    pub store: Store,
    /// The name comments are written under.
    pub author: String,
    pub plan: Plan,
    pub series: Series,
    /// The commits named with `--prev`, resolved.
    pub prevs: Vec<String>,
}

/// What a diff is read against.
#[derive(Clone, Debug, Default)]
pub enum Against {
    /// The parent of the commit, or the auto-merge when it is a merge.
    #[default]
    Parent,
    /// One of the bases a merge offers.
    Merge(Base),
    /// A tree named outright, which is how one patch set is read against
    /// another.
    Tree(String),
}

impl Session {
    /// Open a repository and load the first batch of its series.
    pub async fn open(cwd: &std::path::Path, opts: &Options, langs: Languages) -> Result<Self> {
        Self::with(cwd, opts, langs, Arc::new(Highlighter::new()), None).await
    }

    /// The same, with the parts the caller owns: a highlighter built once for
    /// the whole run, and a store rooted where the tests want it.
    pub async fn with(
        cwd: &std::path::Path,
        opts: &Options,
        langs: Languages,
        highlighter: Arc<Highlighter>,
        store: Option<Store>,
    ) -> Result<Self> {
        let git = Git::discover(cwd).await?;
        let repo = repo::info(&git).await?;
        let (plan, batch) = series::first_batch(&git, opts).await?;

        let oldest = batch
            .changes
            .last()
            .map(|c| c.commit.clone())
            .unwrap_or_else(|| plan.head.clone());

        let store = match store {
            Some(store) => store,
            None => Store::open(&repo.id)?,
        };
        let author = comments::author_name(&git).await;

        let series = Series {
            repo: repo.clone(),
            head: plan.head.clone(),
            oldest,
            changes: batch.changes,
            boundary: batch.boundary,
        };

        let mut prevs = Vec::new();
        for rev in &opts.prevs {
            match commit::resolve(&git, rev).await {
                Ok(hash) => prevs.push(hash),
                Err(error) => eprintln!("qreview: --prev {rev} is not a commit: {error}"),
            }
        }

        let mut session = Self {
            git,
            repo,
            langs,
            highlighter,
            store,
            author,
            plan,
            series,
            prevs,
        };
        session.count_comments();
        session.count_patch_sets().await;

        Ok(session)
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
        self.count_comments();
        self.count_patch_sets().await;

        Ok(added)
    }

    /// The tree a change is diffed against.
    ///
    /// A normal change is diffed against its first parent, or against the
    /// empty tree when it is a root commit. A merge takes the base the reader
    /// picked, and the auto-merge by default.
    pub async fn base_of(&self, rev: &str, against: &Against) -> Result<String> {
        if let Against::Tree(tree) = against {
            return Ok(tree.clone());
        }

        let info = commit::info(&self.git, rev).await?;

        if let Against::Merge(base) = against
            && let Some(tree) = merge::base_of(&self.git, &info, *base).await
        {
            return Ok(tree);
        }

        // A merge with no base asked for reads against the auto-merge, the
        // way Gerrit shows one.
        if info.is_merge()
            && matches!(against, Against::Parent)
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
    pub async fn files(&self, rev: &str, against: &Against) -> Result<Vec<FileEntry>> {
        let base = self.base_of(rev, against).await?;
        let mut entries = diff::files(&self.git, &base, rev).await?;

        for entry in &mut entries {
            entry.language = self.langs.of(&entry.path).unwrap_or_default().to_owned();
        }
        Ok(entries)
    }

    /// The diff of one file of a change.
    pub async fn diff(&self, rev: &str, path: &str, against: &Against) -> Result<Option<FileDiff>> {
        let base = self.base_of(rev, against).await?;
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

    /// Put the comment counts on the series.
    ///
    /// A change file that cannot be read counts as zero here. The series must
    /// still load, and the failure is said where the change opens.
    fn count_comments(&mut self) {
        for change in &mut self.series.changes {
            let counts = comments::counts(&self.store, &change.key);
            change.comment_count = counts.total;
            change.unresolved_count = counts.unresolved;
        }
    }

    /// The patch sets of a change, oldest first.
    pub async fn patch_sets(&self, key: &str) -> Result<Vec<PatchSet>> {
        let rev = self
            .commit_of(key)
            .with_context(|| format!("no change {key} in the series"))?;
        let info = commit::info(&self.git, &rev).await?;

        patchset::of_change(&self.git, &info, &self.prevs).await
    }

    /// The commit of a patch set, or of the change when none is named.
    pub async fn target_of(&self, key: &str, patch_set: Option<usize>) -> Result<String> {
        let rev = self
            .commit_of(key)
            .with_context(|| format!("no change {key} in the series"))?;

        let Some(number) = patch_set else {
            return Ok(rev);
        };

        self.patch_sets(key)
            .await?
            .into_iter()
            .find(|set| set.number == number)
            .map(|set| set.commit)
            .with_context(|| format!("change {key} has no patch set {number}"))
    }

    async fn count_patch_sets(&mut self) {
        if self.prevs.is_empty() {
            return;
        }

        let keys: Vec<_> = self.series.changes.iter().map(|c| c.key.clone()).collect();
        let mut counts = Vec::with_capacity(keys.len());
        for key in &keys {
            counts.push(self.patch_sets(key).await.map(|s| s.len()).unwrap_or(1));
        }
        for (change, count) in self.series.changes.iter_mut().zip(counts) {
            change.patch_set_count = count;
        }
    }

    /// The review of a change.
    pub fn comments(&self, key: &str, subject: &str) -> Result<ChangeFile> {
        comments::read(&self.store, key, subject)
    }

    /// Write a comment on a change.
    pub async fn add_comment(&self, key: &str, new: NewComment) -> Result<Comment> {
        let change = self
            .series
            .changes
            .iter()
            .find(|c| c.key == key)
            .map(|c| (c.commit.clone(), c.subject.clone()));

        let (rev, subject) = match change {
            Some(pair) => pair,
            None => {
                let commit = self
                    .commit_of(key)
                    .with_context(|| format!("no change {key} in the series"))?;
                let info = commit::info(&self.git, &commit).await?;
                (commit, info.subject)
            }
        };
        let base = self.base_of(&rev, &Against::Parent).await?;

        Target {
            store: &self.store,
            git: &self.git,
            rev: &rev,
            base: &base,
            key,
            subject: &subject,
            author: &self.author,
            // One patch set until M5 adds the rest.
            patch_set: 1,
        }
        .add(new)
        .await
    }

    pub fn edit_comment(&self, key: &str, id: &str, edit: EditComment) -> Result<Comment> {
        comments::edit(&self.store, key, id, edit)
    }

    pub fn delete_comment(&self, key: &str, id: &str) -> Result<usize> {
        comments::delete(&self.store, key, id)
    }

    /// The commits a merge brings in.
    pub async fn merge_list(&self, rev: &str) -> Result<Vec<crate::git::commit::CommitInfo>> {
        let info = commit::info(&self.git, rev).await?;

        merge::merge_list(&self.git, &info).await
    }
}
