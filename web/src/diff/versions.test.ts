import { describe, expect, it } from 'vitest';
import { ofVersion, rounds } from './versions';
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

describe('ofVersion', () => {
  it('takes a remark written on the version being read', () => {
    expect(ofVersion(remark('a', 'abc'), 'abc')).toBe(true);
  });

  it('leaves out one written on another version', () => {
    expect(ofVersion(remark('a', 'old'), 'abc')).toBe(false);
  });

  it('keeps one that names no version at all', () => {
    // A store older than format 3. It is the only version it can belong to.
    expect(ofVersion(remark('a', ''), 'abc')).toBe(true);
  });
});

describe('rounds', () => {
  it('tells the round being written from the one before it', () => {
    const change = {
      commit: 'now',
      comments: [remark('a', 'now'), remark('b', 'before'), remark('c', '')],
    };

    const { now, earlier } = rounds(change);

    expect(now.map((c) => c.id)).toEqual(['a', 'c']);
    expect(earlier.map((c) => c.id)).toEqual(['b']);
  });
});
