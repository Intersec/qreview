import { describe, expect, it } from 'vitest';
import { blockOf, lineWithin, linesOf, within } from './blocks';

describe('linesOf', () => {
  it('reads a range', () => {
    expect(linesOf('12-18')).toEqual({ from: 12, to: 18 });
  });

  it('says nothing about a value it cannot read', () => {
    expect(linesOf(undefined)).toBeNull();
    expect(linesOf('12')).toBeNull();
  });
});

describe('blockOf', () => {
  const blocks = [
    { from: 1, to: 1 },
    { from: 3, to: 5 },
    { from: 8, to: 8 },
  ];

  it('finds the block that holds the line', () => {
    expect(blockOf(blocks, 1)).toBe(0);
    expect(blockOf(blocks, 4)).toBe(1);
    expect(blockOf(blocks, 8)).toBe(2);
  });

  it('puts a blank line on the next block', () => {
    expect(blockOf(blocks, 2)).toBe(1);
    expect(blockOf(blocks, 6)).toBe(2);
  });

  it('has no place past the last block', () => {
    expect(blockOf(blocks, 9)).toBeNull();
  });
});

describe('within', () => {
  const block = { from: 10, to: 13 };

  it('places a line by its share of the lines of the block', () => {
    expect(within(block, 10)).toBe(0);
    expect(within(block, 12)).toBe(0.5);
  });

  it('keeps a line outside the block at an edge', () => {
    expect(within(block, 2)).toBe(0);
    expect(within(block, 40)).toBe(0.75);
  });

  it('comes back to the line through lineWithin', () => {
    for (let line = 10; line <= 13; line += 1) {
      expect(lineWithin(block, within(block, line))).toBe(line);
    }
  });
});

describe('lineWithin', () => {
  it('keeps a share past the bottom on the last line', () => {
    expect(lineWithin({ from: 10, to: 13 }, 1)).toBe(13);
    expect(lineWithin({ from: 10, to: 13 }, -0.2)).toBe(10);
  });
});
