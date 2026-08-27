<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from 'vue';
import CommentBox from './CommentBox.vue';
import CommentCard from './CommentCard.vue';
import ContextBar from './ContextBar.vue';
import DiffRow from './DiffRow.vue';
import { gaps, type Gap } from '@/diff/gaps';
import { label } from '@/diff/paths';
import { pairs } from '@/diff/pairs';
import { places, slot } from '@/diff/drafts';
import type { Mark } from '@/diff/segments';
import type { Comment, FileDiff, Hunk, NewComment, Row, Side } from '@/api/types';

const props = defineProps<{
  /// The change being read. An unfinished remark is kept under it.
  changeKey: string;
  diff: FileDiff;
  split: boolean;
  comments: Comment[];
  /// Where a comment lands in the patch set being read.
  placement: (
    id: string,
  ) => { line: number | null; endLine: number | null; lost: boolean } | undefined;
  /// Read a run of lines the diff does not carry.
  loadLines: (from: number, to: number) => Promise<Row[]>;
  /// The comments whose place in this patch set is gone.
  lost: Comment[];
  /// True when the diff leaves out what differs only by whitespace.
  /// True when a long line is folded rather than scrolled to.
  wrap: boolean;
}>();
const emit = defineEmits<{
  'update:split': [value: boolean];
  add: [comment: NewComment];
  edit: [id: string, body: string];
  remove: [id: string];
}>();

/// Above this, the browser spends more time building the DOM than the reader
/// spends reading. The rest is one command away in the terminal.
const MAX_ROWS = 2000;

/// The lines a comment box is open on, as `side:line`.
///
/// More than one, because a box that holds an unfinished remark opens again
/// when the reader comes back to the file.
const writing = ref(new Set<string>());

function opening(row: Row | null): boolean {
  return row !== null && writing.value.has(key(row));
}

/// What an open box covers, when it was opened on a range.
///
/// The box keeps its own copy. What the reader picks after that belongs to
/// the next comment, and clicking another line must not turn this one into
/// a comment on a single line.
const covered = reactive(new Map<string, Picked>());

function openBox(at: string, range?: Picked) {
  writing.value = new Set(writing.value).add(at);
  if (range) {
    covered.set(at, { ...range });
  }
}

function closeBox(at: string) {
  const rest = new Set(writing.value);
  rest.delete(at);
  writing.value = rest;
  covered.delete(at);
}

/// What a comment would cover: one line, several, or a part of a line.
///
/// A reader picks it with the mouse, by selecting the text, or with the
/// keyboard, by holding shift. It is drawn until the comment is written or
/// the reader picks something else.
interface Picked {
  side: Side;
  start: number;
  end: number;
  /// Where the range opens on `start` and closes on `end`, in UTF-16 units.
  /// Absent when it covers whole lines.
  startChar?: number;
  endChar?: number;
}
const picked = ref<Picked | null>(null);
/// Where the floating button sits, in the coordinates of the window.
const offer = ref<{ x: number; y: number } | null>(null);

/// Which column the reader started a selection in.
///
/// A row of the side by side view is one row of a table, so a selection in
/// one column takes the other one with it, and the clipboard holds every
/// line twice. The other column is made unselectable while the drag lasts.
const selecting = ref<Side | null>(null);

/// The class that leaves one column of the table selectable.
const only = computed(() => (selecting.value ? `only-${selecting.value}` : ''));

function onDown(event: MouseEvent) {
  const from = event.target as HTMLElement | null;
  const cell = from?.closest<HTMLElement>('td.code-cell[data-column]');
  const column = cell?.dataset.column;

  selecting.value = column === 'old' || column === 'new' ? column : null;
}

/// Which of the two "write about" boxes is open.
const about = ref<'change' | 'file' | null>(null);

/// The context a reader opened, and what is left to open, per gap.
const opened = reactive(new Map<string, Row[]>());
const left = reactive(new Map<string, { from: number; to: number }>());
const loading = ref(false);

/// A new file starts with nothing opened.
watch(
  () => props.diff,
  (fresh, before) => {
    // The context a reader opened belongs to the rows that were there.
    opened.clear();
    left.clear();

    if (fresh.path !== before?.path) {
      // A box left open would come back on whatever line of the new file
      // happens to carry the same number. What the reader typed and did not
      // save comes back instead, on the lines it was typed on.
      about.value = null;
      cursor.value = null;
      writing.value = new Set(places(props.changeKey, fresh.path));

      return;
    }

    // The same file, read again. A box stays open while the line it sits on
    // is still there, so a setting the reader changes does not take it away.
    const here = new Set(walkable.value.map(key));
    writing.value = new Set([...writing.value].filter((at) => here.has(at)));
    if (cursor.value !== null && !walkable.value.some((row) => key(row) === cursor.value)) {
      cursor.value = null;
    }
  },
);

type Block = { kind: 'gap'; gap: Gap } | { kind: 'hunk'; hunk: Hunk };

/// The gaps and the hunks, in the order they are read.
const blocks = computed<Block[]>(() => {
  const found = gaps(shown.value, props.diff.lineCount ?? null);
  const out: Block[] = [];

  shown.value.forEach((hunk, index) => {
    const before = found.find((gap) => gap.key === `before-${index}`);
    if (before) {
      out.push({ kind: 'gap', gap: before });
    }
    out.push({ kind: 'hunk', hunk });
  });

  const after = found.find((gap) => gap.key === 'after');
  if (after) {
    out.push({ kind: 'gap', gap: after });
  }
  return out;
});

function rest(gap: Gap): { from: number; to: number } {
  return left.get(gap.key) ?? { from: gap.from, to: gap.to };
}

function isOpen(gap: Gap): boolean {
  const range = rest(gap);
  return range.to < range.from;
}

function rowsOf(gap: Gap): Row[] {
  return opened.get(gap.key) ?? [];
}

/// The lines opened above what is still closed, and the ones below.
///
/// A short step at the top of a gap opens the lines under the code above, so
/// they belong before the bar. A step at the bottom opens the lines over the
/// code below, and they belong after it.
function rowsBefore(gap: Gap): Row[] {
  if (isOpen(gap)) {
    return rowsOf(gap);
  }
  const still = rest(gap);
  return rowsOf(gap).filter((row) => (row.newLine ?? 0) < still.from);
}

function rowsAfter(gap: Gap): Row[] {
  if (isOpen(gap)) {
    return [];
  }
  const still = rest(gap);
  return rowsOf(gap).filter((row) => (row.newLine ?? 0) > still.to);
}

/// Open a run of the gap, and keep what is still closed.
async function open(gap: Gap, from: number, to: number) {
  if (loading.value) {
    return;
  }
  loading.value = true;
  try {
    const fetched = await props.loadLines(from, to);
    for (const row of fetched) {
      row.oldLine = row.newLine === null ? null : row.newLine + gap.offset;
    }

    const before = rowsOf(gap);
    const all = [...before, ...fetched].sort((a, b) => (a.newLine ?? 0) - (b.newLine ?? 0));
    opened.set(gap.key, all);

    const range = rest(gap);
    left.set(gap.key, {
      from: from <= range.from ? to + 1 : range.from,
      to: to >= range.to ? from - 1 : range.to,
    });
  } finally {
    loading.value = false;
  }
}

const total = computed(() => props.diff.hunks.reduce((n, h) => n + h.rows.length, 0));
const capped = computed(() => total.value > MAX_ROWS);

const shown = computed(() => {
  if (!capped.value) {
    return props.diff.hunks;
  }
  const out = [];
  let left = MAX_ROWS;
  for (const hunk of props.diff.hunks) {
    if (left <= 0) {
      break;
    }
    out.push({ ...hunk, rows: hunk.rows.slice(0, left) });
    left -= hunk.rows.length;
  }
  return out;
});

function sideOf(row: Row): Side {
  return row.kind === 'remove' ? 'old' : 'new';
}

function lineOf(row: Row): number | null {
  return row.kind === 'remove' ? row.oldLine : row.newLine;
}

function key(row: Row): string {
  return `${sideOf(row)}:${lineOf(row)}`;
}

function at(row: Row | null) {
  if (!row) {
    return [];
  }
  const line = lineOf(row);
  if (line === null) {
    return [];
  }
  return props.comments.filter((comment) => {
    const found = span(comment);

    return found?.side === sideOf(row) && found.end === line;
  });
}

/// The lines a comment covers in the patch set being read.
///
/// The card sits under the last of them, the way it reads: the remark comes
/// after what it is about.
function span(comment: Comment): { side: Side; start: number; end: number } | null {
  const anchor = comment.anchor;
  if (anchor?.file !== props.diff.path || anchor.startLine === null) {
    return null;
  }
  const placed = props.placement(comment.id);
  if (placed?.lost) {
    return null;
  }
  const start = placed?.line ?? anchor.startLine;
  const end = placed?.endLine ?? anchor.endLine ?? start;

  return { side: anchor.side, start, end: Math.max(start, end) };
}

/// Every range drawn on the code: the one the keyboard is picking, and the
/// ones the comments of this file cover.
///
/// A range picked with the mouse is not drawn. The browser draws it already,
/// as the selection, and repainting the rows under a selection replaces the
/// text nodes it is anchored on: an end that loses its node falls back to
/// the start of the line, and the selection spreads to the whole lines.
const drawn = computed<Picked[]>(() => {
  const out: Picked[] = picked.value && choosing.value ? [picked.value] : [];
  out.push(...covered.values());

  for (const comment of props.comments) {
    const found = span(comment);
    if (found && (found.end > found.start || comment.anchor?.startChar !== null)) {
      out.push({
        ...found,
        startChar: comment.anchor?.startChar ?? undefined,
        endChar: comment.anchor?.endChar ?? undefined,
      });
    }
  }
  return out;
});

/// The part of one row that a range covers, if any.
///
/// `column` is the side the cell stands in. A context line is drawn in both
/// columns of the side by side view, and only the column that owns the side
/// of the range carries the mark.
function markOf(row: Row | null, column: Side): Mark | undefined {
  if (!row || sideOf(row) !== column) {
    return undefined;
  }
  const line = lineOf(row);
  const range = drawn.value.find(
    (r) => r.side === column && line !== null && r.start <= line && line <= r.end,
  );
  if (!range || line === null) {
    return undefined;
  }

  const length = row.text.length;
  const from = line === range.start ? (range.startChar ?? 0) : 0;
  const to = line === range.end ? (range.endChar ?? length) : length;

  return { start: from, end: Math.max(to, from) };
}

/// A row that carries a comment, or an open box, gets its own row underneath
/// so the code above it stays aligned with the other side.
function talkative(row: Row | null): boolean {
  return row !== null && (at(row).length > 0 || opening(row));
}

/// The line the keyboard is on, as `side:line`.
const cursor = ref<string | null>(null);

/// Every row the reader can walk, in the order they are drawn.
const walkable = computed<Row[]>(() => {
  const out: Row[] = [];
  for (const block of blocks.value) {
    if (block.kind === 'gap') {
      out.push(...rowsOf(block.gap));
    } else if (block.hunk) {
      out.push(...block.hunk.rows);
    }
  }
  return out;
});

function place(row: Row | undefined) {
  if (!row) {
    return;
  }
  cursor.value = key(row);
  void nextTick(() => {
    document.querySelector('.row-cursor')?.scrollIntoView({ block: 'center' });
  });
}

/// One line down or up.
///
/// While the reader is choosing a range, the movement grows it from the line
/// it started on rather than leaving that line behind.
function moveLine(by: number) {
  const list = walkable.value;
  if (list.length === 0) {
    return;
  }
  const at = list.findIndex((row) => key(row) === cursor.value);

  if (!choosing.value) {
    const next = at === -1 ? 0 : Math.min(Math.max(at + by, 0), list.length - 1);
    clearPicked();
    place(list[next]);
    return;
  }

  // A range lives on one side. The other side of a changed line belongs to
  // another version of the file, so the walk steps over it.
  const side = picked.value?.side;
  let next = at + by;
  while (next >= 0 && next < list.length && sideOf(list[next]) !== side) {
    next += by;
  }
  const row = list[next];
  const line = row ? lineOf(row) : null;
  const first = origin.value;
  if (!row || line === null || first === null) {
    return;
  }
  picked.value = {
    side: sideOf(row),
    start: Math.min(first, line),
    end: Math.max(first, line),
  };
  place(row);
}

/// True while the keyboard is choosing a range, after `v`.
const choosing = ref(false);
/// The line a keyboard range started on.
const origin = ref<number | null>(null);

/// Start a range on the line the keyboard is on, or drop the one being
/// chosen. `j` and `k` then grow it, and `c` writes on it.
function startRange() {
  if (choosing.value) {
    clearPicked();
    return;
  }
  if (cursor.value === null) {
    moveLine(1);
  }
  const row = walkable.value.find((r) => key(r) === cursor.value);
  const line = row ? lineOf(row) : null;
  if (!row || line === null) {
    return;
  }
  choosing.value = true;
  origin.value = line;
  picked.value = { side: sideOf(row), start: line, end: line };
}

/// The first line of the next hunk, or of the one before.
function moveHunk(by: number) {
  const heads = blocks.value
    .filter((block) => block.kind === 'hunk')
    .map((block) => block.hunk?.rows[0])
    .filter((row): row is Row => row !== undefined);
  if (heads.length === 0) {
    return;
  }

  const list = walkable.value;
  const here = list.findIndex((row) => key(row) === cursor.value);
  const found =
    by > 0
      ? heads.find((head) => list.indexOf(head) > here)
      : [...heads].reverse().find((head) => list.indexOf(head) < here);

  place(found ?? heads[by > 0 ? heads.length - 1 : 0]);
}

/// Write on what is picked, or on the line the keyboard is on.
function commentHere() {
  if (picked.value) {
    writeOnPicked();
    return;
  }

  if (cursor.value === null) {
    moveLine(1);
  }
  if (cursor.value !== null) {
    openBox(cursor.value);
  }
}

/// Open the box under the last line of the range.
function writeOnPicked() {
  if (!picked.value) {
    return;
  }
  offer.value = null;
  openBox(`${picked.value.side}:${picked.value.end}`, picked.value);
  window.getSelection()?.removeAllRanges();
}

/// Put the keyboard on one line and bring it into view. This is how the
/// list of comments arrives at the place a remark speaks of.
function revealLine(side: Side, line: number) {
  const row = walkable.value.find((r) => sideOf(r) === side && lineOf(r) === line);
  if (row) {
    clearPicked();
    place(row);
  }
}

defineExpose({ moveLine, moveHunk, commentHere, startRange, clearPicked, revealLine });

/// What the reader selected with the mouse, as lines and characters.
///
/// The cells carry the line they hold, so a selection in the page is read
/// back from the DOM rather than guessed from coordinates.
function fromSelection(): Picked | null {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
    return null;
  }
  const range = selection.getRangeAt(0);
  const head = cellOf(range.startContainer);
  const tail = cellOf(range.endContainer);
  if (!head || !tail || head.side !== tail.side) {
    return null;
  }

  const startChar = charOffset(head.cell, range.startContainer, range.startOffset);
  const endChar = charOffset(tail.cell, range.endContainer, range.endOffset);
  const forward = head.line < tail.line || (head.line === tail.line && startChar <= endChar);
  const first = forward ? head : tail;
  const last = forward ? tail : head;

  return {
    side: first.side,
    start: first.line,
    end: last.line,
    startChar: forward ? startChar : endChar,
    endChar: forward ? endChar : startChar,
  };
}

/// The code cell a node sits in, and the line it holds.
function cellOf(node: Node): { cell: HTMLElement; side: Side; line: number } | null {
  const start = node instanceof HTMLElement ? node : node.parentElement;
  const cell = start?.closest<HTMLElement>('td.code-cell[data-line]');
  const line = Number(cell?.dataset.line);
  const side = cell?.dataset.side;
  if (!cell || !Number.isFinite(line) || (side !== 'old' && side !== 'new')) {
    return null;
  }
  return { cell, side, line };
}

/// How many UTF-16 units of the cell come before a point in it.
function charOffset(cell: HTMLElement, node: Node, offset: number): number {
  const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
  let count = 0;

  while (walker.nextNode()) {
    const text = walker.currentNode;
    if (text === node) {
      return count + offset;
    }
    count += text.textContent?.length ?? 0;
  }
  // The point is not in a text node of this cell: the whole cell, then.
  return node === cell && offset === 0 ? 0 : count;
}

/// A selection in the code offers to become a comment.
function onSelect(event: MouseEvent) {
  // A click inside a comment row is not a click on the code. Saving a box
  // is one of those, and it must change nothing under it.
  const from = event.target as HTMLElement | null;
  if (from?.closest('tr.talk, .offer')) {
    return;
  }

  const found = fromSelection();
  if (found) {
    picked.value = found;
    origin.value = found.start;
    // The mouse took over from `v`, and its pick must not be drawn either.
    choosing.value = false;
    // At the right edge of the pane, on the line the reader stopped on.
    // Under the pointer it would take the right click that was meant for
    // the selection, and the menu of a button carries no Copy.
    const pane = (event.currentTarget as HTMLElement).getBoundingClientRect();
    offer.value = { x: pane.right - 10, y: event.clientY };
    return;
  }

  // A plain click puts the keyboard on the line, so `c` writes there. No
  // scrolling: the reader is looking at the line already.
  clearPicked();
  const cell = cellOf(event.target as Node);
  if (cell) {
    cursor.value = `${cell.side}:${cell.line}`;
  }
}

function clearPicked() {
  picked.value = null;
  offer.value = null;
  origin.value = null;
  choosing.value = false;
}

/// The left column of a pair only speaks for a removed line.
///
/// A context line is the same row on both sides, so without this both cells
/// would draw the comment, and the box would appear twice.
function onCursor(row: Row | null): boolean {
  return row !== null && cursor.value === key(row);
}

function ownLeft(pair: { left: Row | null }): Row | null {
  return pair.left?.kind === 'remove' ? pair.left : null;
}

function write(row: Row, body: string) {
  const line = lineOf(row);
  if (line === null) {
    return;
  }
  const range = covered.get(key(row)) ?? null;

  emit('add', {
    scope: range ? 'range' : 'line',
    file: props.diff.path,
    side: sideOf(row),
    startLine: range ? range.start : line,
    endLine: range ? range.end : line,
    startChar: range?.startChar,
    endChar: range?.endChar,
    body,
  });
  closeBox(key(row));
  clearPicked();
}

/// A comment about the whole change, or about this file. Neither belongs to
/// a line, so both sit above the diff rather than in a pane of their own.
const loose = computed(() =>
  props.comments.filter(
    (comment) =>
      comment.scope === 'change' ||
      (comment.scope === 'file' && comment.anchor?.file === props.diff.path),
  ),
);

function writeAbout(scope: 'change' | 'file', body: string) {
  emit('add', {
    scope,
    file: scope === 'file' ? props.diff.path : undefined,
    side: scope === 'file' ? 'new' : undefined,
    body,
  });
  about.value = null;
}

/// Where an unfinished remark on this row is kept.
function draftAt(row: Row): string {
  return slot(props.changeKey, props.diff.path, key(row));
}

/// What the box says it is about.
function boxLabel(row: Row): string {
  const range = covered.get(key(row));
  if (!range) {
    return 'A remark about this line';
  }
  const part = cuts(range) ? 'a part of ' : '';
  if (range.end > range.start) {
    return `A remark about ${part}lines ${range.start} to ${range.end}`;
  }
  return `A remark about ${part}this line`;
}

/// Whether the range opens or closes inside a line rather than on its ends.
///
/// A mouse selection of whole lines still carries characters, 0 to the
/// length of the last line, and that is not a part of anything.
function cuts(range: Picked): boolean {
  if (range.startChar === undefined && range.endChar === undefined) {
    return false;
  }
  const last = walkable.value.find((r) => sideOf(r) === range.side && lineOf(r) === range.end);
  const length = last?.text.length ?? 0;
  return (range.startChar ?? 0) > 0 || (range.endChar ?? length) < length;
}

/// What the floating button offers to write on.
const offerLabel = computed(() => {
  const range = picked.value;
  if (!range || range.end === range.start) {
    return 'Comment on this';
  }
  const count = range.end - range.start + 1;
  return cuts(range) ? `Comment on part of ${count} lines` : `Comment on ${count} lines`;
});

function toggle(row: Row | null) {
  if (row) {
    if (opening(row)) {
      closeBox(key(row));
    } else {
      openBox(key(row));
    }
  }
}
</script>

<template>
  <div class="diff-pane">
    <header class="file-bar">
      <div class="file-name">
        <h2>
          <span v-if="diff.oldPath" class="from">{{ diff.oldPath }} →</span>
          {{ label(diff.path) }}
        </h2>
        <p class="file-facts">
          {{ diff.status }}
          <span v-if="diff.language"> · {{ diff.language }}</span>
          · <span class="added">+{{ diff.added }}</span>
          <span class="removed">−{{ diff.removed }}</span>
        </p>
      </div>

      <span class="bar-actions">
        <button type="button" class="chip" @click="about = about === 'file' ? null : 'file'">
          Comment on the file
        </button>
        <button type="button" class="chip" @click="about = about === 'change' ? null : 'change'">
          Comment on the change
        </button>
        <button
          type="button"
          class="chip"
          :aria-pressed="split"
          @click="emit('update:split', !split)"
        >
          {{ split ? 'Unified' : 'Side by side' }}
        </button>
      </span>
    </header>

    <section v-if="about || loose.length || lost.length" class="above-diff">
      <CommentBox
        v-if="about"
        :label="about === 'change' ? 'A remark about the change' : 'A remark about the file'"
        @save="(body) => writeAbout(about!, body)"
        @cancel="about = null"
      />

      <CommentCard
        v-for="comment in loose"
        :key="comment.id"
        :comment="comment"
        @edit="(id, body) => emit('edit', id, body)"
        @remove="(id) => emit('remove', id)"
      />

      <div v-if="lost.length" class="lost">
        <p class="lost-title">Could not be placed · {{ lost.length }}</p>
        <p class="quiet">
          The line these were written on is not in this patch set. They are kept here rather than
          moved to a line nobody chose.
        </p>
        <p v-for="comment in lost" :key="comment.id" class="lost-item">
          <code>{{ comment.anchor?.file }}:{{ comment.anchor?.startLine }}</code>
          <span class="quiet"> patch set {{ comment.patchSet }} · </span>{{ comment.body }}
        </p>
      </div>
    </section>

    <p v-if="diff.binary" class="note">A binary file has no diff to read.</p>

    <p v-else-if="diff.hunks.length === 0" class="note">Nothing changed inside this file.</p>

    <div v-else class="diff-scroll" @mousedown="onDown" @mouseup="onSelect">
      <table v-if="split" class="code" :class="[wrap ? '' : 'nowrap', only]">
        <colgroup>
          <col class="gut" />
          <col />
          <col class="gut" />
          <col />
        </colgroup>
        <tbody v-for="(block, b) in blocks" :key="b">
          <template v-if="block.kind === 'gap'">
            <template v-for="row in rowsBefore(block.gap)" :key="`ga${row.newLine}`">
              <tr>
                <DiffRow :row="row" side="old" :mark="markOf(row, 'old')" />
                <DiffRow
                  :row="row"
                  side="new"
                  :mark="markOf(row, 'new')"
                  commentable
                  @comment="toggle(row)"
                />
              </tr>
              <tr v-if="talkative(row)" class="talk">
                <td colspan="2"></td>
                <td colspan="2">
                  <CommentCard
                    v-for="comment in at(row)"
                    :key="comment.id"
                    :comment="comment"
                    @edit="(id, body) => emit('edit', id, body)"
                    @remove="(id) => emit('remove', id)"
                  />
                  <CommentBox
                    v-if="opening(row)"
                    :draft="draftAt(row!)"
                    :label="boxLabel(row)"
                    @save="(body) => write(row, body)"
                    @cancel="closeBox(key(row!))"
                  />
                </td>
              </tr>
            </template>
            <ContextBar
              v-if="!isOpen(block.gap)"
              :from="rest(block.gap).from"
              :to="rest(block.gap).to"
              :columns="4"
              :busy="loading"
              @open="(from, to) => open(block.gap, from, to)"
            />
            <template v-for="row in rowsAfter(block.gap)" :key="`gb${row.newLine}`">
              <tr :class="onCursor(row) ? 'row-cursor' : ''">
                <DiffRow :row="row" side="old" :mark="markOf(row, 'old')" />
                <DiffRow
                  :row="row"
                  side="new"
                  :mark="markOf(row, 'new')"
                  commentable
                  @comment="toggle(row)"
                />
              </tr>
              <tr v-if="talkative(row)" class="talk">
                <td colspan="2"></td>
                <td colspan="2">
                  <CommentCard
                    v-for="comment in at(row)"
                    :key="comment.id"
                    :comment="comment"
                    @edit="(id, body) => emit('edit', id, body)"
                    @remove="(id) => emit('remove', id)"
                  />
                  <CommentBox
                    v-if="opening(row)"
                    :draft="draftAt(row!)"
                    :label="boxLabel(row)"
                    @save="(body) => write(row, body)"
                    @cancel="closeBox(key(row!))"
                  />
                </td>
              </tr>
            </template>
          </template>

          <template v-for="(pair, p) in pairs(block.hunk?.rows ?? [])" v-else :key="p">
            <tr :class="onCursor(pair.right) || onCursor(ownLeft(pair)) ? 'row-cursor' : ''">
              <DiffRow
                :row="pair.left"
                side="old"
                :mark="markOf(pair.left, 'old')"
                @comment="toggle(pair.left)"
              />
              <DiffRow
                :row="pair.right"
                side="new"
                :mark="markOf(pair.right, 'new')"
                commentable
                @comment="toggle(pair.right)"
              />
            </tr>
            <!-- A comment sits under the side it was written on, not across
               both, so the two columns keep meaning what they mean. -->
            <tr v-if="talkative(ownLeft(pair)) || talkative(pair.right)" class="talk">
              <td colspan="2">
                <CommentCard
                  v-for="comment in at(ownLeft(pair))"
                  :key="comment.id"
                  :comment="comment"
                  @edit="(id, body) => emit('edit', id, body)"
                  @remove="(id) => emit('remove', id)"
                />
                <CommentBox
                  v-if="opening(ownLeft(pair))"
                  :draft="draftAt(ownLeft(pair)!)"
                  :label="boxLabel(ownLeft(pair)!)"
                  @save="(body) => write(ownLeft(pair)!, body)"
                  @cancel="closeBox(key(ownLeft(pair)!))"
                />
              </td>
              <td colspan="2">
                <CommentCard
                  v-for="comment in at(pair.right)"
                  :key="comment.id"
                  :comment="comment"
                  @edit="(id, body) => emit('edit', id, body)"
                  @remove="(id) => emit('remove', id)"
                />
                <CommentBox
                  v-if="opening(pair.right)"
                  :draft="draftAt(pair.right!)"
                  :label="boxLabel(pair.right!)"
                  @save="(body) => write(pair.right!, body)"
                  @cancel="closeBox(key(pair.right!))"
                />
              </td>
            </tr>
          </template>
        </tbody>
      </table>

      <table v-else class="code" :class="[wrap ? '' : 'nowrap', only]">
        <colgroup>
          <col class="gut" />
          <col class="gut" />
          <col />
        </colgroup>
        <tbody v-for="(block, b) in blocks" :key="b">
          <template v-if="block.kind === 'gap'">
            <template v-for="row in rowsBefore(block.gap)" :key="`ga${row.newLine}`">
              <tr>
                <td class="gutter">{{ row.oldLine ?? '' }}</td>
                <DiffRow
                  :row="row"
                  side="new"
                  :mark="markOf(row, sideOf(row))"
                  commentable
                  @comment="toggle(row)"
                />
              </tr>
              <tr v-if="talkative(row)" class="talk">
                <td colspan="3">
                  <CommentCard
                    v-for="comment in at(row)"
                    :key="comment.id"
                    :comment="comment"
                    @edit="(id, body) => emit('edit', id, body)"
                    @remove="(id) => emit('remove', id)"
                  />
                  <CommentBox
                    v-if="opening(row)"
                    :draft="draftAt(row!)"
                    :label="boxLabel(row)"
                    @save="(body) => write(row, body)"
                    @cancel="closeBox(key(row!))"
                  />
                </td>
              </tr>
            </template>
            <ContextBar
              v-if="!isOpen(block.gap)"
              :from="rest(block.gap).from"
              :to="rest(block.gap).to"
              :columns="3"
              :busy="loading"
              @open="(from, to) => open(block.gap, from, to)"
            />
            <template v-for="row in rowsAfter(block.gap)" :key="`gb${row.newLine}`">
              <tr :class="onCursor(row) ? 'row-cursor' : ''">
                <td class="gutter" :class="`gutter-${row.kind}`">{{ row.oldLine ?? '' }}</td>
                <DiffRow
                  :row="row"
                  side="new"
                  :mark="markOf(row, sideOf(row))"
                  commentable
                  @comment="toggle(row)"
                />
              </tr>
              <tr v-if="talkative(row)" class="talk">
                <td colspan="3">
                  <CommentCard
                    v-for="comment in at(row)"
                    :key="comment.id"
                    :comment="comment"
                    @edit="(id, body) => emit('edit', id, body)"
                    @remove="(id) => emit('remove', id)"
                  />
                  <CommentBox
                    v-if="opening(row)"
                    :draft="draftAt(row!)"
                    :label="boxLabel(row)"
                    @save="(body) => write(row, body)"
                    @cancel="closeBox(key(row!))"
                  />
                </td>
              </tr>
            </template>
          </template>

          <template v-for="(row, r) in block.hunk?.rows ?? []" v-else :key="r">
            <tr :class="onCursor(row) ? 'row-cursor' : ''">
              <td class="gutter" :class="`gutter-${row.kind}`">{{ row.oldLine ?? '' }}</td>
              <DiffRow
                :row="row"
                side="new"
                :mark="markOf(row, sideOf(row))"
                commentable
                @comment="toggle(row)"
              />
            </tr>
            <tr v-if="talkative(row)" class="talk">
              <td colspan="3">
                <CommentCard
                  v-for="comment in at(row)"
                  :key="comment.id"
                  :comment="comment"
                  @edit="(id, body) => emit('edit', id, body)"
                  @remove="(id) => emit('remove', id)"
                />
                <CommentBox
                  v-if="opening(row)"
                  :draft="draftAt(row!)"
                  :label="boxLabel(row)"
                  @save="(body) => write(row, body)"
                  @cancel="closeBox(key(row!))"
                />
              </td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>

    <button
      v-if="offer && picked"
      type="button"
      class="offer"
      :style="{ left: `${offer.x}px`, top: `${offer.y + 12}px` }"
      @click="writeOnPicked"
    >
      {{ offerLabel }}
    </button>

    <p v-if="capped" role="status" class="note warn">
      This file is very large. {{ total - MAX_ROWS }} rows are not shown, because building them
      costs more than reading them.
    </p>
  </div>
</template>
