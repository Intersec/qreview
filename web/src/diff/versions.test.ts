import { describe, expect, it } from 'vitest';
import { isCurrent, rounds } from './versions';
import type { Comment } from '@/api/types';

function remark(id: string, commit: string): Comment {
  return {
    id,
    patchSet: 1,
    commit,
    createdAt: '',
    updatedAt: '',
    scope: 'line',
    body: id,
    anchor: null,
  };
}

describe('isCurrent', () => {
  it('takes a remark written on the version being read', () => {
    expect(isCurrent(remark('a', 'abc'), 'abc')).toBe(true);
  });

  it('leaves out one written on another version', () => {
    expect(isCurrent(remark('a', 'old'), 'abc')).toBe(false);
  });

  it('keeps one that names no version at all', () => {
    // A store older than format 3. It is the only version it can belong to.
    expect(isCurrent(remark('a', ''), 'abc')).toBe(true);
  });
});

describe('rounds', () => {
  it('tells the current remarks from the previous ones', () => {
    const change = {
      commit: 'now',
      comments: [remark('a', 'now'), remark('b', 'before'), remark('c', '')],
    };

    const { current, previous } = rounds(change);

    expect(current.map((c) => c.id)).toEqual(['a', 'c']);
    expect(previous.map((c) => c.id)).toEqual(['b']);
  });
});
