- **Fix, migrate and Clean** — after an amend, `qreview migrate --from
  <old sha>` declares the new version, with no need for a `Change-Id`, and
  moves every thread to its new line. The old version still shows each
  thread where it stood, with a link to where it went. **Fix** (or `a`) asks the agent to
  deal with the open threads of a change, behind a dialog. **Clean** deletes
  the done threads, with an Undo.
