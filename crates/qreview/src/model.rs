//! The types that cross the wire.
//!
//! These are the contract with the interface. A change here is a change to
//! `web/src/api/types.ts`, and a snapshot test says so.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// The top level directory of the repository.
    pub root: String,
    /// The canonical remote URL, when there is a remote.
    pub remote: Option<String>,
    /// The identity the comment store is keyed by.
    pub id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub repo: RepoInfo,
    /// The newest change of the series.
    pub head: String,
    /// The oldest change loaded so far.
    pub oldest: String,
    pub changes: Vec<ChangeSummary>,
    /// Why the walk stopped.
    pub boundary: Boundary,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSummary {
    /// The Change-Id, or `sha-<hash>` when the commit carries none.
    pub key: String,
    pub change_id: Option<String>,
    pub subject: String,
    pub author: String,
    pub commit: String,
    pub patch_set_count: usize,
    pub comment_count: usize,
    /// The reader marked this change read.
    pub reviewed: bool,
    pub is_merge: bool,
    /// The work that is not committed yet, not a commit of the history.
    #[serde(default)]
    pub worktree: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BoundaryKind {
    /// Another line of history starts here. Never crossed in silence.
    Merge,
    /// A tag points at the commit. A release is a history boundary.
    Tag,
    /// The base that the rules resolved.
    Base,
    /// The guess stopped, and `reason` says on which signal.
    Guess,
    /// The batch filled up. Nothing is wrong; load more.
    Batch,
    /// The history has no parent left.
    Root,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Boundary {
    pub kind: BoundaryKind,
    /// The commit under the boundary. It is not loaded yet.
    pub commit: Option<String>,
    /// Shown on the card, for example "on origin/rel-3.0".
    pub reason: String,
    /// True when a guess produced this stop.
    pub guessed: bool,
    pub merge: Option<MergeInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MergeInfo {
    pub subject: String,
    pub parents: Vec<ParentInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParentInfo {
    pub commit: String,
    /// What the commit is called, for example `side` or `origin/rel-3.0`.
    pub name: String,
    /// True when a remote-tracking ref reaches it. Following it would drag a
    /// whole branch into the review.
    pub remote: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// One file of a change, without its content.
///
/// The file list route answers with these. A change of 200 files must not
/// build 200 diffs to show the first one.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    /// Set on a rename or a copy.
    pub old_path: Option<String>,
    pub status: FileStatus,
    /// The syntect language name.
    pub language: String,
    pub binary: bool,
    pub added: usize,
    pub removed: usize,
}

/// The comments of one change, in the order a review reads them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChangeComments {
    pub key: String,
    pub subject: String,
    pub commit: String,
    pub comments: Vec<crate::store::model::Comment>,
}

/// One file of a change, with its content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    #[serde(flatten)]
    pub file: FileEntry,
    pub hunks: Vec<Hunk>,
    /// How many lines the new side has, so the interface knows how much
    /// context sits between the hunks and after the last one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    /// What git prints after the second `@@`, usually the enclosing function.
    pub header: String,
    pub rows: Vec<Row>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RowKind {
    Context,
    Add,
    Remove,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub kind: RowKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
    /// The file has no newline after this line.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_newline: bool,
    /// Syntax classes, from syntect. Empty until the file is highlighted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tokens: Vec<Span>,
    /// Intra-line marks. Absent on a context row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordSpan>,
}

/// A range of `Row::text` that carries a syntax class.
///
/// The offsets are UTF-16 code units, which is what a browser slices with.
/// They count bytes inside the server, until the row leaves it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub start: usize,
    pub end: usize,
    /// A CSS class, for example `keyword control`.
    pub cls: String,
}

/// A range of `Row::text` that changed inside the line, in UTF-16 units.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WordSpan {
    pub start: usize,
    pub end: usize,
}
