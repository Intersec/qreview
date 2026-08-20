# Test policy

The policy is strict on purpose. qreview reads a developer's real repository
and holds review work that exists nowhere else. A silent failure loses that
work.

There is no CI yet. The rules below are written so that the day a GitLab
pipeline exists, it runs `make check` and nothing else changes.

## The gate

```sh
make check
```

One command. It runs, in this order: ESLint, Prettier, Vitest, the interface
build, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, the
release build, and the browser tests. It prints one line per step and stays quiet until a step fails. A red
`check` means the task is not done.

Do not pipe `check` through `tail` or `grep`. A Prettier failure carries no
word that you can grep for.

## Rules

1. **A unit test sits in the file it tests**, in a `#[cfg(test)] mod tests`
   block. An integration test that drives the binary sits in `tests/`.
2. **Shared fixtures live in `crates/qreview/src/testutil/`**, behind
   `#[cfg(test)]`, because a corpus is read by several modules. Recorded
   answers and diff corpora are files under `crates/qreview/tests/data/`.
3. **No network in a test. Ever.** The Gerrit module is tested against
   recorded answers in `crates/qreview/tests/data/gerrit/`. A test that opens a socket is a
   failed test.
4. **Git code is tested against a real repository.** `testutil::build_repo`
   makes one in a temporary directory with real `git` commands. No mock of the
   `git` binary. A mock agrees with the code that wrote it, not with git.
5. **A bug gets a test before the fix.** The test fails, then the fix makes it
   pass. The commit carries both.
6. **A parser owes a corpus.** The diff parser and the Gerrit answer parser
   are pure functions from text to structure. Every shape we have seen becomes
   a case in the corpus, pinned with `insta`.
7. **An expectation records what the code does**, not what it must do. A
   case that is wrong on purpose carries a defect code and says so in its
   name. So an unexpected result turns the run red, and a fix turns its own
   case red, which is the reminder to update the expectation.
8. **No coverage number.** A percentage rewards a test on a getter and says
   nothing about the loop that eats the review. What we count is in the table
   below.
9. **`insta` snapshots cover the shapes that cross the wire.** One per API
   route and one for the export. Review a changed snapshot with `cargo insta
   review`. Never accept one without reading it.

## What must be covered

| Area | The invariant a test must hold |
|---|---|
| Base resolution | The six rules, in order, on a real repository. A detached HEAD and a branch with no upstream both have a case |
| The walk | The first batch, then a second batch of 5. The walk follows the first parent and never enters the second. The list only grows, and a diff already built is not rebuilt |
| The guess | Each of the five stop signals, one case each. The cap holds at 10. The boundary says which signal stopped it |
| Merge review | The auto-merge base against a repository with a real conflict resolution. Parent 1 and parent 2 as bases. The `--cc` fallback path |
| Change identity | A `Change-Id` survives an amend and a rebase. A commit without one falls back to the hash |
| Diff parsing | Every hunk of a real `git diff-tree` output. A rename, a copy, a binary file, a file with no trailing newline, a CRLF file |
| Intra-line diff | The word spans of a changed line, and the case where a line changed completely |
| Comment storage | A write is atomic. A read of a corrupt file reports an error and never deletes the file. Two writes in the same second do not lose one |
| Anchoring | The three branches: same blob, moved line found by context, and nothing found. An unanchored comment is never dropped |
| Gerrit parsing | The recorded answers, including a change with no patch set and a query that returns nothing |
| Language map | Every extension of the built-in map resolves to the expected language. An unknown extension falls back to plain text and does not fail |
| Highlighting | The syntax spans and the intra-line spans on the same row do not overlap and cover the text exactly. A row is reproducible, so it is pinned by a snapshot |
| Export | The exact text for a fixed set of comments. The format is a contract with a Claude session, so it is pinned by a snapshot |
| API routes | Every route answers with the documented shape. A request without the token gets 401 |

## The repository fixture

```rust
let repo = build_repo(&[
    commit("first").file("src/a.blk", "int a;\n"),
    commit("second").file("src/a.blk", "int b;\n").change_id("I8f3a…"),
    merge("Merge branch rel-2.1 into rel-3.0").from("side"),
]);
```

It writes a real repository in a temporary directory, runs `git init`, sets a
fixed author and a fixed date, and removes the directory when the value is
dropped. A fixed date matters: a test that depends on the wall clock is a test
that fails on a Monday.

The builder can make a merge with a real conflict resolution, because the
merge review and the walk both need one.

## The interface

Most of the logic left the browser when the colors moved to the server, so the
interface carries less to test. Vitest covers what is left: a component that
pairs rows, cuts a row into segments, or renders a comment body.

The types in `web/src/api/types.ts` are written by hand. One `insta` snapshot
per route pins the Rust shape, so a struct that drifts from the TypeScript
turns a Rust test red.

## The browser

`web/e2e/` drives a real browser with Playwright, against the real binary on
a real repository. It is part of `make check`.

- **The browser is the one on the machine.** Playwright can download its own,
  and a 150 MB download in every clone buys nothing: the interface has to work
  in the browser the reader uses. `QREVIEW_BROWSER` names one outright.
- **No browser, no lie.** `make check` prints `skip playwright` when it finds
  none, rather than passing quietly. The pipeline installs chromium, so the
  skip only ever happens on a developer machine.
- **The fixture is a real repository**, built by `e2e/fixture.ts` with the
  shapes a reviewer meets: a modified file, a new file, a rename, two hunks
  far apart, and a merge under the boundary.
- **The token is read from what the binary prints**, never forced in, so the
  test walks the same path a person does.
- **A console error fails a test.** One suite watches the console and expects
  it silent: a Vue warning about a dropped attribute is how the side by side
  view lost its colours.

`make shots` writes screenshots of both themes into `web/e2e/.shots`. They are
not a test. They are how a person, or an agent with no eyes on the screen,
sees what the interface looks like.

## Before the CI exists

Run `make check` before every commit. The day the pipeline lands, it is the
same command in the `verify` stage, so nothing in this document changes.
