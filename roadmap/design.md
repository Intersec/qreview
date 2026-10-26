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
binary. The process stops with Ctrl-C. A closed page does not stop it, so
an agent can go on. The server says in its terminal when the page opens.
It says when the page closes, with the URL to open it again, and when the
page comes back. The page counts as closed when no page has held a wait on
the events (section 8.3) for 3 seconds, so a reload is not a close. The
server also says when an agent connects, and when it disconnects, by the
rule of section 8.3 for an agent that listens.

On a terminal, a status line under these messages says how the review
stands now: the page (not open yet, open, closed) and the agent (absent,
connected, fixing or answering on a change named by its subject, left).
The server redraws it every second, in place, and clears it when it stops.
In a file or a pipe, and under `--trace`, whose lines would break into it,
there is no status line: the messages are all there is. `NO_COLOR` takes
the colors away.

The server binds the loopback address only. It generates a random token at
startup. The first page request carries the token in the query string, and the
server answers with a session cookie. A request without the cookie or the
token gets 401. This stops another local user, and a page in another tab, from
reading the repository.

The token is also in `server-<worktree-id>.json` (section 11), a file only
its owner can read, so the commands of an agent in a terminal of the same
user reach the server. Another local user still cannot.

The server also listens on a Unix socket,
`/tmp/qreview-<uid>/<repo-id>-<worktree-id>.sock`, for the commands of an
agent. An agent in a sandbox runs each command in a
network of its own, where `127.0.0.1` is not the machine of the user, but it
shares `/tmp`. The directory is made with mode `0700` and refused when
another user owns it or others can enter it; the socket has mode `0600`.
The token is asked for there as on the port. The browser uses the port.

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
6. The merge base with the branch **Gerrit** says the change is on. Asked
   only when rules 4 and 5 answered nothing, or answered further than
   `series.maxCommits`.
7. No base resolves. The tool guesses. See below.

Rule 6 exists because `.gerrit-branch` names the *integration* branch. Work
pushed to a feature branch under it carries that same file, so rule 5 lands
far below the start of the series, and nothing in a clone can tell which
branch a change was pushed to. Gerrit can. A base within `maxCommits` is
trusted as it is, so a repository whose local rules work pays no round trip,
and a server that says nothing leaves the base rule 5 found.

A resolved base is never thrown away. The first batch loads at most
`series.maxCommits` commits of it (50 by default) and stops on a `batch`
boundary when the base is further, with the number of commits left on the
card. A base is where the series ends; a long one is read in pieces, and it
is always worth more than a guess.

#### The guess, when no base resolves

The guess is best effort and it says so. It loads at most
`series.guessMax` commits (10 by default) and stops at the first of these
signals:

| Order | Signal | Why it ends the guess |
|---|---|---|
| 1 | A merge commit | Another line of history starts here |
| 2 | A tag points at the commit | A release is a history boundary |
| 3 | A remote-tracking ref reaches the commit, and that ref does not reach the head | Work already on a branch of the server is rarely the series you are about to review |
| 4 | The author is not the author of the head of the series | The walk reached shared work |
| 5 | `series.guessMax` commits are loaded | The cap |

A ref that reaches the head reaches the whole series, so signal 3 skips it.
The branch a series is pushed to is exactly such a ref, and without this the
guess ends under the head of every pushed branch. Gerrit bounds a push with
the branch heads and never with the ref being pushed; this is the same rule.

Signal 4 compares against the author of the head, not against `user.email`.
A series belongs to whoever wrote its newest commit, and a reader reviews the
work of other people.

Signals 3 and 4 end the **first batch** only. They never end a later batch,
and they never draw a boundary card of their own. The reason is that both are
wrong often enough: a pushed commit can be under review, and a commit of
somebody else can sit inside a series. Once the reader has asked for more,
the count is the only bound left.

#### Loading more

Every stop carries a **Load up to 5 older** action. The batch size after the
first is `series.batchSize`, 5 by default, and the label carries the number
that is configured. The walk can still meet a boundary before it reaches the
count, so the label says **up to**.

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
| Batch | The number loaded, and how far the base still is when one is known |
| Root | The history has no parent left |

Every card but the root also names the commit the button would load first:
its short hash and its subject. `boundary.commit` is that commit, and it is
not loaded yet.

A `batch` card under a known base carries `remaining`, and a second button,
**Load the rest (N)**. The end is a number there, not a walk, so reaching it
is one click. The number is on the button because a base that is wrong is a
base that is far, and the reader sees that before clicking.

The merge itself is loaded, and the walk stops **under** it. The card sits
below the merge in the list, and `commit` names the first parent of the
merge. The commits the merge brings in are another line of history, and
nothing loads them.

Every card carries **Load up to 5 older**, which follows the first parent. A
merge card also carries, when the second parent is not a remote branch,
**Follow the other parent**.

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
| A declared link | A commit that `qreview migrate` named as the previous version of this one. See section 5.6 |
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

A merge is a change of the series like any other. Its bases sit in the base
selector, beside the patch sets, the way Gerrit puts them there:

| Base | What it shows |
|---|---|
| The auto-merge | The conflict resolution. The default |
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

type RepoInfo = {
  root: string;            // the top level directory
  remote: string | null;   // the canonical remote URL, when there is one
  id: string;              // what the comment store is keyed by
  name: string;            // what to call it: the project, or the directory
};

type Boundary = {
  kind: 'merge' | 'tag' | 'base' | 'guess' | 'root';
  commit: string;          // the commit under the boundary, not loaded yet
  subject: string | null;  // its subject, so the card says what comes next
  remaining: number | null;// commits left to the base, on a short batch
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
  parents: string[];       // the parents of the commit, in order
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
      server-<worktree-id>.json    where the server of a worktree listens (section 11)
      lock                         held for each read and write of a change file
      changes/
        I8f3a...c21.json           one file per change
        sha-4a91...f0.json
      commits/
        4a91...f0.json             one file per commit that has a declared link
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
  "version": 4,
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
      },
      "author": "reader",
      "done": false
    },
    {
      "id": "c-01J...",
      "parent": "c-01H...",
      "author": "agent",
      "patchSet": 2,
      "commit": "af448g15...",
      "createdAt": "2026-08-20T14:05:40Z",
      "updatedAt": "2026-08-20T14:05:40Z",
      "scope": "line",
      "body": "Should a closed socket end the loop, or retry the connect?",
      "blocked": true
    }
  ]
}
```

Format 4 added `author`, `parent`, `done` and `blocked`, and `moves`, the
earlier places of a remark that `qreview migrate` moved (section 5.6). Each
one carries a default, so a file of format 3 reads as a set of remarks by
the reader, each alone in its thread, none of them done, none of them moved.

The versions of a change are not stored. They are read from git, and the
`commit` of each comment says which one a remark was written against. See
section 5.4.

`scope` is `line`, `range`, `file`, or `change`. A `file` comment has an anchor
with no line. A `change` comment has no anchor.

Nothing writes a `change` comment any more. Gerrit has no useful place for a
remark about the whole change — you cannot reply to one — so the convention is
to write it on the commit message, which is a file like any other. That is
what a session already reads, and what the export already lists first. The
ones a `change` scope left behind are still read, and shown on the commit
message with the rest.

A `range` covers `startLine` to `endLine`. It can also open and close inside a
line: `startChar` and `endChar` then hold the first character on the first
line and the one after the last on the last line. Both count **UTF-16 code
units**, the units the browser measures a selection in, and the units every
offset that crosses the wire already uses. A range with no characters covers
whole lines.

A comment can open a thread. The reader is one author; an agent that works
on the same series in a terminal is the other. Both write remarks, both
reply, and either one closes a thread with Done. See section 5.5.

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

**A remark names the version it was written on, and that is the only word on
whether it still counts.** A remark written on the sha the change carries now
is a **current** one. Any other is a **previous** one: the round it belongs to
is over. Nothing guesses whether the work was done; the sha is a fact, and the
tool says no more than it knows.

A version that is not the newest is history. Its remarks are shown and cannot
be edited there: the newest version is where a review is written.

**A previous remark is shown, and counted nowhere.** The export, the counts
on the series, on the files and on the copy buttons all hold the current
remarks and no others: a round that is over must not be handed to an agent
again, or it does work that is done. The remarks Gerrit holds are counted
nowhere either, for the same reason and by the same rule.

**The diff shows the remarks of the version on the screen, and no others.** A
remark of another version speaks of code that is not there. Open the patch set
it was written on and it is on its line, grey rather than the draft yellow,
tagged `previous`, naming the patch set and the sha. Nothing is hidden: the
pane lists every remark of the change, under the version each one belongs to.

Two versions read one against the other are both on the screen. A remark of
the version on the left stands in the left column, on its own line, the way
section 6.3 places a remark of Gerrit. A remark on a removed line of that
version speaks of its parent, which no column shows, so the diff leaves it
out.

**A previous remark is read, and never written on.** It has no Edit and no
Delete: the round it belongs to is over, and a record that can be rewritten is
not a record. A version that is not the newest takes no new remark either: one
written there would be anchored on the newest version, at the line numbers of
the one on the screen.

**A change with no `Change-Id` finds its earlier versions in the reflog**,
when no link of section 5.6 names them.
The `Change-Id` is what keeps a review under one key across an amend. Without
one the key follows the sha, so the round before sits under the key of a
commit nothing points at. Git has not lost that commit: an amend stops
pointing at it, and the reflog keeps the pointer.

A commit of the reflog is an earlier version of a change when it carries the
same subject, when the series cannot reach it — a commit of the history that
shares a subject is left alone — and when that subject names exactly one
change of the series. It becomes a patch set, and its remarks are read as
remarks of that change.

Nothing is moved. The store keeps every file where it is, and a link that is
wrong costs a patch set in a list and a read-only remark from a round that is
over. A change that carries a `Change-Id` is left out of this: its identity is
in its message, and guessing beside that is only a way to be wrong.

**A change opens on its newest version, always.** The number of a patch set
means nothing across changes: patch set 4 of one is not the work of patch set
4 of another, and a reader who opened an old version of one change did not ask
for an old version of the next.

**A remark whose line the version has lost falls back rather than gives up.**
The file it was written on is the nearest true place, and when the change no
longer touches that file, the commit message is.

Everything that belongs to no line of the file stands **before the first
line**, inside the code, the way Gerrit puts a remark about a file above line
1. A band above the diff took the top of every screen, on every file, whether
it held anything or not.

### 5.5 Threads and the agent

An agent, such as a Claude Code session, works on the series in a terminal.
It starts qreview, writes remarks, answers the ones of the reader, and
amends the commits itself. qreview never starts the agent and never calls a
network service for it: it reads what the agent commits, and it stores what
the agent writes through the commands of section 11.

**A thread is a remark and its replies.** A reply names its remark in
`parent`, and only a remark that opens a thread is a parent: a reply to a
reply is a reply to the thread. A reply carries no anchor and no scope of
its own; it stands where its thread stands. Its `commit` is the version the
change carried when it was written. The round a thread belongs to is the
`commit` of its remark, and section 5.4 reads that one only.

**`author` is `reader` or `agent`.** The commands of section 11 send
`"author": "agent"`; the interface sends no author, and a write without one
is the reader's. Both hold the same session token, so this is a label and
not a permission. The interface draws the writing of the agent in a style of
its own.

**`done` is on the remark that opens the thread**, the way Gerrit has a
Done box on a thread. The agent or the reader checks it. A done thread is
grey and folded, and one click on its header folds or unfolds it without a
change of state. Clearing the box opens the thread again, and so does a new
reply of the reader.

**A done thread is counted nowhere and left out of the export.** The work is
done; the agent must not get it again. It is still on its line, and it is
never deleted by qreview on its own.

**`blocked` is on a reply of the agent.** It says that the agent needs the
reader before it can change the code: it asked a question, or it met a
problem it cannot solve alone. A thread is blocked while its last reply is a
blocked one, so the next reply of the reader ends it, with no flag to clear.

A blocked thread asks for the reader:

- it opens unfolded, in a strong color;
- its change in the series and its file in the file list carry a mark;
- the title of the browser tab starts with the number of blocked threads, so
  the reader sees it from another window.

**The agent answers when the reader asks, and not before.** Every write of
the reader reaches the page at once, but `qreview wait` returns on a
request alone: **Ask agent** on a thread, or **Save and ask** in a box.
Ctrl+Shift+Enter asks too: in a box it saves first, and on a line it asks
about the thread under the cursor. The request names one thread. The reader writes,
changes and deletes remarks in peace, and a remark costs the agent nothing
until it is asked about.

**A previous thread is read, and never written on**, the rule of section
5.4. It takes no reply and no change of `done`.

**Deleting a remark deletes its thread.** A reply deleted alone leaves the
thread as it was.

### 5.6 New versions, declared

An agent that amends a commit knows which commit the new one replaces. It
says so, and qreview stops guessing:

```
qreview migrate --from <old sha> [--to <new sha>] [--done <id>,...] [--blocked <id>=<text>]...
```

**The link belongs to the commits.** `migrate` writes one small file per
commit under `commits/`:

```json
{ "previousVersion": "4a91...", "nextVersion": null, "declaredAt": "2026-10-03T14:02:11Z" }
```

The old commit gets `nextVersion`, the new one `previousVersion`. The
versions of a change are the chain these links make, walked back from the
commit under review. Each one is a patch set, and its remarks are read as
remarks of the change, whatever key they were written under. A change needs
no `Change-Id` for this. The reflog guess of section 5.4 stays for an amend
that nobody declared.

`migrate` refuses, and changes nothing, when:

- the old commit already has another next version: one commit, one next;
- both commits carry a `Change-Id` and the two differ. When one is present
  it is a check, never a requirement.

**The threads move with the declaration.** Every thread of the old version,
open and done, moves to the key of the new commit: its `Change-Id`, or
`sha-<new>`. A remark gets the new `commit` and the new anchor, and its
earlier place goes onto `moves`:

```json
"moves": [{ "commit": "4a91...", "anchor": { ... }, "movedAt": "..." }]
```

So the move is explicit, stored, and readable afterwards: the rule that a
comment is never moved in silence holds. A done thread moves too, grey and
folded, until the reader cleans it. Older rounds stay on their own version.

**The old version keeps its record.** A thread that moved is stored once,
on its new version. Read on a version it left, it stands there again: at
its place in `moves`, with the replies written on that version, read only,
and a tag that says where it went and leads there. The comments route
builds these copies for the versions on the screen; each takes the id
`<id>@<sha>`, and `moved` in the answer maps it to the thread and the
version it went to. Nothing stores them, counts them, or exports them.

The new key is written before the old one is emptied. A write that fails
between the two leaves a thread in both places, never in neither.

**The new line of a remark** comes from the first of these that gives one:

1. A line map built from `git diff <old> <new> -- <file>`, `-M` on, run as a
   child process so it follows `diff.algorithm`. A line outside the hunks
   moves by the offset of the hunks above it. A line inside a hunk was
   changed by the new version, and goes to the first line of the new text of
   that hunk, or to the line before the hunk when the hunk only removes.
2. The fallback of section 5.4: the file, then the commit message.

A remark on the old side reads the parents of the two commits instead. A
range keeps its length, as in section 5.3.

`--done` and `--blocked` reply as the agent, on the new version, after the
move. One command, so the reader never sees the threads half moved.

**The unsaved remarks move too.** The browser keeps what the reader typed
and did not save (section 10.1), and the server never sees it, so a migrate
cannot move it. The `migrated` event names both versions and the key the old
one was kept under; the page asks `POST /api/map` where each of its places
landed, by the same line map, and moves each draft there. A draft whose file
the new version deleted goes to the commit message, with a first line that
says where it was typed. The drafts of a reply or an edit are kept under the
id of their thread, which a move does not change, so they need nothing.

### 5.7 Fix and Clean

**Fix** asks the agent to correct the change on the screen. A button, and
`a` outside a text field, the key that opens the reply dialog in Gerrit.

- It opens a dialog that lists the open threads the agent will deal with:
  the place and the first line of each. `Enter` sends, `Escape` closes, and
  no other key does anything. A key typed by mistake opens the dialog and
  sends nothing.
- If a thread of the change waits for the reader, the dialog lists those
  threads instead and sends nothing. `Enter` jumps to the first one.
- Without an agent, the button is disabled and `a` opens nothing. The server
  knows an agent listens while a `qreview wait` is open, or ended shortly
  before (section 8.3), or while a fix runs, and says so on every answer of
  `/api/events`.
- Sending writes a `fix` event, author the reader, naming the change.
  `qreview wait` returns it like any other.

**A fix runs until the agent waits again.** The agent works with no wait
open, for minutes, so the end of its work is the next wait it opens. The
server then writes an `idle` event, once, when the agent wrote since its
last wait. Until then every answer of `/api/events` carries `"working":
true`, and the agent counts as listening, for 30 minutes at most: past
that, it is taken for gone.

While the agent works, the page holds what the agent writes: it notes which
change to read again, and whether a migrate asks for the whole series, and
reads nothing. It says **Agent working…** beside a disabled Fix. When the
work ends, it moves the drafts (section 5.6) and reads once. A fix longer
than ten minutes is shown once every ten minutes, and held again, so a long
fix shows where it stands without a reload at each step. A page opened
during a fix learns of it from the first answer of `/api/events`.

**A reader who looks elsewhere is told.** When a fix ends, or the agent
answers outside one, and the page does not have the focus, the page shows
a notification of the browser: the fix is done, or the agent answered, and
how many questions wait. The browser asks for leave on the first Fix or
Ask, the clicks that start work for the agent, and its answer is the only
setting.

**The tab title says whose turn it is.** It starts with ⏳ while the agent
works on a fix or on a question asked, and with ◑◒◐◓, turning once a
second, once the agent is done and until the page has the focus again.
While a question of the agent waits, the turn stays the reader's whatever
the focus: looking at the page answers nothing. A tab among many, or a
notification refused, still says it.

**Clean** deletes for good every done thread of the change on the screen. A
thread reopened since is not done, and stays. The browser waits three
seconds with an Undo before it asks the server, because nothing brings a
deleted thread back.

## 6. Gerrit integration

### 6.1 Reading the coordinates

| Item | Source |
|---|---|
| Host and port | The `origin` remote URL. `ssh://review.example.com:29418/myproject` gives the host and 29418 |
| Project | The path of the remote URL, without a trailing `.git` |
| Branch | `git show <rev>:.gerrit-branch` when that file exists in the commit, then the configuration, then the upstream branch name |
| Change-Id | The commit trailer |

The list of versions is ordered the way they were written, and the commit
under review is last whatever the dates say: it is what a reader opens on.
Gerrit owns the numbering of the versions it knows, because those numbers are
what a reviewer on the server sees. A version Gerrit never saw is not a patch
set of the change and takes no number of one: the interface calls it **local**.

Each version says where it is, because the two facts are separate. The name
says whether the server knows it: `Patch set 3`, or `Local`. The mark after
the date says when the commit is not in this clone: `· not fetched`, and a
version that is here carries nothing. A mark that reads the same on every
version says nothing at all, and an amended-away version is often still in the
object database, so `here` was on most of them.

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
| A side | Reads every line as a line of the version it was posted on |
| A reply link | Two remarks on one line are a thread, in the order given |
| A timestamp | Shows the patch set instead |

A remark shows on the version it was posted on and nowhere else, the way
Gerrit shows it:

| The remark was posted on | Where it shows |
|---|---|
| The version on the right | The right column, on its line |
| The patch set on the left | The left column, on its line |
| Neither | Not in this diff |

A remark of another version speaks of code that is not on the screen, the
rule of section 5.4. It is anchored on its own version by the rules of
section 5.3. A version that was never fetched holds no line to hash, so its
remarks are unplaced, and they stand at the top of their file.

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

**The binary carries one grammar of its own, for IOP.** The bundled set has
none. The grammar lives in `crates/qreview/grammars/`. It sits in a set of its
own: a grammar added to the bundled set costs 750 ms at every start. The
grammars of the user join the bundled set, and the lookup asks that set
first, so a grammar of the user wins.

The highlight of a file is computed once per patch set and cached in memory
for the life of the process, keyed by the blob hash. Two patch sets that share
a file therefore highlight it once.

**A parse stops where the reader stops.** It always starts at line 1, because
a block comment or a multi-line string needs the lines before it, but it goes
no further than the deepest line a diff shows. The state where it stopped is
kept beside the spans, so opening more context carries on instead of starting
again. A change near the top of a file of ten thousand lines therefore costs
the top of it and nothing else.

**A long run is read without colors.** A run of context of more than 2000
lines comes back with no syntax spans, and the interface then drops the colors
of the whole file, hunks included, for as long as such a run is open. The
spans are most of what a row weighs on the wire and each one is a DOM node, so
this is what lets a reader open a gap of any length at all. It covers the
whole file because a table colored in patches reads as a defect rather than as
a rule, and folding the gap back brings the colors straight back: they never
left the browser.

## 8. HTTP API

All routes are under `/api`, all answers are JSON, all errors carry
`{ "error": { "code": "...", "message": "..." } }`.

| Method and route | What it does |
|---|---|
| `GET /api/session` | The repository, the first batch of the series, the tool version and the commit it was built from |
| `POST /api/session/refresh` | Resolve the series again, and answer with what git holds now |
| `POST /api/series/extend` | Load the next batch. Body: `{ "count": 5, "parent": 1 }` |
| `GET /api/changes/:key` | The change, its patch sets, its comment count |
| `PATCH /api/changes/:key` | Mark the change read, or unread |
| `GET /api/changes/:key/files?ps=2&base=parent` | The file list with the statistics, the commit message first |
| `GET /api/changes/:key/diff?ps=2&base=parent&file=...` | The hunks of one file |
| `GET /api/changes/:key/lines?file=...&from=1&to=400` | A run of lines the diff left out, so the reader can open the context. A run of more than 2000 lines carries no syntax spans |
| `GET /api/changes/:key/patchsets` | The versions of the change, oldest first |
| `GET /api/changes/:key/posted?ps=2&base=ps:1` | The remarks already on Gerrit on the two versions read, each placed in its column |
| `GET /api/changes/:key/mergelist` | The commits a merge brings in |
| `GET /api/comments` | Every comment of the session, in reading order |
| `GET /api/update` | Whether a newer qreview is out. Empty when nothing answers |
| `GET /api/changes/:key/comments` | Every comment of the change, placed on the version read, and the copies of the threads that moved away from a version on the screen. See section 5.6 |
| `POST /api/changes/:key/comments` | Create a remark, or a reply when the body names a `parent`. `"author": "agent"` writes as the agent, and a reply may carry `"done": true` or `"blocked": true`. `"ask": true` asks the agent about the thread once it is saved |
| `POST /api/changes/:key/comments/:id/ask` | Ask the agent to answer the thread of a comment. See section 5.5 |
| `PATCH /api/changes/:key/comments/:id` | Change the body, or check or clear `done` on a thread |
| `DELETE /api/changes/:key/comments/:id` | Delete a comment and its replies |
| `GET /api/events?after=<n>` | Wait for the next writes to the store, and answer with them. See section 8.3 |
| `POST /api/migrate` | Link a new version to its previous one and move its threads. Body: `{ "from", "to", "done": [ids], "blocked": [{ "id", "body" }] }`. See section 5.6 |
| `POST /api/map` | Where places of one version stand in the next. Body: `{ "from", "to", "places": [{ "file", "side", "line" }] }`. Answers `{ "places": [...] }`, `null` for a file the new version deleted. See section 5.6 |
| `POST /api/changes/:key/fix` | Ask the agent to fix the change. See section 5.7 |
| `POST /api/changes/:key/clean` | Delete the done threads of the change |
| `POST /api/changes/:key/patchsets/fetch` | Fetch one Gerrit patch set |
| `GET /api/export?scope=change&key=...` | The export text |
| `GET /api/config` | The three layers, folded |
| `PUT /api/config` | Write what the panel changed, and read them back |

Two static routes sit outside `/api`: `/` serves the interface, and
`/theme/<name>.css` serves the stylesheet that names the syntax classes.

`ws=ignore` leaves out what differs only by whitespace. `ps` names the patch
set to read, and the last one is the default. `base`
takes `parent` (the default), `ps:<n>` to read one patch set against another,
and on a merge `automerge` (its default) or `parent<n>`, one for each parent
the merge carries.

One file at a time on the diff route. A change with 200 files must not build
200 diffs to show the first one.

`GET /api/session` catches up with the working tree, and nothing else: a page
load must not walk the history a second time. `POST /api/session/refresh` is
the reader asking outright, so it resolves the head and the base again, and
replays the batches the reader had loaded.

### 8.1 One list, four counts

`GET /api/comments` answers with the comments of every change, in the order
of section 9: the oldest commit first, and inside it the order of the
export. The interface counts from that one answer, and from nothing else:
the two copy buttons, the changes in the series, the files of a change, and
the pane that lists the remarks. Four numbers computed four ways are four
numbers that end up disagreeing.

It also means a count is right the moment a remark is written, which a
number the server put in the file list would not be.

### 8.2 The place in the fragment

The address bar says where the reader is:

```
#change=<key>&file=<path>&ps=<n>&base=<base>
```

`file`, `ps` and `base` are there only when they say something. A path keeps
its slashes and `ps:2` its colon, because neither one means anything in a
fragment; every other character is escaped, so a file with a `&` in its name
comes back whole.

The browser then owns the history of the review. Back and forward walk the
changes, the files and the versions that were read, and a reload comes back
to the last place rather than to the newest change.

The fragment, not the path, for two reasons. The query already carries the
session token, and the server answers one address: a path of its own would
need a route for every place, and a reload would ask the server for a page it
does not have.

A move writes twice. The place the reader is going to is pushed when they
click, so Back answers at once, and the place the move landed on replaces
that entry. Opening a change lands on a file of it, and that is one place,
not two.

A fragment that names a change the series does not hold is not a failure. The
newest change opens, the way a run with no fragment starts.

### 8.3 Events

Two parties write to the store now, and each must see what the other wrote
without a reload. The server is the only writer of the store, so it knows
every write the moment it is made. It numbers them, and keeps the last few
hundred in memory.

`GET /api/events` with no `after` answers at once, with no event and the
number to start from. `GET /api/events?after=<n>` answers at once with every
event numbered above `n`, or holds the request until one comes, for at most 30 seconds, and then
answers with an empty list. Each answer carries the number to ask after
next, `agent`, true while an agent listens, `working`, true while it
works on a fix (section 5.7), and `left`, true when an agent listened to
this server and listens no more. An agent listens while a `qreview wait`
is open, during a fix, and for ten minutes after an `ask` that it has not
answered with a new wait: a long answer is not leaving. After a wait that
answered, it listens for 30 seconds more, the time to start the next one.
After a wait that it hung up on, its `qreview wait` stopped, and it listens
for 5 seconds more. An agent that arrives or hangs up wakes the waits of
the pages, and a page wait answers when that grace runs out, so the page
hears of it at once and not at the end of its wait.

`task` names what the agent does: `{ "kind": "fix", "key" }` during a fix,
`{ "kind": "answer", "key", "id" }` after an `ask` that reached a listening
agent, and `null` otherwise. A fix comes first: a question asked during a
fix does not hide it. A new wait of the agent ends the task. The top bar of
the page names the task, with the subject of the change or the start of
the remark, and a click on it opens the change or the thread.

While `agent` is true and no task runs, the top bar of the page says that
the agent is connected. When `agent` turns false, the page sends a browser
notification. While `left` is true, the page says that the agent stopped
listening, and to ask it to run `qreview wait` again. An `after`
the server no longer holds, because it restarted or the
event fell out of memory, answers with `"reset": true`, and the reader reads
everything again.

```ts
type Task = { kind: 'fix' | 'answer'; key: string; id: string | null };

type Event = {
  seq: number;
  kind: 'comment' | 'reply' | 'edited' | 'done' | 'deleted' | 'refresh' | 'ask'
      | 'migrated' | 'fix' | 'clean' | 'idle';
  author: 'reader' | 'agent';
  key: string;             // the change, empty on a refresh and an idle
  id: string | null;       // the comment, absent on a refresh
  from?: string;           // on a migrated: the old version,
  to?: string;             //   the new one,
  fromKey?: string;        //   and the key the old one was kept under
};
```

A write names its author: in the body of a comment or of an edit, and as
`?author=agent` on a delete and on a refresh, which carry no body.

The interface holds one such request open, with `who=reader`, and reloads the comments of a
change when an event of the agent names it, or once at the end of a fix
(section 5.7). `qreview wait` holds one too, with `who=agent`, and
returns on the first request of the reader: an `ask`, never a mere write. One route serves both, so there is
one way to learn of a write.

When the wait of the page reaches no server, or the server refuses its
token, the server stopped. The page asks once more a second later, then
stops listening. A new server has a new token, so this page never reaches
it again. The page then closes its tab, unless the browser holds unsaved
remarks. A browser lets a script close only a tab that a script opened, or
a tab with one entry in its history. Where the tab stays, a banner says
that the server stopped, and to close the tab, and the title of the tab
starts with `⏹`.

The icon of the tab carries a dot for the state of the review, the most
urgent first: red when the server stopped, blue while the agent works on a
task, orange when the agent left, green while it listens. With no agent,
the icon is the plain logo. A reader sees it from another tab, where the
top bar is out of sight.

## 9. Export for a Claude session

Two ways out: a button in the interface that copies to the clipboard, and
`qreview export` on the command line.

The format is Markdown, made to be pasted into a session. It opens by saying
where the review comes from and what is being asked, because the thing that
reads it has no other context:

````markdown
## Review: myproject@main, commit af448g1500

I reviewed this commit and left the comments below. Please address them.
Each comment is about the lines marked `>`; the lines around them are
context only.

Change: net: fix the retry loop
Patch set 2 · 2 comments

1. `src/net.blk:42`

   ```c
     40 |     rc = connect(fd, addr);
     41 |     if (rc < 0) {
   > 42 |         for (;;) {
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
- The lines a comment covers carry a `>` at the left edge of the excerpt,
  and the opening says so once. A reader that does not count gives every
  line the same weight, and a remark then lands on the line it fits best
  rather than on the one it was written on.
- A place on the old side reads `` `src/net.blk:42` (before the change) ``.
  A deleted line has no number in the new file, and the excerpt above it
  comes from the version before the change.
- A range that opens or closes inside a line quotes the text it covers,
  after the place: `` `src/net.blk:42`, on `for (;;)` `` on one line, and
  `` `src/net.blk:42-43`, from `for (;;) {` to `rc = read(fd);` `` over
  several. The text, not the columns: the reader can act on the text, and a
  column is a count in units it does not know. Bounds that fall on the ends
  of the lines quote nothing, because the lines say it already. A text that
  occurs more than once on its line says which one: `` on the second `%d` ``.
- A remark alone, by the reader, carries no author: it reads the way it did
  before threads existed. A remark of the agent opens with
  `(written by the agent)`.
- A reply follows its remark, in the order it was written, as
  `Reply from the reader:` or `Reply from the agent:` and its body. A blocked
  reply of the agent adds `(waiting for an answer)`.
- When a thread holds a reply, the opening adds one sentence: "Some comments
  hold a conversation; when the last reply is a question to you, answer it
  instead of changing the code."
- Only open threads are in it. A done thread is left out, the way a previous
  one is, and the line under the count says how many.
- The comments are numbered, so the answer can name one.
- The export names the commit and the patch set, so the session knows the
  state the remarks were written against.
- Only the current remarks are in it: the ones written on the sha the change
  carries now. A previous round is over, and an agent that reads it again
  redoes work that is done. A line under the count says how many were left
  out.

The comments are ordered the way a reader walks the code:

- the commits from the oldest to the newest, which is the order they were
  written in and the order they will be corrected in;
- inside a commit, the files in alphabetic order, with the commit message
  before them, and a remark about the whole change before that, because it
  belongs to no file;
- inside a file, top to bottom. Two remarks on one line keep the order they
  were written in.

**`qreview export --json`** prints the same threads, in the same order, for
a program. The Markdown names a comment by its number, which changes when a
remark is added; the JSON names it by its `id`, which a command of section
11 can act on.

```ts
type ExportJson = {
  repo: string;
  changes: {
    key: string;
    subject: string;
    commit: string;
    patchSet: number;
    threads: {
      id: string;
      place: string;       // "src/net.blk:42", the way the Markdown says it
      side: 'old' | 'new' | null;
      line: number | null;
      excerpt: string;     // the excerpt of the Markdown, `>` marks included
      blocked: boolean;
      comments: { id: string; author: 'reader' | 'agent'; body: string }[];
    }[];
  }[];
};
```

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
| `qreview.drafts` | Remarks typed and not saved, by change, file and line, and replies and edits by thread |

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
qreview -v                    print the series and the files before the URL
qreview --trace               print on standard error what the work costs
qreview export [--key <id>]   print the export text to stdout
qreview export --json         the same threads as JSON, with their ids
qreview list                  list the stored reviews of this repository
qreview skill                 print the procedure of an agent, for this release
```

**The commands of the agent.** They write as the agent, through the server
that runs on the repository:

```
qreview comment <place> --body <text> [--key <id>]
                              write a remark on the newest version
qreview reply <id> --body <text> [--done] [--blocked]
                              reply to a thread
qreview wait [--after <n>] [--timeout <s>]
                              print the next requests of the reader as JSON
qreview refresh               read the repository again, as the ⟳ button
qreview migrate --from <old> [--to <new>] [--done <id>,...] [--blocked <id>=<text>]...
                              declare a new version and move its threads
```

`<place>` is `<file>`, `<file>:<side>:<line>` or `<file>:<side>:<from>-<to>`,
with `<side>` `old` or `new`, the place of section 4. A file alone writes a
remark about the file. A path may hold a colon: the side and the line are
read from the right. `--key` names the change; without it, the change is
the newest one of the series that touches the file. A body of `-` is read
from standard input, so a long answer needs no quoting.

`comment` and `reply` print the comment they wrote, its `id` included.
`wait` prints `{ "events": [...], "next": <n> }`. It returns on a request
only: an `ask` event, never a remark being written, changed or deleted.
Each request carries `thread`, the remark and its replies in order, empty
when the thread was deleted since. `next` is the `--after` of the next
wait, so no request made between two waits is missed. Without `--after`, it waits
from now. It exits 0 with events, and 1 when the time runs out, 600 seconds
by default. A `"reset": true` says the server restarted: read the review
again with `export --json`.

`wait` stays attached across a stop of the server. When no server answers,
it looks for one every second until the time runs out, so it attaches to a
server that the user starts again. When the server it waited on stops and
another one starts, it answers `"reset": true`. When no server comes before
the time runs out, it fails with the message to run `qreview`.

**The server of a worktree.** The worktrees of a clone share one store
(section 5.1), but each one runs a server of its own. `<worktree-id>` is the
first 16 hexadecimal characters of the SHA-256 of the real path of the top
level directory of the worktree. At startup the server writes
`server-<worktree-id>.json` beside `repo.json` in the directory of the
repository, with mode `0600`:

```json
{ "pid": 41207, "port": 38412, "token": "...", "socket": "/tmp/qreview-1000/4f5959a8bfd8b629-c710c85489f3b4c3.sock" }
```

It removes the file, and the socket, when it stops on Ctrl-C, `SIGTERM` or
`SIGHUP`. A server that cannot write the directory of the repository does
not start: it would lose every comment. That happens inside a sandbox, and
the error says to start qreview outside it. The commands above
read the file of their worktree to find the server, and try the socket
before the port. None of them starts a server, and each says to run
`qreview` when no server answers. A file whose server does not answer is a
stale one, left by a crash, and is ignored.

`qreview` itself reads it too. When a server already answers on the
worktree, it prints that URL and stops, and opens no second tab: an agent
that starts qreview twice gets the review that is open. Reviewing another
range in the same worktree means stopping that server first.

Two servers can then write one store. Each read of a change file and the
write that follows it hold a lock on the file `lock` in the directory of
the repository, so two writes that cross never lose one of them. A server
does not see the writes of the other one until its page reads again.

**The trace.** `--trace`, or `QREVIEW_TRACE=1`, prints one line per piece of
work to standard error: every git child process, every highlight pass, every
diff parse, and every HTTP request with the status it answered. Each line
carries the time since the process started and what the work cost, so a slow
read is measured and not guessed. Standard output stays clean, because
`export` writes there. The token never appears in a line.

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
