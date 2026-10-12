// What a reader has typed and has not saved yet.
//
// A remark takes a minute to write, and opening another file used to throw
// it away. It is kept in the browser instead, under the change and the place
// it belongs to, and dropped when the comment is saved or the box is closed.
//
// The browser and not the server: a half-written remark is not a comment. It
// must not reach the export, and it must not be one more thing to delete.

import type { DraftPlace } from '@/api/types';
import { COMMIT_MSG } from './paths';

const KEY = 'qreview.drafts';

/// Where a draft belongs: the change, the file, and the place in the file.
export function slot(change: string, file: string, where: string): string {
  return `${change}|${file}|${where}`;
}

function all(): Record<string, string> {
  try {
    const held = localStorage.getItem(KEY);
    const read: unknown = held === null ? {} : JSON.parse(held);

    return read !== null && typeof read === 'object' ? (read as Record<string, string>) : {};
  } catch {
    // A corrupt or refused store loses the drafts and nothing else.
    return {};
  }
}

function keep(drafts: Record<string, string>) {
  try {
    localStorage.setItem(KEY, JSON.stringify(drafts));
  } catch {
    // A full or refused store is not worth an error in the reader's face.
  }
}

export function read(at: string): string {
  return all()[at] ?? '';
}

export function write(at: string, text: string) {
  const drafts = all();

  if (text.trim() === '') {
    delete drafts[at];
  } else {
    drafts[at] = text;
  }
  keep(drafts);
}

export function drop(at: string) {
  write(at, '');
}

/// Every place of one change and file that holds a draft.
export function places(change: string, file: string): string[] {
  const head = `${change}|${file}|`;

  return Object.keys(all())
    .filter((at) => at.startsWith(head))
    .map((at) => at.slice(head.length));
}

/// Where the draft of a reply, or of an edit, to a thread is kept. A thread
/// keeps its id when it moves to a new version, so its drafts follow it.
export function threadSlot(id: string, box: 'reply' | 'edit'): string {
  return slot('thread', id, box);
}

/// Move the drafts of one version of a change onto the next one.
///
/// `map` says where each place landed, the way the server moved the
/// threads. A draft whose file is gone goes to the commit message, with a
/// line that says where it was typed: what the reader wrote is never lost.
export async function moveDrafts(
  fromKey: string,
  toKey: string,
  map: (places: DraftPlace[]) => Promise<(DraftPlace | null)[]>,
): Promise<number> {
  const head = `${fromKey}|`;
  const held = Object.entries(all()).filter(([at]) => at.startsWith(head));
  if (held.length === 0) {
    return 0;
  }
  const places = held.map(([at]) => parse(at.slice(head.length)));
  const landed = await map(places);

  // Every draft leaves first, so one that lands where another stood does
  // not take its text.
  const drafts = all();
  for (const [at] of held) {
    delete drafts[at];
  }
  held.forEach(([, text], i) => {
    const was = places[i]!;
    const now = landed[i];
    let to: string;
    let body = text;
    if (now) {
      to = slot(toKey, now.file, where(now));
    } else {
      to = slot(toKey, COMMIT_MSG, 'new:1');
      body = `(typed on ${was.file}${was.line === null ? '' : `:${was.line}`})\n${text}`;
    }
    drafts[to] = drafts[to] ? `${drafts[to]}\n\n${body}` : body;
  });
  keep(drafts);

  return held.length;
}

/// `file|side:line`, or `file|` for a draft that has no line.
function parse(rest: string): DraftPlace {
  const cut = rest.lastIndexOf('|');
  const file = rest.slice(0, cut);
  const [side, line] = rest.slice(cut + 1).split(':');

  return {
    file,
    side: side === 'old' ? 'old' : 'new',
    line: line === undefined || line === '' ? null : Number(line),
  };
}

function where(place: DraftPlace): string {
  return place.line === null ? '' : `${place.side}:${place.line}`;
}
