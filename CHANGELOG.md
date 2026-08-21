# Changelog

All notable changes to **qreview** are written here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/).

The version policy is in [CONTRIBUTING.md](./CONTRIBUTING.md).

Changes that wait for a release are not written here. Each one is a file under
[`changelog/`](./changelog/), in the group it belongs to. See
[`changelog/README.md`](./changelog/README.md).

<!-- The release script writes new versions under this line. -->

## [0.2.0]

### Added

- **Know where you are** — a bar above the diff carries the subject of the
  change, its commit, and which file of the change is open, with Prev and
  Next beside it.
- **Open the context between two hunks** — a bar says how many lines it hides
  and opens them, whole or ten at a time. A diff carries what changed and
  three lines around it, and the rest is one click away.
- **Files grouped by directory** — the directory is said once and the files
  under it carry their name alone, so a change of forty files stays readable
  in a narrow pane.
- **Ignore whitespace** — a switch above the diff leaves out the lines that
  differ only by spacing.
- **Mark a change read** — a box beside each change of the series, kept
  between sessions. It says nothing about the comments: a change can be read
  and still carry remarks.

### Changed

- **The diff reads like Gerrit** — dense monospace rows, the pale line and
  the stronger word inside it, and a line-number gutter left plain. Both
  themes follow the Gerrit palette.
- **The window belongs to the code** — the comments pane is gone and the two
  left panes are one. A comment about the file or the change now sits above
  the diff, where it is written. The code went from about half the window to
  five sixths of it, and `[` hides the sidebar as well.

### Fixed

- **Copy the series** said the series held nothing. It trusted a comment
  count taken when the session opened, so a comment written a minute later
  was invisible to it.
- **The date of a commit** is the same on every machine. One git writes
  `+00:00` for UTC and another writes `Z`, so the tool reads a count of
  seconds and formats it itself.
- **Load 5 older** did nothing at the base of a series, and nothing at a
  merge. The base is where the first batch stopped, not a wall, and a merge
  now joins the list, where it is reviewable, while the walk goes on down the
  first parent.
- **The side by side view** — the two gutters took a quarter of the pane
  each, so the code started in the middle of the window, and no line was
  green or red. A comment now sits under the side it was written on rather
  than across both.

### Removed

- **Resolving a thread** — it is a conversation with a reviewer, and a review
  of your own series before the push has none. Correct the code and delete
  the remark.
## [0.1.0]

### Added

- **Comments that survive an amend** — write on a line, on a file, or on the
  change, answer in a thread, and resolve it. Comments are keyed by the
  `Change-Id`, so amending the commit keeps them.
- **Export for a session** — two buttons put the review in the clipboard as
  Markdown, code first and comment after, and `qreview export` prints the
  same text.
- **Custom file types** — a repository declares its own extensions in
  `.qreview.json` and every reader gets the colours with no setup. A grammar
  file dropped in the configuration directory adds a language.
- **Gerrit patch sets** — on a repository with a Gerrit remote, the versions
  already pushed are listed with the numbers the server gave them, and one is
  fetched only when you open it. `--no-gerrit` skips the question.
- **Local review of a Git series** — `qreview` reads the repository, opens a
  browser, and shows the series one change at a time, with the files of the
  change and a diff you can read unified or side by side.
- **Merge review** — a merge is read against the auto-merge, so the diff is
  the conflict resolution and not the whole branch the merge brought in. The
  two parents stay one click away.
- **Patch sets** — read any version of a change against any other. Name an
  older commit with `--prev`, and a comment written on one version is found
  again in the next, or listed apart when its line is gone.
- **Load 5 older** — the start of a series is often impossible to compute, so
  the walk stops at a boundary that says why, and going further back is one
  click. A merge, a tag, the base, and a guessed start each say what they are.
## [0.0.0] — unreleased

The project starts. Nothing is released yet. See
[`roadmap/plan.md`](./roadmap/plan.md) for the work order.
