import { describe, expect, it } from 'vitest';
import { isBlocked, lastWord, repliesBy, resolvesHere } from './threads';
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
    gerrit: null,
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

describe('lastWord', () => {
  it('is the newest reply of the thread, or the remark alone', () => {
    const remark = comment('a', { body: '/review' });
    const replies = [
      comment('2', { parent: 'a', body: 'final' }),
      comment('1', { parent: 'a', body: 'draft' }),
    ];

    expect(lastWord(remark, [remark])).toBe('/review');
    expect(lastWord(remark, [remark, ...replies])).toBe('final');
    expect(lastWord(replies[1]!, [remark, ...replies])).toBe('draft');
  });
});

describe('resolvesHere', () => {
  const mark = (state: 'staged' | 'sent') => ({
    state,
    patchSet: 1,
    resolve: true,
    id: 'd1',
    sentAt: null,
  });

  it('counts a resolving reply until Gerrit holds it, or lost it', () => {
    const staged = comment('1', { parent: 'gerrit:g1', gerrit: mark('staged') });
    const sent = comment('2', { parent: 'gerrit:g1', gerrit: mark('sent') });

    expect(resolvesHere([staged], () => 'published')).toBe(true);
    expect(resolvesHere([sent], () => 'draft')).toBe(true);
    expect(resolvesHere([sent], () => null)).toBe(true);
    expect(resolvesHere([sent], () => 'published')).toBe(false);
    expect(resolvesHere([sent], () => 'deleted')).toBe(false);
  });
});
