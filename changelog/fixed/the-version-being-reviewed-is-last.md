- The commit under review is the last version in the picker again. A version
  the server never saw was numbered after the newest and landed at the end, so
  a change opened on an old version whenever a remark had been written on one
  before it was pushed. The list follows the order the versions were written,
  and the one being reviewed is last.
- A version Gerrit never saw is called **Local** rather than given a patch set
  number of its own. That number meant nothing to anybody reading the same
  change on the server.
- A version that is only on the server says when it was pushed. It showed no
  date at all.
- Each version in the picker says whether the server knows it: `Patch set 3`
  when it does, `Local` when it does not. A version whose commit is not in
  this clone still says `· not fetched`.
