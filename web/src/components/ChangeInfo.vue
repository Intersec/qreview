<script setup lang="ts">
import { computed } from 'vue';
import type { GerritChange, PatchSet } from '@/api/types';

const props = defineProps<{
  /// The change on the server. Without one, there is nothing to show.
  gerrit: GerritChange | null;
  sets: PatchSet[];
}>();

/// The words Gerrit shows for a status.
const STATUS: Record<string, string> = {
  NEW: 'Active',
  MERGED: 'Merged',
  ABANDONED: 'Abandoned',
};

const status = computed(() => {
  const raw = props.gerrit?.status ?? '';
  return STATUS[raw] ?? raw;
});

/// The remarks of the change, every version together, as Gerrit counts them.
const posted = computed(() => props.sets.reduce((sum, set) => sum + (set.posted ?? 0), 0));

/// Where those remarks are, so the reader knows which version to open.
const where = computed(() =>
  props.sets
    .filter((set) => (set.posted ?? 0) > 0)
    .map((set) => `Patchset ${set.number}`)
    .join(', '),
);
</script>

<template>
  <dl v-if="gerrit" class="change-info">
    <div>
      <dt>Change</dt>
      <dd>
        <a :href="gerrit.url" target="_blank" rel="noreferrer">{{ gerrit.number }}</a>
        <span class="status" :class="`status-${gerrit.status.toLowerCase()}`">{{ status }}</span>
      </dd>
    </div>
    <div v-if="gerrit.owner">
      <dt>Owner</dt>
      <dd>{{ gerrit.owner }}</dd>
    </div>
    <div>
      <dt>Repo | Branch</dt>
      <dd>{{ gerrit.project }} | {{ gerrit.branch }}</dd>
    </div>
    <div v-if="gerrit.topic">
      <dt>Topic</dt>
      <dd>{{ gerrit.topic }}</dd>
    </div>
    <div>
      <dt>Comments</dt>
      <dd v-if="posted > 0">{{ posted }} on Gerrit, on {{ where }}</dd>
      <dd v-else class="quiet">none on Gerrit</dd>
    </div>
  </dl>
</template>
