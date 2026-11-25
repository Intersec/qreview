<script setup lang="ts">
// The series, where Gerrit puts the relation chain of a change: newest
// first, the change being read marked with an arrow.

import { inject } from 'vue';
import { THREADS } from '@/diff/threads';
import BoundaryCard from '../BoundaryCard.vue';
import type { ChangeSummary, Series } from '@/api/types';

defineProps<{
  series: Series;
  selected: string | null;
  busy: boolean;
  /// How many commits one click of the boundary card asks for.
  batch: number;
  /// How many comments each change of the series carries.
  counts: Map<string, number>;
  /// How many of them are threads of reviewers on Gerrit that wait.
  gerritCounts?: Map<string, number>;
}>();
/// Absent where no review provides it, as in a test of this list alone.
const threads = inject(THREADS, null);
const emit = defineEmits<{
  openChange: [key: string];
  mark: [key: string, reviewed: boolean];
  more: [count?: number];
}>();

function short(change: ChangeSummary): string {
  return change.commit.slice(0, 8);
}
</script>

<template>
  <section class="relation-chain">
    <p class="box-title">
      Relation chain <span class="quiet">({{ series.changes.length }})</span>
    </p>
    <ul>
      <li
        v-for="change in series.changes"
        :key="change.key"
        :class="change.key === selected ? 'is-picked' : ''"
      >
        <span class="chain-arrow" aria-hidden="true">{{ change.key === selected ? '→' : '' }}</span>
        <button
          type="button"
          class="row-button change-row"
          :class="change.reviewed ? 'is-read' : ''"
          :aria-current="change.key === selected ? 'true' : undefined"
          :title="`${short(change)} · ${change.author}`"
          @click="emit('openChange', change.key)"
        >
          <span class="change-subject">{{ change.subject }}</span>
        </button>
        <span class="change-facts">
          <!-- The sha of the working tree is synthetic: it changes at every
               keystroke and names nothing a reader can look up. -->
          <span v-if="change.worktree" class="tag tag-worktree">not committed</span>
          <span v-if="change.isMerge" class="tag">merge</span>
          <span v-if="!change.changeId && !change.worktree" class="tag">no Change-Id</span>
          <span
            v-if="gerritCounts?.get(change.key)"
            class="count count-gerrit"
            title="Threads of reviewers on Gerrit that wait for an answer"
            >{{ gerritCounts.get(change.key) }} Gerrit</span
          >
          <span
            v-if="(counts.get(change.key) ?? 0) - (gerritCounts?.get(change.key) ?? 0) > 0"
            class="count"
            title="Remarks of this review to handle"
            >{{ (counts.get(change.key) ?? 0) - (gerritCounts?.get(change.key) ?? 0) }} ✎</span
          >
          <span
            v-if="threads?.blockedChanges.value.has(change.key)"
            class="awaits"
            title="The agent waits for your answer"
            >waits for you</span
          >
          <span v-if="change.lines" class="stat">
            <span class="added">+{{ change.lines.added }}</span
            ><span class="removed">−{{ change.lines.removed }}</span>
          </span>
        </span>
        <button
          type="button"
          class="mark-read"
          :aria-pressed="change.reviewed"
          :title="change.reviewed ? 'Marked read' : 'Mark it read'"
          @click.stop="emit('mark', change.key, !change.reviewed)"
        >
          {{ change.reviewed ? '☑' : '☐' }}
        </button>
      </li>
    </ul>
    <BoundaryCard
      :boundary="series.boundary"
      :batch="batch"
      :busy="busy"
      @more="(count?: number) => emit('more', count)"
    />
  </section>
</template>
