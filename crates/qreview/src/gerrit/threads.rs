//! The remarks of a change as the REST API gives them: with an id, a reply
//! link, a side, a range and a resolved state.
//!
//! The ssh query gives none of these. See section 6.8 of the design.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use super::Person;

/// One comment of Gerrit, a `CommentInfo`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct RestComment {
    /// The key of the map that holds the comment, not a field of it.
    #[serde(default)]
    pub path: String,
    pub id: String,
    #[serde(default)]
    pub patch_set: usize,
    /// `PARENT` for the left side. Absent for the revision.
    #[serde(default)]
    pub side: Option<String>,
    /// Absent, or 0, on a remark about the whole file.
    #[serde(default)]
    pub line: Option<usize>,
    #[serde(default)]
    pub range: Option<Range>,
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub updated: String,
    /// Absent on a draft: its author is the reader.
    #[serde(default)]
    pub author: Option<Person>,
    #[serde(default)]
    pub unresolved: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Range {
    pub start_line: usize,
    #[serde(default)]
    pub start_character: usize,
    pub end_line: usize,
    #[serde(default)]
    pub end_character: usize,
}

impl RestComment {
    pub fn on_parent(&self) -> bool {
        self.side.as_deref() == Some("PARENT")
    }
}

/// The comments of `GET …/comments` or `GET …/drafts`: a map from a path to
/// its comments. Answers in the order of the dates.
pub fn parse(answer: &Value) -> Vec<RestComment> {
    let mut out = Vec::new();
    let Some(files) = answer.as_object() else {
        return out;
    };
    for (path, comments) in files {
        for comment in comments.as_array().into_iter().flatten() {
            // One comment that Gerrit wrote in a shape we do not know must
            // not hide the others.
            if let Ok(mut comment) = serde_json::from_value::<RestComment>(comment.clone()) {
                comment.path = path.clone();
                out.push(comment);
            }
        }
    }
    out.sort_by(|a, b| a.updated.cmp(&b.updated));
    out
}

/// The comment that opens the thread of each comment, by id.
///
/// A comment that answers a comment Gerrit did not give, deleted or a
/// draft of somebody else, opens a thread of its own: it is never lost.
pub fn roots(comments: &[RestComment]) -> HashMap<String, String> {
    let by_id: HashMap<&str, &RestComment> = comments.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut out = HashMap::new();

    for comment in comments {
        let mut at = comment;
        // A loop in the links would never end: stop after as many steps as
        // there are comments.
        for _ in 0..comments.len() {
            match at.in_reply_to.as_deref().and_then(|id| by_id.get(id)) {
                Some(parent) => at = parent,
                None => break,
            }
        }
        out.insert(comment.id.clone(), at.id.clone());
    }
    out
}

/// The threads that wait for an answer, by the id of their first comment.
///
/// A thread is unresolved when its newest comment is, the rule of Gerrit.
pub fn unresolved(comments: &[RestComment]) -> Vec<String> {
    let roots = roots(comments);
    let mut last: HashMap<&str, &RestComment> = HashMap::new();
    for comment in comments {
        let root = roots[&comment.id].as_str();
        let newer = last
            .get(root)
            .is_none_or(|known| known.updated <= comment.updated);
        if newer {
            last.insert(root, comment);
        }
    }
    let mut out: Vec<String> = last
        .into_iter()
        .filter(|(_, comment)| comment.unresolved == Some(true))
        .map(|(root, _)| root.to_owned())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded() -> Vec<RestComment> {
        let path = format!(
            "{}/tests/data/gerrit/rest-comments.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(path).unwrap();
        parse(&serde_json::from_str(&text).unwrap())
    }

    #[test]
    fn every_comment_carries_its_path_and_its_place() {
        let found = recorded();

        assert_eq!(found.len(), 5);
        // In the order of the dates, whatever the file.
        assert_eq!(found[0].id, "w1");
        let first = &found[1];
        assert_eq!(first.path, "src/net.c");
        assert_eq!(first.id, "a1");
        assert_eq!(first.patch_set, 1);
        assert_eq!(first.line, Some(3));
        assert_eq!(first.author.as_ref().unwrap().label(), "Jane Reviewer");
        assert_eq!(
            first.range,
            Some(Range {
                start_line: 3,
                start_character: 4,
                end_line: 3,
                end_character: 8
            })
        );

        let old = found.iter().find(|c| c.id == "p1").unwrap();
        assert!(old.on_parent());
        let whole = found.iter().find(|c| c.id == "w1").unwrap();
        assert_eq!(whole.path, "/PATCHSET_LEVEL");
    }

    #[test]
    fn a_reply_joins_the_thread_of_the_comment_it_answers() {
        let roots = roots(&recorded());

        assert_eq!(roots["a1"], "a1");
        assert_eq!(roots["a2"], "a1");
        assert_eq!(roots["a3"], "a1");
        assert_eq!(roots["p1"], "p1");
    }

    #[test]
    fn a_thread_waits_when_its_newest_comment_does() {
        // a1 asks, a2 answers and resolves, a3 opens it again. p1 is
        // resolved, and w1 waits.
        assert_eq!(unresolved(&recorded()), ["a1", "w1"]);
    }

    #[test]
    fn a_reply_to_a_comment_not_given_opens_its_own_thread() {
        let lost = RestComment {
            id: "r".to_owned(),
            in_reply_to: Some("gone".to_owned()),
            unresolved: Some(true),
            ..RestComment::default()
        };

        assert_eq!(roots(std::slice::from_ref(&lost))["r"], "r");
        assert_eq!(unresolved(&[lost]), ["r"]);
    }

    #[test]
    fn something_else_than_a_map_is_no_comment() {
        assert!(parse(&serde_json::json!([1, 2])).is_empty());
        assert!(parse(&serde_json::json!({ "a.c": [{ "no": "id" }] })).is_empty());
    }
}
