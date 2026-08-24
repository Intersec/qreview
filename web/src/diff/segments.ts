// One row, cut into the pieces a template can paint.
//
// A row carries two sets of spans: the syntax classes and the intra-line
// marks. They overlap freely, so the row is cut at every boundary of either
// set and each piece knows both facts.

import type { Row, Span, WordSpan } from '@/api/types';

export interface Segment {
  text: string;
  /** The syntax classes, empty when nothing claims the piece. */
  cls: string;
  /** True when the piece is part of what changed inside the line. */
  changed: boolean;
  /** True when the piece is inside the range a comment covers. */
  marked: boolean;
}

/** A range of the row, in UTF-16 units, that a comment covers. */
export interface Mark {
  start: number;
  end: number;
}

export function segments(row: Row, mark?: Mark): Segment[] {
  const text = row.text;
  if (text === '') {
    return [];
  }

  const tokens = clamp(row.tokens ?? [], text.length);
  const words = clamp(row.words ?? [], text.length);
  const marks = mark ? clamp([mark], text.length) : [];

  if (tokens.length === 0 && words.length === 0 && marks.length === 0) {
    return [{ text, cls: '', changed: false, marked: false }];
  }

  const cuts = new Set<number>([0, text.length]);
  for (const span of [...tokens, ...words, ...marks]) {
    cuts.add(span.start);
    cuts.add(span.end);
  }

  const points = [...cuts].sort((a, b) => a - b);
  const out: Segment[] = [];

  for (let i = 0; i + 1 < points.length; i += 1) {
    const start = points[i];
    const end = points[i + 1];
    if (start >= end) {
      continue;
    }
    out.push({
      text: text.slice(start, end),
      cls: covering(tokens, start)?.cls ?? '',
      changed: covering(words, start) !== undefined,
      marked: covering(marks, start) !== undefined,
    });
  }
  return merge(out);
}

function clamp<T extends Span | WordSpan | Mark>(spans: T[], length: number): T[] {
  return spans
    .filter((s) => s.start < s.end && s.start < length)
    .map((s) => ({ ...s, end: Math.min(s.end, length) }));
}

function covering<T extends Span | WordSpan | Mark>(spans: T[], at: number): T | undefined {
  return spans.find((s) => s.start <= at && at < s.end);
}

/** Join neighbours that carry the same two facts, so the DOM stays small. */
function merge(segments: Segment[]): Segment[] {
  const out: Segment[] = [];

  for (const segment of segments) {
    const last = out[out.length - 1];
    if (
      last &&
      last.cls === segment.cls &&
      last.changed === segment.changed &&
      last.marked === segment.marked
    ) {
      last.text += segment.text;
      continue;
    }
    out.push({ ...segment });
  }
  return out;
}
