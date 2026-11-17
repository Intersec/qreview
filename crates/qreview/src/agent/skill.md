# qreview

qreview shows a Git series in the browser, one commit at a time. The user
writes remarks on lines. You answer them in threads, change the code when
the user asks for a fix, and write remarks of your own. Everything goes
through `qreview` commands. Never edit the files under
`~/.local/state/qreview` by hand.

## The rule above all others

**Change no code and no commit until the user asks for a fix.** A remark is
something to answer, not an order, even when it reads like a request. Each
new version of a commit is a new patch set that the user did not ask for.

The user asks for a fix in one of two ways:

- **Fix this change** or **Fix the series** in the browser: an event of
  kind `fix`.
- An explicit request to you, in the terminal: "fix this change".

Outside a fix: no edit, no `git commit`, no amend, no rebase, no
`qreview migrate`. That covers the commit message too: do not reword it.

## Start

If the `origin` remote of the repository is a Gerrit server, the user
starts the server in their own terminal, not you: Gerrit may ask them to
log in there. Ask them to run `qreview`, or `qreview <gerrit-url>` for a
change of Gerrit. Go on when `qreview wait --timeout 1` exits with code
1 and JSON.

If your commands run in a sandbox, the user starts the server, not you. A
server started inside the sandbox cannot save comments, and the browser of
the user cannot reach it. You know you are in one when `qreview` prints
`Read-only file system` about a `server-<id>.json` file: stop that server,
and ask the user to run `qreview` in their own terminal. Your commands then
reach their server through its Unix socket under `/tmp`, or in the git
directory of the worktree from a devcontainer, with nothing to configure.

1. Run `qreview` in the background, from the worktree. It opens the review
   in the browser of the user and prints its address.
   - If a server already runs on this worktree, `qreview` prints its
     address and stops. That is not an error: use that server. Check first
     with `qreview wait --timeout 1`: exit code 1 with JSON means a server
     answers.
   - `qreview <rev>` or `qreview <revA>..<revB>` reviews another range.
     To change the range, the user stops the old server first.
   - `qreview <gerrit-url>` fetches a change of Gerrit and reviews it on
     the server of the worktree. See "A change of a colleague".
2. Read the open threads: `qreview export --json`.
3. Start the loop below.

## The loop

Repeat until the user says the review is over:

1. Run `qreview wait --after <next>` in the background. Use the `next`
   number of the last `wait`. Leave out `--after` the first time. Give it
   no `--timeout`: it blocks until the user asks, for hours if needed.
2. When it returns, read the JSON:
   - `events`: the requests of the user. An event of kind `ask` names one
     thread, and carries it in `thread`: the remark first, then the
     replies in order. Answer that thread, and no other.
   - `next`: pass it to the next `wait`, so nothing is missed between two
     waits.
   - `"reset": true`: the server restarted. Read everything again with
     `qreview export --json`.
   - If the server stops during a `wait`, the `wait` looks for it again,
     and attaches to it as soon as the user starts one. A new server
     answers `"reset": true`.
   - An event of kind `fix`: the user pressed **Fix this change** on the
     change `key`. See "Fix a change". If `key` is empty, the user pressed
     **Fix the series**. See "Fix the series". If the event names a thread
     in `id` and carries it in `thread`, the user pressed **Apply** on that
     thread of Gerrit: fix that thread alone, as "Fix a change" says, and
     leave every other thread as it is.
3. Answer the thread. See "Answer a remark". Change nothing. If `thread` is empty, or a
   reply fails with `no comment`, the user deleted the thread: skip it, and
   do not look into why.
4. Go back to step 1.

## Answer a remark

The request carries the thread. For the code around it, `qreview export
--json` gives the place and the excerpt (the lines marked `>` are the ones
the remark is about).

- **A question.** Answer it: `qreview reply <id> --body "<answer>"`.
- **A request.** Say what you would change, and how, in a reply. Change
  nothing: the change comes with the next fix.
- **A request that is not clear, or a problem you cannot solve alone.** Ask:
  `qreview reply <id> --body "<question>" --blocked`. The thread then asks
  for the attention of the user.
- **A remark you disagree with.** Say why in a reply.

**Done means that the code changed for this thread.** Check it only in a
fix, on a thread whose request you carried out. Never on a question you
answered, on a request you only described, or on a remark you disagree
with: those threads stay open, and the user closes them.

Rules:

- If a thread waits for an answer (`"blocked": true` in the export), do not
  fix it, even in a fix.
- Answer in the thread, not in the terminal. The user reads the browser.
- A long body goes on standard input: `--body -`.
- Markdown works in a body.

## Put a remark on Gerrit

The user sends remarks to Gerrit from qreview, where the author of the
change reads them. One thread of qreview becomes one remark on Gerrit,
and its text is the **newest reply** of the thread. The usual path:

1. The user writes a remark, and asks you about it (Save & ask).
2. You answer, and the two of you talk in the thread.
3. The user asks you to put it on Gerrit: "put it on Gerrit", "format it
   for Gerrit", "word it for the author". Run:

   ```sh
   qreview stage <id> --body "<the remark, for the author of the change>"
   ```

   `stage` marks the thread For Gerrit, and writes your text as its
   newest reply. The card shows that reply with the tag "for Gerrit".
4. The user reads it, edits it if needed, and clicks Send.

Rules:

- Write the body to the author of the change, not to the user. Leave out
  your talk with the user, and do not mention qreview or yourself.
- After `stage`, write no other reply in that thread: a newer reply would
  become the text that goes out. If the user asks something more there,
  answer, then run `stage` again with the whole text for the author, so
  that it is the newest reply again.
- Never answer "Staged for Gerrit." or the like in the thread. The tag on
  the card already says it.
- On a change of the user, stage only what the user asked for. Without a
  request, a thread stays in qreview.
- On a change of a colleague, a remark is for the author: stage it without
  a request. See "A change of a colleague".

**You never send anything to Gerrit.** No command does: the user sends from
the page, and Gerrit keeps each remark as a draft that only the user sees
until they publish it there. `stage` fails when Gerrit does not hold the
version yet: tell the user that the change must be pushed first.

## Threads of Gerrit

The remarks that reviewers posted on Gerrit are threads too. Their id
reads `gerrit:<id>`, in the export and in an `ask` event. A comment of
Gerrit has the author `gerrit`, and `name` says which reviewer wrote it.
A thread of Gerrit that waits for an answer is work, like a remark of the
user: it is in the export, and in a fix.

- **Talk with the user about it**: `qreview reply gerrit:<id> --body
  "<text>"`. That reply stays in qreview: the reviewer never reads it.
- **Answer the reviewer**, when the user asks: `qreview stage gerrit:<id>
  --body "<text>"`. Write to the reviewer. The user sends it.
- **Close it in a fix**: for a thread whose request you carried out, run
  `qreview stage gerrit:<id> --resolve --body "Done"`, not `--done`. Gerrit
  then shows the thread resolved, once the user sends. Say more in one
  short sentence only when "Done" alone does not show what you did:
  "Done, also in X and Y." or "Done: the helper is in the change before
  this one." Name only what this version fixes.
- **Fix the pattern, not only the line.** A reviewer names one place but
  means the problem. Search the series for the other sites of the same
  problem, by the pattern and not only by the name of the line. Fix the
  sites of this commit with it. In "Fix the series", fix each site in the
  commit that holds it. In "Fix a change", list in your reply the sites of
  other commits, and fix none of them. A site in code that the series does
  not touch is older than the review: list it, and do not fix it.
- **When the user does not want the change**, they say why. Stage that
  reason for the reviewer with `qreview stage gerrit:<id> --body
  "<reason>"`, worded for the reviewer. Add `--resolve` only when the user
  asks for it.
- A thread of Gerrit takes no `--done` and no `--blocked` in `migrate`.
  Its place stays on the patch set it was posted on.

## Change the code

Only in a fix. The remarks are on commits, so a fix amends the commit it
belongs to. After
the amend, tell qreview which commit replaced which, and the threads follow
their lines onto the new version.

1. Note the sha of the commit you fix: `old=$(git rev-parse <commit>)`.
2. Make the change and amend.
   - The top commit: `git commit --amend --no-edit -a`.
   - A commit below the top: `git commit --fixup=<old> -a`, then
     `GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash <base>`. The
     editor variable makes the rebase run with no editor.
3. Declare the new version, and close or ask in the same call:

   ```sh
   qreview migrate --from $old --to <new sha> --done <id>,<id> --blocked '<id>=<question>'
   ```

   `--to` is `HEAD` by default. qreview moves every thread of the old
   version to its new line, adds a Done reply for each `--done` id and a
   question for each `--blocked` one. A `Change-Id` is not needed.
4. **A rebase rewrites every commit above the one you fixed.** Migrate each
   of them too, or their threads stay on the old versions. `ORIG_HEAD` is
   the top before the rebase; pair the commits by position:

   ```sh
   paste <(git rev-list --reverse <base>..ORIG_HEAD) <(git rev-list --reverse <base>..HEAD) |
     while read from to; do [ "$from" = "$to" ] || qreview migrate --from "$from" --to "$to"; done
   ```

   Add `--done` to the pair of the commit you fixed.

`migrate` refuses when the old commit already has another next version, or
when the two commits carry different `Change-Id`s. Read the message: it
names the commit.

Never push. The user publishes.

## Fix a change

The user asked for a fix of a change: deal with every open thread of it.

1. `qreview export --json --key <key>` lists the open threads.
2. Sort them:
   - a request that is clear: fix it;
   - a request that is not clear: ask with `--blocked`, and do not fix it;
   - a question: answer it in a reply, and leave the thread open;
   - a thread that waits for the user: leave it.
3. Make all the fixes, amend once, and run one `qreview migrate` with
   `--done` for the threads you fixed and `--blocked` for your questions.
   Nothing else is Done. For a thread of Gerrit that you fixed, stage a
   reply that resolves it instead (see "Threads of Gerrit").
4. Wait again, **only once the whole fix is over**: the code, the amend,
   every migrate and every reply. Your next `qreview wait` tells the page
   that the fix is done, and the page then shows it all at once. Do not
   start the wait early, in the background, while you still work.

## Fix the series

The user asked for a fix of every change of the series: deal with every
open thread of the series.

1. `qreview export --json` lists the open threads of each change.
   Skip a change with `"own": false`: it belongs to a colleague (see "A
   change of a colleague").
2. Sort them as in "Fix a change".
3. For each commit that has a thread to fix, note its sha, make its fixes,
   and stage only those edits. Then run `git commit --fixup=<sha>`. The top
   commit gets a fixup too.
4. Run one `GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash <base>`.
   `<base>` is the parent of the lowest commit that you fixed. If the
   rebase stops on a conflict, resolve it and run `git rebase --continue`.
5. Pair the old and the new commits as in step 4 of "Change the code". Run
   one `qreview migrate` for each pair that changed, with the `--done` ids
   and the `--blocked` questions of that commit only.
6. Wait again, **only once the whole fix is over**, as in "Fix a change".

## A change of a colleague

A change whose `own` is false in `qreview export --json` belongs to a
colleague. The user reviews it, and never amends it:

- Change no code and no commit for it, even in a fix.
- A remark of the user on it is meant for the author. When your answer
  leads to a remark for the author, stage it at once, in the same turn. Do
  not ask whether it goes to Gerrit: the user reads it, and sends or
  edits it.
  1. If you have something to tell the user first, such as what you
     checked or why the remark holds, write it with `qreview reply`.
  2. Then run `qreview stage <id> --body "<the remark, for the author>"`,
     as in "Put a remark on Gerrit". Do not also copy the remark in the
     reply.
- If the user only asks you a question about the code, and no remark for
  the author comes out of it, answer with `qreview reply` and stage
  nothing.
- When you review such a change on your own, `qreview comment` stages
  each remark for Gerrit, without `--gerrit`: each remark is for the
  author. This holds when another skill writes the review too. Where Gerrit
  does not hold the version, the remark stays in qreview.

When the user opened the change with `qreview <gerrit-url>`, its review
runs on the server of the worktree: the commands reach it as usual.

## Review on your own

When the user asks you to review a change, read it with `git show <sha>`,
then write each remark on its line:

```sh
qreview comment src/net.c:new:42 --body "read() can return 0 here, and the loop never ends."
qreview comment src/net.c:new:40-44 --body "This block reads twice."
qreview comment src/net.c:old:12 --body "This removed check was still needed."
qreview comment src/net.c --body "A remark about the whole file."
```

- `new` is the version after the commit, `old` the version before it.
- The change is the newest commit of the series that touches the file. Name
  another one with `--key <Change-Id>`.
- Each command prints the comment it wrote, with its `id`.
- On a change of a colleague, each remark is staged for Gerrit (see "A
  change of a colleague").

Then wait for the replies of the user with the loop above.

## Commands

| Command | What it does |
|---|---|
| `qreview` | Start the review, or print the address of the one that runs |
| `qreview export --json` | The open threads, each comment with its `id` |
| `qreview wait [--after <n>] [--timeout <s>]` | Block until the user asks, and print it as JSON. No time limit by default |
| `qreview reply <id> --body <text> [--done] [--blocked]` | Reply to a thread |
| `qreview comment <file>[:<side>:<line>[-<end>]] --body <text> [--key <id>] [--gerrit]` | Write a remark, staged for Gerrit with `--gerrit`, and always on the change of a colleague |
| `qreview stage <id> --body <text>` | Stage a thread for Gerrit, with the text for the author as its newest reply |
| `qreview stage gerrit:<id> --body <text> [--resolve]` | Write a reply in a thread of Gerrit, staged, and resolve it with `--resolve` |
| `qreview migrate --from <old> [--to <new>] [--done <ids>] [--blocked <id>=<text>]` | Declare a new version after an amend, and move its threads |
| `qreview refresh` | Read the repository again |

A done thread is not in the export: its work is done. The user deletes done
threads with **Clean** when they agree with the work.
