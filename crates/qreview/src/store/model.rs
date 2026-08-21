//! What a review holds on disk.

use serde::{Deserialize, Serialize};

/// The format of a change file. A change owes a migration, or a refusal with
/// a clear message. Never a silent read of an older shape.
pub const VERSION: u32 = 1;

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
    pub created_at: String,
    pub updated_at: String,
    pub scope: Scope,
    pub body: String,
    #[serde(default)]
    pub anchor: Option<Anchor>,
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
