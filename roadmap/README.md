# Roadmap

This directory holds the design of qreview. It is the memory of the project
across sessions. Read it before you write code.

| Document | What it holds |
|---|---|
| [`concept.md`](./concept.md) | The problem, the users, the non-goals, the prior art |
| [`stack.md`](./stack.md) | The technology decisions and the rejected alternatives |
| [`design.md`](./design.md) | The architecture, the data model, the storage format, the API |
| [`testing.md`](./testing.md) | The test policy. It is strict on purpose |

The work itself is not here. The backlog is the
[GitHub issues](https://github.com/Intersec/qreview/issues), because an issue
carries a discussion that a line of Markdown cannot.

`design.md` and `stack.md` are a contract. If a change contradicts the design,
stop and report. Do not change the design in silence.

## Outside this directory

| File | What it holds |
|---|---|
| [`../README.md`](../README.md) | What the tool is and how to use it |
| [`../CLAUDE.md`](../CLAUDE.md) | What a Claude session must do |
| [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Direction, branches, commits, versions, release |
| [`../CHANGELOG.md`](../CHANGELOG.md) | Released versions only |
| [`../changelog/`](../changelog/) | One file per pending changelog entry |
| [`../LICENSE`](../LICENSE) | Apache-2.0 |
