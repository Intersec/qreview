// The blocks of a rendered Markdown file, and the lines each one came from.
//
// The server writes `data-lines="12-18"` on every leaf block. The diff
// scrolls by lines of the source, so this is where a line finds the block
// the rendered pane brings up. See `roadmap/design.md` section 3.8.

export interface Block {
  from: number;
  to: number;
}

/// The lines of a `data-lines` value, or null when it says nothing.
export function linesOf(value: string | undefined): Block | null {
  const match = /^(\d+)-(\d+)$/.exec(value ?? '');
  if (!match) {
    return null;
  }
  return { from: Number(match[1]), to: Number(match[2]) };
}

/// The block a line of the new side shows on, as an index into `blocks`.
///
/// A blank line between two blocks belongs to no block, so it counts for the
/// next one. A line past the last block has no block.
export function blockOf(blocks: Block[], line: number): number | null {
  const holding = blocks.findIndex((block) => block.from <= line && line <= block.to);
  if (holding !== -1) {
    return holding;
  }
  const next = blocks.findIndex((block) => block.from > line);
  return next === -1 ? null : next;
}
