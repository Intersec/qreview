// What the interface knows: the series, the change being read, its files,
// and the diff of the file being read.

import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '@/api/client';
import type { FileDiff, FileEntry, Series } from '@/api/types';

export const useReview = defineStore('review', () => {
  const version = ref('');
  const series = ref<Series | null>(null);
  const changeKey = ref<string | null>(null);
  const files = ref<FileEntry[]>([]);
  const filePath = ref<string | null>(null);
  const diff = ref<FileDiff | null>(null);
  const error = ref<string | null>(null);
  const busy = ref(false);

  async function guard(work: () => Promise<void>) {
    busy.value = true;
    error.value = null;
    try {
      await work();
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e);
    } finally {
      busy.value = false;
    }
  }

  async function load() {
    await guard(async () => {
      const body = await api.session();
      version.value = body.version;
      series.value = body.series;

      const first = body.series.changes[0];
      if (first) {
        await openChange(first.key);
      }
    });
  }

  /** Load the next batch. It only appends, so nothing already read moves. */
  async function loadMore(count = 5) {
    await guard(async () => {
      series.value = await api.extend(count);
    });
  }

  async function openChange(key: string) {
    await guard(async () => {
      changeKey.value = key;
      files.value = await api.files(key);
      diff.value = null;
      filePath.value = null;

      const first = files.value.find((f) => !f.binary);
      if (first) {
        await openFile(first.path);
      }
    });
  }

  async function openFile(path: string) {
    const key = changeKey.value;
    if (!key) {
      return;
    }
    await guard(async () => {
      filePath.value = path;
      diff.value = await api.diff(key, path);
    });
  }

  return {
    version,
    series,
    changeKey,
    files,
    filePath,
    diff,
    error,
    busy,
    load,
    loadMore,
    openChange,
    openFile,
  };
});
