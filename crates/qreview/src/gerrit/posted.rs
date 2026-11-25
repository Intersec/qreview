//! The remarks already posted on Gerrit, turned into something placeable.
//!
//! What is already there, on the line it speaks of, beside your own
//! remarks. The REST API gives each remark with its id, its thread, its
//! side and its state (section 6.8 of the design). The ssh answer, when
//! REST gives nothing, holds a file, a line, an author and a text: no id,
//! no side and no reply link (section 6.3).

use super::threads::{self, Range, RestComment};
use crate::anchor::{self, Placed};
use crate::comments::{self, NewComment};
use crate::commitmsg;
use crate::git::commit;
use crate::git::exec::Git;
use crate::model::PostedComment;
use crate::store::model::{Author, Comment, Scope, Side};

/// The file name Gerrit 3 gives to a remark written in the Reply box of a
/// change. Such a remark has no line.
const PATCHSET_LEVEL: &str = "/PATCHSET_LEVEL";

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
/// on the screen. `rev` is the version on the right, and `base` what it is
/// read against. `left` is that version, when it is one: a remark on the
/// parent side then has no column.
pub async fn on_screen(
    git: &Git,
    found: Vec<Posted>,
    rev: &str,
    base: &str,
    left: Option<&str>,
) -> (Vec<PostedComment>, Vec<Placed>) {
    let mut wire = Vec::new();
    let mut right_side = Vec::new();
    let mut left_side = Vec::new();

    for posted in found {
        let on_parent = posted
            .placeable
            .anchor
            .as_ref()
            .is_some_and(|anchor| anchor.side == Side::Old);
        if posted.placeable.commit == rev && !(on_parent && left.is_some()) {
            right_side.push(posted.placeable);
        } else if left == Some(posted.placeable.commit.as_str()) && !on_parent {
            left_side.push(posted.placeable);
        } else {
            continue;
        }
        wire.push(posted.wire);
    }

    // Each remark reads the version it was posted on, whatever column that
    // version stands in.
    let mut placed = anchor::place_all(git, &right_side, rev, base).await;
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
    // The ssh answer carries no id. This one is made from the place, and it
    // is the same on every query as long as the server says the same.
    let id = format!("g{patch_set}-{nth}");
    build(
        git,
        Remark {
            thread: id.clone(),
            id,
            patch_set,
            revision,
            author: posted.reviewer.label(),
            body: &posted.message,
            path: &posted.file,
            line: posted.line,
            range: None,
            on_parent: false,
            in_reply_to: None,
            unresolved: None,
            updated: None,
        },
    )
    .await
}

/// Every remark of a change as the REST API gives it, with its thread and
/// its state. A remark of a patch set that Gerrit did not list is dropped:
/// it has no revision to read.
pub async fn of_rest(git: &Git, change: &super::Change, comments: &[RestComment]) -> Vec<Posted> {
    let roots = threads::roots(comments);
    let mut out = Vec::new();

    for comment in comments {
        let Some(set) = change
            .patch_sets
            .iter()
            .find(|set| set.number == comment.patch_set)
        else {
            continue;
        };
        let author = comment
            .author
            .as_ref()
            .map_or_else(|| "you, a draft".to_owned(), super::Person::label);
        out.push(
            build(
                git,
                Remark {
                    id: comment.id.clone(),
                    thread: roots[&comment.id].clone(),
                    patch_set: set.number,
                    revision: &set.revision,
                    author,
                    body: &comment.message,
                    path: &comment.path,
                    line: comment.line,
                    range: comment.range,
                    on_parent: comment.on_parent(),
                    in_reply_to: comment.in_reply_to.clone(),
                    unresolved: comment.unresolved,
                    updated: Some(comment.updated.clone()),
                },
            )
            .await,
        );
    }
    out
}

/// A remark of Gerrit, in the coordinates of Gerrit.
struct Remark<'a> {
    id: String,
    thread: String,
    patch_set: usize,
    revision: &'a str,
    author: String,
    body: &'a str,
    path: &'a str,
    line: Option<usize>,
    range: Option<Range>,
    on_parent: bool,
    in_reply_to: Option<String>,
    unresolved: Option<bool>,
    updated: Option<String>,
}

async fn build(git: &Git, remark: Remark<'_>) -> Posted {
    // A remark on the left side reads the first parent of the revision.
    let base = match remark.on_parent {
        true => commit::info(git, remark.revision)
            .await
            .ok()
            .and_then(|info| info.parents.first().cloned()),
        false => None,
    };
    let tree = base.as_deref().unwrap_or(remark.revision);

    // No change holds a file of that name, so a remark left there would show
    // nowhere. The message is where qreview puts a remark on the change.
    let (file, lines) = match remark.path {
        PATCHSET_LEVEL => (commitmsg::PATH.to_owned(), None),
        file => (file.to_owned(), lines_of(git, &remark, tree).await),
    };
    let line = lines.map(|l| l.1);
    let wire = PostedComment {
        id: remark.id.clone(),
        thread: remark.thread,
        patch_set: remark.patch_set,
        author: remark.author,
        body: remark.body.to_owned(),
        file: file.clone(),
        line,
        in_reply_to: remark.in_reply_to,
        unresolved: remark.unresolved,
        updated: remark.updated,
    };

    let (start_char, end_char) = match (lines, remark.range) {
        (Some(_), Some(range)) => (Some(range.start_character), Some(range.end_character)),
        _ => (None, None),
    };
    let scope = match lines {
        Some((start, end)) if start != end || start_char.is_some() => Scope::Range,
        Some(_) => Scope::Line,
        None => Scope::File,
    };
    let side = match remark.on_parent {
        true => Side::Old,
        false => Side::New,
    };
    let new = NewComment {
        scope,
        file: Some(file),
        side: Some(side),
        start_line: lines.map(|l| l.0),
        end_line: line,
        start_char,
        end_char,
        body: remark.body.to_owned(),
        author: Author::Reader,
        agent_name: None,
        parent: None,
        done: None,
        blocked: false,
        ask: false,
        gerrit: None,
    };

    // A version that is not in this clone reads as no blob, no hash and no
    // context. That is a bare anchor, and a bare anchor is unplaced.
    let base = base.as_deref().unwrap_or("");
    let anchor = comments::anchor_of(git, remark.revision, base, &new)
        .await
        .ok();

    Posted {
        placeable: Comment {
            id: wire.id.clone(),
            patch_set: remark.patch_set,
            commit: remark.revision.to_owned(),
            created_at: String::new(),
            updated_at: String::new(),
            scope,
            body: remark.body.to_owned(),
            anchor,
            author: Author::Reader,
            agent_name: None,
            parent: None,
            done: false,
            blocked: false,
            moves: Vec::new(),
            gerrit: None,
        },
        wire,
    }
}

/// The first and the last line the remark sits on, if it sits on lines.
///
/// Gerrit writes line 0 for a remark about the whole file. And it shows the
/// message as `/COMMIT_MSG` under a header that qreview drops, so a line of
/// the message is shifted by the length of that header.
async fn lines_of(git: &Git, remark: &Remark<'_>, tree: &str) -> Option<(usize, usize)> {
    let end = remark.line.filter(|line| *line > 0)?;
    let start = remark
        .range
        .map_or(end, |range| range.start_line.clamp(1, end));
    if !commitmsg::is(remark.path) {
        return Some((start, end));
    }

    // A version that is not in this clone has no parents to count.
    let parents = commit::info(git, tree).await.ok()?.parents.len();
    // One line per parent, then the author and its date, the committer and
    // its date, and a blank line. A remark on the header is about the whole
    // message.
    let shift = parents + 5;
    let end = end.checked_sub(shift).filter(|line| *line > 0)?;
    let start = start.saturating_sub(shift).max(1);
    Some((start, end))
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
            topic: String::new(),
            owner: Person::default(),
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
        let (_repo, git, first, second, found) = two_versions().await;

        // The second version against its parent: the remark of the first
        // speaks of a line that is not on the screen, and Gerrit hides it.
        let (wire, placed) = on_screen(&git, found, &second, &first, None).await;

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
        let (wire, placed) = on_screen(&git, found, &second, &first, Some(&first)).await;

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
    async fn line_zero_is_read_as_the_whole_file() {
        let repo =
            crate::testutil::build_repo(&[crate::testutil::commit("work").file("a.txt", "1\n")])
                .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;

        let found = of_change(
            &git,
            &change(&head, vec![posted("a.txt", Some(0), "bot", "no test")]),
        )
        .await;

        assert_eq!(found[0].wire.line, None);
        assert_eq!(found[0].placeable.scope, Scope::File);
    }

    /// A commit whose message reads `subject`, blank, `why one`, `why two`.
    async fn one_message() -> (crate::testutil::Repo, Git, String) {
        let repo = crate::testutil::build_repo(&[
            crate::testutil::commit("base").file("a.txt", "0\n"),
            crate::testutil::commit("subject")
                .body("why one\nwhy two")
                .file("a.txt", "1\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;
        (repo, git, head)
    }

    #[tokio::test]
    async fn a_remark_on_the_message_drops_the_gerrit_header() {
        let (_repo, git, head) = one_message().await;

        // One parent: a header of six lines, so line 9 is line 3.
        let found = of_change(
            &git,
            &change(
                &head,
                vec![posted("/COMMIT_MSG", Some(9), "Jane", "say why")],
            ),
        )
        .await;

        assert_eq!(found[0].wire.line, Some(3));
        assert_eq!(found[0].placeable.scope, Scope::Line);
        let anchor = found[0].placeable.anchor.as_ref().expect("it is readable");
        assert_eq!(anchor.line_hash, Some(comments::hash_line("why one")));
    }

    #[tokio::test]
    async fn a_remark_on_the_gerrit_header_is_about_the_whole_message() {
        let (_repo, git, head) = one_message().await;

        let found = of_change(
            &git,
            &change(&head, vec![posted("/COMMIT_MSG", Some(2), "Jane", "who?")]),
        )
        .await;

        assert_eq!(found[0].wire.line, None);
        assert_eq!(found[0].placeable.scope, Scope::File);
    }

    #[tokio::test]
    async fn the_blank_line_under_the_header_is_about_the_whole_message() {
        let (_repo, git, head) = one_message().await;

        // One parent: line 6 is the blank line that closes the header.
        let found = of_change(
            &git,
            &change(&head, vec![posted("/COMMIT_MSG", Some(6), "Jane", "hm")]),
        )
        .await;

        assert_eq!(found[0].wire.line, None);
        assert_eq!(found[0].placeable.scope, Scope::File);
    }

    #[tokio::test]
    async fn a_remark_on_the_whole_change_stands_on_the_message() {
        let (_repo, git, head) = one_message().await;

        // Gerrit 3 files the Reply box under a path no change holds.
        let path = format!(
            "{}/tests/data/gerrit/patchset-level.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let recorded = crate::gerrit::answer::parse(&std::fs::read_to_string(path).unwrap());
        let comments = recorded[0].patch_sets[0].comments.clone();
        assert_eq!(comments[0].file, "/PATCHSET_LEVEL");

        let found = of_change(&git, &change(&head, comments)).await;

        assert_eq!(found[0].wire.file, commitmsg::PATH);
        assert_eq!(found[0].wire.line, None);
        assert_eq!(found[0].wire.body, "Split this in two changes.");
        assert_eq!(found[0].placeable.scope, Scope::File);
        let anchor = found[0].placeable.anchor.as_ref().expect("it is readable");
        assert_eq!(anchor.file, commitmsg::PATH);

        // The line remark beside it is left where it was.
        assert_eq!(found[1].wire.file, "src/log.c");
        assert_eq!(found[1].wire.line, Some(2));
    }

    #[tokio::test]
    async fn a_remark_on_the_message_of_a_version_not_here_has_no_line() {
        let repo =
            crate::testutil::build_repo(&[crate::testutil::commit("work").file("a.txt", "1\n")])
                .await;
        let git = Git::discover(repo.path()).await.unwrap();

        // Without the commit, the length of the header is unknown.
        let found = of_change(
            &git,
            &change(
                "deadbeef",
                vec![posted("/COMMIT_MSG", Some(9), "Jane", "say why")],
            ),
        )
        .await;

        assert_eq!(found[0].wire.line, None);
        assert_eq!(found[0].placeable.scope, Scope::File);
    }

    #[tokio::test]
    async fn the_header_of_a_merge_has_a_line_per_parent() {
        let repo = crate::testutil::build_repo(&[
            crate::testutil::commit("base").file("a.txt", "0\n"),
            crate::testutil::commit("side")
                .on_branch("side")
                .file("b.txt", "b\n"),
            crate::testutil::merge("merge")
                .on_branch("main")
                .from("side")
                .body("why"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let merge = repo.sha("HEAD").await;

        // Two parents: a header of seven lines, so line 10 is line 3.
        let found = of_change(
            &git,
            &change(&merge, vec![posted("/COMMIT_MSG", Some(10), "Jane", "hm")]),
        )
        .await;

        assert_eq!(found[0].wire.line, Some(3));
    }

    #[tokio::test]
    async fn a_remark_over_rest_keeps_its_thread_its_side_and_its_range() {
        let repo = crate::testutil::build_repo(&[
            crate::testutil::commit("first").file("src/net.c", "a\ncheck\nc\n"),
            crate::testutil::commit("work").file("src/net.c", "a\nb\nint read(x);\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;
        let parent = repo.sha("HEAD~1").await;
        let path = format!(
            "{}/tests/data/gerrit/rest-comments.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(path).unwrap();
        let comments = threads::parse(&serde_json::from_str(&text).unwrap());

        let found = of_rest(&git, &change(&head, Vec::new()), &comments).await;
        let by_id = |id: &str| found.iter().find(|p| p.wire.id == id).unwrap();

        let reply = by_id("a3");
        assert_eq!(reply.wire.thread, "a1");
        assert_eq!(reply.wire.in_reply_to.as_deref(), Some("a2"));
        assert_eq!(reply.wire.unresolved, Some(true));

        let ranged = by_id("a1").placeable.anchor.clone().unwrap();
        assert_eq!((ranged.start_line, ranged.end_line), (Some(3), Some(3)));
        assert_eq!((ranged.start_char, ranged.end_char), (Some(4), Some(8)));
        assert_eq!(by_id("a1").placeable.scope, Scope::Range);

        let old = by_id("p1").placeable.anchor.clone().unwrap();
        assert_eq!(old.side, Side::Old);
        assert!(old.blob.is_some(), "it reads the parent");

        let whole = by_id("w1");
        assert_eq!(whole.wire.file, commitmsg::PATH);
        assert_eq!(whole.wire.line, None);

        // The remark on the parent stands in the left column, on the line
        // that the change removed.
        let (_, placed) = on_screen(&git, found, &head, &parent, None).await;
        let on_parent = placed.iter().find(|p| p.id == "p1").unwrap();
        assert_eq!(on_parent.side, Side::Old);
        assert_eq!(on_parent.line, Some(2));
        assert!(!on_parent.lost);
    }
}
