# Changelog

All notable changes to **qreview** are written here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/).

The version policy is in [CONTRIBUTING.md](./CONTRIBUTING.md).

Changes that wait for a release are not written here. Each one is a file under
[`changelog/`](./changelog/), in the group it belongs to. See
[`changelog/README.md`](./changelog/README.md).

<!-- The release script writes new versions under this line. -->

## [0.9.0]

### Added

- **Ctrl+S saves a comment** — the box already answered to Ctrl+Enter. It now
  takes the key Gerrit uses too, and holds the browser's own save back.
- **The word under the pointer** — hovering a name lights it up wherever else
  it stands in the file, so a variable is followed without a search and
  without a click. Whole words only: `fd` is not the `fd` of `fdesc`.
- **The tab names the review** — the page title carries the name of the
  repository and the subject of the change being read, so one window per
  review is easy to find again.
- **A refresh button** — reads the repository again, so a commit amended or
  added while the page is open joins the series without a restart. The reader
  stays on the change and the file that were on the screen.

### Changed

- The export marks the lines a comment covers with a `>` at the left edge of
  the excerpt, and says so once at the top. A reader that does not count
  gave every line of the excerpt the same weight, and a remark on one line
  was applied to its neighbour.
- The button a selection offers, and the box it opens, say when the
  selection opens or closes inside a line: `Comment on part of 2 lines`, `A
  remark about a part of lines 6 to 7`. Whole lines, picked with the mouse
  or with `v`, read as before.

### Fixed

- A comment on a part of a line reaches the export as one. It used to export
  as its whole lines, so a remark on the last sentence of a paragraph read as
  a remark on the two lines it stood on (#8). The heading now quotes the
  text: `` on `for (;;)` `` on one line, `` from `…` to `…` `` over several,
  and `` on the second `%d` `` when the line holds the text more than once.
- A selection that opens or closes inside a line stays what you selected
  once the button appears. The range was painted under the live selection,
  which replaced the text it was anchored on, and the browser spread it to
  the whole lines.

## [0.8.0]

### Added

- **A change with no `Change-Id` finds its earlier versions too.** The key of
  such a change follows the sha, so an amend used to leave the round before it
  under a name nothing claimed, and the review with it. Git still has that
  commit — an amend stops pointing at it, and the reflog keeps the pointer —
  so qreview reads the reflog and links it: the version becomes a patch set
  and its remarks come back, read only, under the version they belong to.
  Nothing in the store is moved.

### Changed

- The diff shows the remarks of the version on the screen and no others. A
  remark of another version speaks of code that is not there; open the patch
  set it was written on and it is on its line. The pane lists them all, as
  before.
- A previous remark names the patch set it belongs to as well as the sha. A
  sha alone is a needle in a reflog.

### Fixed

- A change opens on its newest version again. Reading patch set 4 of one
  change and opening another landed on patch set 4 of that one, which is not
  the same work and is rarely what anybody wants. The number of a patch set
  means nothing across changes.
- The commit under review is the last version in the picker again. A version
  the server never saw was numbered after the newest and landed at the end, so
  a change opened on an old version whenever a remark had been written on one
  before it was pushed. The list follows the order the versions were written,
  and the one being reviewed is last.
- A version Gerrit never saw is called **Local** rather than given a patch set
  number of its own. That number meant nothing to anybody reading the same
  change on the server.
- A version that is only on the server says when it was pushed. It showed no
  date at all.
- Each version in the picker says whether the server knows it: `Patch set 3`
  when it does, `Local` when it does not. A version whose commit is not in
  this clone still says `· not fetched`.

## [0.7.0]

### Changed

- A previous remark cannot be edited or deleted. The round it belongs to is
  over, and a record that can be rewritten is not a record.
- A remark is **current** when it was written on the sha the change carries
  now, and **previous** when it was not. Only the current ones are counted and
  exported. The word *answered* is gone: the tool no longer guesses whether
  work was done, it says which version a remark belongs to, which is a fact.
- In the pane, each group of previous remarks is headed by the version it was
  written on: the short sha and the subject that version carried, cut to fit.
  A change reviewed over several rounds shows one group per round, newest
  first.
- A remark stands where it speaks, the way Gerrit puts one there. On its line;
  and when this version has no such line, before the first line of the file it
  was written on, inside the code; and when the change no longer touches that
  file, before the first line of the commit message. The band above the diff
  is gone, with the flat panel it held: it took the top of every screen, on
  every file, whether it held anything or not. The remarks Gerrit holds fall
  back the same way.
- The patch set you are reading follows you to the next change. Picking patch
  set 1 on one change and opening another used to land on the newest version
  of it, and the number had to be picked again on every change.
- The export and every count hold the remarks of the version under review and
  no others. A round before this one left remarks you have already dealt with,
  and an agent reading them again redoes work that is done. The export says
  how many it left out, and the interface still shows them all: on their line,
  grey and naming their version, and in the pane under the version they were
  written on.

### Fixed

- `--prev` says when the commit you named belongs to no change of the series.
  It is placed by its `Change-Id`, and a series rewritten commit by commit
  carries new ones, so the commit belonged to nothing and was dropped without
  a word.

### Removed

- **Comment on the change** is gone. Gerrit has one and nobody uses it: you
  cannot reply to it. The convention is to write on the commit message
  instead, which is a file like any other, takes replies, and is what a
  session reads first. The button says *Comment on the change* when the commit
  message is the file you are on. The remarks the old scope left are read as
  before, and shown on the commit message.

## [0.6.0]

### Added

- **A logo** — a lowercase q whose stroke is a diff, red above and green
  below. It is the favicon, so a qreview tab is one glance away among twenty
  others, and it sits beside the name in the top bar.
- **A comment on the left side** — click a number of the left column, in
  either view, and the remark speaks of the version before the change. A
  deleted line takes one this way. The remark is anchored on the base, so it
  follows that line across patch sets, and the export says
  `(before the change)` after the place it names.
- **Review what is not committed** — the tracked changes still in the working
  tree stand at the top of the series, as one more change to read and write
  remarks on. Staged or not, both. `--no-worktree` turns it off.
- **The commit under the version** — hover the version in the top bar, and it
  says which commit the binary was built from. `qreview --version` says the
  same. One release names a hundred commits, and a bug report needs the one
  that is running.
- **The Gerrit comments** — the remarks already posted on the server are shown
  beside your own, on the line they speak of, with the name of whoever wrote
  them. Read only: qreview posts nothing, replies to nothing and votes on
  nothing.
- **The second round** — read a series, have the work corrected, read it
  again. qreview offers the version you reviewed as a patch set, without
  `--prev`, and lists apart the remarks the correction has answered, with one
  action that drops them all. A version that is not the newest is read only.

### Changed

- The comment store is format 3. A comment now records the commit it was
  written against, which is what the second round reads. A store written by
  an older qreview is read as it is; its comments name no version and take no
  part in that.

### Fixed

- A file opened while its change was still loading is no longer taken back.
  The change opened on its first file when the list landed, a moment after
  the reader had picked another one, and the click was lost.

## [0.5.4]

### Fixed

- Copying code copies the code. The bar between two hunks no longer lands in
  the clipboard, a line of the side by side view is copied once rather than
  once per column, and the button a selection offers stands at the edge of
  the pane rather than under the pointer, where it took the right click that
  was meant for the selection.

## [0.5.3]

### Fixed

- `cargo xtask release` prints the line that pushes the tag it just wrote.
  `git push --follow-tags` carries a tag only beside a branch it is already
  pushing, so a tag cut after the branch went up reached nobody and no
  release was ever built.

## [0.5.2]

### Changed

- qreview lives on GitHub now, at `Intersec/qreview`. One command installs
  the newest build, with no account and no token, and the version check asks
  the same place by default. `{ "update": { "url": "" } }` turns it off.

## [0.5.1]

### Added

- **qreview says when a newer release is out**, beside the version it runs.
  It ships with no address to ask: name one in `update.url` of the
  configuration, pointing at a releases API that answers with `tag_name`.
  The check runs once per run and a failure to reach it says nothing.

### Changed

- The install steps in the README start on the releases page, where the
  reader is signed in already: download the file, unpack it, put it on the
  PATH. The link they held before sent `curl` to a sign-in page, and the
  sign-in page into `xz`.
- The file of a release is `qreview-linux-x86_64.xz`, with no version in the
  name. One link now holds the newest build, and the install steps name no
  version either. The release page and `qreview --version` say which one it
  is.

### Fixed

- Pressing `c` no longer writes a `c` in the comment it opens. Every key the
  interface answers to is now kept out of the text under it.

## [0.5.0]

### Added

- **The counts of what the session holds.** The two copy buttons say how
  many remarks they would copy, every change of the series says how many it
  carries, and every file of a change says the same. They come from one
  answer, so no two of them disagree, and they are right the moment a remark
  is written.
- **A pane that lists the remarks**, at the foot of the series. The change
  being read comes first, the rest follow in the order of the export, and
  each row names the place and the first line of the remark. A click opens
  the file and puts the keyboard on the line. It folds away, and it is not
  there at all until something is written.
- **The panes can be resized.** Drag the bar between the series and the
  code, or the one between the series and the list of comments. Each pane
  stops while its title is still on the screen, and the browser remembers
  the sizes. The arrow keys move a bar too, once it has the keyboard.

### Fixed

- A comment on one of the first lines of a file is found again instead of
  being listed as unanchored. The place was scored against the middle of the
  lines around it, and a line near the top of a file has fewer lines above
  it than that.
- A comment box takes the keyboard when it opens, so you can type at once. A
  box that comes back with an unfinished remark does not: you opened a file,
  not that box.
- The list of comments keeps its place on a short window. It was squeezed to
  its title, and the series above it kept all the room.
- Pushing a release no longer starts two pipelines on the same commit, one
  for the branch and one for the tag.

## [0.4.3]

### Added

- **A comment on a range.** Select the code with the mouse, over several
  lines or over a part of one, and the selection offers to become a comment.
  The keyboard does it too: `v` starts a range on the line it is on, `j` and
  `k` grow it, `c` writes on it. The range is drawn on the code, and it
  follows its first line into the next patch set.
- **What you type is kept as you type it.** A remark you have not saved
  survives opening another file, another change, and a reload: it comes back
  in its box, on the line it was written on. Cancel drops it.

### Changed

- The export orders the comments the way the code reads: the commits from
  the oldest, the files of a commit in alphabetic order with the commit
  message first, and the remarks of a file from top to bottom. It followed
  the order the remarks were written before.

### Fixed

- A click on a line puts the keyboard on it, so `c` writes there. The
  keyboard stayed where it was, and the remark opened on another line.

## [0.4.2]

### Added

- **The Change-Id opens the change on Gerrit**, when the remote names a
  server.
- **A read that takes a moment says so.** The file list and the diff each
  carry a spinner while they are being read, so a slow repository no longer
  looks like a frozen one. It appears only when the wait is long enough to
  notice.

### Changed

- The README opens with how to install qreview, and the link it names always
  holds the latest release.
- A release now carries `qreview-<tag>-linux-x86_64.xz` rather than the bare
  binary: 2.5M instead of 6.4M. Unpack it with
  `curl -L <link> | xz -d > ~/.local/bin/qreview`.

### Fixed

- The Change-Id in the change bar is shown whole. It was cut short while the
  bar had room for it.
- The `/` key reaches the file filter. It moved nowhere before, and the box
  it moves to now says that the keyboard is in it. A change with more than
  one file always offers the box.

## [0.4.1]

### Fixed

- Pushing a tag releases it. The tag is annotated, so `git push
  --follow-tags` carries it, and the pipeline that starts publishes the
  static Linux x86-64 binary through the GitLab server of the project, with
  no other host in the path.

## [0.4.0]

### Added

- **The commit message is a file of the change.** It is first in the list,
  under the name Gerrit gives it, and a comment sits on a line of it like a
  comment on any other file. Against another patch set the two messages are
  diffed, which is where an amend of the subject shows.
- **A static Linux x86-64 binary on every tag.** The pipeline builds it,
  attaches it to the release with the notes of that version, and publishes
  nothing when the gate is red.

### Fixed

- Opening a file right after changing the version, or a setting, no longer
  falls back to the first file of the change.

## [0.3.1]

### Fixed

- A comment box stays open when the diff loads again. Changing the context
  while writing used to take the box and the text away.
- A setting that only the browser reads no longer makes the diff load again.
  Choosing side by side, wrapping, a tab width, a font size or a theme now
  changes what is on the screen without a round trip.

## [0.3.0]

### Added

- **Two selectors for the versions of a change** — what to read against, an
  arrow, and what to read, each option carrying the number, the commit and
  the date.
- **A preferences panel** — context, diff view, theme, wrapping, whitespace,
  syntax colours, tab width and font size, behind the gear or the comma key.
  It writes `~/.config/qreview/config.json`, so the next run starts the way
  you left it and the command line reads the same file.
- **The keys Gerrit uses** — `j` and `k` walk the lines, `n` and `p` the
  hunks, `]` and `[` the files, `J` and `K` the changes, `c` writes on the
  line the keyboard is on, `u` shows or hides the series, and `?` lists them
  all. The diff carries a cursor for them to move.
- **A theme you choose** — light, dark, or whatever the system says.

### Changed

- **A comment stands alone** — no author, no draft, no reply. All three are
  for a conversation with a reviewer, and a review of your own series before
  the push has none: you write a remark, you correct the code, you delete the
  remark.
- **The context bar** carries the count and two short steps stacked beside
  it: one opens the lines under the code above, the other the lines over the
  code below.
- **Ten lines of context**, not the three git gives. Three is too few to
  judge a change, and the panel sets it.
- **The export says what it is** — the project, the branch, the commit and
  the patch set, then a plain request to address the comments, then the
  comments numbered so an answer can name one.

### Fixed

- **The file bar and the band above the diff** span the pane again. Only the
  code scrolls sideways.
- **A comment on a context line** is drawn once in the side by side view, not
  once per column, and a box left open no longer follows you to the next
  file.
- **Opening the context** puts the lines where they belong: the short step at
  the top of a gap lands above the bar, the one at the bottom below it, and
  both take a comment.
- **A doc comment stays on its line.** `comment.block.documentation` reached
  the page as a class the interface framework also owns, which set
  `display: block` and broke every doc comment in two. Every syntax class is
  prefixed now.
- **A series that carries the same `Change-Id` twice** no longer opens both
  changes at once or shares one review between them.
- **Comparing two versions** lists the files the change touches, not the
  hundred a rebase moved between them.
- **The file list** no longer waits for Gerrit, and an answer that arrives
  after you have moved on is dropped instead of shown as an error.

### Removed

- **The draft mark, the author name and replies** left with resolving, for
  the same reason: nobody else reads this review.

## [0.2.0]

### Added

- **Know where you are** — a bar above the diff carries the subject of the
  change, its commit, and which file of the change is open, with Prev and
  Next beside it.
- **Open the context between two hunks** — a bar says how many lines it hides
  and opens them, whole or ten at a time. A diff carries what changed and
  three lines around it, and the rest is one click away.
- **Files grouped by directory** — the directory is said once and the files
  under it carry their name alone, so a change of forty files stays readable
  in a narrow pane.
- **Ignore whitespace** — a switch above the diff leaves out the lines that
  differ only by spacing.
- **Mark a change read** — a box beside each change of the series, kept
  between sessions. It says nothing about the comments: a change can be read
  and still carry remarks.

### Changed

- **The diff reads like Gerrit** — dense monospace rows, the pale line and
  the stronger word inside it, and a line-number gutter left plain. Both
  themes follow the Gerrit palette.
- **The window belongs to the code** — the comments pane is gone and the two
  left panes are one. A comment about the file or the change now sits above
  the diff, where it is written. The code went from about half the window to
  five sixths of it, and `[` hides the sidebar as well.

### Fixed

- **Copy the series** said the series held nothing. It trusted a comment
  count taken when the session opened, so a comment written a minute later
  was invisible to it.
- **The date of a commit** is the same on every machine. One git writes
  `+00:00` for UTC and another writes `Z`, so the tool reads a count of
  seconds and formats it itself.
- **Load 5 older** did nothing at the base of a series, and nothing at a
  merge. The base is where the first batch stopped, not a wall, and a merge
  now joins the list, where it is reviewable, while the walk goes on down the
  first parent.
- **The side by side view** — the two gutters took a quarter of the pane
  each, so the code started in the middle of the window, and no line was
  green or red. A comment now sits under the side it was written on rather
  than across both.

### Removed

- **Resolving a thread** — it is a conversation with a reviewer, and a review
  of your own series before the push has none. Correct the code and delete
  the remark.

## [0.1.0]

### Added

- **Comments that survive an amend** — write on a line, on a file, or on the
  change, answer in a thread, and resolve it. Comments are keyed by the
  `Change-Id`, so amending the commit keeps them.
- **Export for a session** — two buttons put the review in the clipboard as
  Markdown, code first and comment after, and `qreview export` prints the
  same text.
- **Custom file types** — a repository declares its own extensions in
  `.qreview.json` and every reader gets the colours with no setup. A grammar
  file dropped in the configuration directory adds a language.
- **Gerrit patch sets** — on a repository with a Gerrit remote, the versions
  already pushed are listed with the numbers the server gave them, and one is
  fetched only when you open it. `--no-gerrit` skips the question.
- **Local review of a Git series** — `qreview` reads the repository, opens a
  browser, and shows the series one change at a time, with the files of the
  change and a diff you can read unified or side by side.
- **Merge review** — a merge is read against the auto-merge, so the diff is
  the conflict resolution and not the whole branch the merge brought in. The
  two parents stay one click away.
- **Patch sets** — read any version of a change against any other. Name an
  older commit with `--prev`, and a comment written on one version is found
  again in the next, or listed apart when its line is gone.
- **Load 5 older** — the start of a series is often impossible to compute, so
  the walk stops at a boundary that says why, and going further back is one
  click. A merge, a tag, the base, and a guessed start each say what they are.

## [0.0.0] — unreleased

The project starts. Nothing is released yet. See
[`roadmap/plan.md`](./roadmap/plan.md) for the work order.
