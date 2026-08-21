<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import CommentBox from './CommentBox.vue';
import CommentCard from './CommentCard.vue';
import ContextBar from './ContextBar.vue';
import DiffRow from './DiffRow.vue';
import { gaps, type Gap } from '@/diff/gaps';
import { pairs } from '@/diff/pairs';
import type { Comment, FileDiff, Hunk, NewComment, Row, Side } from '@/api/types';

const props = defineProps<{
  diff: FileDiff;
  split: boolean;
  comments: Comment[];
  /// Where a comment lands in the patch set being read.
  placement: (id: string) => { line: number | null; lost: boolean } | undefined;
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

/// Which line a comment box is open on, as `side:line`.
const writing = ref<string | null>(null);

/// Which of the two "write about" boxes is open.
const about = ref<'change' | 'file' | null>(null);

/// The context a reader opened, and what is left to open, per gap.
const opened = reactive(new Map<string, Row[]>());
const left = reactive(new Map<string, { from: number; to: number }>());
const loading = ref(false);

/// A new file starts with nothing opened.
watch(
  () => [props.diff.path, props.diff.hunks],
  () => {
    opened.clear();
    left.clear();
    // A box left open would come back on whatever line of the new file
    // happens to carry the same number.
    writing.value = null;
    about.value = null;
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
    if (comment.anchor?.file !== props.diff.path || comment.anchor.side !== sideOf(row)) {
      return false;
    }
    const placed = props.placement(comment.id);
    if (placed?.lost) {
      return false;
    }
    return (placed?.line ?? comment.anchor.startLine) === line;
  });
}

/// A row that carries a comment, or an open box, gets its own row underneath
/// so the code above it stays aligned with the other side.
function talkative(row: Row | null): boolean {
  return row !== null && (at(row).length > 0 || writing.value === key(row));
}

/// The left column of a pair only speaks for a removed line.
///
/// A context line is the same row on both sides, so without this both cells
/// would draw the comment, and the box would appear twice.
function ownLeft(pair: { left: Row | null }): Row | null {
  return pair.left?.kind === 'remove' ? pair.left : null;
}

function write(row: Row, body: string) {
  const line = lineOf(row);
  if (line === null) {
    return;
  }
  emit('add', {
    scope: 'line',
    file: props.diff.path,
    side: sideOf(row),
    startLine: line,
    body,
  });
  writing.value = null;
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

function toggle(row: Row | null) {
  if (row) {
    writing.value = writing.value === key(row) ? null : key(row);
  }
}
</script>

<template>
  <div class="diff-pane">
    <header class="file-bar">
      <div class="file-name">
        <h2>
          <span v-if="diff.oldPath" class="from">{{ diff.oldPath }} →</span>
          {{ diff.path }}
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
          On the change
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

    <table v-else-if="split" class="code" :class="wrap ? '' : 'nowrap'">
      <colgroup>
        <col class="gut" />
        <col />
        <col class="gut" />
        <col />
      </colgroup>
      <tbody v-for="(block, b) in blocks" :key="b">
        <template v-if="block.kind === 'gap'">
          <ContextBar
            v-if="!isOpen(block.gap) && block.gap.key !== 'after'"
            :from="rest(block.gap).from"
            :to="rest(block.gap).to"
            :columns="4"
            :busy="loading"
            @open="(from, to) => open(block.gap, from, to)"
          />
          <template v-for="row in rowsOf(block.gap)" :key="`g${row.newLine}`">
            <tr>
              <DiffRow :row="row" side="old" />
              <DiffRow :row="row" side="new" commentable @comment="toggle(row)" />
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
                  v-if="writing === key(row)"
                  label="A remark about this line"
                  @save="(body) => write(row, body)"
                  @cancel="writing = null"
                />
              </td>
            </tr>
          </template>
          <ContextBar
            v-if="!isOpen(block.gap) && block.gap.key === 'after'"
            :from="rest(block.gap).from"
            :to="rest(block.gap).to"
            :columns="4"
            :busy="loading"
            @open="(from, to) => open(block.gap, from, to)"
          />
        </template>

        <template v-for="(pair, p) in pairs(block.hunk?.rows ?? [])" v-else :key="p">
          <tr>
            <DiffRow :row="pair.left" side="old" @comment="toggle(pair.left)" />
            <DiffRow :row="pair.right" side="new" commentable @comment="toggle(pair.right)" />
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
                v-if="ownLeft(pair) && writing === key(ownLeft(pair)!)"
                label="A remark about this line"
                @save="(body) => write(ownLeft(pair)!, body)"
                @cancel="writing = null"
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
                v-if="pair.right && writing === key(pair.right)"
                label="A remark about this line"
                @save="(body) => write(pair.right!, body)"
                @cancel="writing = null"
              />
            </td>
          </tr>
        </template>
      </tbody>
    </table>

    <table v-else class="code" :class="wrap ? '' : 'nowrap'">
      <colgroup>
        <col class="gut" />
        <col class="gut" />
        <col />
      </colgroup>
      <tbody v-for="(block, b) in blocks" :key="b">
        <template v-if="block.kind === 'gap'">
          <ContextBar
            v-if="!isOpen(block.gap) && block.gap.key !== 'after'"
            :from="rest(block.gap).from"
            :to="rest(block.gap).to"
            :columns="3"
            :busy="loading"
            @open="(from, to) => open(block.gap, from, to)"
          />
          <template v-for="row in rowsOf(block.gap)" :key="`g${row.newLine}`">
            <tr>
              <td class="gutter">{{ row.oldLine ?? '' }}</td>
              <DiffRow :row="row" side="new" commentable @comment="toggle(row)" />
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
                  v-if="writing === key(row)"
                  label="A remark about this line"
                  @save="(body) => write(row, body)"
                  @cancel="writing = null"
                />
              </td>
            </tr>
          </template>
          <ContextBar
            v-if="!isOpen(block.gap) && block.gap.key === 'after'"
            :from="rest(block.gap).from"
            :to="rest(block.gap).to"
            :columns="3"
            :busy="loading"
            @open="(from, to) => open(block.gap, from, to)"
          />
        </template>

        <template v-for="(row, r) in block.hunk?.rows ?? []" v-else :key="r">
          <tr>
            <td class="gutter" :class="`gutter-${row.kind}`">{{ row.oldLine ?? '' }}</td>
            <DiffRow :row="row" side="new" commentable @comment="toggle(row)" />
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
                v-if="writing === key(row)"
                label="A remark about this line"
                @save="(body) => write(row, body)"
                @cancel="writing = null"
              />
            </td>
          </tr>
        </template>
      </tbody>
    </table>

    <p v-if="capped" role="status" class="note warn">
      This file is very large. {{ total - MAX_ROWS }} rows are not shown, because building them
      costs more than reading them.
    </p>
  </div>
</template>
