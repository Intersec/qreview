- qreview is much faster on a large repository. The file list of a change is
  read with one git call instead of two and is kept for the run, the two
  sides of a diff are highlighted at the same time, a file is highlighted
  only as far as its hunks reach, the tags are read once per walk instead of
  once per commit, and a commit named by its hash is read once per run. A
  file that took 2.7 s to open now takes 0.6 s, and 0.03 s the second time.
