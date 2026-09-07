<script setup lang="ts">
import type { MergeListItem } from '@/api/types';

/// `base` is what the base selector holds. Absent is the auto-merge.
defineProps<{ base: string | undefined; list: MergeListItem[] }>();
const emit = defineEmits<{ showList: [] }>();
</script>

<template>
  <div
    class="border-b border-amber-300 bg-amber-50 px-3 py-2 text-xs dark:border-amber-800 dark:bg-amber-950/40"
  >
    <div class="flex flex-wrap items-center gap-2">
      <span class="font-semibold uppercase tracking-wide text-amber-900 dark:text-amber-200">
        Merge
      </span>
      <button
        type="button"
        class="ml-auto rounded border border-slate-300 px-2 py-0.5 hover:bg-slate-100 dark:border-slate-600 dark:hover:bg-slate-800"
        @click="emit('showList')"
      >
        What it brings in
      </button>
    </div>

    <p v-if="base === undefined" class="mt-1 text-slate-600 dark:text-slate-400">
      The auto-merge shows what a person resolved. The rest was already reviewed on the branch it
      came from.
    </p>

    <ol v-if="list.length" class="mt-2 space-y-0.5">
      <li v-for="item in list" :key="item.commit" class="truncate">
        <code>{{ item.commit.slice(0, 12) }}</code>
        <span class="ml-2">{{ item.subject }}</span>
        <span class="ml-2 text-slate-500 dark:text-slate-400">{{ item.author }}</span>
      </li>
    </ol>
  </div>
</template>
