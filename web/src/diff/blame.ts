// The blame column, the way Gerrit draws it: a date and a name on each line
// of the left side, and the commit in a tooltip.

import type { Blame, BlameCommit } from '@/api/types';

/** What one line of the left side shows. */
export interface BlameLine {
  commit: BlameCommit;
  /// Where the line stands in the commit that wrote it.
  origin: number;
}

/** Where a click on a line of the blame goes: the change of the series that
 * the commit is, or else the page of its change on Gerrit. */
export type BlameTarget = { key: string } | { url: string };

/** The blame, by line of the file on the left. */
export function byLine(blame: Blame): Map<number, BlameLine> {
  const lines = new Map<number, BlameLine>();

  for (const commit of blame.commits) {
    for (const range of commit.ranges) {
      for (let line = range.start; line <= range.end; line += 1) {
        lines.set(line, { commit, origin: range.origin + line - range.start });
      }
    }
  }

  return lines;
}

/** The commits that wrote the left side of some lines, each one once, in a
 * fixed order. A line that stands only on the right has none. */
export function commitsOn(
  lines: Map<number, BlameLine>,
  rows: { oldLine: number | null }[],
): string[] {
  const found = new Set<string>();

  for (const row of rows) {
    const at = row.oldLine === null ? undefined : lines.get(row.oldLine);
    if (at) {
      found.add(at.commit.commit);
    }
  }
  return [...found].sort();
}

/** `2026-10-30 Nicolas`: the day and the first name, as Gerrit writes them. */
export function label(commit: BlameCommit): string {
  const name = commit.author.trim().split(/\s+/)[0] ?? '';

  return `${commit.date.slice(0, 10)} ${name}`.trim();
}

/** The first lines of the tooltip, the way `git log` heads a commit. */
export function head(commit: BlameCommit): string[] {
  const author = commit.email ? `${commit.author} <${commit.email}>` : commit.author;

  return [
    `Commit ${commit.commit}`,
    `Author: ${author}`,
    `Date: ${commit.date.slice(0, 16).replace('T', ' ')}`,
  ];
}
