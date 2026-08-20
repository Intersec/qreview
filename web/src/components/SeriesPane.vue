<script setup lang="ts">
import BoundaryCard from './BoundaryCard.vue';
import type { ChangeSummary, Series } from '@/api/types';

defineProps<{ series: Series; selected: string | null; busy: boolean }>();
const emit = defineEmits<{ open: [key: string]; more: []; reviewMerge: [] }>();

function label(change: ChangeSummary): string {
  return change.commit.slice(0, 12);
}
</script>

<template>
  <nav class="flex h-full flex-col gap-3 overflow-y-auto p-3">
    <h2 class="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
      Series · {{ series.changes.length }}
    </h2>

    <p v-if="series.changes.length === 0" class="text-sm text-slate-500 dark:text-slate-400">
      No change is loaded.
    </p>

    <ul class="space-y-1">
      <li v-for="change in series.changes" :key="change.key">
        <button
          type="button"
          class="w-full rounded px-2 py-1.5 text-left text-sm hover:bg-slate-100 dark:hover:bg-slate-800"
          :class="change.key === selected ? 'bg-slate-200 font-medium dark:bg-slate-700' : ''"
          :aria-current="change.key === selected ? 'true' : undefined"
          @click="emit('open', change.key)"
        >
          <span class="block truncate">{{ change.subject }}</span>
          <span class="block text-xs text-slate-500 dark:text-slate-400">
            <code>{{ label(change) }}</code>
            <span v-if="!change.changeId" class="ml-1">· no Change-Id</span>
          </span>
        </button>
      </li>
    </ul>

    <BoundaryCard
      :boundary="series.boundary"
      :busy="busy"
      @more="emit('more')"
      @review-merge="emit('reviewMerge')"
    />
  </nav>
</template>
