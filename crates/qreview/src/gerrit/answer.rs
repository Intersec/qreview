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

/// Whoever wrote a comment on the server.
///
/// Every field is optional: a robot account often has no name, and a server
/// hides the address of a user who asked it to.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub username: String,
}

impl Person {
    /// What to show. The name, or the next best thing the server gave.
    pub fn label(&self) -> String {
        for field in [&self.name, &self.username, &self.email] {
            if !field.trim().is_empty() {
                return field.trim().to_owned();
            }
        }
        "someone".to_owned()
    }
}

/// One remark posted on a line of a patch set.
///
/// The ssh query gives no id, no side and no reply link. Two remarks on one
/// line are a thread, in the order the server lists them, and every line
/// number is a line of the new side. See `roadmap/design.md` section 6.3.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InlineComment {
    pub file: String,
    /// Absent on a remark about the whole file.
    #[serde(default)]
    pub line: Option<usize>,
    #[serde(default)]
    pub reviewer: Person,
    #[serde(default)]
    pub message: String,
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
    /// The remarks posted on this version, with `--comments`.
    #[serde(default)]
    pub comments: Vec<InlineComment>,
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
    fn the_remarks_of_a_patch_set_are_read_with_their_author() {
        let changes = parse(&recorded("with-comments.json"));
        let sets = &changes[0].patch_sets;

        assert_eq!(sets[0].comments.len(), 3);
        let first = &sets[0].comments[0];
        assert_eq!(first.file, "src/net.blk");
        assert_eq!(first.line, Some(3));
        assert_eq!(first.reviewer.label(), "Jane Reviewer");
        assert!(first.message.starts_with("This loop retries"));

        // Two remarks on one line are a thread, in the order the server
        // lists them. The ssh answer carries no reply link.
        assert_eq!(sets[0].comments[1].line, Some(3));
        assert_eq!(sets[0].comments[1].reviewer.label(), "A Developer");

        // No line: a remark about the whole file.
        assert_eq!(sets[0].comments[2].line, None);
        assert_eq!(sets[0].comments[2].reviewer.label(), "buildbot");

        assert_eq!(sets[1].comments.len(), 1);
        assert_eq!(sets[1].comments[0].reviewer.label(), "nameless@example.com");
    }

    #[test]
    fn an_answer_with_no_comments_reads_as_none() {
        let changes = parse(&recorded("one-change.json"));

        assert!(changes[0].patch_sets.iter().all(|s| s.comments.is_empty()));
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
