//! A remark of qreview, as a draft of Gerrit.
//!
//! The body of `PUT …/revisions/<revision>/drafts`, a `CommentInput` of
//! Gerrit. See section 6.7 of the design.

use serde_json::{Map, Value, json};

use crate::commitmsg;
use crate::git::commit;
use crate::git::exec::Git;
use crate::store::model::{Comment, Side};

/// The path Gerrit gives a remark about the whole change.
pub const PATCHSET_LEVEL: &str = "/PATCHSET_LEVEL";

/// The `CommentInput` of a remark written on `rev`, which is read against
/// `base`. `in_reply_to` is the newest comment of the thread of Gerrit that
/// a reply answers.
pub async fn input(
    git: &Git,
    comment: &Comment,
    rev: &str,
    base: &str,
    in_reply_to: Option<&str>,
) -> Value {
    let mut out = Map::new();
    out.insert("message".to_owned(), json!(comment.body));
    let resolve = comment.gerrit.as_ref().is_some_and(|g| g.resolve);
    out.insert("unresolved".to_owned(), json!(!resolve));
    if let Some(id) = in_reply_to {
        out.insert("in_reply_to".to_owned(), json!(id));
    }

    let Some(anchor) = &comment.anchor else {
        out.insert("path".to_owned(), json!(PATCHSET_LEVEL));
        return Value::Object(out);
    };
    let message = commitmsg::is(&anchor.file);
    let (Some(start), Some(end)) = (anchor.start_line, anchor.end_line) else {
        // A remark about the message as a whole is a remark about the change:
        // Gerrit has no file-level remark on `/COMMIT_MSG` in its Reply box.
        let path = if message {
            PATCHSET_LEVEL
        } else {
            &anchor.file
        };
        out.insert("path".to_owned(), json!(path));
        return Value::Object(out);
    };

    out.insert("path".to_owned(), json!(anchor.file));
    let tree = match anchor.side {
        Side::Old => {
            out.insert("side".to_owned(), json!("PARENT"));
            base
        }
        Side::New => rev,
    };

    // Gerrit shows the message under a header that qreview drops: one line
    // per parent, the author and the committer with their dates, and a
    // blank line. See section 6.3.
    let shift = match message {
        true => header_of(git, tree).await,
        false => 0,
    };
    out.insert("line".to_owned(), json!(end + shift));

    let whole_line = anchor.start_char.is_none() && anchor.end_char.is_none();
    if whole_line && start == end {
        return Value::Object(out);
    }

    // A range of whole lines ends at the end of its last line.
    let end_char = match anchor.end_char {
        Some(at) => Some(at),
        None => line_length(git, tree, &anchor.file, end).await,
    };
    let Some(end_char) = end_char else {
        // The line cannot be read: the remark still lands, on its last line.
        return Value::Object(out);
    };
    out.insert(
        "range".to_owned(),
        json!({
            "start_line": start + shift,
            "start_character": anchor.start_char.unwrap_or(0),
            "end_line": end + shift,
            "end_character": end_char,
        }),
    );
    Value::Object(out)
}

/// How many lines Gerrit puts above the message of a commit.
async fn header_of(git: &Git, rev: &str) -> usize {
    let parents = commit::info(git, rev)
        .await
        .map_or(1, |info| info.parents.len());
    parents + 5
}

/// The length of a line, in UTF-16 units, the units of every character
/// offset of the store and of Gerrit.
async fn line_length(git: &Git, tree: &str, path: &str, line: usize) -> Option<usize> {
    let text = match commitmsg::is(path) {
        true => commitmsg::text(git, tree).await?,
        false => git
            .text_of_object(&["show", &format!("{tree}:{path}")])
            .await
            .ok()?,
    };
    let found = text.lines().nth(line.checked_sub(1)?)?;
    Some(found.encode_utf16().count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::model::{Anchor, Author, Scope};
    use crate::testutil::{build_repo, commit};

    fn remark(file: &str, side: Side, lines: Option<(usize, usize)>) -> Comment {
        Comment {
            id: "c-1".to_owned(),
            patch_set: 1,
            commit: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            scope: Scope::Line,
            body: "Say why.".to_owned(),
            anchor: Some(Anchor {
                file: file.to_owned(),
                side,
                start_line: lines.map(|l| l.0),
                end_line: lines.map(|l| l.1),
                start_char: None,
                end_char: None,
                blob: None,
                line_hash: None,
                context: Vec::new(),
            }),
            author: Author::Reader,
            parent: None,
            done: false,
            blocked: false,
            moves: Vec::new(),
            gerrit: None,
        }
    }

    async fn repo() -> (crate::testutil::Repo, Git, String, String) {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: a change").file("a.c", "one\ntwo é\nthree\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;
        let parent = repo.sha("HEAD~1").await;
        (repo, git, head, parent)
    }

    #[tokio::test]
    async fn every_kind_of_remark_has_its_place_in_gerrit() {
        let (_repo, git, head, parent) = repo().await;
        let mut range = remark("a.c", Side::New, Some((2, 2)));
        if let Some(anchor) = range.anchor.as_mut() {
            anchor.start_char = Some(4);
            anchor.end_char = Some(5);
        }
        let mut change = remark("a.c", Side::New, None);
        change.anchor = None;

        let cases = [
            ("a line", remark("a.c", Side::New, Some((2, 2)))),
            ("whole lines", remark("a.c", Side::New, Some((2, 3)))),
            ("a part of a line", range),
            ("the old side", remark("a.c", Side::Old, Some((1, 1)))),
            ("the file", remark("a.c", Side::New, None)),
            (
                "a line of the message",
                remark("/COMMIT_MSG", Side::New, Some((1, 1))),
            ),
            ("the message", remark("/COMMIT_MSG", Side::New, None)),
            ("the change", change),
        ];
        let mut out = String::new();
        for (name, comment) in cases {
            let value = input(&git, &comment, &head, &parent, None).await;
            out.push_str(&format!("{name}: {value}\n"));
        }
        insta::assert_snapshot!(out);
    }

    #[tokio::test]
    async fn a_reply_names_the_thread_and_may_resolve_it() {
        let (_repo, git, head, parent) = repo().await;
        let mut reply = remark("a.c", Side::New, Some((2, 2)));
        reply.gerrit = Some(crate::store::model::Gerrit {
            state: crate::store::model::Outbox::Staged,
            patch_set: 1,
            resolve: true,
            id: None,
            sent_at: None,
        });

        let value = input(&git, &reply, &head, &parent, Some("last")).await;

        assert_eq!(value["in_reply_to"], "last");
        assert_eq!(value["unresolved"], false);
        assert_eq!(value["line"], 2);
    }
}
