<script setup lang="ts">
import { computed, ref } from 'vue';
import type { FileEntry } from '@/api/types';

const props = defineProps<{ files: FileEntry[]; selected: string | null }>();
const emit = defineEmits<{ open: [path: string] }>();

const filter = ref('');
const box = ref<HTMLInputElement | null>(null);

const shown = computed(() => {
  const needle = filter.value.trim().toLowerCase();
  if (needle === '') {
    return props.files;
  }
  return props.files.filter(
    (file) =>
      file.path.toLowerCase().includes(needle) ||
      (file.oldPath ?? '').toLowerCase().includes(needle),
  );
});

defineExpose({ focusFilter: () => box.value?.focus() });

const MARK: Record<FileEntry['status'], string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  renamed: 'R',
  copied: 'C',
};
</script>

<template>
  <div class="flex h-full flex-col overflow-y-auto p-3">
    <h2 class="text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
      Files · {{ shown.length
      }}<span v-if="shown.length !== files.length"> of {{ files.length }}</span>
    </h2>

    <input
      ref="box"
      v-model="filter"
      type="search"
      placeholder="Filter the files"
      aria-label="Filter the files"
      class="mt-2 w-full rounded border border-slate-300 px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
    />

    <p v-if="shown.length === 0" class="mt-2 text-sm text-slate-500 dark:text-slate-400">
      No file matches.
    </p>

    <ul class="mt-2 space-y-0.5">
      <li v-for="file in shown" :key="file.path">
        <button
          type="button"
          class="flex w-full items-baseline gap-2 rounded px-2 py-1 text-left text-sm hover:bg-slate-100 disabled:opacity-60 dark:hover:bg-slate-800"
          :class="file.path === selected ? 'bg-slate-200 dark:bg-slate-700' : ''"
          :disabled="file.binary"
          @click="emit('open', file.path)"
        >
          <span class="w-3 shrink-0 font-mono text-xs text-slate-500 dark:text-slate-400">
            {{ MARK[file.status] }}
          </span>
          <span class="min-w-0 flex-1 truncate">
            <span v-if="file.oldPath" class="text-slate-500 dark:text-slate-400">
              {{ file.oldPath }} →
            </span>
            {{ file.path }}
          </span>
          <span v-if="file.binary" class="shrink-0 text-xs text-slate-500 dark:text-slate-400">
            binary
          </span>
          <span v-else class="shrink-0 font-mono text-xs">
            <span class="text-emerald-700 dark:text-emerald-400">+{{ file.added }}</span>
            <span class="ml-1 text-rose-700 dark:text-rose-400">−{{ file.removed }}</span>
          </span>
        </button>
      </li>
    </ul>
  </div>
</template>
