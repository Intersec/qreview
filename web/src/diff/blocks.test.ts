import { describe, expect, it } from 'vitest';
import { blockOf, linesOf } from './blocks';

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
