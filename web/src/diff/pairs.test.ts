import { describe, expect, it } from 'vitest';
import { pairs } from './pairs';
import type { Row } from '@/api/types';

function row(kind: Row['kind'], text: string): Row {
  const old = kind === 'add' ? null : 1;
  const fresh = kind === 'remove' ? null : 1;
  return { kind, oldLine: old, newLine: fresh, text };
}

describe('pairs', () => {
  it('puts a context row on both sides', () => {
    const [pair] = pairs([row('context', 'same')]);
    expect(pair.left?.text).toBe('same');
    expect(pair.right?.text).toBe('same');
  });

  it('faces a removal with the addition that replaced it', () => {
    const out = pairs([row('remove', 'old'), row('add', 'new')]);
    expect(out).toHaveLength(1);
    expect([out[0].left?.text, out[0].right?.text]).toEqual(['old', 'new']);
  });

  it('leaves the right side empty when nothing replaced the removal', () => {
    const out = pairs([row('remove', 'a'), row('remove', 'b'), row('add', 'A')]);
    expect(out.map((p) => [p.left?.text ?? null, p.right?.text ?? null])).toEqual([
      ['a', 'A'],
      ['b', null],
    ]);
  });

  it('leaves the left side empty when the addition replaced nothing', () => {
    const out = pairs([row('remove', 'a'), row('add', 'A'), row('add', 'B')]);
    expect(out.map((p) => [p.left?.text ?? null, p.right?.text ?? null])).toEqual([
      ['a', 'A'],
      [null, 'B'],
    ]);
  });

  it('keeps a pure addition on the right', () => {
    const out = pairs([row('add', 'only')]);
    expect(out).toEqual([{ left: null, right: out[0].right }]);
  });

  it('keeps every row, in order', () => {
    const rows = [
      row('context', 'one'),
      row('remove', 'two'),
      row('add', 'TWO'),
      row('context', 'three'),
      row('add', 'four'),
    ];
    const out = pairs(rows);
    const seen = out.flatMap((p) => [p.left, p.right]).filter((r): r is Row => r !== null);

    // A context row is on both sides, so it is counted twice.
    expect(seen.filter((r) => r.kind !== 'context')).toHaveLength(3);
    expect(out.map((p) => p.right?.text ?? p.left?.text)).toEqual(['one', 'TWO', 'three', 'four']);
  });
});
