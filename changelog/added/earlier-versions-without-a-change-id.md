- **A change with no `Change-Id` finds its earlier versions too.** The key of
  such a change follows the sha, so an amend used to leave the round before it
  under a name nothing claimed, and the review with it. Git still has that
  commit — an amend stops pointing at it, and the reflog keeps the pointer —
  so qreview reads the reflog and links it: the version becomes a patch set
  and its remarks come back, read only, under the version they belong to.
  Nothing in the store is moved.
