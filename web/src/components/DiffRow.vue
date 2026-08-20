<script setup lang="ts">
import { segments } from '@/diff/segments';
import type { Row } from '@/api/types';

defineProps<{ row: Row | null; side: 'left' | 'right' }>();

const SIGN: Record<Row['kind'], string> = { context: ' ', add: '+', remove: '−' };
</script>

<template>
  <template v-if="row">
    <td class="w-12 select-none px-2 text-right align-top text-slate-400 dark:text-slate-500">
      {{ side === 'left' ? (row.oldLine ?? '') : (row.newLine ?? '') }}
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
  </template>
  <template v-else>
    <td class="w-12"></td>
    <td class="bg-slate-50 dark:bg-slate-800/50"></td>
  </template>
</template>
