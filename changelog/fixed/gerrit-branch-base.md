- The series was computed from the wrong branch on a feature branch. The base
  came from `.gerrit-branch`, which names the integration branch. A new rule
  asks Gerrit which branch the change is on. It runs under the local rules,
  and only when they answer nothing or answer further than
  `series.maxCommits`.
