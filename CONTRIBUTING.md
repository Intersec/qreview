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
git push --follow-tags
```

`cargo xtask release` writes an annotated tag, which is the kind
`--follow-tags` pushes. A lightweight tag would stay on your machine and no
release pipeline would start.

The tag starts a pipeline that runs the gate again, builds the static Linux
x86-64 binary, uploads it to the package registry of the project, and creates
the release that links to it. A tag whose gate is red publishes nothing.

One pipeline runs, not two. The push carries the release commit and its tag,
and GitLab would start a pipeline for each. The branch one stands down on a
commit named `chore(release): v…`, because the tag one runs the same gate on
the same commit and is the one that publishes.

The pipeline talks to its own GitLab server and to nothing else. It pulls no
image from another registry, and `scripts/gitlab-release.sh` calls the API of
the project it runs in.

A server behind a private certificate authority needs that CA in the job.
The release job takes it from either of two places, and installs it before it
calls the API:

- `CI_SERVER_TLS_CA_FILE`, which the runner sets on its own when it is
  configured with `tls-ca-file`. Nothing to do in the project.
- `ADDITIONAL_CA_CERT_BUNDLE`, a CI/CD variable of the project or of its
  group, holding the CA. The name is GitLab's own. A File variable and a
  plain one both work.

Without either, `curl` stops with `SSL certificate problem: unable to get
local issuer certificate`. A variable marked **Protect variable** reaches a
tag only when the tag is protected, so protect the tag pattern too, or leave
the variable unprotected.

`make dist` packs the same file on your machine, under `dist/`, beside the
binary itself. Use it to release locally: to hand a colleague a build before
a tag, or when no pipeline is available. `make dist TAG=v1.2.3` names it.

The version lives in `Cargo.toml`, alone. The `package.json` of the interface
is private and carries no version.

## Tests

Read [`roadmap/testing.md`](./roadmap/testing.md). The short form: a test
beside the code, real repositories as fixtures, no network, and a bug gets a
test before the fix.
