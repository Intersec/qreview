<script setup lang="ts">
import { computed } from 'vue';
import type { Boundary } from '@/api/types';

const props = defineProps<{ boundary: Boundary; busy: boolean }>();
const emit = defineEmits<{ more: [] }>();

const title = computed(() => {
  switch (props.boundary.kind) {
    case 'merge':
      return 'Merge';
    case 'tag':
      return 'Tag';
    case 'base':
      return 'Base of the series';
    case 'guess':
      return 'Guessed start';
    case 'batch':
      return 'More to load';
    case 'root':
      return 'Start of the history';
  }
  return 'Boundary';
});

const short = computed(() => props.boundary.commit?.slice(0, 12) ?? '');
</script>

<template>
  <section
    class="rounded border border-dashed border-slate-400 p-3 text-xs dark:border-slate-600"
    :class="boundary.kind === 'merge' ? 'bg-amber-50 dark:bg-amber-950/30' : ''"
  >
    <p class="font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
      {{ title }}
    </p>
    <p class="mt-1 text-slate-700 dark:text-slate-300">{{ boundary.reason }}</p>

    <p v-if="boundary.guessed" class="mt-1 text-slate-500 dark:text-slate-400">
      This is a guess. Load more to go further back.
    </p>

    <div v-if="boundary.merge" class="mt-2">
      <p class="font-medium text-slate-700 dark:text-slate-300">{{ boundary.merge.subject }}</p>
      <ul class="mt-1 space-y-0.5">
        <li v-for="(parent, i) in boundary.merge.parents" :key="parent.commit">
          <span class="text-slate-500 dark:text-slate-400">parent {{ i + 1 }}</span>
          <code class="ml-1">{{ parent.commit.slice(0, 12) }}</code>
          <span class="ml-1">{{ parent.name }}</span>
          <span v-if="parent.remote" class="ml-1 text-slate-500 dark:text-slate-400">
            (a remote branch)
          </span>
        </li>
      </ul>
    </div>

    <p v-if="short" class="mt-2 text-slate-500 dark:text-slate-400">
      next commit <code>{{ short }}</code>
    </p>

    <button
      v-if="boundary.commit"
      type="button"
      class="mt-2 rounded border border-slate-400 px-2 py-1 hover:bg-slate-100 disabled:opacity-50 dark:border-slate-600 dark:hover:bg-slate-800"
      :disabled="busy"
      @click="emit('more')"
    >
      Load 5 older
    </button>
  </section>
</template>
