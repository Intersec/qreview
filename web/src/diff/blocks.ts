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

/// How far down its block a line stands: 0 at the top, under 1 at the
/// bottom.
///
/// The text of a block wraps to the width of the pane, so a line of the
/// source has no place of its own on the page. Its share of the lines of the
/// block is the best guess, and it keeps the two sides moving together.
export function within(block: Block, line: number): number {
  const span = block.to - block.from + 1;
  return Math.min(Math.max(line - block.from, 0), span - 1) / span;
}

/// The line at a share of the height of a block. The inverse of `within`.
export function lineWithin(block: Block, share: number): number {
  const span = block.to - block.from + 1;
  const row = Math.floor(Math.min(Math.max(share, 0), 1) * span);
  return Math.min(block.from + row, block.to);
}
