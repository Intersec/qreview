// The lines a diff does not carry.
//
// A hunk holds what changed and a few lines around it. Everything between two
// hunks, above the first and below the last, is a gap the reader can open.

import type { Hunk } from '@/api/types';

export interface Gap {
  /// Stable across renders, so an opened gap stays open.
  key: string;
  /// The first and the last line of the gap, on the new side.
  from: number;
  to: number;
  /// What to add to a new line number to get the old one.
  offset: number;
}

export function gaps(hunks: Hunk[], lineCount: number | null): Gap[] {
  const out: Gap[] = [];
  if (hunks.length === 0) {
    return out;
  }

  hunks.forEach((hunk, index) => {
    const previous = hunks[index - 1];
    const from = previous ? previous.newStart + previous.newLines : 1;
    const to = hunk.newStart - 1;

    if (to >= from) {
      out.push({
        key: `before-${index}`,
        from,
        to,
        offset: hunk.oldStart - hunk.newStart,
      });
    }
  });

  const last = hunks[hunks.length - 1];
  const from = last.newStart + last.newLines;
  if (lineCount !== null && lineCount >= from) {
    out.push({
      key: 'after',
      from,
      to: lineCount,
      offset: last.oldStart + last.oldLines - (last.newStart + last.newLines),
    });
  }
  return out;
}
