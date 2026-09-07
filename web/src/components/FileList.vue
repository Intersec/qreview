<script setup lang="ts">
import { computed, ref } from 'vue';
import { group } from '@/diff/tree';
import LoadingVeil from './LoadingVeil.vue';
import type { FileEntry } from '@/api/types';

const props = defineProps<{
  files: FileEntry[];
  filePath: string | null;
  /// True while the list is being read.
  loading: boolean;
  /// How many comments sit in each file.
  inFile: Map<string, number>;
}>();
const emit = defineEmits<{ openFile: [path: string] }>();

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

const groups = computed(() => group(shown.value));

defineExpose({ focusFilter: () => box.value?.focus() });
</script>

<template>
  <div class="files">
    <!-- `/` moves here, so the box is there whenever it can filter
       anything. One file needs no filter. -->
    <input
      v-if="files.length > 1"
      ref="box"
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
        :title="file.entry.oldPath ? `${file.entry.oldPath} → ${file.entry.path}` : file.entry.path"
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
    <LoadingVeil :when="loading" label="Reading the files" />
  </div>
</template>
