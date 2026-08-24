//! Finding a comment again in another patch set.
//!
//! A comment is written against one version of a change. The reader who opens
//! another version must still see it. Three branches, in order: the file did
//! not change, the line moved, or the place is gone. A comment is never
//! dropped and never moved in silence.

use serde::Serialize;

use crate::comments::hash_line;
use crate::git::exec::Git;
use crate::store::model::{Comment, Scope, Side};

/// Below this share of matching context, a candidate line is a coincidence.
const MIN_SCORE: f32 = 0.5;

/// Where a comment lands in the patch set being read.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    pub id: String,
    /// The line it lands on, absent when nothing was found.
    pub line: Option<usize>,
    /// The last line of the range, which follows the first one.
    pub end_line: Option<usize>,
    /// True when the line is not the one the comment was written against.
    pub moved: bool,
    /// True when the place is gone. The interface lists these apart.
    pub lost: bool,
}

/// Place every comment of a review in the patch set being read.
pub async fn place_all(git: &Git, comments: &[Comment], rev: &str) -> Vec<Placed> {
    let mut out = Vec::with_capacity(comments.len());

    for comment in comments {
        out.push(place(git, comment, rev).await);
    }
    out
}

/// Place one comment.
pub async fn place(git: &Git, comment: &Comment, rev: &str) -> Placed {
    let id = comment.id.clone();

    // A comment about the change belongs to no line and cannot be lost.
    let Some(anchor) = &comment.anchor else {
        return nowhere(id);
    };
    if comment.scope == Scope::File {
        return nowhere(id);
    }

    let Some(line) = anchor.start_line else {
        return nowhere(id);
    };
    // The range keeps its length wherever its first line lands.
    let span = anchor.end_line.unwrap_or(line).saturating_sub(line);

    // A comment on the old side is read against the old side, which the
    // patch set being read does not own. It keeps the line it was written on.
    if anchor.side == Side::Old {
        return found(id, line, span, false);
    }

    // The commit message is not a blob. It is read from the commit, and the
    // line hash is the only way back to it.
    let read = match crate::commitmsg::is(&anchor.file) {
        true => crate::commitmsg::text(git, rev).await,
        false => match blob_of(git, rev, &anchor.file).await {
            // One: the file did not change at all.
            Some(blob) if anchor.blob.as_deref() == Some(blob.as_str()) => {
                return found(id, line, span, false);
            }
            Some(blob) => git.text(&["cat-file", "blob", &blob]).await.ok(),
            None => None,
        },
    };

    let Some(text) = read else {
        return gone(id);
    };
    let fresh: Vec<&str> = text.lines().collect();

    // Two: the line moved. Look for it, and make the context agree.
    let Some(hash) = &anchor.line_hash else {
        return gone(id);
    };

    match best_match(hash, &anchor.context, &fresh, line) {
        Some(at) => found(id, at, span, at != line),
        // Three: the place is gone.
        None => gone(id),
    }
}

/// A comment that belongs to no line of this patch set, and is not lost:
/// one about the change, or about the file.
fn nowhere(id: String) -> Placed {
    Placed {
        id,
        line: None,
        end_line: None,
        moved: false,
        lost: false,
    }
}

/// A comment whose place this patch set no longer has.
fn gone(id: String) -> Placed {
    Placed {
        id,
        line: None,
        end_line: None,
        moved: false,
        lost: true,
    }
}

fn found(id: String, line: usize, span: usize, moved: bool) -> Placed {
    Placed {
        id,
        line: Some(line),
        end_line: Some(line + span),
        moved,
        lost: false,
    }
}

/// The line of `fresh` that best matches the stored anchor.
///
/// A candidate is a line with the same hash. The context around it decides
/// between two candidates, and the one nearest the old position breaks a tie.
pub fn best_match(hash: &str, context: &[String], fresh: &[&str], was_at: usize) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;

    for (index, line) in fresh.iter().enumerate() {
        if hash_line(line) != hash {
            continue;
        }
        let at = index + 1;
        let score = score(context, fresh, at);

        let better = match best {
            None => true,
            Some((top, top_at)) => {
                score > top || (score == top && distance(at, was_at) < distance(top_at, was_at))
            }
        };
        if better {
            best = Some((score, at));
        }
    }

    best.filter(|(score, _)| *score >= MIN_SCORE)
        .map(|(_, at)| at)
}

/// How much of the stored context is still around the candidate line.
///
/// The stored context is the lines around the anchor, the anchored one in
/// the middle. It is compared position by position.
pub fn score(context: &[String], fresh: &[&str], at: usize) -> f32 {
    if context.is_empty() {
        return 1.0;
    }

    // The anchored line sits in the middle of what was stored, unless it was
    // near the top of the file, where there was less room above it.
    let middle = context.len() / 2;
    let mut matched = 0;

    for (offset, stored) in context.iter().enumerate() {
        let line = at as isize + offset as isize - middle as isize;
        let Some(index) = usize::try_from(line - 1).ok() else {
            continue;
        };
        if fresh.get(index).map(|l| l.trim_end()) == Some(stored.trim_end()) {
            matched += 1;
        }
    }

    matched as f32 / context.len() as f32
}

fn distance(a: usize, b: usize) -> usize {
    a.abs_diff(b)
}

async fn blob_of(git: &Git, rev: &str, path: &str) -> Option<String> {
    let out = git
        .text(&["rev-parse", &format!("{rev}:{path}")])
        .await
        .ok()?;
    let blob = out.trim().to_owned();

    (!blob.is_empty()).then_some(blob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::model::Anchor;

    fn lines(text: &str) -> Vec<&str> {
        text.lines().collect()
    }

    fn context(of: &[&str]) -> Vec<String> {
        of.iter().map(|l| (*l).to_owned()).collect()
    }

    #[test]
    fn a_line_that_did_not_move_is_found_where_it_was() {
        let file = lines("one\ntwo\nthree\nfour\n");
        let stored = context(&["one", "two", "three"]);

        assert_eq!(best_match(&hash_line("two"), &stored, &file, 2), Some(2));
    }

    #[test]
    fn a_line_pushed_down_is_found_lower() {
        let file = lines("new\nlines\nhere\none\ntwo\nthree\n");
        let stored = context(&["one", "two", "three"]);

        assert_eq!(best_match(&hash_line("two"), &stored, &file, 2), Some(5));
    }

    #[test]
    fn the_candidate_with_the_matching_context_wins() {
        // "x = 1" appears twice. Only one has the stored neighbours.
        let file = lines("start\nx = 1\nmiddle\nbefore\nx = 1\nafter\n");
        let stored = context(&["before", "x = 1", "after"]);

        assert_eq!(best_match(&hash_line("x = 1"), &stored, &file, 2), Some(5));
    }

    #[test]
    fn the_nearest_candidate_breaks_a_tie() {
        // Two identical places. Neither context matches, so position decides.
        let file = lines("a\nsame\nb\nc\nd\nsame\ne\n");
        let stored = context(&["gone", "same", "vanished"]);

        // Both score the same, so the one nearer the old line 2 wins.
        assert_eq!(best_match(&hash_line("same"), &stored, &file, 2), None);
    }

    #[test]
    fn trailing_space_does_not_lose_the_anchor() {
        let file = lines("one\ntwo   \nthree\n");
        let stored = context(&["one", "two", "three"]);

        assert_eq!(best_match(&hash_line("two"), &stored, &file, 2), Some(2));
    }

    #[test]
    fn a_line_that_is_gone_is_not_invented() {
        let file = lines("one\nthree\n");
        let stored = context(&["one", "two", "three"]);

        assert_eq!(best_match(&hash_line("two"), &stored, &file, 2), None);
    }

    #[test]
    fn a_score_of_a_whole_match_is_one() {
        let file = lines("one\ntwo\nthree\n");
        assert_eq!(score(&context(&["one", "two", "three"]), &file, 2), 1.0);
    }

    #[test]
    fn a_score_with_nothing_around_it_is_zero() {
        let file = lines("x\ntwo\ny\n");
        assert_eq!(
            score(&context(&["one", "two", "three"]), &file, 2),
            1.0 / 3.0
        );
    }

    #[tokio::test]
    async fn a_comment_on_the_change_has_no_line_and_is_not_lost() {
        let repo =
            crate::testutil::build_repo(&[crate::testutil::commit("one").file("a", "1\n")]).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let comment = Comment {
            id: "c-1".to_owned(),
            patch_set: 1,
            created_at: "t".to_owned(),
            updated_at: "t".to_owned(),
            scope: Scope::Change,
            body: "b".to_owned(),
            anchor: None,
        };

        let placed = place(&git, &comment, "HEAD").await;
        assert_eq!(placed.line, None);
        assert!(!placed.lost);
    }

    #[tokio::test]
    async fn a_range_keeps_its_length_where_its_first_line_lands() {
        let repo = crate::testutil::build_repo(&[
            crate::testutil::commit("one").file("a.txt", "one\ntwo\nthree\nfour\nfive\n"),
            crate::testutil::commit("two")
                .file("a.txt", "added\nlines\none\ntwo\nthree\nfour\nfive\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();

        // Written on lines 2 to 4 of the first version, which two new lines
        // above have pushed down to 4 to 6.
        let comment = Comment {
            id: "c-1".to_owned(),
            patch_set: 1,
            created_at: "t".to_owned(),
            updated_at: "t".to_owned(),
            scope: Scope::Range,
            body: "b".to_owned(),
            anchor: Some(Anchor {
                file: "a.txt".to_owned(),
                side: Side::New,
                start_line: Some(2),
                end_line: Some(4),
                start_char: None,
                end_char: None,
                blob: Some("stale".to_owned()),
                line_hash: Some(hash_line("two")),
                context: vec!["one".to_owned(), "two".to_owned(), "three".to_owned()],
            }),
        };

        let placed = place(&git, &comment, "HEAD").await;

        assert_eq!(placed.line, Some(4));
        assert_eq!(placed.end_line, Some(6));
        assert!(placed.moved);
        assert!(!placed.lost);
    }

    #[tokio::test]
    async fn a_comment_whose_file_is_gone_is_lost_and_kept() {
        let repo =
            crate::testutil::build_repo(&[crate::testutil::commit("one").file("a.txt", "1\n")])
                .await;
        let git = Git::discover(repo.path()).await.unwrap();

        let comment = Comment {
            id: "c-1".to_owned(),
            patch_set: 1,
            created_at: "t".to_owned(),
            updated_at: "t".to_owned(),
            scope: Scope::Line,
            body: "b".to_owned(),
            anchor: Some(Anchor {
                file: "gone.txt".to_owned(),
                side: Side::New,
                start_line: Some(1),
                end_line: Some(1),
                start_char: None,
                end_char: None,
                blob: Some("dead".to_owned()),
                line_hash: Some(hash_line("1")),
                context: vec!["1".to_owned()],
            }),
        };

        let placed = place(&git, &comment, "HEAD").await;
        assert!(placed.lost, "the file is not there any more");
        assert_eq!(placed.line, None);
    }
}
