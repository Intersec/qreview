// The threads of a review: a remark and the replies written under it.
//
// The cards stand in many places of the diff, so they reach the threads
// through one context rather than through every component between them.

import type { ComputedRef, InjectionKey } from 'vue';
import type { Comment } from '@/api/types';

export interface Threads {
  /// The replies of a remark, in the order they were written.
  repliesOf(id: string): Comment[];
  reply(id: string, body: string): Promise<void>;
  setDone(id: string, done: boolean): Promise<void>;
  /// The changes, and the files of the change on the screen, that hold a
  /// thread waiting for the reader.
  blockedChanges: ComputedRef<Set<string>>;
  blockedFiles: ComputedRef<Set<string>>;
}

export const THREADS: InjectionKey<Threads> = Symbol('threads');

/// The replies of each remark, oldest first.
export function repliesBy(comments: Comment[]): Map<string, Comment[]> {
  const out = new Map<string, Comment[]>();
  for (const comment of comments) {
    if (comment.parent !== null) {
      out.set(comment.parent, [...(out.get(comment.parent) ?? []), comment]);
    }
  }
  for (const replies of out.values()) {
    replies.sort((a, b) => a.createdAt.localeCompare(b.createdAt));
  }
  return out;
}

/// True when the thread waits for the reader: its last reply is a blocked
/// one of the agent. The next reply of the reader ends that.
export function isBlocked(replies: Comment[]): boolean {
  const last = replies[replies.length - 1];
  return last !== undefined && last.blocked && last.author === 'agent';
}
