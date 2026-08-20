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
    pub unresolved_count: usize,
    pub is_merge: bool,
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
