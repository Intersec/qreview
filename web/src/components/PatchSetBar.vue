<script setup lang="ts">
import { computed } from 'vue';
import type { GerritChange, PatchSet } from '@/api/types';

const props = defineProps<{
  sets: PatchSet[];
  current: number | undefined;
  against: string | undefined;
  gerrit: GerritChange | null;
}>();
const emit = defineEmits<{ open: [ps: number | undefined, base?: string]; fetch: [ps: number] }>();

const last = computed(() => props.sets[props.sets.length - 1]?.number);
const reading = computed(() => props.current ?? last.value);

const target = computed(() => props.sets.find((s) => s.number === reading.value));
const baseSet = computed(() => {
  const number = Number(props.against?.replace('ps:', ''));
  return Number.isFinite(number) ? props.sets.find((s) => s.number === number) : undefined;
});

/// Two versions written on different parents. A diff between them used to
/// carry the rebase; it no longer does, but the reader still has to know
/// that the two sides did not start from the same place.
const rebased = computed(
  () => target.value && baseSet.value && target.value.parent !== baseSet.value.parent,
);

function label(set: PatchSet): string {
  const when = set.createdAt ? set.createdAt.slice(0, 10) : '';
  const where = set.available ? '' : ' · not fetched';

  return `Patch set ${set.number} | ${set.commit.slice(0, 7)} ${when}${where}`;
}

function pickBase(value: string) {
  emit('open', reading.value, value === 'parent' ? undefined : value);
}

function pickTarget(value: string) {
  const number = Number(value);
  const set = props.sets.find((s) => s.number === number);
  if (set && !set.available) {
    emit('fetch', number);
    return;
  }
  emit('open', number, props.against);
}
</script>

<template>
  <div v-if="sets.length > 1 || gerrit" class="patch-bar">
    <label class="sr-only" for="base-of">Read against</label>
    <select
      id="base-of"
      class="picker"
      :value="against ?? 'parent'"
      @change="pickBase(($event.target as HTMLSelectElement).value)"
    >
      <option value="parent">Base | its parent</option>
      <option
        v-for="set in sets.filter((s) => s.number !== reading)"
        :key="`b${set.number}`"
        :value="`ps:${set.number}`"
      >
        {{ label(set) }}
      </option>
    </select>

    <span class="arrow">→</span>

    <label class="sr-only" for="read-set">Patch set to read</label>
    <select
      id="read-set"
      class="picker"
      :value="String(reading)"
      @change="pickTarget(($event.target as HTMLSelectElement).value)"
    >
      <option v-for="set in sets" :key="`t${set.number}`" :value="String(set.number)">
        {{ label(set) }}
      </option>
    </select>

    <span v-if="gerrit" class="quiet">
      Gerrit change
      <a :href="gerrit.url" target="_blank" rel="noreferrer">{{ gerrit.number }}</a>
      on {{ gerrit.branch }} · {{ gerrit.status }}
    </span>

    <span v-if="rebased" class="rebased">
      These two versions sit on different bases. Only what the change touches is listed.
    </span>
  </div>
</template>
