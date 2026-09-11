<script setup lang="ts">
import { ref } from 'vue';
import BoundaryCard from './BoundaryCard.vue';
import CommentList from './CommentList.vue';
import FileList from './FileList.vue';
import PaneSplit from './PaneSplit.vue';
import type { ChangeComments, ChangeSummary, FileEntry, Series, Side } from '@/api/types';

defineProps<{
  series: Series;
  selected: string | null;
  files: FileEntry[];
  filePath: string | null;
  busy: boolean;
  /// How many commits one click of the boundary card asks for.
  batch: number;
  /// True while the file list of the open change is being read.
  loadingFiles: boolean;
  /// How many comments each change of the series carries.
  counts: Map<string, number>;
  /// Every comment of the session, change by change.
  written: ChangeComments[];
  /// How many comments sit in each file of the change being read.
  inFile: Map<string, number>;
}>();
const emit = defineEmits<{
  openChange: [key: string];
  openFile: [path: string];
  go: [key: string, file: string, side: Side, line: number | null];
  mark: [key: string, reviewed: boolean];
  more: [];
}>();

// A ref inside a `v-for` is a list, even when one element carries it. Only
// the open change draws its files, so the list holds one at most.
const inChange = ref<InstanceType<typeof FileList>[]>([]);

/// How tall the list of comments is. Its title must stay on the screen,
/// and so must a line or two of the series above it.
const LIST_MIN = 28;
const listHeight = ref(Number(localStorage.getItem('qreview.comments.height')) || 180);

function taller(by: number) {
  const room = Math.max(LIST_MIN, window.innerHeight - 200);
  listHeight.value = Math.min(Math.max(listHeight.value - by, LIST_MIN), room);
}

function keepHeight() {
  localStorage.setItem('qreview.comments.height', String(Math.round(listHeight.value)));
}

function short(change: ChangeSummary): string {
  return change.commit.slice(0, 8);
}

defineExpose({ focusFilter: () => inChange.value[0]?.focusFilter() });
</script>

<template>
  <nav class="side">
    <p class="pane-title side-head">Series · {{ series.changes.length }}</p>

    <div class="side-scroll">
      <ul>
        <li v-for="change in series.changes" :key="change.key">
          <span class="change-line">
            <button
              type="button"
              class="mark-read"
              :aria-pressed="change.reviewed"
              :title="change.reviewed ? 'Marked read' : 'Mark it read'"
              @click.stop="emit('mark', change.key, !change.reviewed)"
            >
              {{ change.reviewed ? '☑' : '☐' }}
            </button>
            <button
              type="button"
              class="row-button change-row"
              :class="[
                change.key === selected ? 'is-picked' : '',
                change.reviewed ? 'is-read' : '',
              ]"
              :aria-current="change.key === selected ? 'true' : undefined"
              @click="emit('openChange', change.key)"
            >
              <span class="change-subject">{{ change.subject }}</span>
              <span class="change-facts">
                <!-- The sha of the working tree is synthetic: it changes at
                   every keystroke and names nothing a reader can look up. -->
                <span v-if="change.worktree" class="tag tag-worktree">not committed</span>
                <code v-else>{{ short(change) }}</code>
                <span v-if="change.isMerge" class="tag">merge</span>
                <span v-if="!change.changeId && !change.worktree" class="tag">no Change-Id</span>
                <span
                  v-else-if="change.key.startsWith('sha-')"
                  class="tag"
                  title="Another change in
              this series carries the same Change-Id, so this one is keyed by its hash"
                >
                  same Change-Id
                </span>
                <span v-if="change.patchSetCount > 1">
                  · {{ change.patchSetCount }} patch sets
                </span>
                <span v-if="counts.get(change.key)" class="count"
                  >· {{ counts.get(change.key) }} ✎</span
                >
              </span>
            </button>
          </span>

          <!-- The files of the change being read, and of no other. -->
          <FileList
            v-if="change.key === selected"
            ref="inChange"
            :files="files"
            :file-path="filePath"
            :loading="loadingFiles"
            :in-file="inFile"
            @open-file="emit('openFile', $event)"
          />
        </li>
      </ul>

      <BoundaryCard :boundary="series.boundary" :batch="batch" :busy="busy" @more="emit('more')" />
    </div>

    <PaneSplit
      v-if="written.length > 0"
      direction="horizontal"
      label="Make the list of comments taller or shorter"
      @move="taller"
      @done="keepHeight"
    />

    <!-- Last, and it takes the room that is left rather than pushing the
         series out of the pane. -->
    <CommentList
      :height="listHeight"
      :written="written"
      :open-key="selected"
      @go="(key, file, side, line) => emit('go', key, file, side, line)"
    />
  </nav>
</template>
