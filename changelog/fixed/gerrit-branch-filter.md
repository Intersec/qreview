- Gerrit answered nothing for a change pushed to a feature branch. The query
  filtered on the branch that `.gerrit-branch` names, which is the
  integration branch, not the branch the change was pushed to. The query now
  asks by `Change-Id` and project alone, and the branch shown comes from the
  answer.
