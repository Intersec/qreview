<script setup lang="ts">
import { segments } from '@/diff/segments';
import type { FileDiff, Row } from '@/api/types';

defineProps<{ diff: FileDiff }>();

const SIGN: Record<Row['kind'], string> = { context: ' ', add: '+', remove: '−' };

function rowClass(kind: Row['kind']): string {
  if (kind === 'add') {
    return 'bg-emerald-50 dark:bg-emerald-950/40';
  }
  if (kind === 'remove') {
    return 'bg-rose-50 dark:bg-rose-950/40';
  }
  return '';
}
</script>

<template>
  <div class="h-full overflow-auto">
    <header
      class="sticky top-0 border-b border-slate-200 bg-white px-3 py-2 dark:border-slate-700 dark:bg-slate-900"
    >
      <h2 class="font-mono text-sm">
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
    </header>

    <p v-if="diff.binary" class="p-3 text-sm text-slate-500 dark:text-slate-400">
      A binary file has no diff to read.
    </p>

    <p v-else-if="diff.hunks.length === 0" class="p-3 text-sm text-slate-500 dark:text-slate-400">
      Nothing changed inside this file.
    </p>

    <table v-else class="code w-full border-collapse font-mono text-xs">
      <tbody v-for="(hunk, h) in diff.hunks" :key="h">
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
  </div>
</template>
