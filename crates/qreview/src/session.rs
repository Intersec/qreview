//! What one run of qreview knows.
//!
//! The command line builds it, the server shares it, and the interface reads
//! it through the API. Nothing here touches the working tree.

use anyhow::{Context, Result};

use std::sync::Arc;

use crate::comments::{self, EditComment, NewComment, Target};
use crate::commitmsg;
use crate::diff;
use crate::gerrit::{self, Coordinates};
use crate::git::commit;
use crate::git::exec::Git;
use crate::git::merge::{self, Base};
use crate::highlight::Highlighter;
use crate::lang::Languages;
use crate::model::{BoundaryKind, ChangeSummary, FileDiff, FileEntry, RepoInfo, RowKind, Series};
use crate::patchset::{self, PatchSet};
use crate::repo;
use crate::series::{self, Options, Plan};
use crate::store::Store;
use crate::store::model::{ChangeFile, Comment};
use crate::worktree;

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
    /// Show the tracked changes that are not committed as a change.
    pub worktree: bool,
    /// How the run asked for the series, kept so a refresh can ask again.
    opts: Options,
    /// The size of every batch the reader loaded past the first one. A
    /// refresh replays them, so a series read down to a merge comes back as
    /// deep as it was.
    extends: Vec<usize>,
    /// The earlier versions of a change that carries no `Change-Id`, found
    /// in the reflog. See `reflog.rs`.
    linked: std::collections::HashMap<String, Vec<String>>,
    /// One query per change, kept for the life of the run.
    gerrit_answers: tokio::sync::Mutex<std::collections::HashMap<String, Option<gerrit::Change>>>,
    /// The file list of a pair of trees, kept for the life of the run.
    ///
    /// Rename and copy detection is the expensive half of a diff, and the
    /// answer never moves: a commit is immutable, and the synthetic commit
    /// of the working tree gets a new hash whenever the tree changes.
    file_lists: std::sync::Mutex<std::collections::HashMap<String, Arc<Vec<FileEntry>>>>,
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
            worktree: opts.worktree,
            opts: opts.clone(),
            extends: Vec::new(),
            linked: std::collections::HashMap::new(),
            gerrit_answers: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            file_lists: std::sync::Mutex::new(std::collections::HashMap::new()),
        };
        session.make_keys_unique();
        session.link_versions().await;
        session.count_comments();
        session.count_patch_sets().await;
        session.refresh_worktree().await;
        session.report_lost_prevs().await;

        Ok(session)
    }

    /// Find the earlier versions of the changes that carry no `Change-Id`.
    ///
    /// A `Change-Id` is what keeps a review under one key across an amend.
    /// Without one the key follows the sha, so the round before is filed
    /// under a name nothing claims. The reflog still has that commit, and
    /// `reflog.rs` says which of its commits belong to which change.
    ///
    /// Nothing is moved: the store keeps every file where it is. The linked
    /// versions become patch sets, and their remarks are read as remarks of
    /// the change they belong to, which is what they always were.
    async fn link_versions(&mut self) {
        self.linked =
            crate::reflog::earlier_versions(&self.git, &self.series.changes, &self.series.head)
                .await;
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
        self.extends.push(count);
        self.make_keys_unique();
        self.link_versions().await;
        self.count_comments();
        self.count_patch_sets().await;

        Ok(added)
    }

    /// Read the repository again, the way this run first read it.
    ///
    /// A commit amended, a commit added, a rebase, a branch checked out: the
    /// series on the screen is the one git held when the page loaded, and
    /// nothing in it says so. This is the reader asking what git holds now.
    ///
    /// The walk starts over from the head, and the batches the reader loaded
    /// are replayed, so a series read down past a merge comes back as deep
    /// as it was. A change the repository no longer holds leaves the series,
    /// and its remarks stay in the store: nothing but the reader deletes a
    /// remark, and a rebase away and back finds them again.
    pub async fn refresh(&mut self) -> Result<()> {
        let (plan, batch) = series::first_batch(&self.git, &self.opts).await?;

        let oldest = batch
            .changes
            .last()
            .map(|c| c.commit.clone())
            .unwrap_or_else(|| plan.head.clone());

        self.series = Series {
            repo: self.repo.clone(),
            head: plan.head.clone(),
            oldest,
            changes: batch.changes,
            boundary: batch.boundary,
        };
        self.plan = plan;

        // The head can be another commit now, and the branch a change is
        // pushed to is read from the head.
        self.gerrit = match self.opts.gerrit {
            true => {
                coords_of(
                    &self.git,
                    &self.series.head,
                    self.opts.integration_branch.as_deref(),
                )
                .await
            }
            false => None,
        };
        // A patch set pushed since the page loaded is part of what git holds
        // now, so the answer kept for the run is thrown away.
        self.gerrit_answers.lock().await.clear();

        for count in std::mem::take(&mut self.extends) {
            // The boundary can be gone, and then there is nothing to walk.
            // That is an answer, not a failure.
            if let Err(error) = self.extend(count).await {
                eprintln!("qreview: the older commits could not be read again: {error}");
                break;
            }
        }

        self.make_keys_unique();
        self.link_versions().await;
        self.count_comments();
        self.count_patch_sets().await;
        self.refresh_worktree().await;

        Ok(())
    }

    /// Put the work that is not committed at the top of the series, take it
    /// away, or leave it alone.
    ///
    /// The interface asks for the session again at every reload, and the
    /// working tree has moved by then. This is where it catches up.
    pub async fn refresh_worktree(&mut self) {
        let had = self.series.changes.first().is_some_and(|c| c.worktree);
        let found = match self.worktree {
            true => self.uncommitted().await,
            false => None,
        };

        match (had, found) {
            (true, Some(change)) => self.series.changes[0] = change,
            (true, None) => {
                self.series.changes.remove(0);
                return;
            }
            (false, Some(change)) => self.series.changes.insert(0, change),
            (false, None) => return,
        }
        self.count_comments();
    }

    /// The change the working tree stands for, when it stands for one.
    ///
    /// Only when the series is the checkout. A series read from another
    /// revision has nothing to do with what sits in the tree, and putting the
    /// tree on top of it would be a diff of two unrelated things.
    async fn uncommitted(&self) -> Option<ChangeSummary> {
        let head = commit::resolve(&self.git, "HEAD").await.ok()?;
        if head != self.series.head {
            return None;
        }
        let hash = worktree::commit_of(&self.git).await?;
        let info = commit::info(&self.git, &hash).await.ok()?;

        Some(worktree::summary(&hash, &info.author))
    }

    /// Say which commits named with `--prev` belong to no change.
    ///
    /// A `--prev` is placed by its `Change-Id`. A series rewritten commit by
    /// commit, rather than amended, carries new ones, and the version that
    /// was reviewed then belongs to nothing. It was dropped without a word,
    /// and the reader was left looking at a patch set list of one.
    async fn report_lost_prevs(&self) {
        let mut named = Vec::new();
        for hash in &self.prevs {
            if let Ok(info) = commit::info(&self.git, hash).await {
                named.push(info);
            }
        }

        for lost in patchset::unclaimed(&named, &self.series.changes) {
            let what = match lost.change_id() {
                Some(id) => format!("its Change-Id {id} names none of them"),
                None => "it carries no Change-Id, so nothing places it".to_owned(),
            };
            eprintln!(
                "qreview: --prev {} belongs to no change of the series: {what}.",
                &lost.hash[..lost.hash.len().min(12)]
            );
        }
    }

    /// The versions before this one that these remarks were written on.
    ///
    /// Newest first, each with the subject it carried, so the pane can head
    /// a group of previous remarks with the version it belongs to. A commit
    /// git no longer has keeps its sha and loses its subject.
    async fn versions_of(&self, comments: &[Comment], current: &str) -> Vec<crate::model::Version> {
        let mut seen: Vec<&str> = Vec::new();
        for comment in comments {
            let named = comment.commit.as_str();
            if !named.is_empty() && named != current && !seen.contains(&named) {
                seen.push(named);
            }
        }

        let mut found = Vec::with_capacity(seen.len());
        for commit in seen {
            let info = commit::info(&self.git, commit).await.ok();
            found.push((
                info.as_ref().map(|i| i.date.clone()).unwrap_or_default(),
                crate::model::Version {
                    commit: commit.to_owned(),
                    subject: info.map(|i| i.subject).unwrap_or_default(),
                },
            ));
        }
        // Newest first: the round just before this one is the one a reader
        // is answering, and the older ones sink under it.
        found.sort_by(|a, b| b.0.cmp(&a.0));

        found.into_iter().map(|(_, version)| version).collect()
    }

    /// The versions of a change that carry a remark, and are not the one
    /// under review.
    ///
    /// An amend gives the change a new sha. The remarks stay, keyed by the
    /// `Change-Id`, and each names the commit it was written against. That
    /// name is the version the reader reviewed last time, found without
    /// being told. A commit git no longer has is dropped later, by
    /// `patchset::of_change`.
    pub fn reviewed_versions(&self, key: &str, current: &str) -> Vec<String> {
        let Ok(file) = self.store.load(key, "") else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();

        for comment in &file.comments {
            if comment.commit.is_empty() || comment.commit == current {
                continue;
            }
            if !out.contains(&comment.commit) {
                out.push(comment.commit.clone());
            }
        }

        // And the ones the reflog links to a change with no `Change-Id`.
        for commit in self.linked.get(key).into_iter().flatten() {
            if commit != current && !out.contains(commit) {
                out.push(commit.clone());
            }
        }
        out
    }

    /// The remarks already posted on Gerrit for a change.
    ///
    /// Read only: qreview writes nothing to the server. Gerrit is optional,
    /// so a change the server does not know, a query that failed, or a
    /// server with no such option all answer with nothing at all.
    pub async fn posted_comments(&self, key: &str) -> Vec<gerrit::posted::Posted> {
        let Some(rev) = self.commit_of(key) else {
            return Vec::new();
        };
        let Ok(info) = commit::info(&self.git, &rev).await else {
            return Vec::new();
        };
        let Some(change) = self.ask_gerrit(&info).await else {
            return Vec::new();
        };

        gerrit::posted::of_change(&self.git, &change).await
    }

    /// True when the revision is the work that is not committed.
    pub fn is_worktree(&self, rev: &str) -> bool {
        self.series
            .changes
            .iter()
            .any(|change| change.worktree && change.commit == rev)
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
    /// The files a review shows, the commit message first.
    ///
    /// `qreview list` prints the work a change does, and the message is not
    /// part of that. The interface reviews it like a file, so only the
    /// interface asks for this list.
    pub async fn review_files(
        &self,
        rev: &str,
        against: &Against,
        how: &diff::How,
    ) -> Result<Vec<FileEntry>> {
        let mut entries = self.files(rev, against, how).await?;

        // The work that is not committed carries no message to review. The
        // one on the synthetic commit is a label this tool wrote.
        if !self.is_worktree(rev)
            && let Some(new) = commitmsg::text(&self.git, rev).await
        {
            let old = self.message_base(against).await;
            entries.insert(0, commitmsg::entry(&old, &new));
        }
        Ok(entries)
    }

    /// Every comment of the session, change by change, in the order a review
    /// reads them: the oldest commit first, and inside it the order of the
    /// export.
    pub async fn all_comments(&self) -> Vec<crate::model::ChangeComments> {
        let mut out = Vec::new();

        for change in self.series.changes.iter().rev() {
            let Ok(file) = self.comments(&change.key, &change.subject) else {
                continue;
            };
            if file.comments.is_empty() {
                continue;
            }
            let mut comments = file.comments;
            comments::in_reading_order(&mut comments);
            let versions = self.versions_of(&comments, &change.commit).await;

            out.push(crate::model::ChangeComments {
                key: change.key.clone(),
                subject: change.subject.clone(),
                commit: change.commit.clone(),
                comments,
                versions,
            });
        }
        out
    }

    /// The message the reviewed one is read against.
    ///
    /// Only another patch set carries one. The parent of a change carries
    /// another message, and a diff of the two says nothing about the work.
    async fn message_base(&self, against: &Against) -> String {
        match against {
            Against::Tree(other) => commitmsg::text(&self.git, other).await.unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// The file list of a pair of trees, from the cache or from git.
    ///
    /// Every read of a file asks for it, only to learn the old path of a
    /// rename. Asking git again would run the rename and copy detection
    /// again, which on a large repository is most of what a read costs.
    async fn entries(&self, base: &str, rev: &str, how: &diff::How) -> Result<Vec<FileEntry>> {
        // Of the three options, only `-w` changes which files differ.
        let key = format!("{base} {rev} {}", how.ignore_ws);

        if let Some(hit) = self.file_lists.lock().unwrap().get(&key) {
            crate::trace::note(|| format!("file list of {rev}, from the cache"));
            return Ok(hit.as_ref().clone());
        }

        let entries = diff::files(&self.git, base, rev, how).await?;
        self.file_lists
            .lock()
            .unwrap()
            .insert(key, Arc::new(entries.clone()));

        Ok(entries)
    }

    pub async fn files(
        &self,
        rev: &str,
        against: &Against,
        how: &diff::How,
    ) -> Result<Vec<FileEntry>> {
        let base = self.base_of(rev, against).await?;
        let mut entries = self.entries(&base, rev, how).await?;

        // Two versions of one change, read against each other. Between them
        // sits everything the rebase brought, and none of it is the work.
        // Only the files the change itself touches are worth a row.
        if let Against::Tree(other) = against
            && let Some(touched) = self.touched_by(&[rev, other], how).await
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
        how: &diff::How,
    ) -> Option<std::collections::HashSet<String>> {
        let mut touched = std::collections::HashSet::new();

        for rev in revs {
            let info = commit::info(&self.git, rev).await.ok()?;
            let parent = info
                .parents
                .first()
                .cloned()
                .unwrap_or_else(|| diff::EMPTY_TREE.to_owned());

            for entry in self.entries(&parent, rev, how).await.ok()? {
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
        how: &diff::How,
    ) -> Result<Option<FileDiff>> {
        if commitmsg::is(path) {
            return Ok(self.message_diff(rev, against, how).await);
        }

        let base = self.base_of(rev, against).await?;
        let old = self
            .entries(&base, rev, how)
            .await?
            .into_iter()
            .find(|e| e.path == path)
            .and_then(|e| e.old_path);

        let mut found = diff::file(&self.git, &base, rev, path, old.as_deref(), how).await?;

        if let Some(file) = found.as_mut() {
            let language = self.langs.of(path).map(str::to_owned);
            file.file.language = language.clone().unwrap_or_default();

            if !file.file.binary && how.syntax {
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

    /// The diff of the commit message.
    async fn message_diff(
        &self,
        rev: &str,
        against: &Against,
        how: &diff::How,
    ) -> Option<FileDiff> {
        let new = commitmsg::text(&self.git, rev).await?;
        let old = self.message_base(against).await;
        let mut file = commitmsg::diff(&old, &new, how.context);

        for hunk in &mut file.hunks {
            for row in &mut hunk.rows {
                crate::offsets::to_utf16(row);
            }
        }
        Some(file)
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
        let painted = match commitmsg::is(path) {
            true => commitmsg::text(&self.git, rev)
                .await
                .map(|text| (text, crate::highlight::Lines::default())),
            false => {
                self.read_blob(rev, path, language.as_deref(), path, to)
                    .await
            }
        };
        let Some((text, spans)) = painted else {
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
        let (last_old, last_new) = last_lines(file);

        // Both sides at once. Each one runs its git reads and then its own
        // highlight thread, so a file costs one pass of waiting, not two.
        // Neither goes past the last line the hunks reach: the rest of a
        // large file costs seconds and no row shows it.
        let (new_side, old_side) = tokio::join!(
            self.blob(rev, &path, language, &path, last_new),
            self.blob(base, old_path.unwrap_or(&path), language, &path, last_old),
        );

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

    /// Read one side of a file and highlight it down to line `upto`.
    async fn blob(
        &self,
        rev: &str,
        path: &str,
        language: Option<&str>,
        for_path: &str,
        upto: usize,
    ) -> Option<(usize, crate::highlight::Lines)> {
        let (text, lines) = self.read_blob(rev, path, language, for_path, upto).await?;

        Some((text.lines().count(), lines))
    }

    /// The text of a blob and its syntax spans.
    ///
    /// The highlight is plain computation, and on a file of a few hundred
    /// kilobytes it runs for a second. A runtime thread must stay free to
    /// answer the other requests, so that second is spent on the blocking
    /// pool.
    async fn read_blob(
        &self,
        rev: &str,
        path: &str,
        language: Option<&str>,
        for_path: &str,
        upto: usize,
    ) -> Option<(String, crate::highlight::Lines)> {
        let spec = format!("{rev}:{path}");
        let blob = self.git.text(&["rev-parse", &spec]).await.ok()?;
        let blob = blob.trim().to_owned();
        let text = self.git.text(&["cat-file", "blob", &blob]).await.ok()?;

        let highlighter = self.highlighter.clone();
        let language = language.map(str::to_owned);
        let for_path = for_path.to_owned();

        tokio::task::spawn_blocking(move || {
            let lines = highlighter.lines_upto(&blob, &text, language.as_deref(), &for_path, upto);
            (text, lines)
        })
        .await
        .ok()
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

    /// Make sure no two changes answer to the same key.
    ///
    /// A series can carry the same `Change-Id` twice: a cherry-pick that
    /// kept the trailer, a revert, a rebase that went wrong. Both changes
    /// then answered to one key, so both opened the same files and shared
    /// one review. The later one falls back to its hash, which is what a
    /// commit with no trailer at all uses.
    fn make_keys_unique(&mut self) {
        let mut seen = std::collections::HashSet::new();

        for change in &mut self.series.changes {
            if !seen.insert(change.key.clone()) {
                change.key = format!("sha-{}", change.commit);
                seen.insert(change.key.clone());
            }
        }
    }

    /// Put the comment counts on the series.
    ///
    /// A change file that cannot be read counts as zero here. The series must
    /// still load, and the failure is said where the change opens.
    fn count_comments(&mut self) {
        for change in &mut self.series.changes {
            let counts = comments::counts(&self.store, &change.key, &change.commit);
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

        // The work that is not committed has one version: the one on the
        // disk. It has no Change-Id either, so a `--prev` with none would
        // otherwise be read as an older version of it.
        if self.is_worktree(&rev) {
            return patchset::of_change(&self.git, &info, &[]).await;
        }

        // The versions the reader named, and the ones the store remembers
        // from the remarks written on them. The second is what makes a
        // second round work without `--prev`.
        let mut older = self.prevs.clone();
        older.extend(self.reviewed_versions(key, &rev));

        let mut sets = patchset::of_change(&self.git, &info, &older).await?;

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

    /// The name of the branch being reviewed, for a person to read.
    pub async fn branch(&self) -> String {
        if let Some(coords) = &self.gerrit
            && let Some(branch) = &coords.branch
        {
            return branch.clone();
        }

        let out = self
            .git
            .text(&["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .unwrap_or_default();
        let name = out.trim();

        match name {
            "" | "HEAD" => "detached".to_owned(),
            other => other.to_owned(),
        }
    }

    /// What the repository is called, for a person to read.
    pub fn project(&self) -> String {
        self.repo.name.clone()
    }

    /// What Gerrit calls a change, when the server knows it.
    pub async fn gerrit_change(&self, key: &str) -> Option<gerrit::Change> {
        let rev = self.commit_of(key)?;
        let info = commit::info(&self.git, &rev).await.ok()?;

        self.ask_gerrit(&info).await
    }

    async fn has_commit(&self, rev: &str) -> bool {
        let call = ["cat-file", "-e", &format!("{rev}^{{commit}}")];

        // A commit this clone holds, it keeps. One it does not can arrive
        // with a fetch, and a failed call is never kept, so the answer is
        // asked again until it is yes.
        match commit::is_object_name(rev) {
            true => self.git.text_of_object(&call).await.is_ok(),
            false => self.git.text(&call).await.is_ok(),
        }
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
        let keys: Vec<_> = self
            .series
            .changes
            .iter()
            .map(|c| (c.key.clone(), c.commit.clone()))
            .collect();

        // Which changes are worth asking the server about. Asking costs a
        // round trip, and a change that cannot have a second version is not
        // worth one.
        let asked: Vec<_> = keys
            .iter()
            .filter(|(key, commit)| {
                !self.prevs.is_empty() || !self.reviewed_versions(key, commit).is_empty()
            })
            .map(|(key, _)| key.clone())
            .collect();

        self.prime_gerrit(&asked).await;

        let mut counts = Vec::with_capacity(keys.len());
        for (key, _) in &keys {
            if !asked.contains(key) {
                counts.push(1);
                continue;
            }
            counts.push(self.patch_sets(key).await.map(|s| s.len()).unwrap_or(1));
        }
        for (change, count) in self.series.changes.iter_mut().zip(counts) {
            change.patch_set_count = count;
        }
    }

    /// Ask Gerrit about a set of changes at once, and keep every answer.
    ///
    /// The round trip is the cost, not the query, so the whole series is
    /// asked for together. Every change asked about is written into the
    /// answers, the ones the server never heard of as `None`, so nothing
    /// below asks a second time.
    async fn prime_gerrit(&self, keys: &[String]) {
        let Some(coords) = self.gerrit.as_ref() else {
            return;
        };

        let mut wanted = Vec::new();
        for key in keys {
            let Some(rev) = self.commit_of(key) else {
                continue;
            };
            let Ok(info) = commit::info(&self.git, &rev).await else {
                continue;
            };
            if let Some(change_id) = info.change_id() {
                wanted.push(change_id.to_owned());
            }
        }

        let mut answers = self.gerrit_answers.lock().await;
        wanted.retain(|id| !answers.contains_key(id));
        if wanted.is_empty() {
            return;
        }

        let ids: Vec<&str> = wanted.iter().map(String::as_str).collect();
        let found = match gerrit::query_many(coords, &ids).await {
            Ok(found) => found,
            // The server said no. It may still answer about one change at a
            // time, so nothing is written down and the reader who opens a
            // change asks again for that one.
            Err(gerrit::Failed::Refused(error)) => {
                eprintln!("qreview: {error}");
                return;
            }
            // Nothing answered. Asking once per change would only make the
            // reader wait once per change, so the answer is no for all.
            Err(gerrit::Failed::Unreachable(error)) => {
                eprintln!("qreview: {error}");
                for id in wanted {
                    answers.insert(id, None);
                }
                return;
            }
        };

        for id in &wanted {
            let answer = found.iter().find(|change| change.id == *id).cloned();
            answers.insert(id.clone(), answer);
        }
    }

    /// The review of a change: its own remarks, and the ones written on the
    /// versions the reflog links to it.
    ///
    /// A change with no `Change-Id` is a new key at every amend, so the
    /// round before it sits under the key of a commit nothing points at. The
    /// remarks are read here rather than moved: the store keeps every file
    /// where it is, and nothing is rewritten on a guess.
    ///
    /// Read only. A write goes to the store under the key of the change it
    /// is written on, never through this.
    pub fn comments(&self, key: &str, subject: &str) -> Result<ChangeFile> {
        let mut file = comments::read(&self.store, key, subject)?;

        for commit in self.linked.get(key).into_iter().flatten() {
            let Ok(older) = comments::read(&self.store, &format!("sha-{commit}"), "") else {
                continue;
            };
            file.comments.extend(older.comments);
        }
        Ok(file)
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

/// The deepest line each side of a diff shows, old first.
///
/// A row of the old side is a removal, and every other row belongs to the
/// new one. Nothing below these two lines is painted.
fn last_lines(file: &FileDiff) -> (usize, usize) {
    let mut old = 0;
    let mut new = 0;

    for hunk in &file.hunks {
        for row in &hunk.rows {
            match row.kind {
                RowKind::Remove => old = old.max(row.old_line.unwrap_or(0)),
                _ => new = new.max(row.new_line.unwrap_or(0)),
            }
        }
    }
    (old, new)
}

/// Where the Gerrit server is, when the remote names one.
async fn coords_of(git: &Git, rev: &str, branch: Option<&str>) -> Option<Coordinates> {
    gerrit::coords::of_repo(git, rev, branch).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Languages;
    use crate::series::Options;
    use crate::testutil::{build_repo, commit};

    /// The file list is kept for the run, and every reader gets its own copy.
    ///
    /// `files` fills in the language of each entry and, between two patch
    /// sets, drops the rows the rebase brought. A cache that handed out the
    /// stored list itself would keep those edits and answer the next reader
    /// with them.
    #[tokio::test]
    async fn the_kept_file_list_is_the_same_on_the_second_read() {
        let repo = build_repo(&[
            commit("before").file("src/a.c", "int a;\n"),
            commit("after")
                .file("src/a.c", "int b;\n")
                .file("src/new.py", "x = 1\n"),
        ])
        .await;

        let mut langs = Languages::new();
        langs.extend(&std::collections::HashMap::from([(
            "c".to_owned(),
            "c".to_owned(),
        )]));
        let session = Session::open(repo.path(), &Options::new(), langs)
            .await
            .unwrap();

        let head = session.series.head.clone();
        let how = diff::How::default();
        let first = session.files(&head, &Against::Parent, &how).await.unwrap();
        let second = session.files(&head, &Against::Parent, &how).await.unwrap();

        assert_eq!(first.len(), 2, "{first:?}");
        assert_eq!(first, second);
        assert!(
            first.iter().any(|entry| entry.language == "c"),
            "the reader gets the language: {first:?}"
        );

        let base = session.base_of(&head, &Against::Parent).await.unwrap();
        let kept = session.entries(&base, &head, &how).await.unwrap();
        assert!(
            kept.iter().all(|entry| entry.language.is_empty()),
            "the kept list must carry nothing a reader added: {kept:?}"
        );
    }

    /// A diff paints the file only as far as its hunks reach. The lines the
    /// reader opens afterwards must still carry their colors, and the parse
    /// must carry the state of the lines above them.
    #[tokio::test]
    async fn the_lines_opened_after_the_diff_are_painted_too() {
        let mut before = String::from("int a = 0;\n");
        let mut after = String::from("int a = 1;\n");
        for text in [&mut before, &mut after] {
            for number in 2..30 {
                text.push_str(&format!("int x{number};\n"));
            }
            text.push_str("/* a comment\n   that runs on\n   and on */\n");
        }

        let repo = build_repo(&[
            commit("before").file("a.c", &before),
            commit("after").file("a.c", &after),
        ])
        .await;
        let session = Session::open(repo.path(), &Options::new(), Languages::new())
            .await
            .unwrap();
        let head = session.series.head.clone();

        // The hunk stops around line 11, well above the comment.
        session
            .diff(&head, "a.c", &Against::Parent, &diff::How::default())
            .await
            .unwrap()
            .expect("the change touches the file");

        let rows = session.lines(&head, "a.c", 30, 32).await.unwrap();
        let classes: Vec<&str> = rows[1].tokens.iter().map(|s| s.cls.as_str()).collect();

        assert_eq!(rows[1].text, "   that runs on");
        assert!(
            classes.iter().any(|cls| cls.starts_with("tok-comment")),
            "the comment opened on the line above: {classes:?}"
        );
    }
}
