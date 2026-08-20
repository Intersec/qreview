// What the interface knows: the series, the change being read, its files,
// and the diff of the file being read.

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { api } from '@/api/client';
import type {
  ChangeFile,
  Comment,
  EditComment,
  FileDiff,
  FileEntry,
  MergeBase,
  MergeListItem,
  NewComment,
  Series,
} from '@/api/types';

export const useReview = defineStore('review', () => {
  const version = ref('');
  const series = ref<Series | null>(null);
  const changeKey = ref<string | null>(null);
  const files = ref<FileEntry[]>([]);
  const filePath = ref<string | null>(null);
  const diff = ref<FileDiff | null>(null);
  const error = ref<string | null>(null);
  const busy = ref(false);
  const mergeBase = ref<MergeBase | undefined>(undefined);
  const review = ref<ChangeFile | null>(null);
  const split = ref(localStorage.getItem('qreview.split') === 'yes');
  const mergeList = ref<MergeListItem[]>([]);

  /// True while the reader is on the merge under the boundary.
  const onMerge = computed(
    () => changeKey.value !== null && changeKey.value === series.value?.boundary.commit,
  );

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

  async function openChange(key: string, base?: MergeBase) {
    await guard(async () => {
      changeKey.value = key;
      mergeBase.value = base;
      mergeList.value = [];
      review.value = await api.comments(key);
      files.value = await api.files(key, base);
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
      diff.value = await api.diff(key, path, mergeBase.value);
    });
  }

  /// Open the merge under the boundary, against the base the reader picked.
  async function openMerge(base?: MergeBase) {
    const commit = series.value?.boundary.commit;
    if (!commit) {
      return;
    }
    await openChange(commit, base);
  }

  async function loadMergeList() {
    const key = changeKey.value;
    if (!key) {
      return;
    }
    await guard(async () => {
      mergeList.value = await api.mergeList(key);
    });
  }

  /// The threads of the change: a first comment and the replies under it.
  function threads(): { first: Comment; replies: Comment[] }[] {
    const comments = review.value?.comments ?? [];

    return comments
      .filter((c) => c.parentId === null)
      .map((first) => ({
        first,
        replies: comments.filter((c) => c.parentId === first.id),
      }));
  }

  async function reload() {
    const key = changeKey.value;
    if (key) {
      review.value = await api.comments(key);
    }
  }

  async function addComment(comment: NewComment) {
    const key = changeKey.value;
    if (!key) {
      return;
    }
    await guard(async () => {
      await api.addComment(key, comment);
      await reload();
    });
  }

  async function editComment(id: string, edit: EditComment) {
    const key = changeKey.value;
    if (!key) {
      return;
    }
    await guard(async () => {
      await api.editComment(key, id, edit);
      await reload();
    });
  }

  async function deleteComment(id: string) {
    const key = changeKey.value;
    if (!key) {
      return;
    }
    await guard(async () => {
      await api.deleteComment(key, id);
      await reload();
    });
  }

  function setSplit(value: boolean) {
    split.value = value;
    localStorage.setItem('qreview.split', value ? 'yes' : 'no');
  }

  return {
    review,
    threads,
    addComment,
    editComment,
    deleteComment,
    split,
    setSplit,
    version,
    series,
    changeKey,
    files,
    filePath,
    diff,
    error,
    busy,
    mergeBase,
    mergeList,
    onMerge,
    load,
    loadMore,
    openChange,
    openFile,
    openMerge,
    loadMergeList,
  };
});
