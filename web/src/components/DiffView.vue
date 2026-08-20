<script setup lang="ts">
import { computed, ref } from 'vue';
import CommentBox from './CommentBox.vue';
import CommentThread from './CommentThread.vue';
import DiffRow from './DiffRow.vue';
import { pairs } from '@/diff/pairs';
import type { Comment, FileDiff, NewComment, Row, Side } from '@/api/types';

const props = defineProps<{
  diff: FileDiff;
  split: boolean;
  threads: { first: Comment; replies: Comment[] }[];
  /// Where a comment lands in the patch set being read.
  placement: (id: string) => { line: number | null; lost: boolean } | undefined;
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
      <tbody v-for="(hunk, h) in shown" :key="h">
        <tr class="hunk">
          <td colspan="4">
            @@ −{{ hunk.oldStart }},{{ hunk.oldLines }} +{{ hunk.newStart }},{{ hunk.newLines }} @@
            <span v-if="hunk.header" class="hunk-header">{{ hunk.header }}</span>
          </td>
        </tr>
        <template v-for="(pair, p) in pairs(hunk.rows)" :key="`${h}-${p}`">
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
      <tbody v-for="(hunk, h) in shown" :key="h">
        <tr class="hunk">
          <td colspan="3">
            @@ −{{ hunk.oldStart }},{{ hunk.oldLines }} +{{ hunk.newStart }},{{ hunk.newLines }} @@
            <span v-if="hunk.header" class="hunk-header">{{ hunk.header }}</span>
          </td>
        </tr>
        <template v-for="(row, r) in hunk.rows" :key="`${h}-${r}`">
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
