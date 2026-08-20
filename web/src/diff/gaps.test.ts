import { describe, expect, it } from 'vitest';
import { gaps } from './gaps';
import type { Hunk } from '@/api/types';

function hunk(oldStart: number, oldLines: number, newStart: number, newLines: number): Hunk {
  return { oldStart, oldLines, newStart, newLines, header: '', rows: [] };
}

describe('gaps', () => {
  it('finds nothing in a file with no hunk', () => {
    expect(gaps([], 100)).toEqual([]);
  });

  it('finds the lines above the first hunk', () => {
    const [gap] = gaps([hunk(10, 6, 10, 6)], null);
    expect([gap.from, gap.to]).toEqual([1, 9]);
  });

  it('finds nothing above a hunk that starts at the first line', () => {
    expect(gaps([hunk(1, 6, 1, 6)], null)).toEqual([]);
  });

  it('finds the lines between two hunks', () => {
    const found = gaps([hunk(1, 6, 1, 6), hunk(24, 7, 24, 7)], null);
    expect(found).toHaveLength(1);
    expect([found[0].from, found[0].to]).toEqual([7, 23]);
  });

  it('finds the lines after the last hunk when the length is known', () => {
    const found = gaps([hunk(1, 6, 1, 6)], 30);
    expect(found.map((g) => g.key)).toEqual(['after']);
    expect([found[0].from, found[0].to]).toEqual([7, 30]);
  });

  it('finds nothing after the last hunk when the length is not known', () => {
    expect(gaps([hunk(1, 6, 1, 6)], null)).toEqual([]);
  });

  it('finds nothing after a hunk that reaches the end', () => {
    expect(gaps([hunk(1, 30, 1, 30)], 30)).toEqual([]);
  });

  it('carries the offset from the new side to the old one', () => {
    // Six lines were added above, so the old side is six behind.
    const [gap] = gaps([hunk(4, 5, 10, 5)], null);
    expect(gap.offset).toBe(-6);
  });

  it('gives each gap a key of its own', () => {
    const found = gaps([hunk(10, 2, 10, 2), hunk(30, 2, 30, 2)], 50);
    expect(new Set(found.map((g) => g.key)).size).toBe(found.length);
  });
});
