//! Who wrote each line of a file, as `git blame` says.
//!
//! The blame reads a commit, never the working tree. It adds no option of
//! its own, so the configuration of the user applies, and the answer is the
//! one that `git blame` prints in the terminal.

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use super::commit::is_object_name;
use super::exec::{Git, SLOW};

/// A run of lines that one commit wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub commit: String,
    /// The first line of the run, in the file that was blamed.
    pub start: usize,
    pub count: usize,
    /// Where the first line stood in the commit that wrote it.
    pub origin: usize,
    /// The path of the file in that commit. A later rename changes it.
    pub path: String,
}

/// What a blame found.
#[derive(Debug, Default)]
pub struct Found {
    pub runs: Vec<Run>,
    /// The commits where the blame stopped reading: the limit of a range, or
    /// a root commit.
    pub boundary: HashSet<String>,
}

/// Blame a file as a commit holds it.
///
/// With `since`, the blame reads only the commits above it, and leaves the
/// older lines on the boundary. `compose` takes them further.
///
/// A commit named by its full hash has one blame, so the run asks once.
pub async fn of_file(git: &Git, rev: &str, since: Option<&str>, path: &str) -> Result<Found> {
    let range = since.map(|since| format!("{since}..{rev}"));
    let call = [
        "blame",
        "--incremental",
        range.as_deref().unwrap_or(rev),
        "--",
        path,
    ];
    let out = match is_object_name(rev) && since.is_none_or(is_object_name) {
        true => git.text_of_object_within(&call, SLOW).await?,
        false => git.text(&call).await?,
    };

    Ok(parse(&out))
}

/// Read the `--incremental` format, and give the runs in the order of the
/// lines.
///
/// A group opens on `<commit> <origin> <line> <count>` and closes on
/// `filename`. The facts of a commit come with its first group only. git
/// prints the groups in the order that it finds them. It can also cut the
/// work of one commit into several groups.
pub fn parse(out: &str) -> Found {
    let mut found = Found::default();
    let mut open: Option<Run> = None;

    for line in out.lines() {
        if let Some(path) = line.strip_prefix("filename ") {
            if let Some(mut run) = open.take() {
                run.path = crate::diff::parse::unquote(path);
                found.runs.push(run);
            }
        } else if line == "boundary" {
            if let Some(run) = &open {
                found.boundary.insert(run.commit.clone());
            }
        } else if let Some(run) = header(line) {
            open = Some(run);
        }
    }
    found.runs = merged(found.runs);
    found
}

/// Take the lines that a range left on its boundary to the commits that
/// wrote them.
///
/// `below` holds the blame of the file as each boundary commit holds it,
/// keyed by commit and path. A line that a range leaves on a boundary
/// commit names its place in that commit, and the blame of that commit
/// names who wrote it. The answer is the one of a plain blame.
pub fn compose(recent: Found, below: &HashMap<(String, String), Vec<Run>>) -> Vec<Run> {
    let mut out = Vec::with_capacity(recent.runs.len());

    for run in recent.runs {
        let older = match recent.boundary.contains(&run.commit) {
            true => below.get(&(run.commit.clone(), run.path.clone())),
            false => None,
        };
        let Some(older) = older else {
            out.push(run);
            continue;
        };

        // Where the run stood in the boundary commit.
        let (from, to) = (run.origin, run.origin + run.count);
        let first = older.partition_point(|o| o.start + o.count <= from);
        for o in older[first..].iter().take_while(|o| o.start < to) {
            let (lo, hi) = (from.max(o.start), to.min(o.start + o.count));
            out.push(Run {
                commit: o.commit.clone(),
                start: run.start + (lo - from),
                count: hi - lo,
                origin: o.origin + (lo - o.start),
                path: o.path.clone(),
            });
        }
    }
    merged(out)
}

/// The runs in the order of the lines, each piece that git cut in two put
/// back together.
fn merged(mut found: Vec<Run>) -> Vec<Run> {
    found.sort_by_key(|run| run.start);

    let mut runs: Vec<Run> = Vec::with_capacity(found.len());
    for run in found {
        match runs.last_mut() {
            Some(last) if follows(last, &run) => last.count += run.count,
            _ => runs.push(run),
        }
    }
    runs
}

/// The next run is the same piece of the same commit, cut in two by git.
fn follows(last: &Run, next: &Run) -> bool {
    last.commit == next.commit
        && last.path == next.path
        && last.start + last.count == next.start
        && last.origin + last.count == next.origin
}

/// `<commit> <origin> <line> <count>`, or `None` for any other line.
fn header(line: &str) -> Option<Run> {
    let mut words = line.split(' ');
    let commit = words.next().filter(|word| is_hash(word))?;
    let origin = words.next()?.parse().ok()?;
    let start = words.next()?.parse().ok()?;
    let count = words.next()?.parse().ok()?;
    if words.next().is_some() {
        return None;
    }

    Some(Run {
        commit: commit.to_owned(),
        start,
        count,
        origin,
        path: String::new(),
    })
}

/// A full hash, SHA-1 or SHA-256. A line of facts opens with a word.
fn is_hash(word: &str) -> bool {
    word.len() >= 40 && word.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{build_repo, commit};

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    /// What git prints, groups out of order, the facts on the first group of
    /// each commit only.
    const RECORDED: &str = "\
2222222222222222222222222222222222222222 2 2 1
author Second Author
author-mail <second@example.com>
author-time 1767225660
author-tz +0000
committer Second Author
committer-mail <second@example.com>
committer-time 1767225660
committer-tz +0000
summary second: change a line
previous 1111111111111111111111111111111111111111 a.txt
filename a.txt
1111111111111111111111111111111111111111 3 3 2
author Test Author
author-mail <author@example.com>
author-time 1767225600
author-tz +0000
committer Test Author
committer-mail <author@example.com>
committer-time 1767225600
committer-tz +0000
summary first: start
boundary
filename a.txt
1111111111111111111111111111111111111111 1 1 1
filename a.txt
";

    #[test]
    fn the_groups_come_back_in_the_order_of_the_lines() {
        let runs = parse(RECORDED).runs;

        let starts: Vec<(&str, usize, usize)> = runs
            .iter()
            .map(|run| (run.commit.as_str(), run.start, run.count))
            .collect();
        assert_eq!(starts, [(A, 1, 1), (B, 2, 1), (A, 3, 2)]);
        assert!(runs.iter().all(|run| run.path == "a.txt"));
    }

    /// The root commit stops a blame, and its facts say so once.
    #[test]
    fn a_boundary_names_its_commit() {
        assert_eq!(parse(RECORDED).boundary, HashSet::from([A.to_owned()]));
    }

    /// A line of facts never opens a run, even one whose words are numbers.
    #[test]
    fn only_a_header_opens_a_run() {
        assert!(header("author-time 1767225600").is_none());
        assert!(header("summary 1 2 3").is_none());
        assert!(header(&format!("{A} 1 2")).is_none());
        assert_eq!(header(&format!("{A} 4 7 2")).unwrap().origin, 4);
    }

    /// git cuts the work of one commit into two groups at times. They read
    /// as one run when they follow each other in both files.
    #[test]
    fn two_groups_that_follow_each_other_are_one_run() {
        let out = format!("{A} 1 1 2\nfilename a.txt\n{A} 3 3 1\nfilename a.txt\n");

        assert_eq!(
            parse(&out).runs,
            [Run {
                commit: A.to_owned(),
                start: 1,
                count: 3,
                origin: 1,
                path: "a.txt".to_owned(),
            }]
        );
    }

    /// Two runs of one commit that were apart where it wrote them stay two,
    /// so each line keeps its place in that commit.
    #[test]
    fn lines_that_were_apart_stay_apart() {
        let out = format!("{A} 1 1 1\nfilename a.txt\n{A} 9 2 1\nfilename a.txt\n");

        assert_eq!(parse(&out).runs.len(), 2);
    }

    #[tokio::test]
    async fn each_line_names_the_commit_that_wrote_it() {
        let repo = build_repo(&[
            commit("first: start").file("a.txt", "one\ntwo\nthree\n"),
            commit("second: change a line")
                .file("a.txt", "one\nTWO\nthree\n")
                .author("Second Author", "second@example.com"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let first = repo.sha("HEAD~1").await;
        let second = repo.sha("HEAD").await;

        let runs = of_file(&git, &second, None, "a.txt").await.unwrap().runs;

        let seen: Vec<(&str, usize, usize, usize)> = runs
            .iter()
            .map(|run| (run.commit.as_str(), run.start, run.count, run.origin))
            .collect();
        assert_eq!(
            seen,
            [
                (first.as_str(), 1, 1, 1),
                (second.as_str(), 2, 1, 2),
                (first.as_str(), 3, 1, 3),
            ]
        );
    }

    /// A rename keeps the history of the lines, the way `git blame` does in
    /// the terminal, and each run names the path its commit wrote.
    #[tokio::test]
    async fn a_renamed_file_keeps_its_history() {
        let repo = build_repo(&[
            commit("first: start").file("old.txt", "one\ntwo\n"),
            commit("second: rename")
                .delete("old.txt")
                .file("new.txt", "one\ntwo\nthree\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let head = repo.sha("HEAD").await;

        let runs = of_file(&git, &head, None, "new.txt").await.unwrap().runs;

        assert_eq!(runs[0].path, "old.txt");
        assert_eq!(runs[0].count, 2);
        assert_eq!(runs[1].path, "new.txt");
        assert_eq!(runs[1].start, 3);
    }

    fn run(commit: &str, start: usize, count: usize, origin: usize, path: &str) -> Run {
        Run {
            commit: commit.to_owned(),
            start,
            count,
            origin,
            path: path.to_owned(),
        }
    }

    /// A run that a range left on its boundary takes the commits that the
    /// blame of the boundary names, line for line.
    #[test]
    fn the_boundary_hands_its_lines_to_the_older_blame() {
        const C: &str = "3333333333333333333333333333333333333333";
        let recent = Found {
            runs: vec![run(B, 1, 1, 1, "a.txt"), run(A, 2, 3, 4, "a.txt")],
            boundary: HashSet::from([A.to_owned()]),
        };
        // In A, the lines 4 to 6 of a.txt: C wrote 4, A wrote 5 and 6.
        let below = HashMap::from([(
            (A.to_owned(), "a.txt".to_owned()),
            vec![
                run(A, 1, 3, 1, "a.txt"),
                run(C, 4, 1, 9, "old.txt"),
                run(A, 5, 2, 5, "a.txt"),
            ],
        )]);

        assert_eq!(
            compose(recent, &below),
            [
                run(B, 1, 1, 1, "a.txt"),
                run(C, 2, 1, 9, "old.txt"),
                run(A, 3, 2, 5, "a.txt"),
            ]
        );
    }

    /// The range and the blame under it give what a plain `git blame`
    /// gives, a rename under the range included.
    #[tokio::test]
    async fn a_range_and_the_blame_under_it_are_the_plain_blame() {
        let repo = build_repo(&[
            commit("first: start").file("old.txt", "one\ntwo\nthree\nfour\n"),
            commit("second: rename")
                .delete("old.txt")
                .file("a.txt", "one\nTWO\nthree\nfour\n"),
            commit("third: change a line").file("a.txt", "ONE\nTWO\nthree\nfour\n"),
            commit("fourth: add a line").file("a.txt", "ONE\nTWO\nthree\n3.5\nfour\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let floor = repo.sha("HEAD~2").await;
        let head = repo.sha("HEAD").await;

        let recent = of_file(&git, &head, Some(&floor), "a.txt").await.unwrap();
        assert_eq!(recent.boundary, HashSet::from([floor.clone()]));
        let below = HashMap::from([(
            (floor.clone(), "a.txt".to_owned()),
            of_file(&git, &floor, None, "a.txt").await.unwrap().runs,
        )]);

        assert_eq!(
            compose(recent, &below),
            of_file(&git, &head, None, "a.txt").await.unwrap().runs
        );
    }
}
