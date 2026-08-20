<script setup lang="ts">
import type { MergeBase, MergeListItem } from '@/api/types';

defineProps<{ base: MergeBase | undefined; list: MergeListItem[] }>();
const emit = defineEmits<{ pick: [base: MergeBase]; showList: [] }>();

const CHOICES: { id: MergeBase; label: string; hint: string }[] = [
  { id: 'automerge', label: 'Auto-merge', hint: 'the conflict resolution alone' },
  { id: 'parent1', label: 'Parent 1', hint: 'everything the merge brought in' },
  { id: 'parent2', label: 'Parent 2', hint: 'the same, from the other side' },
];
</script>

<template>
  <div
    class="border-b border-amber-300 bg-amber-50 px-3 py-2 text-xs dark:border-amber-800 dark:bg-amber-950/40"
  >
    <div class="flex flex-wrap items-center gap-2">
      <span class="font-semibold uppercase tracking-wide text-amber-900 dark:text-amber-200">
        Merge
      </span>
      <span class="text-slate-600 dark:text-slate-300">read against</span>
      <button
        v-for="choice in CHOICES"
        :key="choice.id"
        type="button"
        class="rounded border px-2 py-0.5"
        :class="
          (base ?? 'automerge') === choice.id
            ? 'border-amber-600 bg-amber-200 font-medium dark:border-amber-500 dark:bg-amber-900'
            : 'border-slate-300 hover:bg-slate-100 dark:border-slate-600 dark:hover:bg-slate-800'
        "
        :title="choice.hint"
        :aria-pressed="(base ?? 'automerge') === choice.id"
        @click="emit('pick', choice.id)"
      >
        {{ choice.label }}
      </button>

      <button
        type="button"
        class="ml-auto rounded border border-slate-300 px-2 py-0.5 hover:bg-slate-100 dark:border-slate-600 dark:hover:bg-slate-800"
        @click="emit('showList')"
      >
        What it brings in
      </button>
    </div>

    <p v-if="(base ?? 'automerge') === 'automerge'" class="mt-1 text-slate-600 dark:text-slate-400">
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
