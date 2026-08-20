<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import DiffView from './components/DiffView.vue';
import FileList from './components/FileList.vue';
import MergeBar from './components/MergeBar.vue';
import SeriesPane from './components/SeriesPane.vue';
import { useReview } from './stores/review';

const review = useReview();
const {
  series,
  changeKey,
  files,
  filePath,
  diff,
  error,
  busy,
  version,
  onMerge,
  mergeBase,
  mergeList,
  split,
} = storeToRefs(review);

const fileList = ref<InstanceType<typeof FileList> | null>(null);

/// Move through the review without the mouse.
///
/// j and k walk the files, n and p walk the changes, u swaps the two diff
/// views, and / jumps to the filter. A key typed into a field is the text of
/// that field and nothing else.
function onKey(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  const typing =
    target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable;
  if (typing || event.metaKey || event.ctrlKey || event.altKey) {
    return;
  }

  const step = (list: string[], current: string | null, by: number): string | null => {
    if (list.length === 0) {
      return null;
    }
    const at = current === null ? -1 : list.indexOf(current);
    const next = Math.min(Math.max(at + by, 0), list.length - 1);
    return list[next] ?? null;
  };

  const paths = files.value.filter((f) => !f.binary).map((f) => f.path);
  const keys = series.value?.changes.map((c) => c.key) ?? [];

  switch (event.key) {
    case 'j':
    case 'k': {
      const path = step(paths, filePath.value, event.key === 'j' ? 1 : -1);
      if (path) {
        review.openFile(path);
      }
      break;
    }
    case 'n':
    case 'p': {
      const key = step(keys, changeKey.value, event.key === 'n' ? 1 : -1);
      if (key) {
        review.openChange(key);
      }
      break;
    }
    case 'u':
      review.setSplit(!split.value);
      break;
    case '/':
      event.preventDefault();
      fileList.value?.focusFilter();
      break;
    default:
      return;
  }
}

onMounted(() => {
  review.load();
  window.addEventListener('keydown', onKey);
});

onBeforeUnmount(() => window.removeEventListener('keydown', onKey));
</script>

<template>
  <div class="flex h-screen flex-col bg-white text-slate-900 dark:bg-slate-900 dark:text-slate-100">
    <header
      class="flex shrink-0 items-baseline gap-3 border-b border-slate-200 px-3 py-2 dark:border-slate-700"
    >
      <h1 class="font-semibold">qreview</h1>
      <p v-if="series" class="truncate text-xs text-slate-500 dark:text-slate-400">
        {{ series.repo.remote ?? series.repo.root }}
      </p>
      <p class="ml-auto text-xs text-slate-400 dark:text-slate-500">{{ version }}</p>
    </header>

    <p
      v-if="error"
      role="alert"
      class="bg-rose-100 px-3 py-2 text-sm text-rose-900 dark:bg-rose-950 dark:text-rose-200"
    >
      {{ error }}
    </p>

    <main v-if="series" class="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[18rem_20rem_1fr]">
      <SeriesPane
        class="border-b border-slate-200 md:border-b-0 md:border-r dark:border-slate-700"
        :series="series"
        :selected="changeKey"
        :busy="busy"
        @open="review.openChange"
        @more="review.loadMore(5)"
        @review-merge="review.openMerge()"
      />
      <FileList
        ref="fileList"
        class="border-b border-slate-200 md:border-b-0 md:border-r dark:border-slate-700"
        :files="files"
        :selected="filePath"
        @open="review.openFile"
      />
      <section class="flex min-h-0 flex-col">
        <MergeBar
          v-if="onMerge"
          :base="mergeBase"
          :list="mergeList"
          @pick="review.openMerge($event)"
          @show-list="review.loadMergeList()"
        />
        <DiffView
          v-if="diff"
          class="min-h-0 flex-1"
          :diff="diff"
          :split="split"
          @update:split="review.setSplit"
        />
        <p v-else class="p-3 text-sm text-slate-500 dark:text-slate-400">
          {{
            files.length === 0 ? 'This change touches no file.' : 'Pick a file to read its diff.'
          }}
        </p>
      </section>
    </main>

    <p v-else class="p-3 text-sm text-slate-500 dark:text-slate-400">Reading the repository…</p>
  </div>
</template>
