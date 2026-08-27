import { describe, expect, it } from 'vitest';
import { segments } from './segments';
import type { Row } from '@/api/types';

function row(
  text: string,
  tokens: [number, number, string][] = [],
  words: [number, number][] = [],
): Row {
  return {
    kind: 'add',
    oldLine: null,
    newLine: 1,
    text,
    tokens: tokens.map(([start, end, cls]) => ({ start, end, cls })),
    words: words.map(([start, end]) => ({ start, end })),
  };
}

describe('segments', () => {
  it('gives one plain piece when nothing claims the row', () => {
    expect(segments(row('plain text'))).toEqual([
      { text: 'plain text', cls: '', changed: false, marked: false, same: false },
    ]);
  });

  it('gives nothing for an empty row', () => {
    expect(segments(row(''))).toEqual([]);
  });

  it('keeps the text the row carries, whole and in order', () => {
    const out = segments(row('let a = 1;', [[0, 3, 'keyword']], [[8, 9]]));
    expect(out.map((s) => s.text).join('')).toBe('let a = 1;');
  });

  it('carries the syntax class of the piece', () => {
    const out = segments(row('int x;', [[0, 3, 'storage type']]));
    expect(out[0]).toEqual({
      text: 'int',
      cls: 'storage type',
      changed: false,
      marked: false,
      same: false,
    });
    expect(out[1].cls).toBe('');
  });

  it('marks the piece that changed inside the line', () => {
    const out = segments(row('a = two;', [], [[4, 7]]));
    expect(out.filter((s) => s.changed).map((s) => s.text)).toEqual(['two']);
  });

  it('cuts where a mark crosses a syntax span', () => {
    // The string spans 4..11, the change only 5..8.
    const out = segments(row('x = "abcdef"', [[4, 12, 'string']], [[5, 8]]));
    const changed = out.filter((s) => s.changed);

    expect(changed).toHaveLength(1);
    expect(changed[0].text).toBe('abc');
    expect(changed[0].cls).toBe('string');
    expect(out.map((s) => s.text).join('')).toBe('x = "abcdef"');
  });

  it('joins neighbours that carry the same facts', () => {
    // Two touching spans of one class are one piece in the DOM.
    const out = segments(
      row('abcdef', [
        [0, 3, 'comment'],
        [3, 6, 'comment'],
      ]),
    );
    expect(out).toHaveLength(1);
    expect(out[0].text).toBe('abcdef');
  });

  it('ignores a span that runs past the end of the row', () => {
    const out = segments(row('short', [[0, 99, 'keyword']]));
    expect(out.map((s) => s.text).join('')).toBe('short');
    expect(out[0].cls).toBe('keyword');
  });

  it('ignores an empty or reversed span', () => {
    const out = segments(
      row('abc', [
        [1, 1, 'x'],
        [3, 2, 'y'],
      ]),
    );
    expect(out).toEqual([{ text: 'abc', cls: '', changed: false, marked: false, same: false }]);
  });

  it('slices a line with an accent where the server said', () => {
    // "héllo world": the server counts é as one UTF-16 unit, so world is 6..11.
    const out = segments(row('héllo world', [], [[6, 11]]));
    expect(out.filter((s) => s.changed).map((s) => s.text)).toEqual(['world']);
  });

  it('cuts the row at the ends of a marked range', () => {
    const found = segments(row('one two three'), { start: 4, end: 7 });

    expect(found.map((s) => s.text)).toEqual(['one ', 'two', ' three']);
    expect(found.map((s) => s.marked)).toEqual([false, true, false]);
  });

  it('marks nothing when no range is given', () => {
    expect(segments(row('one two')).every((s) => !s.marked)).toBe(true);
  });

  it('cuts the row at the ends of the word the pointer is on', () => {
    const found = segments(row('fd = open(fd);'), undefined, [
      { start: 0, end: 2 },
      { start: 10, end: 12 },
    ]);

    expect(found.map((s) => s.text)).toEqual(['fd', ' = open(', 'fd', ');']);
    expect(found.map((s) => s.same)).toEqual([true, false, true, false]);
  });

  it('carries the word and the syntax class on the same piece', () => {
    const out = segments(row('int fd;', [[0, 3, 'storage type']]), undefined, [
      { start: 4, end: 6 },
    ]);
    const same = out.filter((s) => s.same);

    expect(same.map((s) => s.text)).toEqual(['fd']);
    expect(out[0].cls).toBe('storage type');
    expect(out.map((s) => s.text).join('')).toBe('int fd;');
  });

  it('keeps the word apart from what changed inside the line', () => {
    // The change covers `open(fd)`, the pointer only `fd`.
    const out = segments(row('x = open(fd);', [], [[4, 12]]), undefined, [{ start: 9, end: 11 }]);

    expect(out.filter((s) => s.same).map((s) => s.text)).toEqual(['fd']);
    expect(
      out
        .filter((s) => s.changed)
        .map((s) => s.text)
        .join(''),
    ).toBe('open(fd)');
  });

  it('marks nothing when the pointer is on no word', () => {
    expect(segments(row('one two')).every((s) => !s.same)).toBe(true);
  });
});
