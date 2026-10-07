//! What a review holds on disk.

use serde::{Deserialize, Serialize};

/// The format of a change file. A change owes a migration, or a refusal with
/// a clear message. Never a silent read of an older shape.
///
/// 2 added the character range of an anchor. A file of format 1 reads as one
/// whose comments cover whole lines, and `Store::load` stamps it 2 so the
/// next write says what it is.
///
/// 3 added the commit a comment was written against, which is what lets a
/// second round find the version that was reviewed. A comment of format 2
/// names none, and takes no part in that.
///
/// 4 added threads: the author of a comment, the remark a reply belongs to,
/// and the Done and blocked states. A file of format 3 reads as remarks of
/// the reader, each alone in its thread, none of them done.
pub const VERSION: u32 = 4;

/// Everything a review of one change holds.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeFile {
    pub version: u32,
    /// The Change-Id, or `sha-<hash>` when the commit carries none.
    pub key: String,
    pub subject: String,
    /// The reader marked this change read. It says nothing about the
    /// comments: a change can be read and still carry remarks.
    #[serde(default)]
    pub reviewed: bool,
    #[serde(default)]
    pub comments: Vec<Comment>,
}

impl ChangeFile {
    pub fn new(key: &str, subject: &str) -> Self {
        Self {
            version: VERSION,
            key: key.to_owned(),
            subject: subject.to_owned(),
            reviewed: false,
            comments: Vec::new(),
        }
    }
}

/// What a comment is attached to.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Line,
    Range,
    File,
    Change,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Old,
    New,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    /// The patch set the comment was written against.
    pub patch_set: usize,
    /// The commit it was written against.
    ///
    /// An amend gives the change a new sha, and this is how a later run
    /// finds the version that was reviewed without being told. Empty on a
    /// comment written by a qreview older than format 3.
    #[serde(default)]
    pub commit: String,
    pub created_at: String,
    pub updated_at: String,
    pub scope: Scope,
    pub body: String,
    #[serde(default)]
    pub anchor: Option<Anchor>,
    #[serde(default)]
    pub author: Author,
    /// The remark that opens the thread, on a reply. A reply has no anchor:
    /// it stands where its thread stands.
    #[serde(default)]
    pub parent: Option<String>,
    /// The thread is done. Only the remark that opens it carries this.
    #[serde(default)]
    pub done: bool,
    /// A reply of the agent that waits for the reader before the code can
    /// change.
    #[serde(default)]
    pub blocked: bool,
}

impl Comment {
    /// True when this comment opens a thread.
    pub fn is_remark(&self) -> bool {
        self.parent.is_none()
    }
}

/// Who wrote a comment. A label, not a permission: both hold the same token.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Author {
    #[default]
    Reader,
    Agent,
}

/// Where a comment sits, and enough context to find that place again in
/// another patch set.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub file: String,
    pub side: Side,
    /// Absent on a comment about the file as a whole.
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
    /// The first character of the range on `start_line`, and the one after
    /// the last on `end_line`, in UTF-16 units. Absent when the comment
    /// covers whole lines.
    ///
    /// UTF-16 because the browser makes them, out of a selection in the
    /// page, and every offset that crosses the wire counts the same units.
    #[serde(default)]
    pub start_char: Option<usize>,
    #[serde(default)]
    pub end_char: Option<usize>,
    /// The blob the lines were read from.
    #[serde(default)]
    pub blob: Option<String>,
    /// A hash of the anchored line, trimmed of trailing space.
    #[serde(default)]
    pub line_hash: Option<String>,
    /// The lines around the anchor, for finding it again.
    #[serde(default)]
    pub context: Vec<String>,
}
