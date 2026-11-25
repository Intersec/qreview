<script setup lang="ts">
// What Gerrit knows of the change, in the left column of the change page,
// laid out as the Change Info of Gerrit.

import { computed } from 'vue';
import type { ChangeSummary, GerritChange, PatchSet } from '@/api/types';

const props = defineProps<{
  /// The change on the server. Null for a change it does not know.
  gerrit: GerritChange | null;
  sets: PatchSet[];
  /// The change being read, for what the clone knows of it.
  change: ChangeSummary | null;
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
  <section class="change-box">
    <p class="box-title">Change Info</p>
    <dl>
      <template v-if="gerrit">
        <dt>Change</dt>
        <dd>
          <a :href="gerrit.url" target="_blank" rel="noreferrer">{{ gerrit.number }}</a>
          <span class="status" :class="`status-${gerrit.status.toLowerCase()}`">{{ status }}</span>
        </dd>
        <template v-if="gerrit.owner">
          <dt>Owner</dt>
          <dd>{{ gerrit.owner }}</dd>
        </template>
        <dt>Repo | Branch</dt>
        <dd>{{ gerrit.project }} | {{ gerrit.branch }}</dd>
        <template v-if="gerrit.topic">
          <dt>Topic</dt>
          <dd>{{ gerrit.topic }}</dd>
        </template>
        <dt>Comments</dt>
        <dd v-if="posted > 0">{{ posted }} on Gerrit, on {{ where }}</dd>
        <dd v-else class="quiet">none on Gerrit</dd>
      </template>
      <template v-else-if="change">
        <dt>Author</dt>
        <dd>{{ change.author }}</dd>
        <dt>Gerrit</dt>
        <dd class="quiet">not on the server</dd>
      </template>
      <template v-if="change && !change.worktree">
        <dt>Commit</dt>
        <dd>
          <code>{{ change.commit.slice(0, 12) }}</code>
        </dd>
      </template>
    </dl>
  </section>
</template>
