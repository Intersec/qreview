import { describe, expect, it } from 'vitest';
import { occurrences, wordAt } from './hover';

describe('wordAt', () => {
  const line = 'int connect_once(int fd)';

  it('reads the name the caret stands in', () => {
    expect(wordAt(line, 6)).toBe('connect_once');
  });

  it('reads the name the caret opens', () => {
    expect(wordAt(line, 4)).toBe('connect_once');
  });

  it('reads the name the caret closes', () => {
    // Right after the last letter, which is where a pointer on it lands.
    expect(wordAt(line, 16)).toBe('connect_once');
  });

  it('gives nothing when no name touches the caret', () => {
    expect(wordAt('a  b', 2)).toBeNull();
    expect(wordAt('  ', 1)).toBeNull();
  });

  it('gives nothing past the end of the line', () => {
    expect(wordAt('int', 9)).toBeNull();
    expect(wordAt('', 0)).toBeNull();
  });

  it('keeps an underscore and a dollar inside a name', () => {
    expect(wordAt('let $a_1 = 2;', 5)).toBe('$a_1');
  });

  it('cuts a name on a dot and on a dash', () => {
    expect(wordAt('foo.bar', 1)).toBe('foo');
    expect(wordAt('font-size: 1px', 6)).toBe('size');
  });

  it('gives nothing for a run of digits', () => {
    expect(wordAt('x = 4096;', 5)).toBeNull();
  });

  it('keeps a name that only starts with a digit', () => {
    expect(wordAt('0x1f', 1)).toBe('0x1f');
  });

  it('counts an accent the way the wire counts it', () => {
    expect(wordAt('héllo world', 8)).toBe('world');
  });
});

describe('occurrences', () => {
  it('finds the word wherever it stands alone', () => {
    expect(occurrences('fd = open(fd);', 'fd')).toEqual([
      { start: 0, end: 2 },
      { start: 10, end: 12 },
    ]);
  });

  it('leaves a longer name alone', () => {
    expect(occurrences('int fdesc, myfd, fd_2;', 'fd')).toEqual([]);
  });

  it('finds the word at both ends of the line', () => {
    expect(occurrences('fd', 'fd')).toEqual([{ start: 0, end: 2 }]);
  });

  it('finds nothing in a line that does not carry it', () => {
    expect(occurrences('return 1;', 'fd')).toEqual([]);
  });

  it('does not step over a match it has to reject', () => {
    // The first `aa` is inside `aaa`, and the second one is not.
    expect(occurrences('aaa aa', 'aa')).toEqual([{ start: 4, end: 6 }]);
  });

  it('finds nothing for an empty word', () => {
    expect(occurrences('anything', '')).toEqual([]);
  });
});
