# qreview — working notes for Claude

qreview is a local code review tool for a Git series, with a browser
interface and the Gerrit model: one change at a time, with patch sets. Rust
for the binary, Vue 3 for the interface, embedded in that binary. It ships as
one file and needs no runtime.

The project is in **development**. No code exists yet.
[README.md](./README.md) says what the tool is.
[CONTRIBUTING.md](./CONTRIBUTING.md) holds the branch model, the commits, and
the release. This file holds what a session must do.

## Start of a session

1. Read [`roadmap/plan.md`](./roadmap/plan.md). Take the first task that is
   not `[x]`.
2. Read the part of [`roadmap/design.md`](./roadmap/design.md) that the task
   touches.
3. Do the task.

Do not start a task that is not the next one, unless the user names it.

## Before you finish

Leave this green:

```sh
make check   # eslint + prettier + vitest + vite build
             # + cargo fmt + clippy + cargo test + release build
```

`check` is the whole gate and the only command to run. It prints one line per
step and stays quiet until a step fails, and then it prints that step in full.
Do not pipe it through `tail` or `grep`. A Prettier failure carries no word
that you can grep for. `make check V=1` prints everything.

Fix the format with `make fmt`. It runs `cargo fmt` and `prettier --write`.

**You cannot see the interface. `make shots` can.** It writes screenshots of
both themes into `web/e2e/.shots`, from the real binary on a real repository.
Look at them before saying that a visual change works.

`make dev` runs the server and Vite together, with hot reload on the
interface. Use it instead of rebuilding the binary to look at a component.
Open the URL Vite prints, not the one the server prints.

The interface is built before every Rust step, because the binary embeds
`web/dist`. A clean checkout needs `make setup` once.

## End of a task

1. Commit the task alone.
2. Mark the task `[x]` in [`roadmap/plan.md`](./roadmap/plan.md).
3. Update [`roadmap/features.md`](./roadmap/features.md) when a feature
   changes status.
4. Drop a changelog fragment when a user can see the change. See
   [`changelog/README.md`](./changelog/README.md).

## The design is a contract

[`roadmap/design.md`](./roadmap/design.md) holds the data model, the storage
format, the API routes, and the export format. Other code depends on them.

If the task needs a different design, stop and report. Name what breaks and
propose the change. Once the user agrees, write the decision in
[`roadmap/stack.md`](./roadmap/stack.md) with the date, then write the code.
Never change the design in silence.

Two parts are contracts with something outside the code:

- **The storage format** is read by a future version of the tool. A change
  owes a `version` bump and a migration.
- **The export format** is read by a Claude session. A snapshot test pins it.

## Tests

Read [`roadmap/testing.md`](./roadmap/testing.md). The rules that a session
breaks most often:

- A unit test sits in the file it tests, in `#[cfg(test)] mod tests`.
- **No network in a test.** Gerrit is tested against recorded answers.
- **No mock of the `git` binary.** Git code runs against a real repository,
  built by `testutil::build_repo` in a temporary directory.
- A bug gets a test before the fix. The commit carries both.
- Read a changed `insta` snapshot before you accept it.

## Code style

`rustfmt` and `clippy` own the Rust style, ESLint and Prettier own the
interface. `make check` gates all four, and clippy runs with `-D warnings`, so
a warning is an error. Run `make fmt` to correct the format.

Never silence a lint with an `#[allow]` without a comment on the line above
that says why.

Two rules that no tool enforces:

- Code comments **in English** and **short**. One line where it fits.
- A comment says **why**, not what. The code says what.

## Rules that hold from the first line

- **The server never writes to the working tree.** It reads with `git`. The
  only write is `git fetch`, and only when the user asks for a Gerrit patch
  set. A task that needs a write is a stop-and-report.
- **The server binds `127.0.0.1` only**, with a session token. No other
  interface, ever.
- **Gerrit is optional at every point.** A query that fails, times out, or
  finds nothing leaves the local review working.
- **A comment is never lost.** A comment that cannot be anchored on the
  selected patch set is shown in the unanchored panel. It is never dropped and
  never moved in silence.
- **A merge is never crossed in silence.** The walk follows the first parent,
  stops at the merge, and shows a boundary card. Loading more is always an
  explicit action of the reader.
- **`git` is a child process**, never a library. The tool must agree with what
  the developer sees in the terminal, `diff.algorithm` included.
- **The rows leave the server finished.** The server parses the diff, computes
  the word spans, highlights with `syntect`, and merges the spans. The browser
  paints. Do not move that work into the interface.
- **Nothing reads the working tree.** Every commit, tree, and blob comes from
  the object database. This is what lets a reader review a series that is not
  the checkout, and it is why a dirty worktree changes no diff.
- **The binary carries the interface.** A release build embeds `web/dist`.
  Nothing at run time reads a file that the binary does not hold, except the
  repository, the configuration, and the comment store.

## Established patterns

This section fills as the code grows. Add a line when a pattern becomes the
way the project does a thing. Keep each line short.

- Nothing yet.

## Git workflow

- **Never push, never merge.** The user publishes. A rebase of the current
  branch is expected.
- **Commit at every task**, so the user tests each step alone.
- **Amend rather than stack** while the commit is only on your branch. A
  commit that is on `main` is public. Add a new commit instead.

## Commits

- Say the **why**. A subject alone is enough only for a trivial change.
- English, imperative. Subject 72 characters or less, body wrapped at 72.
- A Conventional Commits prefix in the subject: `feat`, `fix`, `docs`, `test`,
  `refactor`, `perf`, `chore`, `ci`, `style`.
- No `Change-Id` trailer. The remote is GitHub, not Gerrit.
- End with a `Co-Authored-By: Claude …` trailer.

## Documentation

`roadmap/` holds the plan and the design. `README.md` holds the use.
`CONTRIBUTING.md` holds the process.

**This is an open-source project.** Write for a reader who has never seen your
code base. Never commit the name of a host, a repository, a branch, or a
service that belongs to one organization. Examples use
`review.example.com` and `myproject`. A site specific language map belongs in
the `.qreview.json` of that site.
