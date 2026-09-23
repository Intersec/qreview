//! The remarks already posted on Gerrit, turned into something placeable.
//!
//! Read only, always. qreview never writes to the server: it votes on
//! nothing, posts nothing and replies to nothing. What it does is show what
//! is already there, on the line it speaks of, beside your own remarks.
//!
//! What the ssh answer gives, and what it does not, is in `roadmap/design.md`
//! section 6.4. The short of it: a file, a line, an author and a text. No id,
//! no side and no reply link.

use crate::anchor::{self, Placed};
use crate::comments::{self, NewComment};
use crate::commitmsg;
use crate::git::exec::Git;
use crate::model::PostedComment;
use crate::store::model::{Comment, Scope, Side};

/// One posted remark, ready for the interface and for the anchoring.
pub struct Posted {
    /// What the interface reads.
    pub wire: PostedComment,
    /// The same, shaped the way `anchor::place` wants it.
    pub placeable: Comment,
}

/// Every remark of every version of a change, oldest version first.
///
/// A version that is not in this clone has no line to hash, so its remarks
/// come back with an anchor that names a place and proves nothing. The
/// anchoring then calls them unplaced, which is the truth, rather than
/// dropping them or putting them on a line nobody chose.
pub async fn of_change(git: &Git, change: &super::Change) -> Vec<Posted> {
    let mut out = Vec::new();

    for set in &change.patch_sets {
        for (nth, posted) in set.comments.iter().enumerate() {
            out.push(one(git, set.number, nth, posted, &set.revision).await);
        }
    }
    out
}

/// The remarks of the two versions on the screen, each placed in its column.
///
/// Gerrit shows a remark on the patch set it was posted on and nowhere else,
/// and so does this: a remark of another version speaks of code that is not
/// on the screen. `rev` is the version on the right, and `left` the version
/// it is read against, when that is one.
pub async fn on_screen(
    git: &Git,
    found: Vec<Posted>,
    rev: &str,
    left: Option<&str>,
) -> (Vec<PostedComment>, Vec<Placed>) {
    let mut wire = Vec::new();
    let mut right_side = Vec::new();
    let mut left_side = Vec::new();

    for posted in found {
        if posted.placeable.commit == rev {
            right_side.push(posted.placeable);
        } else if left == Some(posted.placeable.commit.as_str()) {
            left_side.push(posted.placeable);
        } else {
            continue;
        }
        wire.push(posted.wire);
    }

    // Each remark reads the version it was posted on, whatever column that
    // version stands in.
    let mut placed = anchor::place_all(git, &right_side, rev, rev).await;
    if let Some(left) = left {
        let on_left = anchor::place_all(git, &left_side, left, left).await;
        placed.extend(on_left.into_iter().map(|p| Placed {
            side: Side::Old,
            ..p
        }));
    }

    (wire, placed)
}

async fn one(
    git: &Git,
    patch_set: usize,
    nth: usize,
    posted: &super::InlineComment,
    revision: &str,
) -> Posted {
    let line = line_of(posted);
    let wire = PostedComment {
        // The ssh answer carries no id. This one is made from the place, and
        // it is the same on every query as long as the server says the same.
        id: format!("g{patch_set}-{nth}"),
        patch_set,
        author: posted.reviewer.label(),
        body: posted.message.clone(),
        file: posted.file.clone(),
        line,
    };

    let scope = match line {
        Some(_) => Scope::Line,
        None => Scope::File,
    };
    let new = NewComment {
        scope,
        file: Some(posted.file.clone()),
        side: Some(Side::New),
        start_line: line,
        end_line: line,
        start_char: None,
        end_char: None,
        body: posted.message.clone(),
    };

    // A version that is not in this clone reads as no blob, no hash and no
    // context. That is a bare anchor, and a bare anchor is unplaced.
    let anchor = comments::anchor_of(git, revision, "", &new).await.ok();

    Posted {
        placeable: Comment {
            id: wire.id.clone(),
            patch_set,
            commit: revision.to_owned(),
            created_at: String::new(),
            updated_at: String::new(),
            scope,
            body: posted.message.clone(),
            anchor,
        },
        wire,
    }
}

/// The line the remark sits on, if it sits on one.
///
/// Gerrit writes line 0 for a remark about the whole file. And the message it
/// shows as `/COMMIT_MSG` carries a header of its own that qreview drops, so
/// the numbers of the two do not match: such a remark is read as a remark
/// about the message, not about a line of it.
fn line_of(posted: &super::InlineComment) -> Option<usize> {
    if commitmsg::is(&posted.file) {
        return None;
    }
    posted.line.filter(|line| *line > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gerrit::{Change, InlineComment, PatchSet, Person};

    fn posted(file: &str, line: Option<usize>, who: &str, what: &str) -> InlineComment {
        InlineComment {
            file: file.to_owned(),
            line,
            reviewer: Person {
                name: who.to_owned(),
                ..Person::default()
            },
            message: what.to_owned(),
        }
    }

    fn change(revision: &str, comments: Vec<InlineComment>) -> Change {
        versions(vec![(revision, comments)])
    }

    /// A change with one patch set per entry, numbered from 1.
    fn versions(sets: Vec<(&str, Vec<InlineComment>)>) -> Change {
        Change {
            project: "myproject".to_owned(),
            branch: "main".to_owned(),
            id: "Iwork".to_owned(),
            number: 1,
            subject: "work".to_owned(),
            url: String::new(),
            status: "NEW".to_owned(),
            patch_sets: sets
                .into_iter()
                .enumerate()
                .map(|(nth, (revision, comments))| PatchSet {
                    number: nth + 1,
                    revision: revision.to_owned(),
                    git_ref: format!("refs/changes/01/1/{}", nth + 1),
                    created_on: 0,
                    kind: "REWORK".to_owned(),
                    comments,
                })
                .collect(),
        }
    }

    /// Two versions of one file, where the second rewrote line 2. The first
    /// carries a remark on that line, the second a remark on line 3.
    async fn two_versions() -> (crate::testutil::Repo, Git, String, String, Vec<Posted>) {
        let repo = crate::testutil::build_repo(&[
            crate::testutil::commit("first").file("a.txt", "one\ntwo\nthree\n"),
            crate::testutil::commit("second").file("a.txt", "one\nTWO\nthree\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let first = repo.sha("HEAD~1").await;
        let second = repo.sha("HEAD").await;

        let found = of_change(
            &git,
            &versions(vec![
                (&first, vec![posted("a.txt", Some(2), "Jane", "too long")]),
                (&second, vec![posted("a.txt", Some(3), "Jane", "and this")]),
            ]),
        )
        .await;

        (repo, git, first, second, found)
    }

    #[tokio::test]
    async fn a_remark_of_a_version_off_the_screen_is_not_shown() {
        let (_repo, git, _first, second, found) = two_versions().await;

        // The second version against its parent: the remark of the first
        // speaks of a line that is not on the screen, and Gerrit hides it.
        let (wire, placed) = on_screen(&git, found, &second, None).await;

        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].body, "and this");
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].side, Side::New);
        assert_eq!(placed[0].line, Some(3));
    }

    #[tokio::test]
    async fn a_remark_of_the_version_read_against_stands_on_the_left() {
        let (_repo, git, first, second, found) = two_versions().await;

        // The second version has rewritten line 2. The first is the left
        // column, and line 2 is there, where the remark was posted.
        let (wire, placed) = on_screen(&git, found, &second, Some(&first)).await;

        assert_eq!(wire.len(), 2);
        let left = placed.iter().find(|p| p.id == wire[0].id).unwrap();
        assert_eq!(left.side, Side::Old);
        assert_eq!(left.line, Some(2));
        assert!(!left.lost);
        assert!(!left.moved);

        let right = placed.iter().find(|p| p.id == wire[1].id).unwrap();
        assert_eq!(right.side, Side::New);
        assert_eq!(right.line, Some(3));
    }

    #[tokio::test]
    async fn a_remark_is_anchored_on_the_version_it_was_posted_on() {
        let repo = crate::testutil::build_repo(&[
            crate::testutil::commit("base").file("a.txt", "0\n"),
            crate::testutil::commit("work").file("a.txt", "one\ntwo\nthree\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;

        let found = of_change(
            &git,
            &change(&head, vec![posted("a.txt", Some(2), "Jane", "why two")]),
        )
        .await;

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].wire.author, "Jane");
        assert_eq!(found[0].wire.line, Some(2));

        let anchor = found[0].placeable.anchor.as_ref().expect("it is readable");
        assert_eq!(anchor.line_hash, Some(comments::hash_line("two")));
        assert_eq!(anchor.context, ["one", "two", "three"]);
    }

    #[tokio::test]
    async fn a_version_that_is_not_here_gives_a_remark_with_a_bare_anchor() {
        let repo =
            crate::testutil::build_repo(&[crate::testutil::commit("work").file("a.txt", "1\n")])
                .await;
        let git = Git::discover(repo.path()).await.unwrap();

        let found = of_change(
            &git,
            &change("deadbeef", vec![posted("a.txt", Some(1), "Jane", "hm")]),
        )
        .await;

        // It names the place and proves nothing, so the anchoring calls it
        // unplaced rather than putting it on whatever line 1 holds now.
        let anchor = found[0]
            .placeable
            .anchor
            .as_ref()
            .expect("it names a place");
        assert_eq!(anchor.start_line, Some(1));
        assert_eq!(anchor.line_hash, None);
        assert!(anchor.context.is_empty());
        assert_eq!(found[0].wire.body, "hm");
    }

    #[tokio::test]
    async fn line_zero_and_the_commit_message_are_read_as_the_whole_file() {
        let repo =
            crate::testutil::build_repo(&[crate::testutil::commit("work").file("a.txt", "1\n")])
                .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;

        let found = of_change(
            &git,
            &change(
                &head,
                vec![
                    posted("a.txt", Some(0), "bot", "no test"),
                    // Gerrit puts a header of five lines above the message.
                    // qreview does not, so the numbers cannot be trusted.
                    posted("/COMMIT_MSG", Some(9), "Jane", "say why"),
                ],
            ),
        )
        .await;

        assert_eq!(found[0].wire.line, None);
        assert_eq!(found[0].placeable.scope, Scope::File);
        assert_eq!(found[1].wire.line, None);
        assert_eq!(found[1].placeable.scope, Scope::File);
    }
}
