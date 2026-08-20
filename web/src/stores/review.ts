// What the interface knows: the series, the change being read, its files,
// and the diff of the file being read.

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { api } from '@/api/client';
import type {
  Comment,
  EditComment,
  FileDiff,
  FileEntry,
  MergeBase,
  MergeListItem,
  GerritChange,
  NewComment,
  PatchSet,
  Placed,
  Review,
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
  const review = ref<Review | null>(null);
  const patchSets = ref<PatchSet[]>([]);
  const gerrit = ref<GerritChange | null>(null);
  /// The patch set being read. The last one when it is not set.
  const patchSet = ref<number | undefined>(undefined);
  /// What that patch set is read against.
  const against = ref<string | undefined>(undefined);
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
      patchSet.value = undefined;
      against.value = undefined;
      const versions = await api.patchSets(key).catch(() => ({ sets: [], gerrit: null }));
      patchSets.value = versions.sets;
      gerrit.value = versions.gerrit;
      review.value = await api.comments(key);
      files.value = await api.files(key, undefined, base);
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
      diff.value = await api.diff(key, path, patchSet.value, against.value ?? mergeBase.value);
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

  /// Read another version of the change, against another one.
  async function openPatchSet(number: number | undefined, base?: string) {
    const key = changeKey.value;
    if (!key) {
      return;
    }
    await guard(async () => {
      patchSet.value = number;
      against.value = base;
      review.value = await api.comments(key, number);
      files.value = await api.files(key, number, base);

      const stays = files.value.find((f) => f.path === filePath.value && !f.binary);
      const first = stays ?? files.value.find((f) => !f.binary);
      diff.value = first ? await api.diff(key, first.path, number, base) : null;
      filePath.value = first?.path ?? null;
    });
  }

  /// The review as Markdown, in the clipboard.
  ///
  /// The clipboard is refused outside a secure context in some browsers, and
  /// the text is worth more than the convenience, so the failure hands it
  /// back rather than swallowing it.
  async function copyExport(scope: 'change' | 'series'): Promise<string | null> {
    const key = scope === 'change' ? (changeKey.value ?? undefined) : undefined;
    let text = '';
    await guard(async () => {
      text = await api.exportText(key);
      await navigator.clipboard.writeText(text);
    });
    return error.value ? text : null;
  }

  /// Bring a patch set that lives on Gerrit into this clone.
  async function fetchPatchSet(number: number) {
    const key = changeKey.value;
    if (!key) {
      return;
    }
    await guard(async () => {
      await api.fetchPatchSet(key, number);
      const versions = await api.patchSets(key);
      patchSets.value = versions.sets;
      gerrit.value = versions.gerrit;
    });
  }

  /// Where a comment lands in the patch set being read.
  function placement(id: string): Placed | undefined {
    return review.value?.placed.find((p) => p.id === id);
  }

  /// The comments whose place is gone. They are never dropped.
  function lost(): Comment[] {
    const comments = review.value?.comments ?? [];

    return comments.filter((c) => placement(c.id)?.lost);
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
      review.value = await api.comments(key, patchSet.value);
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
    patchSets,
    gerrit,
    fetchPatchSet,
    copyExport,
    patchSet,
    against,
    openPatchSet,
    placement,
    lost,
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
