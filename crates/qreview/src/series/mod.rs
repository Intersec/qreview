//! The series: which commits are under review, and where the walk stops.
//!
//! A series is not resolved once. It is a walk backwards from a head, loaded
//! in batches, that stops at a boundary and says which one. See
//! `roadmap/design.md` section 3.1.

mod refs;

use anyhow::{Result, bail};

use crate::gerrit::{self, Coordinates};
use crate::git::commit::{self, CommitInfo};
use crate::git::exec::Git;
use crate::model::{Boundary, BoundaryKind, ChangeSummary, MergeInfo, ParentInfo};

pub use refs::{is_on_a_remote, name_of, remotes_holding, tags_by_commit};

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
    /// Ask Gerrit for the patch sets already pushed.
    pub gerrit: bool,
    /// Show the tracked changes that are not committed as a change of the
    /// series, above the newest commit.
    pub worktree: bool,
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
            gerrit: true,
            worktree: true,
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
pub async fn plan(git: &Git, opts: &Options, gerrit: Option<&Coordinates>) -> Result<Plan> {
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

    // Rules 4 and 5.
    let local = match upstream_base(git, &head).await {
        Some(base) => Some((base, "the upstream of the branch")),
        None => integration_base(git, opts, &head)
            .await
            .map(|base| (base, "the merge base with the integration branch")),
    };

    // Rule 6: Gerrit, when the clone answered nothing or answered far.
    //
    // `.gerrit-branch` names the integration branch, and work pushed to a
    // feature branch under it carries that same file. So a base that far is
    // usually the wrong branch, and Gerrit is the only place that knows
    // which branch the change was pushed to. A near answer is trusted as it
    // is, and that repository pays no round trip.
    let found = match &local {
        Some((base, _)) if near(git, base, &head, opts.max_commits).await => local,
        _ => match gerrit_base(git, gerrit, &head).await {
            Some(base) => Some((base, "the branch Gerrit has the change on")),
            None => local,
        },
    };

    match found {
        // A base is never thrown away for its length. It is where the series
        // ends, and a first batch that does not reach it stops on a count,
        // which the card names along with what is left.
        Some((base, rule)) => Ok(Plan {
            head,
            base: Some((base, rule)),
            guessing: false,
            limit: opts.max_commits,
        }),
        None => Ok(Plan {
            head,
            base: None,
            guessing: true,
            limit: opts.guess_max,
        }),
    }
}

/// Rule 6: the merge base with the branch Gerrit has the change on.
///
/// Gerrit is the only place that knows that branch. `.gerrit-branch` names
/// the integration branch, and work pushed to a feature branch under it
/// carries that same file, so rule 7 reads it as the integration branch and
/// lands far too low.
///
/// It is asked after the upstream and before the file, so a repository whose
/// upstream answers pays no round trip. Gerrit stays optional: no answer
/// falls through to the rule below.
async fn gerrit_base(git: &Git, gerrit: Option<&Coordinates>, head: &str) -> Option<String> {
    let coords = gerrit?;
    let info = commit::info(git, head).await.ok()?;
    let change_id = info.change_id()?;
    let change = gerrit::query(coords, change_id, &info.hash).await.ok()??;

    base_on_branch(git, &change.branch, head).await
}

/// The merge base with a branch of the server, named without the remote.
///
/// A branch that is the head itself is no base: a series read on the branch
/// it was pushed to would be empty.
async fn base_on_branch(git: &Git, branch: &str, head: &str) -> Option<String> {
    let other = commit::resolve(git, &format!("origin/{branch}"))
        .await
        .ok()?;
    if other == head {
        return None;
    }

    merge_base(git, &other, head).await
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

/// Is the base close enough to be the start of a series on its own?
///
/// Not a refusal: a base further than this is kept when nothing better
/// answers. It only says when to spend a round trip on Gerrit.
async fn near(git: &Git, base: &str, head: &str, max: usize) -> bool {
    count_between(git, base, head).await.unwrap_or(0) <= max
}

/// How many commits of the first-parent line stand in `from..to`.
async fn count_between(git: &Git, from: &str, to: &str) -> Option<usize> {
    let range = format!("{from}..{to}");
    let out = git
        .text(&["rev-list", "--count", "--first-parent", &range])
        .await
        .ok()?;

    out.trim().parse::<usize>().ok().filter(|n| *n > 0)
}

/// Walk backwards from `start`, following the first parent, and stop at the
/// first boundary.
///
/// `start` is the newest commit that is not loaded yet. The walk never
/// crosses a merge: a merge becomes the boundary, and the reader decides.
///
/// `guess` is true for the first batch of a plan that found no base. It is
/// the only batch that ends on the two soft signals, and `author` is the
/// address they compare against.
pub async fn walk(
    git: &Git,
    plan: &Plan,
    start: &str,
    limit: usize,
    guess: bool,
    author: Option<&str>,
) -> Result<Batch> {
    let base = plan.base.as_ref().map(|(b, _)| b.as_str());
    let mut changes = Vec::new();
    let mut current = Some(start.to_owned());
    // Both read on the first commit that can end on them, and not before: a
    // walk that stops at once must not pay for them.
    let mut tags = None;
    let mut on_head = None;

    while let Some(hash) = current {
        // The base is the end of the series, whatever else the commit is.
        if Some(hash.as_str()) == base {
            let rule = plan.base.as_ref().map(|(_, r)| *r).unwrap_or("the base");
            return Ok(done(git, changes, BoundaryKind::Base, &hash, rule, false, None).await);
        }

        if changes.len() == limit {
            let kind = if guess {
                BoundaryKind::Guess
            } else {
                BoundaryKind::Batch
            };
            // A known base says how much is left, so the count on the card
            // is a distance rather than a cliff.
            let left = match base {
                Some(base) if !guess => count_between(git, base, &hash).await,
                _ => None,
            };
            let reason = match (guess, left) {
                (true, _) => format!("the guess stopped at its cap of {limit}"),
                (false, Some(left)) => format!("{limit} commits loaded, {left} to the base"),
                (false, None) => format!("{limit} commits loaded"),
            };
            let mut batch = done(git, changes, kind, &hash, &reason, guess, None).await;
            batch.boundary.remaining = left;

            return Ok(batch);
        }

        let info = commit::info(git, &hash).await?;

        // A merge is a change like any other, and the walk stops under it.
        // The commits it brings in are another line of history, so crossing
        // the merge stays an explicit action of the reader.
        if info.is_merge() {
            let merge = merge_info(git, &info, &plan.head).await;
            let reason = format!("under the merge {}", short(&hash));
            // `is_merge` is two parents at least, so the first one is here.
            let under = info.parents[0].clone();

            changes.push(summary(info));
            return Ok(done(
                git,
                changes,
                BoundaryKind::Merge,
                &under,
                &reason,
                false,
                merge,
            )
            .await);
        }

        // The head itself may carry a tag. Only a tag under the series ends it.
        if !changes.is_empty() {
            let tags = match tags {
                Some(ref tags) => tags,
                None => tags.insert(tags_by_commit(git).await),
            };

            if let Some(tag) = tags.get(&hash) {
                let reason = format!("the tag {tag}");
                return Ok(
                    done(git, changes, BoundaryKind::Tag, &hash, &reason, false, None).await,
                );
            }
        }

        // Two signals that only ever end the guess, which is the first
        // batch. Both are wrong often enough to make a bad hard stop: a
        // pushed commit can be under review, and a commit of somebody else
        // can sit inside a series. Once the reader has asked for more, the
        // count is the only bound left.
        if guess && !changes.is_empty() {
            let on_head = match on_head {
                Some(ref refs) => refs,
                None => on_head.insert(remotes_holding(git, &plan.head).await),
            };

            if let Some(name) = is_on_a_remote(git, &hash, on_head).await {
                let reason = format!("on {name}");
                return Ok(done(
                    git,
                    changes,
                    BoundaryKind::Guess,
                    &hash,
                    &reason,
                    true,
                    None,
                )
                .await);
            }
            if let Some(author) = author
                && !author.is_empty()
                && info.email != author
            {
                let reason = format!("written by {}", info.author);
                return Ok(done(
                    git,
                    changes,
                    BoundaryKind::Guess,
                    &hash,
                    &reason,
                    true,
                    None,
                )
                .await);
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
            subject: None,
            remaining: None,
            reason: "the history has no parent left".to_owned(),
            guessed: false,
            merge: None,
        },
    })
}

/// A boundary on `commit`, the commit the card names and the button loads.
///
/// The subject comes with it. A hash alone says nothing, and the reader
/// decides whether to go further from what is written there.
async fn done(
    git: &Git,
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
            subject: commit::info(git, commit).await.ok().map(|it| it.subject),
            remaining: None,
            reason: reason.to_owned(),
            guessed,
            merge,
        },
    }
}

pub(crate) fn summary(info: CommitInfo) -> ChangeSummary {
    ChangeSummary {
        key: info.key(),
        change_id: info.change_id().map(str::to_owned),
        subject: info.subject.clone(),
        author: info.author.clone(),
        commit: info.hash.clone(),
        parents: info.parents.clone(),
        // The store fills these in. The walk knows nothing about comments.
        patch_set_count: 1,
        comment_count: 0,
        reviewed: false,
        is_merge: info.is_merge(),
        worktree: false,
    }
}

/// What a merge card shows about the merge and each of its parents.
///
/// A parent counts as remote only for a branch that does not hold the head.
/// The branch the series itself was pushed to holds every parent, and
/// following one of them drags nothing new in.
async fn merge_info(git: &Git, info: &CommitInfo, head: &str) -> Option<MergeInfo> {
    let on_head = remotes_holding(git, head).await;
    let mut parents = Vec::new();

    for parent in &info.parents {
        parents.push(ParentInfo {
            commit: parent.clone(),
            name: name_of(git, parent).await,
            remote: is_on_a_remote(git, parent, &on_head).await.is_some(),
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
///
/// This is the only batch the guess bounds, so it is the only one that reads
/// the author of the head.
pub async fn first_batch(
    git: &Git,
    opts: &Options,
    gerrit: Option<&Coordinates>,
) -> Result<(Plan, Batch)> {
    let plan = plan(git, opts, gerrit).await?;
    let head = plan.head.clone();
    let author = match plan.guessing {
        true => author_of(git, &head).await,
        false => None,
    };
    let batch = walk(
        git,
        &plan,
        &head,
        plan.limit,
        plan.guessing,
        author.as_deref(),
    )
    .await?;

    Ok((plan, batch))
}

/// The next batch, from the commit the last boundary named.
///
/// The reader asked for it, so only a real boundary ends it: the base, a
/// merge, a tag, the root, or the count.
pub async fn extend(git: &Git, plan: &Plan, from: &str, count: usize) -> Result<Batch> {
    walk(git, plan, from, count, false, None).await
}

/// Who the series belongs to: the author of its newest commit.
///
/// Not `user.email`. A reader reviews the work of other people, and their
/// own address would end every such series at its first commit.
pub async fn author_of(git: &Git, head: &str) -> Option<String> {
    let email = commit::info(git, head).await.ok()?.email;

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
        let (plan, batch) = first_batch(&git, &o, None).await.unwrap();

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
        let (_, batch) = first_batch(&git, &o, None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["change 4", "change 3"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Base);
    }

    #[tokio::test]
    async fn rule_3_a_single_revision_is_that_commit_alone() {
        let repo = line(5).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let mut o = opts();
        o.rev = Some("HEAD~1".to_owned());
        let (_, batch) = first_batch(&git, &o, None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["change 4"]);
    }

    #[tokio::test]
    async fn rule_4_the_upstream_of_the_branch() {
        let repo = line(4).await;
        repo.remote("origin", "ssh://review.example.com:29418/myproject")
            .await;
        repo.track("main", "origin", "HEAD~2").await;

        let git = Git::discover(repo.path()).await.unwrap();
        let (plan, batch) = first_batch(&git, &opts(), None).await.unwrap();

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
        let (plan, batch) = first_batch(&git, &opts(), None).await.unwrap();

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
        let (plan, batch) = first_batch(&git, &opts(), None).await.unwrap();

        assert!(plan.guessing);
        assert_eq!(batch.changes.len(), 3);
        assert_eq!(batch.boundary.kind, BoundaryKind::Root);
    }

    #[tokio::test]
    async fn a_base_further_than_the_cap_is_kept_and_the_card_says_so() {
        let repo = line(12).await;
        repo.remote("origin", "ssh://review.example.com:29418/myproject")
            .await;
        repo.track("main", "origin", "HEAD~11").await;

        let git = Git::discover(repo.path()).await.unwrap();
        let mut o = opts();
        o.max_commits = 5;
        let (plan, batch) = first_batch(&git, &o, None).await.unwrap();

        // A base is where the series ends. A long one is loaded in pieces;
        // it is never thrown away for a guess that knows less.
        assert!(!plan.guessing);
        assert_eq!(batch.changes.len(), 5);
        assert_eq!(batch.boundary.kind, BoundaryKind::Batch);
        assert_eq!(batch.boundary.remaining, Some(6));
        assert_eq!(batch.boundary.reason, "5 commits loaded, 6 to the base");
    }

    #[tokio::test]
    async fn the_last_batch_of_a_long_base_lands_on_the_base() {
        let repo = line(12).await;
        repo.remote("origin", "ssh://review.example.com:29418/myproject")
            .await;
        repo.track("main", "origin", "HEAD~11").await;

        let git = Git::discover(repo.path()).await.unwrap();
        let mut o = opts();
        o.max_commits = 5;
        let (plan, first) = first_batch(&git, &o, None).await.unwrap();

        let from = first.boundary.commit.clone().unwrap();
        let rest = extend(&git, &plan, &from, first.boundary.remaining.unwrap())
            .await
            .unwrap();

        assert_eq!(rest.changes.len(), 6);
        assert_eq!(rest.boundary.kind, BoundaryKind::Base);
        assert_eq!(rest.boundary.remaining, None);
    }

    /// Rule 6 in two halves. The query is covered by the browser tests,
    /// which run against a fake `ssh`; this is the half that reads git.
    #[tokio::test]
    async fn the_branch_gerrit_names_gives_the_base() {
        let repo = line(6).await;
        let feature = repo.sha("HEAD~2").await;
        repo.git(&["update-ref", "refs/remotes/origin/f/rel/work", &feature])
            .await;
        let head = repo.sha("HEAD").await;

        let git = Git::discover(repo.path()).await.unwrap();

        assert_eq!(
            base_on_branch(&git, "f/rel/work", &head).await,
            Some(feature)
        );
        // A branch the clone does not have answers nothing, and the rule
        // below takes over.
        assert_eq!(base_on_branch(&git, "no/such/branch", &head).await, None);
    }

    #[tokio::test]
    async fn a_branch_that_is_the_head_is_no_base() {
        let repo = line(4).await;
        let head = repo.sha("HEAD").await;
        repo.git(&["update-ref", "refs/remotes/origin/mine", &head])
            .await;

        let git = Git::discover(repo.path()).await.unwrap();

        assert_eq!(base_on_branch(&git, "mine", &head).await, None);
    }

    #[tokio::test]
    async fn the_guess_stops_at_its_cap() {
        let repo = line(14).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

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
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

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
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

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
        let (_, batch) = first_batch(&git, &o, None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine two", "mine one"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Base);
    }

    #[tokio::test]
    async fn the_walk_loads_the_merge_and_stops_under_it() {
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
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        assert_eq!(
            subjects(&batch).await,
            ["after the merge", "Merge side into main"],
            "the merge is a change of the series"
        );
        assert_eq!(batch.boundary.kind, BoundaryKind::Merge);

        // Nothing under the merge is loaded, and the boundary names the
        // commit the reader would go on with: the first parent.
        let loaded = batch.changes.last().expect("the merge is loaded");
        assert!(loaded.is_merge);
        assert_eq!(batch.boundary.commit.as_ref(), loaded.parents.first());

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
    async fn a_head_that_is_a_merge_is_the_change_of_the_series() {
        let repo = build_repo(&[
            commit("base").file("f", "a\n"),
            commit("side work").on_branch("side").file("g", "1\n"),
            commit("main work").on_branch("main").file("h", "1\n"),
            merge("Merge side into main").from("side"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["Merge side into main"]);
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
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Tag);
        assert!(
            batch.boundary.reason.contains("v1.0"),
            "{}",
            batch.boundary.reason
        );
    }

    /// An annotated tag names a tag object, not a commit. The map that ends
    /// a walk is keyed by the commit, so the tag has to be peeled.
    #[tokio::test]
    async fn an_annotated_tag_under_the_series_ends_the_batch_too() {
        let repo = build_repo(&[
            commit("older").file("a", "1\n"),
            commit("the release").file("a", "2\n"),
            commit("mine").file("a", "3\n"),
        ])
        .await;
        repo.git(&["tag", "-a", "v2.0", "-m", "the release", "HEAD^"])
            .await;

        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine"]);
        assert_eq!(batch.boundary.kind, BoundaryKind::Tag);
        assert!(
            batch.boundary.reason.contains("v2.0"),
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
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["mine", "older"]);
    }

    #[tokio::test]
    async fn a_batch_fills_up_and_the_next_one_continues() {
        let repo = line(9).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let mut o = opts();
        o.guess_max = 4;
        let (plan, first) = first_batch(&git, &o, None).await.unwrap();

        assert_eq!(first.changes.len(), 4);
        let from = first.boundary.commit.clone().unwrap();

        let second = extend(&git, &plan, &from, 3).await.unwrap();
        assert_eq!(
            subjects(&second).await,
            ["change 5", "change 4", "change 3"]
        );
        // The guess is the first batch. A later one is full, and nothing
        // about a full batch is a guess.
        assert_eq!(second.boundary.kind, BoundaryKind::Batch);
        assert!(!second.boundary.guessed);
    }

    #[tokio::test]
    async fn the_branch_the_series_is_pushed_to_ends_nothing() {
        let repo = line(6).await;
        let head = repo.sha("HEAD").await;
        repo.git(&["update-ref", "refs/remotes/origin/mob/mine", &head])
            .await;

        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        // That ref holds every commit of the series, so it says nothing
        // about where the series starts.
        assert_eq!(batch.changes.len(), 6);
        assert_eq!(batch.boundary.kind, BoundaryKind::Root);
    }

    #[tokio::test]
    async fn a_pushed_branch_still_loads_a_whole_batch() {
        let repo = line(9).await;
        let head = repo.sha("HEAD").await;
        let older = repo.sha("HEAD~1").await;
        repo.git(&["update-ref", "refs/remotes/origin/mob/mine", &head])
            .await;
        repo.git(&["update-ref", "refs/remotes/origin/main", &older])
            .await;

        let git = Git::discover(repo.path()).await.unwrap();
        let (plan, first) = first_batch(&git, &opts(), None).await.unwrap();

        // `origin/main` does not hold the head, so it still ends the guess.
        assert_eq!(first.changes.len(), 1);

        // The reader asked for five. A signal that only ends a guess must
        // not end the batch, or the series loads one commit per click.
        let from = first.boundary.commit.clone().unwrap();
        let second = extend(&git, &plan, &from, 5).await.unwrap();

        assert_eq!(second.changes.len(), 5);
    }

    #[tokio::test]
    async fn a_series_of_somebody_else_reads_to_its_start() {
        let repo = build_repo(&[
            commit("shared ground").file("a", "1\n"),
            commit("theirs one")
                .file("a", "2\n")
                .author("Other Person", "other@example.com"),
            commit("theirs two")
                .file("a", "3\n")
                .author("Other Person", "other@example.com"),
        ])
        .await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        // The series belongs to whoever wrote its head. Comparing against
        // `user.email` ended this one under its first commit.
        assert_eq!(subjects(&batch).await, ["theirs two", "theirs one"]);
        assert!(batch.boundary.reason.contains("Test Author"));
    }

    #[tokio::test]
    async fn a_boundary_names_the_commit_it_stops_above() {
        let repo = line(9).await;
        let git = Git::discover(repo.path()).await.unwrap();

        let mut o = opts();
        o.guess_max = 4;
        let (_, batch) = first_batch(&git, &o, None).await.unwrap();

        // The card says what the button would load, so the reader decides
        // from what is written there rather than from a hash.
        assert_eq!(batch.boundary.subject.as_deref(), Some("change 5"));
    }

    #[tokio::test]
    async fn the_root_names_no_commit_and_no_subject() {
        let repo = line(2).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

        assert!(batch.boundary.commit.is_none());
        assert!(batch.boundary.subject.is_none());
    }

    #[tokio::test]
    async fn the_walk_reaches_the_root() {
        let repo = line(2).await;
        let git = Git::discover(repo.path()).await.unwrap();
        let (_, batch) = first_batch(&git, &opts(), None).await.unwrap();

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
        let (_, batch) = first_batch(&git, &o, None).await.unwrap();

        assert_eq!(subjects(&batch).await, ["change 4", "change 3"]);
    }
}
