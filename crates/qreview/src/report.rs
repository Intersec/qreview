//! The series as text, for the command line.

use std::fmt::Write;

use crate::model::{BoundaryKind, FileEntry, FileStatus, Series};

/// One change and the files it touches.
pub struct ChangeFiles {
    pub key: String,
    pub files: Vec<FileEntry>,
}

/// Render the series the way the command line prints it.
pub fn render(series: &Series, files: &[ChangeFiles]) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "repository  {}", series.repo.root);
    if let Some(remote) = &series.repo.remote {
        let _ = writeln!(out, "remote      {remote}");
    }
    let _ = writeln!(out, "id          {}", series.repo.id);
    let _ = writeln!(out);

    if series.changes.is_empty() {
        let _ = writeln!(out, "No change is loaded.");
    }

    for (index, change) in series.changes.iter().enumerate() {
        let _ = writeln!(
            out,
            "{:>3}. {}  {}",
            index + 1,
            match change.worktree {
                // The sha of the working tree is synthetic. Printing it
                // would invite someone to look it up.
                true => "not committed".to_owned(),
                false => short(&change.commit).to_owned(),
            },
            change.subject
        );
        let _ = writeln!(
            out,
            "     {}  {}",
            change.key,
            if change.change_id.is_some() || change.worktree {
                ""
            } else {
                "(no Change-Id: an amend loses the comments)"
            }
        );

        if let Some(entry) = files.iter().find(|f| f.key == change.key) {
            for file in &entry.files {
                let _ = writeln!(out, "     {}", file_line(file));
            }
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "{}", boundary_line(series));
    out
}

fn file_line(file: &FileEntry) -> String {
    let status = match file.status {
        FileStatus::Added => "A",
        FileStatus::Modified => "M",
        FileStatus::Deleted => "D",
        FileStatus::Renamed => "R",
        FileStatus::Copied => "C",
    };

    let name = match &file.old_path {
        Some(old) => format!("{old} -> {}", file.path),
        None => file.path.clone(),
    };

    if file.binary {
        return format!("{status} {name}  binary");
    }
    format!("{status} {name}  +{} -{}", file.added, file.removed)
}

fn boundary_line(series: &Series) -> String {
    let b = &series.boundary;
    let what = match b.kind {
        BoundaryKind::Merge => "merge",
        BoundaryKind::Tag => "tag",
        BoundaryKind::Base => "base",
        BoundaryKind::Guess => "guess",
        BoundaryKind::Batch => "batch",
        BoundaryKind::Root => "root",
    };

    let mut line = format!("boundary: {what} — {}", b.reason);
    if b.guessed {
        line.push_str(" (a guess: load more to go further)");
    }
    if let Some(merge) = &b.merge {
        let parents: Vec<_> = merge
            .parents
            .iter()
            .map(|p| format!("{} {}", short(&p.commit), p.name))
            .collect();
        line.push_str(&format!(
            "\n          {}\n          {}",
            merge.subject,
            parents.join("  |  ")
        ));
    }
    line
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Languages;
    use crate::series::Options;
    use crate::session::Session;
    use crate::testutil::{build_repo, commit, merge};

    async fn rendered(repo: &crate::testutil::Repo, opts: Options) -> String {
        let session = Session::open(repo.path(), &opts, Languages::new())
            .await
            .unwrap();

        let mut files = Vec::new();
        for change in &session.series.changes {
            files.push(ChangeFiles {
                key: change.key.clone(),
                files: session
                    .files(
                        &change.commit,
                        &crate::session::Against::Parent,
                        &crate::diff::How::default(),
                    )
                    .await
                    .unwrap(),
            });
        }
        render(&session.series, &files)
    }

    #[tokio::test]
    async fn the_report_names_the_changes_and_their_files() {
        let repo = build_repo(&[
            commit("first: start").file("src/a.blk", "int a;\n"),
            commit("second: go on")
                .file("src/a.blk", "int b;\n")
                .file("src/new.c", "int c;\n")
                .change_id("I8f3ac21"),
        ])
        .await;

        let out = rendered(&repo, Options::new()).await;

        assert!(out.contains("second: go on"), "{out}");
        assert!(out.contains("I8f3ac21"), "{out}");
        assert!(out.contains("M src/a.blk  +1 -1"), "{out}");
        assert!(out.contains("A src/new.c  +1 -0"), "{out}");
        assert!(
            out.contains("no Change-Id"),
            "the first commit has none: {out}"
        );
    }

    #[tokio::test]
    async fn the_report_ends_with_the_boundary() {
        let repo = build_repo(&[
            commit("base").file("f", "a\n"),
            commit("side").on_branch("side").file("g", "1\n"),
            commit("main").on_branch("main").file("h", "1\n"),
            merge("Merge side into main").from("side"),
            commit("after").file("i", "1\n"),
        ])
        .await;

        let out = rendered(&repo, Options::new()).await;

        assert!(out.contains("boundary: merge"), "{out}");
        assert!(out.contains("Merge side into main"), "{out}");
    }

    #[tokio::test]
    async fn a_guess_says_that_it_guessed() {
        let commits: Vec<_> = (1..=12)
            .map(|i| commit(&format!("change {i}")).file("a", &format!("{i}\n")))
            .collect();
        let repo = build_repo(&commits).await;

        let out = rendered(&repo, Options::new()).await;

        assert!(out.contains("boundary: guess"), "{out}");
        assert!(out.contains("a guess"), "{out}");
    }

    #[tokio::test]
    async fn an_empty_series_says_so() {
        let repo = build_repo(&[
            commit("base").file("f", "a\n"),
            commit("side").on_branch("side").file("g", "1\n"),
            commit("main").on_branch("main").file("h", "1\n"),
            merge("Merge side into main").from("side"),
        ])
        .await;

        let out = rendered(&repo, Options::new()).await;

        assert!(out.contains("No change is loaded."), "{out}");
        assert!(out.contains("boundary: merge"), "{out}");
    }
}
