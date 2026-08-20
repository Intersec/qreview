<script setup lang="ts">
import { computed } from 'vue';
import type { GerritChange, PatchSet } from '@/api/types';

const props = defineProps<{
  sets: PatchSet[];
  current: number | undefined;
  against: string | undefined;
  gerrit: GerritChange | null;
}>();
const emit = defineEmits<{
  open: [ps: number | undefined, base?: string];
  fetch: [ps: number];
}>();

const last = computed(() => props.sets[props.sets.length - 1]?.number);
const reading = computed(() => props.current ?? last.value);

const target = computed(() => props.sets.find((s) => s.number === reading.value));
const baseSet = computed(() => {
  const number = Number(props.against?.replace('ps:', ''));
  return Number.isFinite(number) ? props.sets.find((s) => s.number === number) : undefined;
});

/// Two versions written on different parents. A diff between them carries
/// the rebase, and the reader has to know rather than be surprised.
const rebased = computed(
  () => target.value && baseSet.value && target.value.parent !== baseSet.value.parent,
);
</script>

<template>
  <div
    v-if="sets.length > 1 || gerrit"
    class="border-b border-slate-200 bg-slate-50 px-3 py-2 text-xs dark:border-slate-700 dark:bg-slate-800/60"
  >
    <div class="flex flex-wrap items-center gap-2">
      <span class="font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
        Patch set
      </span>
      <button
        v-for="set in sets"
        :key="set.number"
        type="button"
        class="rounded border px-2 py-0.5"
        :class="
          set.number === reading
            ? 'border-slate-500 bg-slate-200 font-medium dark:border-slate-400 dark:bg-slate-700'
            : 'border-slate-300 hover:bg-slate-100 dark:border-slate-600 dark:hover:bg-slate-700'
        "
        :aria-pressed="set.number === reading"
        :title="`${set.origin} · ${set.commit.slice(0, 12)}${set.available ? '' : ' · not fetched yet'}`"
        @click="set.available ? emit('open', set.number, against) : emit('fetch', set.number)"
      >
        {{ set.number }}<span v-if="!set.available" aria-hidden="true"> ↓</span>
      </button>

      <span class="ml-2 text-slate-600 dark:text-slate-300">against</span>
      <button
        type="button"
        class="rounded border px-2 py-0.5"
        :class="
          against
            ? 'border-slate-300 hover:bg-slate-100 dark:border-slate-600 dark:hover:bg-slate-700'
            : 'border-slate-500 bg-slate-200 font-medium dark:border-slate-400 dark:bg-slate-700'
        "
        @click="emit('open', reading, undefined)"
      >
        its parent
      </button>
      <button
        v-for="set in sets.filter((s) => s.number !== reading)"
        :key="`base-${set.number}`"
        type="button"
        class="rounded border px-2 py-0.5"
        :class="
          against === `ps:${set.number}`
            ? 'border-slate-500 bg-slate-200 font-medium dark:border-slate-400 dark:bg-slate-700'
            : 'border-slate-300 hover:bg-slate-100 dark:border-slate-600 dark:hover:bg-slate-700'
        "
        @click="emit('open', reading, `ps:${set.number}`)"
      >
        patch set {{ set.number }}
      </button>
    </div>

    <p v-if="gerrit" class="mt-1 text-slate-600 dark:text-slate-400">
      Gerrit change
      <a :href="gerrit.url" target="_blank" rel="noreferrer" class="underline">
        {{ gerrit.number }}
      </a>
      on {{ gerrit.branch }} · {{ gerrit.status }}. A number with an arrow is on the server and not
      in this clone yet.
    </p>

    <p v-if="rebased" class="mt-1 text-amber-800 dark:text-amber-300">
      These two versions sit on different bases. The diff carries the rebase as well as the work.
    </p>
  </div>
</template>
