import { describe, expect, it } from 'vitest';
import { format, parse, same } from './place';

describe('format', () => {
  it('names the change alone when nothing else is open', () => {
    expect(format({ change: 'I8f3ac21' })).toBe('#change=I8f3ac21');
  });

  it('keeps the slashes of a path, so the URL stays readable', () => {
    expect(format({ change: 'I8f3ac21', file: 'src/net.blk' })).toBe(
      '#change=I8f3ac21&file=src/net.blk',
    );
  });

  it('carries the patch set and what it is read against', () => {
    expect(format({ change: 'I8f3ac21', file: 'a.c', ps: 2, base: 'ps:1' })).toBe(
      '#change=I8f3ac21&file=a.c&ps=2&base=ps:1',
    );
  });
});

describe('parse', () => {
  it('reads back what format wrote', () => {
    const place = { change: 'sha-4a91f0', file: 'src/net.blk', ps: 3, base: 'parent2' };

    expect(parse(format(place))).toEqual(place);
  });

  it('reads back a file whose name carries what a URL uses', () => {
    const place = { change: 'I1', file: 'src/a & b + c.md' };

    expect(parse(format(place))).toEqual({ ...place, ps: undefined, base: undefined });
  });

  it('names no place without a change', () => {
    expect(parse('')).toBeNull();
    expect(parse('#')).toBeNull();
    expect(parse('#file=src/net.blk')).toBeNull();
  });

  it('leaves out a patch set that is not a number', () => {
    expect(parse('#change=I1&ps=newest')?.ps).toBeUndefined();
  });
});

describe('same', () => {
  it('is true for two places that say the same thing', () => {
    expect(same({ change: 'I1', file: 'a.c' }, { change: 'I1', file: 'a.c' })).toBe(true);
  });

  it('tells a file apart, and a patch set apart', () => {
    expect(same({ change: 'I1', file: 'a.c' }, { change: 'I1', file: 'b.c' })).toBe(false);
    expect(same({ change: 'I1', ps: 1 }, { change: 'I1', ps: 2 })).toBe(false);
  });

  it('is true for no place twice, and false against one', () => {
    expect(same(null, null)).toBe(true);
    expect(same(null, { change: 'I1' })).toBe(false);
  });
});
