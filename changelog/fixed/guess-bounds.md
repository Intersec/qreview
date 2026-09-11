- The series loaded one commit per click on a branch that is pushed. The
  guess ended at any commit a remote-tracking ref reached, and the branch
  pushed for review reaches every commit of the series. A ref that reaches
  the head of the series is now ignored, and the two soft signals end the
  first batch only, which is what the design always said.
