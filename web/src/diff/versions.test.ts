import { describe, expect, it } from 'vitest';
import { isCurrent, open, previousGroups, rounds, toHandle } from './versions';
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
    author: 'reader',
    parent: null,
    done: false,
    blocked: false,
    gerrit: null,
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

describe('toHandle', () => {
  it('keeps a remark staged for Gerrit until it is sent, which open leaves out', () => {
    const mark = (state: 'staged' | 'sent') => ({
      state,
      patchSet: 1,
      resolve: false,
      id: null,
      sentAt: null,
    });
    const staged = { ...remark('staged', 'now'), gerrit: mark('staged') };
    const sent = { ...remark('sent', 'now'), gerrit: mark('sent') };
    const done = { ...remark('done', 'now'), done: true };
    const change = {
      commit: 'now',
      comments: [remark('open', 'now'), staged, sent, done, remark('before', 'old')],
    };

    expect(toHandle(change).map((c) => c.id)).toEqual(['open', 'staged']);
    expect(open(change).map((c) => c.id)).toEqual(['open']);
  });
});

describe('previousGroups', () => {
  const change = {
    key: 'Iwork',
    subject: 'work: a change',
    commit: 'now',
    comments: [remark('a', 'now'), remark('b', 'one'), remark('c', 'two'), remark('d', 'one')],
    versions: [
      { commit: 'two', subject: 'work: the second try' },
      { commit: 'one', subject: 'work: the first try' },
    ],
  };

  it('groups the previous remarks by the version they were written on', () => {
    const groups = previousGroups(change);

    expect(groups.map((g) => g.version.commit)).toEqual(['two', 'one']);
    expect(groups[0].comments.map((c) => c.id)).toEqual(['c']);
    expect(groups[1].comments.map((c) => c.id)).toEqual(['b', 'd']);
  });

  it('leaves out a version no remark names any more', () => {
    const gone = { ...change, comments: [remark('a', 'now'), remark('b', 'one')] };

    expect(previousGroups(gone).map((g) => g.version.commit)).toEqual(['one']);
  });
});

describe('open', () => {
  it('counts the remarks of this version that are not done, and no reply', () => {
    const done = { ...remark('done', 'abc'), done: true };
    const reply = { ...remark('reply', 'abc'), parent: 'a', author: 'agent' as const };
    const change = {
      commit: 'abc',
      comments: [remark('a', 'abc'), done, reply, remark('old', 'old')],
    };

    expect(open(change).map((c) => c.id)).toEqual(['a']);
    expect(rounds(change).current.map((c) => c.id)).toEqual(['a', 'done']);
  });
});
