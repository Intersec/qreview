---
name: qreview
description: >-
  Review a Git series with the user in qreview, a local review tool in the
  browser with the Gerrit model. Use when the user asks to open qreview, to
  review a commit or a series together, to answer or address their review
  remarks, to read the comments they left in qreview, or to review a change
  yourself and leave remarks. Triggers: "qreview", "open the review",
  "answer my remarks", "address my comments", "review this series",
  "review my commits".
---

# qreview

qreview shows a Git series in the browser, one commit at a time. The user
writes remarks on lines. You answer them in threads, change the code, and
write remarks of your own. Everything goes through `qreview` commands. Never
edit the files under `~/.local/state/qreview` by hand.

## Start

If your commands run in a sandbox, the user starts the server, not you. A
server started inside the sandbox cannot save comments, and the browser of
the user cannot reach it. You know you are in one when `qreview` prints
`Read-only file system` about `server.json`: stop that server, and ask the
user to run `qreview` in their own terminal. Your commands then reach their
server through its Unix socket under `/tmp`, with nothing to configure.

1. Run `qreview` in the background, from the repository. It opens the review
   in the browser of the user and prints its address.
   - If a server already runs on the repository, `qreview` prints its
     address and stops. That is not an error: use that server. Check first
     with `qreview wait --timeout 1`: exit code 1 with JSON means a server
     answers.
   - `qreview <rev>` or `qreview <revA>..<revB>` reviews another range.
     To change the range, the user stops the old server first.
2. Read the open threads: `qreview export --json`.
3. Start the loop below.

## The loop

Repeat until the user says the review is over:

1. Run `qreview wait --after <next>` in the background. Use the `next`
   number of the last `wait`. Leave out `--after` the first time.
2. When it returns, read the JSON:
   - `events`: the requests of the user. An event of kind `ask` names one
     thread, and carries it in `thread`: the remark first, then the
     replies in order. Answer that thread, and no other.
   - `next`: pass it to the next `wait`, so nothing is missed between two
     waits.
   - `"reset": true`: the server restarted. Read everything again with
     `qreview export --json`.
   - Exit code 1 and no event: the time ran out. Wait again.
   - An event of kind `fix`: the user pressed **Fix** on the change `key`.
     See "Fix a change".
3. Answer the thread. See "Answer a remark". If `thread` is empty, or a
   reply fails with `no comment`, the user deleted the thread: skip it, and
   do not look into why.
4. Go back to step 1.

## Answer a remark

The request carries the thread. For the code around it, `qreview export
--json` gives the place and the excerpt (the lines marked `>` are the ones
the remark is about).

- **A question about the code.** Answer it:
  `qreview reply <id> --body "<answer>"`. Do not change the code.
- **A request that is clear.** Change the code, amend, and close the thread
  in the same call that moves it: see "Change the code".
- **A request that is not clear, or a problem you cannot solve alone.** Ask,
  and wait for the user: `qreview reply <id> --body "<question>" --blocked`.
  The thread then asks for the attention of the user. Change no code for
  that thread until the user answers.
- **A remark you disagree with.** Say why in a reply. Do not check Done.

Rules:

- If a thread waits for an answer (`"blocked": true` in the export), make no
  new version of its commit.
- Answer in the thread, not in the terminal. The user reads the browser.
- A long body goes on standard input: `--body -`.
- Markdown works in a body.

## Change the code

The remarks are on commits, so a fix amends the commit it belongs to. After
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

The user pressed **Fix** on a change: deal with every open thread of it.

1. `qreview export --json --key <key>` lists the open threads.
2. For each thread: answer a question, fix a clear request, ask with
   `--blocked` about anything that is not clear.
3. Make all the fixes, amend once, and run one `qreview migrate` with every
   `--done` and every `--blocked`.
4. Wait again.

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

Then wait for the replies of the user with the loop above.

## Commands

| Command | What it does |
|---|---|
| `qreview` | Start the review, or print the address of the one that runs |
| `qreview export --json` | The open threads, each comment with its `id` |
| `qreview wait [--after <n>] [--timeout <s>]` | Block until the user writes, and print it as JSON |
| `qreview reply <id> --body <text> [--done] [--blocked]` | Reply to a thread |
| `qreview comment <file>[:<side>:<line>[-<end>]] --body <text> [--key <id>]` | Write a remark |
| `qreview migrate --from <old> [--to <new>] [--done <ids>] [--blocked <id>=<text>]` | Declare a new version after an amend, and move its threads |
| `qreview refresh` | Read the repository again |

A done thread is not in the export: its work is done. The user deletes done
threads with **Clean** when they agree with the work.
