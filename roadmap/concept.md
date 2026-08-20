# Concept

qreview is a local code review tool with a browser interface. It reviews a
Git series the way Gerrit does: one change at a time, with patch sets.

## The problem

A team on Gerrit pushes to Gerrit and reviews in Gerrit. The review of your
own work, before the push, has no equivalent tool. You read the diff in the
terminal, keep the remarks in your head or in a scratch file, and lose them at
the next amend.

Local diff viewers exist. They all copy the GitHub model: one branch, one
merge request, one flat diff. That model hides the two things that matter in a
Gerrit workflow:

- **The commit is the review unit.** A series of eight commits is eight
  reviews, not one. A remark belongs to a commit, not to a branch.
- **The patch set is the second axis.** The question is often not "what does
  this change do" but "what changed since the last version of this change".

## What qreview is

A command that reads the local repository, starts a small web server on the
loopback address, and opens the browser:

```
qreview                                          # the current series
qreview af448g1                                  # one commit
qreview af448g1 --prev 4888eaa8 --prev 774faaa9  # with local patch sets
```

The browser shows a Gerrit-like review surface:

- The series on the left. One line per commit, with the review state.
- The files of the selected commit, then the diff, side by side.
- Comments on a line, on a range, on a file, or on the change.
- A patch set selector, with a diff between any two patch sets.
- Syntax colors, with a map from extension to language that you control.
- An export of the comments for a Claude session.

Comments live on disk and survive an amend, because they are keyed by
`Change-Id` and not by commit hash.

## Who it is for

Developers who work in a Gerrit workflow. The first user is the author of the
series, before the push. A second reader can use it on a fetched series, but
qreview does not replace Gerrit for team review. Nothing is shared through a
server.

## Non-goals

- **Not a Gerrit replacement.** No votes, no submit, no CI results, no
  reviewer management. qreview reads from Gerrit. It never writes to Gerrit.
- **Not a Git client.** No commit, no rebase, no amend from the interface.
- **Not a merge request tool.** No GitHub, no GitLab, no forge API.
- **Not multi-user.** One developer, one machine, no authentication beyond
  the loopback address and a session token.
- **Not a way to share a review.** Sharing a review with a colleague is what
  Gerrit is for. Push the series and comment there.

## Prior art

We looked for an existing tool before we chose to build one.

| Tool | What it gives | Why it does not fit |
|---|---|---|
| [tuicr](https://github.com/agavra/tuicr) | Review TUI, comments kept between sessions, export to a forge or to the clipboard | Terminal only. GitHub model. Weak colors, and an unknown extension is plain text |
| [difit](https://github.com/yoshiko-pg/difit) | Local web server, GitHub "Files changed" view, line comments, "Copy Prompt" for an AI agent | The closest tool. Single diff range, no series, no change identity, no patch sets |
| [git-appraise](https://github.com/google/git-appraise) | Distributed review, review data in `git notes`, a web interface | Review data model is one request per branch. Little activity. The interface reads more than it writes |
| [diffity](https://github.com/nilbuild/diffity), [git-diff-viewer](https://github.com/ishikawa-pro/git-diff-viewer) | Diff view with comments made for AI agents | Same GitHub model, no series |
| Gerrit itself | The model we want | A server. It sees the series only after the push |

Two ideas are worth a copy. difit shows that a comment export written for an
AI agent is the feature that makes a local review tool useful every day.
git-appraise shows that review data keyed to the repository, and not to a
directory, is what survives.

The rest is new work. No local tool implements the change and patch set model.
