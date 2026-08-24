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
use crate::store::model::{Anchor, ChangeFile, Comment, Scope, Side};

/// How many lines above and below the anchor are kept.
const CONTEXT: usize = 3;

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
}

/// What the interface sends to change one.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditComment {
    #[serde(default)]
    pub body: Option<String>,
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
pub fn counts(store: &Store, key: &str) -> Counts {
    match store.load(key, "") {
        Ok(file) => Counts {
            total: file.comments.len(),
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
        let now = now();
        let anchor = match new.scope {
            Scope::Change => None,
            _ => Some(anchor_of(self.git, self.rev, self.base, &new).await?),
        };

        let comment = Comment {
            id: new_id(),
            patch_set: self.patch_set,
            created_at: now.clone(),
            updated_at: now,
            scope: new.scope,
            body: new.body.trim_end().to_owned(),
            anchor,
        };

        file.comments.push(comment.clone());
        self.store.save(&file)?;

        Ok(comment)
    }
}

/// Change the text of a comment, or resolve its thread.
pub fn edit(store: &Store, key: &str, id: &str, edit: EditComment) -> Result<Comment> {
    let mut file = store.load(key, "")?;
    let found = file
        .comments
        .iter_mut()
        .find(|c| c.id == id)
        .with_context(|| format!("no comment {id}"))?;

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

/// Delete a comment.
pub fn delete(store: &Store, key: &str, id: &str) -> Result<usize> {
    let mut file = store.load(key, "")?;
    let before = file.comments.len();

    if !file.comments.iter().any(|c| c.id == id) {
        bail!("no comment {id}");
    }

    file.comments.retain(|c| c.id != id);
    store.save(&file)?;

    Ok(before - file.comments.len())
}

/// Where the comment sits, with enough of the file around it to find the
/// place again in another patch set.
async fn anchor_of(git: &Git, rev: &str, base: &str, new: &NewComment) -> Result<Anchor> {
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
