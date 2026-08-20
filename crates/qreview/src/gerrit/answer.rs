//! Reading what `gerrit query` answers.
//!
//! One JSON object per line, and a last line of statistics. The parser is a
//! pure function from text to structure, so every answer we have seen becomes
//! a case in the corpus.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub project: String,
    pub branch: String,
    pub id: String,
    pub number: u64,
    pub subject: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "patchSets")]
    pub patch_sets: Vec<PatchSet>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchSet {
    pub number: usize,
    /// The commit of this version.
    pub revision: String,
    /// `refs/changes/NN/CCCC/P`, what a fetch asks for.
    #[serde(rename = "ref")]
    pub git_ref: String,
    #[serde(default)]
    pub created_on: i64,
    #[serde(default)]
    pub kind: String,
}

/// Parse the answer of `gerrit query --format=JSON`.
///
/// A line that is not a change is skipped, including the statistics line and
/// anything a newer server adds.
pub fn parse(text: &str) -> Vec<Change> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str::<Change>(line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded(name: &str) -> String {
        let path = format!("{}/tests/data/gerrit/{name}", env!("CARGO_MANIFEST_DIR"));

        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
    }

    #[test]
    fn one_change_with_three_patch_sets() {
        let changes = parse(&recorded("one-change.json"));

        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(change.number, 12321);
        assert_eq!(change.project, "myproject");
        assert_eq!(change.branch, "rel-3.0");
        assert_eq!(change.status, "NEW");
        assert_eq!(change.patch_sets.len(), 3);
        assert_eq!(change.patch_sets[2].number, 3);
        assert_eq!(change.patch_sets[2].git_ref, "refs/changes/21/12321/3");
    }

    #[test]
    fn a_query_that_found_nothing_is_an_empty_list() {
        assert!(parse(&recorded("nothing.json")).is_empty());
    }

    #[test]
    fn a_change_with_no_patch_set_still_parses() {
        let changes = parse(&recorded("no-patchsets.json"));

        assert_eq!(changes.len(), 1);
        assert!(changes[0].patch_sets.is_empty());
    }

    #[test]
    fn a_field_a_newer_server_adds_is_ignored() {
        let changes = parse(&recorded("extra-fields.json"));

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].number, 4242);
    }

    #[test]
    fn a_broken_line_does_not_lose_the_good_ones() {
        let text = format!("not json at all\n{}", recorded("one-change.json"));

        assert_eq!(parse(&text).len(), 1);
    }
}
