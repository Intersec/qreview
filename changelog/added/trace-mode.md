- **A trace mode** — `--trace`, or `QREVIEW_TRACE=1`, prints on standard
  error what every piece of work costs: each child process, git, `ssh` to
  Gerrit and `curl` alike, each highlight pass, each diff parse, and each
  request with the status it answered.
