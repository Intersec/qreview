// Which version of a change a remark was written on.
//
// A second round reads a change that has been corrected since. A remark
// written on the sha the change carries now is a **current** one; anything
// else is a **previous** one, left by a round that is over. Only the current
// ones are counted, and only the current ones are exported.

import type { ChangeComments, Comment, Version } from '@/api/types';

/// True when the remark was written on the sha the change carries now.
///
/// A remark from a store older than format 3 names no version. It counts as
/// current: it is the only version it can belong to, and leaving it out
/// would hide a review that nothing else would show again.
export function isCurrent(comment: Comment, commit: string): boolean {
  return comment.commit === '' || comment.commit === commit;
}

/// The remarks of the version a change carries now, and the ones before.
export function rounds<T extends { comments: Comment[]; commit: string }>(
  change: T,
): { current: Comment[]; previous: Comment[] } {
  const current: Comment[] = [];
  const previous: Comment[] = [];

  for (const comment of change.comments) {
    (isCurrent(comment, change.commit) ? current : previous).push(comment);
  }
  return { current, previous };
}

/// The previous remarks of a change, in groups, one per version they were
/// written on.
///
/// The server orders the versions, newest first, so the round just before
/// this one comes first. A remark whose version the server could not name
/// is grouped under its sha alone.
export function previousGroups(
  change: ChangeComments,
): { version: Version; comments: Comment[] }[] {
  const { previous } = rounds(change);
  const out: { version: Version; comments: Comment[] }[] = [];

  for (const version of change.versions) {
    const comments = previous.filter((comment) => comment.commit === version.commit);
    if (comments.length > 0) {
      out.push({ version, comments });
    }
  }
  return out;
}
