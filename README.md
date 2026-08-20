# qreview

Local code review for a Git series, in your browser, with the Gerrit model:
one change at a time, with patch sets.

One binary, no runtime to install, nothing leaves your machine.

## Why

Local diff viewers copy the GitHub model: one branch, one flat diff. A Gerrit
workflow needs two other things. The commit is the review unit, so a series of
eight commits is eight reviews. The patch set is the second axis, because the
question is often "what changed since the last version of this change".

qreview reads the local repository, starts a small web server on the loopback
address, and opens the browser.

## Use

```sh
qreview                                          # the current series
qreview af448g1                                  # one commit
qreview af448g1..HEAD                            # a range
qreview af448g1 --prev 4888eaa8 --prev 774faaa9  # with older patch sets
qreview export                                   # the comments, as Markdown
qreview list                                     # what this repository holds
```

| Option | What it does |
|---|---|
| `--base <rev>` | Set the base of the series. It wins over every rule |
| `--prev <sha>` | Treat a commit as an older patch set. Repeatable |
| `--no-gerrit` | Skip the Gerrit query |
| `--no-open` | Print the URL, open no browser |
| `--port <n>` | Use a fixed port |

What the browser shows:

- The series on the left, one line per commit, ending in a card that says why
  the walk stopped there and offers **Load 5 older**.
- The files of the change, then the diff, unified or side by side, with the
  changed words marked inside a line.
- Comments on a line, on a file, or on the change, in threads you can
  resolve. Markdown in the body.
- A patch set selector, and a diff between any two versions.
- A merge read against the auto-merge, so you see the conflict resolution and
  not the whole branch it brought in.
- Two buttons that put the review in the clipboard, made to be pasted into a
  session with an AI agent.

| Key | What it does |
|---|---|
| `j` / `k` | The next or the previous file |
| `n` / `p` | The next or the previous change |
| `u` | Swap unified and side by side |
| `/` | Jump to the file filter |

Comments are stored under `~/.local/state/qreview`, keyed by `Change-Id`. An
amend keeps them.

## Install

Copy the binary and run it. It needs `git` on the PATH and nothing else.

```sh
curl -o ~/.local/bin/qreview <the release artifact>
chmod +x ~/.local/bin/qreview
```

### Build from source

Rust and Node, both pinned in `.tool-versions` for
[mise](https://mise.jdx.dev) or asdf. Node builds the interface. It is not
needed to run the tool.

```sh
git clone <this repository>
cd qreview
make setup         # the interface dependencies, once
make build         # the interface, then the binary
make check         # the gate: lint, format, tests, build
make install       # into ~/.local/bin
make dist          # a static musl binary, for a colleague
```

`make dist` adds the `x86_64-unknown-linux-musl` target when it is missing. It
needs no C toolchain: Rust links musl with its own bundled linker.

## Gerrit

When the repository has a Gerrit remote, qreview lists the patch sets already
pushed and can diff the local commit against them. It reads the host, the
port, and the project from the remote URL, and the target branch from
`.gerrit-branch` when the file exists.

qreview only reads. It never votes, never comments on the server, and never
pushes.

## Custom file types

Bundled grammars claim the common extensions on their own. A code base with
its own file types declares them once, in `.qreview.json` at its top level, so
every reader of that repository gets the map with no setup:

```json
{ "languages": { "blk": "c", "pxc": "c", "pyx": "cython", "iop": "d" } }
```

A grammar is data too. Drop a `.sublime-syntax` or `.tmLanguage` file in
`~/.config/qreview/grammars/` and name it in the map. No rebuild.

Personal settings go in `~/.config/qreview/config.json`. See
`examples/config.json`.

## Documentation

| Document | What it holds |
|---|---|
| [`roadmap/concept.md`](./roadmap/concept.md) | The problem, the users, the non-goals, the prior art |
| [`roadmap/design.md`](./roadmap/design.md) | The architecture, the data model, the API |
| [`roadmap/plan.md`](./roadmap/plan.md) | The milestones and the tasks |
| [`examples/`](./examples/) | Configuration files to copy |
| [`roadmap/testing.md`](./roadmap/testing.md) | The test policy |
| [`CONTRIBUTING.md`](./CONTRIBUTING.md) | The branch model, the commits, the release |

## License

Apache-2.0. See [`LICENSE`](./LICENSE).
