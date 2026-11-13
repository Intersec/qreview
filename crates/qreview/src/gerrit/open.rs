//! `qreview <gerrit-url>`: the change that an address of Gerrit names.
//!
//! See section 6.9 of the design.

use anyhow::{Context, Result, anyhow, bail};

use super::{Change, Coordinates, PatchSet, TIMEOUT, answer, ask};
use crate::git::commit;
use crate::git::exec::Git;

/// What an address names: a change, and a patch set when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub number: u64,
    pub patch_set: Option<usize>,
}

/// True when the argument is an address and not a revision. A revision
/// never holds `://`.
pub fn is_address(text: &str) -> bool {
    text.starts_with("https://") || text.starts_with("http://")
}

/// Read `…/c/<project>/+/<n>[/<ps>]`, `…/c/<n>` or `…/<n>`.
///
/// What follows the patch set, a file or a range of patch sets, is not
/// part of what to review, and is left out.
pub fn parse(url: &str) -> Option<Target> {
    let rest = url.split_once("://")?.1;
    let rest = rest.split(['?', '#']).next()?;
    let path: Vec<&str> = rest.split('/').skip(1).filter(|p| !p.is_empty()).collect();

    // The newer form: the project, then `+`, then the change.
    let after = match path.iter().position(|part| *part == "+") {
        Some(plus) => &path[plus + 1..],
        // `/c/<n>`, or `/<n>`, the short form a server redirects.
        None => match path.first() {
            Some(&"c") => &path[1..],
            _ => &path[..],
        },
    };
    let number = after.first()?.parse().ok()?;
    let patch_set = after.get(1).and_then(|p| p.parse().ok());

    Some(Target { number, patch_set })
}

/// The change and the patch set to review.
#[derive(Clone, Debug)]
pub struct Found {
    pub change: Change,
    pub patch_set: PatchSet,
}

/// Ask Gerrit about the change, over ssh, and pick the patch set.
///
/// Gerrit is not optional here: without an answer there is nothing to
/// review, so every failure is an error that says what went wrong.
pub async fn find(coords: &Coordinates, target: Target) -> Result<Found> {
    let text = ask(coords, &format!("change:{}", target.number), false, TIMEOUT)
        .await
        .map_err(super::Failed::error)?;

    choose(&answer::parse(&text), coords, target)
}

fn choose(changes: &[Change], coords: &Coordinates, target: Target) -> Result<Found> {
    let change = changes
        .iter()
        .find(|c| c.number == target.number)
        .ok_or_else(|| anyhow!("{} knows no change {}", coords.host, target.number))?;

    // A clone holds the objects of its own project only.
    if change.project != coords.project {
        bail!(
            "change {} belongs to {}, and origin is {}. Run qreview in a clone of {}",
            target.number,
            change.project,
            coords.project,
            change.project
        );
    }

    let patch_set = match target.patch_set {
        Some(number) => change
            .patch_sets
            .iter()
            .find(|set| set.number == number)
            .ok_or_else(|| anyhow!("change {} has no patch set {number}", target.number))?,
        None => change
            .patch_sets
            .iter()
            .max_by_key(|set| set.number)
            .ok_or_else(|| anyhow!("change {} has no patch set", target.number))?,
    };

    Ok(Found {
        change: change.clone(),
        patch_set: patch_set.clone(),
    })
}

/// Fetch the patch set, and find the base of its series.
///
/// The fetch writes `FETCH_HEAD` and objects, and no ref. The base is the
/// merge base with the branch that Gerrit names, rule 6 of section 3.1:
/// `None` when the clone has no such branch, and the series is then the
/// change alone.
pub async fn fetch(git: &Git, found: &Found) -> Result<(String, Option<String>)> {
    git.text(&["fetch", "origin", &found.patch_set.git_ref])
        .await
        .with_context(|| format!("cannot fetch {}", found.patch_set.git_ref))?;
    let head = commit::resolve(git, &found.patch_set.revision).await?;

    let branch = format!("refs/remotes/origin/{}", found.change.branch);
    let base = git
        .text(&["merge-base", &head, &branch])
        .await
        .ok()
        .map(|base| base.trim().to_owned())
        .filter(|base| !base.is_empty());
    Ok((head, base))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(number: u64, patch_set: Option<usize>) -> Option<Target> {
        Some(Target { number, patch_set })
    }

    #[test]
    fn every_form_of_the_address_names_the_change() {
        let base = "https://review.example.com";
        assert_eq!(
            parse(&format!("{base}/c/myproject/+/12321")),
            target(12321, None)
        );
        assert_eq!(
            parse(&format!("{base}/c/myproject/+/12321/3")),
            target(12321, Some(3))
        );
        assert_eq!(parse(&format!("{base}/12321")), target(12321, None));
        assert_eq!(parse(&format!("{base}/c/12321/")), target(12321, None));
        // A project with a slash, a file after the patch set, a fragment.
        assert_eq!(
            parse(&format!("{base}/c/team/myproject/+/12321/2/src/net.c#12")),
            target(12321, Some(2))
        );
        // A range of patch sets is no patch set.
        assert_eq!(
            parse(&format!("{base}/c/myproject/+/12321/1..2")),
            target(12321, None)
        );
    }

    #[test]
    fn an_address_with_no_number_names_nothing() {
        assert_eq!(parse("https://review.example.com/"), None);
        assert_eq!(parse("https://review.example.com/c/myproject"), None);
        assert_eq!(parse("main"), None);
    }

    #[test]
    fn only_an_address_is_an_address() {
        assert!(is_address("https://review.example.com/12321"));
        assert!(!is_address("origin/main"));
        assert!(!is_address("HEAD~2..HEAD"));
    }

    fn coords() -> Coordinates {
        Coordinates {
            host: "review.example.com".to_owned(),
            port: 29418,
            project: "myproject".to_owned(),
            branch: None,
        }
    }

    fn recorded() -> Vec<Change> {
        let path = format!(
            "{}/tests/data/gerrit/one-change.json",
            env!("CARGO_MANIFEST_DIR")
        );
        answer::parse(&std::fs::read_to_string(path).unwrap())
    }

    #[test]
    fn the_newest_patch_set_is_the_default() {
        let found = choose(
            &recorded(),
            &coords(),
            Target {
                number: 12321,
                patch_set: None,
            },
        )
        .unwrap();

        assert_eq!(found.patch_set.number, 3);
        assert_eq!(found.change.branch, "rel-3.0");
    }

    #[test]
    fn a_patch_set_the_address_names_is_the_one() {
        let found = choose(
            &recorded(),
            &coords(),
            Target {
                number: 12321,
                patch_set: Some(2),
            },
        )
        .unwrap();
        assert_eq!(found.patch_set.git_ref, "refs/changes/21/12321/2");

        let missing = choose(
            &recorded(),
            &coords(),
            Target {
                number: 12321,
                patch_set: Some(9),
            },
        );
        assert!(missing.is_err());
    }

    #[test]
    fn a_change_of_another_project_names_both() {
        let other = Coordinates {
            project: "another".to_owned(),
            ..coords()
        };
        let error = choose(
            &recorded(),
            &other,
            Target {
                number: 12321,
                patch_set: None,
            },
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("myproject") && error.contains("another"),
            "{error}"
        );
    }

    #[test]
    fn a_change_the_server_does_not_know_is_an_error() {
        let error = choose(
            &[],
            &coords(),
            Target {
                number: 1,
                patch_set: None,
            },
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("no change 1"), "{error}");
    }
}
