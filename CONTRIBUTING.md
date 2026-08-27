# Contributing

The rules of the project. They are short because the project is small.

## Direction

qreview is a local review tool for a Gerrit workflow. Two lines decide what
gets in:

- **The change and the patch set are the model.** A feature that pushes the
  tool back toward the GitHub single-diff model does not belong here.
- **Nothing leaves the machine.** qreview reads Gerrit over ssh. It writes
  nothing to a server, and it sends nothing anywhere else.

The non-goals are in [`roadmap/concept.md`](./roadmap/concept.md). A request
against one of them is refused, not delayed.

## Ownership

Nicolas Pauss owns the project and the direction. The code is Apache-2.0 and
open source.

Write every document for a reader who has never seen your code base. Name no
host, no repository, and no branch that belongs to one organization. A site
specific language map belongs in the `.qreview.json` of that site, never in
this repository.

## Branches

One long-lived branch, `main`. Work on a short branch named after the change,
`feat/patchset-selector` or `fix/anchor-crlf`. Rebase on `main` and merge with
a fast-forward. No long-lived integration branch.

Never push to `main` with a red `make check`.

## Commits

- One task, one commit. The task is the unit that a reader can test alone.
- Say the **why**. A subject alone is enough only for a trivial change.
- English, imperative, subject 72 characters or less, body wrapped at 72.
- A [Conventional Commits](https://www.conventionalcommits.org) prefix in the
  subject, counted in the 72: `feat`, `fix`, `docs`, `test`, `refactor`,
  `perf`, `chore`, `ci`, `style`.
- Amend while the commit is only on your branch. Add a new commit once it is
  on `main`.
- No `Change-Id` trailer. The remote is GitHub, not Gerrit.

### The issue a commit answers

Name it in the body, on a line of its own:

```
Closes #12      the issue is done, and closes when the commit reaches main
Refs #12        the commit is part of the answer, the issue stays open
```

`Closes`, `Fixes` and `Resolves` all close, and each issue needs its own
keyword: `Closes #12, closes #13`. GitHub closes an issue when the commit
lands on the default branch, so pushing is what closes it. Nothing else to
do, and no token to hold.

`Refs` when the fix is not the whole of it, or when the reporter has to say
whether their case is answered. An issue closed too early is one nobody
reads again.

## Changelog

Do not edit `CHANGELOG.md`. It holds released versions, and
`cargo xtask release` is the only thing that writes it.

A change that a user can see drops one file in `changelog/added`,
`changelog/changed`, `changelog/fixed`, or `changelog/removed`. One file per
entry, named after the change. See
[`changelog/README.md`](./changelog/README.md).

## Versions

[Semantic versioning](https://semver.org), read for a tool before 1.0:

- **`0.x`** — while the tool is not yet used every day by more than one
  person.
- **Minor** (`0.1` → `0.2`) — a notable feature. Most releases.
- **Patch** (`0.1.0` → `0.1.1`) — fixes only.
- **`1.0`** — the milestones of `roadmap/plan.md` are done and a second person
  uses the tool.
- **Major** — a change of storage format that the tool cannot migrate, or a
  change of direction.

The storage format carries a `version` field. A format change owes a
migration, or a refusal with a clear message. Never a silent read of an older
shape.

## Release

```sh
make check
cargo xtask release   # collects the changelog fragments, writes CHANGELOG.md,
                      # bumps the version in Cargo.toml, tags
git push origin main v1.2.3
```

Push the tag by name. `cargo xtask release` prints the line to run, with the
tag it just wrote.

Not `--follow-tags`: it carries annotated tags only beside refs it is
already pushing, so it sends nothing at all when the branch is up to date,
and the release workflow never starts. A tag that reaches nobody publishes
nothing.

The tag starts the `release` workflow: it runs the gate again on the tagged
commit, builds the static Linux x86-64 binary, and creates the GitHub
release with the notes of that version and the binary attached. A tag whose
gate is red publishes nothing. There is nothing to configure: the token that
publishes is the one Actions hands to the job.

One run of the gate, not two. The push carries the release commit and its
tag, and GitHub starts a workflow for each ref. The `check` workflow stands
down on a commit named `chore(release): v…`, because the `release` workflow
runs the same gate on the same commit and is the one that publishes.

`make dist` packs the same file on your machine, under `dist/`, beside the
binary itself. Use it to release locally: to hand a colleague a build before
a tag, or when no pipeline is available.

The name carries no version, on purpose: `qreview-linux-x86_64.xz`, the same
on your machine and on the server. The release page and the package registry
both say which version it is, and a name that never changes is what the
permanent link of the newest release points at. `qreview --version` says it
too.

The version lives in `Cargo.toml`, alone. The `package.json` of the interface
is private and carries no version.

## Tests

Read [`roadmap/testing.md`](./roadmap/testing.md). The short form: a test
beside the code, real repositories as fixtures, no network, and a bug gets a
test before the fix.
