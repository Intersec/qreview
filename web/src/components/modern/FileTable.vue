<script setup lang="ts">
import { computed, inject, ref } from 'vue';
import { THREADS } from '@/diff/threads';
import { COMMIT_MSG } from '@/diff/paths';
import { group } from '@/diff/tree';
import LoadingVeil from '../LoadingVeil.vue';
import type { FileEntry } from '@/api/types';

/// Absent where no review provides it, as in a test of this list alone.
const threads = inject(THREADS, null);

const props = defineProps<{
  files: FileEntry[];
  filePath: string | null;
  /// True while the list is being read.
  loading: boolean;
  /// How many comments sit in each file.
  inFile: Map<string, number>;
  /// How many Gerrit remarks sit in each file of the versions on the screen.
  postedInFile?: Map<string, number>;
  /// The diff of the open file has landed, so its box can unfold to the
  /// height it will keep.
  openReady?: boolean;
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

/// The largest file of the change, by the lines it touches. The size bar of
/// each row is measured against it, as on Gerrit.
const largest = computed(() =>
  Math.max(1, ...props.files.map((file) => file.added + file.removed)),
);

function widthOf(lines: number): string {
  return `${(100 * lines) / largest.value}%`;
}

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
    <!-- Laid out as the file table of Gerrit: the path, the comments, the
         size, the lines. -->
    <div class="file-head" aria-hidden="true">
      <span class="mark"></span>
      <span class="file-path">File</span>
      <span class="file-comments">Comments</span>
      <span class="file-size">Size</span>
      <span class="stat">Delta</span>
      <span></span>
    </div>
    <template v-for="folder in groups" :key="folder.dir">
      <template v-for="file in folder.files" :key="file.entry.path">
        <button
          type="button"
          class="row-button file-row"
          :class="file.entry.path === filePath ? 'is-picked' : ''"
          :disabled="file.entry.binary"
          :aria-expanded="file.entry.path === filePath"
          :title="
            file.entry.oldPath ? `${file.entry.oldPath} → ${file.entry.path}` : file.entry.path
          "
          @click="emit('openFile', file.entry.path)"
        >
          <span class="mark"
            ><span v-if="file.entry.path !== COMMIT_MSG" class="status-box">{{
              MARK[file.entry.status]
            }}</span></span
          >
          <span class="file-path"
            ><span v-if="folder.dir" class="file-dir">{{ folder.dir }}/</span
            ><span class="file-name">{{ file.name }}</span></span
          >
          <span class="file-comments">
            <span
              v-if="inFile.get(file.entry.path)"
              class="comment-chip is-mine"
              :title="`${inFile.get(file.entry.path)} comments in qreview`"
              ><svg viewBox="0 0 16 16" aria-hidden="true">
                <path d="M2 2h12v9H6l-3 3v-3H2z" /></svg
              >{{ inFile.get(file.entry.path) }}</span
            >
            <span
              v-if="postedInFile?.get(file.entry.path)"
              class="comment-chip"
              :title="`${postedInFile.get(file.entry.path)} Gerrit comments on the versions on the screen`"
              ><svg viewBox="0 0 16 16" aria-hidden="true">
                <path d="M2 2h12v9H6l-3 3v-3H2z" /></svg
              >{{ postedInFile.get(file.entry.path) }}</span
            >
            <span
              v-if="threads?.blockedFiles.value.has(file.entry.path)"
              class="awaits"
              title="The agent waits for your answer"
              >!</span
            >
          </span>
          <span class="file-size">
            <span v-if="!file.entry.binary" class="size-bar">
              <span class="size-added" :style="{ width: widthOf(file.entry.added) }"></span
              ><span class="size-removed" :style="{ width: widthOf(file.entry.removed) }"></span>
            </span>
          </span>
          <span v-if="file.entry.binary" class="stat quiet">bin</span>
          <span v-else class="stat">
            <span class="removed">−{{ file.entry.removed }}</span
            ><span class="added">+{{ file.entry.added }}</span>
          </span>
          <svg class="row-chevron" viewBox="0 0 16 16" aria-hidden="true">
            <path :d="file.entry.path === filePath ? 'M4 10l4-4 4 4' : 'M4 6l4 4 4-4'" />
          </svg>
        </button>
        <!-- The diff of the open file stands under its row. It unfolds once
             it is read, so it never jumps when the content lands. -->
        <Transition name="unfold">
          <div v-if="file.entry.path === filePath && openReady" class="unfold-box">
            <slot name="open" />
          </div>
        </Transition>
      </template>
    </template>
    <p v-if="shown.length === 0" class="quiet pad">No file matches.</p>
    <LoadingVeil :when="loading" label="Reading the files" />
  </div>
</template>
