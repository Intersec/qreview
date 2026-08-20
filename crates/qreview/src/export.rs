//! The review, as text to paste into a session.
//!
//! The format is a contract with whatever reads it, so a snapshot test pins
//! it. The code comes before the comment: the reader needs the context first.

use std::fmt::Write;

use anyhow::Result;

use crate::session::Session;
use crate::store::model::{Comment, Scope, Side};

/// Lines of code shown around a comment.
const CONTEXT: usize = 2;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Include the threads somebody marked resolved.
    pub all: bool,
}

/// The review of one change.
pub async fn change(session: &Session, key: &str, opts: Options) -> Result<String> {
    let Some(commit) = session.commit_of(key) else {
        anyhow::bail!("no change {key} in the series");
    };
    let subject = session
        .series
        .changes
        .iter()
        .find(|c| c.key == key)
        .map(|c| c.subject.clone())
        .unwrap_or_default();

    let file = session.comments(key, &subject)?;
    let threads = threads_of(&file.comments, opts);

    let mut out = String::new();
    let _ = writeln!(out, "# Review: {subject}");

    let sets = session.patch_sets(key).await.unwrap_or_default();
    let patch_set = sets.last().map(|s| s.number).unwrap_or(1);
    let _ = writeln!(
        out,
        "Commit {} (patch set {patch_set}) · {} comments, {} unresolved",
        short(&commit),
        file.comments.len(),
        file.unresolved()
    );

    if threads.is_empty() {
        let _ = writeln!(out, "\nNothing to report.");
        return Ok(out);
    }

    for (first, replies) in threads {
        let _ = writeln!(out);
        let _ = writeln!(out, "## {}", place_of(first));

        if let Some(excerpt) = excerpt(session, &commit, first).await {
            let _ = writeln!(out, "```{}", language_of(session, first));
            let _ = write!(out, "{excerpt}");
            let _ = writeln!(out, "```");
        }

        let _ = writeln!(out, "**{}** — {}", first.author, one_line(&first.body));
        for reply in replies {
            let _ = writeln!(out, "> **{}** — {}", reply.author, one_line(&reply.body));
        }
    }
    Ok(out)
}

/// The review of every change of the series.
pub async fn series(session: &Session, opts: Options) -> Result<String> {
    let mut out = String::new();

    for summary in &session.series.changes {
        if summary.comment_count == 0 {
            continue;
        }
        if !out.is_empty() {
            let _ = writeln!(out, "\n---\n");
        }
        out.push_str(&change(session, &summary.key, opts).await?);
    }

    if out.is_empty() {
        out.push_str("No comment in this series.\n");
    }
    Ok(out)
}

/// The threads worth exporting, first comment and replies.
fn threads_of(comments: &[Comment], opts: Options) -> Vec<(&Comment, Vec<&Comment>)> {
    comments
        .iter()
        .filter(|c| c.parent_id.is_none())
        .filter(|c| opts.all || !c.resolved)
        .map(|first| {
            let replies = comments
                .iter()
                .filter(|c| c.parent_id.as_deref() == Some(first.id.as_str()))
                .collect();
            (first, replies)
        })
        .collect()
}

fn place_of(comment: &Comment) -> String {
    match (&comment.anchor, comment.scope) {
        (Some(anchor), Scope::File) => format!("{} (the file)", anchor.file),
        (Some(anchor), _) => match anchor.start_line {
            Some(line) => format!("{}:{line}", anchor.file),
            None => anchor.file.clone(),
        },
        (None, _) => "The change".to_owned(),
    }
}

fn language_of(session: &Session, comment: &Comment) -> String {
    comment
        .anchor
        .as_ref()
        .and_then(|anchor| session.langs.of(&anchor.file))
        .unwrap_or("")
        .to_owned()
}

/// The lines around the comment, with their real numbers.
async fn excerpt(session: &Session, commit: &str, comment: &Comment) -> Option<String> {
    let anchor = comment.anchor.as_ref()?;
    let line = anchor.start_line?;
    let rev = match anchor.side {
        Side::New => commit.to_owned(),
        Side::Old => session
            .base_of(commit, &crate::session::Against::Parent)
            .await
            .ok()?,
    };

    let text = session
        .git
        .text(&["show", &format!("{rev}:{}", anchor.file)])
        .await
        .ok()?;
    let lines: Vec<&str> = text.lines().collect();

    let from = line.saturating_sub(CONTEXT).max(1);
    let to = (line + CONTEXT).min(lines.len());
    if from > lines.len() {
        return None;
    }

    let width = to.to_string().len();
    let mut out = String::new();
    for number in from..=to {
        let _ = writeln!(out, "{number:>width$} | {}", lines[number - 1]);
    }
    Some(out)
}

/// A comment body on one line, so the export stays scannable.
fn one_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comments::NewComment;
    use crate::lang::Languages;
    use crate::series::Options as SeriesOptions;
    use crate::store::Store;
    use crate::testutil::{Repo, build_repo, commit};

    async fn session_of(repo: &Repo) -> Session {
        let store = Store::at(repo.path().join(".qreview-test").as_path());
        let opts = SeriesOptions {
            gerrit: false,
            ..SeriesOptions::new()
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

    fn line_comment(file: &str, line: usize, body: &str) -> NewComment {
        NewComment {
            parent_id: None,
            scope: Scope::Line,
            file: Some(file.to_owned()),
            side: Some(Side::New),
            start_line: Some(line),
            end_line: Some(line),
            body: body.to_owned(),
            draft: false,
        }
    }

    async fn reviewed() -> Repo {
        build_repo(&[
            commit("base").file("src/net.blk", "int old(void);\n"),
            commit("net: fix the retry loop").change_id("Iretry").file(
                "src/net.blk",
                "int connect_once(int fd)\n{\n    for (;;) {\n        read(fd);\n    }\n}\n",
            ),
        ])
        .await
    }

    #[tokio::test]
    async fn the_export_of_a_change_is_pinned() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        session
            .add_comment(
                "Iretry",
                line_comment(
                    "src/net.blk",
                    3,
                    "This loop never ends when the socket closes.",
                ),
            )
            .await
            .unwrap();
        session
            .add_comment(
                "Iretry",
                NewComment {
                    parent_id: None,
                    scope: Scope::Change,
                    file: None,
                    side: None,
                    start_line: None,
                    end_line: None,
                    body: "The whole change needs a test.".to_owned(),
                    draft: false,
                },
            )
            .await
            .unwrap();

        let text = change(&session, "Iretry", Options::default())
            .await
            .unwrap();
        // The commit hash changes with the fixture, so it is taken out of
        // the snapshot rather than making the test brittle.
        let stable = text
            .lines()
            .map(|line| {
                if line.starts_with("Commit ") {
                    "Commit <hash> (patch set 1) · 2 comments, 2 unresolved".to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        insta::assert_snapshot!(stable);
    }

    #[tokio::test]
    async fn a_resolved_thread_is_left_out_unless_it_is_asked_for() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        let comment = session
            .add_comment("Iretry", line_comment("src/net.blk", 3, "a remark"))
            .await
            .unwrap();
        session
            .edit_comment(
                "Iretry",
                &comment.id,
                crate::comments::EditComment {
                    resolved: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();

        let quiet = change(&session, "Iretry", Options::default())
            .await
            .unwrap();
        assert!(quiet.contains("Nothing to report"), "{quiet}");

        let full = change(&session, "Iretry", Options { all: true })
            .await
            .unwrap();
        assert!(full.contains("a remark"), "{full}");
    }

    #[tokio::test]
    async fn a_reply_is_quoted_under_the_comment_it_answers() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        let first = session
            .add_comment("Iretry", line_comment("src/net.blk", 3, "why?"))
            .await
            .unwrap();
        session
            .add_comment(
                "Iretry",
                NewComment {
                    parent_id: Some(first.id),
                    scope: Scope::Change,
                    file: None,
                    side: None,
                    start_line: None,
                    end_line: None,
                    body: "because of the timeout".to_owned(),
                    draft: false,
                },
            )
            .await
            .unwrap();

        let text = change(&session, "Iretry", Options::default())
            .await
            .unwrap();
        assert!(
            text.contains("> **Test Author** — because of the timeout"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn a_body_over_several_lines_is_exported_on_one() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        session
            .add_comment(
                "Iretry",
                line_comment("src/net.blk", 3, "one line\n\nand another one"),
            )
            .await
            .unwrap();

        let text = change(&session, "Iretry", Options::default())
            .await
            .unwrap();
        assert!(text.contains("— one line and another one"), "{text}");
    }

    #[tokio::test]
    async fn a_series_with_no_comment_says_so() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        let text = series(&session, Options::default()).await.unwrap();
        assert_eq!(text, "No comment in this series.\n");
    }
}
