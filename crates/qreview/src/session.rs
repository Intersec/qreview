//! What one run of qreview knows.
//!
//! The command line builds it, the server shares it, and the interface reads
//! it through the API. Nothing here touches the working tree.

use anyhow::{Context, Result};

use std::sync::Arc;

use crate::comments::{self, EditComment, NewComment, Target};
use crate::diff;
use crate::gerrit::{self, Coordinates};
use crate::git::commit;
use crate::git::exec::Git;
use crate::git::merge::{self, Base};
use crate::highlight::Highlighter;
use crate::lang::Languages;
use crate::model::{BoundaryKind, FileDiff, FileEntry, RepoInfo, RowKind, Series};
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
    pub plan: Plan,
    pub series: Series,
    /// The commits named with `--prev`, resolved.
    pub prevs: Vec<String>,
    /// Where the Gerrit server is, when the remote names one.
    pub gerrit: Option<Coordinates>,
    /// One query per change, kept for the life of the run.
    gerrit_answers: tokio::sync::Mutex<std::collections::HashMap<String, Option<gerrit::Change>>>,
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

        let gerrit = match opts.gerrit {
            true => coords_of(&git, &plan.head, opts.integration_branch.as_deref()).await,
            false => None,
        };

        let mut session = Self {
            git,
            repo,
            langs,
            highlighter,
            store,
            plan,
            series,
            prevs,
            gerrit,
            gerrit_answers: tokio::sync::Mutex::new(std::collections::HashMap::new()),
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
        let boundary = self.series.boundary.clone();
        let Some(mut from) = boundary.commit.clone() else {
            return Ok(0);
        };

        let mut changes = Vec::new();

        // Past a merge, the merge itself joins the list: it is reviewable,
        // and the card that held it is about to be replaced. The walk then
        // continues on the first parent, never on the second.
        if boundary.kind == BoundaryKind::Merge {
            let info = commit::info(&self.git, &from).await?;
            let Some(parent) = info.parents.first().cloned() else {
                return Ok(0);
            };
            changes.push(series::summary(info));
            from = parent;
        }

        // The base is where the first batch stopped. Going further back is
        // exactly what the reader asked for, so it no longer applies.
        let plan = Plan {
            base: None,
            ..self.plan.clone()
        };
        let batch = series::extend(&self.git, &plan, &from, count).await?;

        changes.extend(batch.changes);
        let added = changes.len();

        if let Some(last) = changes.last() {
            self.series.oldest = last.commit.clone();
        }
        self.series.changes.extend(changes);
        self.series.boundary = batch.boundary;
        self.plan = plan;
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
    pub async fn files(
        &self,
        rev: &str,
        against: &Against,
        ignore_ws: bool,
    ) -> Result<Vec<FileEntry>> {
        let base = self.base_of(rev, against).await?;
        let mut entries = diff::files(&self.git, &base, rev, ignore_ws).await?;

        // Two versions of one change, read against each other. Between them
        // sits everything the rebase brought, and none of it is the work.
        // Only the files the change itself touches are worth a row.
        if let Against::Tree(other) = against
            && let Some(touched) = self.touched_by(&[rev, other], ignore_ws).await
        {
            entries.retain(|entry| {
                touched.contains(&entry.path)
                    || entry
                        .old_path
                        .as_ref()
                        .is_some_and(|old| touched.contains(old))
            });
        }

        for entry in &mut entries {
            entry.language = self.langs.of(&entry.path).unwrap_or_default().to_owned();
        }
        Ok(entries)
    }

    /// The files that these commits touch, each against its own parent.
    async fn touched_by(
        &self,
        revs: &[&str],
        ignore_ws: bool,
    ) -> Option<std::collections::HashSet<String>> {
        let mut touched = std::collections::HashSet::new();

        for rev in revs {
            let info = commit::info(&self.git, rev).await.ok()?;
            let parent = info
                .parents
                .first()
                .cloned()
                .unwrap_or_else(|| diff::EMPTY_TREE.to_owned());

            for entry in diff::files(&self.git, &parent, rev, ignore_ws).await.ok()? {
                if let Some(old) = entry.old_path {
                    touched.insert(old);
                }
                touched.insert(entry.path);
            }
        }
        Some(touched)
    }

    /// The diff of one file of a change.
    pub async fn diff(
        &self,
        rev: &str,
        path: &str,
        against: &Against,
        ignore_ws: bool,
    ) -> Result<Option<FileDiff>> {
        let base = self.base_of(rev, against).await?;
        let old = diff::files(&self.git, &base, rev, ignore_ws)
            .await?
            .into_iter()
            .find(|e| e.path == path)
            .and_then(|e| e.old_path);

        let mut found = diff::file(&self.git, &base, rev, path, old.as_deref(), ignore_ws).await?;

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

    /// A run of lines of a file, as context rows.
    ///
    /// The diff carries only what changed and the few lines around it. This
    /// is how the reader opens the rest, a piece at a time.
    pub async fn lines(
        &self,
        rev: &str,
        path: &str,
        from: usize,
        to: usize,
    ) -> Result<Vec<crate::model::Row>> {
        let language = self.langs.of(path).map(str::to_owned);
        let Some((text, spans)) = self.read_and_paint(rev, path, language.as_deref()).await else {
            return Ok(Vec::new());
        };

        let all: Vec<&str> = text.lines().collect();
        let from = from.max(1);
        let to = to.min(all.len());
        let mut rows = Vec::new();

        for number in from..=to {
            let mut row = crate::model::Row {
                kind: RowKind::Context,
                old_line: None,
                new_line: Some(number),
                text: all[number - 1].to_owned(),
                no_newline: false,
                tokens: spans.get(number - 1).cloned().unwrap_or_default(),
                words: Vec::new(),
            };
            crate::offsets::to_utf16(&mut row);
            rows.push(row);
        }
        Ok(rows)
    }

    /// The text of a file and its syntax spans, both from the object
    /// database.
    async fn read_and_paint(
        &self,
        rev: &str,
        path: &str,
        language: Option<&str>,
    ) -> Option<(String, crate::highlight::Lines)> {
        let spec = format!("{rev}:{path}");
        let blob = self.git.text(&["rev-parse", &spec]).await.ok()?;
        let blob = blob.trim().to_owned();
        let text = self.git.text(&["cat-file", "blob", &blob]).await.ok()?;
        let lines = self.highlighter.lines(&blob, &text, language, path);

        Some((text, lines))
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

        file.line_count = new_side.as_ref().map(|(count, _)| *count);
        let new_side = new_side.map(|(_, lines)| lines);
        let old_side = old_side.map(|(_, lines)| lines);

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
    ) -> Option<(usize, crate::highlight::Lines)> {
        let spec = format!("{rev}:{path}");
        let blob = self.git.text(&["rev-parse", &spec]).await.ok()?;
        let blob = blob.trim().to_owned();
        let text = self.git.text(&["cat-file", "blob", &blob]).await.ok()?;
        let lines = self.highlighter.lines(&blob, &text, language, for_path);

        Some((text.lines().count(), lines))
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
            change.reviewed = counts.reviewed;
        }
    }

    /// The patch sets of a change, oldest first.
    ///
    /// Gerrit is asked once per change and the answer is kept. A failure
    /// there leaves the local list working, because the local list is the
    /// part the reader owns.
    pub async fn patch_sets(&self, key: &str) -> Result<Vec<PatchSet>> {
        let rev = self
            .commit_of(key)
            .with_context(|| format!("no change {key} in the series"))?;
        let info = commit::info(&self.git, &rev).await?;
        let mut sets = patchset::of_change(&self.git, &info, &self.prevs).await?;

        if let Some(change) = self.ask_gerrit(&info).await {
            sets = patchset::merge_gerrit(sets, &change.patch_sets);
        }

        for set in &mut sets {
            set.available = self.has_commit(&set.commit).await;
        }
        Ok(sets)
    }

    /// What Gerrit knows about a change, asked once.
    async fn ask_gerrit(&self, info: &commit::CommitInfo) -> Option<gerrit::Change> {
        let coords = self.gerrit.as_ref()?;
        let change_id = info.change_id()?;

        let mut answers = self.gerrit_answers.lock().await;
        if let Some(known) = answers.get(change_id) {
            return known.clone();
        }

        let answer = match gerrit::query(coords, change_id).await {
            Ok(answer) => answer,
            Err(error) => {
                // Say it once, and go on. The local review is what matters.
                eprintln!("qreview: {error}");
                None
            }
        };
        answers.insert(change_id.to_owned(), answer.clone());

        answer
    }

    /// What Gerrit calls a change, when the server knows it.
    pub async fn gerrit_change(&self, key: &str) -> Option<gerrit::Change> {
        let rev = self.commit_of(key)?;
        let info = commit::info(&self.git, &rev).await.ok()?;

        self.ask_gerrit(&info).await
    }

    async fn has_commit(&self, rev: &str) -> bool {
        self.git
            .text(&["cat-file", "-e", &format!("{rev}^{{commit}}")])
            .await
            .is_ok()
    }

    /// Fetch one Gerrit patch set into this clone.
    ///
    /// The only write this tool makes to the repository, and only when the
    /// reader asks for it.
    pub async fn fetch_patch_set(&self, key: &str, number: usize) -> Result<PatchSet> {
        let sets = self.patch_sets(key).await?;
        let set = sets
            .into_iter()
            .find(|set| set.number == number)
            .with_context(|| format!("change {key} has no patch set {number}"))?;

        if set.available {
            return Ok(set);
        }
        let git_ref = set
            .gerrit_ref
            .clone()
            .with_context(|| format!("patch set {number} is not on Gerrit"))?;

        self.git.text(&["fetch", "origin", &git_ref]).await?;

        let info = commit::info(&self.git, &set.commit).await?;
        Ok(PatchSet {
            parent: info.parents.first().cloned(),
            created_at: info.date,
            subject: info.subject,
            available: true,
            ..set
        })
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
            // One patch set until M5 adds the rest.
            patch_set: 1,
        }
        .add(new)
        .await
    }

    /// Mark a change read, or unread.
    pub fn mark_reviewed(&mut self, key: &str, reviewed: bool) -> Result<()> {
        let subject = self
            .series
            .changes
            .iter()
            .find(|c| c.key == key)
            .map(|c| c.subject.clone())
            .unwrap_or_default();

        comments::mark(&self.store, key, &subject, reviewed)?;
        if let Some(change) = self.series.changes.iter_mut().find(|c| c.key == key) {
            change.reviewed = reviewed;
        }
        Ok(())
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

/// Where the Gerrit server is, when the remote names one.
async fn coords_of(git: &Git, rev: &str, branch: Option<&str>) -> Option<Coordinates> {
    gerrit::coords::of_repo(git, rev, branch).await
}
