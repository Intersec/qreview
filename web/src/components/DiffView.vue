<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import CommentBox from './CommentBox.vue';
import CommentThread from './CommentThread.vue';
import ContextBar from './ContextBar.vue';
import DiffRow from './DiffRow.vue';
import { gaps, type Gap } from '@/diff/gaps';
import { pairs } from '@/diff/pairs';
import type { Comment, FileDiff, Hunk, NewComment, Row, Side } from '@/api/types';

const props = defineProps<{
  diff: FileDiff;
  split: boolean;
  threads: { first: Comment; replies: Comment[] }[];
  /// Where a comment lands in the patch set being read.
  placement: (id: string) => { line: number | null; lost: boolean } | undefined;
  /// Read a run of lines the diff does not carry.
  loadLines: (from: number, to: number) => Promise<Row[]>;
}>();
const emit = defineEmits<{
  'update:split': [value: boolean];
  add: [comment: NewComment];
  edit: [id: string, body: string];
  remove: [id: string];
  resolve: [id: string, resolved: boolean];
}>();

/// Above this, the browser spends more time building the DOM than the reader
/// spends reading. The rest is one command away in the terminal.
const MAX_ROWS = 2000;

/// Which line a comment box is open on, as `side:line`.
const writing = ref<string | null>(null);

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
  return props.threads.filter((t) => {
    if (t.first.anchor?.file !== props.diff.path || t.first.anchor.side !== sideOf(row)) {
      return false;
    }
    const placed = props.placement(t.first.id);
    if (placed?.lost) {
      return false;
    }
    return (placed?.line ?? t.first.anchor.startLine) === line;
  });
}

/// A row that carries a comment, or an open box, gets its own row underneath
/// so the code above it stays aligned with the other side.
function talkative(row: Row | null): boolean {
  return row !== null && (at(row).length > 0 || writing.value === key(row));
}

function write(row: Row, body: string, draft: boolean) {
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
    draft,
  });
  writing.value = null;
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

      <button
        type="button"
        class="chip"
        :aria-pressed="split"
        @click="emit('update:split', !split)"
      >
        {{ split ? 'Unified' : 'Side by side' }}
      </button>
    </header>

    <p v-if="diff.binary" class="note">A binary file has no diff to read.</p>

    <p v-else-if="diff.hunks.length === 0" class="note">Nothing changed inside this file.</p>

    <table v-else-if="split" class="code">
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
          <tr v-for="row in rowsOf(block.gap)" :key="`g${row.newLine}`">
            <DiffRow :row="row" side="old" />
            <DiffRow :row="row" side="new" commentable @comment="toggle(row)" />
          </tr>
          <ContextBar
            v-if="!isOpen(block.gap) && block.gap.key === 'after'"
            :from="rest(block.gap).from"
            :to="rest(block.gap).to"
            :columns="4"
            :busy="loading"
            hunk-above
            @open="(from, to) => open(block.gap, from, to)"
          />
        </template>

        <template v-for="(pair, p) in pairs(block.hunk?.rows ?? [])" v-else :key="p">
          <tr>
            <DiffRow :row="pair.left" side="old" @comment="toggle(pair.left)" />
            <DiffRow :row="pair.right" side="new" commentable @comment="toggle(pair.right)" />
          </tr>
          <tr v-if="talkative(pair.left) || talkative(pair.right)" class="talk">
            <td colspan="4">
              <template v-for="row in [pair.left, pair.right]">
                <CommentThread
                  v-for="thread in at(row)"
                  :key="thread.first.id"
                  :first="thread.first"
                  :replies="thread.replies"
                  @reply="
                    (id, body, draft) => emit('add', { parentId: id, scope: 'change', body, draft })
                  "
                  @edit="(id, body) => emit('edit', id, body)"
                  @remove="(id) => emit('remove', id)"
                  @resolve="(id, value) => emit('resolve', id, value)"
                />
              </template>
              <CommentBox
                v-if="pair.right && writing === key(pair.right)"
                label="A remark about this line"
                @save="(body, draft) => write(pair.right!, body, draft)"
                @cancel="writing = null"
              />
              <CommentBox
                v-else-if="pair.left && writing === key(pair.left)"
                label="A remark about this line"
                @save="(body, draft) => write(pair.left!, body, draft)"
                @cancel="writing = null"
              />
            </td>
          </tr>
        </template>
      </tbody>
    </table>

    <table v-else class="code">
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
          <tr v-for="row in rowsOf(block.gap)" :key="`g${row.newLine}`">
            <td class="gutter">{{ row.oldLine ?? '' }}</td>
            <DiffRow :row="row" side="new" commentable @comment="toggle(row)" />
          </tr>
          <ContextBar
            v-if="!isOpen(block.gap) && block.gap.key === 'after'"
            :from="rest(block.gap).from"
            :to="rest(block.gap).to"
            :columns="3"
            :busy="loading"
            hunk-above
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
              <CommentThread
                v-for="thread in at(row)"
                :key="thread.first.id"
                :first="thread.first"
                :replies="thread.replies"
                @reply="
                  (id, body, draft) => emit('add', { parentId: id, scope: 'change', body, draft })
                "
                @edit="(id, body) => emit('edit', id, body)"
                @remove="(id) => emit('remove', id)"
                @resolve="(id, value) => emit('resolve', id, value)"
              />
              <CommentBox
                v-if="writing === key(row)"
                label="A remark about this line"
                @save="(body, draft) => write(row, body, draft)"
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
