# Plan

The milestones, in order. Each one ends with a state that a person can use or
judge. Each task is one commit with `make check` green.

Mark a task `[x]` when it is done. Add a task when you find one. Do not delete
a task that was dropped: strike it and write why.

**Status: every milestone through M12 is done.** Two tasks of M3 are still
open, both marked below. Everything after the milestones is in the GitHub
issues.

The version that is cut is in [`CHANGELOG.md`](../CHANGELOG.md). This file
does not name one: it said `v0.5.4` for five releases.

---

## M0 — Foundation

*Exit: `make check` is green on an empty project, and the governance files are
in place.*

- [x] The roadmap: `concept.md`, `stack.md`, `design.md`, `features.md`,
      `plan.md`, `testing.md`
- [x] `LICENSE` (Apache-2.0)
- [x] `CLAUDE.md`, `README.md`, `CONTRIBUTING.md`
- [x] `CHANGELOG.md` and the `changelog/` fragment directories
- [x] The cargo workspace: `crates/qreview`, `crates/xtask`
- [x] `rustfmt.toml`, `clippy.toml`, and `-D warnings` in the gate
- [x] `web/`: Vite, Vue 3, TypeScript, Tailwind, ESLint, Prettier
- [x] `.tool-versions`: rust 1.92.0, nodejs 24.13.1, read by asdf
- [x] `rust-embed`, with the debug build reading `web/dist` from disk
- [x] The `Makefile`: `setup`, `dev`, `build`, `check`, `test`, `fmt`,
      `install`, `dist`
- [x] `make check`, one line per step, quiet until a step fails
- [x] A pipeline that runs `make check` on every push
- [x] `.gitignore`, `.editorconfig`
- [x] The first commit

## M1 — The git core, no interface

*Exit: `qreview --no-open` prints the first batch of the series and the file
list of each change as text, and the tests cover the six resolution rules and
the guess.*

- [x] `model.rs`, the types of `design.md` section 4
- [x] `git/exec.rs`, the child process wrapper, with a timeout and a clear
      error when `git` is missing
- [x] `testutil::build_repo`, the repository builder, merges included
- [x] Base resolution, the six rules of `design.md` section 3.1
- [x] Any revision as the head of a series, not only `HEAD`. Nothing reads the
      working tree
- [x] `.gerrit-branch` read from the reviewed commit, not from disk
- [x] The first-parent walk, in batches, with the boundary it stops on
- [x] The best-effort guess, capped at 10, with the signal that stopped it
- [x] Change identity: the `Change-Id` trailer, the `sha-` fallback
- [x] The file list of a change, with the statistics
- [x] The diff parser, from `git diff-tree -p` to `FileDiff`
- [x] The intra-line word diff, with the `similar` crate
- [x] The corpus of diff shapes: rename, copy, binary, no trailing newline,
      CRLF, empty file
- [x] `cli.rs`, the argument parsing with clap, and the text output

## M2 — The server and the first view

*Exit: `qreview` opens the browser, shows a colored unified diff of every
change of the batch, and loads the next batch on demand.*

- [x] axum on the loopback address, a random port, the session token
- [x] `GET /api/session`, `/api/changes/:key`, `/files`, `/diff`
- [x] `POST /api/series/extend`, the next batch
- [x] The 401 on a request with no token
- [x] The Vue application, the Vite build, Tailwind, Pinia
- [x] `rust-embed`, and the release binary that serves the interface alone
- [x] The three panes: the series, the files, the diff
- [x] The boundary card and the **Load 5 older** action
- [x] The unified diff view
- [x] The syntax theme, light and dark in one stylesheet
- [x] `syntect`, the class output, and the spans on every row
- [x] The highlight cache, keyed by blob hash
- [x] The language map, and the example site map
- [x] The user grammar directory, loaded at startup
- [x] Make sure whether a Cython grammar is bundled. Written in `stack.md`:
      there is none, so `.pyx` maps to Python
- [x] The browser open, and `--no-open`

## M3 — A diff you want to read

*Exit: the side-by-side view is usable on a commit of 100 files or more.*

- [x] The side-by-side view
- [x] The intra-line marks
- [x] Folded context, opened line by line or whole. Done by the context bar
      of M8
- [x] Renames and copies as one file
- [x] The merge review: the auto-merge base, parent 1, parent 2
- [x] The merge list, behind a button
- [x] Git older than 2.38 says so and reads against the first parent
- [ ] **Follow the other parent** on a merge card
- [x] The file list with a filter
- [x] Keyboard navigation: j and k for files, n and p for changes, u for
      the view, / for the filter
- [ ] Measure the time to first paint on a 5000 line file. Write the number
      and the limit in `stack.md`
- [x] A very large file stops at two thousand rows and says so

## M4 — Comments

*Exit: a whole review of a series can be written and found again after a
restart.*

- [x] The store: the layout, the repository identity, the atomic write
- [x] A read that survives a corrupt file and never deletes it
- [x] The comment routes, create, edit, delete, resolve
- [x] A comment on a line, on a file, on the change
- [x] Threads and replies
- [x] The draft mark
- [x] Markdown in the body, sanitized before it is shown
- [x] The comment count on the series pane
- [x] Make sure that an amend keeps the comments

## M5 — Patch sets

*Exit: `qreview <rev> --prev <sha> --prev <sha>` shows three patch sets and
diffs any two of them.*

- [x] The patch set model and the selector
- [x] `--prev`, repeatable
- [x] A diff of a patch set against its parent
- [x] A diff between two patch sets
- [x] The label when the parents differ
- [x] The anchoring, the three branches of `design.md` section 5.3
- [x] The panel of the comments that could not be anchored

## M6 — Gerrit

*Exit: on a repository with a Gerrit remote, the patch sets already pushed
appear without any option.*

- [x] The remote URL parser, and the canonical form used by the store
- [x] The branch from `.gerrit-branch`, then the configuration
- [x] The ssh query, with the timeout
- [x] The parser of the answer, against recorded fixtures
- [x] The lazy fetch of a patch set ref
- [x] Every failure path leaves the local review working
- [x] `--no-gerrit`
- [x] The change number and the Gerrit URL in the interface

## M7 — Export, configuration, and v0.1.0

*Exit: a colleague copies one binary, runs it, and reviews a series on the
first try. No Node, no cargo, no install step.*

- [x] The Markdown export, one change and the whole series
- [x] The copy button
- [x] `qreview export` and `qreview list`
- [x] The snapshot test that pins the export format
- [x] The three configuration layers
- [x] `examples/config.json` and `examples/languages.json`
- [x] The README: install, use, and the custom file types section
- [x] `cargo xtask release`: the changelog collection, the version bump, the
      tag
- [x] `make dist`, the musl binary, and the size of it written in `stack.md`
- [x] Tag `v0.1.0`

---

## M8 — It has to look like Gerrit

*Exit: a person who reviews in Gerrit every day opens qreview and is not
surprised by anything.*

- [x] The browser harness: Playwright on the browser already installed, a
      real repository, and the binary under it
- [x] `make shots`, so a change to the interface can be looked at
- [x] The browser tests inside `make check`, and a plain skip without a
      browser
- [x] The side by side view: widths from a colgroup, colours on the row
- [x] The gutter left plain, the way Gerrit leaves it
- [x] The context bar between hunks: `+N common lines` and `+10`
- [x] The `lines` route, which reads what the diff does not carry
- [x] A comment card that reads like a Gerrit draft
- [x] The window belongs to the code: one sidebar, the files under the
      change they belong to, and no pane standing empty
- [x] Resolving a thread is gone. It is a conversation with a reviewer, and
      a local review before the push has none
- [x] The change header: the subject, the commit, and which file of the
      change is open, with Prev and Next
- [x] A comment anchored under its own side in the side by side view
- [x] The files grouped under the directory they live in
- [x] A whitespace toggle
- [x] A per-change read mark, kept between sessions

## M9 — What the reader asked for after using it

- [x] A comment stands alone: no author, no draft, no reply, no resolving
- [x] Ten lines of context, not the three git gives
- [x] A preferences panel, which writes the file the command line reads
- [x] A theme the reader chooses
- [x] The keys Gerrit uses, and `?` to list them
- [x] A mock Gerrit for the browser tests: a fake `ssh` on the PATH and a
      bare repository as the remote

## M10 — The commit message, and a release the pipeline builds

- [x] The commit message as a file of the change, `/COMMIT_MSG`, first in the
      list and commentable
- [x] A release job on the pipeline: the static Linux x86-64 binary, attached
      to the tag

## M11 — What the reader asked for after using it, again

- [x] A comment on a range of lines, and on a part of a line: picked with
      the mouse, or with `v` and the movement keys
- [x] A remark that is typed is kept as it is typed, and survives another
      file, another change and a reload
- [x] The export ordered: the oldest commit first, the files in alphabetic
      order with the commit message first, and each file top to bottom

## M12 — Seeing what the session holds

- [x] The count of comments on the two copy buttons, on each change of the
      series, and on each file of a change
- [x] A pane that lists the remarks: the change being read first, then the
      rest in the order of the export
- [x] A row of that pane opens the place it names
- [x] The bar between the series and the code, and the one between the
      series and the comments, dragged to give a pane more room

- [x] Say at start when a newer release is out, and say nothing at all when
      the question cannot be asked

- [x] GitHub Actions, the install from the releases, and the address the
      version check asks

## After that

The backlog is in the GitHub issues. A rebase-aware diff between two patch
sets is issue #5.

One idea has no issue yet: an HTTP transport for Gerrit, for a server with
ssh closed.

## Session protocol

1. Read this file. Take the first task that is not `[x]`.
2. Read the part of [`design.md`](./design.md) that the task touches.
3. Write the code and the tests together. See [`testing.md`](./testing.md).
4. Run `make check`.
5. Commit the task alone, with the why in the message.
6. Mark the task `[x]` here. Move the **Status** line when a milestone ends.

If a task contradicts [`design.md`](./design.md), stop and report. Write the
new decision in [`stack.md`](./stack.md) with the date, then continue.
