# Design

The architecture, the data model, and the contracts. Read the part you touch
before you write code. If the code must contradict this document, stop and
report.

## 1. Process model

```
qreview [rev] [options]
    |
    |-- read the git context      (repository, series, changes)
    |-- read the configuration    (defaults, user, repository)
    |-- query Gerrit              (optional, ssh)
    |-- start the server          127.0.0.1:<random port>
    `-- open the browser          http://127.0.0.1:<port>/?t=<token>
```

One process, one binary, no runtime to install. The interface is inside the
binary. The process stops with Ctrl-C, or after the browser closes and no
request arrives for the idle timeout.

The server binds the loopback address only. It generates a random token at
startup. The first page request carries the token in the query string, and the
server answers with a session cookie. A request without the cookie or the
token gets 401. This stops another local user, and a page in another tab, from
reading the repository.

The server never writes to the working tree. It never reads it either. Every
commit, tree, and blob comes from the object database, so a series under
review does not have to be the checkout, and a dirty worktree changes nothing.
The only write is `git fetch`, and only when the user asks for a Gerrit patch
set.

## 2. Layout

```
Cargo.toml            the workspace
crates/
  qreview/
    src/
      main.rs         the binary: arguments, server start, browser
      cli.rs          the arguments, clap
      lib.rs          everything below, so it can be tested without a process
      git/            the child process wrapper, the diff parser
      series/         the walk, the boundaries, the guess
      gerrit/         the remote URL parser, the ssh query, the fetch
      store/          the comment storage, the anchoring
      lang/           the extension map, syntect, the class output
      api/            the axum routes
      model.rs        the types that cross the wire
      testutil.rs     the repository fixture, test builds only
  xtask/              the release and changelog chores
web/                  the Vue application, built by Vite
  src/
  dist/               the Vite output, embedded by rust-embed
tests/                the integration tests of the binary
Makefile
```

The library and the binary are one package. A binary alone would make every
item that only a test calls look dead, and `-D warnings` would reject it.

`model.rs` holds every type that crosses the wire, with
`#[serde(rename_all = "camelCase")]`. The TypeScript shapes in section 4 are
the contract. They are written by hand in `web/src/api/types.ts` and pinned by
one snapshot test per route, so a Rust struct that changes shape turns a test
red.

The interface is served from `web/dist`, embedded by `rust-embed`. A debug
build reads the directory from disk, so `make dev` needs no rebuild of the
binary when a component changes.

## 3. The git model

### 3.1 Series

A series is the ordered list of commits under review. It is not resolved once.
It is a walk backwards from the head, loaded in batches, that stops at a
boundary and says which one.

The walk always follows the first parent. A second parent is another line of
history, and following it drags a whole branch into the review.

#### The first batch

0. Any revision works, not only `HEAD`. A remote-tracking branch, a fetched
   Gerrit ref, or an old tag are all valid heads for a series.
1. `--base <rev>` on the command line. The series is `<rev>..HEAD`.
2. A revision range argument, `<revA>..<revB>`.
3. A single revision argument. The series is that commit alone.
4. The upstream of the current branch, `@{upstream}`. The series is
   `@{upstream}..HEAD`.
5. The merge base with the integration branch. The name comes from the
   `.gerrit-branch` file **of the reviewed commit** when it has one, then from
   the configuration, then from `origin/HEAD`.
6. No base resolves. The tool guesses. See below.

A resolved base of `series.maxCommits` commits or fewer (50 by default) is
loaded whole. A longer one falls back to the guess, because a base that gives
200 commits is a wrong base.

#### The guess, when no base resolves

The guess is best effort and it says so. It loads at most
`series.guessMax` commits (10 by default) and stops at the first of these
signals:

| Order | Signal | Why it ends the guess |
|---|---|---|
| 1 | A merge commit | Another line of history starts here |
| 2 | A tag points at the commit | A release is a history boundary |
| 3 | The commit is on a remote-tracking ref | Pushed work is rarely the series you are about to review |
| 4 | The author is not the user of `user.email` | The walk reached shared work |
| 5 | `series.guessMax` commits are loaded | The cap |

Signals 3 and 4 apply to the guess only. They never end a batch later in the
walk, and they never draw a boundary card of their own. The reason is that
both are wrong often enough: a pushed commit can be yours and under review,
and a colleague's commit can sit inside your series.

#### Loading more

Every stop carries a **Load 5 older** action. The batch size after the first
is `series.batchSize`, 5 by default. The count is in the label, so the batch
size is never a surprise.

Loading more is purely additive. It appends older changes to the list and it
never changes a diff that is already shown, because every change is diffed
against its own parent.

There is no refusal on a long series. The walk stops early and the reader
extends it. A wrong stop costs one click, and a wrong base used to cost a
restart.

#### Boundaries

A batch ends at a boundary card that names the reason:

| Boundary | The card shows |
|---|---|
| Merge | The subject, both parents, and the branch each one carries |
| Tag | The tag name and the commit |
| Resolved base | The base and the rule that found it |
| Guess cap | The number loaded and the signal that stopped the guess |
| Batch | The number loaded. Nothing is wrong, there is simply more |
| Root | The history has no parent left |

Every card carries **Load 5 older**. A merge card also carries **Review the
merge** and, when the second parent is not a remote branch, **Follow the other
parent**.

#### The work that is not committed

The tracked changes that are not committed are one more change, above the
newest commit. `git stash create` writes them into the object database, and
`commit-tree` makes a plain commit of that tree on `HEAD`. From there the tool
reads them the way it reads any commit.

| | |
|---|---|
| Key | `working-tree`, never a sha: the commit changes at every keystroke |
| Shown when | the series stands on the checkout, and the tree is dirty |
| Holds | every tracked change, staged or not. Never an untracked file |
| Has no | commit message to review, patch set, or `Change-Id` |
| Off with | `--no-worktree`, or `series.worktree` in the configuration |

The commit is stamped with a fixed date, so the same tree always gives the
same commit: a reload moves no sha on the screen and writes no second object.
Nothing shows that date. See `roadmap/stack.md`, 2026-08-27.

### 3.2 Change identity

The key of a review is the `Change-Id` trailer of the commit. It survives an
amend and a rebase, which is exactly what a comment must survive.

When a commit has no `Change-Id`, the key is `sha-<full commit hash>`. The
interface says so, because comments on that key are lost at the next amend.

### 3.3 Patch sets

A patch set is one version of a change. The sources, in the order the
interface lists them:

| Source | Where it comes from |
|---|---|
| Local | The commit under review, always the last patch set |
| `--prev <sha>` | A commit given on the command line |
| Gerrit | `refs/changes/NN/CCCC/P`, fetched after the ssh query |

Each patch set carries its commit hash, its parent, its author date, and its
origin. The interface can diff any patch set against its own parent (the
default, the Gerrit behavior) or against another patch set.

CAUTION: A diff between two patch sets that sit on different bases shows the
rebase noise. This is a known limit of the simple approach. The interface
labels the pair when the parents differ, so the reader is not surprised. A
real rebase-aware diff is a later task.

### 3.4 Diff production

The diff comes from `git diff-tree -p --no-color -M -C
--find-copies-harder --full-index <base> <target>` with the algorithm from the
local configuration. The parser produces the structure of section 4.

The server does the whole job before it answers: it parses the hunks, computes
the intra-line word spans with the `similar` crate, highlights both sides with
`syntect`, and merges the two kinds of span into the rows. The browser
receives rows that it only has to paint. A large file therefore costs the
browser the text and nothing else.

### 3.5 The commit message

The commit message is one more file of the change, first in the list, under
the path `/COMMIT_MSG`. A path of a git tree never opens with a slash, so
this one collides with nothing. Gerrit uses the same name.

The text is the message of the commit and nothing else, from the subject to
the last trailer. Gerrit puts a header of five lines above it, with the
parent, the author and the two dates. This tool leaves it out. A local
review of your own series already shows the author and the commit in the
change bar, and the committer date changes at every amend, which would put a
false change in the message at every patch set.

What the message is read against depends on the base:

- **Against the parent**, the whole message is new. The parent carries
  another message, and a diff of the two says nothing about the work.
- **Against another patch set**, the two messages are diffed. This is where
  an amend of the subject or of the body shows.
- **Two messages that are the same** still make one hunk of plain lines. The
  reader opened the message to read it.

The message is not a blob, so two things read it from the commit instead:
the anchoring of section 5.3, and the excerpt of the export. A comment on a
line of the message is anchored by the hash of the line and its context, the
same way a comment on a file is.

`qreview list` does not print it. That listing says what work a change does,
and the message is not part of that.

The diff of two messages is computed with the `similar` crate, not with
`git`. The two texts are not in the object database, and writing them there
to diff them would break the rule that the tool writes nothing.

### 3.6 Reviewing a merge

A merge is reviewable, the way Gerrit reviews one. The default base is not a
parent. It is the **auto-merge**: the tree that git produces on its own from
the two parents. The diff against it is the work a person did, which is the
conflict resolution and nothing else.

Measured on a real merge between two release branches of a large C code
base:

| Base | Lines |
|---|---|
| Auto-merge (the default) | 8 406 |
| First parent | 140 774 |

The 140 774 lines are work that was already reviewed on the other branch. The
8 406 lines were never reviewed anywhere.

```sh
tree=$(git merge-tree --write-tree <parent1> <parent2> | head -1)
git diff-tree -p --no-color "$tree" <merge>
```

`git merge-tree --write-tree` needs git 2.38 or later. The first line of the
output is the tree. The lines after it describe the conflicts, and the exit
code is non-zero when there is one, which is normal here.

The base selector of a merge offers:

| Base | What it shows |
|---|---|
| Auto-merge | The conflict resolution. The default |
| Parent 1 | Everything the merge brought in from the other side |
| Parent 2 | The same, from the other direction |

A **merge list** tab lists the commits that the merge brings in,
`git log --oneline <parent1>..<parent2>`. It is a list, not a review surface.

CAUTION: On git older than 2.38, `merge-tree --write-tree` does not exist and
there is no auto-merge base. The tool says so once, on standard error, and
falls back to the first parent, which is honest but long. It does **not** fall
back to `git diff-tree --cc`: a combined diff is a different format with no
single old line number, and an untested parser for it is worse than a base
selector the reader can see.

## 4. Data model

The types below are the contract between the two sides. The Rust structs in
`model.rs` are the source. The TypeScript below is what the interface reads.

```ts
type Series = {
  repo: RepoInfo;
  head: string;            // commit hash of the newest change
  oldest: string;          // commit hash of the oldest change loaded
  changes: ChangeSummary[];
  boundary: Boundary;      // why the walk stopped
};

type Boundary = {
  kind: 'merge' | 'tag' | 'base' | 'guess' | 'root';
  commit: string;          // the commit under the boundary, not loaded yet
  reason: string;          // shown on the card, for example "on origin/rel-3.0"
  guessed: boolean;        // true when a guess produced this stop
  merge?: {
    subject: string;
    parents: { commit: string; refs: string[]; remote: boolean }[];
  };
};

type ChangeSummary = {
  key: string;             // Change-Id, or "sha-<hash>"
  changeId: string | null;
  subject: string;
  author: string;
  commit: string;          // the local commit hash
  patchSetCount: number;
  commentCount: number;
  unresolvedCount: number;
  isMerge: boolean;
};

type PatchSet = {
  number: number;          // 1 based, the local commit is the highest
  commit: string;
  parent: string;
  origin: 'local' | 'prev' | 'gerrit';
  createdAt: string;       // ISO 8601
  gerritRef?: string;      // refs/changes/NN/CCCC/P
};

type FileDiff = {
  path: string;
  oldPath: string | null;  // set on a rename or a copy
  status: 'added' | 'modified' | 'deleted' | 'renamed' | 'copied';
  language: string;        // the syntect language name
  binary: boolean;
  added: number;
  removed: number;
  hunks: Hunk[];
};

type Hunk = {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  header: string;
  rows: Row[];
};

type Row = {
  kind: 'context' | 'add' | 'remove';
  oldLine: number | null;
  newLine: number | null;
  text: string;
  tokens: Span[];          // syntax classes, from syntect
  words?: WordSpan[];      // intra-line marks, absent on a context row
};

type Span = {
  start: number;           // a byte offset into text
  end: number;
  cls: string;             // a CSS class, for example "src keyword control"
};
```

## 5. Comment storage

### 5.1 Layout

```
$XDG_STATE_HOME/qreview/           (default ~/.local/state/qreview)
  repos/
    <repo-id>/
      repo.json                    remote URL, last known path, tool version
      changes/
        I8f3a...c21.json           one file per change
        sha-4a91...f0.json
```

`<repo-id>` is the first 16 hexadecimal characters of the SHA-256 of the
canonical remote URL. The canonical form drops the user, the port, and a
trailing `.git`, so `ssh://review.example.com:29418/myproject` and
`review.example.com:myproject.git` are the same repository. A repository with no remote falls back to the hash of the
real path of the top level directory.

One file per change keeps a write small and a manual repair possible. A
comment route is scoped under its change for the same reason: one file is
read, and one file is written.

### 5.2 The file of a change

```json
{
  "version": 3,
  "key": "I8f3a...c21",
  "subject": "net: fix the retry loop",
  "reviewed": false,
  "comments": [
    {
      "id": "c-01H...",
      "patchSet": 2,
      "commit": "af448g15...",
      "createdAt": "2026-08-20T14:02:11Z",
      "updatedAt": "2026-08-20T14:02:11Z",
      "scope": "line",
      "body": "This loop retries forever when the socket is closed.",
      "anchor": {
        "file": "src/net.blk",
        "side": "new",
        "startLine": 42,
        "endLine": 42,
        "blob": "b7a1...",
        "lineHash": "sha256:9c1f...",
        "context": ["    for (;;) {", "        rc = read(fd);", "..."]
      }
    }
  ]
}
```

The versions of a change are not stored. They are read from git, and the
`commit` of each comment says which one a remark was written against. See
section 5.4.

`scope` is `line`, `range`, `file`, or `change`. A `file` comment has an anchor
with no line. A `change` comment has no anchor.

A `range` covers `startLine` to `endLine`. It can also open and close inside a
line: `startChar` and `endChar` then hold the first character on the first
line and the one after the last on the last line. Both count **UTF-16 code
units**, the units the browser measures a selection in, and the units every
offset that crosses the wire already uses. A range with no characters covers
whole lines.

A comment stands alone. There is no author, no draft, no reply and no
resolving: all four are for a conversation with a reviewer, and a review of
your own series before the push has none. You write a remark, you correct the
code, you delete the remark.

What a reader has typed and not saved is not a comment, and it never reaches
the store or the export. The browser keeps it under `qreview.drafts` in its
own storage, keyed by the change, the file and the line, and the box opens
again on it when the reader comes back to that file. Saving or cancelling
drops it.

A draft is a comment like any other, with `"draft": true`. Nothing publishes
it anywhere, so a draft is only a mark that the author has not finished.

### 5.3 Anchoring across patch sets

A comment is written against one patch set. The reader who opens another patch
set must still see it.

```
1. The blob of the file is the same       -> the same line, done.
2. Look for lines with the same lineHash  -> score each candidate on the
   context lines, keep the best score above the threshold.
3. No candidate                           -> the comment is unanchored.
```

The three branches read the tree the `side` of the anchor names: `new` reads
the patch set, `old` reads what it is diffed against. Every line of the left
column takes a remark, deleted or not, and it is anchored on the base. A
rebase that moves that line moves the remark with it.

A range is anchored by its first line, and keeps its length: the last line
follows the first by however many lines it stood below it. The characters
are kept as they were written. Anchoring reads lines, and a line that moved
is still the same line.

An unanchored comment is never dropped and never moved in silence. The
interface lists it in a panel beside the file, with the patch set it was
written against and the text of the line it pointed to.

### 5.4 The second round

You review, an agent corrects the code and amends the commits, you review
again. The remarks of the first round are all still there, and every one of
them is one you have already dealt with.

Each comment records the commit it was written against. Two things follow.

**The version that was reviewed is found on its own.** A commit that carries
a remark and is not the one under review is offered as a patch set, without
`--prev`. The diff between the two is what the correction did.

**A remark that was answered says so.** The line it spoke of is not in this
version any more, and the remark was not written on this version, so the only
thing that can have taken the line away is the work in between. The interface
lists these apart, under **Answered**, with one action that drops them all.

That is a guess, and it is the honest one available: nothing else in the
repository says whether a remark was dealt with. It never deletes anything on
its own.

A version that is not the newest is history. Its remarks are shown and cannot
be edited there: the newest version is where a review is written.

**A remark of an earlier round is shown, and counted nowhere.** The export,
the counts on the series, on the files and on the copy buttons all hold the
remarks of the version under review and no others. The interface still shows
the rest: on the line they anchor to, dimmed and naming their version, and in
the pane under a line that says where they came from. Nothing is hidden, and
nothing counts twice.

**A remark whose line the version has lost falls back rather than gives up.**
The file it was written on is the nearest true place, and when the change no
longer touches that file, the change is. It is never a list off to one side.

## 6. Gerrit integration

### 6.1 Reading the coordinates

| Item | Source |
|---|---|
| Host and port | The `origin` remote URL. `ssh://review.example.com:29418/myproject` gives the host and 29418 |
| Project | The path of the remote URL, without a trailing `.git` |
| Branch | `git show <rev>:.gerrit-branch` when that file exists in the commit, then the configuration, then the upstream branch name |
| Change-Id | The commit trailer |

### 6.2 The query

```sh
ssh -p 29418 review.example.com gerrit query --format=JSON --patch-sets \
    --comments "change:I8f3a...c21 project:myproject branch:rel-3.0"
```

The answer gives the change number and one entry per patch set, with the ref
and the remarks already posted on it. Then, for each patch set the user opens:

```sh
git fetch origin refs/changes/21/12321/1
```

The fetch is lazy. The query alone lists the patch sets. Nothing is downloaded
before the user selects one.

### 6.3 The remarks already posted

`--comments` adds to the query what reviewers have already written on each
version. qreview shows them, read only, beside your own. It never posts,
never replies and never votes.

A server that does not know an option answers nothing at all, and the patch
sets would go with the remarks. So a query the server *refused* is asked again
without `--comments`. One that was never answered is not: nothing came back
the first time, and the reader is already waiting.

The ssh answer gives a file, a line, an author and a text, and nothing else:

| Missing | What qreview does |
|---|---|
| An id | Makes one from the patch set and the place |
| A side | Reads every line as a line of the new side |
| A reply link | Two remarks on one line are a thread, in the order given |
| A timestamp | Shows the patch set instead |

A remark is anchored on the version it was posted on, by the rules of
section 5.3, and follows its line into the version being read. A version that
was never fetched holds no line to hash, so its remarks are unplaced, and the
panel beside the file says so.

Gerrit shows the commit message as `/COMMIT_MSG` with a header of its own.
qreview drops that header, so the two do not count lines the same way. A
remark on the message is read as a remark about the message, not about a line
of it.

### 6.4 Failure

Gerrit is optional at every point. A query that fails, times out, or finds
nothing produces a warning line in the interface and leaves the local review
working. The default timeout is 5 seconds. `--no-gerrit` skips the query.

## 7. Languages and colors

The colors are computed in the server by `syntect`, which reads TextMate and
Sublime grammars. The output is a list of CSS classes per row, never a color.
A theme is therefore a stylesheet, and the light and dark pair costs one
highlight pass, not two.

The map from extension to language is data, not code. Bundled grammars claim
the common extensions on their own. The built-in map only carries the ones
they do not:

| Extension | Language |
|---|---|
| `blk`, `blkk`, `pxc` | c |
| `pyx`, `pxd`, `pxi` | cython, or python when no Cython grammar is bundled |
| `iop` | d |
| `tpl` | jinja |

A code base with its own file types adds them in `.qreview.json` at its top
level, so every reader of that repository gets the map with no setup.
`examples/languages.json` shows the shape.

Three layers decide the language of a file:

1. The user map in the configuration.
2. The default map above.
3. What `syntect` finds from the first line of the file.

An unknown extension falls back to plain text and never fails. The interface
shows the language of the file and lets the reader change it for the session.

**A grammar is data.** A `.sublime-syntax` or `.tmLanguage` file dropped in
`$XDG_CONFIG_HOME/qreview/grammars/` is loaded at startup and can be named in
the language map. A house file format therefore needs no change to this
repository and no rebuild of the binary.

The highlight of a file is computed once per patch set and cached in memory
for the life of the process, keyed by the blob hash. Two patch sets that share
a file therefore highlight it once.

## 8. HTTP API

All routes are under `/api`, all answers are JSON, all errors carry
`{ "error": { "code": "...", "message": "..." } }`.

| Method and route | What it does |
|---|---|
| `GET /api/session` | The repository, the first batch of the series, the tool version and the commit it was built from |
| `POST /api/series/extend` | Load the next batch. Body: `{ "count": 5, "parent": 1 }` |
| `GET /api/changes/:key` | The change, its patch sets, its comment count |
| `PATCH /api/changes/:key` | Mark the change read, or unread |
| `GET /api/changes/:key/files?ps=2&base=parent` | The file list with the statistics, the commit message first |
| `GET /api/changes/:key/diff?ps=2&base=parent&file=...` | The hunks of one file |
| `GET /api/changes/:key/patchsets` | The versions of the change, oldest first |
| `GET /api/changes/:key/posted` | The remarks already on Gerrit, placed |
| `GET /api/changes/:key/mergelist` | The commits a merge brings in |
| `GET /api/comments` | Every comment of the session, in reading order |
| `GET /api/update` | Whether a newer qreview is out. Empty when nothing answers |
| `GET /api/changes/:key/comments` | Every comment of the change |
| `POST /api/changes/:key/comments` | Create a comment or a reply |
| `PATCH /api/changes/:key/comments/:id` | Change the body, or resolve the thread |
| `DELETE /api/changes/:key/comments/:id` | Delete a comment and its replies |
| `POST /api/changes/:key/patchsets/fetch` | Fetch one Gerrit patch set |
| `GET /api/export?scope=change&key=...` | The export text |
| `GET /api/config` | The three layers, folded |
| `PUT /api/config` | Write what the panel changed, and read them back |

Two static routes sit outside `/api`: `/` serves the interface, and
`/theme/<name>.css` serves the stylesheet that names the syntax classes.

`ws=ignore` leaves out what differs only by whitespace. `ps` names the patch
set to read, and the last one is the default. `base`
takes `parent` (the default), `ps:<n>` to read one patch set against another,
and on a merge `automerge` (its default), `parent1` or `parent2`.

One file at a time on the diff route. A change with 200 files must not build
200 diffs to show the first one.

### 8.1 One list, four counts

`GET /api/comments` answers with the comments of every change, in the order
of section 9: the oldest commit first, and inside it the order of the
export. The interface counts from that one answer, and from nothing else:
the two copy buttons, the changes in the series, the files of a change, and
the pane that lists the remarks. Four numbers computed four ways are four
numbers that end up disagreeing.

It also means a count is right the moment a remark is written, which a
number the server put in the file list would not be.

## 9. Export for a Claude session

Two ways out: a button in the interface that copies to the clipboard, and
`qreview export` on the command line.

The format is Markdown, made to be pasted into a session. It opens by saying
where the review comes from and what is being asked, because the thing that
reads it has no other context:

````markdown
## Review: myproject@main, commit af448g1500

I reviewed this commit and left the comments below. Please address them.

Change: net: fix the retry loop
Patch set 2 · 2 comments

1. `src/net.blk:42`

   ```c
   40 |     rc = connect(fd, addr);
   41 |     if (rc < 0) {
   42 |         for (;;) {
   43 |             rc = read(fd);
   44 |         }
   ```

   This loop retries forever when the socket is closed.

2. The change as a whole

   The error paths in this file never log the errno.
````

A series of several changes carries one heading per change under the same
opening. A series with one reviewed change reads as that change, because
calling one commit a series helps nobody.

Rules for the format:

- The comment follows the code, never the opposite. The reader needs the
  context first.
- A place on the old side reads `` `src/net.blk:42` (before the change) ``.
  A deleted line has no number in the new file, and the excerpt above it
  comes from the version before the change.
- No author. One person wrote every line of it.
- The comments are numbered, so the answer can name one.
- The export names the commit and the patch set, so the session knows the
  state the remarks were written against.
- Only the remarks of that version are in it. A round before this one left
  remarks the reader has dealt with, and an agent that reads them again redoes
  work that is done. A line under the count says how many were left out.

The comments are ordered the way a reader walks the code:

- the commits from the oldest to the newest, which is the order they were
  written in and the order they will be corrected in;
- inside a commit, the files in alphabetic order, with the commit message
  before them, and a remark about the whole change before that, because it
  belongs to no file;
- inside a file, top to bottom. Two remarks on one line keep the order they
  were written in.

## 10. Configuration

Three layers, the later one wins:

1. The defaults built into the tool, including the language map.
2. `$XDG_CONFIG_HOME/qreview/config.json`, found with the `directories` crate.
3. `.qreview.json` at the top level of the repository, when it exists.

```json
{
  "languages": { "blk": "c", "pxc": "c" },
  "gerrit": { "enabled": true, "branch": null },
  "series": {
    "maxCommits": 50,
    "guessMax": 10,
    "batchSize": 5,
    "integrationBranch": null
  },
  "ui": { "diff": "unified", "theme": "system" },
  "update": { "url": "https://api.github.com/…/releases/latest", "token": null },
  "diff": {
    "context": 10,
    "wrap": false,
    "ignoreWhitespace": false,
    "tabWidth": 4,
    "fontSize": 12,
    "syntax": true
  }
}
```

Ten lines of context, not the three git gives: three is too few to judge a
change.

`PUT /api/config` writes the `diff` and `ui` sections into the file of the
user, and nothing else. A language map that somebody put there by hand stays
where it is. The answer is the three layers folded again, so it says what the
tool will really use and not what was asked for.

A key nobody knows is refused, and the error names it. A configuration that
is silently ignored is worse than one that refuses to start, because nobody
notices the typo. `examples/` holds a copy of both files.

A language map is merged, not replaced: a repository adds its own file types
without repeating the ones the tool already knows.

`update.url` is the address that says which release is the newest. It
defaults to the releases API of the project on GitHub, which is where the
tool is published. An empty address asks nobody, and that is how the check
is turned off. A fork writes its own.

The address must answer with JSON holding `tag_name`, and `html_url` for
the page to link to. That is what the releases API of GitHub answers with.
`update.token` is sent as `Authorization: Bearer`, for a fork that is not
public.

`curl` asks, with a three second cap, once for the life of the run and only
after the interface has painted. Every failure is silence: no network, a
server that is down, a token that is refused, curl that is not installed.
Nothing of that is worth a word to a reader who came to read a diff.

### 10.1 What the browser keeps for itself

Some state belongs to the screen in front of the reader, not to the tool.
The browser keeps it under `qreview.*` in its own storage, and the
configuration file never sees it:

| Key | What it holds |
|---|---|
| `qreview.side` | The series pane is shown or hidden |
| `qreview.side.width` | How wide the series pane is |
| `qreview.comments.height` | How tall the list of comments is |
| `qreview.drafts` | Remarks typed and not saved, by change, file and line |

A size dragged with the mouse would be a poor thing to write to a file that
the command line reads, and the same file on two screens would be wrong on
one of them. Losing this state costs a drag, and nothing else.

## 11. Command line

```
qreview                       review the current series
qreview <rev>                 review one commit
qreview <revA>..<revB>        review a range
qreview <rev> --prev <sha>    add a local patch set, repeatable
qreview --base <rev>          set the base of the series
qreview --no-gerrit           skip the Gerrit query
qreview --no-open             start the server, print the URL, open nothing
qreview --port <n>            use a fixed port
qreview export [--key <id>]   print the export text to stdout
qreview list                  list the stored reviews of this repository
```

## 12. Build and distribution

`make` is the entry point. It calls `npm` for the interface and `cargo` for
the rest.

| Target | What it does |
|---|---|
| `make setup` | `npm ci`. Needed once, and after a lockfile change |
| `make dev` | The Rust server on a fixed port, and Vite with a proxy on `/api` |
| `make build` | `vite build`, then `cargo build --release` |
| `make check` | The whole gate. The only command to run before a commit |
| `make test` | `cargo test` and `vitest run` |
| `make e2e` | The browser tests, against the real binary |
| `make shots` | Screenshots of the interface, into `web/e2e/.shots` |
| `make fmt` | `cargo fmt` and `prettier --write` |
| `make install` | The binary into `~/.local/bin` |
| `make dist` | The `x86_64-unknown-linux-musl` binary, for a colleague. It adds the target if it is missing |

`make check` runs, in this order: `npm run lint`, `npm run format:check`,
`npm run test:run`, `npm run build`, `cargo fmt --check`, `cargo clippy -- -D
warnings`, `cargo test`, `cargo build --release`, then the browser tests.

**The interface is built before every Rust step.** The binary embeds
`web/dist`, so nothing on the Rust side compiles until that directory exists.
`npm run build` also runs `vue-tsc`, so the type check has no step of its
own.

Node is needed to **build** the interface. It is not needed to run qreview.
The release artifact is one static binary.

**The static build needs no C toolchain.** Rust links `musl` with its own
bundled linker and crt objects, so `musl-gcc` is not a prerequisite. Measured
on the scaffold: 1.9 MB, `statically linked`, and it serves the interface with
`web/dist` deleted.

The target itself belongs to rustup, not to asdf. `.tool-versions` pins a Rust
version and knows nothing about targets, so `make dist` runs `rustup target
add` when the target is missing. asdf installs Rust through rustup and points
`RUSTUP_HOME` at its own directory, so this works under asdf, and the target
lands inside the asdf install. Only `dist` does this. `build` never touches
the toolchain.

A debug build serves the interface from `web/dist` on disk. A release build
embeds it. So `make dev` gives Vite hot reload against the real server, and
`make build` gives the single file.
