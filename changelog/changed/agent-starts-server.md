- The agent starts `qreview` itself, as a background task of its session, so
  you no longer start it by hand. The server stops when the session ends.
  Inside a sandbox, `qreview` now refuses to start, where it used to run and
  lose every comment. To let the agent start it there, add `"qreview *"` to
  `sandbox.excludedCommands` in the Claude Code settings.
