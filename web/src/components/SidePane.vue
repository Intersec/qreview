<script setup lang="ts">
import { computed, ref } from 'vue';
import BoundaryCard from './BoundaryCard.vue';
import { group } from '@/diff/tree';
import CommentList from './CommentList.vue';
import PaneSplit from './PaneSplit.vue';
import LoadingVeil from './LoadingVeil.vue';
import type { ChangeComments, ChangeSummary, FileEntry, Series, Side } from '@/api/types';

const props = defineProps<{
  series: Series;
  selected: string | null;
  files: FileEntry[];
  filePath: string | null;
  busy: boolean;
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
  reviewMerge: [];
}>();

const filter = ref('');
// A ref inside a `v-for` is a list, even when one element carries it. Only
// the open change draws the filter, so the list holds one input at most.
const boxes = ref<HTMLInputElement[]>([]);

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

const MARK: Record<FileEntry['status'], string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  renamed: 'R',
  copied: 'C',
};

const groups = computed(() => group(shown.value));

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

function short(change: ChangeSummary): string {
  return change.commit.slice(0, 8);
}

defineExpose({ focusFilter: () => boxes.value[0]?.focus() });
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
                <code>{{ short(change) }}</code>
                <span v-if="change.isMerge" class="tag">merge</span>
                <span v-if="!change.changeId" class="tag">no Change-Id</span>
                <span
                  v-else-if="change.key.startsWith('sha-')"
                  class="tag"
                  title="Another change in
              this series carries the same Change-Id, so this one is keyed by its hash"
                >
                  same Change-Id
                </span>
                <span v-if="change.patchSetCount > 1">· {{ change.patchSetCount }} patch sets</span>
                <span v-if="counts.get(change.key)" class="count"
                  >· {{ counts.get(change.key) }} ✎</span
                >
              </span>
            </button>
          </span>

          <!-- The files of the change being read, and of no other. -->
          <div v-if="change.key === selected" class="files">
            <!-- `/` moves here, so the box is there whenever it can filter
               anything. One file needs no filter. -->
            <input
              v-if="files.length > 1"
              ref="boxes"
              v-model="filter"
              type="search"
              placeholder="Filter the files"
              aria-label="Filter the files"
              class="file-filter"
            />
            <template v-for="folder in groups" :key="folder.dir">
              <p v-if="folder.dir" class="dir">{{ folder.dir }}/</p>
              <button
                v-for="file in folder.files"
                :key="file.entry.path"
                type="button"
                class="row-button file-row"
                :class="file.entry.path === filePath ? 'is-picked' : ''"
                :disabled="file.entry.binary"
                :title="
                  file.entry.oldPath
                    ? `${file.entry.oldPath} → ${file.entry.path}`
                    : file.entry.path
                "
                @click="emit('openFile', file.entry.path)"
              >
                <span class="mark">{{ MARK[file.entry.status] }}</span>
                <span class="file-path">{{ file.name }}</span>
                <span v-if="inFile.get(file.entry.path)" class="count"
                  >{{ inFile.get(file.entry.path) }} ✎</span
                >
                <span v-if="file.entry.binary" class="quiet">bin</span>
                <span v-else class="stat">
                  <span class="added">+{{ file.entry.added }}</span
                  ><span class="removed">−{{ file.entry.removed }}</span>
                </span>
              </button>
            </template>
            <p v-if="shown.length === 0" class="quiet pad">No file matches.</p>
            <LoadingVeil :when="loadingFiles" label="Reading the files" />
          </div>
        </li>
      </ul>

      <BoundaryCard
        :boundary="series.boundary"
        :busy="busy"
        @more="emit('more')"
        @review-merge="emit('reviewMerge')"
      />
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
