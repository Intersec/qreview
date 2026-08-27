//! The remarks already posted on Gerrit, turned into something placeable.
//!
//! Read only, always. qreview never writes to the server: it votes on
//! nothing, posts nothing and replies to nothing. What it does is show what
//! is already there, on the line it speaks of, beside your own remarks.
//!
//! What the ssh answer gives, and what it does not, is in `roadmap/design.md`
//! section 6.4. The short of it: a file, a line, an author and a text. No id,
//! no side and no reply link.

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
        Change {
            project: "myproject".to_owned(),
            branch: "main".to_owned(),
            id: "Iwork".to_owned(),
            number: 1,
            subject: "work".to_owned(),
            url: String::new(),
            status: "NEW".to_owned(),
            patch_sets: vec![PatchSet {
                number: 1,
                revision: revision.to_owned(),
                git_ref: "refs/changes/01/1/1".to_owned(),
                created_on: 0,
                kind: "REWORK".to_owned(),
                comments,
            }],
        }
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
