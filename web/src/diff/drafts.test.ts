// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from 'vitest';
import { drop, moveDrafts, places, read, slot, write } from './drafts';
import type { DraftPlace } from '@/api/types';

describe('drafts', () => {
  beforeEach(() => localStorage.clear());

  it('keeps what was typed, under the place it belongs to', () => {
    const at = slot('Ione', 'src/a.c', 'new:12');
    write(at, 'half a remark');

    expect(read(at)).toBe('half a remark');
    expect(read(slot('Ione', 'src/b.c', 'new:12'))).toBe('');
  });

  it('drops a draft that is emptied, and one that is saved', () => {
    const at = slot('Ione', 'src/a.c', 'new:12');
    write(at, 'a remark');
    write(at, '   ');
    expect(read(at)).toBe('');

    write(at, 'again');
    drop(at);
    expect(read(at)).toBe('');
  });

  it('lists the places of one file that hold a draft', () => {
    write(slot('Ione', 'src/a.c', 'new:1'), 'one');
    write(slot('Ione', 'src/a.c', 'old:4'), 'two');
    write(slot('Ione', 'src/b.c', 'new:1'), 'three');
    write(slot('Itwo', 'src/a.c', 'new:9'), 'four');

    expect(places('Ione', 'src/a.c').sort()).toEqual(['new:1', 'old:4']);
  });

  it('follows a new version of the change, line by line', async () => {
    write(slot('sha-old', 'src/a.c', 'new:2'), 'on two');
    write(slot('sha-old', 'src/a.c', 'old:5'), 'before');
    write(slot('sha-other', 'src/a.c', 'new:2'), 'not this change');
    let asked: DraftPlace[] = [];

    const moved = await moveDrafts('sha-old', 'sha-new', async (held) => {
      asked = held;
      return held.map((place) => ({ ...place, line: (place.line ?? 0) + 1 }));
    });

    expect(moved).toBe(2);
    expect(asked).toContainEqual({ file: 'src/a.c', side: 'old', line: 5 });
    expect(read(slot('sha-new', 'src/a.c', 'new:3'))).toBe('on two');
    expect(read(slot('sha-new', 'src/a.c', 'old:6'))).toBe('before');
    expect(read(slot('sha-old', 'src/a.c', 'new:2'))).toBe('');
    expect(read(slot('sha-other', 'src/a.c', 'new:2'))).toBe('not this change');
  });

  it('puts a draft whose file is gone on the commit message, never away', async () => {
    write(slot('Ione', 'gone.c', 'new:7'), 'still mine');

    await moveDrafts('Ione', 'Ione', async (held) => held.map(() => null));

    expect(read(slot('Ione', '/COMMIT_MSG', 'new:1'))).toBe('(typed on gone.c:7)\nstill mine');
    expect(read(slot('Ione', 'gone.c', 'new:7'))).toBe('');
  });

  it('moves two drafts down one line each, with no mix', async () => {
    write(slot('Ione', 'a.c', 'new:3'), 'three');
    write(slot('Ione', 'a.c', 'new:4'), 'four');

    await moveDrafts('Ione', 'Ione', async (held) =>
      held.map((place) => ({ ...place, line: (place.line ?? 0) + 1 })),
    );

    expect(read(slot('Ione', 'a.c', 'new:4'))).toBe('three');
    expect(read(slot('Ione', 'a.c', 'new:5'))).toBe('four');
  });

  it('asks nothing when the change holds no draft', async () => {
    let asked = false;
    await moveDrafts('Ione', 'Ione', async () => {
      asked = true;
      return [];
    });

    expect(asked).toBe(false);
  });

  it('survives a store that holds nonsense', () => {
    localStorage.setItem('qreview.drafts', 'not json');

    expect(read(slot('Ione', 'a', 'new:1'))).toBe('');
  });
});
