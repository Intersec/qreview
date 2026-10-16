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

The procedure of this skill ships in the qreview binary, so it always
matches the commands of the release that runs. Read it first:

```sh
qreview skill
```

Then follow what it prints.

If `qreview` is not on the PATH, it is not installed. If it does not know
`skill`, it is a release from before this command. In both cases, tell the
user to install a newer release of qreview, and stop.
