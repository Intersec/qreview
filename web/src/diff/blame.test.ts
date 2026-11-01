import { describe, expect, it } from 'vitest';
import { byLine, head, label } from './blame';
import type { BlameCommit, BlameRange } from '@/api/types';

function commit(sha: string, author: string, ranges: BlameRange[]): BlameCommit {
  return {
    commit: sha.repeat(40),
    author,
    email: `${author.split(' ')[0].toLowerCase()}@example.com`,
    date: '2026-10-30T14:02:11Z',
    message: 'net: fix the retry loop\n\nThe loop never stopped.\n',
    path: 'src/net.c',
    ranges,
  };
}

describe('byLine', () => {
  it('names the commit of every line of each range', () => {
    const a = commit('a', 'Jane Doe', [{ start: 1, end: 2, origin: 10 }]);
    const b = commit('b', 'John Roe', [{ start: 3, end: 3, origin: 1 }]);

    const lines = byLine({ commits: [a, b] });

    expect([...lines.keys()]).toEqual([1, 2, 3]);
    expect(lines.get(2)?.commit).toBe(a);
    expect(lines.get(3)?.commit).toBe(b);
  });

  it('counts each line in the commit that wrote it', () => {
    const a = commit('a', 'Jane Doe', [{ start: 5, end: 7, origin: 40 }]);

    expect(byLine({ commits: [a] }).get(7)?.origin).toBe(42);
  });
});

describe('label', () => {
  it('writes the day and the first name, as Gerrit does', () => {
    expect(label(commit('a', 'Jane Q. Doe', []))).toBe('2026-10-30 Jane');
  });
});

describe('head', () => {
  it('heads the tooltip the way git log heads a commit', () => {
    expect(head(commit('a', 'Jane Doe', []))).toEqual([
      `Commit ${'a'.repeat(40)}`,
      'Author: Jane Doe <jane@example.com>',
      'Date: 2026-10-30 14:02',
    ]);
  });
});
