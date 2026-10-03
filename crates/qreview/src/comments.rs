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
use crate::store::model::{Anchor, Author, ChangeFile, Comment, Scope, Side};

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

        let mut file = self.store.load(self.key, self.subject)?;
        if new.parent.is_some() {
            let reply = self.reply(&mut file, new)?;
            self.store.save(&file)?;
            return Ok(reply);
        }
        if new.blocked {
            bail!("only a reply waits for the reader");
        }

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
        };

        file.comments.push(comment.clone());
        self.store.save(&file)?;

        Ok(comment)
    }

    /// Add a reply to its thread, and move the state of the thread with it.
    fn reply(&self, file: &mut ChangeFile, new: NewComment) -> Result<Comment> {
        let parent = new.parent.as_deref().unwrap_or_default();
        let at = thread_of(file, parent)?;
        let remark = &mut file.comments[at];

        if !of_version(remark, self.rev) {
            bail!("this thread belongs to an earlier version, and its round is over");
        }
        if new.blocked && new.author != Author::Agent {
            bail!("only the agent waits for the reader");
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

/// Change the text of a comment, or check or clear the Done box of its
/// thread.
pub fn edit(
    store: &Store,
    key: &str,
    current: &str,
    id: &str,
    edit: EditComment,
) -> Result<Comment> {
    let mut file = store.load(key, "")?;
    let found = file
        .comments
        .iter_mut()
        .find(|c| c.id == id)
        .with_context(|| format!("no comment {id}"))?;

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

/// Delete a comment. A remark goes with its replies, which answer nothing
/// once it is gone.
pub fn delete(store: &Store, key: &str, id: &str) -> Result<usize> {
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

/// True when the remark was written on the version being read.
///
/// A remark from a store older than format 3 names no version. It counts as
/// this one: it is the only version it can belong to, and leaving it out of
/// the export would lose a review that nothing would show again.
pub fn of_version(comment: &Comment, commit: &str) -> bool {
    comment.commit.is_empty() || comment.commit == commit
}

/// True when the comment opens a thread of this version that is not done:
/// work the review still asks for.
pub fn is_open(comment: &Comment, commit: &str) -> bool {
    comment.is_remark() && !comment.done && of_version(comment, commit)
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
            Target {
                store: &self.store,
                git: &self.git,
                rev: &self.rev,
                base: &self.base,
                key: "I8f3a",
                subject: "second",
                patch_set: 1,
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
            &f.rev,
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

        let error = edit(&f.store, "I8f3a", &f.rev, &answer.id, checked).unwrap_err();
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
        assert!(edit(&f.store, "I8f3a", &amended, &first.id, checked).is_err());
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
    async fn a_reply_is_not_counted_as_a_remark() {
        let f = fixture().await;
        let first = f.target().add(remark("one")).await.unwrap();
        f.target()
            .add(reply(&first.id, Author::Agent, "a"))
            .await
            .unwrap();

        assert_eq!(counts(&f.store, "I8f3a", &f.rev).total, 1);
    }
}
