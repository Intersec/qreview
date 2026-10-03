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

1. Run `qreview` in the background, from the repository. It opens the review
   in the browser of the user and prints its address.
   - If a server already runs on the repository, `qreview` prints its
     address and stops. That is not an error: use that server.
   - `qreview <rev>` or `qreview <revA>..<revB>` reviews another range.
     To change the range, the user stops the old server first.
2. Read the open threads: `qreview export --json`.
3. Start the loop below.

## The loop

Repeat until the user says the review is over:

1. Run `qreview wait --after <next>` in the background. Use the `next`
   number of the last `wait`. Leave out `--after` the first time.
2. When it returns, read the JSON:
   - `events`: what the user wrote. Each event carries the `comment`, with
     its `id`, its `body` and its `parent` (null on a new remark).
   - `next`: pass it to the next `wait`, so nothing is missed between two
     waits.
   - `"reset": true`: the server restarted. Read everything again with
     `qreview export --json`.
   - Exit code 1 and no event: the time ran out. Wait again.
3. Deal with each new remark or reply. See "Answer a remark".
4. Go back to step 1.

## Answer a remark

Read the thread first: `qreview export --json` gives the place, the excerpt
of code (the lines marked `>` are the ones the remark is about) and every
reply of the thread.

- **A question about the code.** Answer it:
  `qreview reply <id> --body "<answer>"`. Do not change the code.
- **A request that is clear.** Change the code, then reply with what you did
  and check the Done box: `qreview reply <id> --body "<what changed>" --done`.
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

The remarks are on commits, so a fix amends the commit it belongs to.

1. Make the change in the working tree.
2. **Reply before you amend.** Once a commit is amended, its threads belong
   to an earlier version, and qreview refuses a reply or a Done on them. So
   send every reply and every `--done` for that commit first, then amend it.
   For a commit below the top of the series, run
   `git commit --fixup=<sha>`, then
   `GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash <base>`. The editor
   variable makes the rebase run with no editor.
3. Keep the `Change-Id` trailer of the commit, if it has one. It is what
   keeps the review attached to the change across an amend.
4. Run `qreview refresh`. The browser shows the new version at once.

Never push. The user publishes.

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
| `qreview refresh` | Read the repository again, after an amend |

A done thread is not in the export: its work is done.
