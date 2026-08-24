// @vitest-environment jsdom

import { beforeEach, describe, expect, it } from 'vitest';
import { drop, places, read, slot, write } from './drafts';

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

  it('survives a store that holds nonsense', () => {
    localStorage.setItem('qreview.drafts', 'not json');

    expect(read(slot('Ione', 'a', 'new:1'))).toBe('');
  });
});
