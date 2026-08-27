# Changelog

All notable changes to **qreview** are written here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions
follow [Semantic Versioning](https://semver.org/).

The version policy is in [CONTRIBUTING.md](./CONTRIBUTING.md).

Changes that wait for a release are not written here. Each one is a file under
[`changelog/`](./changelog/), in the group it belongs to. See
[`changelog/README.md`](./changelog/README.md).

<!-- The release script writes new versions under this line. -->

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
