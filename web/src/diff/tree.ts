// The files of a change, grouped by the directory they live in.
//
// A flat list of long paths in a narrow pane is unreadable. The directory is
// said once, and the files under it carry their name alone.

import type { FileEntry } from '@/api/types';
import { label } from '@/diff/paths';

export interface Group {
  /// The directory, or an empty string for the top of the tree.
  dir: string;
  files: { entry: FileEntry; name: string }[];
}

export function group(files: FileEntry[]): Group[] {
  const out: Group[] = [];

  for (const entry of files) {
    // A path that opens with a slash is not in the tree. The commit message
    // is the one such file, and it lives under no directory.
    const outside = entry.path.startsWith('/');
    const cut = entry.path.lastIndexOf('/');
    const dir = outside || cut === -1 ? '' : entry.path.slice(0, cut);
    const name = outside || cut === -1 ? label(entry.path) : entry.path.slice(cut + 1);

    // The order of the files is the order git gave them, so a directory
    // that comes back later gets a group of its own rather than jumping up.
    const last = out[out.length - 1];
    if (last && last.dir === dir) {
      last.files.push({ entry, name });
      continue;
    }
    out.push({ dir, files: [{ entry, name }] });
  }
  return out;
}
