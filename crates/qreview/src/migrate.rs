//! A new version, declared, and its threads moved onto it.
//!
//! An agent that amends a commit knows which commit replaced which. It says
//! so, and the threads of the old version follow their lines into the new
//! one. See `roadmap/design.md` section 5.6.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::comments::{self, NewComment};
use crate::git::commit;
use crate::linemap::{self, FileMap};
use crate::session::{Against, Session};
use crate::store::model::{Anchor, Author, Comment, Move, Scope, Side, Version};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub from: String,
    /// `HEAD` when it is missing.
    #[serde(default)]
    pub to: Option<String>,
    /// Threads to close, with a Done reply of the agent.
    #[serde(default)]
    pub done: Vec<String>,
    /// Threads to ask the reader about.
    #[serde(default)]
    pub blocked: Vec<Blocked>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Blocked {
    pub id: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub from: String,
    pub to: String,
    /// The change the threads now belong to.
    pub key: String,
    pub moved: Vec<Moved>,
    /// The ids of the replies written for `done` and `blocked`.
    pub replies: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub id: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    /// The new version changed the line the remark was about.
    pub touched: bool,
}

/// Declare `to` the next version of `from`, and move the threads.
pub async fn run(session: &mut Session, request: Request) -> Result<Outcome> {
    let from = resolve(session, &request.from).await?;
    let to = resolve(session, request.to.as_deref().unwrap_or("HEAD")).await?;
    if from == to {
        bail!("--from and --to name the same commit, {}", short(&from));
    }

    // The agent amended a moment ago. The series must hold the new commit.
    session.refresh().await?;
    let key = session
        .series
        .changes
        .iter()
        .find(|c| c.commit == to)
        .map(|c| c.key.clone())
        .with_context(|| {
            format!(
                "{} is not in the series under review. Check the range qreview was started on",
                short(&to)
            )
        })?;

    let old_info = commit::info(&session.git, &from).await?;
    let new_info = commit::info(&session.git, &to).await?;
    if let (Some(old), Some(new)) = (old_info.change_id(), new_info.change_id())
        && old != new
    {
        bail!(
            "{} carries the Change-Id {old} and {} carries {new}: they are two changes",
            short(&from),
            short(&to)
        );
    }

    let old_version = session.store.version(&from).unwrap_or_default();
    let new_version = session.store.version(&to).unwrap_or_default();
    if let Some(next) = old_version.next_version.as_deref()
        && next != to
    {
        bail!(
            "{} is already the previous version of {}. One commit has one next version",
            short(&from),
            short(next)
        );
    }

    let now = now();
    session.store.save_version(
        &from,
        &Version {
            next_version: Some(to.clone()),
            declared_at: now.clone(),
            ..old_version
        },
    )?;
    session.store.save_version(
        &to,
        &Version {
            previous_version: Some(from.clone()),
            declared_at: now.clone(),
            ..new_version
        },
    )?;
    session.link_versions().await;

    let moved = move_threads(session, &from, &to, &old_info.key(), &key, &now).await?;

    let mut replies = Vec::new();
    for id in &request.done {
        let reply = agent_reply(id, "Done", Some(true), false);
        replies.push(session.add_comment(&key, reply).await?.id);
    }
    for asked in &request.blocked {
        let reply = agent_reply(&asked.id, &asked.body, None, true);
        replies.push(session.add_comment(&key, reply).await?.id);
    }

    Ok(Outcome {
        from,
        to,
        key,
        moved,
        replies,
    })
}

/// Move every thread of `from` to the key of `to`, each remark on its new
/// line, its old place kept in `moves`.
async fn move_threads(
    session: &Session,
    from: &str,
    to: &str,
    old_key: &str,
    new_key: &str,
    now: &str,
) -> Result<Vec<Moved>> {
    let sets = session.patch_sets(new_key).await.unwrap_or_default();
    let patch_set = sets.last().map_or(1, |s| s.number);
    let old_base = session.base_of(from, &Against::Parent).await?;
    let new_base = session.base_of(to, &Against::Parent).await?;

    let mut source = session.store.load(old_key, "")?;
    let remarks: Vec<String> = source
        .comments
        .iter()
        .filter(|c| c.is_remark() && c.commit == from)
        .map(|c| c.id.clone())
        .collect();
    let in_thread = |c: &Comment| {
        remarks.contains(&c.id) || c.parent.as_ref().is_some_and(|p| remarks.contains(p))
    };

    let mut leaving: Vec<Comment> = source
        .comments
        .iter()
        .filter(|c| in_thread(c))
        .cloned()
        .collect();
    let mut moved = Vec::new();
    for comment in leaving.iter_mut().filter(|c| c.is_remark()) {
        let pair = Pair {
            session,
            from,
            to,
            old_base: &old_base,
            new_base: &new_base,
        };
        let (scope, anchor, touched) = pair.place(comment).await;
        comment.moves.push(Move {
            commit: from.to_owned(),
            anchor: comment.anchor.take(),
            moved_at: now.to_owned(),
        });
        comment.commit = to.to_owned();
        comment.patch_set = patch_set;
        comment.scope = scope;
        moved.push(Moved {
            id: comment.id.clone(),
            file: anchor.as_ref().map(|a| a.file.clone()),
            line: anchor.as_ref().and_then(|a| a.start_line),
            touched,
        });
        comment.anchor = anchor;
    }

    // The new key is written first. A failure between the two writes
    // leaves a thread in both places, never in neither.
    if old_key == new_key {
        source.comments.retain(|c| !in_thread(c));
        source.comments.extend(leaving);
        session.store.save(&source)?;
    } else {
        let subject = session
            .series
            .changes
            .iter()
            .find(|c| c.key == new_key)
            .map(|c| c.subject.clone())
            .unwrap_or_default();
        let mut target = session.store.load(new_key, &subject)?;
        target.comments.extend(leaving);
        session.store.save(&target)?;
        source.comments.retain(|c| !in_thread(c));
        session.store.save(&source)?;
    }
    Ok(moved)
}

/// The two versions a remark moves between.
struct Pair<'a> {
    session: &'a Session,
    from: &'a str,
    to: &'a str,
    old_base: &'a str,
    new_base: &'a str,
}

impl Pair<'_> {
    /// The new scope, anchor and touched flag of a remark.
    async fn place(&self, comment: &Comment) -> (Scope, Option<Anchor>, bool) {
        let Some(anchor) = comment.anchor.as_ref() else {
            return (comment.scope, None, false);
        };
        let (old_rev, new_rev) = match anchor.side {
            Side::New => (self.from, self.to),
            Side::Old => (self.old_base, self.new_base),
        };

        let map = self.map_of(old_rev, new_rev, &anchor.file).await;
        let Some(map) = map else {
            // The file is gone. The commit message is the nearest true
            // place left, as section 5.4 says.
            let fallback = self
                .anchor(Scope::File, crate::commitmsg::PATH, Side::New, None, anchor)
                .await;
            return (Scope::File, fallback, true);
        };

        let (lines, touched) = match anchor.start_line {
            Some(start) => {
                let mapped = map.map(start);
                let length = anchor.end_line.unwrap_or(start).saturating_sub(start);
                (Some((mapped.line, mapped.line + length)), mapped.touched)
            }
            None => (None, false),
        };
        let fresh = self
            .anchor(comment.scope, &map.path, anchor.side, lines, anchor)
            .await;
        (comment.scope, fresh, touched)
    }

    async fn map_of(&self, old_rev: &str, new_rev: &str, file: &str) -> Option<FileMap> {
        let git = &self.session.git;
        if crate::commitmsg::is(file) {
            let old = crate::commitmsg::text(git, old_rev)
                .await
                .unwrap_or_default();
            let new = crate::commitmsg::text(git, new_rev)
                .await
                .unwrap_or_default();
            return Some(linemap::of_texts(file, &old, &new));
        }
        // A map that git cannot make leaves the line where it was, rather
        // than losing the remark.
        match linemap::of_file(git, old_rev, new_rev, file).await {
            Ok(map) => map,
            Err(_) => Some(linemap::of_texts(file, "", "")),
        }
    }

    /// An anchor on the new version, read again so the next move can find
    /// it, with the characters as they were written.
    async fn anchor(
        &self,
        scope: Scope,
        file: &str,
        side: Side,
        lines: Option<(usize, usize)>,
        was: &Anchor,
    ) -> Option<Anchor> {
        let new = NewComment {
            scope,
            file: Some(file.to_owned()),
            side: Some(side),
            start_line: lines.map(|l| l.0),
            end_line: lines.map(|l| l.1),
            start_char: lines.and(was.start_char),
            end_char: lines.and(was.end_char),
            body: String::new(),
            author: Author::Agent,
            parent: None,
            done: None,
            blocked: false,
            ask: false,
        };
        comments::anchor_of(&self.session.git, self.to, self.new_base, &new)
            .await
            .ok()
    }
}

fn agent_reply(parent: &str, body: &str, done: Option<bool>, blocked: bool) -> NewComment {
    NewComment {
        scope: Scope::Line,
        file: None,
        side: None,
        start_line: None,
        end_line: None,
        start_char: None,
        end_char: None,
        body: body.to_owned(),
        author: Author::Agent,
        parent: Some(parent.to_owned()),
        done,
        blocked,
        ask: false,
    }
}

async fn resolve(session: &Session, rev: &str) -> Result<String> {
    let found = session
        .git
        .text(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ])
        .await
        .ok()
        .map(|out| out.trim().to_owned())
        .filter(|sha| !sha.is_empty());

    found.with_context(|| format!("{rev} names no commit of this repository"))
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Languages;
    use crate::series::Options;
    use crate::store::Store;
    use crate::testutil::{Repo, build_repo, commit};

    async fn session_of(repo: &Repo) -> Session {
        let store = Store::at(repo.path().join(".qreview-test").as_path());
        let opts = Options {
            gerrit: false,
            worktree: false,
            ..Options::new()
        };
        Session::with(
            repo.path(),
            &opts,
            Languages::new(),
            std::sync::Arc::new(crate::highlight::Highlighter::new()),
            Some(store),
        )
        .await
        .unwrap()
    }

    /// Rewrite a file and amend the last commit, the way an agent fixes it.
    async fn amend(repo: &Repo, file: &str, text: &str, message: &str) -> String {
        std::fs::write(repo.path().join(file), text).unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "-q", "-m", message]).await;
        repo.sha("HEAD").await
    }

    fn remark(file: &str, line: usize, body: &str) -> NewComment {
        NewComment {
            scope: Scope::Line,
            file: Some(file.to_owned()),
            side: Some(Side::New),
            start_line: Some(line),
            end_line: Some(line),
            start_char: None,
            end_char: None,
            body: body.to_owned(),
            author: Author::Reader,
            parent: None,
            done: None,
            blocked: false,
            ask: false,
        }
    }

    fn request(from: &str) -> Request {
        Request {
            from: from.to_owned(),
            to: None,
            done: Vec::new(),
            blocked: Vec::new(),
        }
    }

    async fn two_versions() -> (Repo, Session, String, String) {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: a change").file("a.c", "one\ntwo\nthree\n"),
        ])
        .await;
        let session = session_of(&repo).await;
        let old = repo.sha("HEAD").await;
        let key = format!("sha-{old}");
        session
            .add_comment(&key, remark("a.c", 3, "three is wrong"))
            .await
            .unwrap();
        (repo, session, old, key)
    }

    #[tokio::test]
    async fn a_declared_version_takes_the_threads_with_no_change_id() {
        let (repo, mut session, old, old_key) = two_versions().await;
        let new = amend(&repo, "a.c", "one\nzero\ntwo\nthree\n", "work: a change").await;

        let outcome = run(&mut session, request(&old)).await.unwrap();

        assert_eq!(outcome.key, format!("sha-{new}"));
        assert_eq!(outcome.moved.len(), 1);
        assert_eq!(outcome.moved[0].line, Some(4), "a line added above");
        assert!(!outcome.moved[0].touched);

        let moved = &session.store.load(&outcome.key, "").unwrap().comments[0];
        assert_eq!(moved.commit, new);
        assert_eq!(moved.body, "three is wrong");
        assert_eq!(moved.moves.len(), 1, "the old place is kept");
        assert_eq!(moved.moves[0].commit, old);
        assert_eq!(moved.moves[0].anchor.as_ref().unwrap().start_line, Some(3));
        assert!(
            session
                .store
                .load(&old_key, "")
                .unwrap()
                .comments
                .is_empty(),
            "nothing is left behind"
        );
    }

    #[tokio::test]
    async fn the_link_is_written_on_both_commits_and_makes_a_patch_set() {
        let (repo, mut session, old, _) = two_versions().await;
        let new = amend(&repo, "a.c", "one\ntwo\nTHREE\n", "work: a change").await;

        let outcome = run(&mut session, request(&old)).await.unwrap();

        let back = session.store.version(&new).unwrap();
        let forth = session.store.version(&old).unwrap();
        assert_eq!(back.previous_version.as_deref(), Some(old.as_str()));
        assert_eq!(forth.next_version.as_deref(), Some(new.as_str()));
        assert!(outcome.moved[0].touched, "the line itself changed");

        let sets = session.patch_sets(&outcome.key).await.unwrap();
        let commits: Vec<_> = sets.iter().map(|s| s.commit.as_str()).collect();
        assert!(commits.contains(&old.as_str()), "{commits:?}");
        assert_eq!(sets.last().unwrap().commit, new);
    }

    #[tokio::test]
    async fn done_and_blocked_reply_as_the_agent_on_the_new_version() {
        let (repo, mut session, old, _) = two_versions().await;
        let id = session
            .store
            .load(&format!("sha-{old}"), "")
            .unwrap()
            .comments[0]
            .id
            .clone();
        amend(&repo, "a.c", "one\ntwo\n3\n", "work: a change").await;

        let outcome = run(
            &mut session,
            Request {
                done: vec![id.clone()],
                ..request(&old)
            },
        )
        .await
        .unwrap();

        let file = session.store.load(&outcome.key, "").unwrap();
        let remark = file.comments.iter().find(|c| c.id == id).unwrap();
        let reply = file
            .comments
            .iter()
            .find(|c| c.parent.as_deref() == Some(id.as_str()))
            .unwrap();
        assert!(remark.done);
        assert_eq!(reply.author, Author::Agent);
        assert_eq!(outcome.replies, std::slice::from_ref(&reply.id));
    }

    #[tokio::test]
    async fn a_remark_on_a_deleted_file_goes_to_the_commit_message() {
        let (repo, mut session, old, _) = two_versions().await;
        std::fs::remove_file(repo.path().join("a.c")).unwrap();
        repo.git(&["add", "-A"]).await;
        repo.git(&["commit", "--amend", "-q", "-m", "work: a change"])
            .await;

        let outcome = run(&mut session, request(&old)).await.unwrap();

        assert_eq!(
            outcome.moved[0].file.as_deref(),
            Some(crate::commitmsg::PATH)
        );
        let moved = &session.store.load(&outcome.key, "").unwrap().comments[0];
        assert_eq!(moved.scope, Scope::File);
        assert_eq!(moved.body, "three is wrong", "never dropped");
    }

    #[tokio::test]
    async fn a_second_next_version_is_refused() {
        let (repo, mut session, old, _) = two_versions().await;
        amend(&repo, "a.c", "one\ntwo\n3\n", "work: a change").await;
        run(&mut session, request(&old)).await.unwrap();
        amend(&repo, "a.c", "one\ntwo\n4\n", "work: a change").await;

        let error = run(&mut session, request(&old)).await.unwrap_err();

        assert!(error.to_string().contains("one next version"), "{error}");
    }

    #[tokio::test]
    async fn two_different_change_ids_are_refused() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: a change")
                .file("a.c", "two\n")
                .change_id("I1111111111111111111111111111111111111111"),
        ])
        .await;
        let mut session = session_of(&repo).await;
        let old = repo.sha("HEAD").await;
        amend(
            &repo,
            "a.c",
            "three\n",
            "work: a change\n\nChange-Id: I2222222222222222222222222222222222222222",
        )
        .await;

        let error = run(&mut session, request(&old)).await.unwrap_err();

        assert!(error.to_string().contains("two changes"), "{error}");
        assert_eq!(session.store.version(&old), None, "nothing is written");
    }

    #[tokio::test]
    async fn the_same_change_id_moves_the_threads_in_place() {
        let id = "I3333333333333333333333333333333333333333";
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: a change")
                .file("a.c", "one\ntwo\n")
                .change_id(id),
        ])
        .await;
        let mut session = session_of(&repo).await;
        let old = repo.sha("HEAD").await;
        session
            .add_comment(id, remark("a.c", 2, "two?"))
            .await
            .unwrap();
        let new = amend(
            &repo,
            "a.c",
            "zero\none\ntwo\n",
            &format!("work: a change\n\nChange-Id: {id}"),
        )
        .await;

        let outcome = run(&mut session, request(&old)).await.unwrap();

        assert_eq!(outcome.key, id);
        let file = session.store.load(id, "").unwrap();
        assert_eq!(file.comments.len(), 1);
        assert_eq!(file.comments[0].commit, new);
        assert_eq!(
            file.comments[0].anchor.as_ref().unwrap().start_line,
            Some(3)
        );
    }
}
