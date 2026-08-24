// What a reader has typed and has not saved yet.
//
// A remark takes a minute to write, and opening another file used to throw
// it away. It is kept in the browser instead, under the change and the place
// it belongs to, and dropped when the comment is saved or the box is closed.
//
// The browser and not the server: a half-written remark is not a comment. It
// must not reach the export, and it must not be one more thing to delete.

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
