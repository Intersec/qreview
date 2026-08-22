# Features

The backlog. It is a target to design against, not a promise of scope.

**Legend:** ✅ works today · 🟡 partial · ⬜ not started · 💡 a constraint that
still binds

Released in v0.1.0 unless the line says otherwise. The work order for what is
left is in [`plan.md`](./plan.md).

## Command line

- ✅ `qreview` — review the series of the current branch
- ✅ `qreview <rev>` — review one commit
- ✅ `qreview <revA>..<revB>` — review a range
- ✅ `qreview <rev> --prev <sha>` — add a local commit as a patch set
- ✅ `--base`, `--no-gerrit`, `--no-open`, `--port`
- ✅ `qreview export` — print the review of a change to stdout
- ✅ `qreview list` — list the stored reviews of this repository
- ✅ A clear error when the directory is not a repository, when the revision
  does not exist, and when the series is too long

## Series and changes

- ✅ Base resolution, the six rules of `design.md`
- ✅ The best-effort guess when no base resolves, 10 commits at most
- ✅ A **Load 5 older** action on every boundary, in batches of 5
- ✅ The backward walk follows the first parent only
- ✅ The boundary card, with the reason: merge, tag, base, guess cap, root
- ✅ **Follow the other parent** on a merge card, when the second parent is
  not a remote branch
- ✅ The change list, with the subject, the author, and the comment count
- ✅ Keyboard navigation between the changes of the series
- ✅ A per-change read mark, kept between sessions
- ✅ Review of a series that is not the checkout: a remote-tracking branch, a
  fetched Gerrit ref, a tag
- 💡 A pushed commit and a colleague's commit both end the guess. Neither ends
  a later batch, because both are wrong often enough to be a bad boundary
- 💡 Loading more is additive. It never changes a diff already shown, because
  every change is diffed against its own parent
- 💡 Nothing reads the working tree. Every byte comes from the object
  database, so the reviewed series never has to be the checkout

## Merges

- ✅ A merge is reviewable, with the auto-merge as the default base
- ✅ The base selector: auto-merge, parent 1, parent 2
- ✅ The merge list, the commits the merge brings in
- ✅ The fallback to `git diff-tree --cc` on git older than 2.38
- 💡 Measured on a real release-branch merge: the auto-merge diff is 8 406
  lines and the first-parent diff is 140 774. Only the first is reviewable
- 💡 A combined diff has no single old line number. A comment on one anchors
  to the new side only

## Diff view

- ✅ The file list of a change, with the added and removed counts
- ✅ The commit message as the first file of the change, commentable like any
  other (v0.4.0)
- ✅ A side-by-side diff
- ✅ The view chosen in the configuration, and remembered after that
- ✅ Intra-line marks on a changed line
- ✅ Renames and copies shown as one file, not two
- ✅ A binary file, an empty file, and a file with no trailing newline
- ✅ Folded context between hunks, opened whole or ten lines at a time
- ✅ Whitespace changes shown or hidden
- ✅ A file too large is shown with a warning and no colors
- ✅ The file tree, with a filter

## Colors

- ✅ `syntect` in the server, with CSS classes as output
- ✅ The syntax spans and the intra-line spans merged into the rows
- ✅ The highlight cache, keyed by blob hash
- ✅ The built-in map for the extensions that grammars do not claim
- ✅ A user map in the configuration
- ✅ A user grammar directory, `.sublime-syntax` loaded at startup
- ✅ The language of a file changed for the session, from the interface
- ✅ A light theme and a dark theme, following the system
- 💡 The browser downloads no grammar. It receives rows that are already
  highlighted, so a large file costs it the text and nothing else
- 💡 A Cython grammar may not be bundled. Then `.pyx` maps to Python until a
  grammar file is dropped in the configuration directory

## Comments

- ✅ A comment on a line
- ⬜ A comment on a range of lines
- ✅ A comment on a file, shown above its diff
- ✅ A comment on the whole change, shown above the diff
- ✅ Edit and delete a comment
- ✅ Markdown in the body of a comment
- ✅ Storage under `~/.local/state/qreview`, one file per change
- ✅ An atomic write, and a read that survives a corrupt file
- ✅ Comments keyed by `Change-Id`, so an amend keeps them

## Patch sets

- ✅ The patch set list of a change
- ✅ A diff of a patch set against its parent
- ✅ A diff between two patch sets
- ✅ A label when the two patch sets sit on different bases
- ✅ Anchoring of a comment on another patch set
- ✅ A panel for the comments that could not be anchored
- ✅ `--prev <sha>` adds a local commit as a patch set
- 💡 A diff between patch sets on different bases carries the rebase noise. A
  rebase-aware diff is a later task

## Gerrit

- ✅ The host, the port, and the project read from the remote URL
- ✅ The branch read from `.gerrit-branch`, then from the configuration
- ✅ `gerrit query --format=JSON --patch-sets` over ssh
- ✅ A lazy fetch of `refs/changes/NN/CCCC/P`
- ✅ A failure or a timeout leaves the local review working
- ✅ `--no-gerrit`
- ✅ The whole path tested against a fake ssh, with a real fetch
- ✅ The Gerrit change number and its URL shown in the interface

## Export

- ✅ The Markdown export of one change
- ✅ The Markdown export of the whole series
- ✅ A copy button in the interface
- ✅ `qreview export` on the command line
- ✅ Resolved threads left out, `--all` to include them
- ⬜ A JSON export, for a tool that reads it

## Later

- ⬜ A rebase-aware diff between two patch sets
- ⬜ A JSON export, and a Claude Code skill that reads it
