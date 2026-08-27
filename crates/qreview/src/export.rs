//! The review, as text to paste into a session.
//!
//! The format is a contract with whatever reads it, so a snapshot test pins
//! it. The code comes before the comment: the reader needs the context first.

use std::fmt::Write;

use anyhow::Result;

use crate::comments;
use crate::session::Session;
use crate::store::model::{Comment, Scope, Side};

/// Lines of code shown around a comment.
const CONTEXT: usize = 2;

/// The review of one change.
pub async fn change(session: &Session, key: &str) -> Result<String> {
    let mut out = String::new();
    let head = header(session, key).await?;

    let _ = writeln!(
        out,
        "## Review: {}, commit {}",
        place(session).await,
        head.short
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "I reviewed this commit and left the comments below. Please address them."
    );
    let _ = writeln!(out);
    out.push_str(&head.about);

    if head.comments.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Nothing to report.");
        return Ok(out);
    }

    out.push_str(&body(session, &head.commit, &head.comments).await);

    Ok(out)
}

/// The review of every change of the series.
pub async fn series(session: &Session) -> Result<String> {
    let mut reviewed = Vec::new();

    // The series is walked backwards, newest first. A review reads the other
    // way: the oldest commit is the one the others were built on.
    for summary in session.series.changes.iter().rev() {
        // The store is asked, never a count the session cached: a comment
        // written a moment ago must be in the export.
        let file = session.comments(&summary.key, &summary.subject)?;
        if !file.comments.is_empty() {
            reviewed.push((summary.key.clone(), file));
        }
    }

    if reviewed.is_empty() {
        return Ok("No comment in this series.\n".to_owned());
    }

    if reviewed.len() == 1 {
        return change(session, &reviewed[0].0).await;
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "## Review: {}, {} commits",
        place(session).await,
        reviewed.len()
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "I reviewed this series and left the comments below. Please address them."
    );

    for (key, _) in &reviewed {
        let head = header(session, key).await?;
        let _ = writeln!(out);
        let _ = writeln!(out, "### {} — {}", head.short, head.subject);
        out.push_str(&body(session, &head.commit, &head.comments).await);
    }
    Ok(out)
}

/// `project@branch`, the way a person names where they are.
async fn place(session: &Session) -> String {
    format!("{}@{}", session.project(), session.branch().await)
}

struct Head {
    commit: String,
    short: String,
    subject: String,
    about: String,
    comments: Vec<crate::store::model::Comment>,
}

async fn header(session: &Session, key: &str) -> Result<Head> {
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
    let sets = session.patch_sets(key).await.unwrap_or_default();
    let patch_set = sets.last().map(|s| s.number).unwrap_or(1);
    let count = file.comments.len();

    let mut about = String::new();
    let _ = writeln!(about, "Change: {subject}");
    let _ = writeln!(
        about,
        "Patch set {patch_set} · {count} comment{}",
        if count == 1 { "" } else { "s" }
    );

    let mut comments = file.comments;
    comments::in_reading_order(&mut comments);

    Ok(Head {
        short: short(&commit).to_owned(),
        commit,
        subject,
        about,
        comments,
    })
}

/// The comments of one change, numbered, each under the code it speaks of.
async fn body(
    session: &Session,
    commit: &str,
    comments: &[crate::store::model::Comment],
) -> String {
    let mut out = String::new();

    for (index, comment) in comments.iter().enumerate() {
        let _ = writeln!(out);
        let _ = match &comment.anchor {
            Some(_) => writeln!(
                out,
                "{}. `{}`{}",
                index + 1,
                place_of(comment),
                before_the_change(comment)
            ),
            None => writeln!(out, "{}. The change as a whole", index + 1),
        };

        if let Some(excerpt) = excerpt(session, commit, comment).await {
            let _ = writeln!(out);
            let _ = writeln!(out, "   ```{}", language_of(session, comment));
            for line in excerpt.lines() {
                let _ = writeln!(out, "   {line}");
            }
            let _ = writeln!(out, "   ```");
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "   {}", one_line(&comment.body));
    }
    out
}

fn place_of(comment: &Comment) -> String {
    let (Some(anchor), scope) = (&comment.anchor, comment.scope) else {
        return "The change".to_owned();
    };
    if scope == Scope::File {
        return format!("{} (the file)", anchor.file);
    }
    let Some(start) = anchor.start_line else {
        return anchor.file.clone();
    };

    match anchor.end_line {
        Some(end) if end > start => format!("{}:{start}-{end}", anchor.file),
        _ => format!("{}:{start}", anchor.file),
    }
}

/// What follows the place, when the place is not in the new file.
///
/// A removed line only exists before the change, and the excerpt above it
/// comes from there. Without this a session reads the number against the
/// version it has, and lands on another line.
fn before_the_change(comment: &Comment) -> &'static str {
    let Some(anchor) = &comment.anchor else {
        return "";
    };

    match anchor.side == Side::Old && anchor.start_line.is_some() {
        true => " (before the change)",
        false => "",
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
    let last = anchor.end_line.unwrap_or(line).max(line);
    let rev = match anchor.side {
        Side::New => commit.to_owned(),
        Side::Old => session
            .base_of(commit, &crate::session::Against::Parent)
            .await
            .ok()?,
    };

    let text = match crate::commitmsg::is(&anchor.file) {
        true => crate::commitmsg::text(&session.git, &rev).await?,
        false => session
            .git
            .text(&["show", &format!("{rev}:{}", anchor.file)])
            .await
            .ok()?,
    };
    let lines: Vec<&str> = text.lines().collect();

    let from = line.saturating_sub(CONTEXT).max(1);
    let to = (last + CONTEXT).min(lines.len());
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
            scope: Scope::Line,
            file: Some(file.to_owned()),
            side: Some(Side::New),
            start_line: Some(line),
            end_line: Some(line),
            start_char: None,
            end_char: None,
            body: body.to_owned(),
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
                    scope: Scope::Change,
                    file: None,
                    side: None,
                    start_line: None,
                    end_line: None,
                    start_char: None,
                    end_char: None,
                    body: "The whole change needs a test.".to_owned(),
                },
            )
            .await
            .unwrap();

        let text = change(&session, "Iretry").await.unwrap();
        // The commit hash changes with the fixture, so it is taken out of
        // the snapshot rather than making the test brittle.
        let stable = text
            .lines()
            .map(|line| {
                if line.starts_with("## Review:") {
                    "## Review: <repo>@main, commit <hash>".to_owned()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        insta::assert_snapshot!(stable);
    }

    #[tokio::test]
    async fn the_comments_are_exported_by_file_then_top_to_bottom() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: several files")
                .file("a.c", "one\ntwo\n")
                .file("b.c", "three\n")
                .change_id("Iorder"),
        ])
        .await;
        let session = session_of(&repo).await;

        // Written out of order on purpose: the second file first, and the
        // second line of the first file before its first line. The name
        // orders the files, and the line orders the remarks inside one.
        for (file, line, body) in [
            ("b.c", 1, "About b."),
            ("a.c", 2, "Written first, on line 2 of a."),
            ("a.c", 1, "Written second, on line 1 of a."),
        ] {
            session
                .add_comment("Iorder", line_comment(file, line, body))
                .await
                .unwrap();
        }
        session
            .add_comment(
                "Iorder",
                NewComment {
                    scope: Scope::Change,
                    file: None,
                    side: None,
                    start_line: None,
                    end_line: None,
                    start_char: None,
                    end_char: None,
                    body: "About the change.".to_owned(),
                },
            )
            .await
            .unwrap();

        let text = change(&session, "Iorder").await.unwrap();
        let places: Vec<usize> = [
            "About the change.",
            "Written second, on line 1 of a.",
            "Written first, on line 2 of a.",
            "About b.",
        ]
        .iter()
        .map(|body| {
            text.find(body)
                .unwrap_or_else(|| panic!("{body} is not in\n{text}"))
        })
        .collect();

        assert!(
            places.windows(2).all(|two| two[0] < two[1]),
            "the order is wrong:\n{text}"
        );
    }

    #[tokio::test]
    async fn the_commit_message_comes_before_the_files() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: a change")
                .file("a.c", "one\ntwo\n")
                .change_id("Imsgfirst"),
        ])
        .await;
        let session = session_of(&repo).await;

        session
            .add_comment("Imsgfirst", line_comment("a.c", 1, "About the code."))
            .await
            .unwrap();
        session
            .add_comment(
                "Imsgfirst",
                line_comment("/COMMIT_MSG", 1, "About the subject."),
            )
            .await
            .unwrap();

        let text = change(&session, "Imsgfirst").await.unwrap();
        let message = text.find("About the subject.").unwrap();
        let code = text.find("About the code.").unwrap();

        assert!(message < code, "the message reads first:\n{text}");
    }

    #[tokio::test]
    async fn a_series_reads_from_the_oldest_commit() {
        let repo = build_repo(&[
            commit("base").file("a.txt", "0\n"),
            commit("first: the older one")
                .file("a.txt", "1\n")
                .change_id("Iolder"),
            commit("second: the newer one")
                .file("b.txt", "2\n")
                .change_id("Inewer"),
        ])
        .await;
        let session = session_of(&repo).await;

        for key in ["Iolder", "Inewer"] {
            session
                .add_comment(
                    key,
                    NewComment {
                        scope: Scope::Change,
                        file: None,
                        side: None,
                        start_line: None,
                        end_line: None,
                        start_char: None,
                        end_char: None,
                        body: format!("A remark on {key}."),
                    },
                )
                .await
                .unwrap();
        }

        let text = series(&session).await.unwrap();
        let older = text.find("first: the older one").unwrap();
        let newer = text.find("second: the newer one").unwrap();

        assert!(
            older < newer,
            "the series is walked backwards, the review is not:\n{text}"
        );
    }

    #[tokio::test]
    async fn a_comment_on_a_range_names_both_ends_and_shows_them() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: several lines")
                .file("a.c", "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n")
                .change_id("Irange"),
        ])
        .await;
        let session = session_of(&repo).await;

        session
            .add_comment(
                "Irange",
                NewComment {
                    scope: Scope::Range,
                    file: Some("a.c".to_owned()),
                    side: Some(Side::New),
                    start_line: Some(3),
                    end_line: Some(5),
                    start_char: Some(1),
                    end_char: Some(3),
                    body: "These three lines say one thing.".to_owned(),
                },
            )
            .await
            .unwrap();

        let text = change(&session, "Irange").await.unwrap();

        assert!(text.contains("`a.c:3-5`"), "{text}");
        for line in ["3 | three", "4 | four", "5 | five"] {
            assert!(text.contains(line), "{line} is missing from\n{text}");
        }
    }

    #[tokio::test]
    async fn a_comment_on_a_removed_line_says_it_is_before_the_change() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\ntwo\nthree\n"),
            commit("work: drop a line")
                .file("a.c", "one\nthree\n")
                .change_id("Iremoved"),
        ])
        .await;
        let session = session_of(&repo).await;

        session
            .add_comment(
                "Iremoved",
                NewComment {
                    scope: Scope::Line,
                    file: Some("a.c".to_owned()),
                    side: Some(Side::Old),
                    start_line: Some(2),
                    end_line: Some(2),
                    start_char: None,
                    end_char: None,
                    body: "This line was doing something.".to_owned(),
                },
            )
            .await
            .unwrap();

        let text = change(&session, "Iremoved").await.unwrap();

        // Line 2 of the new file is `three`. Without the mark, a session
        // would read the remark against that line.
        assert!(text.contains("`a.c:2` (before the change)"), "{text}");
        assert!(text.contains("2 | two"), "{text}");
    }

    #[tokio::test]
    async fn a_comment_on_the_commit_message_carries_the_message() {
        let repo = build_repo(&[
            commit("first").file("a.md", "one\n"),
            commit("docs: rename the document")
                .file("b.md", "two\n")
                .change_id("Iexportmsg"),
        ])
        .await;
        let session = session_of(&repo).await;

        session
            .add_comment(
                "Iexportmsg",
                line_comment("/COMMIT_MSG", 1, "The subject says what, not why."),
            )
            .await
            .unwrap();

        let text = change(&session, "Iexportmsg").await.unwrap();

        assert!(text.contains("`/COMMIT_MSG:1`"), "{text}");
        assert!(text.contains("1 | docs: rename the document"), "{text}");
        assert!(text.contains("The subject says what, not why."), "{text}");
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

        let text = change(&session, "Iretry").await.unwrap();
        assert!(text.contains("one line and another one"), "{text}");
    }

    #[tokio::test]
    async fn a_series_of_several_changes_names_each_one() {
        let repo = build_repo(&[
            commit("base").file("a.txt", "0\n"),
            commit("first: a thing")
                .file("a.txt", "1\n")
                .change_id("Ione"),
            commit("second: another")
                .file("b.txt", "2\n")
                .change_id("Itwo"),
        ])
        .await;
        let session = session_of(&repo).await;

        for key in ["Ione", "Itwo"] {
            session
                .add_comment(
                    key,
                    NewComment {
                        scope: Scope::Change,
                        file: None,
                        side: None,
                        start_line: None,
                        end_line: None,
                        start_char: None,
                        end_char: None,
                        body: format!("A remark on {key}."),
                    },
                )
                .await
                .unwrap();
        }

        let text = series(&session).await.unwrap();

        assert!(text.contains("2 commits"), "{text}");
        assert!(text.contains("I reviewed this series"), "{text}");
        assert!(text.contains("— second: another"), "{text}");
        assert!(text.contains("— first: a thing"), "{text}");
        assert!(text.contains("A remark on Ione."), "{text}");
    }

    #[tokio::test]
    async fn a_series_with_one_reviewed_change_reads_as_that_change() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        session
            .add_comment(
                "Iretry",
                line_comment("src/net.blk", 3, "This loop never ends."),
            )
            .await
            .unwrap();

        let text = series(&session).await.unwrap();
        assert!(text.contains("commit "), "{text}");
        assert!(
            !text.contains("commits"),
            "one change is not a series: {text}"
        );
    }

    #[tokio::test]
    async fn a_series_with_no_comment_says_so() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        let text = series(&session).await.unwrap();
        assert_eq!(text, "No comment in this series.\n");
    }
}
