<script setup lang="ts">
import { onMounted } from 'vue';
import { storeToRefs } from 'pinia';
import DiffView from './components/DiffView.vue';
import FileList from './components/FileList.vue';
import SeriesPane from './components/SeriesPane.vue';
import { useReview } from './stores/review';

const review = useReview();
const { series, changeKey, files, filePath, diff, error, busy, version } = storeToRefs(review);

onMounted(() => review.load());
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
      />
      <FileList
        class="border-b border-slate-200 md:border-b-0 md:border-r dark:border-slate-700"
        :files="files"
        :selected="filePath"
        @open="review.openFile"
      />
      <DiffView v-if="diff" :diff="diff" />
      <p v-else class="p-3 text-sm text-slate-500 dark:text-slate-400">
        Pick a file to read its diff.
      </p>
    </main>

    <p v-else class="p-3 text-sm text-slate-500 dark:text-slate-400">Reading the repository…</p>
  </div>
</template>
