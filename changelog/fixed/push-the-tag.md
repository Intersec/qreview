- `cargo xtask release` prints the line that pushes the tag it just wrote.
  `git push --follow-tags` carries a tag only beside a branch it is already
  pushing, so a tag cut after the branch went up reached nobody and no
  release was ever built.
