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

/// Said once at the top. A reader that does not count gives every line of
/// an excerpt the same weight, and a remark then lands on the line it fits
/// best rather than on the one it was written on.
const RULE: &str = "Each comment is about the lines marked `>`; the lines around them are\n\
context only.";

/// The review of one change.
pub async fn change(session: &Session, key: &str) -> Result<String> {
    let mut out = String::new();
    let head = header(session, key).await?;

    let _ = writeln!(out, "## Review: {}, {}", place(session).await, head.what());
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "I reviewed this commit and left the comments below. Please address them."
    );
    if !head.comments.is_empty() {
        let _ = writeln!(out, "{RULE}");
    }
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
        // A change whose only remarks belong to a round before this one has
        // nothing to say here, and a heading with nothing under it says less
        // than no heading at all.
        let live = file
            .comments
            .iter()
            .any(|comment| comments::of_version(comment, &summary.commit));
        if live {
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
    let _ = writeln!(out, "{RULE}");

    for (key, _) in &reviewed {
        let head = header(session, key).await?;
        let _ = writeln!(out);
        let _ = writeln!(out, "### {} — {}", head.name(), head.subject);
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
    /// The work that is not committed. It has no sha worth printing: the one
    /// it carries is synthetic, and a session cannot look it up.
    worktree: bool,
}

impl Head {
    /// What the opening line calls the thing being reviewed.
    fn what(&self) -> String {
        match self.worktree {
            true => "the changes that are not committed".to_owned(),
            false => format!("commit {}", self.short),
        }
    }

    /// What names it in the heading of one change of a series.
    fn name(&self) -> &str {
        match self.worktree {
            true => "not committed",
            false => &self.short,
        }
    }
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

    let worktree = session.is_worktree(&commit);

    // Only the remarks of the version under review. The round before this
    // one left remarks the reader has already dealt with, and an agent
    // reading them again would redo work that is done.
    let before = file.comments.len();
    let mut comments: Vec<Comment> = file
        .comments
        .into_iter()
        .filter(|comment| comments::of_version(comment, &commit))
        .collect();
    let earlier = before - comments.len();
    comments::in_reading_order(&mut comments);

    let count = comments.len();
    let mut about = String::new();
    let _ = writeln!(about, "Change: {subject}");
    let _ = writeln!(
        about,
        "{} · {count} comment{}",
        match worktree {
            true => "Not committed yet".to_owned(),
            false => format!("Patch set {patch_set}"),
        },
        if count == 1 { "" } else { "s" }
    );
    if earlier > 0 {
        let _ = writeln!(
            about,
            "{earlier} more {} written on an earlier version, and left out here.",
            if earlier == 1 { "was" } else { "were" }
        );
    }

    Ok(Head {
        short: short(&commit).to_owned(),
        commit,
        subject,
        about,
        comments,
        worktree,
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
        let text = source_of(session, commit, comment).await;
        let lines: Option<Vec<&str>> = text.as_deref().map(|t| t.lines().collect());
        let _ = match &comment.anchor {
            Some(_) => writeln!(
                out,
                "{}. `{}`{}{}",
                index + 1,
                place_of(comment),
                before_the_change(comment),
                cut_of(comment, lines.as_deref())
            ),
            None => writeln!(out, "{}. The change as a whole", index + 1),
        };

        if let Some(excerpt) = excerpt(comment, lines.as_deref()) {
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

/// The text the comment was written on, from the object database.
async fn source_of(session: &Session, commit: &str, comment: &Comment) -> Option<String> {
    let anchor = comment.anchor.as_ref()?;
    anchor.start_line?;
    let rev = match anchor.side {
        Side::New => commit.to_owned(),
        Side::Old => session
            .base_of(commit, &crate::session::Against::Parent)
            .await
            .ok()?,
    };

    match crate::commitmsg::is(&anchor.file) {
        true => crate::commitmsg::text(&session.git, &rev).await,
        false => session
            .git
            .text(&["show", &format!("{rev}:{}", anchor.file)])
            .await
            .ok(),
    }
}

/// The lines around the comment, with their real numbers. The lines the
/// comment covers carry a `>` at the left edge, the others two spaces.
fn excerpt(comment: &Comment, lines: Option<&[&str]>) -> Option<String> {
    let anchor = comment.anchor.as_ref()?;
    let lines = lines?;
    let line = anchor.start_line?;
    let last = anchor.end_line.unwrap_or(line).max(line);

    let from = line.saturating_sub(CONTEXT).max(1);
    let to = (last + CONTEXT).min(lines.len());
    if from > lines.len() {
        return None;
    }

    let width = to.to_string().len();
    let mut out = String::new();
    for number in from..=to {
        let mark = if (line..=last).contains(&number) {
            '>'
        } else {
            ' '
        };
        let _ = writeln!(out, "{mark} {number:>width$} | {}", lines[number - 1]);
    }
    Some(out)
}

/// What the heading adds when the range opens or closes inside a line.
///
/// The text is quoted, not the columns: the reader can act on `for (;;)`,
/// and a column is a count in units it does not know. Bounds that fall on
/// the ends of the lines say nothing more than the lines do.
fn cut_of(comment: &Comment, lines: Option<&[&str]>) -> String {
    let (Some(anchor), Some(lines)) = (&comment.anchor, lines) else {
        return String::new();
    };
    let (Some(start), Some(from), Some(to)) =
        (anchor.start_line, anchor.start_char, anchor.end_char)
    else {
        return String::new();
    };
    let end = anchor.end_line.unwrap_or(start).max(start);
    let (Some(first), Some(last)) = (
        lines.get(start.saturating_sub(1)),
        lines.get(end.saturating_sub(1)),
    ) else {
        return String::new();
    };

    // The offsets count UTF-16 units, the units the browser measured in.
    let first: Vec<u16> = first.encode_utf16().collect();
    let last: Vec<u16> = last.encode_utf16().collect();
    let from = from.min(first.len());
    let to = to.min(last.len());
    if from == 0 && to == last.len() {
        return String::new();
    }

    if start == end {
        return match quote(&first, from, to.max(from)) {
            Some(text) => format!(", on {text}"),
            None => String::new(),
        };
    }
    match (quote(&first, from, first.len()), quote(&last, 0, to)) {
        (Some(head), Some(tail)) => format!(", from {head} to {tail}"),
        (Some(text), None) | (None, Some(text)) => format!(", on {text}"),
        (None, None) => String::new(),
    }
}

/// The text between two offsets of a line, as a Markdown code span, or
/// nothing when it is only spaces.
///
/// A text that occurs more than once on its line says which one it is:
/// `on the second `%d``, because `on `%d`` alone names both.
fn quote(line: &[u16], from: usize, to: usize) -> Option<String> {
    let head = String::from_utf16_lossy(&line[..from]);
    let picked = String::from_utf16_lossy(&line[from..to]);
    let text = picked.trim();
    if text.is_empty() {
        return None;
    }
    let span = code_span(text);

    let full = format!("{head}{picked}{}", String::from_utf16_lossy(&line[to..]));
    let at = head.len() + picked.len() - picked.trim_start().len();
    let starts: Vec<usize> = (0..=full.len())
        .filter(|&i| full.is_char_boundary(i) && full[i..].starts_with(text))
        .collect();
    if starts.len() < 2 {
        return Some(span);
    }
    let before = starts.iter().filter(|&&i| i < at).count();
    Some(format!("the {} {span}", ordinal(before + 1)))
}

/// A Markdown code span around the text.
fn code_span(text: &str) -> String {
    // A backtick inside needs a longer fence, and a space keeps one at an
    // end from joining the fence.
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// `first` to `ninth` in words, then `10th`, `21st`, `112th`.
fn ordinal(n: usize) -> String {
    const WORDS: [&str; 9] = [
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
    ];
    if let Some(word) = WORDS.get(n.wrapping_sub(1)) {
        return (*word).to_owned();
    }
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
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

    fn range_comment(
        file: &str,
        lines: (usize, usize),
        chars: Option<(usize, usize)>,
        body: &str,
    ) -> NewComment {
        NewComment {
            scope: Scope::Range,
            file: Some(file.to_owned()),
            side: Some(Side::New),
            start_line: Some(lines.0),
            end_line: Some(lines.1),
            start_char: chars.map(|c| c.0),
            end_char: chars.map(|c| c.1),
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
        session
            .add_comment(
                "Iretry",
                range_comment(
                    "src/net.blk",
                    (3, 3),
                    Some((4, 12)),
                    "Only the head of the loop.",
                ),
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
                range_comment("a.c", (3, 5), None, "These three lines say one thing."),
            )
            .await
            .unwrap();

        let text = change(&session, "Irange").await.unwrap();

        assert!(text.contains("`a.c:3-5`\n"), "{text}");
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
    async fn a_remark_of_a_round_before_is_left_out_and_counted_apart() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\ntwo\n"),
            commit("work: a change")
                .file("a.c", "one\ntwo\nthree\n")
                .change_id("Iearlier"),
        ])
        .await;
        let session = session_of(&repo).await;

        session
            .add_comment("Iearlier", line_comment("a.c", 3, "About this round."))
            .await
            .unwrap();

        // A remark of the round before, written on a version that is gone.
        let mut file = session.comments("Iearlier", "work: a change").unwrap();
        let mut older = file.comments[0].clone();
        older.id = "c-older".to_owned();
        older.commit = "0000000000000000000000000000000000000000".to_owned();
        older.body = "Dealt with a round ago.".to_owned();
        file.comments.push(older);
        session.store.save(&file).unwrap();

        let text = change(&session, "Iearlier").await.unwrap();

        assert!(text.contains("About this round."), "{text}");
        assert!(
            !text.contains("Dealt with a round ago."),
            "an agent must not redo work that is done:\n{text}"
        );
        assert!(text.contains("Patch set 1 · 1 comment"), "{text}");
        assert!(
            text.contains("1 more was written on an earlier version, and left out here."),
            "the export says what it left out:\n{text}"
        );
    }

    #[tokio::test]
    async fn the_lines_a_comment_covers_are_marked_and_the_context_is_not() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: several lines")
                .file("a.c", "one\ntwo\nthree\nfour\nfive\nsix\nseven\n")
                .change_id("Imarked"),
        ])
        .await;
        let session = session_of(&repo).await;

        session
            .add_comment(
                "Imarked",
                range_comment("a.c", (3, 5), None, "These three."),
            )
            .await
            .unwrap();

        let text = change(&session, "Imarked").await.unwrap();
        for line in [
            "  1 | one",
            "  2 | two",
            "> 3 | three",
            "> 4 | four",
            "> 5 | five",
            "  6 | six",
            "  7 | seven",
        ] {
            assert!(text.contains(line), "{line:?} is missing from\n{text}");
        }
        assert!(text.contains("marked `>`"), "{text}");
    }

    #[tokio::test]
    async fn a_change_with_nothing_to_report_states_no_rule() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        let text = change(&session, "Iretry").await.unwrap();
        assert!(text.contains("Nothing to report."), "{text}");
        assert!(!text.contains("marked `>`"), "{text}");
    }

    #[tokio::test]
    async fn a_comment_on_a_part_of_a_line_quotes_it() {
        let repo = reviewed().await;
        let session = session_of(&repo).await;

        // Line 3 is `    for (;;) {`, and the reader picked the loop head.
        session
            .add_comment(
                "Iretry",
                range_comment("src/net.blk", (3, 3), Some((4, 12)), "Not forever."),
            )
            .await
            .unwrap();

        let text = change(&session, "Iretry").await.unwrap();
        assert!(text.contains("`src/net.blk:3`, on `for (;;)`"), "{text}");
    }

    #[tokio::test]
    async fn a_range_cut_inside_its_lines_quotes_the_two_ends() {
        // The shape of issue #8: the last sentence of a message, which
        // starts inside one line and ends with the next.
        let first = "own object so that the invariant above still holds. Coalescing is only";
        let second = "lost for that one row, within that one commit.";
        let repo = build_repo(&[
            commit("base").file("a.c", "one\n"),
            commit("rows: coalesce them")
                .body(first)
                .body(second)
                .file("a.c", "two\n")
                .change_id("Icut"),
        ])
        .await;
        let session = session_of(&repo).await;

        let start = first.find("Coalescing").unwrap();
        let end = second.encode_utf16().count();
        session
            .add_comment(
                "Icut",
                range_comment("/COMMIT_MSG", (3, 4), Some((start, end)), "Say why."),
            )
            .await
            .unwrap();

        let text = change(&session, "Icut").await.unwrap();
        let expected = format!("`/COMMIT_MSG:3-4`, from `Coalescing is only` to `{second}`");
        assert!(text.contains(&expected), "{text}");
    }

    #[tokio::test]
    async fn a_range_whose_bounds_fall_on_the_line_ends_quotes_nothing() {
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("work: several lines")
                .file("a.c", "one\ntwo\nthree\nfour\nfive\n")
                .change_id("Iwhole"),
        ])
        .await;
        let session = session_of(&repo).await;

        // The mouse measures whole lines as 0 to the length of the last one.
        session
            .add_comment(
                "Iwhole",
                range_comment("a.c", (3, 5), Some((0, "five".len())), "Whole lines."),
            )
            .await
            .unwrap();

        let text = change(&session, "Iwhole").await.unwrap();
        assert!(text.contains("`a.c:3-5`\n"), "{text}");
    }

    #[tokio::test]
    async fn a_text_that_occurs_twice_on_its_line_says_which_one() {
        let line = "    log(\"flushing %d buffered update(s) of row %d\", n, row);";
        let repo = build_repo(&[
            commit("first").file("a.c", "one\n"),
            commit("log: count the updates")
                .file("a.c", &format!("void f(void)\n{{\n{line}\n}}\n"))
                .change_id("Itwice"),
        ])
        .await;
        let session = session_of(&repo).await;

        let at = line.rfind("%d").unwrap();
        session
            .add_comment(
                "Itwice",
                range_comment("a.c", (3, 3), Some((at, at + 2)), "Use %'d here."),
            )
            .await
            .unwrap();

        let text = change(&session, "Itwice").await.unwrap();
        assert!(text.contains("`a.c:3`, on the second `%d`"), "{text}");
    }

    #[test]
    fn a_quote_counts_itself_only_when_the_line_repeats_it() {
        let units = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let twice = "\"flushing %d of row %d\"";
        let (first, second) = (twice.find("%d").unwrap(), twice.rfind("%d").unwrap());

        assert_eq!(
            quote(&units("    for (;;) {"), 4, 12).unwrap(),
            "`for (;;)`"
        );
        assert_eq!(
            quote(&units(twice), first, first + 2).unwrap(),
            "the first `%d`"
        );
        assert_eq!(
            quote(&units(twice), second, second + 2).unwrap(),
            "the second `%d`"
        );
        // The spaces around a pick are not part of it, and not counted.
        assert_eq!(quote(&units("x x"), 1, 3).unwrap(), "the second `x`");
        assert_eq!(quote(&units("a  x  a"), 1, 6).unwrap(), "`x`");
        assert_eq!(quote(&units("   "), 0, 3), None);
    }

    #[test]
    fn a_quote_with_a_backtick_gets_a_longer_fence() {
        assert_eq!(code_span("a `b` c"), "``a `b` c``");
        assert_eq!(code_span("`x"), "`` `x ``");
    }

    #[test]
    fn ordinals_are_words_first_and_numbers_after() {
        let got: Vec<String> = [1, 2, 3, 9, 10, 11, 12, 13, 21, 22, 23, 112]
            .into_iter()
            .map(ordinal)
            .collect();

        assert_eq!(
            got,
            [
                "first", "second", "third", "ninth", "10th", "11th", "12th", "13th", "21st",
                "22nd", "23rd", "112th"
            ]
        );
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
