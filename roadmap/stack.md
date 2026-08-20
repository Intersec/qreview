# Stack and decisions

This document records the technology choices and the reason for each one. A
decision that changes gets a new entry with a date. Nothing is deleted.

## 2026-08-20 — TypeScript on both sides — SUPERSEDED

> **Superseded the same day** by *A Rust binary with a Vue interface*, below.
> The single binary turned out to be a requirement, not a later comfort, and
> the argument below has a hole: it assumes the colors are computed in the
> browser.

**Decision.** One language for the whole tool. A Node command line and server,
a Vue 3 single page application, one test runner.

| Layer | Choice |
|---|---|
| Command line | Node 24, TypeScript, `commander` for the arguments |
| Server | Fastify on `127.0.0.1`, a random port |
| Git access | The `git` binary, run as a child process |
| Interface | Vue 3, TypeScript, Vite, Tailwind CSS, Pinia |
| Colors | Shiki, the TextMate grammars that VS Code uses |
| Tests | Vitest |
| Style | ESLint and Prettier |

**Why.** The color requirement decides it. Gerrit-quality colors with a map
from extension to language means TextMate grammars, and the good
implementation of those grammars is Shiki, which is JavaScript. A Rust or a
Python server still needs the JavaScript interface for Shiki. A second
language then buys nothing and costs a build.

The second reason is a sibling project on the same stack. Its conventions,
its CI file, and its test layout transfer to this repository.

**Rejected: a Rust server with a TypeScript interface.** One static binary is
the real advantage, and it is a distribution problem, not an architecture
problem. Node has a single executable application mode, and `bun build
--compile` makes one binary. We can add that later without a rewrite.

**Rejected: a Python server with a TypeScript interface.** Two languages
anyway, slower on a large diff, and the hardest of the three to hand to a
colleague.

**Why the `git` binary and not a library.** `libgit2` bindings and `isomorphic-git`
both drift from the behavior of the installed `git`. The tool must agree with
what the developer sees in the terminal, including the local
`diff.algorithm` and `diff.renames` configuration. A child process is also
easier to test, because the fixture is a real repository.

## 2026-08-20 — A Rust binary with a Vue interface

**Decision.** The server, the git access, the diff, the colors, and the
storage are Rust, built into one binary. The interface stays Vue 3 with Vite,
and the built assets are embedded in that binary.

| Layer | Choice |
|---|---|
| Command line | Rust, `clap` |
| Server | `axum` on `tokio`, bound to `127.0.0.1` |
| Git access | The `git` binary, run as a child process |
| Diff | Our parser of `git diff-tree -p`, `similar` for the word diff |
| Colors | `syntect`, TextMate grammars, CSS classes as output |
| Assets | `rust-embed`, the Vite output inside the binary |
| Interface | Vue 3, TypeScript, Vite, Tailwind CSS, Pinia |
| Tests | `cargo test`, `insta` for snapshots, Vitest for the interface |
| Style | `rustfmt`, `clippy`, ESLint, Prettier |

**Why the earlier argument fails.** It said that Gerrit-quality colors need
TextMate grammars, that the good implementation of those grammars is Shiki,
and that Shiki is JavaScript. The first two hold. The third only matters when
the browser does the highlighting. `syntect` reads the same grammars in Rust.

**What the move gains, beyond the binary.**

- The server sends rows that already carry their syntax spans and their
  intra-line diff spans, composed once, in one place that a snapshot test
  pins. Today those two are merged in the browser, per file, on every open.
- No grammar is downloaded. A 5000 line file costs the browser nothing but
  the text.
- `syntect` emits CSS class names, so a theme is a stylesheet. Light and dark
  need no second pass of highlighting.
- A grammar is data. A user drops a `.sublime-syntax` file in the
  configuration directory and gets a language, with no rebuild. A house file
  format therefore needs no change to this repository.

**What it costs.** Two languages and two toolchains, which the earlier entry
called a cost that bought nothing. It now buys the binary. Node becomes a
**build-time** dependency only: nobody who runs qreview needs it.

**Rejected: a Rust interface with Leptos or Dioxus.** One language everywhere
and no Node in the build. Rejected because the interface is the hard part of
this tool, a WASM bundle is not smaller than the Vite output, and the Vue
ecosystem is what this organization knows.

**Rejected: no build step for the interface, plain ES modules.** It removes
Node from the build, and it removes Tailwind, single-file components, and the
type checker with it. The interface carries comment threads, a patch set
selector, virtual scrolling, and keyboard navigation. That is not hand-rolled.

## 2026-08-20 — Make drives the build, xtask does the chores

**Decision.** `make` is the entry point. It calls `npm` for the interface and
`cargo` for everything else. Release chores live in a `cargo xtask` crate.

```
make build     npm ci + vite build, then cargo build --release
make dev       the Rust server, and Vite with a proxy on /api
make check     the whole gate, the only command before a commit
make install   the binary into ~/.local/bin
```

**Why `make` and not a build script.** A `build.rs` that runs `npm` couples
every `cargo check` to the interface build, and a build script that reaches
the network is bad practice. `make` keeps the two builds separate and lets
either one run alone.

**Why an xtask crate and not a shell script.** The release collects the
changelog fragments, writes `CHANGELOG.md`, bumps the version, and tags. That
is parsing and rewriting files, which is fragile in shell. `cargo xtask`
compiles, and it adds no tool to install.

**The version lives in `Cargo.toml`**, alone. The `package.json` of the
interface is private and carries no version.

**The release artifact is a `x86_64-unknown-linux-musl` binary.** A glibc
binary built on one distribution fails on an older one, and that is the first
thing a colleague hits. musl links statically and removes the question.

Measured on the scaffold: 1.9 MB against 1.8 MB for the glibc build, and
`ldd` says `statically linked`. It costs no C toolchain, because Rust links
musl with its own bundled linker. `make dist` adds the target when it is
missing, so the only prerequisite is rustup, which asdf already uses to
install Rust.

Measured again at v0.1.0, with the interface and 199 grammars inside it:
**6.2 MB**, still statically linked, and it serves a review with `web/dist`
deleted. The grammar set is most of the growth, and it is what makes a `.blk`
file read as C with no configuration.

## 2026-08-20 — Comments live in the state directory of the user

**Decision.** Comments are stored under `~/.local/state/qreview/`, keyed by
repository identity and then by `Change-Id`.

**Why.** The data survives a `git clean`, a worktree removal, and a reclone.
It never enters the working tree, so it can never be committed by accident.
The `XDG_STATE_HOME` variable is honored when it is set.

**Cost, accepted.** The comments do not travel with the checkout. A second
worktree of the same repository shares the comments, which is what we want,
because the change is the same change.

**Rejected: `git notes`.** Notes are shareable. They also conflict on a
rebase, need explicit refspecs to fetch and push, and are hard to repair by
hand. Sharing is not a goal at any version: a review that a colleague must see
belongs on Gerrit.

**Rejected: `.git/qreview/`.** Simple and close to the repository, but it dies
with the clone, and a review of a series is exactly the work you do not want
to lose.

## 2026-08-20 — Gerrit patch sets are in v1

**Decision.** The ssh query to Gerrit ships in the first version, not later.

**Why.** The patch set axis is the reason the tool exists. A version without
it is one more GitHub-style diff viewer. The local `--prev` option covers only
the commits that are still in the reflog of the developer.

**Cost, accepted.** The network appears in v1. It is contained: one module,
one command, one parser, and a hard rule that no test touches the network.

## 2026-08-20 — Apache-2.0, open source

**Decision.** Apache-2.0. The project is open source. No document names a
host, a repository, a branch, or a service that belongs to one organization.
Examples use `review.example.com` and `myproject`.

**Why.** A permissive license with a patent grant, and the one a sibling
project already uses.

**This entry replaced an earlier one** that planned an internal repository
first and a publication later. That plan is void, so the entry is rewritten
and not superseded: it holds no reasoning worth keeping. The rule it produced
survives and is stronger. A site specific language map belongs in the
`.qreview.json` of that site, not in this repository.

## 2026-08-20 — One `main` branch, tags, changelog fragments

**Decision.** One long-lived branch. Short feature branches. Semantic version
tags. One file per changelog entry under `changelog/`.

**Why.** The tool is small and has one developer. The fragment system exists
because a shared `Unreleased` section conflicts on every rebase.

## 2026-08-20 — The series is a walk, not a resolution

**Decision.** The series loads in batches from the head, backwards, following
the first parent. It stops at a boundary that names its reason. Every stop
carries a **Load 5 older** action.

**Why.** The start of a series is often impossible to compute. A base that is
wrong by one commit used to mean a restart with `--base`. Now it means one
click. The refusal on a series of more than 50 commits is removed, because
nothing loads 400 commits any more.

**The guess.** When no base resolves, the tool guesses, and it says that it
guessed. The cap is 10 commits. The signals are, in order: a merge, a tag, a
commit on a remote-tracking ref, an author who is not the user, then the cap.

**Assumption to correct if it is wrong.** The last two signals end the guess
only. They are not boundaries in a later batch. A pushed commit can still be
under review, and a colleague's commit can sit inside a series, so both make a
bad hard stop but a good hint for the first screen.

## 2026-08-20 — A merge is reviewed against the auto-merge

**Decision.** The walk stops at a merge and never crosses one in silence. A
merge is reviewable, with the Gerrit model: the default base is the auto-merge
tree, and parent 1 and parent 2 are selectable. A merge list tab names the
commits it brings in.

**Why, measured.** On a real merge between two release branches of a large C
code base, the diff against the auto-merge is 8 406 lines and the diff against
the first parent is 140 774. The large number is work already reviewed on the
other branch. The small number is the conflict resolution, which was reviewed
nowhere. Such a merge often carries hand-resolved conflicts, so those lines are
real risk.

**How.** `git merge-tree --write-tree <p1> <p2>` prints the tree on its first
line. Then `git diff-tree -p <tree> <merge>`. It needs git 2.38 or later,
tested on 2.43. Older git falls back to `git diff-tree --cc`.

## 2026-08-20 — What the grammar set carries, measured

The set is `two-face`, the grammars `bat` ships, read by `syntect` with the
pure-Rust regex engine. It holds 199 syntaxes. Measured, for the languages the
built-in map names:

| Language | Bundled |
|---|---|
| C | yes |
| Python | yes |
| D | yes |
| Jinja2 | yes |
| **Cython** | **no** |

So `.iop` maps to D, which was the plan, and `.pyx`, `.pxd` and `.pxi` map to
Python. Cython is Python with types, so the loss is small: the types read as
identifiers. A user who wants better drops a `.sublime-syntax` file in the
configuration directory, and no rebuild is needed.

A test holds this. Every language the map names must resolve to a grammar,
because a map entry that resolves to nothing shows the file as plain text
while looking configured.

## Open questions

(The Cython question is answered below, under *What the grammar set carries*.)
- **Other platforms.** The tool is built and tested on Linux, and that is the
  only target. Nothing in the design stops the others, and nothing has tried
  them.
- **Big files.** The limit at which the interface becomes slow is unknown.
  Task M3 measures it on a real commit of 100 files or more.
