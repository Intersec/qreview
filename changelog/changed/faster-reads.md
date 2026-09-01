- Opening a file is much faster. The file list of a change is read with one
  git call instead of two and is kept for the run, the two sides of a diff
  are highlighted at the same time, and a file is highlighted only as far as
  its hunks reach. On a large repository, a file that took 2.7 s to open now
  takes 0.7 s, and 0.03 s the second time.
