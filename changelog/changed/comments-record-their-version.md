- The comment store is format 3. A comment now records the commit it was
  written against, which is what the second round reads. A store written by
  an older qreview is read as it is; its comments name no version and take no
  part in that.
