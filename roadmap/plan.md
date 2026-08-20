# Plan

The milestones, in order. Each one ends with a state that a person can use or
judge. Each task is one commit with `make check` green.

Mark a task `[x]` when it is done. Add a task when you find one. Do not delete
a task that was dropped: strike it and write why.

**Status: M0 to M4 are done, except a range comment. M5 is next.**

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
- [x] `.gitlab-ci.yml`, one `verify` stage that runs `make check`
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
- [ ] Folded context, opened line by line or whole
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

- [ ] The patch set model and the selector
- [ ] `--prev`, repeatable
- [ ] A diff of a patch set against its parent
- [ ] A diff between two patch sets
- [ ] The label when the parents differ
- [ ] The anchoring, the three branches of `design.md` section 5.3
- [ ] The panel of the comments that could not be anchored

## M6 — Gerrit

*Exit: on a repository with a Gerrit remote, the patch sets already pushed
appear without any option.*

- [ ] The remote URL parser, and the canonical form used by the store
- [ ] The branch from `.gerrit-branch`, then the configuration
- [ ] The ssh query, with the timeout
- [ ] The parser of the answer, against recorded fixtures
- [ ] The lazy fetch of a patch set ref
- [ ] Every failure path leaves the local review working
- [ ] `--no-gerrit`
- [ ] The change number and the Gerrit URL in the interface

## M7 — Export, configuration, and v0.1.0

*Exit: a colleague copies one binary, runs it, and reviews a series on the
first try. No Node, no cargo, no install step.*

- [ ] The Markdown export, one change and the whole series
- [ ] The copy button
- [ ] `qreview export` and `qreview list`
- [ ] The snapshot test that pins the export format
- [ ] The three configuration layers
- [ ] `examples/config.json` and `examples/languages.json`
- [ ] The README: install, use, and the custom file types section
- [ ] `cargo xtask release`: the changelog collection, the version bump, the
      tag
- [ ] `make dist`, the musl binary, and the size of it written in `stack.md`
- [ ] Tag `v0.1.0`

---

## After v0.1.0

Ordered by the value we expect, not by the effort:

1. A rebase-aware diff between two patch sets.
2. A JSON export, and a Claude Code skill that reads it.

## Session protocol

1. Read this file. Take the first task that is not `[x]`.
2. Read the part of [`design.md`](./design.md) that the task touches.
3. Write the code and the tests together. See [`testing.md`](./testing.md).
4. Run `make check`.
5. Commit the task alone, with the why in the message.
6. Mark the task `[x]` here. Move the **Status** line when a milestone ends.
7. Update [`features.md`](./features.md) when a feature changes status.

If a task contradicts [`design.md`](./design.md), stop and report. Write the
new decision in [`stack.md`](./stack.md) with the date, then continue.
