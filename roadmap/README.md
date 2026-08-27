# Roadmap

This directory holds the plan for qreview. It is the memory of the project
across sessions. Read it before you write code.

| Document | What it holds |
|---|---|
| [`concept.md`](./concept.md) | The problem, the users, the non-goals, the prior art |
| [`stack.md`](./stack.md) | The technology decisions and the rejected alternatives |
| [`design.md`](./design.md) | The architecture, the data model, the storage format, the API |
| [`features.md`](./features.md) | Where the backlog went: the GitHub issues |
| [`plan.md`](./plan.md) | The milestones, the tasks, and the session protocol |
| [`testing.md`](./testing.md) | The test policy. It is strict on purpose |

## How to work a session

1. Read [`plan.md`](./plan.md). Take the first task that is not done.
2. Read the part of [`design.md`](./design.md) that the task touches.
3. Write the code and the tests together.
4. Run `make check`. The task is not done while the command is red.
5. Commit the task alone.
6. Mark the task done in `plan.md`.

If a task contradicts the design, stop and report. Do not change the design in
silence.

## Outside this directory

| File | What it holds |
|---|---|
| [`../README.md`](../README.md) | What the tool is and how to use it |
| [`../CLAUDE.md`](../CLAUDE.md) | What a Claude session must do |
| [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Direction, branches, commits, versions, release |
| [`../CHANGELOG.md`](../CHANGELOG.md) | Released versions only |
| [`../changelog/`](../changelog/) | One file per pending changelog entry |
| [`../LICENSE`](../LICENSE) | Apache-2.0 |
