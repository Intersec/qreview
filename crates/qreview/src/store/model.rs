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
/// and the Done and blocked states, and the earlier places of a remark that
/// a declared new version moved. A file of format 3 reads as remarks of the
/// reader, each alone in its thread, none of them done or moved.
///
/// 5 added the remarks bound for Gerrit. A comment of format 4 carries no
/// `gerrit`, and stays in qreview.
///
/// 6 added the deleted comments, kept apart for a restore by hand and for
/// a study of the reviews, and the name of the agent on its comments. A
/// file of format 5 has no deleted comment, and no agent name.
pub const VERSION: u32 = 6;

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
    /// The comments that Delete and Clean took away, oldest first. Nothing
    /// reads them: a comment moved back to `comments` by hand is restored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<Deleted>,
}

/// A comment taken away, as it stood then.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    pub deleted_at: String,
    pub deleted_by: Author,
    pub how: Removal,
    pub comment: Comment,
}

/// What took a comment away.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Removal {
    /// Delete, on the comment or on the remark that opens its thread.
    Delete,
    /// Clean, which takes the done threads.
    Clean,
}

impl ChangeFile {
    pub fn new(key: &str, subject: &str) -> Self {
        Self {
            version: VERSION,
            key: key.to_owned(),
            subject: subject.to_owned(),
            reviewed: false,
            comments: Vec::new(),
            deleted: Vec::new(),
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
    /// The name that the agent gave when it wrote the comment. A draft of
    /// Gerrit shows it, because Gerrit names only the reader.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
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
    /// Where the remark stood before each declared new version moved it,
    /// oldest first. The move is recorded, never silent.
    #[serde(default)]
    pub moves: Vec<Move>,
    /// Set when the remark goes to Gerrit. Absent on a remark that stays
    /// in qreview, which every remark of format 4 is.
    #[serde(default)]
    pub gerrit: Option<Gerrit>,
}

/// A remark bound for Gerrit, and how far it went.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Gerrit {
    pub state: Outbox,
    /// The patch set of Gerrit that the draft goes to.
    pub patch_set: usize,
    /// On a reply to a thread of Gerrit, the draft resolves that thread.
    #[serde(default)]
    pub resolve: bool,
    /// The id that Gerrit gave the draft.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub sent_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Outbox {
    /// It waits for the reader to click Send.
    Staged,
    /// It is a draft on Gerrit, under the account of the reader.
    Sent,
}

/// One earlier place of a remark.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Move {
    pub commit: String,
    #[serde(default)]
    pub anchor: Option<Anchor>,
    pub moved_at: String,
}

/// The declared versions around one commit, kept under `commits/`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    #[serde(default)]
    pub previous_version: Option<String>,
    #[serde(default)]
    pub next_version: Option<String>,
    #[serde(default)]
    pub declared_at: String,
}

impl Comment {
    /// True when this comment opens a thread.
    pub fn is_remark(&self) -> bool {
        self.parent.is_none()
    }

    /// True when the remark goes to Gerrit, staged or sent. Such a remark
    /// is a message to a colleague, not work for the agent.
    pub fn is_for_gerrit(&self) -> bool {
        self.gerrit.is_some()
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
