<script setup lang="ts">
import { computed, ref } from 'vue';
import BoundaryCard from './BoundaryCard.vue';
import type { ChangeSummary, FileEntry, Series } from '@/api/types';

const props = defineProps<{
  series: Series;
  selected: string | null;
  files: FileEntry[];
  filePath: string | null;
  busy: boolean;
}>();
const emit = defineEmits<{
  openChange: [key: string];
  openFile: [path: string];
  more: [];
  reviewMerge: [];
}>();

const filter = ref('');
const box = ref<HTMLInputElement | null>(null);

const MARK: Record<FileEntry['status'], string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  renamed: 'R',
  copied: 'C',
};

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

defineExpose({ focusFilter: () => box.value?.focus() });
</script>

<template>
  <nav class="side">
    <p class="pane-title side-head">Series · {{ series.changes.length }}</p>

    <ul>
      <li v-for="change in series.changes" :key="change.key">
        <button
          type="button"
          class="row-button change-row"
          :class="change.key === selected ? 'is-picked' : ''"
          :aria-current="change.key === selected ? 'true' : undefined"
          @click="emit('openChange', change.key)"
        >
          <span class="change-subject">{{ change.subject }}</span>
          <span class="change-facts">
            <code>{{ short(change) }}</code>
            <span v-if="change.isMerge" class="tag">merge</span>
            <span v-if="!change.changeId" class="tag">no Change-Id</span>
            <span v-if="change.patchSetCount > 1">· {{ change.patchSetCount }} patch sets</span>
            <span v-if="change.commentCount" class="count">· {{ change.commentCount }} ✎</span>
          </span>
        </button>

        <!-- The files of the change being read, and of no other. -->
        <div v-if="change.key === selected" class="files">
          <input
            v-if="files.length > 6"
            ref="box"
            v-model="filter"
            type="search"
            placeholder="Filter the files"
            aria-label="Filter the files"
            class="file-filter"
          />
          <button
            v-for="file in shown"
            :key="file.path"
            type="button"
            class="row-button file-row"
            :class="file.path === filePath ? 'is-picked' : ''"
            :disabled="file.binary"
            :title="file.oldPath ? `${file.oldPath} → ${file.path}` : file.path"
            @click="emit('openFile', file.path)"
          >
            <span class="mark">{{ MARK[file.status] }}</span>
            <span class="file-path">{{ file.path }}</span>
            <span v-if="file.binary" class="quiet">bin</span>
            <span v-else class="stat">
              <span class="added">+{{ file.added }}</span
              ><span class="removed">−{{ file.removed }}</span>
            </span>
          </button>
          <p v-if="shown.length === 0" class="quiet pad">No file matches.</p>
        </div>
      </li>
    </ul>

    <BoundaryCard
      :boundary="series.boundary"
      :busy="busy"
      @more="emit('more')"
      @review-merge="emit('reviewMerge')"
    />
  </nav>
</template>
