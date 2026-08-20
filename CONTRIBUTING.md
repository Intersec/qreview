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
- No `Change-Id` trailer. The remote is GitLab, not Gerrit.

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
make dist             # the static binary to attach to the tag
```

The version lives in `Cargo.toml`, alone. The `package.json` of the interface
is private and carries no version.

## Tests

Read [`roadmap/testing.md`](./roadmap/testing.md). The short form: a test
beside the code, real repositories as fixtures, no network, and a bug gets a
test before the fix.
