//! The series: which commits are under review, and where the walk stops.
//!
//! A series is not resolved once. It is a walk backwards from a head, loaded
//! in batches, that stops at a boundary and says which one. See
//! `roadmap/design.md` section 3.1.

mod refs;

use anyhow::{Result, bail};

use crate::git::commit::{self, CommitInfo};
use crate::git::exec::Git;
use crate::model::{Boundary, BoundaryKind, ChangeSummary, MergeInfo, ParentInfo};

pub use refs::{is_on_a_remote, name_of, tags_at};

/// How the caller asked for the series.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// `--base <rev>`.
    pub base: Option<String>,
    /// A revision, or a `revA..revB` range.
    pub rev: Option<String>,
    /// A resolved base longer than this is treated as a wrong base.
    pub max_commits: usize,
    /// The cap on the guess.
    pub guess_max: usize,
    /// The size of every batch after the first.
    pub batch_size: usize,
    /// The name of the integration branch, when the configuration names one.
    pub integration_branch: Option<String>,
    /// Commits named with `--prev`, treated as older patch sets.
    pub prevs: Vec<String>,
}

impl Options {
    pub fn new() -> Self {
        Self {
            base: None,
            rev: None,
            max_commits: 50,
            guess_max: 10,
            batch_size: 5,
            integration_branch: None,
            prevs: Vec::new(),
        }
    }
}

/// One batch of the walk.
#[derive(Clone, Debug)]
pub struct Batch {
    pub changes: Vec<ChangeSummary>,
    pub boundary: Boundary,
}

/// What the walk knows before it starts.
#[derive(Clone, Debug)]
pub struct Plan {
    /// The newest commit of the series.
    pub head: String,
    /// The base the rules found, and the rule that found it.
    pub base: Option<(String, &'static str)>,
    /// True when no rule found a base, so the walk guesses.
    pub guessing: bool,
    /// How many commits the first batch may load.
    pub limit: usize,
}

/// Work out the head and the base, by the rules of `design.md` section 3.1.
pub async fn plan(git: &Git, opts: &Options) -> Result<Plan> {
    // Rule 2: a range argument names both ends.
    if let Some(rev) = &opts.rev
        && let Some((from, to)) = rev.split_once("..")
    {
        let from = from.trim();
        let to = if to.trim().is_empty() {
            "HEAD"
        } else {
            to.trim()
        };
        if from.is_empty() {
            bail!("{rev} names no base");
        }
        return Ok(Plan {
            head: commit::resolve(git, to).await?,
            base: Some((commit::resolve(git, from).await?, "the range argument")),
            guessing: false,
            limit: opts.max_commits,
        });
    }

    // Rule 0: any revision is a valid head, not only HEAD.
    let head = commit::resolve(git, opts.rev.as_deref().unwrap_or("HEAD")).await?;

    // Rule 1: --base wins over everything.
    if let Some(base) = &opts.base {
        return Ok(Plan {
            head,
            base: Some((commit::resolve(git, base).await?, "--base")),
            guessing: false,
            limit: opts.max_commits,
        });
    }

    // Rule 3: a single revision is that commit alone.
    if opts.rev.is_some() {
        let base = commit::resolve(git, &format!("{head}^")).await.ok();
        return Ok(Plan {
            head,
            base: base.map(|b| (b, "the revision argument")),
            guessing: false,
            limit: 1,
        });
    }

    // Rules 4 and 5, then rule 6.
    let found = match upstream_base(git, &head).await {
        Some(base) => Some((base, "the upstream of the branch")),
        None => integration_base(git, opts, &head)
            .await
            .map(|base| (base, "the merge base with the integration branch")),
    };

    match found {
        Some((base, rule)) if fits(git, &base, &head, opts.max_commits).await => Ok(Plan {
            head,
            base: Some((base, rule)),
            guessing: false,
            limit: opts.max_commits,
        }),
        // A base that gives 200 commits is a wrong base. Guess instead.
        _ => Ok(Plan {
            head,
            base: None,
            guessing: true,
            limit: opts.guess_max,
        }),
    }
}

/// Rule 4: the upstream of the current branch.
async fn upstream_base(git: &Git, head: &str) -> Option<String> {
    let upstream = commit::resolve(git, "@{upstream}").await.ok()?;
    merge_base(git, &upstream, head).await
}

/// Rule 5: the merge base with the integration branch.
///
/// The name comes from the `.gerrit-branch` file of the reviewed commit, then
/// from the configuration, then from `origin/HEAD`.
async fn integration_base(git: &Git, opts: &Options, head: &str) -> Option<String> {
    let mut names = Vec::new();

    if let Some(branch) = commit::gerrit_branch(git, head).await {
        names.push(format!("origin/{branch}"));
        names.push(branch);
    }
    if let Some(branch) = &opts.integration_branch {
        names.push(format!("origin/{branch}"));
        names.push(branch.clone());
    }
    names.push("origin/HEAD".to_owned());

    for name in names {
        if let Ok(other) = commit::resolve(git, &name).await
            && other != head
            && let Some(base) = merge_base(git, &other, head).await
        {
            return Some(base);
        }
    }
    None
}

async fn merge_base(git: &Git, a: &str, b: &str) -> Option<String> {
    let out = git.text(&["merge-base", a, b]).await.ok()?;
    let base = out.trim().to_owned();
    (!base.is_empty()).then_some(base)
}

/// Is the range short enough to be a real series?
async fn fits(git: &Git, base: &str, head: &str, max: usize) -> bool {
    let range = format!("{base}..{head}");
    let Ok(out) = git
        .text(&["rev-list", "--count", "--first-parent", &range])
        .await
    else {
        return false;
    };
    out.trim()
        .parse::<usize>()
        .map(|n| n <= max)
        .unwrap_or(false)
}

/// Walk backwards from `start`, following the first parent, and stop at the
/// first boundary.
///
/// `start` is the newest commit that is not loaded yet. The walk never
/// crosses a merge: a merge becomes the boundary, and the reader decides.
pub async fn walk(
    git: &Git,
    plan: &Plan,
    start: &str,
    limit: usize,
    me: Option<&str>,
) -> Result<Batch> {
    let base = plan.base.as_ref().map(|(b, _)| b.as_str());
    let mut changes = Vec::new();
    let mut current = Some(start.to_owned());

    while let Some(hash) = current {
        // The base is the end of the series, whatever else the commit is.
        if Some(hash.as_str()) == base {
            let rule = plan.base.as_ref().map(|(_, r)| *r).unwrap_or("the base");
            return Ok(done(changes, BoundaryKind::Base, &hash, rule, false, None));
        }

        if changes.len() == limit {
            let kind = if plan.guessing {
                BoundaryKind::Guess
            } else {
                BoundaryKind::Batch
            };
            let reason = if plan.guessing {
                format!("the guess stopped at its cap of {limit}")
            } else {
                format!("{limit} commits loaded")
            };
            return Ok(done(changes, kind, &hash, &reason, plan.guessing, None));
        }

        let info = commit::info(git, &hash).await?;

        // A merge is the boundary, not a change in the list. The reader can
        // review it from the card, or continue on the first parent.
        if info.is_merge() {
            let merge = merge_info(git, &info).await;
            let reason = format!("the merge {}", short(&hash));
            return Ok(done(
                changes,
                BoundaryKind::Merge,
                &hash,
                &reason,
                false,
                merge,
            ));
        }

        // The head itself may carry a tag. Only a tag under the series ends it.
        if !changes.is_empty()
            && let Some(tag) = tags_at(git, &hash).await.first()
        {
            let reason = format!("the tag {tag}");
            return Ok(done(
                changes,
                BoundaryKind::Tag,
                &hash,
                &reason,
                false,
                None,
            ));
        }

        // Two signals that only ever end a guess. Both are wrong often
        // enough to make a bad boundary: a pushed commit can be under
        // review, and a colleague's commit can sit inside a series.
        if plan.guessing && !changes.is_empty() {
            if let Some(name) = is_on_a_remote(git, &hash).await {
                let reason = format!("on {name}");
                return Ok(done(
                    changes,
                    BoundaryKind::Guess,
                    &hash,
                    &reason,
                    true,
                    None,
                ));
            }
            if let Some(me) = me
                && !me.is_empty()
                && info.email != me
            {
                let reason = format!("written by {}", info.author);
                return Ok(done(
                    changes,
                    BoundaryKind::Guess,
                    &hash,
                    &reason,
                    true,
                    None,
                ));
            }
        }

        current = info.parents.first().cloned();
        changes.push(summary(info));
    }

    Ok(Batch {
        changes,
        boundary: Boundary {
            kind: BoundaryKind::Root,
            commit: None,
            reason: "the history has no parent left".to_owned(),
            guessed: false,
            merge: None,
        },
    })
}

fn done(
    changes: Vec<ChangeSummary>,
    kind: BoundaryKind,
    commit: &str,
    reason: &str,
    guessed: bool,
    merge: Option<MergeInfo>,
) -> Batch {
    Batch {
        changes,
        boundary: Boundary {
            kind,
            commit: Some(commit.to_owned()),
            reason: reason.to_owned(),
            guessed,
            merge,
        },
    }
}

fn summary(info: CommitInfo) -> ChangeSummary {
    ChangeSummary {
        key: info.key(),
        change_id: info.change_id().map(str::to_owned),
        subject: info.subject.clone(),
        author: info.author.clone(),
        commit: info.hash.clone(),
        // The store fills these in. The walk knows nothing about comments.
        patch_set_count: 1,
        comment_count: 0,
        unresolved_count: 0,
        is_merge: info.is_merge(),
    }
}

async fn merge_info(git: &Git, info: &CommitInfo) -> Option<MergeInfo> {
    let mut parents = Vec::new();

    for parent in &info.parents {
        parents.push(ParentInfo {
            commit: parent.clone(),
            name: name_of(git, parent).await,
            remote: is_on_a_remote(git, parent).await.is_some(),
        });
    }
    Some(MergeInfo {
        subject: info.subject.clone(),
        parents,
    })
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

/// The first batch: work out the plan, then walk it.
pub async fn first_batch(git: &Git, opts: &Options) -> Result<(Plan, Batch)> {
    let plan = plan(git, opts).await?;
    let me = my_email(git).await;
    let batch = walk(git, &plan, &plan.head.clone(), plan.limit, me.as_deref()).await?;

    Ok((plan, batch))
}

/// The next batch, from the commit the last boundary named.
pub async fn extend(git: &Git, plan: &Plan, from: &str, count: usize) -> Result<Batch> {
    let me = my_email(git).await;

    walk(git, plan, from, count, me.as_deref()).await
}

/// The email git commits with, used by the guess to tell your work apart.
pub async fn my_email(git: &Git) -> Option<String> {
    let out = git.text(&["config", "--get", "user.email"]).await.ok()?;
    let email = out.trim().to_owned();
    (!email.is_empty()).then_some(email)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{Repo, build_repo, commit, merge};

    fn opts() -> Options {
        Options::new()
    }

    /// A straight line of `n` commits, oldest first, all mine.
    async fn line(n: usize) -> Repo {
        let commits: Vec<_> = (1..=n)
            .map(|i| commit(&format!("change {i}")).file("a.txt", &format!("{i}\n")))
            .collect();

        build_repo(&commits).await
    }

    async fn subjects(batch: &Batch) -> Vec<String> {
        batch.changes.iter().map(|c| c.subject.clone()).collect()
    }

    #[tokio::test]
    async fn rule_1_base_wins_over_everything() {
        let repo = line(4).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let mut o = opts();
        o.base = Some("HEAD~2".to_owned());
        let (plan, batch) = first_batch(&git, &o).await.unwrap();

        assert_eq!(plan.base.as_ref().unwrap().1, "--base");
        assert_eq!(subjects(&batch).await, ["change 4", "change 3"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Base);
    }

    #[tokio::test]
    async fn rule_2_a_range_names_both_ends() {
        let repo = line(5).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let mut o = opts();
        o.rev = Some("HEAD~3..HEAD~1".to_owned());
        let (_, batch) = first_batch(&git, &o).await.unwrap();

        assert_eq!(subjects(&batch).await, ["change 4", "change 3"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Base);
    }

    #[tokio::test]
    async fn rule_3_a_single_revision_is_that_commit_alone() {
        let repo = line(5).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let mut o = opts();
        o.rev = Some("HEAD~1".to_owned());
        let (_, batch) = first_batch(&git, &o).await.unwrap();

        assert_eq!(subjects(&batch).await, ["change 4"]);
    }

    #[tokio::test]
    async fn rule_4_the_upstream_of_the_branch() {
        let repo = line(4).await;
        repo.remote("origin", "ssh://review.example.com:29418/myproject")
            .await;
        repo.track("main", "origin", "HEAD~2").await;

        let git = Git::discover(repo.path()).await.unwrap();
        let (plan, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(plan.base.as_ref().unwrap().1, "the upstream of the branch");
        assert_eq!(subjects(&batch).await, ["change 4", "change 3"]);
    }

    #[tokio::test]
    async fn rule_5_the_gerrit_branch_of_the_reviewed_commit() {
        let repo = build_repo(&[
            commit("old work").file("a.txt", "1\n"),
            commit("release point").file("a.txt", "2\n"),
            commit("mine one")
                .file(".gerrit-branch", "rel-3.0\n")
                .file("a.txt", "3\n"),
            commit("mine two").file("a.txt", "4\n"),
        ])
        .await;
        let base = repo.sha("HEAD~2").await;
        repo.git(&["update-ref", "refs/remotes/origin/rel-3.0", &base])
            .await;

        let git = Git::discover(repo.path()).await.unwrap();
        let (plan, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(
            plan.base.as_ref().unwrap().1,
            "the merge base with the integration branch"
        );
        assert_eq!(subjects(&batch).await, ["mine two", "mine one"]);
    }

    #[tokio::test]
    async fn rule_6_no_base_means_a_guess() {
        let repo = line(3).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (plan, batch) = first_batch(&git, &opts()).await.unwrap();

        assert!(plan.guessing);
        assert_eq!(batch.changes.len(), 3);
        assert_eq!(batch.boundary.kind, BoundaryKind::Root);
    }

    #[tokio::test]
    async fn a_resolved_base_that_is_too_long_falls_back_to_the_guess() {
        let repo = line(12).await;
        repo.remote("origin", "ssh://review.example.com:29418/myproject")
            .await;
        repo.track("main", "origin", "HEAD~11").await;

        let git = Git::discover(repo.path()).await.unwrap();
        let mut o = opts();
        o.max_commits = 5;
        let (plan, batch) = first_batch(&git, &o).await.unwrap();

        assert!(
            plan.guessing,
            "a base of 11 commits with a cap of 5 is a wrong base"
        );
        assert_eq!(batch.changes.len(), 10, "the guess loads its cap");
    }

    #[tokio::test]
    async fn the_guess_stops_at_its_cap() {
        let repo = line(14).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(batch.changes.len(), 10);
        assert_eq!(batch.boundary.kind, BoundaryKind::Guess);
        assert!(batch.boundary.guessed);
        assert!(
            batch.boundary.reason.contains("cap of 10"),
            "{}",
            batch.boundary.reason
        );
    }

    #[tokio::test]
    async fn the_guess_stops_at_a_commit_that_is_on_a_remote() {
        let repo = line(6).await;
        let pushed = repo.sha("HEAD~2").await;
        repo.git(&["update-ref", "refs/remotes/other/main", &pushed])
            .await;

        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(subjects(&batch).await, ["change 6", "change 5"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Guess);
        assert!(
            batch.boundary.reason.contains("other/main"),
            "{}",
            batch.boundary.reason
        );
    }

    #[tokio::test]
    async fn the_guess_stops_at_a_commit_of_somebody_else() {
        let repo = build_repo(&[
            commit("theirs")
                .file("a", "1\n")
                .author("Other Person", "other@example.com"),
            commit("mine one").file("a", "2\n"),
            commit("mine two").file("a", "3\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine two", "mine one"]);
        assert!(
            batch.boundary.reason.contains("Other Person"),
            "{}",
            batch.boundary.reason
        );
    }

    #[tokio::test]
    async fn neither_signal_ends_a_batch_once_a_base_is_known() {
        let repo = build_repo(&[
            commit("theirs")
                .file("a", "1\n")
                .author("Other Person", "other@example.com"),
            commit("mine one").file("a", "2\n"),
            commit("mine two").file("a", "3\n"),
        ])
        .await;
        let pushed = repo.sha("HEAD~1").await;
        repo.git(&["update-ref", "refs/remotes/origin/main", &pushed])
            .await;

        let git = Git::discover(repo.path()).await.unwrap();
        let mut o = opts();
        o.base = Some("HEAD~2".to_owned());
        let (_, batch) = first_batch(&git, &o).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine two", "mine one"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Base);
    }

    #[tokio::test]
    async fn the_walk_stops_at_a_merge_and_never_crosses_it() {
        let repo = build_repo(&[
            commit("base").file("f", "a\nb\nc\n"),
            commit("side work")
                .on_branch("side")
                .file("f", "a\nB2\nc\n"),
            commit("main work")
                .on_branch("main")
                .file("f", "a\nB1\nc\n"),
            merge("Merge side into main")
                .from("side")
                .file("f", "a\nR\nc\n"),
            commit("after the merge").file("g", "1\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(subjects(&batch).await, ["after the merge"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Merge);

        let merge = batch
            .boundary
            .merge
            .as_ref()
            .expect("a merge card carries the parents");
        assert_eq!(merge.subject, "Merge side into main");
        assert_eq!(merge.parents.len(), 2);
        assert!(merge.parents.iter().all(|p| !p.remote));
    }

    #[tokio::test]
    async fn a_head_that_is_a_merge_gives_an_empty_batch_and_the_card() {
        let repo = build_repo(&[
            commit("base").file("f", "a\n"),
            commit("side work").on_branch("side").file("g", "1\n"),
            commit("main work").on_branch("main").file("h", "1\n"),
            merge("Merge side into main").from("side"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert!(batch.changes.is_empty());
        assert_eq!(batch.boundary.kind, BoundaryKind::Merge);
    }

    #[tokio::test]
    async fn a_tag_under_the_series_ends_the_batch() {
        let repo = build_repo(&[
            commit("older").file("a", "1\n"),
            commit("the release").file("a", "2\n").tag("v1.0"),
            commit("mine").file("a", "3\n"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Tag);
        assert!(
            batch.boundary.reason.contains("v1.0"),
            "{}",
            batch.boundary.reason
        );
    }

    #[tokio::test]
    async fn a_tag_on_the_head_does_not_stop_the_walk() {
        let repo = build_repo(&[
            commit("older").file("a", "1\n"),
            commit("mine").file("a", "2\n").tag("v1.0"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine", "older"]);
    }

    #[tokio::test]
    async fn a_batch_fills_up_and_the_next_one_continues() {
        let repo = line(9).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let mut o = opts();
        o.guess_max = 4;
        let (plan, first) = first_batch(&git, &o).await.unwrap();

        assert_eq!(first.changes.len(), 4);
        let from = first.boundary.commit.clone().unwrap();

        let second = extend(&git, &plan, &from, 3).await.unwrap();
        assert_eq!(
            subjects(&second).await,
            ["change 5", "change 4", "change 3"]
        );
        assert_eq!(second.boundary.kind, BoundaryKind::Guess);
    }

    #[tokio::test]
    async fn the_walk_reaches_the_root() {
        let repo = line(2).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts()).await.unwrap();

        assert_eq!(batch.boundary.kind, BoundaryKind::Root);
        assert!(batch.boundary.commit.is_none());
    }

    #[tokio::test]
    async fn a_revision_that_is_not_the_checkout_works() {
        let repo = line(4).await;
        repo.git(&["switch", "--detach", "HEAD~3"]).await;

        let git = Git::discover(repo.path()).await.unwrap();
        let mut o = opts();
        o.rev = Some("main".to_owned());
        o.base = Some("main~2".to_owned());
        let (_, batch) = first_batch(&git, &o).await.unwrap();

        assert_eq!(subjects(&batch).await, ["change 4", "change 3"]);
    }
}
