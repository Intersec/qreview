// Which version of a change a remark was written on.
//
// A second round reads a change that has been corrected since. The remarks
// of the round before are still there, and telling them from the ones being
// written now is what keeps a count, an export and a pane readable.

import type { Comment } from '@/api/types';

/// True when the remark was written on the version named by `commit`.
///
/// A remark from a store older than format 3 names no version. It counts as
/// this one: it is the only version it can belong to, and leaving it out
/// would hide a review that nothing else would show again.
export function ofVersion(comment: Comment, commit: string): boolean {
  return comment.commit === '' || comment.commit === commit;
}

/// The remarks of this round, and the ones a round before it left.
export function rounds<T extends { comments: Comment[]; commit: string }>(
  change: T,
): { now: Comment[]; earlier: Comment[] } {
  const now: Comment[] = [];
  const earlier: Comment[] = [];

  for (const comment of change.comments) {
    (ofVersion(comment, change.commit) ? now : earlier).push(comment);
  }
  return { now, earlier };
}
