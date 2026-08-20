// The rows of a hunk, paired for a side-by-side view.
//
// A unified hunk is a list. A side-by-side view needs two columns, so a run
// of removals and the run of additions after it are read as versions of each
// other, in order, the way a reader reads them.

import type { Row } from '@/api/types';

export interface Pair {
  left: Row | null;
  right: Row | null;
}

export function pairs(rows: Row[]): Pair[] {
  const out: Pair[] = [];
  let index = 0;

  while (index < rows.length) {
    const row = rows[index];

    if (row.kind === 'context') {
      out.push({ left: row, right: row });
      index += 1;
      continue;
    }

    const removes = run(rows, index, 'remove');
    const adds = run(rows, index + removes, 'add');

    for (let i = 0; i < Math.max(removes, adds); i += 1) {
      out.push({
        left: i < removes ? rows[index + i] : null,
        right: i < adds ? rows[index + removes + i] : null,
      });
    }
    index += removes + adds;
  }
  return out;
}

function run(rows: Row[], from: number, kind: Row['kind']): number {
  let count = 0;
  while (from + count < rows.length && rows[from + count].kind === kind) {
    count += 1;
  }
  return count;
}
