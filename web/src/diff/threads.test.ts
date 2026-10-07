import { describe, expect, it } from 'vitest';
import { isBlocked, repliesBy } from './threads';
import type { Comment } from '@/api/types';

function comment(id: string, extra: Partial<Comment> = {}): Comment {
  return {
    id,
    patchSet: 1,
    commit: 'abc',
    createdAt: id,
    updatedAt: id,
    scope: 'line',
    body: id,
    anchor: null,
    author: 'reader',
    parent: null,
    done: false,
    blocked: false,
    ...extra,
  };
}

describe('repliesBy', () => {
  it('puts each reply under its remark, oldest first', () => {
    const replies = repliesBy([
      comment('a'),
      comment('3', { parent: 'a' }),
      comment('1', { parent: 'a' }),
      comment('2', { parent: 'b' }),
    ]);

    expect(replies.get('a')?.map((c) => c.id)).toEqual(['1', '3']);
    expect(replies.get('b')?.map((c) => c.id)).toEqual(['2']);
    expect(replies.has('3')).toBe(false);
  });
});

describe('isBlocked', () => {
  const question = comment('1', { parent: 'a', author: 'agent', blocked: true });

  it('waits while the last reply is a question of the agent', () => {
    expect(isBlocked([question])).toBe(true);
  });

  it('ends when the reader answers', () => {
    expect(isBlocked([question, comment('2', { parent: 'a' })])).toBe(false);
  });

  it('does not wait on a thread with no reply', () => {
    expect(isBlocked([])).toBe(false);
  });
});
