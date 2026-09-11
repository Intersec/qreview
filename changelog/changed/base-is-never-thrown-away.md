- **A resolved base is never thrown away.** A base further than
  `series.maxCommits` used to be called wrong, and the guess took over with
  less to go on. The first batch now loads that many commits of it and stops
  on a card that says how far the base is, with a **Load the rest (N)**
  button next to **Load up to 5 older**.
