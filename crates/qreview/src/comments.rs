//! Writing and reading review comments.
//!
//! A comment is keyed by the change, not by the commit, so an amend keeps it.
//! Where it sits is recorded with enough of the line around it to find that
//! place again in another patch set.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::git::exec::Git;
use crate::store::Store;
use crate::store::model::{Anchor, Author, ChangeFile, Comment, Gerrit, Outbox, Scope, Side};

/// How many lines above and below the anchor are kept.
pub const CONTEXT: usize = 3;

/// What the interface sends to write a comment.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewComment {
    pub scope: Scope,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub side: Option<Side>,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
    /// The character range inside the two lines, in UTF-16 units. Absent
    /// when the comment covers whole lines.
    #[serde(default)]
    pub start_char: Option<usize>,
    #[serde(default)]
    pub end_char: Option<usize>,
    pub body: String,
    /// Absent from the interface, which writes as the reader.
    #[serde(default)]
    pub author: Author,
    /// The comment this one answers. Its thread is the one it joins.
    #[serde(default)]
    pub parent: Option<String>,
    /// On a reply, check or clear the Done box of the thread with it.
    #[serde(default)]
    pub done: Option<bool>,
    /// On a reply of the agent, the thread waits for the reader.
    #[serde(default)]
    pub blocked: bool,
    /// Ask the agent to answer the thread once the comment is saved.
    #[serde(default)]
    pub ask: bool,
    /// Stage the remark for Gerrit. See section 5.8 of the design.
    #[serde(default)]
    pub gerrit: Option<Stage>,
}

/// What staging a remark for Gerrit asks for.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    /// On a reply to a thread of Gerrit, resolve that thread.
    #[serde(default)]
    pub resolve: bool,
}

/// What the interface sends to change one.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditComment {
    #[serde(default)]
    pub body: Option<String>,
    /// Check or clear the Done box. A remark that opens a thread only.
    #[serde(default)]
    pub done: Option<bool>,
    /// Who makes the change, for the event that tells the other party.
    #[serde(default)]
    pub author: Author,
    /// Stage the remark for Gerrit, or, with `null`, keep it in qreview.
    /// Absent leaves it as it is.
    #[serde(default, deserialize_with = "present")]
    pub gerrit: Option<Option<Stage>>,
}

/// A field that is present, even as `null`, is `Some`. Serde reads a
/// `null` as absent otherwise, and `null` is how a remark leaves Gerrit.
fn present<'de, D, T>(from: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(from).map(Some)
}

/// What the change owes the series pane.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Counts {
    pub total: usize,
    pub reviewed: bool,
}

/// The change a comment is being written on.
pub struct Target<'a> {
    pub store: &'a Store,
    pub git: &'a Git,
    /// The commit under review.
    pub rev: &'a str,
    /// What that commit is diffed against.
    pub base: &'a str,
    pub key: &'a str,
    pub subject: &'a str,
    pub patch_set: usize,
    /// The patch set of Gerrit that `rev` is, when Gerrit holds it.
    pub on_gerrit: Option<usize>,
    /// The thread of Gerrit that a reply with a `gerrit:` parent answers.
    pub gerrit_thread: Option<GerritThread>,
}

/// The prefix of a `parent` that names a thread of Gerrit, which the store
/// does not hold. See section 6.8 of the design.
pub const GERRIT_PARENT: &str = "gerrit:";

/// A thread of Gerrit, as a reply in qreview needs it.
#[derive(Clone, Debug)]
pub struct GerritThread {
    /// The id of the comment of Gerrit that opens the thread.
    pub id: String,
    /// The patch set of Gerrit that holds the thread.
    pub patch_set: usize,
    /// The revision of that patch set.
    pub commit: String,
    pub scope: Scope,
    /// The place of the thread, so that the reply goes to the same one.
    pub anchor: Option<Anchor>,
}

/// The id of the thread of Gerrit that a reply answers.
pub fn gerrit_parent(comment: &Comment) -> Option<&str> {
    comment.parent.as_deref()?.strip_prefix(GERRIT_PARENT)
}

/// Read the review of a change.
pub fn read(store: &Store, key: &str, subject: &str) -> Result<ChangeFile> {
    store.load(key, subject)
}

/// The counts the series pane shows.
///
/// Only the open threads of the version under review are counted. A round
/// before this one, or a thread marked done, holds remarks that are dealt
/// with, and a count that holds them says there is work where there is none.
pub fn counts(store: &Store, key: &str, commit: &str) -> Counts {
    match store.load(key, "") {
        Ok(file) => Counts {
            total: file
                .comments
                .iter()
                .filter(|comment| is_open(comment, commit))
                .count(),
            reviewed: file.reviewed,
        },
        // A file that cannot be read must not stop the series from loading.
        // The change opens, and the read fails there, where it can be said.
        Err(_) => Counts::default(),
    }
}

/// Mark a change read, or unread.
pub fn mark(store: &Store, key: &str, subject: &str, reviewed: bool) -> Result<bool> {
    let _lock = store.lock()?;
    let mut file = store.load(key, subject)?;
    file.reviewed = reviewed;
    store.save(&file)?;

    Ok(reviewed)
}

impl Target<'_> {
    /// Write a comment, with the place it sits filled in.
    pub async fn add(&self, new: NewComment) -> Result<Comment> {
        if new.body.trim().is_empty() {
            bail!("a comment with no text says nothing");
        }

        if let Some(thread) = &self.gerrit_thread {
            return self.reply_on_gerrit(thread, new);
        }
        if new.parent.is_some() {
            let _lock = self.store.lock()?;
            let mut file = self.store.load(self.key, self.subject)?;
            let reply = self.reply(&mut file, new)?;
            self.store.save(&file)?;
            return Ok(reply);
        }
        if new.blocked {
            bail!("only a reply waits for the reader");
        }
        let gerrit = match new.gerrit {
            Some(stage) => Some(staged(self.on_gerrit, stage)?),
            None => None,
        };

        let now = now();
        let anchor = match new.scope {
            Scope::Change => None,
            _ => Some(anchor_of(self.git, self.rev, self.base, &new).await?),
        };

        let comment = Comment {
            id: new_id(),
            patch_set: self.patch_set,
            commit: self.rev.to_owned(),
            created_at: now.clone(),
            updated_at: now,
            scope: new.scope,
            body: new.body.trim_end().to_owned(),
            anchor,
            author: new.author,
            parent: None,
            done: false,
            blocked: false,
            moves: Vec::new(),
            gerrit,
        };

        // Read after the anchor: git may take a while, and the lock must
        // not wait on it.
        let _lock = self.store.lock()?;
        let mut file = self.store.load(self.key, self.subject)?;
        file.comments.push(comment.clone());
        self.store.save(&file)?;

        Ok(comment)
    }

    /// Add a reply to a thread of Gerrit. A plain one stays in qreview, the
    /// talk of the reader with the agent. A staged one goes out in the
    /// thread, on its patch set, at the next Send.
    fn reply_on_gerrit(&self, thread: &GerritThread, new: NewComment) -> Result<Comment> {
        if new.blocked && new.author != Author::Agent {
            bail!("only the agent waits for the reader");
        }
        let gerrit = new.gerrit.map(|stage| Gerrit {
            state: Outbox::Staged,
            patch_set: thread.patch_set,
            resolve: stage.resolve,
            id: None,
            sent_at: None,
        });

        let now = now();
        let reply = Comment {
            id: new_id(),
            patch_set: thread.patch_set,
            commit: thread.commit.clone(),
            created_at: now.clone(),
            updated_at: now,
            scope: thread.scope,
            body: new.body.trim_end().to_owned(),
            anchor: thread.anchor.clone(),
            author: new.author,
            parent: Some(format!("{GERRIT_PARENT}{}", thread.id)),
            done: false,
            blocked: new.blocked,
            moves: Vec::new(),
            gerrit,
        };

        let _lock = self.store.lock()?;
        let mut file = self.store.load(self.key, self.subject)?;
        file.comments.push(reply.clone());
        self.store.save(&file)?;
        Ok(reply)
    }

    /// Add a reply to its thread, and move the state of the thread with it.
    fn reply(&self, file: &mut ChangeFile, new: NewComment) -> Result<Comment> {
        let parent = new.parent.as_deref().unwrap_or_default();
        let at = thread_of(file, parent)?;
        let remark = &mut file.comments[at];

        // A staged remark stays on its patch set after an amend, and its
        // newest reply is the text it sends.
        if !of_version(remark, self.rev) && !is_staged(remark) {
            bail!("this thread belongs to an earlier version, and its round is over");
        }
        if new.blocked && new.author != Author::Agent {
            bail!("only the agent waits for the reader");
        }
        if new.gerrit.is_some() {
            bail!("a reply stays in qreview: only the remark of a thread goes to Gerrit");
        }

        // A reply of the reader opens the thread again: it says more
        // remains to do. An explicit box wins over that.
        match (new.done, new.author) {
            (Some(done), _) => remark.done = done,
            (None, Author::Reader) => remark.done = false,
            (None, Author::Agent) => {}
        }

        let now = now();
        let reply = Comment {
            id: new_id(),
            patch_set: self.patch_set,
            commit: self.rev.to_owned(),
            created_at: now.clone(),
            updated_at: now,
            scope: remark.scope,
            body: new.body.trim_end().to_owned(),
            anchor: None,
            author: new.author,
            parent: Some(remark.id.clone()),
            done: false,
            blocked: new.blocked,
            moves: Vec::new(),
            gerrit: None,
        };
        file.comments.push(reply.clone());

        Ok(reply)
    }
}

/// The index of the remark that opens the thread a comment belongs to.
///
/// A reply to a reply joins the thread of its parent: a thread is flat.
fn thread_of(file: &ChangeFile, id: &str) -> Result<usize> {
    let found = file
        .comments
        .iter()
        .find(|c| c.id == id)
        .with_context(|| format!("no comment {id}"))?;
    let root = found.parent.as_deref().unwrap_or(id);

    file.comments
        .iter()
        .position(|c| c.id == root && c.is_remark())
        .with_context(|| format!("comment {id} answers {root}, which is gone"))
}

/// The start of the refusal to stage on a version that Gerrit does not hold.
/// The agent reads it to fall back to a plain remark.
pub const NOT_ON_GERRIT: &str = "Gerrit does not hold this version";

/// The mark of a remark staged for Gerrit, on the patch set `on_gerrit`.
fn staged(on_gerrit: Option<usize>, stage: Stage) -> Result<Gerrit> {
    let patch_set = on_gerrit.with_context(|| {
        format!("{NOT_ON_GERRIT}, so a remark on it cannot go there. Push it first")
    })?;

    Ok(Gerrit {
        state: Outbox::Staged,
        patch_set,
        resolve: stage.resolve,
        id: None,
        sent_at: None,
    })
}

/// Change the text of a comment, check or clear the Done box of its
/// thread, or stage it for Gerrit.
///
/// `on_gerrit` is the patch set of Gerrit that `current` is, when Gerrit
/// holds it.
pub fn edit(
    store: &Store,
    key: &str,
    (current, on_gerrit): (&str, Option<usize>),
    id: &str,
    edit: EditComment,
) -> Result<Comment> {
    let _lock = store.lock()?;
    let mut file = store.load(key, "")?;
    let found = file
        .comments
        .iter_mut()
        .find(|c| c.id == id)
        .with_context(|| format!("no comment {id}"))?;

    let sent = found
        .gerrit
        .as_ref()
        .is_some_and(|g| g.state == Outbox::Sent);
    if sent && (edit.body.is_some() || edit.gerrit.is_some()) {
        bail!("this remark is a draft on Gerrit now. Change it there");
    }

    if let Some(stage) = edit.gerrit {
        // A reply to Gerrit is written for the thread: it can be kept here,
        // and a new one written to send it again.
        if gerrit_parent(found).is_some() {
            match (stage, found.gerrit.as_mut()) {
                // The Done box of the thread: a staged reply keeps its text,
                // and stops resolving the thread.
                (Some(stage), Some(staged)) => staged.resolve = stage.resolve,
                (Some(_), None) => bail!("write a new reply to put it on Gerrit again"),
                (None, _) => found.gerrit = None,
            }
            found.updated_at = now();
            let out = found.clone();
            store.save(&file)?;
            return Ok(out);
        }
        if !found.is_remark() {
            bail!("a reply stays in qreview: only the remark of a thread goes to Gerrit");
        }
        if !of_version(found, current) {
            bail!("this thread belongs to an earlier version, and its round is over");
        }
        found.gerrit = match stage {
            Some(stage) => Some(staged(on_gerrit, stage)?),
            None => None,
        };
    }

    if let Some(done) = edit.done {
        if !found.is_remark() {
            bail!("the Done box is on the remark that opens the thread");
        }
        if !of_version(found, current) {
            bail!("this thread belongs to an earlier version, and its round is over");
        }
        found.done = done;
    }

    if let Some(body) = edit.body {
        if body.trim().is_empty() {
            bail!("a comment with no text says nothing. Delete it instead");
        }
        found.body = body.trim_end().to_owned();
    }
    found.updated_at = now();

    let updated = found.clone();
    store.save(&file)?;

    Ok(updated)
}

/// Forget that a remark was sent to Gerrit, when its draft is gone there.
pub fn forget_sent(store: &Store, key: &str, id: &str) -> Result<()> {
    let _lock = store.lock()?;
    let mut file = store.load(key, "")?;
    if let Some(found) = file.comments.iter_mut().find(|c| c.id == id) {
        found.gerrit = None;
        found.updated_at = now();
    }
    store.save(&file)
}

/// Mark remarks sent to Gerrit, each with the id of its draft.
pub fn mark_sent(store: &Store, key: &str, sent: &[(String, Option<String>)]) -> Result<()> {
    if sent.is_empty() {
        return Ok(());
    }
    let _lock = store.lock()?;
    let mut file = store.load(key, "")?;
    let now = now();
    for comment in &mut file.comments {
        let Some((_, id)) = sent.iter().find(|(sent, _)| *sent == comment.id) else {
            continue;
        };
        if let Some(gerrit) = comment.gerrit.as_mut() {
            gerrit.state = Outbox::Sent;
            gerrit.id = id.clone();
            gerrit.sent_at = Some(now.clone());
        }
    }
    store.save(&file)
}

/// The text that a staged comment sends to Gerrit: the newest reply of its
/// thread, or its own body. The reader often asks the agent to write the
/// remark, and the answer is what the author of the change must read.
pub fn last_word<'a>(comment: &'a Comment, all: &'a [Comment]) -> &'a str {
    if !comment.is_remark() {
        return &comment.body;
    }
    // A reply joins the store after the comments it answers.
    all.iter()
        .rev()
        .find(|c| c.parent.as_deref() == Some(comment.id.as_str()))
        .map_or(&comment.body, |reply| &reply.body)
}

/// True when the remark, or the reply to a thread of Gerrit, waits for the
/// reader to send it to Gerrit.
pub fn is_staged(comment: &Comment) -> bool {
    (comment.is_remark() || gerrit_parent(comment).is_some())
        && comment
            .gerrit
            .as_ref()
            .is_some_and(|g| g.state == Outbox::Staged)
}

/// Delete a comment. A remark goes with its replies, which answer nothing
/// once it is gone.
pub fn delete(store: &Store, key: &str, id: &str) -> Result<usize> {
    let _lock = store.lock()?;
    let mut file = store.load(key, "")?;
    let before = file.comments.len();

    if !file.comments.iter().any(|c| c.id == id) {
        bail!("no comment {id}");
    }

    file.comments
        .retain(|c| c.id != id && c.parent.as_deref() != Some(id));
    store.save(&file)?;

    Ok(before - file.comments.len())
}

/// Delete the done threads of the version under review, replies with them.
/// A thread opened again since is not done, and stays.
pub fn clean(store: &Store, key: &str, current: &str) -> Result<usize> {
    let _lock = store.lock()?;
    let mut file = store.load(key, "")?;
    let before = file.comments.len();
    let done: Vec<String> = file
        .comments
        .iter()
        // A remark for Gerrit hides its Done box: it is a message, not work.
        .filter(|c| c.is_remark() && c.done && !c.is_for_gerrit() && of_version(c, current))
        .map(|c| c.id.clone())
        .collect();

    file.comments
        .retain(|c| !done.contains(&c.id) && !c.parent.as_ref().is_some_and(|p| done.contains(p)));
    if file.comments.len() != before {
        store.save(&file)?;
    }
    Ok(before - file.comments.len())
}

/// True when the remark was written on the version being read.
///
/// A remark from a store older than format 3 names no version. It counts as
/// this one: it is the only version it can belong to, and leaving it out of
/// the export would lose a review that nothing would show again.
pub fn of_version(comment: &Comment, commit: &str) -> bool {
    comment.commit.is_empty() || comment.commit == commit
}

/// Where a moved thread went next: the stored thread, and its version.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MovedTo {
    pub id: String,
    pub commit: String,
}

/// The threads that left one of `versions`, as they stood there.
///
/// A migrate moves a thread and keeps its old place in `moves`. Read on the
/// old version, the thread stands there again, at that place, with the
/// replies written on that version: a record of the round, read only. Each
/// copy takes an id of its own, so it is never mistaken for the thread.
pub fn moved_from(comments: &[Comment], versions: &[&str]) -> Vec<(Comment, MovedTo)> {
    let mut out = Vec::new();
    for remark in comments.iter().filter(|c| c.is_remark()) {
        for (n, was) in remark.moves.iter().enumerate() {
            if !versions.contains(&was.commit.as_str()) {
                continue;
            }
            let next = remark
                .moves
                .get(n + 1)
                .map_or(&remark.commit, |m| &m.commit);
            let to = MovedTo {
                id: remark.id.clone(),
                commit: next.clone(),
            };
            let copy_id = format!("{}@{}", remark.id, short(&was.commit));
            let copy = Comment {
                id: copy_id.clone(),
                commit: was.commit.clone(),
                scope: scope_of(remark.scope, was.anchor.as_ref()),
                anchor: was.anchor.clone(),
                done: false,
                moves: Vec::new(),
                ..remark.clone()
            };
            out.push((copy, to.clone()));
            for reply in comments
                .iter()
                .filter(|c| c.parent.as_deref() == Some(&remark.id) && c.commit == was.commit)
            {
                let reply = Comment {
                    id: format!("{}@{}", reply.id, short(&was.commit)),
                    parent: Some(copy_id.clone()),
                    ..reply.clone()
                };
                out.push((reply, to.clone()));
            }
        }
    }
    out
}

/// The scope a remark had at an earlier place. A move to the commit
/// message, when its file was deleted, made it a file remark.
fn scope_of(now: Scope, was: Option<&Anchor>) -> Scope {
    match was {
        Some(a) if a.start_line.is_some() && matches!(now, Scope::File | Scope::Change) => {
            if a.end_line.is_some_and(|end| Some(end) != a.start_line) {
                Scope::Range
            } else {
                Scope::Line
            }
        }
        _ => now,
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

/// True when the comment opens a thread of this version that is not done:
/// work the review still asks for.
///
/// A remark for Gerrit is a message to a colleague, and no work.
pub fn is_open(comment: &Comment, commit: &str) -> bool {
    comment.is_remark() && !comment.done && !comment.is_for_gerrit() && of_version(comment, commit)
}

/// True when the thread waits for the reader: its last reply is a blocked
/// one of the agent. The next reply of the reader ends that, with no flag
/// to clear.
pub fn is_blocked(comments: &[Comment], remark: &str) -> bool {
    comments
        .iter()
        .filter(|c| c.parent.as_deref() == Some(remark))
        .max_by(|a, b| a.created_at.cmp(&b.created_at))
        .is_some_and(|last| last.blocked && last.author == Author::Agent)
}

/// Put the comments of one change in the order a review reads them.
///
/// The files in alphabetic order, the commit message before them, and a
/// remark about the whole change before that, because it belongs to no
/// file. Inside a file, top to bottom, and two remarks on one line in the
/// order they were written.
///
/// The export reads this order, and so does the pane that lists what the
/// session holds. One order, said once.
pub fn in_reading_order(comments: &mut [Comment]) {
    comments.sort_by(|a, b| {
        place_key(a)
            .cmp(&place_key(b))
            .then_with(|| line_of(a).cmp(&line_of(b)))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
}

/// What orders two comments by the place they speak of. The rank comes
/// first, so the order does not rest on where a slash sits in the alphabet.
fn place_key(comment: &Comment) -> (u8, &str) {
    match comment.anchor.as_ref() {
        None => (0, ""),
        Some(anchor) if crate::commitmsg::is(&anchor.file) => (1, ""),
        Some(anchor) => (2, anchor.file.as_str()),
    }
}

/// The line a comment sits on. A remark about a whole file has none, and
/// comes before the lines of that file.
fn line_of(comment: &Comment) -> Option<usize> {
    comment.anchor.as_ref().and_then(|anchor| anchor.start_line)
}

/// Where the comment sits, with enough of the file around it to find the
/// place again in another patch set.
pub(crate) async fn anchor_of(
    git: &Git,
    rev: &str,
    base: &str,
    new: &NewComment,
) -> Result<Anchor> {
    let file = new
        .file
        .clone()
        .context("a comment on a line or a file must name the file")?;
    let side = new.side.unwrap_or(Side::New);

    let mut anchor = Anchor {
        file: file.clone(),
        side,
        start_line: new.start_line,
        end_line: new.end_line.or(new.start_line),
        start_char: new.start_char,
        end_char: new.end_char,
        blob: None,
        line_hash: None,
        context: Vec::new(),
    };

    if new.scope == Scope::File {
        anchor.start_line = None;
        anchor.end_line = None;
        return Ok(anchor);
    }

    let start = anchor
        .start_line
        .context("a comment on a line needs the line")?;
    let tree = match side {
        Side::New => rev,
        Side::Old => base,
    };

    // The commit message has no blob. The line hash and the context are
    // what carry the comment to the next patch set.
    if crate::commitmsg::is(&file) {
        if let Some(text) = crate::commitmsg::text(git, tree).await {
            let lines: Vec<&str> = text.lines().collect();
            anchor.line_hash = lines.get(start - 1).map(|line| hash_line(line));
            anchor.context = context_of(&lines, start);
        }
        return Ok(anchor);
    }

    // A file that cannot be read still gets a comment. The anchor is weaker,
    // and losing the remark would be worse.
    if let Ok(blob) = git.text(&["rev-parse", &format!("{tree}:{file}")]).await {
        let blob = blob.trim().to_owned();
        if let Ok(text) = git.text(&["cat-file", "blob", &blob]).await {
            let lines: Vec<&str> = text.lines().collect();
            anchor.blob = Some(blob);
            anchor.line_hash = lines.get(start - 1).map(|line| hash_line(line));
            anchor.context = context_of(&lines, start);
        }
    }
    Ok(anchor)
}

/// The lines around the anchor, the anchored one in the middle.
fn context_of(lines: &[&str], start: usize) -> Vec<String> {
    let at = start.saturating_sub(1);
    let from = at.saturating_sub(CONTEXT);
    let to = (at + CONTEXT + 1).min(lines.len());

    lines[from..to].iter().map(|l| (*l).to_owned()).collect()
}

/// The hash of a line, trailing space removed.
///
/// Trailing space is what an editor changes without anybody meaning to, and
/// a comment must not come loose over it.
pub fn hash_line(line: &str) -> String {
    let digest = Sha256::digest(line.trim_end().as_bytes());

    format!("sha256:{}", hex::encode(&digest[..8]))
}

fn new_id() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the system has no randomness");

    format!("c-{}", hex::encode(bytes))
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Repo, build_repo, commit};

    struct Fixture {
        repo: Repo,
        store: Store,
        git: Git,
        rev: String,
        base: String,
    }

    async fn fixture() -> Fixture {
        let repo = build_repo(&[
            commit("first").file("a.c", "int a;\n"),
            commit("second").file("a.c", "int a;\nint b;\n"),
        ])
        .await;
        let store = Store::at(repo.path().join(".qreview-test").as_path());
        let git = Git::discover(repo.path()).await.unwrap();
        let rev = repo.sha("HEAD").await;
        let base = repo.sha("HEAD~1").await;
        Fixture {
            repo,
            store,
            git,
            rev,
            base,
        }
    }

    impl Fixture {
        fn target(&self) -> Target<'_> {
            self.target_on(None)
        }

        /// The same, on a version that Gerrit holds as patch set `on_gerrit`.
        fn target_on(&self, on_gerrit: Option<usize>) -> Target<'_> {
            Target {
                store: &self.store,
                git: &self.git,
                rev: &self.rev,
                base: &self.base,
                key: "I8f3a",
                subject: "second",
                patch_set: 1,
                on_gerrit,
                gerrit_thread: None,
            }
        }

        fn file(&self) -> ChangeFile {
            self.store.load("I8f3a", "second").unwrap()
        }
    }

    fn remark(body: &str) -> NewComment {
        NewComment {
            scope: Scope::Line,
            file: Some("a.c".to_owned()),
            side: Some(Side::New),
            start_line: Some(2),
            end_line: Some(2),
            start_char: None,
            end_char: None,
            body: body.to_owned(),
            author: Author::Reader,
            parent: None,
            done: None,
            blocked: false,
            ask: false,
            gerrit: None,
        }
    }

    fn reply(parent: &str, author: Author, body: &str) -> NewComment {
        NewComment {
            scope: Scope::Change,
            file: None,
            side: None,
            parent: Some(parent.to_owned()),
            author,
            ..remark(body)
        }
    }

    #[tokio::test]
    async fn a_reply_joins_the_thread_and_carries_no_anchor() {
        let f = fixture().await;
        let first = f.target().add(remark("why int?")).await.unwrap();
        let answer = f
            .target()
            .add(reply(&first.id, Author::Agent, "it matches the header"))
            .await
            .unwrap();

        assert_eq!(answer.parent.as_deref(), Some(first.id.as_str()));
        assert_eq!(answer.author, Author::Agent);
        assert_eq!(answer.anchor, None, "a reply stands where its thread does");
        assert_eq!(answer.scope, Scope::Line, "the scope of its thread");
        assert_eq!(answer.commit, f.rev);
    }

    #[tokio::test]
    async fn a_reply_to_a_reply_joins_the_same_thread() {
        let f = fixture().await;
        let first = f.target().add(remark("why int?")).await.unwrap();
        let answer = f
            .target()
            .add(reply(&first.id, Author::Agent, "the header"))
            .await
            .unwrap();
        let again = f
            .target()
            .add(reply(&answer.id, Author::Reader, "which one?"))
            .await
            .unwrap();

        assert_eq!(again.parent.as_deref(), Some(first.id.as_str()));
    }

    #[tokio::test]
    async fn done_closes_the_thread_and_a_reply_of_the_reader_opens_it() {
        let f = fixture().await;
        let first = f.target().add(remark("rename b")).await.unwrap();
        f.target()
            .add(NewComment {
                done: Some(true),
                ..reply(&first.id, Author::Agent, "Done")
            })
            .await
            .unwrap();

        assert!(f.file().comments[0].done);
        assert_eq!(
            counts(&f.store, "I8f3a", &f.rev).total,
            0,
            "done counts nowhere"
        );

        f.target()
            .add(reply(&first.id, Author::Reader, "not quite"))
            .await
            .unwrap();

        assert!(!f.file().comments[0].done, "the reader asks for more");
        assert_eq!(counts(&f.store, "I8f3a", &f.rev).total, 1);
    }

    #[tokio::test]
    async fn a_reply_of_the_agent_leaves_the_done_box_as_it_is() {
        let f = fixture().await;
        let first = f.target().add(remark("rename b")).await.unwrap();
        edit(
            &f.store,
            "I8f3a",
            (&f.rev, None),
            &first.id,
            EditComment {
                done: Some(true),
                ..EditComment::default()
            },
        )
        .unwrap();
        f.target()
            .add(reply(&first.id, Author::Agent, "renamed to count"))
            .await
            .unwrap();

        assert!(f.file().comments[0].done);
    }

    #[tokio::test]
    async fn the_done_box_is_only_on_the_remark() {
        let f = fixture().await;
        let first = f.target().add(remark("rename b")).await.unwrap();
        let answer = f
            .target()
            .add(reply(&first.id, Author::Agent, "how?"))
            .await
            .unwrap();
        let checked = EditComment {
            done: Some(true),
            ..EditComment::default()
        };

        let error = edit(&f.store, "I8f3a", (&f.rev, None), &answer.id, checked).unwrap_err();
        assert!(error.to_string().contains("Done box"), "{error}");
    }

    #[tokio::test]
    async fn a_thread_is_blocked_until_the_reader_answers() {
        let f = fixture().await;
        let first = f.target().add(remark("handle the error")).await.unwrap();
        f.target()
            .add(NewComment {
                blocked: true,
                ..reply(&first.id, Author::Agent, "log it, or return it?")
            })
            .await
            .unwrap();

        assert!(is_blocked(&f.file().comments, &first.id));

        f.target()
            .add(reply(&first.id, Author::Reader, "return it"))
            .await
            .unwrap();

        assert!(!is_blocked(&f.file().comments, &first.id));
    }

    #[tokio::test]
    async fn only_the_agent_waits_for_the_reader() {
        let f = fixture().await;
        let first = f.target().add(remark("handle the error")).await.unwrap();
        let blocked = NewComment {
            blocked: true,
            ..reply(&first.id, Author::Reader, "hm")
        };

        assert!(f.target().add(blocked).await.is_err());
        assert!(
            f.target()
                .add(NewComment {
                    blocked: true,
                    ..remark("a remark")
                })
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_thread_of_an_earlier_version_takes_no_reply_and_no_done() {
        let f = fixture().await;
        let first = f.target().add(remark("rename b")).await.unwrap();
        f.repo
            .git(&["commit", "--amend", "-m", "second, again"])
            .await;
        let amended = f.repo.sha("HEAD").await;
        let later = Target {
            rev: &amended,
            ..f.target()
        };

        let error = later
            .add(reply(&first.id, Author::Agent, "done"))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("round is over"), "{error}");

        let checked = EditComment {
            done: Some(true),
            ..EditComment::default()
        };
        assert!(edit(&f.store, "I8f3a", (&amended, None), &first.id, checked).is_err());
    }

    #[tokio::test]
    async fn deleting_a_remark_deletes_its_thread_and_a_reply_goes_alone() {
        let f = fixture().await;
        let first = f.target().add(remark("one")).await.unwrap();
        let other = f.target().add(remark("two")).await.unwrap();
        f.target()
            .add(reply(&first.id, Author::Agent, "a"))
            .await
            .unwrap();
        let alone = f
            .target()
            .add(reply(&other.id, Author::Agent, "b"))
            .await
            .unwrap();

        assert_eq!(delete(&f.store, "I8f3a", &first.id).unwrap(), 2);
        assert_eq!(delete(&f.store, "I8f3a", &alone.id).unwrap(), 1);
        let left: Vec<_> = f.file().comments.into_iter().map(|c| c.id).collect();
        assert_eq!(left, [other.id]);
    }

    #[tokio::test]
    async fn clean_deletes_the_done_threads_and_keeps_a_reopened_one() {
        let f = fixture().await;
        let gone = f.target().add(remark("one")).await.unwrap();
        let reopened = f.target().add(remark("two")).await.unwrap();
        let open = f.target().add(remark("three")).await.unwrap();
        for id in [&gone.id, &reopened.id] {
            f.target()
                .add(NewComment {
                    done: Some(true),
                    ..reply(id, Author::Agent, "Done")
                })
                .await
                .unwrap();
        }
        f.target()
            .add(reply(&reopened.id, Author::Reader, "not quite"))
            .await
            .unwrap();

        assert_eq!(
            clean(&f.store, "I8f3a", &f.rev).unwrap(),
            2,
            "a remark and its reply"
        );

        let left: Vec<_> = f
            .file()
            .comments
            .into_iter()
            .filter(|c| c.is_remark())
            .map(|c| c.id)
            .collect();
        assert_eq!(left, [reopened.id, open.id]);
    }

    #[tokio::test]
    async fn a_reply_is_not_counted_as_a_remark() {
        let f = fixture().await;
        let first = f.target().add(remark("one")).await.unwrap();
        f.target()
            .add(reply(&first.id, Author::Agent, "a"))
            .await
            .unwrap();

        assert_eq!(counts(&f.store, "I8f3a", &f.rev).total, 1);
    }

    fn staged(body: &str) -> NewComment {
        NewComment {
            gerrit: Some(Stage::default()),
            ..remark(body)
        }
    }

    fn stage(gerrit: Option<Stage>) -> EditComment {
        EditComment {
            gerrit: Some(gerrit),
            ..EditComment::default()
        }
    }

    #[tokio::test]
    async fn clean_keeps_a_done_remark_that_was_staged_for_gerrit() {
        let f = fixture().await;
        let first = f.target_on(Some(2)).add(remark("rename b")).await.unwrap();
        let done = EditComment {
            done: Some(true),
            ..EditComment::default()
        };
        edit(&f.store, "I8f3a", (&f.rev, Some(2)), &first.id, done).unwrap();
        let on = stage(Some(Stage::default()));
        edit(&f.store, "I8f3a", (&f.rev, Some(2)), &first.id, on).unwrap();

        assert_eq!(clean(&f.store, "I8f3a", &f.rev).unwrap(), 0);
        assert_eq!(f.file().comments.len(), 1);
    }

    #[tokio::test]
    async fn a_remark_staged_for_gerrit_names_its_patch_set() {
        let f = fixture().await;
        let written = f.target_on(Some(3)).add(staged("rename b")).await.unwrap();

        let gerrit = written.gerrit.unwrap();
        assert_eq!(gerrit.state, Outbox::Staged);
        assert_eq!(gerrit.patch_set, 3);
        assert_eq!(f.file().comments[0].gerrit, Some(gerrit));
    }

    #[tokio::test]
    async fn a_version_that_gerrit_does_not_hold_takes_no_remark_for_it() {
        let f = fixture().await;
        let error = f.target().add(staged("rename b")).await.unwrap_err();

        assert!(error.to_string().contains("Push it first"), "{error}");
        assert!(f.file().comments.is_empty());
    }

    #[tokio::test]
    async fn a_remark_for_gerrit_is_no_work_for_the_agent() {
        let f = fixture().await;
        let written = f.target_on(Some(3)).add(staged("rename b")).await.unwrap();

        assert!(!is_open(&written, &f.rev));
    }

    #[tokio::test]
    async fn a_staged_thread_sends_its_newest_reply() {
        let f = fixture().await;
        let first = f.target_on(Some(3)).add(remark("/review")).await.unwrap();
        let comments = f.file().comments;
        assert_eq!(last_word(&comments[0], &comments), "/review");

        for (author, body) in [(Author::Agent, "draft"), (Author::Agent, "final")] {
            f.target_on(Some(3))
                .add(reply(&first.id, author, body))
                .await
                .unwrap();
        }
        let comments = f.file().comments;
        assert_eq!(last_word(&comments[0], &comments), "final");
        assert_eq!(last_word(&comments[1], &comments), "draft");
    }

    #[tokio::test]
    async fn a_staged_thread_takes_a_reply_after_an_amend() {
        let f = fixture().await;
        let first = f.target_on(Some(3)).add(staged("/review")).await.unwrap();
        let amended = Target {
            rev: &f.base,
            ..f.target_on(None)
        };

        let answer = amended
            .add(reply(
                &first.id,
                Author::Agent,
                "Name it after what it does.",
            ))
            .await
            .unwrap();
        let comments = f.file().comments;
        assert_eq!(answer.parent.as_deref(), Some(first.id.as_str()));
        assert_eq!(
            last_word(&comments[0], &comments),
            "Name it after what it does."
        );
    }

    #[tokio::test]
    async fn a_reply_does_not_go_to_gerrit() {
        let f = fixture().await;
        let first = f.target_on(Some(3)).add(remark("rename b")).await.unwrap();
        let answer = NewComment {
            gerrit: Some(Stage::default()),
            ..reply(&first.id, Author::Agent, "renamed")
        };

        let error = f.target_on(Some(3)).add(answer).await.unwrap_err();
        assert!(error.to_string().contains("stays in qreview"), "{error}");
    }

    #[tokio::test]
    async fn a_remark_is_staged_and_taken_back() {
        let f = fixture().await;
        let first = f.target().add(remark("rename b")).await.unwrap();

        let on = edit(
            &f.store,
            "I8f3a",
            (&f.rev, Some(2)),
            &first.id,
            stage(Some(Stage::default())),
        );
        assert_eq!(on.unwrap().gerrit.unwrap().patch_set, 2);

        let off = edit(&f.store, "I8f3a", (&f.rev, Some(2)), &first.id, stage(None));
        assert_eq!(off.unwrap().gerrit, None);
        assert_eq!(f.file().comments[0].gerrit, None);
    }

    #[tokio::test]
    async fn a_draft_on_gerrit_is_changed_there_and_not_here() {
        let f = fixture().await;
        let first = f.target_on(Some(3)).add(staged("rename b")).await.unwrap();
        let mut file = f.file();
        file.comments[0].gerrit.as_mut().unwrap().state = Outbox::Sent;
        f.store.save(&file).unwrap();

        let text = EditComment {
            body: Some("rename it".to_owned()),
            ..EditComment::default()
        };
        let error = edit(&f.store, "I8f3a", (&f.rev, Some(3)), &first.id, text).unwrap_err();
        assert!(error.to_string().contains("Change it there"), "{error}");
        let error = edit(&f.store, "I8f3a", (&f.rev, Some(3)), &first.id, stage(None)).unwrap_err();
        assert!(error.to_string().contains("Change it there"), "{error}");
    }

    #[test]
    fn a_null_takes_a_remark_back_and_an_absent_field_leaves_it() {
        let back: EditComment = serde_json::from_str(r#"{ "gerrit": null }"#).unwrap();
        let left: EditComment = serde_json::from_str(r#"{ "body": "x" }"#).unwrap();
        let staged: EditComment = serde_json::from_str(r#"{ "gerrit": {} }"#).unwrap();

        assert!(matches!(back.gerrit, Some(None)));
        assert!(left.gerrit.is_none());
        assert!(matches!(
            staged.gerrit,
            Some(Some(Stage { resolve: false }))
        ));
    }

    #[tokio::test]
    async fn a_reply_to_a_thread_of_gerrit_takes_its_place_and_its_patch_set() {
        let f = fixture().await;
        let thread = GerritThread {
            id: "g1".to_owned(),
            patch_set: 3,
            commit: f.rev.clone(),
            scope: Scope::Line,
            anchor: Some(Anchor {
                file: "a.c".to_owned(),
                side: Side::New,
                start_line: Some(2),
                end_line: Some(2),
                start_char: None,
                end_char: None,
                blob: None,
                line_hash: None,
                context: Vec::new(),
            }),
        };
        let target = Target {
            gerrit_thread: Some(thread),
            ..f.target()
        };

        let talk = target
            .add(reply("gerrit:g1", Author::Agent, "It stops at EOF."))
            .await
            .unwrap();
        assert_eq!(talk.parent.as_deref(), Some("gerrit:g1"));
        assert_eq!(gerrit_parent(&talk), Some("g1"));
        assert!(talk.gerrit.is_none(), "a plain reply stays here");

        let done = target
            .add(NewComment {
                gerrit: Some(Stage { resolve: true }),
                ..reply("gerrit:g1", Author::Reader, "Done")
            })
            .await
            .unwrap();
        let mark = done.gerrit.clone().unwrap();
        assert_eq!((mark.patch_set, mark.resolve), (3, true));
        assert_eq!(done.anchor.as_ref().unwrap().start_line, Some(2));
        assert!(is_staged(&done));

        // The Done box clears the resolve, and keeps the reply staged.
        let open = EditComment {
            gerrit: Some(Some(Stage { resolve: false })),
            ..EditComment::default()
        };
        let open = edit(&f.store, "I8f3a", (&f.rev, Some(3)), &done.id, open).unwrap();
        assert!(is_staged(&open));
        assert!(!open.gerrit.unwrap().resolve);

        // It can be kept here, but not staged again.
        let kept = EditComment {
            gerrit: Some(None),
            ..EditComment::default()
        };
        let back = edit(&f.store, "I8f3a", (&f.rev, None), &done.id, kept).unwrap();
        assert!(back.gerrit.is_none());
        let again = EditComment {
            gerrit: Some(Some(Stage::default())),
            ..EditComment::default()
        };
        let error = edit(&f.store, "I8f3a", (&f.rev, Some(3)), &done.id, again).unwrap_err();
        assert!(error.to_string().contains("new reply"), "{error}");

        // Neither is work for the agent: the thread is on Gerrit.
        assert_eq!(counts(&f.store, "I8f3a", &f.rev).total, 0);
    }
}
