// The threads of a review: a remark and the replies written under it.
//
// The cards stand in many places of the diff, so they reach the threads
// through one context rather than through every component between them.

import type { ComputedRef, InjectionKey, Ref } from 'vue';
import type { Comment, PostedComment } from '@/api/types';

export interface Threads {
  /// The replies of a remark, in the order they were written.
  repliesOf(id: string): Comment[];
  /// The replies of a thread of Gerrit, from Gerrit, oldest first.
  postedRepliesOf?(thread: string): PostedComment[];
  /// Reply to a thread of Gerrit: here only, or staged for Gerrit.
  replyOnGerrit?(
    thread: string,
    body: string,
    stage: { resolve: boolean } | null,
    ask?: boolean,
  ): Promise<void>;
  /// Check or clear the Done box of a thread of Gerrit.
  setGerritDone?(thread: string, done: boolean): Promise<void>;
  /// Delete a reply written here to a thread of Gerrit.
  remove?(id: string): Promise<void>;
  reply(id: string, body: string, ask?: boolean): Promise<void>;
  /// Ask the agent to answer the thread.
  ask(id: string): Promise<void>;
  setDone(id: string, done: boolean): Promise<void>;
  /// Stage a remark for Gerrit, or keep it here.
  setGerrit?(id: string, on: boolean): Promise<void>;
  /// True when Gerrit holds the version the change carries now, so a remark
  /// on it can go there.
  onGerrit?: ComputedRef<boolean>;
  /// The changes, and the files of the change on the screen, that hold a
  /// thread waiting for the reader.
  blockedChanges: ComputedRef<Set<string>>;
  blockedFiles: ComputedRef<Set<string>>;
  /// Where a thread that left this version went: the thread, and its
  /// version. Undefined for a thread that is still here.
  movedTo?(id: string): { id: string; commit: string } | undefined;
  /// Open a thread where it stands now, on the newest version.
  follow?(id: string): Promise<void>;
  /// The thread the agent answers now, from the server: an ask from a box
  /// or an earlier page reaches no card.
  answering?: ComputedRef<string | null>;
}

export const THREADS: InjectionKey<Threads> = Symbol('threads');

/// The prefix of a `parent` that names a thread of Gerrit, which the store
/// does not hold. See section 6.8 of the design.
export const GERRIT_PARENT = 'gerrit:';

/// What the reader last asked of every done thread on the screen: folded or
/// not. Null until the reader asks, so each card starts the way its state
/// says. Each request is a new object: the same key pressed twice must still
/// reach a card that the reader unfolded by hand in between.
export interface FoldDone {
  folded: boolean;
}

export const FOLD_DONE: InjectionKey<Ref<FoldDone | null>> = Symbol('fold done threads');

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

/// True when the last word of the thread is the reader's, so there is
/// something for the agent to answer. Under its own answer, asking again
/// would only buy the same answer twice.
export function awaitsAgent(remark: Comment, replies: Comment[]): boolean {
  return (replies[replies.length - 1] ?? remark).author === 'reader';
}

/// True when a thread of Gerrit is resolved: its newest comment says so,
/// the rule of Gerrit. The ssh answer gives no state: such a thread is
/// neither.
export function isResolved(remark: PostedComment, replies: PostedComment[]): boolean {
  return (replies[replies.length - 1] ?? remark).unresolved === false;
}

/// True when a reply written here resolves the thread of Gerrit: staged or
/// sent, it makes the thread done before Gerrit knows.
export function resolvesHere(replies: Comment[]): boolean {
  return replies.some((reply) => reply.gerrit?.resolve === true);
}
