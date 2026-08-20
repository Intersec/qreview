# Changelog

All notable changes to **qreview** are written here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/).

The version policy is in [CONTRIBUTING.md](./CONTRIBUTING.md).

Changes that wait for a release are not written here. Each one is a file under
[`changelog/`](./changelog/), in the group it belongs to. See
[`changelog/README.md`](./changelog/README.md).

<!-- The release script writes new versions under this line. -->

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
