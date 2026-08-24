// What the interface knows: the series, the change being read, its files,
// and the diff of the file being read.

import { defineStore } from 'pinia';
import { computed, ref, type Ref } from 'vue';
import { api } from '@/api/client';
import type {
  ChangeComments,
  Comment,
  Config,
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

/// The settings the server reads when it builds a diff. A change to one of
/// these means the file list and the diff must be read again; a change to
/// anything else is the browser's business alone.
const SERVER_SIDE = ['context', 'ignoreWhitespace', 'syntax'];

function touchesTheDiff(patch: object): boolean {
  const diff = (patch as { diff?: Record<string, unknown> }).diff;

  return diff !== undefined && SERVER_SIDE.some((name) => name in diff);
}

export const useReview = defineStore('review', () => {
  const version = ref('');
  const series = ref<Series | null>(null);
  const changeKey = ref<string | null>(null);
  const files = ref<FileEntry[]>([]);
  const filePath = ref<string | null>(null);
  const diff = ref<FileDiff | null>(null);
  const error = ref<string | null>(null);
  const busy = ref(false);
  // What is being read right now. The interface waits on a repository that
  // can be slow, and a reader who sees nothing move thinks it is stuck.
  const filesLoading = ref(0);
  const diffLoading = ref(0);
  /// An answer that arrives after the reader has moved on belongs to
  /// nothing. Every read checks that what it was asked for is still what is
  /// being read, rather than counting the reads: opening a change opens a
  /// file too, and a counter cannot tell those two apart.
  const mergeBase = ref<MergeBase | undefined>(undefined);
  const review = ref<Review | null>(null);
  const patchSets = ref<PatchSet[]>([]);
  const gerrit = ref<GerritChange | null>(null);
  /// The patch set being read. The last one when it is not set.
  const patchSet = ref<number | undefined>(undefined);
  /// What that patch set is read against.
  const against = ref<string | undefined>(undefined);
  /// The view the reader last chose. The configuration decides the first
  /// time, and the choice sticks after that.
  /// Everything the panel owns. It comes from the three layers on disk, so
  /// the next run starts the same way and the command line agrees.
  const config = ref<Config | null>(null);
  const split = computed(() => config.value?.ui.diff === 'side-by-side');
  const wrap = computed(() => config.value?.diff.wrap ?? false);
  const ignoreWs = computed(() => config.value?.diff.ignoreWhitespace ?? false);
  const mergeList = ref<MergeListItem[]>([]);

  /// True while the reader is on the merge under the boundary.
  /// Every comment of the session, change by change, in reading order.
  const written = ref<ChangeComments[]>([]);

  const onMerge = computed(
    () => changeKey.value !== null && changeKey.value === series.value?.boundary.commit,
  );
  /// How many comments the session holds, and how many each change holds.
  const total = computed(() =>
    written.value.reduce((sum, change) => sum + change.comments.length, 0),
  );
  /// How many comments sit in each file of the change being read. Out of
  /// the same list as every other count on the screen.
  const inFile = computed(() => {
    const counts = new Map<string, number>();
    const here = written.value.find((change) => change.key === changeKey.value);

    for (const comment of here?.comments ?? []) {
      const file = comment.anchor?.file;
      if (file) {
        counts.set(file, (counts.get(file) ?? 0) + 1);
      }
    }
    return counts;
  });

  const countOf = computed(() => {
    const counts = new Map<string, number>();
    for (const change of written.value) {
      counts.set(change.key, change.comments.length);
    }
    return counts;
  });

  const loadingFiles = computed(() => filesLoading.value > 0);
  const loadingDiff = computed(() => diffLoading.value > 0);

  /// Count one call in, and out whatever happens.
  ///
  /// A counter, not a flag: two reads can overlap, and the older one must
  /// not say that the newer one is done.
  async function track<T>(count: Ref<number>, work: Promise<T>): Promise<T> {
    count.value += 1;
    try {
      return await work;
    } finally {
      count.value -= 1;
    }
  }

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
      config.value = body.config;

      void readWritten();

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
      patchSets.value = [];
      gerrit.value = null;

      // The patch sets are asked for on their own. That call reaches Gerrit
      // over ssh, and the file list must not wait a second for it.
      void api
        .patchSets(key)
        .then((versions) => {
          if (changeKey.value === key) {
            patchSets.value = versions.sets;
            gerrit.value = versions.gerrit;
          }
        })
        .catch(() => undefined);

      const [comments, list] = await track(
        filesLoading,
        Promise.all([api.comments(key), api.files(key, undefined, base, ignoreWs.value)]),
      );
      if (changeKey.value !== key) {
        return;
      }
      review.value = comments;
      files.value = list;
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
      const read = await track(
        diffLoading,
        api.diff(key, path, patchSet.value, against.value ?? mergeBase.value, ignoreWs.value),
      );
      if (changeKey.value === key && filePath.value === path) {
        diff.value = read;
      }
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
    const was = filePath.value;

    await guard(async () => {
      patchSet.value = number;
      against.value = base;
      review.value = await api.comments(key, number);
      files.value = await track(filesLoading, api.files(key, number, base));

      const stays = files.value.find((f) => f.path === was && !f.binary);
      const first = stays ?? files.value.find((f) => !f.binary);
      const read = first ? await track(diffLoading, api.diff(key, first.path, number, base)) : null;

      // The reader can open a file while this is in flight. That choice is
      // newer than this one, so it wins.
      if (filePath.value !== was) {
        return;
      }
      diff.value = read;
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

  /// Mark a change read, or unread.
  async function markChange(key: string, reviewed: boolean) {
    await guard(async () => {
      const updated = await api.markChange(key, reviewed);
      const change = series.value?.changes.find((c) => c.key === key);
      if (change) {
        change.reviewed = updated.reviewed;
      }
    });
  }

  /// A run of lines the diff does not carry, for opening the context.
  async function loadLines(from: number, to: number) {
    const key = changeKey.value;
    const file = filePath.value;
    if (!key || !file) {
      return [];
    }
    return api.lines(key, file, from, to, patchSet.value);
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

  /// The comments of the change being read.
  function comments(): Comment[] {
    return review.value?.comments ?? [];
  }

  async function reload() {
    const key = changeKey.value;
    if (key) {
      review.value = await api.comments(key, patchSet.value);
    }
    await readWritten();
  }

  /// Read what the whole session holds. Every count on the screen comes
  /// from this one list, so no two of them can disagree.
  async function readWritten() {
    try {
      written.value = await api.allComments();
    } catch {
      // A count is not worth an error in the reader's face.
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

  /// Write what the panel changed, and read the file again as it landed.
  async function savePrefs(patch: object) {
    await guard(async () => {
      config.value = await api.saveConfig(patch);

      // Only a setting the server reads is worth reading the diff again for.
      // Re-reading it for a colour or a view would replace what is on the
      // screen, and a comment box open on it would go with it.
      const key = changeKey.value;
      if (!key || !touchesTheDiff(patch)) {
        return;
      }
      const base = against.value ?? mergeBase.value;
      const was = filePath.value;
      files.value = await track(filesLoading, api.files(key, patchSet.value, base, ignoreWs.value));

      const stays = files.value.find((f) => f.path === was && !f.binary);
      const first = stays ?? files.value.find((f) => !f.binary);
      const read = first
        ? await track(diffLoading, api.diff(key, first.path, patchSet.value, base, ignoreWs.value))
        : null;

      // The reader can open another file while this is in flight, and that
      // choice is newer than this one.
      if (filePath.value !== was) {
        return;
      }
      filePath.value = first?.path ?? null;
      diff.value = read;
    });
  }

  function setSplit(value: boolean) {
    void savePrefs({ ui: { diff: value ? 'side-by-side' : 'unified' } });
  }

  return {
    review,
    comments,
    patchSets,
    gerrit,
    fetchPatchSet,
    copyExport,
    markChange,
    loadLines,
    patchSet,
    against,
    openPatchSet,
    placement,
    lost,
    addComment,
    editComment,
    deleteComment,
    config,
    savePrefs,
    split,
    setSplit,
    ignoreWs,
    wrap,
    version,
    series,
    changeKey,
    files,
    filePath,
    diff,
    error,
    busy,
    written,
    total,
    countOf,
    inFile,
    loadingFiles,
    loadingDiff,
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
