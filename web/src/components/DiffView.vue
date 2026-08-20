<script setup lang="ts">
import { computed } from 'vue';
import DiffRow from './DiffRow.vue';
import { pairs } from '@/diff/pairs';
import { segments } from '@/diff/segments';
import type { FileDiff, Row } from '@/api/types';

const props = defineProps<{ diff: FileDiff; split: boolean }>();
const emit = defineEmits<{ 'update:split': [value: boolean] }>();

/// Above this, the browser spends more time building the DOM than the reader
/// spends reading. The rest is one line away, in the terminal.
const MAX_ROWS = 2000;

const SIGN: Record<Row['kind'], string> = { context: ' ', add: '+', remove: '−' };

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

function rowClass(kind: Row['kind']): string {
  if (kind === 'add') {
    return 'bg-emerald-50 dark:bg-emerald-950/40';
  }
  if (kind === 'remove') {
    return 'bg-rose-50 dark:bg-rose-950/40';
  }
  return '';
}

function pairClass(row: Row | null): string {
  return row ? rowClass(row.kind) : '';
}
</script>

<template>
  <div class="h-full overflow-auto">
    <header
      class="sticky top-0 z-10 flex items-baseline gap-3 border-b border-slate-200 bg-white px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
    >
      <div class="min-w-0">
        <h2 class="truncate font-mono text-sm">
          <span v-if="diff.oldPath" class="text-slate-500 dark:text-slate-400">
            {{ diff.oldPath }} →
          </span>
          {{ diff.path }}
        </h2>
        <p class="text-xs text-slate-500 dark:text-slate-400">
          {{ diff.status }}
          <span v-if="diff.language"> · {{ diff.language }}</span>
          · +{{ diff.added }} −{{ diff.removed }}
        </p>
      </div>

      <button
        type="button"
        class="ml-auto shrink-0 rounded border border-slate-300 px-2 py-0.5 text-xs hover:bg-slate-100 dark:border-slate-600 dark:hover:bg-slate-800"
        :aria-pressed="split"
        @click="emit('update:split', !split)"
      >
        {{ split ? 'Unified' : 'Side by side' }}
      </button>
    </header>

    <p v-if="diff.binary" class="p-3 text-sm text-slate-500 dark:text-slate-400">
      A binary file has no diff to read.
    </p>

    <p v-else-if="diff.hunks.length === 0" class="p-3 text-sm text-slate-500 dark:text-slate-400">
      Nothing changed inside this file.
    </p>

    <table v-else-if="split" class="code w-full table-fixed border-collapse font-mono text-xs">
      <tbody v-for="(hunk, h) in shown" :key="h">
        <tr class="bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400">
          <td colspan="4" class="px-2 py-1">
            @@ −{{ hunk.oldStart }},{{ hunk.oldLines }} +{{ hunk.newStart }},{{ hunk.newLines }} @@
            <span v-if="hunk.header" class="ml-2">{{ hunk.header }}</span>
          </td>
        </tr>
        <tr v-for="(pair, p) in pairs(hunk.rows)" :key="`${h}-${p}`">
          <DiffRow :row="pair.left" side="left" :class="pairClass(pair.left)" />
          <DiffRow :row="pair.right" side="right" :class="pairClass(pair.right)" />
        </tr>
      </tbody>
    </table>

    <table v-else class="code w-full border-collapse font-mono text-xs">
      <tbody v-for="(hunk, h) in shown" :key="h">
        <tr class="bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400">
          <td colspan="3" class="px-2 py-1">
            @@ −{{ hunk.oldStart }},{{ hunk.oldLines }} +{{ hunk.newStart }},{{ hunk.newLines }} @@
            <span v-if="hunk.header" class="ml-2">{{ hunk.header }}</span>
          </td>
        </tr>
        <tr v-for="(row, r) in hunk.rows" :key="`${h}-${r}`" :class="rowClass(row.kind)">
          <td class="w-12 select-none px-2 text-right align-top text-slate-400 dark:text-slate-500">
            {{ row.oldLine ?? '' }}
          </td>
          <td class="w-12 select-none px-2 text-right align-top text-slate-400 dark:text-slate-500">
            {{ row.newLine ?? '' }}
          </td>
          <td class="whitespace-pre-wrap break-all px-2 align-top">
            <span class="select-none text-slate-400 dark:text-slate-500">{{ SIGN[row.kind] }}</span
            ><span
              v-for="(seg, s) in segments(row)"
              :key="s"
              :class="[seg.cls, seg.changed ? 'word' : '']"
              >{{ seg.text }}</span
            ><span
              v-if="row.noNewline"
              class="ml-2 text-slate-400 dark:text-slate-500"
              title="the file has no newline after this line"
              >↵?</span
            >
          </td>
        </tr>
      </tbody>
    </table>

    <p
      v-if="capped"
      role="status"
      class="border-t border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
    >
      This file is very large. {{ total - MAX_ROWS }} rows are not shown, because building them
      costs more than reading them.
    </p>
  </div>
</template>
